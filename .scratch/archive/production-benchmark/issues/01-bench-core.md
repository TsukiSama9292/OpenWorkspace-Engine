# 01 — bench-core：純函數測量核心 library + 單元測試

**Track:** core

**What to build:** The pure-function measurement library that all other pieces consume. Given raw inputs (proc snapshots, docker stats JSON lines, per-second rows), it produces parsed, aggregated, reportable output — with zero side effects, so it is unit-testable from fixture data with no Docker running. This is the foundation the orchestrator ticket sources in; it lands independently verifiable (the unit test runner is its own green gate).

**Blocked by:** None — can start immediately.

**Status:** completed

- [x] A sourceable pure-function bash library (bash + awk + sort coreutils only) with no side effects: every function reads from arguments/stdin and writes to stdout or named files, so any function is callable from a unit test.
- [x] Host CPU sampling: given two consecutive `/proc/stat` aggregate snapshots, compute utilization % from the idle/total tick arithmetic (delta of busy ticks over delta of total ticks).
- [x] Host memory sampling: given a `/proc/meminfo` fixture, parse `MemTotal` and `MemAvailable` into bytes.
- [x] `docker stats` parsing: given `--format '{{json .}}'` output lines, parse `{container_name, cpu%, mem_bytes}` — including trimming the trailing `%`, parsing human-size memory (`1.2GiB / 4GiB` → bytes), and skipping header/invalid lines.
- [x] Per-second CSV record formatting: timestamp + metric + value (and per-container wide rows) consistent with the report layout.
- [x] Aggregation: peak (max) and mean (arithmetic) over a window of per-second rows, for both CPU% and memory bytes; correct for a single-sample window.
- [x] Markdown table emission for the four report tables: platform peaks, per-instance peaks, runC-vs-runsc aggregate per remote type, host before→after delta.
- [x] A fixture-driven bash unit-test runner (`tests/`) that sources the library and asserts expected outputs against bundled fixtures; exits non-zero with a diff on any failure. Green = the library gate.
- [x] Zero runtime deps beyond bash + GNU coreutils in the library itself (curl/jq/docker belong to the orchestrator, not here).
