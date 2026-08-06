# 02 — Integration: real cross-process port-lock E2E

**Track:** integration

**What to build:**

Prove the flock arbitration end-to-end against real Docker, across real processes. Two independent API server processes, each with its own database, sharing the same resolved lock directory, concurrently launch real instances through the HTTP API. From the user's perspective: both launches succeed, the two instances are allocated distinct host ports (never the same port to two containers), and after both instances are cleaned up the host has zero residual `runsc` processes.

The test must exercise the actual cross-process path — the one seam the mock harness cannot reach — and the shared-lockdir-by-construction property (both processes derive the same directory for the same UID). It reuses the existing real-Docker E2E infrastructure (live Postgres server, live Docker daemon, real API server) rather than introducing a new harness.

**Blocked by:** 01 — Backend: cross-process host-port locking via flock

**Status:** completed

- [ ] Two independent API server processes, separate databases, one shared lock directory
- [ ] Concurrent launch through each server completes successfully (HTTP 200, status "starting")
- [ ] The two instances are allocated distinct `host_port` values
- [ ] Both instances' containers actually start and bind their ports
- [ ] After both instances are deleted/removed, host residual `runsc` process count is exactly 0
- [ ] The test is repeatable (multiple consecutive runs stay green)
