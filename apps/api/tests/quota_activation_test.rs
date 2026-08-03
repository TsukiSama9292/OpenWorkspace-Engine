//! Postgres-backed integration tests for the quota activation helper and its
//! active-set queries. These exercise the real query layer and the
//! check-and-reserve transaction against a live database (see
//! `scripts/create_test_pg.sh` for how `PG_HOST`/`PG_PORT` are provided).

mod common;

#[path = "common/quota.rs"]
mod quota;

use openworkspace_api::auth::Role;
use openworkspace_api::db::WorkspaceInstanceRepository;
use openworkspace_api::quota::{AllocationMode, QuotaOverride};
use openworkspace_api::quota_activation::{
    activate, count_active_instances_for_user, count_active_instances_global,
    sum_active_resources_by_mode, sum_active_resources_for_user, sum_active_resources_global,
    ActivationError, ActivationKind, ActivationRequest,
};
use quota::{GIB, TestDb, insert_instance, insert_template, insert_user, launch_request};
use uuid::Uuid;

fn restart_request<'a>(
    template: &'a openworkspace_api::db::WorkspaceTemplate,
    user_id: Uuid,
    instance_id: Uuid,
    role: Role,
    overrides: QuotaOverride,
) -> ActivationRequest<'a> {
    ActivationRequest {
        kind: ActivationKind::Restart { instance_id },
        template,
        user_id,
        role,
        user_overrides: overrides,
    }
}

#[tokio::test]
async fn test_active_queries_count_and_sum_only_active_statuses() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, QuotaOverride::default()).await;
    let other = insert_user(&t.db, "other", Role::User, QuotaOverride::default()).await;

    let dedicated = insert_template(&t.db, owner, 2, 2 * GIB, AllocationMode::Dedicated).await;
    let shared = insert_template(&t.db, owner, 1, GIB, AllocationMode::Shared).await;

    // owner: dedicated running (counts), dedicated stopped (does not), shared
    // paused (counts)
    let d_run = insert_instance(&t.db, dedicated.id, owner, "d", "running").await;
    insert_instance(&t.db, dedicated.id, owner, "d", "stopped").await;
    let s_pause = insert_instance(&t.db, shared.id, owner, "s", "paused").await;
    // other: shared running (counts)
    insert_instance(&t.db, shared.id, other, "s", "running").await;
    // owner: shared error (does not)
    insert_instance(&t.db, shared.id, owner, "s", "error").await;

    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 2);
    assert_eq!(count_active_instances_for_user(&t.db, other).await.unwrap(), 1);
    assert_eq!(count_active_instances_global(&t.db).await.unwrap(), 3);

    let user_res = sum_active_resources_for_user(&t.db, owner).await.unwrap();
    assert_eq!(user_res.cpu_cores, 3);
    assert_eq!(user_res.ram_bytes, 3 * GIB);

    let dedicated_res = sum_active_resources_by_mode(&t.db, AllocationMode::Dedicated)
        .await
        .unwrap();
    assert_eq!(dedicated_res.cpu_cores, 2);
    assert_eq!(dedicated_res.ram_bytes, 2 * GIB);

    let shared_res = sum_active_resources_by_mode(&t.db, AllocationMode::Shared)
        .await
        .unwrap();
    assert_eq!(shared_res.cpu_cores, 2);
    assert_eq!(shared_res.ram_bytes, 2 * GIB);

    let global_res = sum_active_resources_global(&t.db).await.unwrap();
    assert_eq!(global_res.cpu_cores, 4);
    assert_eq!(global_res.ram_bytes, 4 * GIB);

    // Sanity: the two paused/running rows are the ones we tracked.
    let d = WorkspaceInstanceRepository::new(&t.db)
        .find_by_id(d_run)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(d.status, "running");
    let s = WorkspaceInstanceRepository::new(&t.db)
        .find_by_id(s_pause)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s.status, "paused");
}

#[tokio::test]
async fn test_launch_reserves_starting_row_and_counts_as_active() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, QuotaOverride::default()).await;
    let template = insert_template(&t.db, owner, 1, GIB, AllocationMode::Shared).await;

    let req = launch_request(&template, owner, Role::User, QuotaOverride::default());
    let instance = activate(&t.db, &req).await.expect("launch should succeed").instance;

    assert_eq!(instance.status, "starting");
    assert_eq!(instance.template_id, template.id);
    assert_eq!(instance.owner_id, owner);
    assert_eq!(instance.instance_number, 1);
    // The auto-name is derived together with the instance number.
    assert_eq!(instance.name, "test-template-1");
    assert!(!instance.access_token.is_empty());
    assert!(!instance.access_password.is_empty());

    // The committed `starting` row counts as active.
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 1);
    let user_res = sum_active_resources_for_user(&t.db, owner).await.unwrap();
    assert_eq!(user_res.cpu_cores, 1);
    assert_eq!(user_res.ram_bytes, GIB);
}

#[tokio::test]
async fn test_launch_rejected_when_user_limit_exceeded_leaves_no_row() {
    let t = TestDb::new().await;
    // Default `user` role: 2 instances / 4 cores / 8 GiB.
    let owner = insert_user(&t.db, "owner", Role::User, QuotaOverride::default()).await;
    let template = insert_template(&t.db, owner, 1, GIB, AllocationMode::Shared).await;

    insert_instance(&t.db, template.id, owner, "t", "running").await;
    insert_instance(&t.db, template.id, owner, "t", "starting").await;

    let req = launch_request(&template, owner, Role::User, QuotaOverride::default());
    let err = activate(&t.db, &req).await.expect_err("launch should fail");

    match err {
        ActivationError::Quota(violation) => {
            use openworkspace_api::quota::QuotaScope;
            assert_eq!(violation.scope, QuotaScope::UserInstance);
            assert_eq!(violation.current, 2);
            assert_eq!(violation.limit, 2);
        }
        ActivationError::Db(e) => panic!("unexpected db error: {e:?}"),
        ActivationError::Conflict(msg) => panic!("unexpected conflict: {msg}"),
    }

    // Rollback left no instance row behind: count is unchanged.
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 2);
    assert_eq!(WorkspaceInstanceRepository::new(&t.db).list_all().await.unwrap().len(), 2);
}

#[tokio::test]
async fn test_restart_flips_existing_row_back_to_starting() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, QuotaOverride::default()).await;
    let template = insert_template(&t.db, owner, 1, GIB, AllocationMode::Shared).await;

    let running = insert_instance(&t.db, template.id, owner, "t", "running").await;

    let req = restart_request(&template, owner, running, Role::User, QuotaOverride::default());
    let instance = activate(&t.db, &req).await.expect("restart should succeed").instance;

    assert_eq!(instance.id, running);
    assert_eq!(instance.status, "starting");

    // The restart is not a new instance: the count does not grow.
    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 1);
    let rows = WorkspaceInstanceRepository::new(&t.db).list_all().await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_concurrent_launches_at_limit_commit_exactly_one() {
    let t = TestDb::new().await;
    let owner = insert_user(&t.db, "owner", Role::User, QuotaOverride::default()).await;
    let template = insert_template(&t.db, owner, 1, GIB, AllocationMode::Shared).await;

    // One instance already active: the default limit is 2, so only one of the
    // two racing launches can win.
    insert_instance(&t.db, template.id, owner, "t", "running").await;

    let req = launch_request(&template, owner, Role::User, QuotaOverride::default());
    let (r1, r2) = tokio::join!(activate(&t.db, &req), activate(&t.db, &req));

    let oks = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    let quota_errs = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(ActivationError::Quota(_))))
        .count();
    assert_eq!(oks, 1, "exactly one racing launch should commit");
    assert_eq!(quota_errs, 1, "the loser should be a quota rejection");

    assert_eq!(count_active_instances_for_user(&t.db, owner).await.unwrap(), 2);
    let rows = WorkspaceInstanceRepository::new(&t.db).list_all().await.unwrap();
    let starting = rows.iter().filter(|i| i.status == "starting").count();
    assert_eq!(starting, 1);
}

#[tokio::test]
async fn test_admin_exempt_from_personal_limits() {
    let t = TestDb::new().await;
    let admin = insert_user(&t.db, "admin", Role::Admin, QuotaOverride::default()).await;
    let template = insert_template(&t.db, admin, 1, GIB, AllocationMode::Shared).await;

    // Two active instances already at/over the default `user` limit of 2.
    insert_instance(&t.db, template.id, admin, "t", "running").await;
    insert_instance(&t.db, template.id, admin, "t", "running").await;

    let req = launch_request(&template, admin, Role::Admin, QuotaOverride::default());
    let instance = activate(&t.db, &req).await.expect("admin should be exempt").instance;
    assert_eq!(instance.status, "starting");
    assert_eq!(count_active_instances_for_user(&t.db, admin).await.unwrap(), 3);
}
