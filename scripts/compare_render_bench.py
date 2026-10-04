"""Compare render JSONL, repeated trials, GPU stages and CI regression gates."""
import argparse
import json
import math
from pathlib import Path
from statistics import median

def read(path):
    records, seen = {}, set()
    for line in Path(path).read_text(encoding="utf-8-sig").splitlines():
        if not line.strip():
            continue
        record = json.loads(line.removeprefix("RENDER_BENCH "))
        timings = record.get("cpu", record)
        mode = "completed_frame" if "completed_frame" in timings else "cpu_frame"
        key = (record["scene"], tuple(record["physical_size"]), record["scale"], record["workload"],
               record.get("algorithm"), record.get("sigma"), mode)
        trial = (key, record.get("trial", 0))
        if trial in seen:
            raise ValueError(f"Duplicate workload/trial in {path}: {trial}")
        seen.add(trial)
        records.setdefault(key, []).append(record)
    if not records:
        raise ValueError(f"No benchmark records in {path}")
    return records

def field(record, name):
    first = name.split(".", 1)[0]
    value = record.get("cpu", record) if first not in record and first in record.get("cpu", {}) else record
    for part in name.split("."):
        value = value[part]
    return value

def compare(before, after, metric=None, stat="p95_ms"):
    if before.keys() != after.keys():
        raise ValueError("Workloads differ: match scenes, physical sizes, scales and timing modes")
    rows = []
    for key, aa in before.items():
        bb = after[key]
        for a in aa + bb:
            for name in ("adapter", "budget_ms", "backend", "driver", "rows", "in_flight", "gpu_timestamps_supported", "build", "warmup", "frames", "panels", "surfaces"):
                if a.get(name) != aa[0].get(name):
                    raise ValueError(f"Incompatible {name}: {key}")
            for name in ("profile", "warmup", "in_flight", "rows", "rustc", "build_env"):
                if a.get("run", {}).get(name) != aa[0].get("run", {}).get(name):
                    raise ValueError(f"Incompatible run {name}: {key}")
        chosen = metric or key[-1]
        if chosen.startswith("gpu.") and any(a.get("gpu_measurements_valid") is False or a.get("gpu_profiles_dropped", 0) > 0 for a in aa + bb):
            raise ValueError(f"Incomplete GPU measurements: {key}")
        a = median(field(record, chosen)[stat] for record in aa)
        b = median(field(record, chosen)[stat] for record in bb)
        if not math.isfinite(a) or not math.isfinite(b) or a < 0 or b < 0:
            raise ValueError("Invalid timing")
        change = (b / a - 1) * 100 if a else (math.inf if b else 0)
        misses = sum(field(record, chosen).get("over_budget", 0) for record in bb)
        samples = sum(field(record, chosen)["samples"] for record in bb)
        rows.append((key, a, b, change, misses, samples, len(aa), len(bb)))
    return rows

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("before")
    p.add_argument("after")
    p.add_argument("--metric", help="e.g. gpu.scroll_copy, gpu.repaint, accessibility")
    p.add_argument("--stat", choices=("p50_ms", "p95_ms", "p99_ms", "mean_ms", "max_ms"), default="p95_ms")
    p.add_argument("--max-regression", type=float, help="Exit 1 above this percentage regression")
    args = p.parse_args()
    if args.max_regression is not None and (not math.isfinite(args.max_regression) or args.max_regression < 0):
        p.error("Regression threshold must be finite and nonnegative")
    try:
        rows = compare(read(args.before), read(args.after), args.metric, args.stat)
    except (ValueError, KeyError) as error:
        p.error(str(error))
    print(f'{"Scene / size / workload":<48} {args.stat:>18} {"change":>10} {"budget misses":>14} {"trials":>8}')
    for key, a, b, change, misses, samples, na, nb in rows:
        label = f'{key[0]} / {key[1][0]}x{key[1][1]} @{key[2]:g} / {key[3]}'
        if key[4] is not None:
            label += f' / {key[4]}'
        if key[5] is not None:
            label += f' σ={key[5]:g}'
        print(f'{label:<48} {a:7.3f} -> {b:7.3f} {change:+9.1f}% {misses:5}/{samples:<8} {na:3}->{nb:<3}')
    if args.max_regression is not None and any(row[3] > args.max_regression for row in rows):
        raise SystemExit(1)

if __name__ == "__main__":
    main()

