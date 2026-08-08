//! Endpoint-level tests for the audit trail (observability-logs spec Decision 8
//! and the authenticated-only `auth.forbidden` middleware): RBAC gating,
//! filters, keyset pagination over the HTTP surface. The audit channel is
//! async-best-effort (flush at most every 500 ms), so reads poll briefly until
//! the expected rows land.

mod common;

use common::TestContext;

async fn audit_entries(ctx: &TestContext, query: &str) -> Vec<serde_json::Value> {
    let resp = ctx.get(&format!("/api/audit{}", query)).await;
    assert_eq!(
        resp.status(),
        200,
        "a can_view_audit_logs holder must be allowed to read the trail"
    );
    let body: serde_json::Value = resp
        .json()
        .await
        .expect("audit response must be JSON");
    body["entries"].as_array().cloned().unwrap_or_default()
}

/// Poll until the writer flushes at least `min` entries for `action` (the async
/// audit channel flushes at most 500 ms after enqueue).
async fn wait_for_action(ctx: &TestContext, action: &str, min: usize) -> Vec<serde_json::Value> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let entries = audit_entries(ctx, &format!("?action={}", action)).await;
        if entries.len() >= min || std::time::Instant::now() > deadline {
            return entries;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

async fn create_plain_user(ctx: &TestContext, username: &str) {
    let resp = ctx
        .post(
            "/api/users",
            &serde_json::json!({ "username": username, "password": "pw" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "admin must be able to create a user");
}

/// Two admin logins enqueue two `auth.login` events; the endpoint returns them
/// newest-first, filters narrow them, and `limit` + `next_cursor` page without
/// overlap or skips.
#[tokio::test]
async fn test_audit_query_filters_and_paginates() {
    let ctx = TestContext::new().await;
    assert_eq!(ctx.login_admin().await.status(), 200);
    assert_eq!(ctx.login_admin().await.status(), 200);

    let logins = wait_for_action(&ctx, "auth.login", 2).await;
    assert!(logins.iter().all(|e| e["action"] == "auth.login"));
    assert!(logins.iter().all(|e| e["outcome"] == "success"));
    assert!(logins.iter().all(|e| e["actor_name"] == "admin"));
    assert!(
        logins.iter().all(|e| e["created_at"].is_string()),
        "entries carry timestamps"
    );

    // Newest-first: the second login precedes the first.
    let t0 = logins[0]["created_at"].as_str().unwrap().to_string();
    let t1 = logins[1]["created_at"].as_str().unwrap().to_string();
    assert!(t0 >= t1, "entries are newest-first");

    // actor substring filter
    let filtered = audit_entries(&ctx, "?actor=min").await;
    assert!(
        filtered.iter().all(|e| e["actor_name"] == "admin"),
        "actor filter narrows to matching rows"
    );
    // outcome filter
    let ok = audit_entries(&ctx, "?outcome=success").await;
    assert!(!ok.is_empty());
    assert!(ok.iter().all(|e| e["outcome"] == "success"));

    // Keyset pagination over the two login rows, one per page.
    let resp = ctx.get("/api/audit?action=auth.login&limit=1").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let page1 = body["entries"].as_array().unwrap();
    assert_eq!(page1.len(), 1);
    let cursor = body["next_cursor"]
        .as_str()
        .expect("more rows remain after the first page")
        .to_string();
    let cursor_q = cursor.replace('+', "%2B");

    let resp2 = ctx
        .get(&format!("/api/audit?action=auth.login&limit=1&cursor={}", cursor_q))
        .await;
    let body2: serde_json::Value = resp2.json().await.unwrap();
    let page2 = body2["entries"].as_array().unwrap();
    assert_eq!(page2.len(), 1);
    assert_ne!(
        page1[0]["id"], page2[0]["id"],
        "pages must not overlap"
    );
}

/// The trail is gated: a user without `can_view_audit_logs` gets 403, and an
/// anonymous client gets 401 (never a leak of the entries).
#[tokio::test]
async fn test_audit_query_denied_without_flag() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    create_plain_user(&ctx, "plain-audit").await;

    ctx.login_user("plain-audit", "pw").await;
    assert_eq!(ctx.get("/api/audit").await.status(), 403);

    let anon = reqwest::Client::new();
    let resp = anon
        .get(format!("{}/api/audit", ctx.base_url))
        .send()
        .await
        .expect("anonymous request failed");
    assert_eq!(resp.status(), 401);
}

/// Authenticated 403s are recorded as `auth.forbidden` failure events;
/// anonymous 401/403 scanner noise is never audited.
#[tokio::test]
async fn test_auth_forbidden_recorded_for_authenticated_403_only() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let username = "forbidden-user";
    create_plain_user(&ctx, username).await;

    // The plain user hits an admin-gated surface and is rejected 403.
    ctx.login_user(username, "pw").await;
    assert_eq!(ctx.get("/api/groups").await.status(), 403);

    // Anonymous hits the same surface: 401, and no `auth.forbidden` row.
    let anon = reqwest::Client::new();
    assert_eq!(
        anon.get(format!("{}/api/groups", ctx.base_url))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );

    // Back to admin to read the trail.
    ctx.login_admin().await;
    let forbidden = wait_for_action(&ctx, "auth.forbidden", 1).await;
    assert!(
        forbidden.iter().any(|e| e["actor_name"] == username),
        "the authenticated 403 must be attributed to its actor"
    );
    assert!(forbidden.iter().all(|e| e["outcome"] == "failure"));
    assert!(
        !forbidden.iter().any(|e| e["actor_name"] == "anonymous"),
        "anonymous noise is never audited"
    );
}
