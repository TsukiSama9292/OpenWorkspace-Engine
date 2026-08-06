# Production Stack CPU/RAM Benchmark — Analysis Report

**Run date:** 2026-08-06 · **Samples:** 60 per window · **Compose commit:** `275bcf3`

> **Window duration caveat.** The script configures 60 s / 1 sample/s, but the
> container windows are driven by `docker stats --no-stream`, whose round-trip
> takes ~3 s, so the platform / instances / host-after windows each span
> **~200 s of wall time (~3.3 s between samples)**. Only the pure-`/proc`
> host-before window is a true 60 s. Each `docker stats` CPU% is the average
> over its inter-sample interval, so the means are still valid time-averages.

## What this test does

Measures how much CPU and memory the OpenWorkspace production compose stack consumes, both at rest and while running six concurrent instances, using a pure-bash benchmark (`scripts/benchmark/benchmark-prod.sh`). The report header records the test hardware (see [Hardware provenance](#hardware-provenance)). The pipeline, in order:

1. **Preflight** — `runsc` runtime registered, port 80 free, dini template images present.
2. **Host before** — sample the *idle* host (`/proc/stat`, `/proc/meminfo`) for 60 s before anything is started.
3. **Platform idle** — `docker compose build` + `up -d`, wait for all four platform containers (ow-traefik, ow-postgres, ow-web, ow-api) to be ready, then sample them + the host for 60 samples.
4. **Six concurrent instances** — log in to the API as `admin`, create six templates (KasmVNC / ttyd / Jupyter × runC / runsc, with `docker_in_instance`, all `no_persistent`), launch all six through the real API path, wait until every instance reports `running`, then sample all six in one synchronized 60-sample window.
5. **Host after** — sample the host again for 60 s.
6. **Teardown** — delete instances/templates, `docker compose down`. Cleanup runs on failure too (EXIT trap).

Container CPU/memory is read from `docker stats --format '{{json .}}'`; the host from `/proc`. Raw samples are in the CSVs; `report.md` aggregates them into four tables.

## Reproducing

```bash
# one-time: allow passwordless root read of dmidecode ONLY (RAM spec), so the
# benchmark itself runs as your user and report files stay yours (not root:root)
echo 'user ALL=(root) NOPASSWD: /usr/sbin/dmidecode' | sudo tee /etc/sudoers.d/ow-dmidecode
sudo chmod 440 /etc/sudoers.d/ow-dmidecode

# full 60 s run published to docs/analysis/production-benchmark/<datetime>/
pnpm run benchmark:prod

# same pipeline, ad-hoc output (gitignored scripts/benchmark/reports/)
bash scripts/benchmark/benchmark-prod.sh

# short verification run (~minutes instead of ~5+ min)
bash scripts/benchmark/smoke_test.sh
```

`pnpm run benchmark:prod` runs the whole script as your normal user — report files are owned by you, not root — and reaches for the RAM spec only through `sudo -n dmidecode` (the scoped sudoers entry above; no such entry means it silently falls back to the `/proc` figure). Each run creates a new `docs/analysis/production-benchmark/<datetime>/` folder (report.md + CSVs). This file (README.md) is the living analysis; each run folder is the raw evidence for that date. Ad-hoc runs without `--out` land in the gitignored `scripts/benchmark/reports/` dir.

## Hardware provenance

`report.md` header records the test host:

- **CPU** — model, thread count, max clock, read from `lscpu` (falls back to `/proc/cpuinfo`).
- **RAM** — with root access to `dmidecode` available the report shows the **installed** total — this machine: two 16 GB DDR4-2666 modules → `RAM: 32 GB (DDR4-2666, 2 module(s))` — plus module type, configured speed, and module count. Without root access it falls back to the smaller `/proc/meminfo` usable figure (`RAM: 31 GB`). The snapshot below was captured with the scoped sudoers entry active, so it shows the full `32 GB` spec.

## Files

Each run lives in its own folder; the snapshot analyzed in this README is `2026-08-06-235532/`:

| File | Contents |
| --- | --- |
| `report.md` | The four aggregated tables (platform peaks, per-instance peaks, runC-vs-runsc aggregate, host before→after) + provenance |
| `platform.csv` | `epoch,container,cpu_percent,mem_bytes` — 4 containers × 60 samples |
| `instances.csv` | `epoch,container,cpu_percent,mem_bytes` — 6 instances × 60 synchronized samples |
| `host-before.csv` / `host-after.csv` | `epoch,cpu_percent,mem_available_bytes,mem_total_bytes` — 60 samples each |

## Findings

*(Numbers below are from the analyzed snapshot `2026-08-06-235532/`.)*

### 1. The platform itself is cheap

| Container | Peak CPU | Peak mem |
| --- | --- | --- |
| ow-traefik | 0.00% | 19.0 MB |
| ow-postgres | 3.66% | 35.4 MB |
| ow-web | 3.10% | 10.9 MB |
| ow-api | 3.42% | 2.8 MB |

Idle platform cost ≈ **3.7% peak CPU (postgres) and ~68 MB total peak memory** (19.0 + 35.4 + 10.9 + 2.8 MB). This is the baseline every user has to live with.

### 2. KasmVNC is the dominant cost — and runsc multiplies it

| Instance | Runtime | Mean CPU | Peak CPU | Mean mem | Peak mem |
| --- | --- | --- | --- | --- | --- |
| kasmvnc | **runsc** | **13.58%** | 207.77% | **904 MB** | 909 MB |
| kasmvnc | runc | 1.26% | 4.87% | 320 MB | 320 MB |
| ttyd | runsc | 0.96% | 4.94% | 96 MB | 96 MB |
| ttyd | runc | 0.15% | 2.44% | 45 MB | 45 MB |
| jupyter | runsc | 0.88% | 1.76% | 223 MB | 223 MB |
| jupyter | runc | 0.11% | 0.24% | 172 MB | 173 MB |

These are **idle instances** — no client was connected to any viewer. The numbers are the cost of the services just running.

**Memory is flat, not spiky:** for every container `mean_mem ≈ peak_mem` (within a few MB). The memory figures are steady-state cost, not one-off spikes — a runsc-KasmVNC instance *sustains* ~904 MB the whole window. The peak CPU (207.77%) is the `docker stats` first-sample artifact¹ — and it also inflates the reported mean, so the **steady-state mean is ≈ 10.3%** (the 13.58% in the table includes the 207.77 first sample spread over 60; per-container numbers in §3 use the artifact-free means). Even with the artifact sample excluded, runsc-kasmvnc spent **30 of the remaining 59 samples above 5% CPU**, i.e. it is genuinely busy even while idle.

### 3. The runsc cost is real but concentrated in KasmVNC

- **runsc KasmVNC ≈ 8.6× the CPU** of runC (≈10.3% vs ≈1.2% mean, first-sample artifacts removed) and **≈ 2.8× the memory** (904 MB vs 320 MB mean). Idle a KasmVNC instance alone eats about a tenth of one core under runsc.
- ttyd / jupyter feel runsc much less: ~6–8× CPU but on tiny absolute values (0.9% vs 0.15%), memory +2.1× (ttyd 96 vs 45 MB) and +1.3× (jupyter 223 vs 172 MB).
- Conclusion: for `kasmvnc` templates, gVisor sandboxing is expensive; for the terminal/notebook remote types it is close to free. If a deployment is memory- or CPU-bound, runC kasmvnc instances are a viable trade (weaker isolation, ~2.8× less memory).

### 4. Host impact

Six instances running (60-sample window spanning ~200 s): mean CPU **2.65% → 5.14%** (+2.49), available memory **25.0 GB → 23.1 GB** (−1.89 GB). The stack's absolute footprint is modest — the ~904 MB single runsc-KasmVNC instance dominates.

¹ The `docker stats` first sample counts CPU accumulated since container start, dividing by a tiny interval — it inflates the reported peak (207.77%) **and** slightly inflates the mean. The CPU means and ratios quoted in this analysis (§2, §3) exclude that first sample per container; peak values in table 2 are the raw reported numbers.

## Caveats

- **Single run, single machine.** Reproduce before treating numbers as general truth — runtime versions, host CPU/RAM, and Docker/gVisor versions all shift results.
- **Idle instances.** No noVNC client was connected; interactive use will raise KasmVNC CPU beyond these idle numbers.
- **First `docker stats` sample inflates peak CPU** per container (startup artifact); it also slightly inflates the raw mean. §2/§3 means and ratios exclude it; peak values in table 2 are the raw reported numbers.
- **ttyd peak values are single correlated samples.** Both ttyd containers spiked together (4.94% / 2.44%) at the same timestamp — a one-sample system event, not sustained behavior (their means are 0.96% / 0.15%).
- **Container windows spanned ~200 s, not 60 s.** The configured 60 samples are ~3.3 s apart because each `docker stats --no-stream` round-trip takes ~3 s; only host-before was a true 60 s window.
- **Not a load test** — the windows are quiescent operation, not sustained user activity.
