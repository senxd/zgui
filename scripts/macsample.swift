import Foundation
// macsample <pid> <delay> <seconds> <interval>: CPU and memory of a process
// between delay and delay+seconds, polled every interval. JSON on stdout.
let a = CommandLine.arguments
let pid = pid_t(a[1])!, delay = Double(a[2])!, seconds = Double(a[3])!, interval = Double(a[4])!
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func sample() -> rusage_info_v4? {
  var info = rusage_info_v4()
  let ok = withUnsafeMutablePointer(to: &info) { p in
    p.withMemoryRebound(to: rusage_info_t?.self, capacity: 1) { proc_pid_rusage(pid, RUSAGE_INFO_V4, $0) }
  }
  return ok == 0 ? info : nil
}
let ns = { (t: UInt64) in Double(t) * Double(tb.numer) / Double(tb.denom) / 1e9 }
Thread.sleep(forTimeInterval: delay)
guard let first = sample() else { print("{\"error\":\"exited\"}"); exit(1) }
let start = Date()
var footprints: [Double] = [], residents: [Double] = []
var last = first
while Date().timeIntervalSince(start) < seconds {
  Thread.sleep(forTimeInterval: interval)
  guard let s = sample() else { break }
  footprints.append(Double(s.ri_phys_footprint)); residents.append(Double(s.ri_resident_size)); last = s
}
let wall = Date().timeIntervalSince(start)
let cpu = ns(last.ri_user_time - first.ri_user_time) + ns(last.ri_system_time - first.ri_system_time)
let mean = { (v: [Double]) in v.isEmpty ? 0 : v.reduce(0, +) / Double(v.count) }
print(String(format: "{\"wall_seconds\":%.4f,\"cpu_seconds\":%.4f,\"cpu_percent_one_core\":%.4f,\"mean_footprint_bytes\":%.0f,\"peak_footprint_bytes\":%.0f,\"mean_rss_bytes\":%.0f,\"peak_rss_bytes\":%.0f,\"samples\":%d}",
  wall, cpu, cpu / wall * 100, mean(footprints), footprints.max() ?? 0, mean(residents), residents.max() ?? 0, footprints.count))
