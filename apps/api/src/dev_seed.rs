//! Dev-only database fixtures.
//!
//! Gated by `OW_DEV_SEED` (see `Settings::dev_seed`): `main.rs` calls
//! [`seed_dev_data`] at startup only when the flag is set. Production compose
//! never sets it, so these fixtures cannot leak into production data.
//!
//! Every fixture here must be idempotent (safe to run on every dev boot) and
//! must never alter an existing row's secrets or edits — a developer who
//! changed a password or tweaked a fixture keeps their changes across
//! restarts.
//!
//! Extension point: future dev fixtures (seed settings) get their own
//! `seed_dev_*` function in the owning repository plus one call in
//! [`seed_dev_data`], following `seed_dev_user` / `seed_dev_templates`.

use sea_orm::DatabaseConnection;

use crate::db::{UserRepository, WorkspaceTemplateRepository};

/// Seed all dev fixtures. Currently: the plain `user` account plus the three
/// default templates (KasmVNC desktop, ttyd terminal, Jupyter lab).
pub async fn seed_dev_data(
    db: &DatabaseConnection,
    dev_user_password: &str,
) -> Result<(), sea_orm::DbErr> {
    UserRepository::new(db)
        .seed_dev_user(dev_user_password)
        .await?;
    match UserRepository::new(db).find_by_username("admin").await? {
        Some(admin) => {
            WorkspaceTemplateRepository::new(db)
                .seed_dev_templates(admin.id)
                .await?;
        }
        None => {
            tracing::warn!("OW_DEV_SEED: admin user missing, skipping dev template fixtures");
        }
    }
    Ok(())
}
