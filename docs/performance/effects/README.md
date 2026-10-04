# Native effects baseline

[effects_bench.rs](../../../crates/zgui-gpu/examples/effects_bench.rs) runs native offscreen GPU workloads. [amd-vulkan.jsonl](amd-vulkan.jsonl) contains the complete results recorded on 2026-10-03: AMD Radeon(TM) Graphics, Vulkan, AMD proprietary driver, release build, 1280×720 at scale 1, 30 warmup frames and 240 measured frames per case. GPU timestamps were supported and no profiles were dropped.

Four translucent panels overlap the same patterned backdrop. Each panel covers approximately 486×346 physical pixels. `overlap` changes backdrop opacity; `scroll` translates clipped scroll content; `foreground` blinks a cursor painted after the filters; `resize` resizes the target and panel geometry. Separate cases animate typed uniforms, resize shader textures, update eight dither surfaces, and animate the dither suffix of a static `blur → dither` chain.

GPU values below are p50 / p95 / p99 milliseconds, measured from native timestamp queries. Both blur algorithms use the same nominal sigma; their kernels produce different images. Ratios compare medians for this scene and device.

| Workload | Sigma | Gaussian GPU ms | Dual Kawase GPU ms | Median speedup |
| --- | ---: | ---: | ---: | ---: |
| Overlapping panels | 6 | 2.256 / 2.363 / 4.948 | 1.309 / 3.317 / 4.645 | 1.72× |
| Overlapping panels | 24 | 6.537 / 6.726 / 13.524 | 1.379 / 3.620 / 3.919 | 4.74× |
| Scrolling beneath panels | 24 | 6.483 / 6.697 / 13.399 | 1.323 / 3.439 / 3.713 | 4.90× |
| Target and panel resize | 24 | 6.363 / 6.496 / 13.202 | 1.323 / 3.515 / 3.826 | 4.81× |
| Foreground cursor, cached source | 24 | 0.355 / 0.368 / 0.381 | 0.346 / 0.356 / 0.363 | 1.03× |

Kawase encodes more passes: overlap sigma 24 used 0.734 ms median CPU encode/submit versus Gaussian's 0.302 ms. Completed-frame medians, including the benchmark's explicit GPU wait, were 2.271 versus 6.982 ms. At sigma 6, Kawase's median improved but its p95 was higher; these measurements do not justify a universal speedup claim or an automatic quality cutoff.

Cached foreground frames filtered zero pixels, with four blur cache hits per frame and a cheap composite for either backend. Recomputed overlap frames filtered 675,328 output pixels per frame. Gaussian retained 9 MiB of blur textures; Kawase retained 6 MiB plus approximately 0.50 MiB of shared pyramid scratch. These counts exclude render targets and driver allocations.

One animated shader surface cost 0.169 ms median GPU time; eight dither surfaces cost 0.699 ms. Both allocated zero shader resources after warmup. Shader resize allocated one resource bundle per frame. The animated chain cost 0.182 ms, reused its blur stage every frame, and dispatched only its dither stage, with zero resource allocations. The runner asserts these cache and allocation invariants.

Run from the zgui workspace:

```powershell
$env:ZGUI_BENCH_OUTPUT = 'effects-results.jsonl'
cargo run --release -p zgui-gpu --features benchmark --example effects_bench
```

The runner appends JSONL records. Use a fresh output path for each baseline. `ZGUI_BENCH_FRAMES`, `ZGUI_BENCH_WARMUP`, `ZGUI_BENCH_ONLY` (substring filter), `ZGUI_BENCH_SIZES` (e.g. `1280x720,1920x1080`), `ZGUI_BENCH_SIGMAS` (e.g. `6,24`), `ZGUI_BENCH_HZ`, `ZGUI_BENCH_IN_FLIGHT`, and `ZGUI_BENCH_TRACE` configure sampling and an optional Perfetto CPU trace. Default in-flight count is one; waits and profiling belong to the benchmark, not production scheduling. No image readback is included in timed frames.

The JSON records contain CPU stage distributions, per-label GPU distributions, filtered output pixels, allocation counters, and retained memory. GPU `total` is the interval between the first and last timestamp, including gaps between passes. Each per-label value sums that label's spans within a frame. Shader-resource and effect-chain memory include final textures also counted in image memory; do not sum those overlapping fields. This is one unpaced run on a shared desktop, so inspect tails and rerun on target hardware before choosing budgets.
