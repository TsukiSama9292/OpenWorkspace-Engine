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
    }
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
