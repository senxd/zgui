use super::tracks::MAX_KEYFRAMES;
use super::*;
use crate::{
    compose::{View, ViewHandle, column, component, div, provide, scroll, switch},
    input::{InputEvent, PointerButton},
    scene::Color,
    style::{Styled, Styles},
    task::LocalExecutor,
    widgets::Ui,
};

struct Harness<T> {
    ui: Ui,
    root: ViewHandle,
    data: T,
    executor: Rc<RefCell<LocalExecutor>>,
    frames: FrameClock,
    policy: MotionPolicy,
    start: Instant,
}
impl<T: 'static> Harness<T> {
    fn new(build: impl FnOnce(&mut Context) -> (View, T) + 'static) -> Self {
        let executor = Rc::new(RefCell::new(LocalExecutor::new()));
        let frames = FrameClock::new();
        let mut ui = Ui::new(400., 400.);
        let policy = MotionPolicy {
            active: ui.signal(true),
            reduced: ui.signal(false),
        };
        let output = Rc::new(RefCell::new(None));
        let capture = output.clone();
        let root = ui.mount(provide(
            frames.clone(),
            provide(
                TaskRunner::from_executor(executor.clone()),
                provide(
                    policy.clone(),
                    component(move |cx| {
                        let (view, data) = build(cx);
                        *capture.borrow_mut() = Some(data);
                        view
                    }),
                ),
            ),
        ));
        let data = output.borrow_mut().take().unwrap();
        Self {
            ui,
            root,
            data,
            executor,
            frames,
            policy,
            start: Instant::now(),
        }
    }
    fn tick(&self) -> usize {
        self.executor.borrow_mut().tick()
    }
    fn frame(&self, index: u64, millis: u64) -> usize {
        let delivered = self.frames.deliver(Frame {
            index,
            time: self.start + Duration::from_millis(millis),
            interval: Duration::from_millis(10),
        });
        self.tick();
        delivered
    }
}
fn curve(from: f32, to: f32) -> Keyframes<f32> {
    Keyframes::between(from, to, Easing::Linear)
}
fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}
fn tween(value: u64) -> Transition {
    Transition::tween(ms(value), Easing::Linear)
}
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.02, "{a} vs {b}");
}

#[test]
fn compiled_curves_validate_boundaries_and_preserve_typed_color() {
    let closed = Keyframes::new([
        Keyframe::new(0., 0.),
        Keyframe::new(0.5, 1.),
        Keyframe::new(1., 0.),
    ])
    .unwrap();
    near(closed.sample(0.25), 0.5);
    assert_eq!(closed.sample(0.5), 1.);
    assert_eq!(closed.sample(1.), 0.);
    assert!(
        Keyframes::new([
            Keyframe::new(0., 0.),
            Keyframe::new(0., 1.),
            Keyframe::new(1., 0.)
        ])
        .is_err()
    );
    assert!(Keyframes::new([Keyframe::new(0., f32::NAN), Keyframe::new(1., 1.)]).is_err());
    assert!(
        Keyframes::new(
            (0..=MAX_KEYFRAMES).map(|i| Keyframe::new(i as f32 / MAX_KEYFRAMES as f32, 1.))
        )
        .is_err()
    );
    assert!(Repeat::count(0).is_err());
    let tiny = f32::from_bits(2);
    let subnormal = Keyframes::new([
        Keyframe::new(0., 0.),
        Keyframe::new(tiny, 1.),
        Keyframe::new(1., 2.),
    ])
    .unwrap();
    near(subnormal.sample(f32::from_bits(1)), 0.5);
    let black = MotionColor::from_color(Color(0, 0, 0, 255));
    let white = MotionColor::from_color(Color(255, 255, 255, 255));
    assert_eq!(
        black.interpolate(white, 0.5).color(),
        Color(188, 188, 188, 255)
    );
    let transparent_red = MotionColor::from_color(Color(255, 0, 0, 0));
    let blue = MotionColor::from_color(Color(0, 0, 255, 255));
    assert_eq!(
        transparent_red.interpolate(blue, 0.5).color(),
        Color(0, 0, 255, 128)
    );
    assert_eq!(curve(f32::MAX, -f32::MAX).sample(0.5), 0.);
}

#[test]
fn closed_loops_pause_seek_repeat_and_reduced_motion_use_one_driver() {
    let h = Harness::new(|cx| (div(), cx.motion_value(0.)));
    let value = &h.data;
    let frames = Keyframes::new([
        Keyframe::new(0., 0.),
        Keyframe::new(0.5, 1.),
        Keyframe::new(1., 0.),
    ])
    .unwrap();
    let run = value.animate_keyframes(
        frames,
        ms(100),
        Playback {
            repeat: Repeat::count(2).unwrap(),
            ..Default::default()
        },
    );
    assert_eq!(h.tick(), 1);
    assert_eq!(h.frame(1, 25), 1);
    assert!(value.get() > 0.3);
    value.pause();
    h.tick();
    let paused = value.get();
    assert!(!h.frames.wants_frame());
    assert_eq!(h.frame(2, 10_000), 0);
    assert_eq!(value.get(), paused);
    value.seek(ms(150));
    near(value.get(), 1.);
    assert!(!h.frames.wants_frame());
    value.resume();
    h.tick();
    h.frame(3, 10_010);
    near(value.get(), 0.8);
    h.frame(4, 10_100);
    assert_eq!(run.result.get(), Some(Completion::Finished));
    assert!(!h.frames.wants_frame());
    let forever = value.animate_keyframes(
        curve(0., 1.),
        ms(100),
        Playback {
            repeat: Repeat::Forever,
            alternate: true,
            ..Default::default()
        },
    );
    value.seek(ms(175));
    near(value.get(), 0.25);
    h.policy.reduced.set(true);
    h.tick();
    assert_eq!(forever.result.get(), Some(Completion::Finished));
    assert_eq!(value.get(), 1.);
    assert!(!h.frames.wants_frame());
}

#[test]
fn explicit_pause_and_rate_preserve_active_delay() {
    let h = Harness::new(|cx| (div(), cx.motion_value(0.)));
    let value = &h.data;
    value.animate_to(1., tween(100).delay(Duration::from_secs(5)));
    h.tick();
    value.track.run.borrow_mut().as_mut().unwrap().last =
        Some(Instant::now() - Duration::from_secs(2));
    value.pause();
    h.tick();
    let elapsed = value.track.run.borrow().as_ref().unwrap().elapsed;
    assert!((1.99..2.1).contains(&elapsed.as_secs_f64()));
    value.set_rate(2.);
    assert_eq!(value.track.run.borrow().as_ref().unwrap().elapsed, elapsed);
    value.resume();
    h.tick();
    value.track.run.borrow_mut().as_mut().unwrap().last =
        Some(Instant::now() - Duration::from_secs(1));
    value.set_rate(0.5);
    h.tick();
    let elapsed = value.track.run.borrow().as_ref().unwrap().elapsed;
    assert!((3.99..4.2).contains(&elapsed.as_secs_f64()));
    assert!(!h.frames.wants_frame());
}

#[test]
fn retargeting_a_manually_paused_track_never_accrues_inactive_time() {
    let h = Harness::new(|cx| (div(), cx.motion_value(0.)));
    h.data.pause();
    h.data.animate_to(1., tween(100));
    h.tick();
    assert!(h.data.track.run.borrow().as_ref().unwrap().last.is_none());
    // Even a stale external host timestamp cannot advance a manually paused run.
    h.data.track.run.borrow_mut().as_mut().unwrap().last =
        Some(Instant::now() - Duration::from_secs(2));
    h.policy.active.set(false);
    h.tick();
    assert_eq!(
        h.data.track.run.borrow().as_ref().unwrap().elapsed,
        Duration::ZERO
    );
    h.policy.active.set(true);
    h.tick();
    assert!(!h.frames.wants_frame());
    h.data.resume();
    h.tick();
    h.frame(1, 50);
    near(h.data.get(), 0.1);
}

#[test]
fn timeline_batches_parallel_clips_and_sequences_on_one_transport() {
    let h = Harness::new(|cx| {
        let timeline = Timeline::new(cx);
        let x = timeline
            .track(Duration::ZERO, ms(100), curve(0., 10.))
            .unwrap();
        let color = timeline
            .track(
                Duration::ZERO,
                ms(100),
                Keyframes::between(
                    MotionColor::from_color(Color(0, 0, 0, 255)),
                    MotionColor::from_color(Color(255, 255, 255, 255)),
                    Easing::Linear,
                ),
            )
            .unwrap();
        let y = timeline.then(ms(100), curve(0., 20.)).unwrap();
        (div(), (timeline, x, y, color))
    });
    let (timeline, x, y, color) = &h.data;
    assert_eq!(timeline.duration(), ms(200));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let capture = seen.clone();
    let a = x.clone();
    let b = y.clone();
    let c = color.clone();
    let _effect = h.ui.runtime.effect(move || {
        capture
            .borrow_mut()
            .push((a.get(), b.get(), c.get().color()))
    });
    timeline.seek(ms(50));
    h.tick();
    assert!(!h.frames.wants_frame());
    assert_eq!(
        seen.borrow().last().copied(),
        Some((5., 0., Color(188, 188, 188, 255)))
    );
    assert_eq!(seen.borrow().len(), 2, "parallel outputs commit atomically");
    timeline.seek(ms(150));
    assert_eq!((x.get(), y.get()), (10., 10.));
    assert!(timeline.then(ms(10), curve(0., 1.)).is_err());
    let run = timeline.play();
    h.tick();
    assert_eq!(h.frame(1, 50), 1);
    near(y.get(), 12.);
    timeline.pause();
    h.tick();
    assert!(!h.frames.wants_frame());
    timeline.seek(ms(200));
    assert_eq!(
        run.result.get(),
        None,
        "paused endpoint retains its pending handle"
    );
    timeline.play();
    h.tick();
    h.frame(2, 60);
    assert_eq!(run.result.get(), Some(Completion::Finished));
    assert!(!h.frames.wants_frame());
    let reverse = timeline.animate_to(0.);
    h.tick();
    h.frame(3, 100);
    assert!(x.get() <= 10. && y.get() < 20.);
    let before = (x.get(), y.get());
    timeline.animate_to(1.);
    assert_eq!(
        before,
        (x.get(), y.get()),
        "direction changes preserve the pose"
    );
    assert_eq!(reverse.result.get(), Some(Completion::Cancelled));
    h.root.unmount();
    h.tick();
    assert!(!h.frames.wants_frame());
}

#[test]
fn component_retains_timeline_sampling_when_only_outputs_are_kept() {
    let h = Harness::new(|cx| {
        let timeline = Timeline::new(cx);
        let output = timeline.then(ms(100), curve(0., 1.)).unwrap();
        timeline.play();
        (div(), output)
    });
    h.tick();
    h.frame(1, 50);
    assert!(h.data.get() > 0.3 && h.data.get() < 0.7);
    h.frame(2, 150);
    assert_eq!(h.data.get(), 1.);
    assert!(!h.frames.wants_frame());
}

#[test]
fn stopped_timeline_seek_publishes_only_the_requested_pose() {
    let h = Harness::new(|cx| {
        let timeline = Timeline::new(cx);
        let value = timeline.then(ms(100), curve(0., 1.)).unwrap();
        (div(), (timeline, value))
    });
    h.data.0.seek(ms(80));
    h.data.0.stop();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let capture = seen.clone();
    let read = h.data.1.clone();
    let _effect =
        h.ui.runtime
            .effect(move || capture.borrow_mut().push(read.get()));
    h.data.0.seek(ms(50));
    assert_eq!(&*seen.borrow(), &[0.8, 0.5]);
    assert!(!h.frames.wants_frame());
}

#[test]
fn many_timelines_and_group_cancellation_share_one_refresh() {
    let h = Harness::new(|cx| {
        let timelines = (0..32)
            .map(|_| {
                let timeline = Timeline::new(cx);
                timeline.then(ms(100), curve(0., 1.)).unwrap();
                timeline
            })
            .collect::<Vec<_>>();
        (div(), timelines)
    });
    let group =
        h.ui.runtime
            .batch(|| AnimationGroup::new(h.data.iter().map(Timeline::play)));
    assert_eq!(h.tick(), 1);
    assert_eq!(h.frame(1, 50), 1);
    let mut wait = std::pin::pin!(group.finished());
    assert!(
        wait.as_mut()
            .poll(&mut PollContext::from_waker(Waker::noop()))
            .is_pending()
    );
    h.data[0].stop();
    assert_eq!(
        wait.as_mut()
            .poll(&mut PollContext::from_waker(Waker::noop())),
        Poll::Ready(Completion::Cancelled)
    );
    h.root.unmount();
    h.tick();
    assert!(!h.frames.wants_frame());
}

#[test]
fn derived_points_mount_in_batches_and_reject_partial_invalid_updates() {
    let h = Harness::new(|cx| (div(), MotionPoint::new(cx, Vec2::new(1., 2.))));
    let point = &h.data;
    let seen = Rc::new(RefCell::new(Vec::new()));
    let capture = seen.clone();
    let read = point.signal();
    let _effect =
        h.ui.runtime
            .effect(move || capture.borrow_mut().push(read.get()));
    point.set(Vec2::new(3., 4.));
    assert_eq!(&*seen.borrow(), &[Vec2::new(1., 2.), Vec2::new(3., 4.)]);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || point.set(Vec2::new(8., f32::NAN))
        ))
        .is_err()
    );
    assert_eq!(point.get(), Vec2::new(3., 4.));
    let group = point.animate_to(Vec2::new(13., 14.), tween(100));
    assert_eq!(h.tick(), 1);
    h.frame(1, 50);
    near(point.get().y - point.get().x, 1.);
    h.frame(2, 150);
    assert_eq!(group.completion(), Some(Completion::Finished));
    // Runtime.effect defers inside switch rebuilding as well as initial mounting.
    let enabled = h.ui.signal(false);
    let read = enabled.clone();
    let built = Rc::new(Cell::new(0));
    let count = built.clone();
    let mut ui = h.ui.shared();
    let _root = ui.mount(switch(
        move || read.get(),
        move |enabled, _| {
            if enabled {
                let count = count.clone();
                component(move |cx| {
                    let _point = MotionPoint::new(cx, Vec2::default());
                    count.set(count.get() + 1);
                    div()
                })
            } else {
                div()
            }
        },
    ));
    enabled.set(true);
    assert_eq!(built.get(), 1);
}

#[test]
fn layout_projection_composes_hover_and_nested_motion_without_relayout() {
    let mut h = Harness::new(|cx| {
        let size = cx.state(20.);
        let read = size.clone();
        (
            column().w(300.).children([
                div()
                    .reactive_style(move || Styles::new().h(read.get()))
                    .w(300.),
                column()
                    .id("parent")
                    .w(200.)
                    .h(100.)
                    .layout_motion(tween(100))
                    .child(
                        div()
                            .id("child")
                            .w(40.)
                            .h(40.)
                            .bg(Color(200, 100, 20, 255))
                            .translate(3., 0.)
                            .hover(|style| style.translate(10., 0.))
                            .layout_motion(tween(100)),
                    ),
            ]),
            size,
        )
    });
    h.ui.prepare_frame();
    let parent = h.root.find("parent").unwrap();
    let child = h.root.find("child").unwrap();
    let initial = h.ui.scene.borrow().bounds(child);
    assert_eq!(initial.y, 20.);
    h.ui.dispatch(InputEvent::PointerMove {
        x: initial.x + 5.,
        y: initial.y + 5.,
    });
    h.ui.prepare_frame();
    assert_eq!(h.ui.scene.borrow().bounds(child).x, 10.);
    h.data.set(70.);
    h.ui.prepare_frame();
    assert_eq!(h.ui.scene.borrow().layout_bounds(parent).y, 70.);
    near(h.ui.scene.borrow().bounds(parent).y, 20.);
    near(h.ui.scene.borrow().bounds(child).y, 20.);
    near(h.ui.scene.borrow().bounds(child).x, 10.);
    h.ui.scene.borrow_mut().flush();
    h.tick();
    h.frame(1, 50);
    h.ui.prepare_frame();
    assert_eq!(
        h.ui.scene.borrow_mut().flush().layout_nodes,
        0,
        "projection sampling is paint-only"
    );
    let midway = h.ui.scene.borrow().bounds(child).y;
    assert!(midway > 20. && midway < 70.);
    h.data.set(100.);
    h.ui.prepare_frame();
    near(h.ui.scene.borrow().bounds(child).y, midway);
    h.tick();
    h.frame(2, 200);
    h.ui.prepare_frame();
    near(h.ui.scene.borrow().bounds(child).y, 100.);
    assert!(!h.frames.wants_frame());
}

#[test]
fn measured_scroll_progress_reacts_to_extent_and_does_not_start_layout_motion() {
    let h = Harness::new(|cx| {
        let offset = cx.state(0.);
        let height = cx.state(300.);
        let read = height.clone();
        let progress = ScrollProgress::new(cx);
        let view = progress.bind(
            scroll(offset.clone())
                .w(100.)
                .h(100.)
                .scrollbar(false)
                .child(
                    div()
                        .id("content")
                        .w(100.)
                        .reactive_style(move || Styles::new().h(read.get()))
                        .layout_motion(tween(100)),
                ),
        );
        (view, (offset, height, progress))
    });
    let (offset, height, progress) = &h.data;
    h.ui.prepare_frame();
    h.ui.scene.borrow_mut().flush();
    offset.set(100.);
    h.ui.prepare_frame();
    near(progress.signal().get(), 0.5);
    assert!(
        !h.frames.wants_frame(),
        "scroll translation is not a layout relocation"
    );
    assert_eq!(h.ui.scene.borrow_mut().flush().layout_nodes, 0);
    height.set(500.);
    h.ui.prepare_frame();
    near(progress.signal().get(), 0.25);
    height.set(50.);
    h.ui.prepare_frame();
    assert_eq!(progress.signal().get(), 0.);
    assert_eq!(offset.get(), 0.);
}

#[test]
fn opposing_parent_child_layout_changes_preserve_the_child_world_position() {
    let h = Harness::new(|cx| {
        let outer = cx.state(20.);
        let inner = cx.state(50.);
        let a = outer.clone();
        let b = inner.clone();
        (
            column().w(200.).children([
                div().reactive_style(move || Styles::new().h(a.get())),
                column().layout_motion(tween(100)).children([
                    div().reactive_style(move || Styles::new().h(b.get())),
                    div()
                        .id("child")
                        .size(20., 20.)
                        .bg(Color(255, 255, 255, 255))
                        .layout_motion(tween(100)),
                ]),
            ]),
            (outer, inner),
        )
    });
    h.ui.prepare_frame();
    let child = h.root.find("child").unwrap();
    near(h.ui.scene.borrow().bounds(child).y, 70.);
    h.ui.runtime.batch(|| {
        h.data.0.set(70.);
        h.data.1.set(0.);
    });
    h.ui.prepare_frame();
    near(h.ui.scene.borrow().layout_bounds(child).y, 70.);
    near(h.ui.scene.borrow().bounds(child).y, 70.);
    // Use one virtual origin for the cancelling parent/child tweens, rather
    // than measuring the small wall-clock gap between their retarget calls.
    let scheduler = h.frames.motion_scheduler(
        h.ui.runtime.clone(),
        Some(TaskRunner::from_executor(h.executor.clone())),
    );
    for track in scheduler.tracks.borrow().iter() {
        if let Some(run) = track.run.borrow_mut().as_mut() {
            run.last = Some(h.start);
        }
    }
    h.tick();
    h.frame(1, 50);
    h.ui.prepare_frame();
    near(h.ui.scene.borrow().bounds(child).y, 70.);
}

#[test]
fn drag_release_keeps_fresh_velocity_and_capture_survives_secondary_release() {
    let mut h = Harness::new(|cx| {
        let drag = DragMotion::new(cx, 0., MotionAxis::X, 0.0..=100.)
            .unwrap()
            .snap_points([0., 50., 100.])
            .unwrap();
        let view = drag.bind(div().id("drag").w(40.).h(40.).bg(Color(255, 255, 255, 255)));
        (view, drag)
    });
    let drag = &h.data;
    let now = Instant::now();
    drag.begin(0., now);
    drag.update(10., now + ms(10));
    let run = drag.release(30., now + ms(200)).unwrap();
    assert!(
        drag.velocity() > 90.,
        "a fresh release sample must not be mistaken for stale velocity"
    );
    assert_eq!(run.result.get(), None);
    drag.cancel();
    h.ui.prepare_frame();
    h.ui.dispatch(InputEvent::PointerDown {
        x: 5.,
        y: 5.,
        button: PointerButton::Primary,
    });
    assert!(drag.dragging());
    assert_eq!(h.ui.input.captured(), h.root.find("drag"));
    h.ui.dispatch(InputEvent::PointerMove { x: 300., y: 300. });
    assert_eq!(drag.get(), 100.);
    h.ui.dispatch(InputEvent::PointerUp {
        x: 300.,
        y: 300.,
        button: PointerButton::Secondary,
    });
    assert!(drag.dragging());
    assert!(h.ui.input.captured().is_some());
    h.ui.dispatch(InputEvent::PointerCancel);
    assert!(!drag.dragging());
    assert!(h.ui.input.captured().is_none());
    h.root.unmount();
    assert!(!drag.begin(0., Instant::now()));
    h.tick();
    assert!(!h.frames.wants_frame());
}

#[test]
fn named_states_are_idempotent_and_presence_waits_for_all_exit_tracks() {
    let h = Harness::new(|cx| {
        let states = MotionStates::new(
            cx,
            "idle",
            [("idle", 0., tween(100)), ("hover", 1., tween(100))],
        )
        .unwrap();
        let presence = Presence::new(cx, true);
        let translation = cx.motion_value(1.);
        let blur = cx.motion_value(24.);
        (div(), (states, presence, translation, blur))
    });
    let (states, presence, translation, blur) = &h.data;
    assert!(states.set("missing").is_err());
    let hover = states.set("hover").unwrap();
    let repeated = states.set("hover").unwrap();
    states.value.stop();
    assert_eq!(hover.result.get(), Some(Completion::Cancelled));
    assert_eq!(repeated.result.get(), Some(Completion::Cancelled));
    let exit = AnimationGroup::new([
        translation.animate_to(0., tween(100)),
        blur.animate_to(0., tween(200)),
    ]);
    presence.set_present_with(false, tween(50), exit);
    h.tick();
    h.frame(1, 150);
    assert!(presence.mounted().get(), "blur exit is still running");
    presence.set_present(true, Transition::instant());
    let latest = blur.animate_to(24., tween(300));
    presence.set_present_with(false, Transition::instant(), AnimationGroup::new([latest]));
    h.tick();
    h.frame(2, 250);
    h.tick();
    assert!(
        presence.mounted().get(),
        "earlier exit cannot remove a later epoch"
    );
    h.frame(3, 600);
    h.tick();
    assert!(!presence.mounted().get());
    assert!(!h.frames.wants_frame());
}

#[test]
fn inspector_is_opt_in_idle_and_rejects_foreign_or_disposed_ids() {
    let h = Harness::new(|cx| {
        let point = MotionPoint::new(cx, Vec2::default());
        point.x.label("horizontal");
        let inspector = MotionInspector::new(cx);
        (div(), (point, inspector))
    });
    let (point, inspector) = &h.data;
    assert!(!inspector.snapshot().display_frame_requested);
    assert_eq!(h.tick(), 0);
    let snapshot = inspector.snapshot();
    assert_eq!(snapshot.total_tracks, 2);
    let id = snapshot
        .tracks
        .iter()
        .find(|t| t.label == "horizontal")
        .unwrap()
        .id;
    let other = Harness::new(|cx| (div(), MotionInspector::new(cx)));
    assert!(!other.data.pause(id));
    let foreign_thread_id = std::thread::spawn(|| {
        let h = Harness::new(|cx| {
            let _value = cx.motion_value(0.);
            (div(), MotionInspector::new(cx))
        });
        h.data.snapshot().tracks[0].id
    })
    .join()
    .unwrap();
    assert!(!inspector.pause(foreign_thread_id));
    point.animate_to(Vec2::new(10., 20.), tween(100));
    h.tick();
    h.frame(1, 50);
    assert_eq!(inspector.snapshot().last_sampled_tracks, 2);
    assert!(inspector.pause(id));
    h.tick();
    assert!(inspector.seek(id, ms(75)));
    near(point.x.get(), 7.5);
    assert!(!inspector.set_rate(id, f64::NAN));
    assert!(inspector.set_rate(id, 0.5));
    assert!(inspector.stop(id));
    h.frame(2, 150);
    assert!(!h.frames.wants_frame());
    h.root.unmount();
    h.tick();
    assert!(inspector.snapshot().tracks.is_empty());
    assert!(!inspector.resume(id));
    assert!(!h.frames.wants_frame());
}

#[test]
fn headless_timeline_scrubs_and_presence_groups_settle_synchronously() {
    let mut ui = Ui::new(100., 100.);
    let output = Rc::new(RefCell::new(None));
    let capture = output.clone();
    let _root = ui.mount(component(move |cx| {
        let timeline = Timeline::new(cx);
        let value = timeline.then(ms(100), curve(0., 1.)).unwrap();
        let presence = Presence::new(cx, true);
        *capture.borrow_mut() = Some((timeline, value, presence));
        div()
    }));
    let (timeline, value, presence) = output.borrow_mut().take().unwrap();
    let run = timeline.play();
    assert_eq!(run.result.get(), Some(Completion::Finished));
    assert_eq!(value.get(), 1.);
    timeline.seek(ms(50));
    near(value.get(), 0.5);
    let exit = timeline.animate_to(0.);
    presence.set_present_with(false, Transition::instant(), AnimationGroup::new([exit]));
    assert!(!presence.mounted().get());
    assert_eq!(value.get(), 0.);
}

#[test]
#[ignore = "run in release mode with --ignored --nocapture for timing"]
fn compiled_timeline_release_benchmark() {
    for count in [1, 32, 128] {
        let h = Harness::new(move |cx| {
            let timeline = Timeline::new(cx);
            let outputs = (0..count)
                .map(|_| {
                    timeline
                        .track(
                            Duration::ZERO,
                            Duration::from_secs(60),
                            Keyframes::new([
                                Keyframe::new(0., Vec2::default()),
                                Keyframe::new(0.25, Vec2::new(10., 20.)),
                                Keyframe::new(0.5, Vec2::new(20., 10.)),
                                Keyframe::new(0.75, Vec2::new(10., 30.)),
                                Keyframe::new(1., Vec2::default()),
                            ])
                            .unwrap(),
                        )
                        .unwrap()
                })
                .collect::<Vec<_>>();
            (div(), (timeline, outputs))
        });
        let _consumers = h
            .data
            .1
            .iter()
            .map(|signal| {
                let signal = signal.clone();
                h.ui.runtime.effect(move || {
                    std::hint::black_box(signal.get());
                })
            })
            .collect::<Vec<_>>();
        h.data.0.play();
        assert_eq!(h.tick(), 1);
        let started = Instant::now();
        for frame in 1..=1000 {
            assert_eq!(h.frame(frame, frame * 10), 1);
        }
        let elapsed = started.elapsed();
        println!(
            "timeline clips={count} vec2 channels={} keyframes=5 frames=1000 driver_polls=1000 us/frame={:.3}",
            count * 2,
            elapsed.as_secs_f64() * 1000.
        );
    }
}
