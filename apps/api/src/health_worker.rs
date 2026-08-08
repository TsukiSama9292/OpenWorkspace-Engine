use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sea_orm::DatabaseConnection;
use tokio::time::MissedTickBehavior;

use crate::audit::{action, target, due_for_prune, retention_cutoff, AuditEvent, AuditSender};
use crate::db::{AuditLogRepository, WorkspaceInstanceRepository, WorkspaceTemplateRepository};
use crate::docker::{DockerService, RemoteType};
use crate::metrics::MetricsStore;
use crate::monitor::{MetricsSampler, SAMPLE_EVERY_TICKS};
use crate::vnc_cache::VncCache;

const PROBE_TIMEOUT_SECS: i64 = 120;

pub async fn run(
    db: DatabaseConnection,
    docker: Arc<dyn DockerService>,
    vnc_cache: VncCache,
    host_gateway_ip: String,
    metrics: Arc<MetricsStore>,
    audit: AuditSender,
    audit_retention_days: i64,
) {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to build reqwest client for health worker");

    let mut interval = tokio::time::interval(Duration::from_secs(3));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut sampler = MetricsSampler::new();
    let mut tick: u32 = 0;
    let mut last_prune_at: Option<chrono::DateTime<Utc>> = None;

    loop {
        interval.tick().await;

        tick += 1;
        if tick.is_multiple_of(SAMPLE_EVERY_TICKS) {
            let active = sampler.sample_once(&db, &*docker, &metrics).await;
            if !active.is_empty() {
                tracing::debug!("Monitor sampled {} active instances", active.len());
            }
        }

        let instance_repo = WorkspaceInstanceRepository::new(&db);
        let template_repo = WorkspaceTemplateRepository::new(&db);

        match check_instances(&instance_repo, &vnc_cache, &client, &host_gateway_ip).await {
            Ok(count) => {
                if count > 0 {
                    tracing::debug!("Health worker updated {} instances", count);
                }
            }
            Err(e) => tracing::error!("Health worker error: {}", e),
        }

        match check_auto_sleep(&instance_repo, &template_repo, &*docker, &vnc_cache, Some(&audit), Utc::now()).await {
            Ok(count) => {
                if count > 0 {
                    tracing::debug!("Auto-sleep worker acted on {} instances", count);
                }
            }
            Err(e) => tracing::error!("Auto-sleep worker error: {}", e),
        }

        match check_keep_time(&instance_repo, &template_repo, &*docker, &vnc_cache, Some(&audit), Utc::now()).await {
            Ok(count) => {
                if count > 0 {
                    tracing::debug!("Keep-time worker acted on {} instances", count);
                }
            }
            Err(e) => tracing::error!("Keep-time worker error: {}", e),
        }

        // Daily audit-log prune (observability-logs spec Decision 7): runs at
        // most once per 24 h behind the pure `due_for_prune` gate. Best-effort
        // like every health-worker task.
        let now = Utc::now();
        match maybe_prune_audit(&db, last_prune_at, now, audit_retention_days).await {
            Ok(updated) => last_prune_at = updated,
            Err(e) => tracing::error!("{}", e),
        }
    }
}

/// Run the audit prune when due (the pure `due_for_prune` gate). Returns the
/// new `last_prune_at` — `now` after a successful run, unchanged when not due.
/// Kept small and dependency-free so the wiring is testable.
pub async fn maybe_prune_audit(
    db: &DatabaseConnection,
    last_prune_at: Option<chrono::DateTime<Utc>>,
    now: chrono::DateTime<Utc>,
    retention_days: i64,
) -> Result<Option<chrono::DateTime<Utc>>, String> {
    if !due_for_prune(last_prune_at, now) {
        return Ok(last_prune_at);
    }
    let cutoff = retention_cutoff(now, retention_days);
    match AuditLogRepository::new(db).prune_older_than(cutoff).await {
        Ok(_) => Ok(Some(now)),
        Err(e) => Err(format!("audit prune failed: {}", e)),
    }
}

pub async fn check_auto_sleep(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    template_repo: &WorkspaceTemplateRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    audit: Option<&AuditSender>,
    now: chrono::DateTime<Utc>,
) -> Result<usize, String> {
    let instances = instance_repo
        .list_running_with_started_at()
        .await
        .map_err(|e| format!("DB query failed: {}", e))?;

    let mut triggered = 0;

    for instance in &instances {
        let template = match template_repo.find_by_id(instance.template_id).await {
            Ok(Some(template)) => template,
            Ok(None) => {
                tracing::warn!("Auto-sleep: template not found for instance '{}'", instance.name);
                continue;
            }
            Err(e) => {
                tracing::warn!("Auto-sleep: template lookup failed for instance '{}': {}", instance.name, e);
                continue;
            }
        };

        let Some(max_run_seconds) = template.max_run_seconds else {
            continue;
        };
        let Some(started_at) = instance.started_at else {
            continue;
        };

        let elapsed = (now - started_at).num_seconds();
        if elapsed < max_run_seconds {
            continue;
        }

        let result = match template.timeout_action.as_str() {
            "remove" => crate::timeout_action::remove(instance_repo, docker, vnc_cache, instance, "Auto-sleep").await,
            "stop" => crate::timeout_action::stop(instance_repo, docker, vnc_cache, instance, "Auto-sleep").await,
            "pause" => crate::timeout_action::pause(instance_repo, docker, instance, "Auto-sleep").await,
            other => {
                tracing::warn!(
                    "Auto-sleep: invalid timeout_action '{}' for template '{}'",
                    other,
                    template.name
                );
                continue;
            }
        };

        match result {
            Ok(()) => {
                triggered += 1;
                if let Some(audit) = audit {
                    audit.emit(
                        AuditEvent::system(action::INSTANCE_AUTO_SLEEP, target::INSTANCE)
                            .with_target(Some(instance.id.to_string()), Some(instance.name.clone()))
                            .with_detail(serde_json::json!({
                                "reason": "auto-sleep",
                                "action": template.timeout_action,
                            })),
                    );
                }
            }
            Err(e) => tracing::error!("Auto-sleep failed for instance '{}': {}", instance.name, e),
        }
    }

    Ok(triggered)
}

pub async fn check_keep_time(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    template_repo: &WorkspaceTemplateRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    audit: Option<&AuditSender>,
    now: chrono::DateTime<Utc>,
) -> Result<usize, String> {
    let instances = instance_repo
        .list_running_with_last_seen_at()
        .await
        .map_err(|e| format!("DB query failed: {}", e))?;

    let mut triggered = 0;

    for instance in &instances {
        let template = match template_repo.find_by_id(instance.template_id).await {
            Ok(Some(template)) => template,
            Ok(None) => {
                tracing::warn!("Keep-time: template not found for instance '{}'", instance.name);
                continue;
            }
            Err(e) => {
                tracing::warn!("Keep-time: template lookup failed for instance '{}': {}", instance.name, e);
                continue;
            }
        };

        let Some(keep_time_seconds) = template.keep_time_seconds else {
            continue;
        };
        let Some(last_seen_at) = instance.last_seen_at else {
            continue;
        };

        let elapsed = (now - last_seen_at).num_seconds();

        // A live client connection to the session means the user still has the
        // remote desktop / terminal open — reset the timer instead of
        // reclaiming. The browser-focus heartbeat is a secondary signal, so
        // this keeps in-use sessions alive even when focus detection fails
        // (e.g. interaction inside an embedded iframe). Connection checks are
        // fail-open: if we cannot inspect the container, we skip enforcement.
        let container_id = match instance.container_id.as_ref() {
            Some(id) => id,
            None => {
                tracing::warn!(
                    "Keep-time: instance '{}' has no container_id; skipping",
                    instance.name
                );
                continue;
            }
        };
        let port = match template.remote_type.parse::<RemoteType>() {
            Ok(remote_type) => remote_type.port(),
            Err(e) => {
                tracing::warn!(
                    "Keep-time: invalid remote_type '{}' for instance '{}': {}",
                    template.remote_type,
                    instance.name,
                    e
                );
                continue;
            }
        };
        match docker.has_session_connection(container_id, port).await {
            Ok(true) => {
                if let Err(e) = instance_repo.update_last_seen_at(instance.id, Some(now)).await {
                    tracing::error!(
                        "Keep-time: failed to refresh last_seen_at for instance '{}': {}",
                        instance.name,
                        e
                    );
                }
                tracing::info!(
                    "Keep-time: instance '{}' has an active session; resetting timer",
                    instance.name
                );
                continue;
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    "Keep-time: connection check failed for instance '{}': {} (skipping)",
                    instance.name,
                    e
                );
                continue;
            }
        }

        if elapsed < keep_time_seconds {
            continue;
        }

        let result = match template.keep_time_action.as_str() {
            "remove" => crate::timeout_action::remove(instance_repo, docker, vnc_cache, instance, "Keep-time").await,
            "stop" => crate::timeout_action::stop(instance_repo, docker, vnc_cache, instance, "Keep-time").await,
            "pause" => crate::timeout_action::pause(instance_repo, docker, instance, "Keep-time").await,
            other => {
                tracing::warn!(
                    "Keep-time: invalid keep_time_action '{}' for template '{}'",
                    other,
                    template.name
                );
                continue;
            }
        };

        match result {
            Ok(()) => {
                triggered += 1;
                if let Some(audit) = audit {
                    audit.emit(
                        AuditEvent::system(action::INSTANCE_AUTO_SLEEP, target::INSTANCE)
                            .with_target(Some(instance.id.to_string()), Some(instance.name.clone()))
                            .with_detail(serde_json::json!({
                                "reason": "keep-time",
                                "action": template.keep_time_action,
                            })),
                    );
                }
            }
            Err(e) => tracing::error!("Keep-time failed for instance '{}': {}", instance.name, e),
        }
    }

    Ok(triggered)
}

pub async fn check_instances(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    vnc_cache: &VncCache,
    client: &reqwest::Client,
    host_gateway_ip: &str,
) -> Result<usize, String> {
    let instances = instance_repo
        .list_by_status("starting")
        .await
        .map_err(|e| format!("DB query failed: {}", e))?;

    let mut updated = 0;

    for instance in &instances {
        let result = check_single_instance(instance_repo, vnc_cache, client, instance, host_gateway_ip).await;
        if let Err(e) = result {
            tracing::warn!("Health check for instance '{}': {}", instance.name, e);
        } else {
            updated += 1;
        }
    }

    Ok(updated)
}

async fn check_single_instance(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    vnc_cache: &VncCache,
    client: &reqwest::Client,
    instance: &crate::db::WorkspaceInstance,
    host_gateway_ip: &str,
) -> Result<(), String> {
    // Probe the exact path real traffic uses: the host gateway IP + the
    // instance's published host port. A container IP is never involved.
    let host_port = instance
        .host_port
        .ok_or_else(|| "no host_port".to_string())?;

    let url = format!("https://{host_gateway_ip}:{host_port}/");
    match client.get(&url).send().await {
        Ok(_) => {
            instance_repo
                .update_status(instance.id, "running")
                .await
                .map_err(|e| format!("status update failed: {}", e))?;
            instance_repo
                .update_started_at(instance.id, Some(Utc::now()))
                .await
                .map_err(|e| format!("started_at update failed: {}", e))?;
            instance_repo
                .update_last_seen_at(instance.id, Some(Utc::now()))
                .await
                .map_err(|e| format!("last_seen_at update failed: {}", e))?;
            vnc_cache.insert(&instance.access_token, "running");
            tracing::info!(
                "Health check passed for instance '{}' ({}:{})",
                instance.name,
                host_gateway_ip,
                host_port
            );
            Ok(())
        }
        Err(_) => {
            check_timeout(instance_repo, instance, "probe failed").await
        }
    }
}

async fn check_timeout(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    instance: &crate::db::WorkspaceInstance,
    reason: &str,
) -> Result<(), String> {
    let elapsed = (Utc::now() - instance.updated_at).num_seconds();
    if elapsed >= PROBE_TIMEOUT_SECS {
        instance_repo
            .update_status(instance.id, "error")
            .await
            .map_err(|e| format!("status update failed: {}", e))?;
        tracing::warn!(
            "Health check timeout for instance '{}' ({}s elapsed): {}",
            instance.name,
            elapsed,
            reason
        );
        Ok(())
    } else {
        Err(format!("{} ({}s elapsed)", reason, elapsed))
    }
}
