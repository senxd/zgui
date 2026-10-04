//! A spring hover interaction and an interruptible retained panel exit.
use std::time::Duration;
use zgui::{compose::prelude::*, input::InputEvent};
use zgui_desktop::Application;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::new().run(|cx| {
        cx.render(component(|cx| {
            let hover = cx.motion_value(0.0);
            let position = hover.signal();
            let presence = Presence::new(cx, true);
            let mounted = presence.mounted();
            let opacity = presence.progress.signal();
            let mut open = true;
            column()
                .p(32.0)
                .gap(24.0)
                .w_full()
                .h_full()
                .bg(rgb(0x171717))
                .child(
                    button()
                        .px(20.0)
                        .py(12.0)
                        .rounded(8.0)
                        .bg(rgb(0x4a58df))
                        .child(text("Toggle panel").text_color(rgb(0xffffff)))
                        .reactive_style(move || Styles::new().translate(0.0, -4.0 * position.get()))
                        .on_event(move |event| match event.event {
                            InputEvent::PointerEnter => {
                                hover.animate_to(1.0, Transition::spring(Spring::default()));
                            }
                            InputEvent::PointerLeave => {
                                hover.animate_to(0.0, Transition::spring(Spring::default()));
                            }
                            _ => {}
                        })
                        .on_click(move || {
                            open = !open;
                            presence.set_present(
                                open,
                                Transition::tween(
                                    Duration::from_millis(180),
                                    Easing::cubic_bezier(0.16, 1.0, 0.3, 1.0),
                                ),
                            );
                        }),
                )
                .child(switch(
                    move || mounted.get(),
                    move |mounted, _| {
                        if mounted {
                            let opacity = opacity.clone();
                            div()
                                .p(24.0)
                                .rounded(12.0)
                                .bg(rgb(0x303030))
                                .child(
                                    text("Close and reopen before the exit finishes.")
                                        .text_color(rgb(0xffffff)),
                                )
                                .reactive_style(move || {
                                    let t = opacity.get();
                                    Styles::new().opacity(t).translate(0.0, 12.0 * (1.0 - t))
                                })
                        } else {
                            div().hidden()
                        }
                    },
                ))
        }));
    })
}
