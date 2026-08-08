use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use bcrypt::verify;
use serde::Deserialize;

use super::super::AppState;
use crate::audit::{action, client_ip, target, AuditEvent};
use crate::auth::{AuthUser, create_token, set_cookie};
use crate::db::{PolicyRepository, UserRepository};
use crate::openapi::ContextEnvelope;

#[derive(Deserialize, utoipa::ToSchema)]
pub(crate) struct LoginRequest {
    username: String,
    password: String,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/auth/login", post(login))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "authenticated; sets the ow_token cookie", body = ContextEnvelope),
        (status = 401, description = "invalid credentials"),
        (status = 400, description = "request body is not valid JSON (syntax error)"),
        (status = 415, description = "missing Content-Type: application/json"),
        (status = 422, description = "malformed JSON body"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn login(
    State(state): State<AppState>,
    req_headers: axum::http::HeaderMap,
    Json(input): Json<LoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let repo = UserRepository::new(&state.db);
    let client_ip = client_ip(&req_headers);

    let user = match repo.find_by_username(&input.username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            // Unknown username: still audited as a failed sign-in, snapshotted
            // from the submitted username (spec Decision 3 — never NULL/empty).
            state.audit.emit(
                AuditEvent::anonymous(Some(&input.username), action::LOGIN_FAILURE, target::USER, client_ip),
            );
            return Err(StatusCode::UNAUTHORIZED);
        }
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    let valid = verify(&input.password, &user.password_hash).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid {
        // Failed login: no authenticated user, so the actor is the submitted
        // username and the outcome is failure (audit spec Decision 3).
        state.audit.emit(
            AuditEvent::anonymous(Some(&input.username), action::LOGIN_FAILURE, target::USER, client_ip)
                .with_target(Some(user.id.to_string()), Some(user.username.clone())),
        );
        return Err(StatusCode::UNAUTHORIZED);
    }

    let context = PolicyRepository::new(&state.db)
        .load_effective_context(user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let auth = AuthUser {
        user_id: user.id,
        username: user.username.clone(),
        context: context.clone(),
        client_ip: client_ip.clone(),
    };
    state.audit.emit(
        AuditEvent::from_auth(&auth, action::LOGIN, target::USER)
            .with_target(Some(user.id.to_string()), Some(user.username.clone())),
    );

    let token = create_token(&user.id, &state.settings.jwt_secret)?;

    let mut headers = axum::http::HeaderMap::new();
    set_cookie(&mut headers, &token);

    Ok((
        headers,
        Json(serde_json::json!({ "context": context })),
    ))
}
