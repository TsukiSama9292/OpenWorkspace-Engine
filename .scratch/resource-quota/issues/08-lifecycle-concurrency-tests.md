# 08 — Lifecycle accounting & concurrency verification tests

**What to build:** proof that reservations track the instance lifecycle and that the locks really prevent overshoot. A dedicated integration-test sweep asserts the spec's lifecycle rules — pausing keeps count + personal quota + dedicated pool locked; stopping and deleting release the count, the personal quota, and (for dedicated instances) the host pool immediately; and concurrency — two simultaneous launches from the same user at the instance limit, and two simultaneous launches from different users at the global limit, each allow exactly one to succeed. From a developer's perspective: after this ticket, the enforcement is verified end-to-end against a real Postgres, not just unit-checked.

**Blocked by:** 06 — Wire pre-flight into launch & start.

**Status:** ready-for-agent

- [ ] Paused instances remain counted and keep their resource reservations (personal quota and, for dedicated, the host pool).
- [ ] Stopped and deleted instances release the count and personal quota; deleted/stopped dedicated instances release the host dedicated pool.
- [ ] Two concurrent launches by the same user at the per-user instance limit → exactly one succeeds, the other gets the `user_instance` violation.
- [ ] Two concurrent launches by different users at the global instance limit → exactly one succeeds, the other gets the `host_instance` violation.
- [ ] Assertions read the counter/sum queries (external behavior), not internal helper calls.
