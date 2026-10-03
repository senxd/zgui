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
paint properties for feedback and entrances. Position projection, drag/snap input and bounded timelines are described below.
General scene scale/rotation is outside this API.

Run correctness checks with `cargo test -p zgui motion`. The ignored release
benchmark reports scheduling and reactive-consumer cost without claiming GPU
or end-to-end presentation performance.
See [measured results and reproduction](motion-performance.md).

## Continuous shader effects

Use `zgui::effects` for decorative loops. It owns a separate shared task and
window budget; reducing that budget does not interrupt finite Motion transitions.
The driver sleeps until the next selected effect's sample is due. Different
cadences share one task without polling every display refresh. Rates round down
to whole display refreshes and respect the window's frame cap.

```rust,no_run
use zgui::{compose::prelude::*, effects::{EffectOptions, EffectScheduler}};

let view = component(|cx| {
    let scheduler = EffectScheduler::new(cx);
    scheduler.set_budget(3);
    let clock = scheduler.clock(cx, EffectOptions { hz: 12.0, priority: 2 });
    let frame = clock.frame();
    clock.bind(div().size(80.0, 80.0).reactive_style(move || {
        let seconds = frame.get().elapsed.as_secs_f32();
        Styles::new().opacity(0.8 + 0.2 * seconds.sin())
    }))
});
```

Lower priorities win; equal priorities keep registration order. `clock.bind(view)`
tracks clipping, offscreen geometry and native presentation visibility. Hidden,
disabled, reduced-motion and budgeted-out clocks freeze their active time and
release frame demand. Set `clock.enabled()` for application-specific eligibility;
`clock.reset()` restarts phase. Unmount disposes the clock even if an outside
handler retains a clone. Without an executor/frame service, the sample stays static.

Continuous clocks inherit `MotionPolicy` or accept one through `clock_with_policy`.
Reduced motion pauses these clocks; the application chooses a static shader state.
Finite `MotionValue` runs still finish at their target under reduced motion.
Read a MotionValue or EffectFrame inside `image_signal` to drive typed
`ShaderUniforms`; see [retained shaders and chains](images-and-animation.md).

Backdrop blur retains bounded filtered textures and compositing parameters.
Foreground changes reuse them; source changes invalidate overlapping filters in
paint order. `GpuRenderer::set_blur_algorithm(BlurAlgorithm::DualKawase)` switches
the backend, while `Gaussian` restores the reference filter. `WindowOptions`
also exposes `blur_algorithm`; the library default is Gaussian. Kawase approximates
the same sigma with a pyramid and blends adjacent depths to avoid radius jumps.
Sampling stays within each clipped panel. Both paths preserve opacity and edge fade.

See [native effects measurements](performance/effects/README.md) for GPU time,
p95/p99 frame costs, allocations and memory. This is separate from scalar
animation performance.

## Keyframes and timelines

`Keyframes<T>` validates 2..=1024 finite, strictly increasing offsets from 0 to 1
and compiles reciprocal segment spans once. `Keyframe::easing` controls the
segment starting at that keyframe. Sampling searches the compiled array; it
allocates nothing. Supported properties include `f32`, `Vec2` and `MotionColor`,
or a copyable custom `Interpolate` implementation. Interpolation must be pure.
`MotionColor` decodes sRGB endpoints once and interpolates linear premultiplied
RGBA, avoiding dark fades and transparent-color halos. Convert with `.color()`
when applying a scene color or use `.premultiplied()` for linear shader inputs.

```rust,no_run
use std::time::Duration;
use zgui::compose::prelude::*;

let view = component(|cx| {
    let timeline = Timeline::new(cx);
    let position = timeline.track(Duration::ZERO, Duration::from_millis(300),
        Keyframes::between(Vec2::new(0.0, 24.0), Vec2::default(), Easing::EaseOut)
    ).unwrap();
    let opacity = timeline.track(Duration::ZERO, Duration::from_millis(180),
        Keyframes::between(0.0, 1.0, Easing::EaseOut)
    ).unwrap();
    timeline.play();
    div().reactive_style(move || {
        let p = position.get();
        Styles::new().translate(p.x, p.y).opacity(opacity.get())
    })
});
```

Register up to 128 clips before playback. Equal start times run in parallel;
`then(duration, frames)` appends after the latest clip end. One owned scalar
transport samples every clip and batches all outputs. Timelines share the
existing scheduler with other motion values and keep their sampler alive until
component disposal, even if only output signals remain. Read output signals;
mutating them directly bypasses the transport.

`play` resumes or starts, `restart` cancels and begins again, `pause` freezes,
`stop` cancels, and `seek` samples immediately. Seeking a stopped timeline
creates a paused run, including at the endpoint; `play` completes that endpoint.
A running timeline keeps playing after seek. `set_rate(0.5)` slows playback;
rates must be finite and within 0.01..=100. `animate_to(0.0/1.0)` reverses from
its current pose with proportional remaining duration. These controls are also
available on individual `MotionValue`s.

`playback(Playback { repeat: Repeat::count(3)?, alternate: true, delay, .. })`
configures timeline loops before playback; `MotionValue::animate_keyframes`
accepts the same configuration. A closed curve such as `[0, 1, 0]` still runs.
Alternate even counts end at the starting value; ordinary repeats end at the
last keyframe. `Forever` requires positive duration and settles at the last
keyframe under reduced motion. Re-enabling motion requires an explicit restart.
Use the budgeted `EffectScheduler` above for continuous decorative shaders;
it handles visibility and lower sampling cadences.

`cx.derive(|| ...)` creates a component-owned derived signal. `MotionPoint`
provides a batched two-axis value with spring interruption and grouped
completion. Derived signals do not request frames.

## Layout, input and named states

`.layout_motion(Transition::spring(Spring::default()))` opts a retained view
into position projection. New layout applies once, then its previous visual
position translates to the final position. Size changes apply immediately;
projection composes with base/hover transforms and nested projected ancestors.
Layout observation excludes all paint transforms, including scroll offsets,
so animation frames do not trigger layout or restart the projection. Keep
identity with `keyed` when reordering rows. Newly mounted or hidden views begin
at their settled layout; this is not shared-element or size morphing.

`DragMotion::new(cx, initial, MotionAxis::X, 0.0..=360.0)?` binds direct pointer
input with `.bind(view)`. `.snap_points([0.0, 180.0, 360.0])?` selects the nearest
point to a 150 ms velocity projection on release; `.spring(...)` tunes settling.
The output signal is clamped to bounds while the underlying spring preserves
velocity. Primary pointer capture keeps dragging outside the view; secondary
release does not end it. Cancel/focus loss/disposal stop the gesture. Manual
`begin/update/release` accept an `Instant` for deterministic host input.

`ScrollProgress::new(cx).bind(scroll(offset))` publishes normalized measured
progress, using the actual content extent minus viewport extent. Resize and
content changes update it; a non-scrollable extent yields zero.

`MotionStates::new(cx, "idle", [("idle", 0.0, idle_transition),
("hover", 1.0, hover_transition)])?` supplies a bounded named scalar state table.
Derive multiple properties from its signal. `set("hover")?` applies that state's
transition; setting the current state is idempotent.

`AnimationGroup::new([translation_exit, blur_exit])` combines up to 128 runs in
one runtime. Await `.finished()` or inspect `.completion()`: all finished means
Finished; any cancelled means Cancelled. `Presence::set_present_with(false,
opacity_exit, group)` waits for its own opacity and every supplied exit before
unmounting. Create exits in the owning parent; reopening invalidates older exit
callbacks. Cancelling an exit retains the view. At most 127 extra exits can be
supplied because Presence adds its own track.

## On-demand inspector and native demo

Create `MotionInspector::new(cx)` to enable sample timing for that component's
scheduler. `.snapshot()` returns bounded registered tracks with stable IDs,
labels, state, values, velocities, active count, frame demand and the most recent
sample cost. `.label("panel / blur")` names a `MotionValue`. The inspector's
`pause/resume/seek/set_rate/stop` controls reject foreign or disposed IDs. Neither
snapshots nor controls introduce a polling task; paused/idle inspection requests
no display refreshes. Timing uses two clock reads only while an inspector owner
exists. Snapshots allocate on request and cap the registry at 4096 tracks;
`total_tracks` and `registry_truncated` indicate omitted tracks.

Native windows provide `RenderDiagnostics`, included in snapshots as the last
rendered frame's layout nodes, shader dispatches/resource allocations and blur
passes. CPU motion sample time includes batched reactive consumers; it is not
GPU time. Existing GPU benchmarks measure shader cost separately.

Run `cargo run -p zgui-desktop --example motion` for coordinated translation,
opacity, backdrop blur and a persistent dither shader, playback controls,
interruptible grouped exits, drag/snap, measured scrolling, layout projection
and inspector snapshots. The shader has a lazy matching CPU fallback.

Run regressions with `cargo test -p zgui motion --lib`. Measure typed timeline
sampling with `cargo test -p zgui --release compiled_timeline_release_benchmark
--lib -- --ignored --nocapture`.
