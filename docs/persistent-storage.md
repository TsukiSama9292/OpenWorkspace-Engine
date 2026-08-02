# Persistent Storage

How Instance user data is persisted across stop / start / delete using Docker
**Local Bind-mounted Named Volumes**, so a user's work (Jupyter notebooks, IDE
settings, installed packages) survives container lifecycle events and only an
explicit **reset** wipes it.

## Overview

Each Instance is a container launched from a Template. Without persistence, a
stop / remove destroys everything in the container. With persistence, the
user's **whole home directory** is backed by a Named Volume that lives on the
host at a predictable, API-controlled path.

| Persistence source | Location |
|---|---|
| Design decisions / lifecycles | this document |
| Original PRD + all implementation tickets | `.scratch/persistent_storage/spec.md`, `.scratch/persistent_storage/issues/*.md` |
| Pure-function implementation | `apps/api/src/persistent_volume.rs` |
| Docker lifecycle implementation | `apps/api/src/docker.rs` |
| Route / DB wiring | `apps/api/src/routes/workspace/instances.rs`, `apps/api/src/db.rs` |
| Frontend | `apps/web/src/routes/+page.svelte`, `apps/web/src/lib/api/template-actions.ts`, `apps/web/src/lib/components/forms/TemplateResources.svelte` |

## Core mechanism

**Local Bind-mounted Named Volume.** A Docker Named Volume whose backing
storage is bind-mounted onto a fixed host path via `driver=local`,
`type=none` / `device=<host_path>` / `o=bind`. Containers mount the volume **by
name**, not by host path.

**Docker copy-up ("Populate").** When a brand-new **empty** volume is first
mounted onto a non-empty container path, the Docker daemon copies the image's
built-in files into the volume. Because the volume backs the user's whole
home, the image's `.bashrc`, X11 / VNC / Jupyter configs survive — no masking,
no black-screen, no crash — and everything the user writes afterwards persists.

**Whole-home mount target** (hard-coded per remote type, see
[`persistent_container_target`](#path-resolution--naming)):

| remote_type | in-container mount target |
|---|---|
| `kasmvnc` | `/home/kasm-user` |
| `ttyd` | `/home/ow_user` |
| `jupyter` | `/home/ow_user` |

## Data model

| Field | Entity | Meaning |
|---|---|---|
| `persistent_storage_path` | Template | Host **root directory** for persistent data (e.g. `/mnt/ow_dir`). `NULL` / unset disables persistence for that Template. |
| `mount_persistent` | Instance | `true` when the Instance uses persistence (both `use` and `reset` modes). |
| `resolved_volume_host_path` | Instance | The API-resolved, per-Instance host path, stored at launch and reused by start / restart. |

No new DB columns were added; the two legacy Instance fields are reused.

## Path resolution & naming

All persistence decisions are pure functions in `apps/api/src/persistent_volume.rs`
(no Docker, unit-testable — mirrors `network_qos.rs`).

```
resolved host path = {persistent_storage_path}/{template_name}/{owner_user_id}
```

- `resolve_persistent_host_path(root, template_name, owner_user_id) -> Result<String, PathError>`
  validates: root must be absolute (`/`-prefixed), no `..` segments, no empty
  segments, no injection characters. `persistent_storage_path = NULL` →
  `resolve_persistent_host_path_opt` returns `None` (persistence disabled).
- `persistent_volume_name(resolved_host_path) -> String` → `ow-persist-<FNV-1a 64-bit hex>`.
  A pure function of the host path (lowercase, no `/`, length < 255), so a
  Template rename never changes an existing Instance's volume; reset recomputes
  the same name to locate it.
- `persistent_container_target(remote_type) -> Option<&'static str>` → the mount
  table above (unknown remote types return `None`).

The path is resolved **server-side at launch** and persisted on the Instance
record. The client never supplies a host path (any incoming value is ignored).
Later start / restart reuse the stored path, they do not re-resolve.

## Lifecycle

### Launch (`POST /api/instances`)

The request carries a persistence mode:

```json
{ "template_id": "...", "persistence": "use_persistent" }
```

| Mode | `mount_persistent` | Behaviour |
|---|---|---|
| `use_persistent` | `true` | mount the resolved host dir as a Named Volume |
| `no_persistent` | `false` | no mount, unlimited launches (default at the API level) |
| `reset_persistent` | `true` | wipe existing data, then launch fresh |

**One-persistent-Instance rule:** if a `mount_persistent = true` Instance
already exists for the same `(template_id, owner_id)`, `use_persistent` and
`reset_persistent` return **409**. `no_persistent` is never blocked. Exceptions:
an `error`-state record does not trigger the 409 — the stale record is deleted,
the old volume wiped, and the launch re-prepares fresh.

A Template with `persistent_storage_path = NULL` degrades to `no_persistent`
(no volume, no rejection). An invalid configured root returns 400.

Launch sequence for a persistent mode:

1. Create the DB record first (`instance_repo.launch`).
2. `prepare_persistent_volume(host_path, volume_name)` — **idempotent**:
   - if the Volume declaration already exists (a previous Instance was deleted
     but its data preserved), reuse it as-is — no helper, no wipe, no
     re-population;
   - otherwise run an `alpine --rm` helper container that `mkdir -p`s the host
     dir and `chown 1000:1000`s it (UID 1000 = both `kasm-user` and `ow_user`),
     then `create_local_bind_volume`.
3. `create_container_from_template` mounts the **volume name** at the
   per-remote-type home dir (Docker copy-up populates the built-in home on
   first mount).

`reset_persistent` runs `remove_persistent_volume` (helper empties the host
dir + `docker remove_volume`) **before** the prepare step, so the next
first-mount re-populates the image's built-in home files.

**Failure semantics:** if the helper / volume prep / container creation fails at
any stage, the Instance is marked `error` and the **DB record is kept** (visible
on the dashboard with a `docker_error` field). A later launch can replace a
broken `error` record (see above).

### Stop / Start (`POST /api/instances/{id}/stop` | `/start`)

- Stop removes the Traefik route and stops the container. The volume + data are
  untouched.
- Start re-creates / starts the container. For a persistent Instance it first:
  1. **Backfills** a legacy path: if `mount_persistent = true` but
     `resolved_volume_host_path` is empty, the path is resolved with the current
     rules and persisted (`update_resolved_volume_host_path`);
  2. **Ensures** the Volume declaration: `ensure_persistent_volume` inspects the
     volume and re-declares it via `create_volume` only if Docker lost it (e.g.
     `docker volume prune`) — never re-populates data.

### Delete (`DELETE /api/instances/{id}`)

Delete **preserves persistent data**. It only:

1. removes the Traefik route,
2. stops + removes the container,
3. deletes the DB record.

The host data directory and the Volume declaration are **kept**, so a later
`use_persistent` launch for the same (Template, owner) reuses the exact same
data. The only destructive action is **`reset_persistent`** (or wiping a broken
`error` record). Orphaned-but-reusable host dirs are a deliberate
consequence — see [Out of scope](#out-of-scope).

### Lifecycle summary

| Action | Container | Route | DB record | Host data + Volume |
|---|---|---|---|---|
| Launch `use_persistent` | create | write | create | prepare (reuse if exists) |
| Launch `reset_persistent` | create | write | create | wipe → prepare fresh |
| Stop | stop | remove | keep | keep |
| Start | start/create | write | keep | ensure (re-declare if lost) |
| Delete | stop + remove | remove | delete | **keep** (reusable) |

## Frontend

The dashboard UI is English. Persistence-related behaviour:

- **Template form** (`TemplateResources.svelte`): the field is labelled
  `Persistent Root Directory` with hint `/data/persistent`; it is prefilled on
  create and edit.
- **Launch modal** (`+page.svelte`): a `Data Persistence` select sits next to
  the `Open in` (Current Page / New Tab) selector. It is shown **only when the
  Template sets `persistent_storage_path`** (`showPersistenceSelect`); otherwise
  the launch always uses `no_persistent`.
  - Options top-to-bottom: `Use persistent storage` (**default**) →
    `No persistent storage` → `Reset persistent storage` (bottom).
  - Selecting `Reset persistent storage` triggers a `window.confirm` warning
    ("will erase the existing data and start a fresh environment"); cancelling
    restores the previous selection.
- **Payload** (`template-actions.ts::launchInstance`): sends
  `{ template_id, persistence, mount_persistent }` (no client host path).
- **Badge**: persistent Instance cards show a `persist` badge when
  `mount_persistent` is true.

## Security

- The host path is resolved and validated **server-side**; users cannot mount
  arbitrary host paths into their containers.
- Validation rejects relative roots, `..` traversal, empty segments and
  injection characters.
- The API container never mounts the persistent root and never touches host
  files directly — every host filesystem mutation (mkdir / chown / empty) goes
  through a short-lived `alpine --rm` helper container on the Docker daemon's
  host.

## Known behaviours & out of scope

- **Docker copy-up happens only for a brand-new empty volume.** A non-empty host
  dir (even leftover dotfiles like `.DS_Store`) blocks re-population. The only
  way back to the pristine image home is `reset_persistent`.
- **Image upgrades do not propagate.** Built-in home files are copied once;
  after an image upgrade an existing tenant's home is not overwritten (data is
  deliberately never clobbered).
- **Template renames** keep an existing Instance's path and volume (they are
  stored on the Instance, derived only from the original name).
- **No GC / quotas** for host data dirs; dirs left by deleted Instances are
  intentionally kept for reuse.
- **Single host only** — local bind volumes are not portable across machines
  (no cluster support).
- **No backup / export / download** of persistent data.

## Testing

- **API (482 tests: 158 unit + 324 integration):** pure-function unit tests (`persistent_volume.rs`), mock
  route tests (`instances_mock_test.rs`) covering the 409 rule, reset ordering,
  delete-preserves-volume (`.never()`), error-state launch, restart backfill +
  volume ensure, broken-record replacement; real-Docker integration tests
  (`docker_test.rs`, `docker_lifecycle_test.rs`) covering copy-up population,
  data survival across container recreation, idempotent re-prepare reuse,
  volume re-declaration after loss, and end-to-end server-side path resolution.
  Run via `apps/api/scripts/run_tests.sh` (nextest + Postgres container).
- **Frontend (154 vitest tests, 10 files)** covering the launch payload contract
  (`template-actions.test.ts`), template form, dashboard view, and countdown.
- **Zero-warning policy:** `apps/api/scripts/check.sh` must pass under both the
  default and `docker` feature gates.

## Related docs

- [System Architecture](architecture.md) — instance lifecycle, routing, DB schema
- [API Reference](api-reference.md) — endpoint payloads
- [VNC Authentication](vnc-auth.md)
