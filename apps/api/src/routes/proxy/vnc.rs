use axum::{
    extract::State,
    http::{header, StatusCode},
    routing::get,
    Router,
};
use uuid::Uuid;

use super::super::AppState;
use crate::auth::Claims;
use crate::db::WorkspaceInstanceRepository;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/vnc/verify", get(vnc_verify))
}

async fn vnc_verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::http::HeaderMap, StatusCode> {
    let secret = &state.settings.jwt_secret;

    let cookie = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = cookie
        .split(';')
        .find(|c| c.trim().starts_with("ow_token="))
        .and_then(|c| c.trim().strip_prefix("ow_token="))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_data = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let user_id: Uuid = token_data
        .claims
        .sub
        .parse()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let forwarded_uri = headers
        .get("X-Forwarded-Uri")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let vnc_token = forwarded_uri
        .strip_prefix("/kasmvnc/")
        .and_then(|rest| rest.strip_suffix("/websockify"))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    match state.vnc_cache.get(vnc_token) {
        Some(entry) => {
            if entry.status != "running" {
                return Err(StatusCode::NOT_FOUND);
            }
        }
        None => {
            let instance_repo = WorkspaceInstanceRepository::new(&state.db);
            let instance = instance_repo
                .find_by_access_token(vnc_token)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;

            if instance.status != "running" {
                return Err(StatusCode::NOT_FOUND);
            }

            state.vnc_cache.insert(vnc_token, &instance.status);
        }
    }

    let mut resp_headers = axum::http::HeaderMap::new();
    resp_headers.insert("X-Forwarded-User", user_id.to_string().parse().unwrap());

    Ok(resp_headers)
}
