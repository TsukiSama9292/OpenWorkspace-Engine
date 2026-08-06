# 01 — Port-Pool Networking for Instances

**What to build:** Every instance publishes its single service port (KasmVNC / ttyd / Jupyter) to the host Docker-bridge gateway IP at an API-allocated host port from the pool; Traefik routes to `https://host.docker.internal:<host_port>`; the API persists the allocation in the database and never needs a container IP. End-to-end: launch an instance and the platform URL keeps working through Traefik, the service is reachable at the gateway IP, and the API reports the allocated port.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] Host gateway IP and the port pool range come from environment settings (defaults `172.17.0.1` and `10000–20000`, range-exclusive upper bound).
- [x] A pure allocation function returns the lowest unused port for a given used-port set and range, and `None` when the pool is exhausted.
- [x] Instances have a nullable `host_port` with a UNIQUE index; existing rows default to `NULL` via migration.
- [x] Before committing an allocation, a best-effort TCP probe against the gateway IP skips ports already listening.
- [x] Launch allocates a host port, persists it, publishes the instance's service to `<gateway>:<host_port>`, and writes the Traefik route targeting `https://host.docker.internal:<host_port>` with the `kasm-insecure` transport preserved.
- [x] If container creation fails on a port conflict, the next free port is retried before the launch fails.
- [x] Instance API responses include `host_port`.
- [x] Prod Traefik can reach the published port (host-gateway mapping present); Dev Traefik already has it.
- [x] Seams 1–4 tests (pure allocation, mocked launch flow, real-Docker port binding, DB) are green.
