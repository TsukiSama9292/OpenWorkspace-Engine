# Backup and Restore (Transitional CLI Procedures)

> Audience: operations engineers. This is the **transitional** backup &
> restore story for the control plane while the platform ships no built-in
> automation for either the API or the web UI. Everything here is standard
> CLI work against the production Compose stack under
> `docker/openworkspace/`. When a real backup feature lands, this document is
> superseded.

## 1. The boundary: what the platform can and cannot back up

Be explicit about scope before running anything:

| Layer | Backed up here? | Who owns it |
|---|---|---|
| Control-plane database (PostgreSQL) | **Yes** — full logical dump & restore below | The platform (server-side data only) |
| User instance persistent data (PVs) | **No** — see below | Operations / storage infrastructure |

### Control-plane database — the platform's responsibility

The PostgreSQL database is the server-side source of truth: users, groups,
templates, instances, persistent-volume registry, admin settings. It is
**only the server's data layer** — it does **not** contain, and this procedure
does **not** touch, any developer/user work stored inside instance sessions.

### User instance persistent data (PVs) — outside this procedure's scope

Persistent instance data (a user's home directory mounted from a per-template
`persistent_storage_path` host directory) is **not backed up by the platform**.
There is no built-in backup or export; the platform deliberately never
auto-copies it. True durability for that data exists **only** when the
operator has deployed the persistent volume storage on a device that provides
its own backup and restore — e.g. a **NAS** (with its own snapshots / RAID /
replication) or another storage system with built-in backup/restore
capabilities. If PVs live on plain host disk, a platform-level restore of the
database does **nothing** for user session data — a DB restore alone can even
recreate registry rows that reference data which no longer exists. **Backing up
and restoring developer data is an operations/infrastructure concern, not a
platform one.**

For completeness, a host-level `tar` of the PV mount directories (e.g.
`/var/lib/openworkspace/pv/` or whatever the templates' `persistent_storage_path`
values point at) is a **best-effort, manual** copy — suitable for a quick
snapshot, **not** a substitute for backing up the storage backend itself. See
[Persistent Volumes: best-effort host tar](#4-persistent-volumes-best-effort-host-tar)
below.

## 2. Identify the control-plane database container

The production Postgres service runs as `ow-postgres` (see
`docker/openworkspace/docker-compose.yml`). The credentials and database name
come from the same file's environment defaults:

```yaml
POSTGRES_USER=${POSTGRES_USER:-postgres}
POSTGRES_DB=${POSTGRES_DB:-postgres}
```

So with default settings the container is `ow-postgres`, user `postgres`,
database `postgres`. All commands below use shell defaults so they honor the
actual Compose environment when it differs:

```bash
DB_CONTAINER=${DB_CONTAINER:-ow-postgres}
DB_USER=${POSTGRES_USER:-postgres}
DB_NAME=${POSTGRES_DB:-postgres}
```

> Dev stack note: the dev Compose file (`docker/openworkspace_dev/`) uses
> `ow-dev-postgres`. Point `DB_CONTAINER` there when backing up a dev stack.

## 3. Backup: control-plane database

`pg_dump` produces a consistent logical snapshot while Postgres keeps running,
so **no service needs to be stopped for a backup**:

```bash
docker exec -t "$DB_CONTAINER" pg_dump -U "$DB_USER" "$DB_NAME" > ow_backup.sql
```

Options worth adding once you rely on it:

- `-Fc` + `pg_restore` for faster, selective restores:
  `docker exec -t "$DB_CONTAINER" pg_dump -Fc -U "$DB_USER" "$DB_NAME" > ow_backup.dump`
- `--no-owner` / `--no-privileges` if you restore into a differently-managed DB.
- Keep backups off the same disk as `server-pgdata`; rotate and test-restore
  periodically.

## 4. Persistent volumes: best-effort host tar

If you choose to snapshot the PV directories on host disk, pack them while no
instances are writing — i.e. instances should be stopped. The exact layout is
defined by each template's `persistent_storage_path`; a common layout is a
root like `/var/lib/openworkspace/pv/`:

```bash
tar -czf ow_pv_backup.tar.gz -C /var/lib/openworkspace /pv
```

Restore by unpacking over the same paths:

```bash
tar -xzf ow_pv_backup.tar.gz -C /var/lib/openworkspace
```

Again: this is a manual, best-effort copy. The durable solution for user data
is deploying that storage on a NAS or storage system with its own backup and
restore. This platform procedure does not guarantee it.

## 5. Restore: control-plane database

**Stop the control-plane services first.** The API and web containers keep
writing to the database; restoring under live writers causes data races and a
partially overwritten DB. Postgres itself must keep running (it is the target),
so stop everything else:

```bash
docker compose -f docker/openworkspace/docker-compose.yml stop api web traefik
```

Then replay the plain-SQL dump (the output of section 3):

```bash
cat ow_backup.sql | docker exec -i "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME"
```

- This appends onto the current database. For a **full overwrite** (replace
  existing data, not merge), take the database down and restore into a fresh
  one, or re-create it first:
  ```bash
  docker exec -i "$DB_CONTAINER" psql -U "$DB_USER" -d postgres -c "DROP DATABASE \"$DB_NAME\";"
  docker exec -i "$DB_CONTAINER" psql -U "$DB_USER" -d postgres -c "CREATE DATABASE \"$DB_NAME\";"
  cat ow_backup.sql | docker exec -i "$DB_CONTAINER" psql -U "$DB_USER" -d "$DB_NAME"
  ```
- The `-t` / `-i` flags matter: the dump is captured from the container's
  stdout with no TTY, and fed back via stdin — never allocate a TTY for the
  `psql` side (`-i` is intentional, do not add `-t`).

After the restore succeeds, start the control plane back up:

```bash
docker compose -f docker/openworkspace/docker-compose.yml start api web traefik
```

Verify before resuming service: log in, check groups/users/templates are
present, and spot-check instance records.

## Related docs

- `docs/developer-guide/tech-stack.md` — tech decisions, deployment flow, env vars.
- `docs/user-guide/persistent-storage.md` — what persistent instance data is
  and its known limitations (incl. "no backup/export built in").
- `docs/user-guide/rbac.md` — what lives in the control-plane DB.
