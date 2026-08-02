# 08 — Isolation + DNS smoke test

**What to build:** the host-level end-to-end proof, on a real host, that the whole feature holds together — the pattern set by `dini_smoke_test.sh` / `apply_bw_smoke.sh`. The smoke script (new or extended) creates **two** instances on both runtimes and asserts: they cannot reach each other (no ping/connectivity between their unique `/30` IPs), each can reach the internet, each can resolve DNS in-instance (proving the resolv.conf rewrite path works under `runsc`), and each holds its own unique `/30` IP. It cleans up its instances (and their networks) on exit so the host accumulates nothing.

**Blocked by:** 04 — launch-network-wiring; 05 — start-delete-network-wiring; 07 — image-resolv-conf-rewrite.

**Status:** done

- [x] Script runs on both `runc` and `runsc` and exits non-zero on any failed assertion.
- [x] Two launched instances are mutually unreachable while each reaches the internet and resolves DNS.
- [x] Each instance holds a unique `/30` IP (no two instances share a subnet).
- [x] Instance networks are removed when the script tears down its instances (host shows no leftover `ow-*` bridges).
- [x] Output is readable per-step so a host or image change pinpoints the failing stage.

## Comments

- Mirrors the repo precedent: `scripts/dini_smoke_test.sh` and `scripts/apply_bw_smoke.sh`.
- Delivered as `scripts/network_isolation_smoke_test.sh` (host-level, same shape as `dini_smoke_test.sh`). Live run on this host: 11/11 checks PASS on both runtimes (`runc runsc`) — unique `/30` IPs, self-listener + own-gateway routing, raw-IP internet reachability, mutual unreachability both ways, `OW_DNS` resolv.conf rewrite + in-instance `getent hosts example.com`, and a `[5]` assertion that no `ow-smoke-*` bridges survive teardown.
- Two design notes from the live run: (a) the internet probe is a DNS-independent raw-IP TCP connect (`nc -w 8 1.1.1.1 443`) because the busybox isolation instances have no `OW_DNS` rewrite and Docker's embedded resolver (127.0.0.11) does not bind under runsc — name resolution is asserted separately against an OW instance in `[4]`; (b) the isolation/DNS assertions are split across busybox instances (topology, per the repo's own `test_two_networks_mutually_isolated`) and an OW instance (rewrite, per `test_runsc_dns_rewrite_in_instance`), matching the proven integration-test technique.
