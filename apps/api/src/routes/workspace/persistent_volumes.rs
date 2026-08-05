use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use super::super::AppState;
use crate::auth::AuthUser;
use crate::db::{
    PersistentVolume, PersistentVolumeRepository, UserRepository, VOLUME_STATUS_ORPHANED,
};
use crate::persistent_volume::persistent_volume_name;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/persistent-volumes", get(list_persistent_volumes))
        .route(
            "/api/persistent-volumes/{id}/cleanup",
            post(cleanup_persistent_volume),
        )
}

/// The orphaned-volumes view (spec Decision 7 + user story 16): available only
/// to system admins and `can_manage_users` holders, never scoped by group.
/// Only `orphaned` rows are returned; a still-referenced (`active`) volume is
/// not an orphan and stays hidden here.
async fn list_persistent_volumes(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_users() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = PersistentVolumeRepository::new(&state.db);
    let volumes = repo
        .list_orphaned()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let user_repo = UserRepository::new(&state.db);
    let mut volumes_json = Vec::with_capacity(volumes.len());
    for volume in volumes {
        let owner_username = match volume.owner_id {
            Some(owner_id) => user_repo
                .find_by_id(owner_id)
                .await
                .ok()
                .flatten()
                .map(|user| user.username),
            None => None,
        };
        volumes_json.push(volume_to_json(&volume, owner_username.as_deref()));
    }

    Ok(Json(serde_json::json!({ "volumes": volumes_json })))
}

/// The double-confirmed "thorough cleanup": the frontend confirms twice, and
/// the endpoint then empties the host directory, removes the Docker Volume
/// declaration, and finally deletes the registry row. Only `orphaned` volumes
/// are cleanable — an `active` volume is still referenced by a live instance
/// and must never be destroyed.
async fn cleanup_persistent_volume(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    if !auth.can_manage_users() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = PersistentVolumeRepository::new(&state.db);
    let volume = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if volume.status != VOLUME_STATUS_ORPHANED {
        return Err(StatusCode::CONFLICT);
    }

    let volume_name = persistent_volume_name(&volume.host_path);
    if let Err(e) = state
        .docker
        .remove_persistent_volume(&volume.host_path, &volume_name)
        .await
    {
        tracing::error!(
            "Failed to cleanup persistent volume '{}' (id={}): {}",
            volume.host_path,
            id,
            e
        );
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let deleted = repo
        .delete(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

fn volume_to_json(volume: &PersistentVolume, owner_username: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": volume.id,
        "host_path": volume.host_path,
        "owner_id": volume.owner_id,
        "owner_username": owner_username,
        "status": volume.status,
        "created_at": volume.created_at,
    })
}
