# 02 — Integration: real cross-process subnet-lock E2E

**Track:** integration

**What to build:**

Prove the subnet flock arbitration end-to-end against real Docker, across real processes, by extending the existing two-process flock E2E. Two independent API server processes, each with its own database, sharing the same resolved lock directory, concurrently launch real instances through the HTTP API. From the user's perspective: both launches succeed, the two instances are allocated distinct `/30` subnets (never the same block to two networks) as well as distinct host ports, and the shared-lockdir-by-construction property is exercised under real contention.

This reuses the existing real-Docker E2E infrastructure (live Postgres server, live Docker daemon, real API servers) rather than introducing a new harness. The existing assertions on distinct host ports and clean host state stay; the new assertion lands alongside them.

**Blocked by:** 01 — Backend: cross-process /30 subnet locking via flock

**Status:** ready-for-agent

- [x] Two independent API server processes, separate databases, one shared lock directory
- [x] Concurrent launch through each server completes successfully (HTTP 200, status "starting")
- [x] The two instances are allocated distinct host ports
- [x] The two instances are allocated distinct `/30` subnets (each network carries a different block from the base range)
- [x] Both instances' containers actually start
- [x] The test is repeatable (multiple consecutive runs stay green)
