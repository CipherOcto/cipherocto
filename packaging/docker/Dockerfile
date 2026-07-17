# Multi-stage Dockerfile for `octo-whatsapp`.
# Phase 5 Part G — production deployment artifact.
#
# Usage:
#   docker build -f packaging/docker/Dockerfile -t octo-whatsapp:phase5 .
#   docker run -d --name octo-wa -v /var/lib/octo/whatsapp:/var/lib/octo/whatsapp \
#                          -v /var/log/octo/whatsapp:/var/log/octo/whatsapp \
#                          -p 127.0.0.1:7778:7778 \
#                          octo-whatsapp:phase5 daemon --name default
#
# The HEALTHCHECK probes the daemon's HTTP `/health` (default
# loopback 127.0.0.1:7778) and exits 0 only on 200.

# ---- Stage 1: builder ----
FROM rust:1.83-bookworm AS builder

WORKDIR /build

# Cache layer: only rebuild when Cargo.toml changes.
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Build the release binary (single-binary, statically linked where possible).
RUN cargo build --release --bin octo-whatsapp -p octo-whatsapp

# ---- Stage 2: minimal runtime ----
FROM debian:bookworm-slim

# Runtime dependencies: ca-certificates (TLS), libssl3 (HTTPS), tini (PID 1).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 tini \
 && rm -rf /var/lib/apt/lists/*

# Create the unprivileged runtime user (UID 1000, no login).
RUN groupadd -g 1000 octo \
 && useradd -u 1000 -g octo \
        -d /var/lib/octo/whatsapp \
        -s /usr/sbin/nologin \
        -M \
        octo

# Copy the release binary from the builder stage.
COPY --from=builder /build/target/release/octo-whatsapp /usr/local/bin/octo-whatsapp

# Persistent state + logs.
RUN mkdir -p /var/lib/octo/whatsapp /var/log/octo/whatsapp \
 && chown -R octo:octo /var/lib/octo/whatsapp /var/log/octo/whatsapp

USER octo

# Expose the observability HTTP surface (loopback only by default;
# operators wanting external access must reverse-proxy + auth at the
# proxy layer).
EXPOSE 7778

# Volume declarations for persistent state.
VOLUME ["/var/lib/octo/whatsapp", "/var/log/octo/whatsapp"]

# Health check: probe the HTTP `/health` endpoint via a CLI subcommand
# that exits 0 only when the daemon reports `is_live = true`.
#  --interval=30s  : probe every 30 seconds
#  --timeout=5s    : bail after 5 seconds
#  --start-period=60s : allow 60 seconds for the daemon to boot
#  --retries=3     : 3 consecutive failures trip the unhealthy state
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
  CMD ["/usr/local/bin/octo-whatsapp", "health", "--probe"] || exit 1

# Use tini as PID 1 to forward signals + reap zombies.
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/octo-whatsapp"]
CMD ["daemon", "--name", "default"]