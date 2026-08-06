# 03 — int-bench-smoke：live 端對端驗證

**Track:** integration

**What to build:** The proof that the orchestrator works end-to-end against a real host and a real production compose stack — and that it leaves the host clean. A `--smoke` mode shortens every window so the full pipeline (preflight → compose up → login → six-instance launch → synchronized sampling → teardown → report) completes in seconds; a dedicated smoke script (same shape as the existing network-isolation/dini smoke scripts) asserts the observable side effects and post-teardown state.

**Blocked by:** 02 — bench-orchestrator

**Status:** completed

- [x] `--smoke` mode in the orchestrator: all windows shortened (default ~5s) with the same code path as a full run, so orchestration and teardown are exercised for real in seconds.
- [x] A smoke script (`scripts/benchmark/` …) mirroring `scripts/network_isolation_smoke_test.sh` conventions (`set -euo pipefail`, `FAILED` counter, `log`/`note`/`fail` helpers) that runs `--smoke` and asserts:
  - [x] preflight passes (or fails with the documented fix message);
  - [x] all four platform containers reach `healthy`;
  - [x] all six instances reach `running`;
  - [x] a synchronized short window samples all six containers + host, and report files (CSV + `report.md` with the four tables) are produced;
  - [x] teardown leaves no instance rows, no test templates, no leftover instance containers/networks, and the compose stack down (host restored to pre-run state).
- [x] Smoke passes on both runC and runsc legs of the instance matrix (the script run covers both runtimes by construction).
- [x] The smoke script exits non-zero with a diff on any failed assertion, and never leaves partial state behind on failure (trap-driven cleanup).
