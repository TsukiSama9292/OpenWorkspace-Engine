//! Two-tier in-memory metrics store backing the Monitor dashboard
//! (monitor-dashboard spec). Pure logic — no I/O, no Docker — so every
//! behavior is unit-testable in isolation.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use uuid::Uuid;

/// Tier-1 ring capacity: 240 samples at 15 s = 1 hour.
pub const TIER1_CAPACITY: usize = 240;
/// Tier-2 ring capacity: 288 five-minute aggregates = 24 hours.
pub const TIER2_CAPACITY: usize = 288;
/// Samples folded per Tier-2 aggregate: 20 × 15 s = 5 minutes.
pub const WINDOW_SAMPLES: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub ts: i64,
    pub cpu_percent: f64,
    pub mem_used_bytes: u64,
    /// Host RAM total (host samples) or the container memory limit (instance
    /// samples). Constant per entity; taken from the latest sample.
    pub mem_total_bytes: u64,
    /// Host disk totals; zero for instance samples.
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AggregatedSample {
    pub ts: i64,
    pub cpu_mean: f64,
    pub cpu_peak: f64,
    pub mem_used_mean: u64,
    pub mem_used_peak: u64,
    pub mem_total_bytes: u64,
    pub disk_used_mean: u64,
    pub disk_total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Range {
    /// Tier-1 fine-grained series (15 s), 1 hour.
    Hour,
    /// Tier-2 aggregated series (5 min), 24 hours.
    Day,
}

/// What the snapshot endpoint returns for one entity (host or instance).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntitySnapshot {
    pub cpu_percent: f64,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
    pub cpu_series: Vec<f64>,
    pub mem_series: Vec<u64>,
    pub disk_series: Vec<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub host: EntitySnapshot,
    pub instances: Vec<(Uuid, EntitySnapshot)>,
}

pub struct MetricsStore {
    inner: Mutex<MetricsState>,
}

struct MetricsState {
    host: EntityMetrics,
    instances: HashMap<Uuid, EntityMetrics>,
}

#[derive(Default)]
struct EntityMetrics {
    tier1: VecDeque<Sample>,
    tier2: VecDeque<AggregatedSample>,
    window: VecDeque<Sample>,
}

/// Aggregate a window of samples into one Tier-2 aggregate: mean + peak of
/// CPU % and RAM usage, mean disk usage, and the totals from the window end.
pub fn aggregate_window(window: &[Sample]) -> AggregatedSample {
    assert!(!window.is_empty(), "cannot aggregate an empty window");
    let n = window.len() as f64;
    let mut cpu_sum = 0.0_f64;
    let mut cpu_peak = 0.0_f64;
    let mut mem_sum: u64 = 0;
    let mut mem_peak: u64 = 0;
    let mut disk_sum: u64 = 0;
    for s in window {
        cpu_sum += s.cpu_percent;
        cpu_peak = cpu_peak.max(s.cpu_percent);
        mem_sum += s.mem_used_bytes;
        mem_peak = mem_peak.max(s.mem_used_bytes);
        disk_sum += s.disk_used_bytes;
    }
    let last = window.last().expect("window checked non-empty");
    AggregatedSample {
        ts: last.ts,
        cpu_mean: cpu_sum / n,
        cpu_peak,
        mem_used_mean: (mem_sum as f64 / n) as u64,
        mem_used_peak: mem_peak,
        mem_total_bytes: last.mem_total_bytes,
        disk_used_mean: (disk_sum as f64 / n) as u64,
        disk_total_bytes: last.disk_total_bytes,
    }
}

impl EntityMetrics {
    fn new() -> Self {
        Self::default()
    }

    fn record(&mut self, sample: Sample) {
        if self.tier1.len() == TIER1_CAPACITY {
            self.tier1.pop_front();
        }
        self.tier1.push_back(sample);

        if self.window.len() == WINDOW_SAMPLES {
            self.window.pop_front();
        }
        self.window.push_back(sample);
        if self.window.len() == WINDOW_SAMPLES {
            let window = self.window.iter().cloned().collect::<Vec<_>>();
            if self.tier2.len() == TIER2_CAPACITY {
                self.tier2.pop_front();
            }
            self.tier2.push_back(aggregate_window(&window));
            self.window.clear();
        }
    }
}

impl EntitySnapshot {
    fn from_metrics(m: &EntityMetrics, range: Range) -> Self {
        let latest = m.tier1.back();
        let (cpu_series, mem_series, disk_series) = match range {
            Range::Hour => (
                m.tier1.iter().map(|s| s.cpu_percent).collect(),
                m.tier1.iter().map(|s| s.mem_used_bytes).collect(),
                m.tier1.iter().map(|s| s.disk_used_bytes).collect(),
            ),
            Range::Day => (
                m.tier2.iter().map(|s| s.cpu_mean).collect(),
                m.tier2.iter().map(|s| s.mem_used_mean).collect(),
                m.tier2.iter().map(|s| s.disk_used_mean).collect(),
            ),
        };
        EntitySnapshot {
            cpu_percent: latest.map(|s| s.cpu_percent).unwrap_or(0.0),
            mem_used_bytes: latest.map(|s| s.mem_used_bytes).unwrap_or(0),
            mem_total_bytes: latest.map(|s| s.mem_total_bytes).unwrap_or(0),
            disk_used_bytes: latest.map(|s| s.disk_used_bytes).unwrap_or(0),
            disk_total_bytes: latest.map(|s| s.disk_total_bytes).unwrap_or(0),
            cpu_series,
            mem_series,
            disk_series,
        }
    }
}

impl Default for MetricsState {
    fn default() -> Self {
        Self {
            host: EntityMetrics::new(),
            instances: HashMap::new(),
        }
    }
}

impl MetricsStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(MetricsState::default()),
        }
    }

    pub fn record_host(&self, sample: Sample) {
        self.inner
            .lock()
            .expect("metrics lock poisoned")
            .host
            .record(sample);
    }

    pub fn record_instance(&self, id: Uuid, sample: Sample) {
        let mut state = self.inner.lock().expect("metrics lock poisoned");
        let m = state.instances.entry(id).or_default();
        m.record(sample);
    }

    /// Drop series for instances no longer active (bounded memory).
    pub fn retain_active(&self, ids: &[Uuid]) {
        let mut state = self.inner.lock().expect("metrics lock poisoned");
        state.instances.retain(|id, _| ids.contains(id));
    }

    pub fn snapshot(&self, range: Range) -> Snapshot {
        let state = self.inner.lock().expect("metrics lock poisoned");
        Snapshot {
            host: EntitySnapshot::from_metrics(&state.host, range),
            instances: state
                .instances
                .iter()
                .map(|(id, m)| (*id, EntitySnapshot::from_metrics(m, range)))
                .collect(),
        }
    }
}

impl Default for MetricsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ts: i64, cpu: f64, mem: u64) -> Sample {
        Sample {
            ts,
            cpu_percent: cpu,
            mem_used_bytes: mem,
            mem_total_bytes: 32_000_000_000,
            disk_used_bytes: 180_000_000_000,
            disk_total_bytes: 500_000_000_000,
        }
    }

    #[test]
    fn tier1_evicts_oldest_past_capacity() {
        let store = MetricsStore::new();
        for i in 0..(TIER1_CAPACITY + 1) {
            store.record_host(sample(i as i64, i as f64, i as u64));
        }
        let snap = store.snapshot(Range::Hour);
        assert_eq!(snap.host.cpu_series.len(), TIER1_CAPACITY);
        assert_eq!(snap.host.cpu_series[0], 1.0);
        assert_eq!(*snap.host.cpu_series.last().unwrap(), TIER1_CAPACITY as f64);
    }

    #[test]
    fn aggregate_window_computes_mean_and_peak() {
        let window = vec![sample(1, 10.0, 100), sample(2, 30.0, 300), sample(3, 20.0, 200)];
        let agg = aggregate_window(&window);
        assert_eq!(agg.ts, 3);
        assert_eq!(agg.cpu_mean, 20.0);
        assert_eq!(agg.cpu_peak, 30.0);
        assert_eq!(agg.mem_used_mean, 200);
        assert_eq!(agg.mem_used_peak, 300);
        assert_eq!(agg.disk_used_mean, 180_000_000_000);
    }

    #[test]
    fn tier2_promotes_every_20_samples() {
        let store = MetricsStore::new();
        for i in 0..40 {
            store.record_host(sample(i as i64, i as f64, i as u64));
        }
        let snap = store.snapshot(Range::Day);
        assert_eq!(snap.host.cpu_series.len(), 2);
        assert!((snap.host.cpu_series[0] - 9.5).abs() < 1e-9);
    }

    #[test]
    fn tier2_capacity_is_24h() {
        let store = MetricsStore::new();
        for i in 0..(TIER2_CAPACITY * WINDOW_SAMPLES + WINDOW_SAMPLES) {
            store.record_host(sample(i as i64, i as f64, i as u64));
        }
        let snap = store.snapshot(Range::Day);
        assert_eq!(snap.host.cpu_series.len(), TIER2_CAPACITY);
    }

    #[test]
    fn snapshot_latest_values_from_last_sample() {
        let store = MetricsStore::new();
        store.record_host(sample(1, 11.0, 5_000_000_000));
        let snap = store.snapshot(Range::Hour);
        assert_eq!(snap.host.cpu_percent, 11.0);
        assert_eq!(snap.host.mem_used_bytes, 5_000_000_000);
        assert_eq!(snap.host.mem_total_bytes, 32_000_000_000);
        assert_eq!(snap.host.disk_used_bytes, 180_000_000_000);
        assert_eq!(snap.host.disk_total_bytes, 500_000_000_000);
    }

    #[test]
    fn instances_are_keyed_and_snapshotted() {
        let store = MetricsStore::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        store.record_instance(a, sample(1, 42.0, 1_000));
        store.record_instance(b, sample(1, 7.0, 2_000));
        let snap = store.snapshot(Range::Hour);
        assert_eq!(snap.instances.len(), 2);
        let snap_a = snap.instances.iter().find(|(id, _)| *id == a).unwrap();
        assert_eq!(snap_a.1.cpu_percent, 42.0);
    }

    #[test]
    fn retain_active_drops_stale_instances() {
        let store = MetricsStore::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        store.record_instance(a, sample(1, 1.0, 1));
        store.record_instance(b, sample(1, 2.0, 2));
        store.retain_active(&[a]);
        let snap = store.snapshot(Range::Hour);
        assert_eq!(snap.instances.len(), 1);
        assert_eq!(snap.instances[0].0, a);
    }

    #[test]
    fn empty_store_snapshots_zeros() {
        let store = MetricsStore::new();
        let snap = store.snapshot(Range::Hour);
        assert_eq!(snap.host.cpu_percent, 0.0);
        assert!(snap.host.cpu_series.is_empty());
        assert!(snap.instances.is_empty());
    }
}
