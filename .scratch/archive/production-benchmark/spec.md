Status: completed

# Production Stack CPU/RAM Benchmark Script

## Problem Statement

The platform's production Docker Compose stack (Traefik + Postgres + web + Rust API) and its per-instance containers have **no resource-consumption telemetry**: the API exposes no `/metrics` endpoint, there is no Prometheus/cAdvisor integration, and `/health` returns only `{"status":"ok"}`. An operator who wants to answer "how much CPU/RAM does this platform actually use, and what does each instance cost?" must today run ad-hoc `docker stats` / `free` by hand, with no reproducible, scriptable, reportable method.

The team also needs to answer specific operational questions that require structured measurement:

- What does the production stack consume when **idle** (right after startup), per container and host-wide?
- What is the **per-instance** cost of a KasmVNC / ttyd / Jupyter workspace, and does the **container runtime** (runC vs runsc/gVisor) materially change that cost?
- What is the **host before→after** delta from "nothing running" to "platform + six concurrent instances"?

The measurement must be scriptable (repeatable, runnable on any host), must export a report for sharing, and — because the operator may run it on a resource-constrained box — must add minimal sampling overhead. The team wants it implemented with the lowest possible dependency footprint: a pure `bash` script, no Node.js, no Python.

## Solution

A single pure-bash benchmark script (`scripts/benchmark/benchmark-prod.sh`) that runs the production compose stack, measures it with one sample per second, concurrently launches six instances (three remote types × two container runtimes, all with Docker-in-Instance enabled), records host state before/after, and writes both per-second CSV data and a Markdown summary report to `scripts/benchmark/reports/`. The script cleans up after itself (instances, templates, compose stack) so the host returns to its pre-run state. A `--smoke` mode runs the same flow with short sampling windows for fast CI-style verification; pure sampling/parsing/aggregation logic is factored into a sourceable function library so it is unit-testable with fixture data without Docker.

Run flow (single script, optional `--phase` filtering):

1. **Preflight** — verify `runsc` runtime is registered in Docker, port 80 is free (no dev-stack conflict), and the six dini template images are present (building them via the repo's image build script if missing).
2. **Host before** — sample host CPU/memory once per second for 60s via `/proc/stat` + `/proc/meminfo` while nothing is running.
3. **Platform idle** — `docker compose build` (not measured), then `up -d`; wait for all four containers healthy; sample the four platform containers (`docker stats --no-stream`) plus host once per second for 60s.
4. **Six concurrent instances** — log into the API (admin cookie), create six templates (3 remote types × runC/runsc, all `docker_in_instance=true`, all dini images), launch all six instances concurrently with `no_persistent`, poll until every instance reports `status=running`, then sample all six containers plus host (platform + instances) once per second for a single synchronized 60s window.
5. **Cleanup & report** — delete the six instances, delete the six templates, `docker compose down`; write per-second CSVs and the four-table Markdown summary.

Report tables: (1) platform container peaks, (2) per-instance peaks, (3) runC vs runsc aggregate comparison per remote type, (4) host before→after delta.

## User Stories

1. As an **operator**, I want to run one command that measures the production stack's CPU/RAM from startup, so that I get reproducible numbers instead of ad-hoc `docker stats` sessions.
2. As an **operator**, I want the platform measured while idle (nothing else running) so that the numbers reflect the platform's true baseline cost, uncontaminated by instance load.
3. As an **operator**, I want per-second samples of each platform container (Traefik, Postgres, web, API) for the measurement window, so that I can see startup settling behavior and per-service footprint.
4. As an **operator**, I want the platform measurement to start only after all four containers report healthy, so that the window does not include build/startup noise.
5. As an **operator**, I want each sampled metric to include a peak value, so that I can size the host for the worst case, not just the average.
6. As an **operator**, I want to measure the per-instance cost of a KasmVNC, a ttyd, and a Jupyter workspace, so that I can budget resources per workspace type.
7. As an **operator**, I want each remote type measured under both runC and runsc, so that I can quantify the gVisor sandbox overhead and decide which runtime to default to.
8. As an **operator**, I want all six instances launched concurrently and sampled in one synchronized 60-second window, so that the runC-vs-runsc comparison measures the same host conditions for every instance.
9. As an **operator**, I want the instances launched through the real API path (template creation → instance launch), so that the measurement reflects real port/subnet allocation, run_config handling, and DinI wiring rather than a synthetic `docker run`.
10. As an **operator**, I want each instance to start its sample window only after it reports `running`, so that per-instance numbers reflect steady-state cost, not the launch spike.
11. As an **operator**, I want a host before→after comparison (idle host vs platform + six instances) so that I can see the total cost of running the full system on one box.
12. As an **operator**, I want the report saved as both raw per-second CSV and a Markdown summary, so that I can re-analyze the raw data and share a readable summary.
13. As an **operator**, I want the Markdown summary to include a runC vs runsc aggregate comparison table per remote type, so that the runtime-overhead question is answered at a glance.
14. As an **operator**, I want the script to clean up after itself (delete instances, templates, and the compose stack), so that running a benchmark never leaves test resources behind or alters host state.
15. As an **operator**, I want the script to fail fast with a clear message when preflight checks fail (runsc not registered, port 80 busy, images missing), so that I can fix the environment instead of watching a broken run.
16. As an **operator**, I want to run only part of the pipeline (e.g. just the host-before + platform phases, or just the instance phase), so that I can iterate on one stage without a full 5-minute run.
17. As an **operator**, I want the sample interval and window length configurable, so that I can take longer/short measurements without editing the script.
18. As an **operator**, I want a `--smoke` mode that runs the whole flow with short windows, so that I can verify the script end-to-end in seconds before committing to a full run.
19. As an **operator**, I want the run to be resumable/safe if a phase fails (e.g. cleanup still runs), so that a mid-run failure cannot leave the host with orphaned instances or a running stack.
20. As a **developer**, I want the sampling, parsing, and aggregation logic to be pure functions unit-testable with fixture data without Docker, so that the measurement math is verified independently of a live host.
21. As a **developer**, I want the script to depend only on bash + docker + curl + jq, so that it runs on any Linux box without installing Node.js or Python.
22. As a **developer**, I want the script to read admin credentials from environment variables with documented defaults matching the compose defaults, so that it works out of the box on a fresh checkout.
23. As a **developer**, I want the script to use the repo's existing image-build path when images are missing, so that template images and the benchmark never drift apart.
24. As an **infrastructure admin**, I want the benchmark to be able to run against a production-like compose stack without needing the dev stack, so that I can measure the real deployment shape.
25. As an **infrastructure admin**, I want the report files timestamped and grouped under a dedicated reports directory, so that historical runs can be compared without name collisions.

## Implementation Decisions

### 1. Artifact layout

- **`scripts/benchmark/benchmark-prod.sh`** — the orchestration entry point: preflight → host-before → compose up → platform window → six instances → cleanup → report assembly. Single bash file, `set -euo pipefail`, `#!/usr/bin/env bash`.
- **`scripts/benchmark/benchlib.sh`** — a sourceable pure-function library: host sampling (`/proc/stat` + `/proc/meminfo` parsing), `docker stats --no-stream` output parsing, per-second CSV record formatting, peak/mean aggregation, Markdown table emission. No side effects; every function reads from stdin/arguments and writes to stdout or named files.
- **`scripts/benchmark/reports/`** — timestamped output directory for the run; gitignored (transient artifacts).
- **`scripts/benchmark/tests/`** — fixture-driven unit tests for the pure functions (bash, no Docker).
- CLI: `--phase N` (1=host-before, 2=platform, 3=instances, default all), `--smoke`, `--seconds N` (window length, default 60), `--out DIR`.

### 2. External dependencies

`bash`, `docker` (CLI), `curl`, `jq` — nothing else. No Node.js, no Python. GNU coreutils (`awk`, `sort`, `date`, `seq`) are assumed present as part of any reasonable Linux base. This is an explicit acceptance criterion; the script must not invoke `node`, `python`, or any other runtime.

### 3. Measurement mechanism

- **Host**: parse `/proc/stat` (aggregate CPU ticks; compute utilization delta between consecutive samples) and `/proc/meminfo` (`MemTotal`, `MemAvailable`) once per second. This is the lowest-overhead method and needs no external tools.
- **Platform containers + instances**: one `docker stats --no-stream --format '{{json .}}'` call per tick (a single call returns every running container), parsed by the library into per-container CPU% / memory bytes rows. Overhead is one short Docker API round-trip per second — acceptable and well under the measured quantities.
- **Windows**: host-before 60s (default, configurable); platform 60s starting after all four compose containers report healthy; instance window 60s starting once all six instances report `running`, sampling all six in one synchronized window.

### 4. Compose lifecycle

- `docker compose -f docker/openworkspace/docker-compose.yml build` runs **before** the platform window and is **not** measured (build cost is a developer action, not runtime cost).
- Then `docker compose -f docker/openworkspace/docker-compose.yml up -d`; readiness = all four service containers ready. The compose file defines healthchecks for postgres and web only; traefik and api have none, so a container is "ready" when it reports `healthy`, or — for a container without a healthcheck — when it is `running` (the script waits with a timeout).
- Teardown: `docker compose -f docker/openworkspace/docker-compose.yml down` (keeps volumes; `down -v` is intentionally NOT used — the operator's data must survive). Instance volumes are avoided entirely by launching with `no_persistent`.

### 5. API flow

- **Login**: `POST /api/auth/login` with `{username, password}`; capture the `ow_token` cookie from the response headers and reuse it on subsequent requests (cookie-based auth).
- **Templates**: six `POST /api/templates` calls (3 remote types × 2 runtimes), each with `docker_in_instance: true` and the matching dini image, `cores`/`memory` left at template defaults unless overridden, `remote_type` matching the image family, `container_runtime` = `runsc` or `docker` (the latter maps to the host's default runC runtime via the API's existing `runtime_to_host_config` mapping). Template names derived from runtime+type so they are identifiable in the report.
- **Launch**: six `POST /api/instances` calls with `persistence: no_persistent`; then poll `GET /api/instances` (or the per-instance endpoint) until every instance reports `status=running`, with a timeout; on timeout, abort with a clear error and still run cleanup.
- **Credentials**: `OW_ADMIN_USER` / `OW_ADMIN_PASSWORD` env vars, defaulting to `admin` / `admin` (matching the compose `ADMIN_PASSWORD` default).
- The API base URL is `http://localhost` (via Traefik, the same path a real user uses).

### 6. Instance matrix

| remote_type | image (dini) | runtime value | runtime meaning |
|---|---|---|---|
| kasmvnc | `tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy` | `runsc` / `docker` | gVisor / runC |
| ttyd | `tsukisama9292/ow-ttyd-ubuntu-dini:jammy` | `runsc` / `docker` | gVisor / runC |
| jupyter | `tsukisama9292/ow-jupyter-ubuntu-dini:jammy` | `runsc` / `docker` | gVisor / runC |

Six instances total. `docker_in_instance=true` for all six (the "use dini template setting" requirement). Runtime values `runsc` vs `docker` are the only differentiator between the two instances of each remote type.

### 7. Preflight checks

- `docker info` output contains `runsc` in its runtimes list (gVisor runtime registered). If missing, print the registration command (`pnpm run init` / `scripts/docker-runtime-gvisor.sh`) and exit.
- Port 80 is free (nothing bound) — guards against a dev-stack or other traefik already running. Checked via `ss -ltn` or `/proc/net/tcp`.
- The six dini images exist locally; if any is missing, run the repo's image build path (`docker/template_images/build.sh`) to produce them, then re-check.

### 8. Reporting

- **CSV**: one file per measurement stream (`host-before.csv`, `platform.csv`, `instances.csv`, `host-after.csv`), per-tick wide format. Host rows are `timestamp,cpu_percent,mem_available_bytes,mem_total_bytes`; container rows are `timestamp,container,cpu_percent,mem_bytes`. Raw per-second data preserved for re-analysis.
- **Markdown summary** with four tables:
  1. Platform container peaks (per container: peak CPU%, peak memory bytes).
  2. Per-instance peaks (instance container, remote type, runtime, peak CPU%, peak memory bytes).
  3. runC vs runsc aggregate comparison per remote type (mean/peak CPU%, mean/peak memory).
  4. Host before→after delta (CPU% and memory totals at both points).
- Output to `scripts/benchmark/reports/bench-<timestamp>/` (gitignored) (CSV + `report.md`); the script prints the peak values to stdout and the report path.
- Peak = max over the window; mean = arithmetic mean over the window.
- Provenance (per Further Notes): timestamp, `docker info` default runtime, the repo commit of the compose file, and image digests of the (deduplicated) template images.

### 9. Failure handling

- The whole script runs under `set -euo pipefail`. A phase failure triggers the same cleanup path as normal completion (instances → templates → compose down), so the host is left clean. If cleanup itself fails, the script prints explicit manual-removal instructions (instance ids, template ids, compose command) instead of silently succeeding.
- Preflight failures exit immediately with the fix instruction (no partial state created).
- A `trap` on `EXIT` runs the cleanup (idempotent: deleting already-deleted instances/templates is tolerated).

### 10. `--smoke` mode

Same flow, but windows shortened (5 seconds) and the instance-launch readiness poll bounded at 60s (real container startup takes real time; a smoke run should not lengthen it, only bound it). Purpose: verify orchestration + teardown in minutes. Preflight checks still run. A fixture-passthrough flag was considered and dropped — the pure functions are unit-tested with fixtures directly (Seam 1), and the smoke mode exercises a live host.

## Testing Decisions

What makes a good test: assert **external behavior** of the pure functions (given this `/proc/stat` snapshot and this `/proc/meminfo`, utilization is X; given this `docker stats` JSON line, the parsed row is Y; given these per-second rows, the peak/mean and Markdown table are Z) and assert the **orchestration side effects** of `--smoke` (compose comes up, instances reach `running`, cleanup leaves no instances/templates/containers/networks behind). Never test internal plumbing of the bash library.

### Seam 1 — Pure-function unit tests (primary, no Docker)

A bash test runner (`scripts/benchmark/tests/run.sh`) sources `benchlib.sh` and asserts against bundled fixture inputs:

- **Host CPU utilization**: given two consecutive `/proc/stat` snapshots (fixture files), the computed utilization percentage matches the expected delta (including the idle/total tick arithmetic).
- **Host memory**: given a `/proc/meminfo` fixture, `MemTotal` and `MemAvailable` parse to the expected bytes.
- **`docker stats` parsing**: given a `--format '{{json .}}'` line fixture, the parsed `{cpu%, mem_bytes, container_name}` row is correct (including the `mem_usage` "1.2GiB / 4GiB" human-parsing to bytes and the trailing-percent trimming).
- **Aggregation**: given per-second rows, peak and mean (CPU% and memory) are computed correctly, including a window with a single sample.
- **Markdown emission**: given a peak table, the emitted Markdown table matches the expected shape (header, alignment, rows).
- The runner exits non-zero on any assertion failure and prints a diff; the fixture set lives in the tests directory.

### Seam 2 — `--smoke` orchestration test (real host)

Implemented as `scripts/benchmark/smoke_test.sh` (mirroring `scripts/network_isolation_smoke_test.sh` prior art): runs the orchestrator with `--smoke` in the background and asserts, live, that the four platform containers reach ready, all six instance containers reach `running` (covering both runtimes by construction), the report files (CSVs + `report.md` with the four tables) are produced with all six instance containers sampled, and teardown leaves no instance rows / no test templates (verified by bringing the stack back up and querying the API), no leftover `bench-*` containers, no `ow-*` instance networks, and the compose stack down. Exits non-zero with a diff on any failed assertion; trap-driven cleanup covers failure paths. Note: the smoke test names its cleanup function `smoke_cleanup` (not `cleanup`) because sourcing `benchmark-prod.sh` redefines shared helper names in the same shell.

### Seam 3 — No Rust/API test changes

The API is exercised only through its existing HTTP surface (login, templates, instances); its behavior is already covered by the Rust integration suite. This spec adds no Rust tests. (If a measurement reveals an API bug, that is tracked separately.)

### Prior art

- `scripts/network_isolation_smoke_test.sh` / `scripts/dini_smoke_test.sh` / `apps/api/scripts/apply_bw_smoke.sh` — the repo's bash smoke-test shape (live host verification, teardown assertions, `FAILED`/`log`/`note`/`fail` helpers).
- `apps/api/src/host_port.rs` / `instance_net.rs` — pure-function-with-fixtures testing precedent (adapts to bash: deterministic inputs → expected outputs).
- The dini image build flow in `docker/template_images/build.sh` is reused as-is for the image-missing preflight.

## Out of Scope

- **Adding `/metrics`, Prometheus, cAdvisor, or any telemetry to the API** — the benchmark measures from outside; adding platform telemetry is a separate feature.
- **Measuring build cost** (compose build is explicitly excluded from measurement).
- **Measuring instance launch latency** — the window starts only after `running`, so launch spikes are excluded by design.
- **Network bandwidth / I/O measurement** — CPU/RAM only, per the requirement.
- **Persistent-volume instances** — all six launch `no_persistent`; persistent storage cost is out of scope.
- **gVisor sandbox internals** (how runsc uses memory) — only the observable per-container CPU/RAM delta.
- **Windows/darwin support** — Linux only (the platform already requires Linux + Docker).
- **Auto-benchmark scheduling / cron integration** — manual runs only in v1.
- **Comparison against the dev stack** — the dev compose is only a conflict to guard against, not a benchmark target.

## Further Notes

- **Runtime value for runC**: the API's `runtime_to_host_config` maps `""` and `"docker"` to the host's default runtime (runC). The benchmark uses `container_runtime: "docker"` for the runC leg of the matrix. If the host's Docker default runtime were ever changed, the runC leg would follow it; the report records the actual `docker info` default runtime for provenance.
- **Admin credentials**: defaults `admin`/`admin` mirror the compose default. Operators on hardened hosts must set `OW_ADMIN_USER`/`OW_ADMIN_PASSWORD` (the compose `.env` `ADMIN_PASSWORD` likewise overrides).
- **`docker compose down` vs `down -v`**: teardown intentionally keeps volumes; the benchmark uses `no_persistent` instances so nothing test-created persists. Operator data is never touched.
- **Port-80 preflight rationale**: the production compose publishes 80; a running dev stack (or any traefik) would bind it. The check prevents a confused measurement and a silently-wrong report.
- **Traefik dashboard** (127.0.0.1:8080) is not measured; it is a localhost-only debug surface.
- The report records provenance: timestamp, `docker info` default runtime, image digests of the six template images, and the compose file commit/status — so a later comparison knows exactly what was measured.
