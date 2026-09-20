use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Per-member resource quotas and the instance-accounting backfill
/// (resource-quotas v2 spec Decision 3).
///
/// Adds the personal resource cap to each `user_groups` membership row:
/// `cpu_quota` / `memory_quota` / `gpu_quota`, all `NOT NULL` with the schema
/// default `0` (blocked). Existing memberships are rewritten to `-1`
/// (unlimited) so the upgrade is behavior-preserving — no tenant is locked out
/// by a quota they never agreed to. New memberships (fresh users, group joins)
/// default to `0` and stay blocked until a manager/admin assigns quotas.
///
/// A `created_at` timestamp is added to `user_groups` so the "oldest
/// membership" tie-break of the highest-tier attribution rule is real going
/// forward. Legacy rows all share the backfill timestamp, so the migration
/// tie-breaks those deterministically by group name.
///
/// The migration also completes the instance-accounting backfill that 000025
/// deferred: every existing instance gets `owner_group_id` = its owner's
/// highest-tier membership (admin > manager > user, oldest membership wins) and
/// its resource snapshot (`host_cpu_cores` / `host_memory_mb` /
/// `host_gpu_count`) plus a frozen `billing_group_snapshot` taken from the
/// group's current pool. Instances whose owner belongs to no group keep a
/// `NULL` billing group (they count against no pool, harmless while pools are
/// `-1`). Memory is converted from the template's bytes to the quota layer's
/// MB; a `-1` (unlimited) template resource stays `-1` on the snapshot.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Per-member resource quotas on the membership rows. `0` = blocked,
        // `>0` = the value, `-1` = unlimited (spec Decision 1).
        conn.execute_unprepared(
            r#"
            ALTER TABLE user_groups
                ADD COLUMN cpu_quota INT NOT NULL DEFAULT 0,
                ADD COLUMN memory_quota BIGINT NOT NULL DEFAULT 0,
                ADD COLUMN gpu_quota INT NOT NULL DEFAULT 0,
                ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            "#,
        )
        .await?;

        // Existing memberships → `-1` (unlimited), so upgrades never lock out
        // a tenant who was never assigned a quota.
        conn.execute_unprepared(
            "UPDATE user_groups SET cpu_quota = -1, memory_quota = -1, gpu_quota = -1",
        )
        .await?;

        // Complete the instance-accounting backfill. Billing group = the
        // owner's highest-tier membership, oldest membership winning ties;
        // legacy rows share the backfill timestamp so the fallback order is
        // by group name.
        conn.execute_unprepared(
            r#"
            UPDATE workspace_instances wi
            SET owner_group_id = (
                SELECT ug.group_id
                FROM user_groups ug
                JOIN groups g ON g.id = ug.group_id
                WHERE ug.user_id = wi.owner_id
                ORDER BY
                    CASE g.kind WHEN 'admin' THEN 2 WHEN 'manager' THEN 1 ELSE 0 END DESC,
                    ug.created_at ASC,
                    g.name ASC
                LIMIT 1
            )
            WHERE wi.owner_group_id IS NULL
            "#,
        )
        .await?;

        // Resource snapshot from the template's current values, memory in the
        // quota layer's MB. A `-1` (unlimited) template resource stays `-1`.
        conn.execute_unprepared(
            r#"
            UPDATE workspace_instances wi
            SET
                host_cpu_cores = t.cores,
                host_memory_mb = CASE
                    WHEN t.memory < 0 THEN -1
                    ELSE ceil(t.memory / 1048576.0)::bigint
                END,
                host_gpu_count = t.gpu_count
            FROM workspace_templates t
            WHERE t.id = wi.template_id
            "#,
        )
        .await?;

        // Freeze the billing group's current pool on each attributed instance.
        conn.execute_unprepared(
            r#"
            UPDATE workspace_instances wi
            SET billing_group_snapshot = jsonb_build_object(
                'group_id', g.id,
                'billing_model', g.billing_model,
                'pool_cpu_cores', g.pool_cpu_cores,
                'pool_memory_mb', g.pool_memory_mb,
                'pool_gpu_count', g.pool_gpu_count
            )
            FROM groups g
            WHERE g.id = wi.owner_group_id AND wi.billing_group_snapshot IS NULL
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // The instance backfill is a data rewrite and cannot be reversed; only
        // the columns this migration added are dropped. The rewritten instance
        // rows remain valid under the pre-000027 schema (the columns they use
        // came from 000025).
        conn.execute_unprepared("ALTER TABLE user_groups DROP COLUMN cpu_quota").await?;
        conn.execute_unprepared("ALTER TABLE user_groups DROP COLUMN memory_quota").await?;
        conn.execute_unprepared("ALTER TABLE user_groups DROP COLUMN gpu_quota").await?;
        conn.execute_unprepared("ALTER TABLE user_groups DROP COLUMN created_at").await?;

        Ok(())
    }
}
