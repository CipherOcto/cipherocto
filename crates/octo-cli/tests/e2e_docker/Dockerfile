# Cross-container (Layer 4) image for the multi-node `octo` end-to-end suite.
#
# Build from the WORKSPACE ROOT (the build context needs `Cargo.toml`,
# the workspace member manifests, and the member sources):
#
#   docker build \
#     -f crates/octo-cli/tests/e2e_docker/Dockerfile \
#     -t octo-e2e-l4:local .
#
# The harness in `e2e_l4_multinode_docker.rs` builds this itself via
# `docker compose build`, so running the command above by hand is only
# needed when debugging a build failure.
#
# Why build from source inside the image rather than COPYing a host
# binary: the container must exercise the code as committed, and a
# host-built binary silently proves nothing about a clean build. The
# build context is the whole workspace because `octo` is a workspace
# member with path dependencies; `-p octo-cli` restricts compilation
# to that subtree.
#
# Why `rust:1.96-slim` specifically: `rust-toolchain.toml` pins the
# channel to 1.96.0, and the image tag tracks the same minor so the
# container cannot drift onto a different compiler than CI uses.

FROM rust:1.96-slim AS builder

# `cmake` is required, not optional: the repository's `.cargo/config.toml`
# sets `AWS_LC_SYS_CMAKE_BUILDER=1` to work around a GCC bug in aws-lc-sys,
# so the CMake path is the one this build actually takes. Without the
# package installed the dependency fails at build-script time.
# `pkg-config` / `libssl-dev` cover the openssl-sys family of transitive
# dependencies. The built binary does not link libssl (verified with
# `ldd` against the workspace build), but the dependency graph still
# probes for it at configure time on some feature combinations, and a
# missing probe fails the build rather than degrading gracefully.
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# One `COPY` rather than a per-crate list. The build context is the
# repository root, and cargo needs more than `crates/` to resolve the
# graph: two path dependencies (`octo-sync`, `determin`) and one path
# dependency (`octo-transport`) live at the repository root outside the
# `crates/*` member glob, `.cargo/config.toml` switches aws-lc-sys to the
# CMake builder, and `[patch.crates-io]` in the root manifest points at
# the vendored `vendor/glass_pumpkin`. Enumerating those is a list that
# rots — each omission fails during resolution with an error naming a
# path rather than the mistake. The accompanying
# `Dockerfile.dockerignore` drops the 7 GB of non-source directories, so
# a whole-tree copy costs about 100 MB.
COPY . .

# `--locked` makes the container build fail rather than silently
# updating `Cargo.lock`; a suite that proves operator-visible behaviour
# must prove it for the exact dependency set in the tree.
RUN cargo build --release --locked -p octo-cli

FROM debian:bookworm-slim

# `ca-certificates` for any future TLS transport adapter.
# `jq` because the operator guide's pipelines are jq pipelines, and a
# container that cannot run the guide's own commands cannot serve as a
# test oracle for the guide.
# `netcat-openbsd` for the cross-container reachability probe: the
# suite records a real `tcp://` endpoint in one node's peer table and
# proves the peer container answers a TCP connect on the compose
# bridge. It provides both the listener and the client.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        jq \
        netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/octo /usr/local/bin/octo
COPY crates/octo-cli/tests/e2e_docker/entrypoint.sh /usr/local/bin/octo-node-entrypoint

RUN chmod 0755 /usr/local/bin/octo-node-entrypoint

# The node's home. Mounted as a named volume by the compose files so
# the suite can exercise durability across a container restart and
# destroy state with `down -v`.
ENV OCTO_HOME=/octo/home

# `octo` is a one-shot dispatcher with no serve or daemon mode, so a
# "node container" is not a running process to supervise. It is an
# environment that holds one node's `$OCTO_HOME` and whose network
# namespace is a real, separate participant on the compose bridge. The
# entrypoint idles; the suite drives `octo` inside it with
# `docker compose exec`.
ENTRYPOINT ["/usr/local/bin/octo-node-entrypoint"]
