# 03 — Instance attaches to its isolated network + OW_DNS env

**What to build:** the container-level wiring that puts an instance on its own `/30`. The instance container configuration gains two new fields: a `network_name` used as the container's `network_mode` (replacing the hardcoded `"bridge"`), and an `instance_dns` value emitted as the `OW_DNS` environment variable on every instance container. The image entrypoints read `OW_DNS` to rewrite `/etc/resolv.conf` (see ticket 07); the API's job here is only to pass the value through.

**Blocked by:** 01 — subnet-allocator-settings (network-name derivation).

**Status:** done

- [x] The container config carries `network_name`; the created container's `network_mode` is that name, never the hardcoded default bridge.
- [x] The container config carries `instance_dns`; the created container's environment includes `OW_DNS=<resolvers>` on every instance (DinI and non-DinI alike).
- [x] Existing behavior preserved when the values are absent (helpers, non-instance containers, and the persistent-volume `alpine` helpers remain on the default bridge).
- [x] Mocked tests assert the container is created with the expected `network_mode` and `OW_DNS` env for each remote type.
- [x] Feature-gated real-Docker test: a container created through the template path with a real `/30` network lands on it and receives the unique instance IP (gateway `.1`, container `.2`).
- [x] Zero-warning policy preserved.

## Comments

- The `dns` field already on the container config (from template `run_config`) is the Docker-level resolver hint and is unaffected; `OW_DNS` is the image-level rewrite contract and both coexist.
- Verified in code: `ContainerConfig` carries `network_name` + `instance_dns`, set from `ensure_instance_network` and `state.settings.instance_dns` in `build_and_create_container` (`instances.rs:920-948`). Real-Docker `test_container_attaches_to_instance_network_with_ow_dns` and the launch-path `test_launch_lands_container_on_dedicated_30_network` pass.
