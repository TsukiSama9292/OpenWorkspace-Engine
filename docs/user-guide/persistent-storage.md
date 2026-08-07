# Persistent Storage

## Overview

Your work in a session — notebooks, terminal history, installed packages,
desktop settings — normally disappears when the container is stopped or
removed. With **persistent storage**, your session's **entire home directory**
is stored on a volume that survives stop, start, and even delete. Only an
explicit **reset** wipes it.

The original design decisions and implementation tickets live in
`.scratch/archive/persistent_storage/` for the technically curious.

## How it works

- Each template can declare a **persistent root directory** on the server. A
  template without one simply never uses persistence.
- When you launch with persistence, the platform creates a dedicated volume for
  you and that template and mounts it as your whole home directory.
- The volume is **named and reusable** — deleting your session does not delete
  the volume. Launching the same template again later picks up exactly where
  you left off.
- The first time a brand-new volume is mounted, the container's built-in home
  files (shell config, VNC/Jupyter settings) are copied into it automatically,
  so the fresh session behaves like a normal new environment — and everything
  you add afterwards persists.

## Launch modes

When you launch a session you choose how your data is handled:

| Mode | What happens |
|------|--------------|
| **Use persistent storage** (default) | Mount your existing volume for this template, reusing your data |
| **No persistent storage** | A throwaway session with no volume |
| **Reset persistent storage** | Wipe your existing data, then start with a fresh environment |

Two rules keep this predictable:

- **One persistent session per template per user.** If a persistent session for
  a template already exists, a second *use*/*reset* launch is refused — use
  the existing session instead. Throwaway sessions are never blocked.
- **Reset is the only destructive action.** Nothing on the server ever
  auto-deletes your data.

## Watch it

Each launch mode in the videos below:

### Use persistent storage

<video controls src="https://github.com/user-attachments/assets/fc8eaa27-0e2e-47d1-9e31-83a0bce57f26" width="100%">
  Your browser does not support the video tag.
</video>

### No persistent storage

<video controls src="https://github.com/user-attachments/assets/b7d80d4b-cca1-4547-9d71-3b42ab85fa7f" width="100%">
  Your browser does not support the video tag.
</video>

### Reset persistent storage

<video controls src="https://github.com/user-attachments/assets/0ad81cd9-5ced-4ae3-bc89-a65f98fe94c3" width="100%">
  Your browser does not support the video tag.
</video>

## Stop, start, delete

| Action | Your data |
|--------|-----------|
| **Stop** | Kept — stop only shuts the container down |
| **Start** | Kept — the same volume is re-mounted |
| **Delete** | Kept — the volume is preserved so a later launch reuses it |
| **Reset** | Wiped, then a fresh volume is created |

## What the dashboard shows

- The launch dialog has a **Data Persistence** selector with the three modes
  above (only shown for templates that support persistence).
- Choosing **Reset** asks you to confirm before erasing anything.
- Persistent session cards carry a small badge so you can tell at a glance
  which sessions have your data.

## Safety

- The server decides where data lives — users can never mount arbitrary server
  paths into their sessions.
- Server-side validation rejects anything that could escape the configured data
  directory.

## Known behaviours and limitations

- **Image upgrades do not propagate.** Built-in home files are copied once.
  After an upgrade, existing data is deliberately never overwritten.
- **Renaming a template** does not move your data or break existing sessions —
  each session remembers its own storage.
- **No automatic cleanup or quotas.** Directories left by deleted sessions are
  kept for reuse on purpose; the admin can remove them manually from the
  **Volumes** tab (double-confirmed).
- **Single server only.** Volumes live on the machine's disk and are not
  portable across machines.
- **No backup or export** of persistent data is built in.

## Related docs

- [System Architecture](architecture.md) — how sessions are created and connected
- [RBAC](rbac.md) — who may manage orphaned volumes
