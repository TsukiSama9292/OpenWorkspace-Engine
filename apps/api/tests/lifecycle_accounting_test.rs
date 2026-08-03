//! Postgres-backed integration tests for lifecycle accounting and the
//! concurrency guarantees of the quota activation helper (spec Decision 2, 3).
//!
//! These assert the spec's lifecycle rules through the public counter/sum
//! queries — the same external behavior a caller observes — never internal
//! helpers:
//!
//! - a paused instance keeps its count, its personal resource reservations,
//!   and (for a dedicated template) its share of the host dedicated pool, so a
//!   fresh launch at the limit is still rejected;
//! - stopping or deleting an instance releases the count, the personal quota,
//!   and (for dedicated) the host pool immediately;
//! - two concurrent launches from the same user at the per-user limit commit
//!   exactly one, the loser getting a `user_instance` rejection;
//! - two concurrent launches from different users at the global limit commit
//!   exactly one, the loser getting a `host_instance` rejection.

mod common;

#[path = "common/quota.rs"]
mod quota;

use openworkspace_api::auth::Role;
use openworkspace_api::db::WorkspaceInstanceRepository;
use openworkspace_api::quota::{AllocationMode, QuotaOverride, QuotaScope};
use openworkspace_api::quota_activation::{
    activate, count_active_instances_for_user, count_active_instances_global,
    sum_active_resources_by_mode, sum_active_resources_for_user, ActivationError,
};
use openworkspace_api::system_settings::{SystemSettings, SystemSettingsRepository};
use quota::{GIB, TestDb, insert_instance, insert_template, insert_user, launch_request};
use uuid::Uuid;

/// A user capped at one instance. The launch override mirrors the row override
/// so the effective quota the check sees matches the stored one.
fn single_slot_override() -> QuotaOverride {
    QuotaOverride {
        instance_limit: Some(1),
        ..Default::default()
    }
}

/// Reserve an instance for `owner` and flip it to the given status, the way
/// the lifecycle transitions do (pause/stop are status updates on an existing
/// row). Returns the reserved row's id.
async fn reserve_and_set_status(
    t: &TestDb,
    template: &openworkspace_api::db::WorkspaceTemplate,
    owner: Uuid,
    status: &str,
) -> Uuid {
    let reservation = activate(
        &t.db,
        &launch_request(template, owner, Role::User, single_slot_override()),
    )
    .await
    .expect("first launch should succeed")
    .instance;
    assert_eq!(reservation.status, "starting");
    WorkspaceInstanceRepository::new(&t.db)
        .update_status(reservation.id, status)
        .await
        .expect("failed to set instance status");
    reservation.id
}

#[tokio::test]
async fn paused_instance_keeps_count_and_reservations() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, single_slot_override()).await;
    let template = insert_template(&t.db, owner, 2, 2 * GIB, AllocationMode::Dedicated).await;

    let paused = reserve_and_set_status(&t, &template, owner, "paused").await;

    // The paused row still counts and still holds the personal reservation.
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 1);
    let user_res = sum_active_resources_for_user(&t.db, owner).await.unwrap();
    assert_eq!(user_res.cpu_cores, 2);
    assert_eq!(user_res.ram_bytes, 2 * GIB);

    // And (dedicated template) still holds the host dedicated pool.
    let dedicated_res = sum_active_resources_by_mode(&t.db, AllocationMode::Dedicated)
        .await
        .unwrap();
    assert_eq!(dedicated_res.cpu_cores, 2);
    assert_eq!(dedicated_res.ram_bytes, 2 * GIB);

    // The paused instance holds the instance slot: a fresh launch at the limit
    // is still rejected with the user_instance scope.
    let err = activate(
        &t.db,
        &launch_request(&template, owner, Role::User, single_slot_override()),
    )
    .await
    .expect_err("launch at a fully-reserved limit should fail");
    match err {
        ActivationError::Quota(v) => {
            assert_eq!(v.scope, QuotaScope::UserInstance);
            assert_eq!(v.current, 1);
            assert_eq!(v.limit, 1);
            assert_eq!(v.requested, 1);
        }
        other => panic!("expected user_instance quota violation, got {other:?}"),
    }

    // The rejection left no row behind.
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 1);
    let stored = WorkspaceInstanceRepository::new(&t.db)
        .find_by_id(paused)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, "paused");
}

#[tokio::test]
async fn stopped_instance_releases_count_and_reservations() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, single_slot_override()).await;
    let template = insert_template(&t.db, owner, 2, 2 * GIB, AllocationMode::Dedicated).await;

    reserve_and_set_status(&t, &template, owner, "stopped").await;

    // Stopping releases the count, the personal quota, and the host pool.
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 0);
    let user_res = sum_active_resources_for_user(&t.db, owner).await.unwrap();
    assert_eq!(user_res.cpu_cores, 0);
    assert_eq!(user_res.ram_bytes, 0);
    let dedicated_res = sum_active_resources_by_mode(&t.db, AllocationMode::Dedicated)
        .await
        .unwrap();
    assert_eq!(dedicated_res.cpu_cores, 0);
    assert_eq!(dedicated_res.ram_bytes, 0);

    // The slot is free again: a fresh launch fits.
    let second = activate(
        &t.db,
        &launch_request(&template, owner, Role::User, single_slot_override()),
    )
    .await
    .expect("launch after stop should succeed")
    .instance;
    assert_eq!(second.status, "starting");
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 1);
}

#[tokio::test]
async fn deleted_instance_releases_count_and_reservations() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, single_slot_override()).await;
    let template = insert_template(&t.db, owner, 2, 2 * GIB, AllocationMode::Dedicated).await;

    let reserved = reserve_and_set_status(&t, &template, owner, "starting").await;
    WorkspaceInstanceRepository::new(&t.db)
        .delete(reserved)
        .await
        .unwrap();

    // Deleting releases the count, the personal quota, and the host pool.
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 0);
    let user_res = sum_active_resources_for_user(&t.db, owner).await.unwrap();
    assert_eq!(user_res.cpu_cores, 0);
    assert_eq!(user_res.ram_bytes, 0);
    let dedicated_res = sum_active_resources_by_mode(&t.db, AllocationMode::Dedicated)
        .await
        .unwrap();
    assert_eq!(dedicated_res.cpu_cores, 0);
    assert_eq!(dedicated_res.ram_bytes, 0);

    // The slot is free again: a fresh launch fits.
    let second = activate(
        &t.db,
        &launch_request(&template, owner, Role::User, single_slot_override()),
    )
    .await
    .expect("launch after delete should succeed")
    .instance;
    assert_eq!(second.status, "starting");
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 1);
}

#[tokio::test]
async fn dedicated_pool_held_while_active_and_released_on_stop_and_delete() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, QuotaOverride::default()).await;
    let dedicated = insert_template(&t.db, owner, 2, 2 * GIB, AllocationMode::Dedicated).await;

    // The launch reserves 2 cores / 2 GiB from the host dedicated pool.
    let first = activate(
        &t.db,
        &launch_request(&dedicated, owner, Role::User, QuotaOverride::default()),
    )
    .await
    .expect("launch should succeed")
    .instance;
    assert_eq!(
        sum_active_resources_by_mode(&t.db, AllocationMode::Dedicated)
            .await
            .unwrap()
            .cpu_cores,
        2
    );

    // Stopping the instance drops the host pool immediately.
    WorkspaceInstanceRepository::new(&t.db)
        .update_status(first.id, "stopped")
        .await
        .unwrap();
    assert_eq!(
        sum_active_resources_by_mode(&t.db, AllocationMode::Dedicated)
            .await
            .unwrap()
            .cpu_cores,
        0
    );

    // A fresh reservation reclaims the pool; deleting drops it again.
    let second = activate(
        &t.db,
        &launch_request(&dedicated, owner, Role::User, QuotaOverride::default()),
    )
    .await
    .expect("re-launch should succeed")
    .instance;
    assert_eq!(
        sum_active_resources_by_mode(&t.db, AllocationMode::Dedicated)
            .await
            .unwrap()
            .cpu_cores,
        2
    );
    WorkspaceInstanceRepository::new(&t.db)
        .delete(second.id)
        .await
        .unwrap();
    assert_eq!(
        sum_active_resources_by_mode(&t.db, AllocationMode::Dedicated)
            .await
            .unwrap()
            .cpu_cores,
        0
    );
}

#[tokio::test]
async fn concurrent_launches_same_user_commit_exactly_one() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, QuotaOverride::default()).await;
    let template = insert_template(&t.db, owner, 1, GIB, AllocationMode::Shared).await;

    // One instance already active: the default limit is 2, so only one of the
    // two racing launches can win.
    insert_instance(&t.db, template.id, owner, "t", "running").await;

    let req = launch_request(&template, owner, Role::User, QuotaOverride::default());
    let (r1, r2) = tokio::join!(activate(&t.db, &req), activate(&t.db, &req));

    assert_eq!(
        [&r1, &r2].iter().filter(|r| r.is_ok()).count(),
        1,
        "exactly one racing launch should commit"
    );

    let loser = if r1.is_ok() { &r2 } else { &r1 };
    match loser {
        Err(ActivationError::Quota(v)) => {
            assert_eq!(v.scope, QuotaScope::UserInstance);
            assert_eq!(v.current, 2);
            assert_eq!(v.limit, 2);
        }
        other => panic!("loser should be a user_instance rejection, got {other:?}"),
    }

    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 2);
    let rows = WorkspaceInstanceRepository::new(&t.db).list_all().await.unwrap();
    assert_eq!(rows.iter().filter(|i| i.status == "starting").count(), 1);
}

#[tokio::test]
async fn concurrent_launches_different_users_at_global_limit_commit_exactly_one() {
    let t = TestDb::new().await;
    let alice = insert_user(&t.db, "alice", Role::User, QuotaOverride::default()).await;
    let bob = insert_user(&t.db, "bob", Role::User, QuotaOverride::default()).await;
    let template = insert_template(&t.db, alice, 1, GIB, AllocationMode::Shared).await;

    // Global cap of one active instance; each user individually has room for
    // far more, so the host-instance check is the binding constraint.
    let current = SystemSettingsRepository::new(&t.db)
        .get()
        .await
        .unwrap()
        .expect("system_settings singleton should be seeded by the migration");
    SystemSettingsRepository::new(&t.db)
        .upsert(&SystemSettings {
            host_instance_limit: 1,
            ..current
        })
        .await
        .unwrap();

    let req_a = launch_request(&template, alice, Role::User, QuotaOverride::default());
    let req_b = launch_request(&template, bob, Role::User, QuotaOverride::default());
    let (r1, r2) = tokio::join!(activate(&t.db, &req_a), activate(&t.db, &req_b));

    assert_eq!(
        [&r1, &r2].iter().filter(|r| r.is_ok()).count(),
        1,
        "exactly one racing launch should commit"
    );

    let loser = if r1.is_ok() { &r2 } else { &r1 };
    match loser {
        Err(ActivationError::Quota(v)) => {
            assert_eq!(v.scope, QuotaScope::HostInstance);
            assert_eq!(v.current, 1);
            assert_eq!(v.limit, 1);
        }
        other => panic!("loser should be a host_instance rejection, got {other:?}"),
    }

    assert_eq!(count_active_instances_global(&t.db).await.unwrap(), 1);
}
