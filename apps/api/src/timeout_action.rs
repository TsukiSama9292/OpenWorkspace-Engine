use crate::db::{PersistentVolumeRepository, WorkspaceInstanceRepository};
use crate::docker::DockerService;
use crate::vnc_cache::VncCache;

pub async fn remove(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    instance: &crate::db::WorkspaceInstance,
    label: &str,
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
            // Same lifecycle rule as `delete_instance` (spec §7): once no other
            // active instance still references the host path, flip the volume
            // registry row to `orphaned` so it shows up in the cleanup view.
            // Best-effort: a sync failure is logged, never a removal error.
            if let Some(host_path) = instance.resolved_volume_host_path.as_deref()
                && let Err(e) = PersistentVolumeRepository::new(instance_repo.db)
                    .sync_status_for_host_path(host_path)
                    .await
                {
                    tracing::warn!(
                        "Failed to sync persistent-volume registry for '{}': {}",
                        host_path,
                        e
                    );
                }
            tracing::info!("{} removed instance '{}'", label, instance.name);
            Ok(())
        }
        Ok(false) => Err("instance row already gone".to_string()),
        Err(e) => Err(format!("delete failed: {}", e)),
    }
}

pub async fn stop(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    instance: &crate::db::WorkspaceInstance,
    label: &str,
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

    // The Traefik route is deliberately kept (same as the stop route): the host
    // port stays reserved, so stop/start causes no route churn.
    vnc_cache.remove(&instance.access_token);

    instance_repo
        .update_status(instance.id, "stopped")
        .await
        .map_err(|e| format!("status update failed: {}", e))?;
    instance_repo.update_started_at(instance.id, None).await.ok();
    instance_repo.update_last_seen_at(instance.id, None).await.ok();

    tracing::info!("{} stopped instance '{}'", label, instance.name);
    Ok(())
}

pub async fn pause(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    instance: &crate::db::WorkspaceInstance,
    label: &str,
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
    instance_repo.update_last_seen_at(instance.id, None).await.ok();

    tracing::info!("{} paused instance '{}'", label, instance.name);
    Ok(())
}
