use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Instances::Table)
                    .add_column_if_not_exists(string_null(Instances::VncToken).unique_key())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Instances::Table)
                    .drop_column(Instances::VncToken)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum Instances {
    Table,
    VncToken,
}
