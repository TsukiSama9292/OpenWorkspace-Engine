use sea_orm_migration::prelude::*;
pub use sea_orm_migration::MigratorTrait;

mod m20260723_000001_create_users_table;
mod m20260723_000002_add_vnc_token;
mod m20260723_000003_create_workspace_tables;
mod m20260723_000004_split_config_instance;
mod m20260723_000005_drop_instance_number_unique;
mod m20260723_000006_add_vnc_password;
mod m20260723_000007_expand_vnc_password;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260723_000001_create_users_table::Migration),
            Box::new(m20260723_000002_add_vnc_token::Migration),
            Box::new(m20260723_000003_create_workspace_tables::Migration),
            Box::new(m20260723_000004_split_config_instance::Migration),
            Box::new(m20260723_000005_drop_instance_number_unique::Migration),
            Box::new(m20260723_000006_add_vnc_password::Migration),
            Box::new(m20260723_000007_expand_vnc_password::Migration),
        ]
    }
}
