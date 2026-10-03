import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from compare_render_bench import read, compare
from render_bench import benchmark_record

def record(trial=1):
    return {"scene":"standard","physical_size":[80,60],"scale":1,"workload":"scroll",
        "adapter":"test","budget_ms":7,"trial":trial,
        "completed_frame":{"p95_ms":2,"samples":10,"over_budget":0},
        "gpu":{"scroll_copy":{"p95_ms":1,"samples":10,"over_budget":0}}}

class ComparisonTests(unittest.TestCase):
    def test_rust_harness_prefix_keeps_first_record(self):
        payload=json.dumps(record())
        self.assertEqual(benchmark_record("RENDER_BENCH "+payload),record())
        self.assertEqual(benchmark_record("test render_matrix ... RENDER_BENCH "+payload),record())
        self.assertIsNone(benchmark_record("test render_matrix ... ok"))
    def write(self, path, rows):
        path.write_text("\n".join(json.dumps(row) for row in rows),encoding="utf-8")
    def test_repeated_trials_and_gpu_stage(self):
        with tempfile.TemporaryDirectory() as directory:
            a,b=Path(directory)/"a.jsonl",Path(directory)/"b.jsonl"
            rows=[record(1),record(2)]
            self.write(a,rows)
            rows[1]["gpu"]["scroll_copy"]["p95_ms"]=3
            self.write(b,rows)
            result=compare(read(a),read(b),"gpu.scroll_copy")[0]
            self.assertEqual(result[1:4],(1,2,100))
            self.assertEqual(result[-2:],(2,2))
    def test_duplicate_empty_and_incompatible_runs_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            a,b=Path(directory)/"a.jsonl",Path(directory)/"b.jsonl"
            self.write(a,[record(),record()])
            with self.assertRaises(ValueError): read(a)
            self.write(a,[])
            with self.assertRaises(ValueError): read(a)
            self.write(a,[record()])
            for property,value in [("adapter","other"),("scale",2),("budget_ms",8),("in_flight",3)]:
                row=record()
                row[property]=value
                self.write(b,[row])
                with self.assertRaises(ValueError): compare(read(a),read(b))
            for property,value in [("rustc","other"),("build_env",{"RUSTFLAGS":"-C opt-level=0"})]:
                row=record(); row["run"]={property:value}
                self.write(b,[row])
                with self.assertRaises(ValueError): compare(read(a),read(b))
    def test_zero_baseline_and_missing_metric(self):
        with tempfile.TemporaryDirectory() as directory:
            a,b=Path(directory)/"a.jsonl",Path(directory)/"b.jsonl"
            row=record(); row["completed_frame"]["p95_ms"]=0
            self.write(a,[row]); self.write(b,[record()])
            self.assertEqual(compare(read(a),read(b))[0][3],float("inf"))
            with self.assertRaises(KeyError): compare(read(a),read(b),"gpu.unsupported")

    def test_effects_keep_algorithms_radii_and_nested_timings_distinct(self):
        with tempfile.TemporaryDirectory() as directory:
            a,b=Path(directory)/"a.jsonl",Path(directory)/"b.jsonl"
            rows=[]
            for algorithm in ("Gaussian","DualKawase"):
                for sigma in (6,24):
                    row=record()
                    row.update(scene="effects",algorithm=algorithm,sigma=sigma,build="release",warmup=30,frames=240)
                    row["cpu"]={"completed_frame":row.pop("completed_frame")}
                    rows.append(row)
            self.write(a,rows); self.write(b,rows)
            self.assertEqual(len(read(a)),4)
            self.assertTrue(all(r[1:4]==(2,2,0) for r in compare(read(a),read(b))))
            self.assertTrue(all(r[1:4]==(2,2,0) for r in compare(read(a),read(b),"cpu.completed_frame")))
            for property,value in [("build","debug"),("warmup",0),("frames",12)]:
                changed=json.loads(json.dumps(rows)); changed[0][property]=value
                self.write(b,changed)
                with self.assertRaises(ValueError): compare(read(a),read(b))

    def test_new_heavy_baseline_does_not_inherit_old_dirty_source_note(self):
        root=Path(__file__).resolve().parents[1]
        historical=root/"docs/results/latest-macos-heavy-2026-09-27-92fd6a4"
        with tempfile.TemporaryDirectory() as directory:
            fixture=Path(directory)/"fresh-heavy"; fixture.mkdir()
            for name in ("summary.json","audit.json","current.csv"):
                shutil.copyfile(historical/name,fixture/name)
            audit=json.loads((fixture/"audit.json").read_text())
            audit.update(commit="a"*40,source_dirty=False)
            (fixture/"audit.json").write_text(json.dumps(audit))
            output=Path(directory)/"site"
            subprocess.run([sys.executable,str(root/"scripts/build_macos_site.py"),str(fixture),"--output",str(output)],check=True,capture_output=True)
            metadata=json.loads((output/"measurement.json").read_text())
            self.assertEqual(metadata["commit"],"a"*40)
            self.assertFalse(metadata["source_dirty"])
            self.assertNotIn("source_note",metadata)

    def test_incomplete_gpu_samples_cannot_pass_a_regression_gate(self):
        with tempfile.TemporaryDirectory() as directory:
            a,b=Path(directory)/"a.jsonl",Path(directory)/"b.jsonl"
            self.write(a,[record()])
            for property,value in [("gpu_measurements_valid",False),("gpu_profiles_dropped",1)]:
                changed=record(); changed[property]=value
                self.write(b,[changed])
                with self.assertRaisesRegex(ValueError,"Incomplete GPU"):
                    compare(read(a),read(b),"gpu.scroll_copy")
                self.assertEqual(compare(read(a),read(b))[0][3],0)

if __name__=="__main__":
    unittest.main()

