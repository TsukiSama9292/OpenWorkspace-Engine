# ---- Builder ---------------------------------------------------------------
# Pin a toolchain that satisfies edition = "2024" (>= 1.85) and the current
# dependency set; matches the host toolchain (1.97.x) that builds this repo.
# The bookworm variant keeps the builder's glibc compatible with the
# debian:bookworm-slim runtime below.
FROM rust:1.97-slim-bookworm AS builder

# No system packages needed: the TLS stack is rustls (pure Rust), so the build
# needs neither OpenSSL headers nor pkg-config.

WORKDIR /app

# Warm the dependency layer cache: copy only the manifests (including the
# `migration` path dependency) plus a stub source tree, and build once. The
# real sources are copied below, so only this crate recompiles on the next
# step.
COPY Cargo.toml Cargo.lock ./
COPY migration ./migration
RUN mkdir -p src \
    && printf 'fn main() {}\n' > src/main.rs \
    && printf 'pub fn placeholder() {}\n' > src/lib.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release

# ---- Runtime ---------------------------------------------------------------
# Runs as root so the container can enter host/instance network namespaces and
# apply tc/HTB limits (CAP_SYS_ADMIN/NET_ADMIN are granted via compose cap_add,
# Docker's default profile would block setns so compose also disables AppArmor).
FROM debian:bookworm-slim

# ca-certificates: HTTPS for image pulls / registries.
# iproute2 (tc/ip) + util-linux (nsenter): per-instance bandwidth-limit engine.
# TLS is pure Rust (rustls), so no libssl3 needed.
RUN apt-get update && apt-get install -y \
        ca-certificates \
        iproute2 \
        util-linux \
        wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/openworkspace-api .

EXPOSE 3000

HEALTHCHECK --interval=15s --timeout=5s --start-period=30s --retries=5 \
    CMD wget -qO- http://127.0.0.1:3000/health >/dev/null || exit 1

CMD ["./openworkspace-api"]
