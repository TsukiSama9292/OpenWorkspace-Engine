# 03 — Docker launch threading

**What to build:** When a workspace instance is launched from a template, the `container_runtime` value from that template (falling back to the system-wide env var default) is applied to the Docker container's `HostConfig.runtime`. A runsc-annotated container actually runs under gVisor; a docker-annotated one uses the daemon's default runtime.

**Blocked by:** 02 — API expose & repository persistence

**Status:** completed

- [ ] In instance launch code (`instances.rs`), compute the effective runtime: if template's `container_runtime` is empty/`"docker"` → use `settings.container_runtime`, otherwise use template's value
- [ ] Thread the resolved runtime into `ContainerConfig.runtime` when constructing it from the template
- [ ] In `create_container_from_template()` (`docker.rs`), apply `runtime_to_host_config(config.runtime.as_deref().unwrap_or("docker"))` to `HostConfig.runtime`
- [ ] Feature-gated Docker integration test (`docker_test.rs`): create a container with `runtime` set to `Some("runsc")` and verify `HostConfig.runtime` is `"runsc"` in inspect output
- [ ] Mock-based test in `instances_mock_test.rs`: verify the mock Docker service receives the expected runtime value
