# 02 — bench-orchestrator：benchmark-prod.sh 完整編排

**Track:** orchestration

**What to build:** The runnable end-to-end benchmark script that turns the pure-function library into a complete measurement run: preflight, production compose lifecycle, host-before and platform windows, API-driven six-instance launch with a synchronized sampling window, teardown, and report output. A full run produces the four-table Markdown summary plus per-second CSVs under a timestamped report directory, and leaves the host exactly as it found it.

**Blocked by:** 01 — bench-core

**Status:** completed

- [x] `scripts/benchmark/benchmark-prod.sh` — single bash file, `set -euo pipefail`, sources `benchlib.sh`, depends only on bash + docker + curl + jq (no node/python; enforced by a check that the script invokes none).
- [x] CLI: `--phase N` (1=host-before, 2=platform, 3=instances; default all), `--smoke` (short windows), `--seconds N` (window length, default 60), `--out DIR` (default `scripts/benchmark/reports/bench-<timestamp>/`).
- [x] Preflight checks with clear fail-fast messages: `runsc` registered in `docker info` runtimes; port 80 free; six dini images present locally (auto-build via the repo image build script when missing, then re-check).
- [x] Host-before window: sample `/proc/stat` + `/proc/meminfo` once per second for the window length while nothing is running.
- [x] Compose lifecycle: `docker compose -f docker/openworkspace/docker-compose.yml build` (unmeasured), then `up -d`; wait for all four containers healthy (docker inspect health) with a timeout.
- [x] Platform idle window: one `docker stats --no-stream` call per second (format `{{json .}}`) sampling the four platform containers + host, for the window length.
- [x] API flow: `POST /api/auth/login` capturing the `ow_token` cookie (credentials from `OW_ADMIN_USER`/`OW_ADMIN_PASSWORD`, defaults `admin`/`admin`); create six templates (kasmvnc/ttyd/jupyter × runsc/`docker`) with `docker_in_instance=true` and the matching dini image; launch six instances with `no_persistent`; poll until every instance reports `running` (timeout → abort with cleanup).
- [x] Instance window: after all six report `running`, one synchronized 60s (configurable) window sampling all six containers + host (platform + instances).
- [x] Teardown on success AND on any failure (EXIT trap, idempotent): delete six instances, delete six templates, `docker compose down` (keep volumes — never `down -v`); if cleanup fails, print explicit manual-removal instructions and exit non-zero.
- [x] Report output: per-second CSVs + `report.md` with the four tables (platform peaks, per-instance peaks, runC-vs-runsc aggregate per remote type, host before→after delta) + provenance line (timestamp, `docker info` default runtime, image digests, compose state); peaks also printed to stdout.
- [x] A full run ends with the host restored to its pre-run state (no instances/templates/containers/networks left behind).
