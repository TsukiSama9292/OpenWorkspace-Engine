//! Monitor-dashboard sampling pass (monitor-dashboard spec Decision 2): one
//! full pass reads host `/proc` metrics and one-shot stats for every active
//! instance, folding everything into the shared in-memory `MetricsStore`.
//! Runs in the health worker on every 5th tick (15 s).

use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::db::WorkspaceInstanceRepository;
use crate::docker::DockerService;
use crate::metrics::{MetricsStore, Sample};
use crate::proc::{self, CpuCounters};

/// The health worker ticks every 3 s; a stats pass happens on every
/// `SAMPLE_EVERY_TICKS`th tick, i.e. every 15 s.
pub const SAMPLE_EVERY_TICKS: u32 = 5;

/// Which instance statuses the Monitor treats as active (sampled + listed).
pub const ACTIVE_STATUSES: [&str; 3] = ["running", "starting", "paused"];

/// Stateful sampler: holds the previous host `/proc/stat` counters so CPU %
/// can be computed from the delta between consecutive passes.
#[derive(Default)]
pub struct MetricsSampler {
    prev_host_cpu: Option<CpuCounters>,
}

impl MetricsSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// One full sampling pass (host + active instances). Every failure is
    /// fail-open: a failed host read leaves zeros for that metric, a failed
    /// `container_stats` read logs and skips that instance. Returns the ids of
    /// the active instances sampled.
    pub async fn sample_once(
        &mut self,
        db: &DatabaseConnection,
        docker: &dyn DockerService,
        metrics: &MetricsStore,
    ) -> Vec<Uuid> {
        let ts = chrono::Utc::now().timestamp();

        metrics.record_host(self.host_sample(ts));

        let repo = WorkspaceInstanceRepository::new(db);
        let active = repo
            .list_active_for_monitoring()
            .await
            .unwrap_or_default();

        for inst in &active {
            let Some(container_id) = inst.container_id.as_deref() else {
                continue;
            };
            match docker.container_stats(container_id).await {
                Ok(stats) => {
                    metrics.record_instance(
                        inst.id,
                        Sample {
                            ts,
                            cpu_percent: stats.cpu_percent.unwrap_or(0.0),
                            mem_used_bytes: stats.mem_used_bytes,
                            mem_total_bytes: stats.mem_limit_bytes,
                            disk_used_bytes: 0,
                            disk_total_bytes: 0,
                        },
                    );
                }
                Err(e) => tracing::warn!(
                    "monitor: container stats failed for '{}': {}",
                    inst.name,
                    e
                ),
            }
        }

        let active_ids: Vec<Uuid> = active.iter().map(|i| i.id).collect();
        metrics.retain_active(&active_ids);
        active_ids
    }

    fn host_sample(&mut self, ts: i64) -> Sample {
        let mut sample = Sample {
            ts,
            cpu_percent: 0.0,
            mem_used_bytes: 0,
            mem_total_bytes: 0,
            disk_used_bytes: 0,
            disk_total_bytes: 0,
        };

        if let Ok(text) = std::fs::read_to_string("/proc/stat")
            && let Some(line) = text.lines().next()
            && let Some(cur) = proc::parse_cpu_counters(line)
        {
            sample.cpu_percent = match self.prev_host_cpu {
                Some(prev) => proc::cpu_busy_percent(&prev, &cur),
                None => 0.0,
            };
            self.prev_host_cpu = Some(cur);
        }

        if let Ok(text) = std::fs::read_to_string("/proc/meminfo")
            && let Some(mem) = proc::parse_meminfo(&text)
        {
            sample.mem_total_bytes = mem.total_kb * 1024;
            sample.mem_used_bytes = mem.total_kb.saturating_sub(mem.available_kb) * 1024;
        }

        if let Ok(disk) = proc::host_disk_usage() {
            sample.disk_used_bytes = disk.used_bytes;
            sample.disk_total_bytes = disk.total_bytes;
        }

        sample
    }
}
