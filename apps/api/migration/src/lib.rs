use sea_orm_migration::prelude::*;
pub use sea_orm_migration::MigratorTrait;

mod m20260723_000001_create_users_table;
mod m20260723_000002_add_vnc_token;
mod m20260723_000003_create_workspace_tables;
mod m20260723_000004_split_config_instance;
mod m20260723_000005_drop_instance_number_unique;
mod m20260723_000006_add_vnc_password;
mod m20260723_000007_expand_vnc_password;
mod m20260723_000008_add_remote_type;
mod m20260723_000009_add_container_runtime;
mod m20260731_000010_add_auto_sleep;
mod m20260731_000011_add_network_bandwidth;
mod m20260801_000012_add_keep_time;
mod m20260801_000013_add_host_port;
mod m20260802_000014_add_docker_in_instance;
mod m20260803_000015_add_system_settings;
mod m20260803_000016_add_user_quota_columns;
mod m20260803_000017_add_template_allocation_mode;
mod m20260803_000018_add_flat_rbac_tables;
mod m20260803_000019_drop_legacy_contract;
mod m20260803_000020_add_system_groups_and_drop_personal_contract;
mod m20260803_000021_add_template_visibility;
mod m20260803_000022_add_can_view_monitoring;
mod m20260807_000023_rename_runtime_value_to_runc;
mod m20260808_000024_add_audit_logs;

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
            Box::new(m20260723_000008_add_remote_type::Migration),
            Box::new(m20260723_000009_add_container_runtime::Migration),
            Box::new(m20260731_000010_add_auto_sleep::Migration),
            Box::new(m20260731_000011_add_network_bandwidth::Migration),
            Box::new(m20260801_000012_add_keep_time::Migration),
            Box::new(m20260801_000013_add_host_port::Migration),
            Box::new(m20260802_000014_add_docker_in_instance::Migration),
            Box::new(m20260803_000015_add_system_settings::Migration),
            Box::new(m20260803_000016_add_user_quota_columns::Migration),
            Box::new(m20260803_000017_add_template_allocation_mode::Migration),
            Box::new(m20260803_000018_add_flat_rbac_tables::Migration),
            Box::new(m20260803_000019_drop_legacy_contract::Migration),
            Box::new(m20260803_000020_add_system_groups_and_drop_personal_contract::Migration),
            Box::new(m20260803_000021_add_template_visibility::Migration),
            Box::new(m20260803_000022_add_can_view_monitoring::Migration),
            Box::new(m20260807_000023_rename_runtime_value_to_runc::Migration),
            Box::new(m20260808_000024_add_audit_logs::Migration),
        ]
    }
}
