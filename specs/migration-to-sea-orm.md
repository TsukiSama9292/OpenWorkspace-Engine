# Spec: Migrate to sea-orm-migration

## Problem Statement

The current database migration system uses `sqlx::migrate!()` with raw SQL files. This approach has several issues:
- No rollback support (no `down` migration)
- SQL files are not type-safe
- CTE scoping bugs when sqlx runs multi-statement migrations separately
- No CLI for generating migrations
- Not comparable to Python's SQLAlchemy + Alembic workflow

## Solution

Replace `sqlx::migrate!()` with `sea-orm-migration`, providing Rust-based version-controlled migrations with up/down support, CLI generation, and a migration tracking table (`seaql_migrations`).

**Critical constraint**: The project uses sqlx 0.8 for queries. Sea-orm-migration 2.0 requires sqlx 0.9 (breaking change). To avoid a full sqlx upgrade, we use **sea-orm-migration 1.x** which is compatible with sqlx 0.8. The migration runner uses its own `DatabaseConnection`; raw sqlx queries continue to use `PgPool` unchanged.

## User Stories

1. As a developer, I want to generate a new migration file with `sea-orm-cli migrate generate`, so that I get a properly named Rust file
2. As a developer, I want each migration to have `up()` and `down()` methods, so that I can rollback changes
3. As a developer, I want migrations to be Rust code using SeaQuery, so that I get type-safe schema definitions
4. As a developer, I want raw SQL support in migrations via `execute_unprepared()`, so that I can handle complex SQL that SeaQuery can't express
5. As a developer, I want the app to auto-apply pending migrations on startup, so that the schema is always up to date
6. As a developer, I want to check migration status via CLI, so that I can see which migrations are applied
7. As a developer, I want to rollback the last N migrations, so that I can undo mistakes
8. As a developer, I want the existing 4 SQL migrations converted to sea-orm-migration Rust files, so that the history is preserved
9. As a developer, I want the migration system to coexist with raw sqlx queries, so that I don't need to rewrite the entire data layer
10. As a developer, I want a `migration/` sub-crate (or module) with its own `Cargo.toml`, so that dependencies are isolated

## Implementation Decisions

### Architecture

- **Sub-crate approach**: Create `apps/api/migration/` as a separate Cargo workspace member with its own `Cargo.toml`. This isolates sea-orm dependencies from the main crate.
- **Dual connection**: The migration runner creates its own `sea_orm::DatabaseConnection` from `DATABASE_URL`. The main app continues using `sqlx::PgPool` for queries.
- **No entity generation**: We do NOT generate sea-orm entities. The `db.rs` repository pattern with raw sqlx stays unchanged.

### Dependencies

```toml
# migration/Cargo.toml
[dependencies]
sea-orm-migration = { version = "1", features = ["sqlx-postgres", "runtime-tokio", "tls-rustls"] }
sea-orm = { version = "1", features = ["sqlx-postgres", "runtime-tokio", "tls-rustls"] }
async-trait = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

The main `apps/api/Cargo.toml` adds:
```toml
migration = { path = "migration" }
```

### Migration File Structure

```
apps/api/migration/
├── Cargo.toml
└── src/
    ├── lib.rs                          # Migrator struct + migration list
    ├── main.rs                         # CLI entry point
    ├── m20260723_000001_create_users_table.rs
    ├── m20260723_000002_add_vnc_token.rs
    ├── m20260723_000003_create_workspace_tables.rs  # Renamed workspaces + registry
    └── m20260723_000004_split_config_instance.rs     # Config/instance split
```

### Migration Contents

Each migration uses `#[derive(DeriveMigrationName)]` and implements `MigrationTrait`:

- **up()**: Uses `manager.create_table()`, `manager.alter_table()`, `manager.exec_stmt()` (for raw SQL), and SeaQuery builder for schema changes
- **down()**: Uses `manager.drop_table()`, `manager.alter_table()` for rollback

The 4 existing migrations map to:
1. `m20260723_000001_create_users_table` — creates `users` and `instances` tables
2. `m20260723_000002_add_vnc_token` — adds `vnc_token` column
3. `m20260723_000003_rename_to_workspaces` — renames to `workspaces`, adds config columns, creates registry tables
4. `m20260723_000004_split_config_instance` — creates `workspace_configs` + `workspace_instances`, migrates data, drops `workspaces`

### main.rs Changes

Replace:
```rust
sqlx::migrate!("./migrations")
    .run(&db)
    .await
    .expect("Failed to run migrations");
```

With:
```rust
use migration::{Migrator, MigratorTrait};

let migrator_db = sea_orm::Database::connect(&database_url).await
    .expect("Failed to connect for migrations");
Migrator::up(&migrator_db, None).await
    .expect("Failed to run migrations");
```

### Migration Tracking

Sea-orm-migration uses its own tracking table `seaql_migrations` (separate from sqlx's `_sqlx_migrations`). On first run, the existing `_sqlx_migrations` table (with versions 1-3) becomes stale. We handle this by:
1. Dropping the old `_sqlx_migrations` table during migration
2. Or: Running `Migrator::fresh()` to start clean (since the DB can be recreated in dev)

**Decision**: For dev, use `Migrator::fresh()` on first run. For production, manually mark old migrations as applied or use `Migrator::reset()`.

### SeaQuery vs Raw SQL in Migrations

For complex operations (CTEs, data migrations with `DISTINCT ON`), use raw SQL via `manager.exec_stmt()`:
```rust
manager.exec_stmt(
    "CREATE TEMPORARY TABLE _migration_configs AS SELECT ..."
).await?;
```

For schema changes (CREATE TABLE, ALTER TABLE, ADD COLUMN), prefer SeaQuery builder for type safety.

## Testing Decisions

- **Schema verification**: After migration, query `information_schema.tables` to verify tables exist
- **Rollback testing**: Run `Migrator::down()` then verify tables are dropped
- **Data migration testing**: Insert test data, run migration, verify data integrity
- **Integration with existing tests**: The 21 existing vitest tests test the frontend; backend tests remain unchanged since db.rs is unchanged

## Out of Scope

- Replacing sqlx queries with sea-orm entity queries (no ORM migration)
- Generating sea-orm entities from the database schema
- Using sea-orm's `DatabaseConnection` for the main app (keep `PgPool`)
- Upgrading sqlx from 0.8 to 0.9 (future work)
- CLI binary for running migrations separately from the app

## Further Notes

- The old `apps/api/migrations/*.sql` files should be kept as reference but are no longer used by the app
- The `seaql_migrations` table tracks which Rust migrations are applied
- `sea-orm-cli` can be installed via `cargo install sea-orm-cli` for generating new migrations
- The migration sub-crate approach follows the official sea-orm recommended project structure
