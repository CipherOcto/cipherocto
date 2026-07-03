# Layer 4 Docker image for quota-router mesh tests.
#
# Builds the CLI binary in a Rust slim image. The entrypoint runs
# `quota-router serve` with configuration via environment variables.
#
# Build from workspace root:
#   docker build -f crates/quota-router-integration-tests/tests/layer4/Dockerfile -t quota-router-test .

FROM rust:1.82-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

RUN cargo build --release -p quota-router-cli

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

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
