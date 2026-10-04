# Motion

`zgui::motion` animates retained scalar signals. Every value using the same
`FrameClock` and reactive `Runtime` shares one parked task. Only active values
are sampled, and all writes are batched before reactive styles run. A clock
shared across different runtimes gets a separate driver for each runtime.

```rust,no_run
use std::time::Duration;
use zgui::compose::prelude::*;

let view = component(|cx| {
    let value = cx.motion_value(0.0);
    let progress = value.signal();
    let click = value.clone();
    button()
        .child("Open")
        .on_click(move || {
            click.animate_to(1.0, Transition::spring(Spring::default()));
        })
        .reactive_style(move || {
            let t = progress.get();
            Styles::new().opacity(t.clamp(0.0, 1.0)).translate(0.0, 12.0 * (1.0 - t))
        })
});
```

`Transition::tween(Duration::from_millis(180), Easing::EaseOut)` is a timed
transition. `Easing::Linear`, `EaseIn`, `EaseOut`, and `EaseInOut` are cheap
polynomial curves. `Easing::cubic_bezier(0.16, 1.0, 0.3, 1.0)` compiles a custom
curve and accurately inverts its x axis. Add `.delay(duration)` for staggered
starts. Delays sleep on the shared timer when no value currently needs frames.

Springs use a closed-form solution prepared at retarget, with no integration
substeps. `Spring` has stiffness, damping, mass, rest_delta, and rest_speed;
all must be finite and positive. Values and velocity are preserved on spring
retarget. Rest thresholds use your value's units; pixel motion may use larger
thresholds than normalized progress. Clamp bounded consumers, not the spring.

`animate_to` returns an `Animation`. Await `.finished()` to get
`Completion::Finished` or `Completion::Cancelled`. Results belong to that run;
replacing a pending run cancels it, and already completed runs stay completed.
Dropping the handle leaves the run playing. `stop()` cancels a run; `set(value)`
cancels and changes its value immediately. Read `signal()` for styles; writing
that signal directly bypasses interruption, so use `set()` for changes.

## Policy and ownership

Provide a `MotionPolicy { active, reduced }` using `provide` for a subtree.
Inactive values freeze their phase and velocity and unregister frame demand;
reduced values immediately finish at the target, including delayed runs.
Policies are observed by one shared scheduler effect, not one task per value.
Applications supply native preference/focus/visibility signals. Without both
`TaskRunner` and `FrameClock`, values settle synchronously.

`Context` retains an ownership lease. Unmount cancels running values even if
event handlers outside the tree retain their handles. No animation can restart
through a disposed handle. An idle driver is parked on its waker, with no frame
requests or timer polling.

For a burst of animations, start them inside `cx.runtime().batch(|| { ... })`
or an existing runtime batch. This also batches policy subscription refreshes
and prevents repeatedly observing an incrementally growing set of active values.

## Exit transitions

Create `Presence` in the parent that owns the conditional region:

```rust,no_run
use std::time::Duration;
use zgui::compose::prelude::*;

let view = component(|cx| {
    let presence = Presence::new(cx, true);
    let mounted = presence.mounted();
    let progress = presence.progress.signal();
    let close = presence.clone();
    column()
        .child(button().child("Close").on_click(move || {
            close.set_present(false, Transition::tween(Duration::from_millis(140), Easing::EaseIn));
        }))
        .child(switch(move || mounted.get(), move |mounted, _cx| {
            if mounted {
                let progress = progress.clone();
                div().child("Panel").reactive_style(move || Styles::new().opacity(progress.get()))
            } else { div().hidden() }
        }))
});
```

Reopening with `set_present(true, enter)` cancels earlier exit removal and
retargets current progress. Presence controls painting lifetime; applications
still manage modal focus, pointer interaction, and accessibility as appropriate.

Translation and opacity use the retained style fast path. Width/height still
perform layout, and expensive procedural image work remains expensive. Prefer
paint properties for feedback and entrances. Automatic layout projection,
general scene scale/rotation, and gesture/timeline engines are outside this API.

Run correctness checks with `cargo test -p zgui motion`. The ignored release
benchmark reports scheduling and reactive-consumer cost without claiming GPU
or end-to-end presentation performance.
See [measured results and reproduction](motion-performance.md).
