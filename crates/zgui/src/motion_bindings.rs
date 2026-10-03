//! Small retained input/state adapters; all release motion uses the shared driver.
use super::*;
use crate::{
    compose::View,
    input::{EventPhase, InputEvent, PointerButton},
};
use std::ops::RangeInclusive;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionAxis {
    X,
    Y,
}
#[derive(Clone, Copy)]
struct Gesture {
    origin: f32,
    value: f32,
    last_pointer: f32,
    last_time: Instant,
}
#[derive(Clone)]
pub struct DragMotion {
    value: MotionValue,
    output: Signal<f32>,
    axis: MotionAxis,
    bounds: RangeInclusive<f32>,
    snaps: Rc<[f32]>,
    spring: Spring,
    gesture: Rc<Cell<Option<Gesture>>>,
}
impl DragMotion {
    pub fn new(
        cx: &mut Context,
        initial: f32,
        axis: MotionAxis,
        bounds: RangeInclusive<f32>,
    ) -> Result<Self, MotionError> {
        if !initial.is_finite()
            || !bounds.start().is_finite()
            || !bounds.end().is_finite()
            || bounds.start() > bounds.end()
            || !bounds.contains(&initial)
        {
            return Err(MotionError(
                "drag requires finite ordered bounds containing its initial value",
            ));
        }
        let value = cx.motion_value(initial);
        let source = value.signal();
        let limits = bounds.clone();
        let output = cx.derive(move || source.get().clamp(*limits.start(), *limits.end()));
        Ok(Self {
            value,
            output,
            axis,
            bounds,
            snaps: Rc::from([]),
            spring: Spring::default(),
            gesture: Rc::default(),
        })
    }
    pub fn signal(&self) -> Signal<f32> {
        self.output.clone()
    }
    pub fn get(&self) -> f32 {
        self.output.get()
    }
    pub fn velocity(&self) -> f32 {
        self.value.velocity()
    }
    pub fn snap_points(
        mut self,
        points: impl IntoIterator<Item = f32>,
    ) -> Result<Self, MotionError> {
        let points: Vec<_> = points.into_iter().take(65).collect();
        if points.len() > 64
            || points
                .iter()
                .any(|p| !p.is_finite() || !self.bounds.contains(p))
        {
            return Err(MotionError(
                "drag snap points must be finite, in bounds, and at most 64",
            ));
        }
        self.snaps = points.into();
        Ok(self)
    }
    pub fn spring(mut self, spring: Spring) -> Self {
        Transition::spring(spring);
        self.spring = spring;
        self
    }
    pub fn begin(&self, pointer: f32, now: Instant) -> bool {
        if !self.value.track.alive.get() || !pointer.is_finite() {
            return false;
        }
        self.value.resume();
        self.value.stop();
        let value = self
            .value
            .track
            .value
            .with_untracked(|v| *v)
            .clamp(*self.bounds.start(), *self.bounds.end());
        self.value.set_with_velocity(value, 0.);
        self.gesture.set(Some(Gesture {
            origin: pointer,
            value,
            last_pointer: pointer,
            last_time: now,
        }));
        true
    }
    pub fn update(&self, pointer: f32, now: Instant) {
        let Some(mut gesture) = self.gesture.get() else {
            return;
        };
        if !pointer.is_finite() || !self.value.track.alive.get() {
            return;
        }
        let next = finite_scalar(gesture.value as f64 + pointer as f64 - gesture.origin as f64)
            .clamp(*self.bounds.start(), *self.bounds.end());
        let previous = self.value.track.value.with_untracked(|v| *v);
        let seconds = now
            .saturating_duration_since(gesture.last_time)
            .as_secs_f64()
            .max(0.001);
        let velocity = finite_scalar((next as f64 - previous as f64) / seconds);
        self.value.set_with_velocity(next, velocity);
        gesture.last_pointer = pointer;
        gesture.last_time = now;
        self.gesture.set(Some(gesture));
    }
    pub fn release(&self, pointer: f32, now: Instant) -> Option<Animation> {
        let gesture = self.gesture.get()?;
        if pointer != gesture.last_pointer {
            self.update(pointer, now);
        }
        let gesture = self.gesture.take()?;
        if !self.value.track.alive.get() {
            return None;
        }
        if now.saturating_duration_since(gesture.last_time) > Duration::from_millis(100) {
            let value = self.value.track.value.with_untracked(|v| *v);
            self.value.set_with_velocity(value, 0.);
        }
        let value = self.value.track.value.with_untracked(|v| *v);
        let projected = (value as f64 + self.value.velocity() as f64 * 0.15)
            .clamp(*self.bounds.start() as f64, *self.bounds.end() as f64);
        let target = self
            .snaps
            .iter()
            .copied()
            .min_by(|a, b| {
                ((*a as f64 - projected).abs()).total_cmp(&(*b as f64 - projected).abs())
            })
            .unwrap_or(projected as f32);
        Some(
            self.value
                .animate_to(target, Transition::spring(self.spring)),
        )
    }
    pub fn cancel(&self) {
        self.gesture.set(None);
        self.value.stop();
    }
    pub fn dragging(&self) -> bool {
        self.gesture.get().is_some() && self.value.track.alive.get()
    }
    pub fn bind(&self, view: View) -> View {
        let drag = self.clone();
        view.on_event(move |event| {
            if event.phase == EventPhase::Capture {
                return;
            }
            let coordinate = |x, y| if drag.axis == MotionAxis::X { x } else { y };
            match event.event {
                InputEvent::PointerDown {
                    x,
                    y,
                    button: PointerButton::Primary,
                } if !event.default_prevented() && !event.capture_requested() => {
                    if drag.begin(coordinate(x, y), Instant::now()) {
                        event.capture_pointer();
                        event.prevent_default();
                    }
                }
                InputEvent::PointerMove { x, y } if drag.dragging() => {
                    drag.update(coordinate(x, y), Instant::now());
                    event.prevent_default();
                }
                InputEvent::PointerUp {
                    x,
                    y,
                    button: PointerButton::Primary,
                } if drag.dragging() => {
                    drag.release(coordinate(x, y), Instant::now());
                    event.release_pointer();
                    event.prevent_default();
                }
                InputEvent::PointerCancel | InputEvent::Blur | InputEvent::FocusScopeClosed
                    if drag.dragging() =>
                {
                    drag.cancel();
                    event.release_pointer();
                }
                _ => {}
            }
        })
    }
}

#[derive(Clone)]
pub struct ScrollProgress(Signal<f32>);
impl ScrollProgress {
    pub fn new(cx: &Context) -> Self {
        Self(cx.state(0.))
    }
    pub fn signal(&self) -> Signal<f32> {
        self.0.clone()
    }
    pub fn bind(&self, view: View) -> View {
        view.scroll_progress(self.0.clone())
    }
}

/// A bounded named state table over a scalar. Derive the component's properties
/// from its value; ordinary spring interruption/velocity semantics are preserved.
#[derive(Clone)]
pub struct MotionStates<K> {
    pub value: MotionValue,
    current: Signal<K>,
    states: Rc<[(K, f32, Transition)]>,
}
impl<K: Clone + Eq + 'static> MotionStates<K> {
    pub fn new(
        cx: &mut Context,
        initial: K,
        states: impl IntoIterator<Item = (K, f32, Transition)>,
    ) -> Result<Self, MotionError> {
        let states: Vec<_> = states.into_iter().take(129).collect();
        if states.is_empty()
            || states.len() > 128
            || states.iter().any(|(_, v, _)| !v.is_finite())
            || states
                .iter()
                .enumerate()
                .any(|(i, (key, _, _))| states[..i].iter().any(|(other, _, _)| key == other))
        {
            return Err(MotionError(
                "state table needs 1..=128 unique keys and finite values",
            ));
        }
        for (_, _, transition) in &states {
            transition.validate();
        }
        let value = states
            .iter()
            .find(|(k, _, _)| *k == initial)
            .ok_or(MotionError("initial motion state is missing"))?
            .1;
        Ok(Self {
            value: cx.motion_value(value),
            current: cx.state(initial),
            states: states.into(),
        })
    }
    pub fn state(&self) -> Signal<K> {
        self.current.clone()
    }
    pub fn signal(&self) -> Signal<f32> {
        self.value.signal()
    }
    pub fn set(&self, key: K) -> Result<Animation, MotionError> {
        let (_, value, transition) = self
            .states
            .iter()
            .find(|(k, _, _)| *k == key)
            .ok_or(MotionError("unknown motion state"))?;
        if !self.value.track.alive.get() {
            return Ok(self.value.animation());
        }
        if self.current.with_untracked(|k| *k == key) {
            return Ok(self.value.animation());
        }
        Ok(self.value.scheduler.runtime.batch(|| {
            let run = self.value.animate_to(*value, *transition);
            self.current.set(key);
            run
        }))
    }
}

pub(crate) fn project_layout(
    ui: &mut crate::widgets::Ui,
    node: crate::scene::NodeId,
    point: MotionPoint,
    transition: Transition,
) -> Signal<Vec2> {
    ui.mark_layout_projected(node);
    let bounds = ui.observe_layout_bounds(node);
    let ancestor_bounds = ui
        .projected_ancestor_bounds(node)
        .map(|(id, _)| ui.observe_layout_bounds(id));
    let signal = point.signal();
    let runtime = ui.runtime.clone();
    let weak = ui.downgrade();
    let mut previous = None;
    ui.bind(node, move || {
        if let Some(ancestor) = &ancestor_bounds {
            ancestor.get();
        }
        let Some(bounds) = bounds.get() else {
            previous = None;
            point.set(Vec2::default());
            return;
        };
        let Some(ui) = weak.upgrade() else { return };
        if !ui.scene.borrow().contains(node) || !ui.scene.borrow().layout_visible(node) {
            previous = None;
            return;
        }
        let ancestor = ui.projected_ancestor_bounds(node);
        let old = previous.replace((bounds, ancestor));
        if let Some((old, old_ancestor)) = old {
            let current = point.signal().with_untracked(|v| *v);
            // World layout displacement is already inherited from the nearest
            // projected ancestor. Subtract it so nested nodes move only once.
            let (ax, ay) = match (old_ancestor, ancestor) {
                (Some((old_id, old)), Some((new_id, new))) if old_id == new_id => {
                    (old.x as f64 - new.x as f64, old.y as f64 - new.y as f64)
                }
                _ => (0., 0.),
            };
            let dx = old.x as f64 - bounds.x as f64 - ax;
            let dy = old.y as f64 - bounds.y as f64 - ay;
            let delta = Vec2::new(
                finite_scalar(current.x as f64 + dx),
                finite_scalar(current.y as f64 + dy),
            );
            if dx != 0. || dy != 0. {
                runtime.batch(|| {
                    point.x.set_with_velocity(delta.x, point.x.velocity());
                    point.y.set_with_velocity(delta.y, point.y.velocity());
                    point.animate_to(Vec2::default(), transition);
                });
            }
        }
    });
    signal
}
