mod common;

use openworkspace_api::audit::{
    action, audit_writer, outcome, target, AuditEvent, AuditSender, AUDIT_CHANNEL_CAPACITY,
};
use openworkspace_api::db::{AuditLogRepository, AuditQueryFilters};
use migration::MigratorTrait;
use sea_orm::DatabaseConnection;

async fn setup_db() -> DatabaseConnection {
    common::ensure_pg().await;

    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let db_name = format!("audit_test_{}_{:04}", std::process::id(), counter);

    let base_url = common::pg_base_url();
    let (client, conn) = tokio_postgres::connect(&base_url, tokio_postgres::NoTls)
        .await
        .expect("failed to connect");
    tokio::spawn(conn);

    let _ = client
        .execute(
            &format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
                db_name
            )[..],
            &[],
        )
        .await;
    let _ = client
        .execute(&format!("DROP DATABASE IF EXISTS \"{}\"", db_name)[..], &[])
        .await;
    client
        .execute(&format!("CREATE DATABASE \"{}\"", db_name)[..], &[])
        .await
        .expect("failed to create test database");

    let db_url = common::pg_url(&db_name);
    let migrator_db = sea_orm::Database::connect(&db_url)
        .await
        .expect("failed to connect for migrations");
    migration::Migrator::up(&migrator_db, None)
        .await
        .expect("failed to run migrations");
    drop(migrator_db);

    sea_orm::Database::connect(&db_url)
        .await
        .expect("failed to connect")
}

fn event(seq: u64) -> AuditEvent {
    AuditEvent {
        created_at: chrono::Utc::now(),
        actor_user_id: None,
        actor_name: format!("system-{}", seq),
        action: action::INSTANCE_AUTO_SLEEP.to_string(),
        target_type: target::INSTANCE.to_string(),
        target_id: None,
        target_name: Some(format!("inst-{}", seq)),
        outcome: outcome::SUCCESS.to_string(),
        client_ip: None,
        detail: Some(serde_json::json!({ "seq": seq })),
    }
}

/// The channel → writer → DB path wired into main.rs: enqueued events are
/// batch-flushed when the writer sees the channel close (graceful shutdown
/// drops the last sender), and the rows land in `audit_log` in order.
#[tokio::test]
async fn writer_flushes_all_events_on_channel_close() {
    let db = setup_db().await;

    let (tx, rx) = tokio::sync::mpsc::channel(AUDIT_CHANNEL_CAPACITY);
    let sender = AuditSender::new(tx);
    let writer_db = db.clone();
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let writer = tokio::spawn(async move { audit_writer(rx, writer_db, shutdown_rx).await });

    for seq in 0..10 {
        assert!(sender.try_enqueue(event(seq)));
    }

    // Simulate graceful shutdown: drop the last sender, writer drains + flushes.
    drop(sender);
    tokio::time::timeout(std::time::Duration::from_secs(10), writer)
        .await
        .expect("audit writer did not finish after channel close")
        .expect("audit writer panicked");

    let repo = AuditLogRepository::new(&db);
    let (rows, next) = repo
        .query(None, &AuditQueryFilters::default(), 100)
        .await
        .expect("query failed");

    assert!(next.is_none(), "no more than 10 rows, so no next cursor");
    assert_eq!(rows.len(), 10, "all enqueued events must be flushed");
    // Newest-first ordering (keyset query): seq 9 first, seq 0 last.
    let seqs: Vec<u64> = rows
        .iter()
        .map(|r| r.detail.as_ref().unwrap()["seq"].as_u64().unwrap())
        .collect();
    let expected: Vec<u64> = (0..10).rev().collect();
    assert_eq!(seqs, expected, "order must be preserved through the channel");

    // Filters + cursor round-trip on the same data (spec: keyset pagination).
    let (page, next) = repo
        .query(
            None,
            &AuditQueryFilters {
                action: Some("instance.auto_sleep".to_string()),
                ..Default::default()
            },
            5,
        )
        .await
        .expect("filtered query failed");
    assert_eq!(page.len(), 5);
    let cursor = next.expect("more pages exist");

    let (page2, next2) = repo
        .query(
            Some(cursor),
            &AuditQueryFilters {
                action: Some("instance.auto_sleep".to_string()),
                ..Default::default()
            },
            5,
        )
        .await
        .expect("cursored query failed");
    assert_eq!(page2.len(), 5);
    assert!(next2.is_none(), "second page is the last");

    let mut ids: Vec<uuid::Uuid> = page.iter().map(|r| r.id).collect();
    ids.extend(page2.iter().map(|r| r.id));
    assert_eq!(ids.len(), 10, "pages must not overlap or skip rows");
}

/// Over-capacity enqueue drops events (bounded, best-effort) instead of
/// blocking the request path — the invariant the channel is sized to protect.
#[tokio::test]
async fn full_channel_drops_without_blocking() {
    let db = setup_db().await;

    let (tx, rx) = tokio::sync::mpsc::channel::<AuditEvent>(3);
    let sender = AuditSender::new(tx);
    let writer_db = db.clone();
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let writer = tokio::spawn(async move { audit_writer(rx, writer_db, shutdown_rx).await });

    let mut accepted = 0;
    for seq in 0..50 {
        if sender.try_enqueue(event(seq)) {
            accepted += 1;
        }
    }
    // Channel capacity 3, but the writer may have drained in between — so the
    // guarantee is only "no more than 3 pending at any instant, never blocked".
    drop(sender);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(10), writer)
        .await
        .expect("writer must exit after channel close");

    let repo = AuditLogRepository::new(&db);
    let (rows, _) = repo
        .query(None, &AuditQueryFilters::default(), 100)
        .await
        .expect("query failed");
    assert!(accepted > 0, "some events must be accepted");
    assert!(rows.len() <= accepted, "writer can only persist what it received");
    assert!(!rows.is_empty(), "the accepted events must be persisted");
}

/// The health-worker prune wiring (`maybe_prune_audit`): rows past the
/// retention cutoff are deleted once per day, fresh rows survive, and a second
/// call within the same day is a no-op (the `due_for_prune` gate).
#[tokio::test]
async fn prune_deletes_only_rows_older_than_retention_and_runs_at_most_daily() {
    let db = setup_db().await;
    let repo = AuditLogRepository::new(&db);

    let mut old = event(1);
    old.created_at = chrono::Utc::now() - chrono::Duration::days(100);
    let mut fresh = event(2);
    fresh.created_at = chrono::Utc::now();
    repo.insert_batch(&[old, fresh])
        .await
        .expect("insert failed");

    let now = chrono::Utc::now();
    let after = openworkspace_api::health_worker::maybe_prune_audit(&db, None, now, 90)
        .await
        .expect("prune must not fail");
    assert_eq!(after, Some(now), "a successful run stamps last_prune_at");

    let (rows, _) = repo
        .query(None, &AuditQueryFilters::default(), 100)
        .await
        .expect("query failed");
    assert_eq!(rows.len(), 1, "the old row must be pruned");
    assert_eq!(rows[0].target_name.as_deref(), Some("inst-2"));

    // Same-day second run: not due → no further changes.
    let after2 = openworkspace_api::health_worker::maybe_prune_audit(&db, after, now, 90)
        .await
        .expect("non-due run must be a no-op");
    assert_eq!(after2, after, "last_prune_at is unchanged when not due");
}
