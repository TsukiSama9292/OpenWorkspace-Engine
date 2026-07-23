use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Users::Table)
                    .if_not_exists()
                    .col(uuid(Users::Id).primary_key().default(Expr::cust("gen_random_uuid()")))
                    .col(string(Users::Username).unique_key().not_null())
                    .col(string(Users::PasswordHash).not_null())
                    .col(string(Users::Role).not_null().default("user"))
                    .col(
                        timestamp_with_time_zone(Users::CreatedAt)
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .col(
                        timestamp_with_time_zone(Users::UpdatedAt)
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Instances::Table)
                    .if_not_exists()
                    .col(uuid(Instances::Id).primary_key().default(Expr::cust("gen_random_uuid()")))
                    .col(string(Instances::Name).not_null())
                    .col(
                        integer(Instances::InstanceNumber)
                            .auto_increment()
                            .unique_key()
                            .not_null(),
                    )
                    .col(string(Instances::ContainerId).null())
                    .col(string(Instances::Status).not_null().default("stopped"))
                    .col(uuid(Instances::OwnerId).not_null())
                    .col(
                        timestamp_with_time_zone(Instances::CreatedAt)
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .col(
                        timestamp_with_time_zone(Instances::UpdatedAt)
                            .not_null()
                            .default(Expr::cust("NOW()")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_instances_owner")
                            .from(Instances::Table, Instances::OwnerId)
                            .to(Users::Table, Users::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Instances::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Users::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
    Username,
    PasswordHash,
    Role,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Instances {
    Table,
    Id,
    Name,
    InstanceNumber,
    ContainerId,
    Status,
    OwnerId,
    CreatedAt,
    UpdatedAt,
}
