import json
from pathlib import Path
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

if __name__=="__main__":
    unittest.main()

