//! Audit trail: event model, pure helpers (redaction, IP extraction, prune
//! scheduling), and the non-blocking write channel (spec Decision 6).
//!
//! Handlers enqueue `AuditEvent`s through a bounded, cloneable `AuditSender`
//! (never blocking the triggering operation); a single writer task drains the
//! channel and batches inserts into the repository. Channel-full and DB-failure
//! degrade to `tracing` warnings/errors — an audit outage never breaks the main
//! flow (User Stories 12/13).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const AUDIT_CHANNEL_CAPACITY: usize = 1024;
pub const AUDIT_BATCH_SIZE: usize = 50;
pub const AUDIT_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
/// Bounded retries per flush (then drop) before an insert failure is accepted.
pub const AUDIT_WRITER_MAX_RETRIES: u32 = 4;

/// Stable action vocabulary (spec Decision 3). Values are part of the API
/// contract and are documented in the regenerated OpenAPI spec.
pub mod action {
    pub const LOGIN: &str = "auth.login";
    pub const LOGOUT: &str = "auth.logout";
    pub const LOGIN_FAILURE: &str = "auth.login_failure";
    pub const FORBIDDEN: &str = "auth.forbidden";
    pub const INSTANCE_CREATE: &str = "instance.create";
    pub const INSTANCE_START: &str = "instance.start";
    pub const INSTANCE_STOP: &str = "instance.stop";
    pub const INSTANCE_DELETE: &str = "instance.delete";
    pub const INSTANCE_RESTART: &str = "instance.restart";
    pub const INSTANCE_PAUSE: &str = "instance.pause";
    pub const INSTANCE_UNPAUSE: &str = "instance.unpause";
    pub const INSTANCE_AUTO_SLEEP: &str = "instance.auto_sleep";
    pub const TEMPLATE_CREATE: &str = "template.create";
    pub const TEMPLATE_UPDATE: &str = "template.update";
    pub const TEMPLATE_DELETE: &str = "template.delete";
    pub const GROUP_CREATE: &str = "group.create";
    pub const GROUP_UPDATE: &str = "group.update";
    pub const GROUP_DELETE: &str = "group.delete";
    pub const GROUP_MEMBERSHIP_CHANGE: &str = "group.membership_change";
    pub const USER_CREATE: &str = "user.create";
    pub const USER_UPDATE: &str = "user.update";
    pub const USER_DELETE: &str = "user.delete";
    pub const USER_PASSWORD_CHANGE: &str = "user.password_change";
    pub const SETTINGS_UPDATE: &str = "settings.update";
    pub const REGISTRY_UPDATE: &str = "registry.update";
}

/// Stable target types (spec Decision 3).
pub mod target {
    pub const INSTANCE: &str = "instance";
    pub const TEMPLATE: &str = "template";
    pub const GROUP: &str = "group";
    pub const USER: &str = "user";
    pub const REGISTRY: &str = "registry";
    pub const SETTINGS: &str = "settings";
    pub const NONE: &str = "none";
}

/// Stable outcomes. `failure` marks a denied/unsuccessful action (e.g. a
/// failed login or a forbidden attempt); everything else is `success`.
pub mod outcome {
    pub const SUCCESS: &str = "success";
    pub const FAILURE: &str = "failure";
}

/// `actor_name` for system-triggered events (auto-sleep, …).
pub const SYSTEM_ACTOR: &str = "system";

/// A single audit event, enqueued by a handler and persisted by the writer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// NULL when no authenticated user exists (failed logins); the row-level
    /// `actor_name` snapshot still identifies the attempt.
    pub actor_user_id: Option<Uuid>,
    pub actor_name: String,
    pub action: String,
    pub target_type: String,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub outcome: String,
    pub client_ip: Option<String>,
    /// Changed-field before/after diff for edit events (already redacted).
    pub detail: Option<serde_json::Value>,
}

impl AuditEvent {
    /// Build a successful event for an authenticated actor. The client IP and
    /// actor snapshot come from the request-level `AuthUser`, so handlers never
    /// read headers themselves.
    pub fn from_auth(auth: &crate::auth::AuthUser, action: &str, target_type: &str) -> Self {
        Self {
            created_at: chrono::Utc::now(),
            actor_user_id: Some(auth.user_id),
            actor_name: auth.username.clone(),
            action: action.to_string(),
            target_type: target_type.to_string(),
            target_id: None,
            target_name: None,
            outcome: outcome::SUCCESS.to_string(),
            client_ip: auth.client_ip.clone(),
            detail: None,
        }
    }

    /// Build an event for an unattended/system actor (auto-sleep, …).
    pub fn system(action: &str, target_type: &str) -> Self {
        Self {
            created_at: chrono::Utc::now(),
            actor_user_id: None,
            actor_name: SYSTEM_ACTOR.to_string(),
            action: action.to_string(),
            target_type: target_type.to_string(),
            target_id: None,
            target_name: None,
            outcome: outcome::SUCCESS.to_string(),
            client_ip: None,
            detail: None,
        }
    }

    /// Build a login-failure event. There is no authenticated user, so the
    /// actor is the *submitted* username (never NULL, never empty — the NOT
    /// NULL constraint on `actor_name` always holds).
    pub fn anonymous(
        submitted_name: Option<&str>,
        action: &str,
        target_type: &str,
        client_ip: Option<String>,
    ) -> Self {
        let name = submitted_name.filter(|s| !s.trim().is_empty()).unwrap_or("anonymous");
        Self {
            created_at: chrono::Utc::now(),
            actor_user_id: None,
            actor_name: name.to_string(),
            action: action.to_string(),
            target_type: target_type.to_string(),
            target_id: None,
            target_name: None,
            outcome: outcome::FAILURE.to_string(),
            client_ip,
            detail: None,
        }
    }

    /// Build an `auth.forbidden` event for an authenticated actor whose request
    /// was rejected with 403 (spec Decision 3). Emitted by the thin response
    /// middleware only — anonymous 401/403 scanner noise is never audited.
    pub fn forbidden(user_id: Uuid, username: String, client_ip: Option<String>) -> Self {
        Self {
            created_at: chrono::Utc::now(),
            actor_user_id: Some(user_id),
            actor_name: username,
            action: action::FORBIDDEN.to_string(),
            target_type: target::NONE.to_string(),
            target_id: None,
            target_name: None,
            outcome: outcome::FAILURE.to_string(),
            client_ip,
            detail: None,
        }
    }

    pub fn with_target(mut self, id: Option<String>, name: Option<String>) -> Self {
        self.target_id = id;
        self.target_name = name;
        self
    }

    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = Some(detail);
        self
    }

    pub fn with_outcome(mut self, outcome: &str) -> Self {
        self.outcome = outcome.to_string();
        self
    }
}

/// Field-name keywords whose values are replaced with `[REDACTED]` in any diff
/// (spec Decision 4). Matched case-insensitively, substring.
const REDACT_KEYWORDS: [&str; 5] = ["password", "secret", "token", "key", "credential"];

fn is_sensitive_field(field: &str) -> bool {
    let lower = field.to_lowercase();
    REDACT_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Pure redaction: walks a JSON diff object and replaces any value whose field
/// name matches a sensitive keyword (`password` | `secret` | `token` | `key` |
/// `credential`, case-insensitive substring) with `"[REDACTED]"`. Non-object
/// values pass through untouched.
pub fn redact_detail(detail: serde_json::Value) -> serde_json::Value {
    match detail {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, value) in map {
                if is_sensitive_field(&key) {
                    out.insert(key, serde_json::Value::String("[REDACTED]".to_string()));
                } else {
                    out.insert(key, value);
                }
            }
            serde_json::Value::Object(out)
        }
        other => other,
    }
}

/// Strip any `user:pass@` userinfo from a URL so embedded credentials never
/// reach the audit diff. A field named `registry_url` is not caught by the
/// keyword redaction above, so the value is scrubbed explicitly at the call
/// site. Pure and side-effect free.
pub fn redact_url_userinfo(url: &str) -> String {
    match url.find("://") {
        Some(scheme_end) => {
            let after_scheme = &url[scheme_end + 3..];
            match after_scheme.find('@') {
                Some(at) if after_scheme[..at].contains(':') => {
                    format!("{}{}", &url[..scheme_end + 3], &after_scheme[at + 1..])
                }
                _ => url.to_string(),
            }
        }
        None => url.to_string(),
    }
}

/// Build a redacted `field: { before, after }` diff from the changed fields.
/// Redaction is built into the helper, so no caller can forget it (spec
/// Decision 4).
pub fn diff_detail(changes: &[(String, serde_json::Value, serde_json::Value)]) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (field, before, after) in changes {
        map.insert(
            field.clone(),
            serde_json::json!({ "before": before, "after": after }),
        );
    }
    redact_detail(serde_json::Value::Object(map))
}

/// Pure client-IP extraction (spec Decision 5): the **rightmost** non-empty
/// `X-Forwarded-For` entry (the last hop Traefik appended), falling back to
/// `X-Real-IP`, then `None` with a single warning.
pub fn client_ip(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        // Rightmost non-empty entry: the last hop Traefik appended. Empty
        // entries (trailing separators) are skipped.
        for entry in value.rsplit(',') {
            let trimmed = entry.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(value) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
    {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    tracing::debug!("No client IP headers (X-Forwarded-For / X-Real-IP) present");
    None
}

/// Pure prune gate (spec Decision 7): prune once per day, counting from the
/// last run. `None` (never pruned) is always due.
pub fn due_for_prune(
    last_prune_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    match last_prune_at {
        None => true,
        Some(last) => (now - last) >= chrono::Duration::hours(24),
    }
}

/// The retention cutoff: any row with `created_at` before this timestamp is
/// pruned. A zero/negative retention keeps the cutoff in the past so nothing is
/// deleted.
pub fn retention_cutoff(
    now: chrono::DateTime<chrono::Utc>,
    retention_days: i64,
) -> chrono::DateTime<chrono::Utc> {
    now - chrono::Duration::days(retention_days.max(0))
}

/// Bounded, cloneable sender used by handlers to enqueue events without
/// blocking. `try_send` drops the event (with a warning) when the channel is
/// full — audit is explicitly best-effort under extreme load.
#[derive(Clone)]
pub struct AuditSender {
    tx: tokio::sync::mpsc::Sender<AuditEvent>,
}

impl AuditSender {
    pub fn new(tx: tokio::sync::mpsc::Sender<AuditEvent>) -> Self {
        Self { tx }
    }

    pub fn try_enqueue(&self, event: AuditEvent) -> bool {
        match self.tx.try_send(event) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!("Audit channel full — dropping audit event: {}", err);
                false
            }
        }
    }

    /// Semantic alias for handler call sites: emit an event, best-effort.
    pub fn emit(&self, event: AuditEvent) -> bool {
        self.try_enqueue(event)
    }

    pub fn sender(&self) -> &tokio::sync::mpsc::Sender<AuditEvent> {
        &self.tx
    }
}

/// Normalize a timestamp to microsecond precision (Postgres `TIMESTAMPTZ`
/// truncates there), so the monotonic stamping below compares like-for-like.
fn micros_truncated(dt: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(dt.timestamp(), dt.timestamp_subsec_micros() * 1_000)
        .expect("created_at is always a valid timestamp")
}

/// Enforce a strictly-monotonic, microsecond-aligned `created_at` on each event
/// the writer appends, so the keyset order `(created_at DESC, id DESC)` is
/// deterministic. Events constructed in the same microsecond (a burst of
/// `Utc::now()` calls) would otherwise tie and fall through to the random-UUID
/// `id` tiebreaker, scrambling insertion order. `last_stamp` carries the
/// previous event's stamp across calls; `None` seeds from the first event.
/// Ordering is preserved because the writer is a single consumer appending
/// sequentially.
pub fn stamp_monotonic(
    event: &mut AuditEvent,
    last_stamp: &mut Option<chrono::DateTime<chrono::Utc>>,
) {
    let ts = micros_truncated(event.created_at);
    let stamped = match last_stamp {
        None => ts,
        Some(last) => {
            if ts <= *last {
                *last + chrono::Duration::microseconds(1)
            } else {
                ts
            }
        }
    };
    event.created_at = stamped;
    *last_stamp = Some(stamped);
}

/// Drain the audit channel and batch-insert into the repository: flush on 50
/// events or every 500 ms, whichever comes first (spec Decision 6). The writer
/// runs until `shutdown` fires (the app's graceful-shutdown hook) or the
/// channel closes; the remaining batch is flushed before the writer returns.
/// Ordering is preserved because MPSC is single-consumer and each event is
/// stamped monotonic on entry.
pub async fn audit_writer(
    mut rx: tokio::sync::mpsc::Receiver<AuditEvent>,
    db: sea_orm::DatabaseConnection,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let repo = crate::db::AuditLogRepository::new(&db);
    let mut batch: Vec<AuditEvent> = Vec::with_capacity(AUDIT_BATCH_SIZE);
    let mut flush_tick = tokio::time::interval(AUDIT_FLUSH_INTERVAL);
    flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_stamp: Option<chrono::DateTime<chrono::Utc>> = None;

    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Some(mut event) => {
                    stamp_monotonic(&mut event, &mut last_stamp);
                    batch.push(event);
                    if batch.len() >= AUDIT_BATCH_SIZE {
                        flush_batch(&repo, &mut batch).await;
                    }
                }
                None => {
                    flush_batch(&repo, &mut batch).await;
                    break;
                }
            },
            changed = shutdown.changed() => {
                if changed.is_ok() && *shutdown.borrow() {
                    // Graceful shutdown: the app is stopping. Drain whatever
                    // remains in the channel (the sender clones may still be
                    // alive, e.g. the health worker's), then flush and exit.
                    while let Ok(mut event) = rx.try_recv() {
                        stamp_monotonic(&mut event, &mut last_stamp);
                        batch.push(event);
                    }
                    flush_batch(&repo, &mut batch).await;
                    break;
                }
            },
            _ = flush_tick.tick() => {
                if !batch.is_empty() {
                    flush_batch(&repo, &mut batch).await;
                }
            }
        }
    }
}

async fn flush_batch(repo: &crate::db::AuditLogRepository<'_>, batch: &mut Vec<AuditEvent>) {
    if batch.is_empty() {
        return;
    }
    // Bounded retry with backoff: a transient DB hiccup (pool exhausted while
    // the whole test suite pounds one Postgres, a restarting peer) must not
    // silently drop the batch. Still best-effort — after the retries are spent
    // the events are dropped with an error, never re-queued (spec Decision 6).
    let mut attempts = 0;
    loop {
        match repo.insert_batch(batch).await {
            Ok(()) => break,
            Err(e) => {
                attempts += 1;
                if attempts >= AUDIT_WRITER_MAX_RETRIES {
                    tracing::error!(
                        "Audit writer: DB insert failed after {attempts} attempts \
                         ({} events lost): {e}",
                        batch.len()
                    );
                    break;
                }
                let delay = std::time::Duration::from_millis(100 * 2u64.pow(attempts - 1));
                tokio::time::sleep(delay).await;
            }
        }
    }
    batch.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use serde_json::json;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(*k, v.parse().unwrap());
        }
        map
    }

    // ── redact_detail ─────────────────────────────────────────

    #[test]
    fn redacts_password_secret_token_key_credential_fields() {
        let detail = serde_json::json!({
            "name": "my template",
            "password": "hunter2",
            "docker_registry_password": "secret1",
            "access_token": "abc123",
            "secret_key": "xyz",
            "credential": "kubeconfig",
        });
        let redacted = redact_detail(detail);
        assert_eq!(redacted["name"], "my template");
        assert_eq!(redacted["password"], "[REDACTED]");
        assert_eq!(redacted["docker_registry_password"], "[REDACTED]");
        assert_eq!(redacted["access_token"], "[REDACTED]");
        assert_eq!(redacted["secret_key"], "[REDACTED]");
        assert_eq!(redacted["credential"], "[REDACTED]");
    }

    #[test]
    fn redaction_is_case_insensitive() {
        let detail = serde_json::json!({ "Password": "x", "SECRET": "y", "ToKeN": "z" });
        let redacted = redact_detail(detail);
        assert_eq!(redacted["Password"], "[REDACTED]");
        assert_eq!(redacted["SECRET"], "[REDACTED]");
        assert_eq!(redacted["ToKeN"], "[REDACTED]");
    }

    #[test]
    fn plain_fields_survive_and_nested_shapes_untouched() {
        let detail = serde_json::json!({
            "cores": 4,
            "memory": 8192,
            "description": "plain text",
            "nested": { "password": "inner-secret" },
        });
        let redacted = redact_detail(detail);
        assert_eq!(redacted["cores"], 4);
        assert_eq!(redacted["memory"], 8192);
        assert_eq!(redacted["description"], "plain text");
        // The rule applies at the top level of the diff object only; nested
        // objects (e.g. a run_config blob) pass through as values.
        assert_eq!(redacted["nested"]["password"], "inner-secret");
    }

    #[test]
    fn redact_passes_non_object_through() {
        assert_eq!(redact_detail(serde_json::json!(42)), serde_json::json!(42));
        assert_eq!(
            redact_detail(serde_json::json!("plain")),
            serde_json::json!("plain")
        );
    }

    #[test]
    fn diff_detail_redacts_sensitive_changes() {
        let diff = diff_detail(&[
            ("name".to_string(), json!("old"), json!("new")),
            ("password".to_string(), json!("hunter2"), json!("hunter3")),
            ("max_run_seconds".to_string(), json!(3600), json!(7200)),
        ]);
        assert_eq!(diff["name"]["before"], "old");
        assert_eq!(diff["name"]["after"], "new");
        assert_eq!(diff["password"], "[REDACTED]");
        assert_eq!(diff["max_run_seconds"]["after"], 7200);
    }

    #[test]
    fn empty_diff_is_empty_object() {
        assert_eq!(diff_detail(&[]), serde_json::json!({}));
    }

    // ── client_ip ──────────────────────────────────────────────

    #[test]
    fn client_ip_takes_rightmost_x_forwarded_for() {
        let h = headers(&[("x-forwarded-for", "10.0.0.2, 10.0.0.3")]);
        assert_eq!(client_ip(&h).as_deref(), Some("10.0.0.3"));
    }

    #[test]
    fn client_ip_single_x_forwarded_for_entry() {
        let h = headers(&[("x-forwarded-for", "203.0.113.7")]);
        assert_eq!(client_ip(&h).as_deref(), Some("203.0.113.7"));
    }

    #[test]
    fn client_ip_ignores_empty_trailing_entries() {
        let h = headers(&[("x-forwarded-for", "10.0.0.2, , ")]);
        assert_eq!(client_ip(&h).as_deref(), Some("10.0.0.2"));
    }

    #[test]
    fn client_ip_falls_back_to_x_real_ip() {
        let h = headers(&[("x-real-ip", "198.51.100.9")]);
        assert_eq!(client_ip(&h).as_deref(), Some("198.51.100.9"));
    }

    #[test]
    fn client_ip_prefers_x_forwarded_for_over_x_real_ip() {
        let h = headers(&[
            ("x-forwarded-for", "10.0.0.9"),
            ("x-real-ip", "198.51.100.9"),
        ]);
        assert_eq!(client_ip(&h).as_deref(), Some("10.0.0.9"));
    }

    #[test]
    fn client_ip_none_when_headers_absent() {
        let h = headers(&[]);
        assert_eq!(client_ip(&h), None);
    }

    // ── due_for_prune / retention_cutoff ───────────────────────

    #[test]
    fn prune_due_when_never_run() {
        assert!(due_for_prune(None, chrono::Utc::now()));
    }

    #[test]
    fn prune_due_after_24h() {
        let now = chrono::Utc::now();
        let last = now - chrono::Duration::hours(24);
        assert!(due_for_prune(Some(last), now));
        let last = now - chrono::Duration::hours(24) - chrono::Duration::minutes(1);
        assert!(due_for_prune(Some(last), now));
    }

    #[test]
    fn prune_not_due_within_24h() {
        let now = chrono::Utc::now();
        let last = now - chrono::Duration::hours(23);
        assert!(!due_for_prune(Some(last), now));
        let last = now - chrono::Duration::seconds(59);
        assert!(!due_for_prune(Some(last), now));
    }

    #[test]
    fn retention_cutoff_is_now_minus_days() {
        let now = chrono::Utc::now();
        let cutoff = retention_cutoff(now, 90);
        assert_eq!((now - cutoff).num_days(), 90);
    }

    #[test]
    fn retention_cutoff_zero_or_negative_keeps_past() {
        let now = chrono::Utc::now();
        let cutoff = retention_cutoff(now, 0);
        assert_eq!(cutoff, now);
        let cutoff_neg = retention_cutoff(now, -5);
        assert_eq!(cutoff_neg, now);
    }

    // ── stamp_monotonic ────────────────────────────────────────

    fn event_with_created_at(ts: chrono::DateTime<chrono::Utc>) -> AuditEvent {
        AuditEvent {
            created_at: ts,
            actor_user_id: None,
            actor_name: SYSTEM_ACTOR.to_string(),
            action: action::INSTANCE_AUTO_SLEEP.to_string(),
            target_type: target::INSTANCE.to_string(),
            target_id: None,
            target_name: None,
            outcome: outcome::SUCCESS.to_string(),
            client_ip: None,
            detail: None,
        }
    }

    #[test]
    fn stamp_monotonic_preserves_distinct_timestamps() {
        let base = micros_truncated(chrono::Utc::now());
        let mut last = None;
        for i in 0..3 {
            let mut e = event_with_created_at(base + chrono::Duration::microseconds(i));
            stamp_monotonic(&mut e, &mut last);
            assert_eq!(e.created_at, base + chrono::Duration::microseconds(i));
        }
    }

    #[test]
    fn stamp_monotonic_bumps_microsecond_ties_forward_in_order() {
        let base = micros_truncated(chrono::Utc::now());
        let mut last = None;
        let mut prev = None;
        for i in 0..6 {
            let mut e = event_with_created_at(base);
            stamp_monotonic(&mut e, &mut last);
            let expected = base + chrono::Duration::microseconds(i);
            assert_eq!(e.created_at, expected, "tie {i} must be bumped in order");
            if let Some(p) = prev {
                assert!(e.created_at > p, "stamps must be strictly increasing");
            }
            prev = Some(e.created_at);
        }
    }

    #[test]
    fn stamp_monotonic_bumps_out_of_order_events_after_previous() {
        let base = micros_truncated(chrono::Utc::now());
        let mut last = None;
        let mut e = event_with_created_at(base);
        stamp_monotonic(&mut e, &mut last);
        assert_eq!(e.created_at, base);
        // A later event with an *earlier* wall-clock time must not regress.
        let mut e2 = event_with_created_at(base - chrono::Duration::seconds(1));
        stamp_monotonic(&mut e2, &mut last);
        assert_eq!(e2.created_at, base + chrono::Duration::microseconds(1));
    }

    // ── AuditSender ────────────────────────────────────────────

    #[tokio::test]
    async fn sender_try_enqueue_on_full_channel_drops_without_blocking() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<AuditEvent>(2);
        let sender = AuditSender::new(tx);
        let event = || AuditEvent {
            created_at: chrono::Utc::now(),
            actor_user_id: None,
            actor_name: SYSTEM_ACTOR.to_string(),
            action: action::INSTANCE_AUTO_SLEEP.to_string(),
            target_type: target::INSTANCE.to_string(),
            target_id: None,
            target_name: None,
            outcome: outcome::SUCCESS.to_string(),
            client_ip: None,
            detail: None,
        };
        assert!(sender.try_enqueue(event()));
        assert!(sender.try_enqueue(event()));
        // Third send on a full channel is dropped (try_send) without blocking.
        assert!(!sender.try_enqueue(event()));
        assert!(rx.recv().await.is_some());
        assert!(rx.recv().await.is_some());
        drop(sender);
        assert!(rx.recv().await.is_none());
    }
}
