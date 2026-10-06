# Layer 4 Docker image for quota-router mesh tests.
#
# Builds the CLI binary in a Rust slim image. The entrypoint runs
# `quota-router serve` with configuration via environment variables.
#
# Build from workspace root:
#   docker build -f crates/quota-router-integration-tests/tests/layer4/Dockerfile -t quota-router-test .
#
# Why `rust:1.96-slim` specifically: `rust-toolchain.toml` pins the
# channel to 1.96.0, and the image tag tracks the same minor so the
# container cannot drift onto a different compiler than CI uses. The
# official `rust:` tag is a fixed toolchain, not a rustup shim, so
# `rust-toolchain.toml` is NOT read inside the container — the tag is
# the only thing keeping these two in sync. This file was on
# `rust:1.82-slim`, fourteen minors behind, and
# `cargo build --release -p quota-router-cli` failed there with exit
# 101 while never having been run. The sibling image at
# `crates/octo-cli/tests/e2e_docker/Dockerfile` was already correct;
# the two had simply drifted apart.

FROM rust:1.96-slim AS builder

# Build dependencies, each one learned from a real build failure of this
# file rather than from a checklist.
#
# `cmake`: the repository's `.cargo/config.toml` sets
# `AWS_LC_SYS_CMAKE_BUILDER=1` to work around a GCC bug in aws-lc-sys, so
# the CMake path is the one this build takes. Without the package the
# build script aborts with `Missing dependency: cmake`.
#
# `g++` + `make`: `rust:1.96-slim` ships `cc` and `gcc` but no C++
# compiler and no `make`, and CMake's default generator is Unix Makefiles.
# aws-lc-sys reaches both, so the build dies at
# `failed to find tool "c++": No such file or directory` and then
# `Configuring incomplete, errors occurred!`.
#
# `pkg-config` / `libssl-dev`: cover the openssl-sys family of transitive
# dependencies, which probe for libssl at configure time and fail rather
# than degrading gracefully when it is absent.
#
# The sibling image at `crates/octo-cli/tests/e2e_docker/Dockerfile`
# needs only the first and last of these; this file's dependency graph
# additionally reaches the aws-lc-sys C++ path. Two images built from the
# same repository root, with different build requirements, neither
# previously exercised.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        cmake g++ make pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release -p quota-router-cli

# The runtime stage must be the SAME Debian release as the builder.
# `rust:1.96-slim` is trixie (glibc 2.41); `debian:bookworm-slim` is glibc
# 2.36. Copying a trixie-linked binary onto bookworm produced
#   quota-router: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.38' not
#   found (required by quota-router)
# and every node container exited 1 within seconds of start.
#
# The sibling image at `crates/octo-cli/tests/e2e_docker/Dockerfile` pairs the
# same builder with the same bookworm runtime and works — but only because
# nothing in *its* dependency graph happens to reference a post-2.36 symbol.
# That is luck, not a property, and a dependency bump can turn it into the
# same crash. Both stages are pinned to trixie here so the pairing is
# correct by construction rather than by accident.
FROM debian:trixie-slim

# `ca-certificates` for any future TLS transport adapter.
# `netcat-openbsd` because the compose healthcheck probes the listen port
# with `nc`; `debian:*-slim` ships no netcat, so without it the probe always
# fails and the suite reports "containers not healthy after 60s" no matter
# how healthy the node actually is.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/quota-router /usr/local/bin/quota-router

# Configuration via environment variables:
#   LISTEN_ADDR    - TCP listen address (default: 0.0.0.0:9100)
#   NETWORK_CONFIG - Path to network config TOML (required)
#   MOCK_PROVIDER  - Use mock provider (default: 0)
#   PEERS          - Comma-separated peer endpoints (node_id:addr)
ENV LISTEN_ADDR=0.0.0.0:9100
ENV MOCK_PROVIDER=0

ENTRYPOINT ["sh", "-c", "\
  exec quota-router serve \
    --listen-addr \"$LISTEN_ADDR\" \
    --network-config \"$NETWORK_CONFIG\" \
    ${MOCK_PROVIDER:+--mock-provider} \
    ${PEERS:+--peers \"$PEERS\"} \
"]
