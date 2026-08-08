//! Audit-trail query endpoint (observability-logs spec Decision 8): a single
//! `GET /api/audit` returns keyset-paginated audit entries, newest first, with
//! AND-combined filters (action, actor substring, target substring, outcome,
//! created-after/before). Gated by the `can_view_audit_logs` group flag (admins
//! and Managers hold it by default). Read-only: this endpoint audits nothing.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::auth::AuthUser;
use crate::db::{AuditCursor, AuditLogRepository, AuditLogEntry};
use crate::openapi::AuditLogEnvelope;

/// The opaque next-page cursor: the last row's `(created_at, id)`, joined with
/// `|` (RFC3339 timestamps never contain `|`; the `+`/`-` offset is URL-encoded
/// by the client and decoded by serde_urlencoded).
fn encode_cursor(cursor: &AuditCursor) -> String {
    format!("{}|{}", cursor.created_at.to_rfc3339(), cursor.id)
}

fn decode_cursor(raw: &str) -> Option<AuditCursor> {
    let mut parts = raw.splitn(2, '|');
    let created_at = parts.next()?.parse::<chrono::DateTime<chrono::Utc>>().ok()?;
    let id = parts.next()?.parse::<Uuid>().ok()?;
    Some(AuditCursor { created_at, id })
}

#[derive(Deserialize)]
pub struct AuditQueryParams {
    /// Exact action vocabulary value, e.g. `user.create`.
    action: Option<String>,
    /// Actor-name substring (case-sensitive, SQL `LIKE` semantics).
    actor: Option<String>,
    /// Target-name substring.
    target: Option<String>,
    /// Exact outcome: `success` | `failure`.
    outcome: Option<String>,
    /// RFC3339: only entries created after this instant.
    after: Option<chrono::DateTime<chrono::Utc>>,
    /// RFC3339: only entries created before this instant.
    before: Option<chrono::DateTime<chrono::Utc>>,
    /// Opaque cursor returned by the previous page (`next_cursor`).
    cursor: Option<String>,
    /// Page size, clamped to [1, 200]; default 50.
    limit: Option<u64>,
}

fn entry_json(entry: &AuditLogEntry) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "created_at": entry.created_at,
        "actor_user_id": entry.actor_user_id,
        "actor_name": entry.actor_name,
        "action": entry.action,
        "target_type": entry.target_type,
        "target_id": entry.target_id,
        "target_name": entry.target_name,
        "outcome": entry.outcome,
        "client_ip": entry.client_ip,
        "detail": entry.detail,
    })
}

#[utoipa::path(
    get,
    path = "/api/audit",
    tag = "admin-gated",
    responses(
        (status = 200, description = "audit entries (newest first) with a next-page cursor when more rows remain", body = AuditLogEnvelope),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 403, description = "requires can_view_audit_logs or admin"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn query_audit(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_view_audit_logs() {
        return Err(StatusCode::FORBIDDEN);
    }

    let cursor = match params.cursor.as_deref() {
        None => None,
        Some(raw) => Some(decode_cursor(raw).ok_or(StatusCode::BAD_REQUEST)?),
    };
    let limit = params.limit.unwrap_or(50).clamp(1, 200);

    let filters = crate::db::AuditQueryFilters {
        action: params.action,
        actor_contains: params.actor,
        target_contains: params.target,
        outcome: params.outcome,
        created_after: params.after,
        created_before: params.before,
    };

    let (entries, next_cursor) = AuditLogRepository::new(&state.db)
        .query(cursor, &filters, limit)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let entries_json: Vec<serde_json::Value> = entries.iter().map(entry_json).collect();
    let next = next_cursor.map(|c| encode_cursor(&c));

    Ok(Json(serde_json::json!({
        "entries": entries_json,
        "next_cursor": next,
    })))
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/audit", get(query_audit))
}
