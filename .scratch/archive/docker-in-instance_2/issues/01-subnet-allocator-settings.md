# 01 — Subnet allocator + instance-network settings

**What to build:** the pure-logic foundation for per-instance isolated networks. Two new settings — `OW_INSTANCE_NET_BASE` (base CIDR for the per-instance `/30` pool, default `10.200.0.0/16`) and `OW_INSTANCE_DNS` (comma-separated resolvers, default `8.8.8.8,1.1.1.1`) — parsed and validated with the existing settings pattern. A pure allocator that, given the set of subnets already in use and the base CIDR, returns the lowest free `/30` (network `.0`, gateway `.1`, instance `.2`), returning `None` when the pool is exhausted. A deterministic network-name derivation from the instance's stable id (e.g. `ow-<instance-id>`), so any caller can recompute the name with no state.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] `OW_INSTANCE_NET_BASE` and `OW_INSTANCE_DNS` exist on the settings struct with the documented defaults, and unit tests cover defaults, env overrides, and invalid-input errors (mirroring the existing `host_gateway_ip` / `host_port_*` tests).
- [x] The subnet allocator is a pure function: given a used-subnet set and the base range it returns the lowest free `/30` in order, skips used blocks, wraps/exhausts correctly, and returns `None` when the pool is full; boundary cases at the top of the base range are covered.
- [x] The allocator treats the base range as a set of aligned `/30` blocks and derives gateway `.1` / instance `.2` from the block's network address.
- [x] Network-name derivation is deterministic and stable for a given instance id.
- [x] No DB schema change; nothing is persisted here.

## Comments

- Part of per-instance `/30` isolation spec (`.scratch/archive/docker-in-instance_2/spec.md`). Default base `10.200.0.0/16` was chosen against the live host ranges (docker0 `172.17.0.0/16`, LAN `10.122.78.0/24`, wireguard `10.0.255.0/24`, tailscale `100.64.0.0/10`).
- Verified in code: `settings.rs` (`instance_net_base`/`instance_dns` at :13-14, defaults at :37-39/:72-73, unit tests `test_instance_net_base_invalid`/`test_instance_net_base_misaligned_rejected`/defaults/overrides at :388-402); `NetBase::parse`/`lowest_free_subnet`/`lowest_free_subnet_from`/`spread_block_offset`/`gateway_ip`/`instance_ip`/`network_name` in `instance_net.rs` with unit tests.
