//! Shared retained overlay controls for ordinary and virtual viewports.
use crate::{
    input::{EventPhase, InputEvent, Key, PointerButton},
    reactive::Signal,
    scene::{Layout, NodeId, NodeKind, Style, Transform},
    semantics::{Role, ScrollAxis, SemanticNode},
    widgets::Ui,
};

pub(crate) fn mount(
    ui: &mut Ui,
    owner: NodeId,
    offset: Signal<f32>,
    size: Signal<(f32, f32)>,
    extent: Signal<f32>,
    horizontal: bool,
) {
    let overlay = ui.container(owner, Layout::Overlay, Style::default());
    let track = ui.container(overlay, Layout::Overlay, Style::default());
    let background =
        ui.scene
            .borrow_mut()
            .append(track, NodeKind::Rect(ui.theme.hover), Style::default());
    let thumb =
        ui.scene
            .borrow_mut()
            .append(track, NodeKind::Rect(ui.theme.accent), Style::default());
    let main = move |size: (f32, f32)| if horizontal { size.0 } else { size.1 };
    let event_size = size.clone();
    let event_extent = extent.clone();
    let event_offset = offset.clone();
    let scene = ui.scene.clone();
    let focused = ui.signal(false);
    let write_focus = focused.clone();
    let thumb_color = ui.theme.accent;
    let focused_color = ui.theme.text;
    let mut grab = None;
    ui.on_event(track, true, move |cx| {
        if cx.phase == EventPhase::Capture {
            return;
        }
        if matches!(cx.event, InputEvent::Focus) {
            write_focus.set(true);
        }
        if matches!(cx.event, InputEvent::Blur) {
            write_focus.set(false);
        }
        if cx.default_prevented()
            && !matches!(
                cx.event,
                InputEvent::PointerUp { .. }
                    | InputEvent::PointerCancel
                    | InputEvent::Blur
                    | InputEvent::Focus
            )
        {
            return;
        }
        let length = main(event_size.get()) as f64;
        let total = event_extent.get() as f64;
        let limit = (total - length).max(0.);
        let thumb_length = (length * length / total.max(1.)).max(24.).min(length);
        let travel = (length - thumb_length).max(0.);
        let position = match cx.event {
            InputEvent::PointerDown { x, y, .. } | InputEvent::PointerMove { x, y } => {
                let bounds = scene.borrow().bounds(track);
                Some(if horizontal {
                    x as f64 - bounds.x as f64
                } else {
                    y as f64 - bounds.y as f64
                })
            }
            _ => None,
        };
        if position.is_some_and(|position| !position.is_finite()) {
            return;
        }
        let before = event_offset.get() as f64;
        let next = match &cx.event {
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            } if limit > 0. => {
                cx.focus();
                let pointer = position.unwrap();
                let thumb_start = before / limit * travel;
                if pointer >= thumb_start && pointer <= thumb_start + thumb_length {
                    grab = Some(pointer - thumb_start);
                    cx.capture_pointer();
                    None
                } else {
                    Some(
                        before
                            + if pointer < thumb_start {
                                -length
                            } else {
                                length
                            },
                    )
                }
            }
            InputEvent::PointerMove { .. } if grab.is_some() => Some(if travel > 0. {
                (position.unwrap() - grab.unwrap()) / travel * limit
            } else {
                0.
            }),
            InputEvent::PointerUp {
                button: PointerButton::Primary,
                ..
            }
            | InputEvent::PointerCancel
            | InputEvent::Blur => {
                grab = None;
                cx.release_pointer();
                None
            }
            InputEvent::KeyDown { key, .. } => match key {
                Key::Home => Some(0.),
                Key::End => Some(limit),
                Key::PageUp => Some(before - length),
                Key::PageDown => Some(before + length),
                Key::ArrowLeft | Key::ArrowUp => Some(before - 40.),
                Key::ArrowRight | Key::ArrowDown => Some(before + 40.),
                _ => None,
            },
            InputEvent::Increment => Some(before + 40.),
            InputEvent::Decrement => Some(before - 40.),
            InputEvent::SetNumericValue(value) if value.is_finite() => Some(*value),
            InputEvent::SetValue(value) => value.parse::<f64>().ok().filter(|v| v.is_finite()),
            _ => None,
        };
        if let Some(next) = next {
            event_offset.set(next.clamp(0., limit) as f32);
            cx.prevent_default();
            cx.stop_propagation();
        } else if matches!(
            cx.event,
            InputEvent::PointerDown {
                button: PointerButton::Primary,
                ..
            }
        ) {
            cx.prevent_default();
            cx.stop_propagation();
        }
    });
    let weak = ui.downgrade();
    ui.bind(overlay, move || {
        let Some(mut ui) = weak.upgrade() else {
            return;
        };
        let (width, height) = size.get();
        let length = main((width, height));
        let total = extent.get();
        let limit = (total - length).max(0.);
        let visible = limit > 0. && length > 0. && width > 0. && height > 0.;
        if ui
            .input
            .options(track)
            .is_none_or(|options| options.disabled == visible)
        {
            ui.set_disabled(track, !visible);
        }
        let fixed = |width, height| Style {
            width: Some(width),
            height: Some(height),
            ..Default::default()
        };
        let mut scene = ui.scene.borrow_mut();
        // Zero area lets underlying controls receive hits outside the track.
        scene.set_style(
            overlay,
            Style {
                margin: crate::scene::Insets {
                    right: width,
                    ..Default::default()
                },
                ..fixed(0., 0.)
            },
        );
        let thickness = if horizontal { height } else { width }.min(8.);
        let track_size = if !visible {
            (0., 0.)
        } else if horizontal {
            (length, thickness)
        } else {
            (thickness, length)
        };
        scene.set_style(track, fixed(track_size.0, track_size.1));
        scene.set_style(background, fixed(track_size.0, track_size.1));
        scene.set_transform(
            track,
            if horizontal {
                Transform {
                    x: 0.,
                    y: height - thickness,
                }
            } else {
                Transform {
                    x: width - thickness,
                    y: 0.,
                }
            },
        );
        let thumb_length = if visible {
            ((length as f64 * length as f64 / total as f64)
                .max(24.)
                .min(length as f64)) as f32
        } else {
            0.
        };
        let thumb_size = if horizontal {
            (thumb_length, track_size.1)
        } else {
            (track_size.0, thumb_length)
        };
        scene.set_style(thumb, fixed(thumb_size.0, thumb_size.1));
        scene.set_kind(
            thumb,
            NodeKind::Rect(if focused.get() {
                focused_color
            } else {
                thumb_color
            }),
        );
        let value = offset.get().clamp(0., limit);
        let position = if limit > 0. {
            (value as f64 / limit as f64 * (length - thumb_length) as f64) as f32
        } else {
            0.
        };
        scene.set_transform(
            thumb,
            if horizontal {
                Transform { x: position, y: 0. }
            } else {
                Transform { x: 0., y: position }
            },
        );
        drop(scene);
        if visible {
            let mut semantic = SemanticNode::new(
                Role::ScrollBar,
                if horizontal {
                    "Horizontal scrollbar"
                } else {
                    "Vertical scrollbar"
                },
            );
            semantic.scroll_axis = Some(if horizontal {
                ScrollAxis::Horizontal
            } else {
                ScrollAxis::Vertical
            });
            semantic.min = Some(0.);
            semantic.max = Some(limit as f64);
            semantic.numeric_value = Some(value as f64);
            ui.semantics.borrow_mut().set(track, semantic);
        } else {
            ui.semantics.borrow_mut().remove(track);
        }
    });
}
