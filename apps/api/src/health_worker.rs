use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use sea_orm::DatabaseConnection;
use tokio::time::MissedTickBehavior;

use crate::db::{WorkspaceInstanceRepository, WorkspaceTemplateRepository};
use crate::docker::{DockerService, RemoteType};
use crate::vnc_cache::VncCache;

const PROBE_TIMEOUT_SECS: i64 = 120;

pub async fn run(
    db: DatabaseConnection,
    docker: Arc<dyn DockerService>,
    vnc_cache: VncCache,
) {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to build reqwest client for health worker");

    let mut interval = tokio::time::interval(Duration::from_secs(3));
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        let instance_repo = WorkspaceInstanceRepository::new(&db);
        let template_repo = WorkspaceTemplateRepository::new(&db);

        match check_instances(&instance_repo, &template_repo, &*docker, &vnc_cache, &client).await {
            Ok(count) => {
                if count > 0 {
                    tracing::debug!("Health worker updated {} instances", count);
                }
            }
            Err(e) => tracing::error!("Health worker error: {}", e),
        }

        match check_auto_sleep(&instance_repo, &template_repo, &*docker, &vnc_cache, Utc::now()).await {
            Ok(count) => {
                if count > 0 {
                    tracing::debug!("Auto-sleep worker acted on {} instances", count);
                }
            }
            Err(e) => tracing::error!("Auto-sleep worker error: {}", e),
        }
    }
}

pub async fn check_auto_sleep(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    template_repo: &WorkspaceTemplateRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
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
            "remove" => auto_sleep_remove(instance_repo, docker, vnc_cache, instance).await,
            "stop" => auto_sleep_stop(instance_repo, docker, vnc_cache, instance).await,
            "pause" => auto_sleep_pause(instance_repo, docker, instance).await,
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
            Ok(()) => triggered += 1,
            Err(e) => tracing::error!("Auto-sleep failed for instance '{}': {}", instance.name, e),
        }
    }

    Ok(triggered)
}

async fn auto_sleep_remove(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    instance: &crate::db::WorkspaceInstance,
) -> Result<(), String> {
    if let Err(e) = crate::route_writer::delete_route(&instance.access_token) {
        tracing::error!("Failed to delete Traefik VNC route: {}", e);
    }
    vnc_cache.remove(&instance.access_token);

    if let Some(ref cid) = instance.container_id {
        crate::docker::stop_and_remove_container(docker, cid, &instance.name).await;
    }

    match instance_repo.delete(instance.id).await {
        Ok(true) => {
            tracing::info!("Auto-sleep removed instance '{}'", instance.name);
            Ok(())
        }
        Ok(false) => Err("instance row already gone".to_string()),
        Err(e) => Err(format!("delete failed: {}", e)),
    }
}

async fn auto_sleep_stop(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    instance: &crate::db::WorkspaceInstance,
) -> Result<(), String> {
    if let Some(ref cid) = instance.container_id {
        match docker.stop_container_by_id(cid).await {
            Ok(()) => {
                tracing::info!("Container for '{}' stopped (id: {})", instance.name, &cid[..12]);
            }
            Err(e) => {
                tracing::warn!("Failed to stop container for '{}': {} (updating DB anyway)", instance.name, e);
            }
        }
    }

    if let Err(e) = crate::route_writer::delete_route(&instance.access_token) {
        tracing::error!("Failed to delete Traefik VNC route: {}", e);
    }
    vnc_cache.remove(&instance.access_token);

    instance_repo
        .update_status(instance.id, "stopped")
        .await
        .map_err(|e| format!("status update failed: {}", e))?;
    instance_repo.update_started_at(instance.id, None).await.ok();

    tracing::info!("Auto-sleep stopped instance '{}'", instance.name);
    Ok(())
}

async fn auto_sleep_pause(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    instance: &crate::db::WorkspaceInstance,
) -> Result<(), String> {
    if let Some(ref cid) = instance.container_id {
        match docker.pause_container_by_id(cid).await {
            Ok(()) => {
                tracing::info!("Container for '{}' paused (id: {})", instance.name, &cid[..12]);
            }
            Err(e) => {
                tracing::warn!("Failed to pause container for '{}': {} (updating DB anyway)", instance.name, e);
            }
        }
    }

    instance_repo
        .update_status(instance.id, "paused")
        .await
        .map_err(|e| format!("status update failed: {}", e))?;
    instance_repo.update_started_at(instance.id, None).await.ok();

    tracing::info!("Auto-sleep paused instance '{}'", instance.name);
    Ok(())
}

pub async fn check_instances(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    template_repo: &WorkspaceTemplateRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    client: &reqwest::Client,
) -> Result<usize, String> {
    let instances = instance_repo
        .list_by_status("starting")
        .await
        .map_err(|e| format!("DB query failed: {}", e))?;

    let mut updated = 0;

    for instance in &instances {
        let result = check_single_instance(instance_repo, template_repo, docker, vnc_cache, client, instance).await;
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
    template_repo: &WorkspaceTemplateRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    client: &reqwest::Client,
    instance: &crate::db::WorkspaceInstance,
) -> Result<(), String> {
    let container_id = instance
        .container_id
        .as_ref()
        .ok_or_else(|| "no container_id".to_string())?;

    let template = template_repo
        .find_by_id(instance.template_id)
        .await
        .map_err(|e| format!("template lookup failed: {}", e))?
        .ok_or_else(|| "template not found".to_string())?;

    let remote_type: RemoteType = template
        .remote_type
        .parse()
        .map_err(|_| "invalid remote_type".to_string())?;

    let port = remote_type.port();

    let ip = match docker.get_container_ip(container_id, docker.network_name()).await {
        Ok(ip) => ip,
        Err(e) => {
            return check_timeout(instance_repo, instance, &format!("get_container_ip failed: {}", e)).await;
        }
    };

    let url = format!("https://{ip}:{port}/");
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
            vnc_cache.insert(&instance.access_token, "running");
            tracing::info!(
                "Health check passed for instance '{}' ({}:{})",
                instance.name,
                ip,
                port
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
