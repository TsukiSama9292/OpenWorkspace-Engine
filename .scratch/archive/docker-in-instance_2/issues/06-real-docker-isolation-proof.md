# 06 — Real-Docker isolation proof

**What to build:** automated proof, against a real Docker daemon, that the topology actually isolates tenants. Feature-gated integration tests that (1) create a `/30` network and attach a container through the template path, asserting the container gets exactly one usable IP (`x.x.x.2`, gateway `.1`); (2) create **two** separate `/30` networks each with a container and assert they are mutually unreachable (no ping either direction) while each still reaches the internet and its own gateway; and (3) exercise the resolv.conf rewrite live under `runsc`: after the rewrite, in-instance name resolution works (and a nested `docker pull` resolves when DinI is on).

**Blocked by:** 02 — docker-network-seam; 03 — container-network-attachment.

**Status:** done

- [x] Feature-gated test: a container on a `/30` network holds the single expected IP `.2`; no other address in the block is a live endpoint.
- [x] Feature-gated test: two `/30` networks with one container each cannot reach each other (ping fails both directions) while both reach the internet and their gateway.
- [x] Feature-gated test (runsc, docker-in-instance): the `OW_DNS`-driven resolv.conf rewrite restores name resolution in the instance (in-instance `curl` to a public host resolves; nested `docker pull` resolves).
- [x] The full API suite stays green with the `docker` feature (`cargo nextest run --features docker`), zero warnings.

## Comments

- Isolation here is Docker's own bridge semantics — per-network iptables FORWARD drop. Verified live during grilling: two `/30` networks ping-ping fail both ways, internet works on each.
- Verified live after the image rebuild (`docker/template_images/build.sh`, which ships `apply-ow-dns.sh`): `docker_test` binary 37/37 PASS with 0 skips — `test_network_single_usable_ip`, `test_two_networks_mutually_isolated`, and `test_runsc_dns_rewrite_in_instance` all exercised against the real daemon on this host (the runsc DNS test previously skipped because the local image lacked the script).
