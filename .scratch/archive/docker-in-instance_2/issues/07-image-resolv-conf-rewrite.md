# 07 — Image entrypoints rewrite resolv.conf from OW_DNS

**What to build:** the image-side half of the DNS contract. On any user-defined bridge, Docker injects `nameserver 127.0.0.11` into the container's `resolv.conf`, and under `runsc` that resolver does not bind — so nothing resolves (the very reason the default bridge was previously chosen). The in-repo image entrypoints for **all six variants** (kasmvnc, ttyd, jupyterlab × plain and `_dini`) rewrite `/etc/resolv.conf` to the `OW_DNS` nameservers before starting their services, and are a no-op when `OW_DNS` is unset. The rewrite runs as root before the entrypoint drops privileges (the `setpriv`/sudo handoff to the app user).

**Blocked by:** None — can start immediately.

**Status:** done

- [x] All six entrypoints apply the rewrite: write `nameserver <resolver>` lines from the comma-separated `OW_DNS` value into `/etc/resolv.conf` before service startup.
- [x] Unset/empty `OW_DNS` leaves `resolv.conf` untouched (backwards-compatible with non-OW usage of the images).
- [x] The rewrite is applied under root before any privilege drop; it does not require extra caps or host changes.
- [x] The rewritten `resolv.conf` is what the in-instance services and (for DinI) the nested `dockerd` inherit.
- [x] Images build cleanly; a live run under `runsc` on a user-defined bridge resolves hostnames after the rewrite (curl + nested `docker pull` when DinI is on).

## Comments

- Contract shared with ticket 03: the API sets `OW_DNS` on every instance; this ticket is the image side that consumes it. Verified live during grilling that the rewrite (unlike `--dns`) fixes resolution under `runsc`.
- Verified after rebuild: all six images (`tsukisama9292/ow-*-ubuntu[:dini]:jammy`) now contain `/usr/local/bin/apply-ow-dns.sh` and the DNS-aware entrypoints (`entrypoint_ow_user_root.sh`, `entrypoint_kasm.sh`, and the three `_dini` entrypoints each invoke it as root before the `setpriv`/sudo drop). Live proof: the ticket-08 smoke `[4]` passes on both runtimes, and `test_runsc_dns_rewrite_in_instance` runs (no longer skips) and passes.
