//! Host metric parsers over `/proc` plus the statfs disk probe. Pure
//! functions for parsing (fixture-driven unit tests), thin syscall wrapper
//! for disk. Mirrors the monitor-dashboard spec's host `/proc` sampling.

use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuCounters {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

/// Parse the aggregate `cpu ...` line from `/proc/stat`.
pub fn parse_cpu_counters(line: &str) -> Option<CpuCounters> {
    let mut it = line.split_whitespace();
    if it.next() != Some("cpu") {
        return None;
    }
    let mut fields = [0u64; 8];
    for f in fields.iter_mut() {
        *f = it.next()?.parse().ok()?;
    }
    Some(CpuCounters {
        user: fields[0],
        nice: fields[1],
        system: fields[2],
        idle: fields[3],
        iowait: fields[4],
        irq: fields[5],
        softirq: fields[6],
        steal: fields[7],
    })
}

impl CpuCounters {
    fn busy(&self) -> u64 {
        self.user + self.nice + self.system + self.iowait + self.irq + self.softirq + self.steal
    }

    fn total(&self) -> u64 {
        self.busy() + self.idle
    }
}

/// CPU busy percentage between two consecutive reads; 0 if no tick progress.
pub fn cpu_busy_percent(prev: &CpuCounters, cur: &CpuCounters) -> f64 {
    let total_delta = cur.total().saturating_sub(prev.total());
    if total_delta == 0 {
        return 0.0;
    }
    let busy_delta = cur.busy().saturating_sub(prev.busy());
    (busy_delta as f64 / total_delta as f64) * 100.0
}

/// Number of logical CPUs in `/proc/stat` text, counted from the `cpuN` per-CPU
/// lines (the aggregate `cpu` line is excluded). 0 when the text is malformed.
fn count_cpu_lines(text: &str) -> u64 {
    text.lines()
        .filter(|line| {
            line.starts_with("cpu")
                && line
                    .as_bytes()
                    .get(3)
                    .is_some_and(|b| b.is_ascii_digit())
        })
        .count() as u64
}

/// Number of logical CPUs on the host (from `/proc/stat`), at least 1.
pub fn host_cpu_count() -> u64 {
    std::fs::read_to_string("/proc/stat")
        .map(|text| count_cpu_lines(&text))
        .unwrap_or(0)
        .max(1)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemInfo {
    pub total_kb: u64,
    pub available_kb: u64,
}

/// Parse MemTotal / MemAvailable from `/proc/meminfo`.
pub fn parse_meminfo(text: &str) -> Option<MemInfo> {
    let mut total_kb: Option<u64> = None;
    let mut available_kb: Option<u64> = None;
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(key) = it.next() else { continue };
        let Some(val) = it.next() else { continue };
        let Ok(val) = val.parse::<u64>() else { continue };
        match key.trim_end_matches(':') {
            "MemTotal" => total_kb = Some(val),
            "MemAvailable" => available_kb = Some(val),
            _ => {}
        }
    }
    Some(MemInfo { total_kb: total_kb?, available_kb: available_kb? })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiskUsage {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

/// Pure: derive used/total bytes from statfs counts.
pub fn disk_usage_from_counts(total_blocks: u64, free_blocks: u64, block_bytes: u64) -> DiskUsage {
    let total_bytes = total_blocks.saturating_mul(block_bytes);
    let free_bytes = free_blocks.saturating_mul(block_bytes);
    DiskUsage {
        used_bytes: total_bytes.saturating_sub(free_bytes),
        total_bytes,
    }
}

/// statfs on the host root filesystem.
pub fn host_disk_usage() -> io::Result<DiskUsage> {
    let fs = rustix::fs::statfs(Path::new("/"))?;
    Ok(disk_usage_from_counts(fs.f_blocks, fs.f_bavail, fs.f_frsize as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT_FIXTURE: &str =
        "cpu  51428 5448 28883 32964784 39949 0 3098 0 0 0";

    fn base_counters() -> CpuCounters {
        CpuCounters {
            user: 51428,
            nice: 5448,
            system: 28883,
            idle: 32964784,
            iowait: 39949,
            irq: 0,
            softirq: 3098,
            steal: 0,
        }
    }

    #[test]
    fn parse_cpu_counters_reads_all_fields() {
        let c = parse_cpu_counters(STAT_FIXTURE).expect("valid line");
        assert_eq!(c, base_counters());
    }

    #[test]
    fn parse_cpu_counters_rejects_malformed() {
        assert_eq!(parse_cpu_counters(""), None);
        assert_eq!(parse_cpu_counters("cpu"), None);
        assert_eq!(parse_cpu_counters("cpu  1 2 3"), None);
        assert_eq!(parse_cpu_counters("cpu  1 2 3 4 5 6 7 nope 9 10"), None);
        assert_eq!(parse_cpu_counters("cpu0  1 2 3 4 5 6 7 8 9 10"), None);
        assert_eq!(parse_cpu_counters("bogus  1 2 3 4 5 6 7 8 9 10"), None);
    }

    #[test]
    fn cpu_busy_percent_computes_delta() {
        let prev = base_counters();
        let cur = CpuCounters { user: 51678, idle: 32965534, ..prev };
        let pct = cpu_busy_percent(&prev, &cur);
        assert!((pct - 25.0).abs() < 1e-9);
    }

    #[test]
    fn cpu_busy_percent_no_tick_progress_is_zero() {
        let prev = base_counters();
        assert_eq!(cpu_busy_percent(&prev, &prev), 0.0);
    }

    #[test]
    fn cpu_busy_percent_counter_reset_is_zero() {
        let prev = base_counters();
        let cur = CpuCounters { user: 51400, ..prev };
        assert_eq!(cpu_busy_percent(&prev, &cur), 0.0);
    }

    #[test]
    fn count_cpu_lines_counts_per_cpu_lines_not_aggregate() {
        let text = "\
cpu  51428 5448 28883 32964784 39949 0 3098 0 0 0
cpu0  1 2 3 4 5 6 7 8 9 10
cpu1  1 2 3 4 5 6 7 8 9 10
cpu2  1 2 3 4 5 6 7 8 9 10
cpu03  1 2 3 4 5 6 7 8 9 10
intr 1234
";
        assert_eq!(count_cpu_lines(text), 4);
    }

    #[test]
    fn count_cpu_lines_malformed_is_zero() {
        assert_eq!(count_cpu_lines(""), 0);
        assert_eq!(count_cpu_lines("cpu  1 2 3\n"), 0);
        assert_eq!(count_cpu_lines("bogus\nintr 1\n"), 0);
    }

    #[test]
    fn host_cpu_count_reads_proc_stat() {
        assert!(host_cpu_count() >= 1);
    }

    const MEMINFO_FIXTURE: &str = "\
MemTotal:       32768512 kB
MemFree:        1234567 kB
MemAvailable:   26000000 kB
Buffers:         543210 kB
Cached:         4321098 kB
";

    #[test]
    fn parse_meminfo_reads_total_and_available() {
        let m = parse_meminfo(MEMINFO_FIXTURE).expect("valid meminfo");
        assert_eq!(m.total_kb, 32768512);
        assert_eq!(m.available_kb, 26000000);
    }

    #[test]
    fn parse_meminfo_tolerates_extra_and_blank_lines() {
        let text = "\n\nMemTotal:       8 kB\n   \nMemAvailable:   4 kB\n\n";
        let m = parse_meminfo(text).expect("tolerant parse");
        assert_eq!((m.total_kb, m.available_kb), (8, 4));
    }

    #[test]
    fn parse_meminfo_missing_fields_is_none() {
        assert_eq!(parse_meminfo(""), None);
        assert_eq!(parse_meminfo("MemTotal:       8 kB\n"), None);
        assert_eq!(parse_meminfo("MemAvailable:   4 kB\n"), None);
        assert_eq!(parse_meminfo("garbage line here\n"), None);
    }

    #[test]
    fn disk_usage_from_counts_matches_math() {
        let d = disk_usage_from_counts(1000, 250, 4096);
        assert_eq!(d.total_bytes, 4_096_000);
        assert_eq!(d.used_bytes, 3_072_000);
    }

    #[test]
    fn host_disk_usage_reads_root_filesystem() {
        let d = host_disk_usage().expect("statfs works on /");
        assert!(d.total_bytes > 0);
        assert!(d.used_bytes <= d.total_bytes);
    }
}
