# Layer 4: Docker Compose Tests

## Prerequisites

- Docker engine running
- `docker compose v2` available
- Ports 19100-19202 available (mapped from container port 9100)
- Built CLI binary (Dockerfile builds it inside the image)

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   node-a     │◄───►│   node-b     │◄───►│   node-c     │
│  :19100      │     │  :19101      │     │  :19202      │
│  mock-a      │     │  mock-b      │     │  mock-c      │
└──────────────┘     └──────────────┘     └──────────────┘
         ▲                    ▲                    ▲
         │      qr-mesh network (bridge)          │
         └────────────────────┴────────────────────┘
```

Each container runs `quota-router serve --mock-provider` with a network config TOML.

## Running

```sh
# 2-node test
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml up -d
cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
  -- --ignored layer4_2node
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml down

# 3-node gossip test
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-3node.yaml up -d
cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
  -- --ignored layer4_3node_gossip
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-3node.yaml down

# Disconnect/heal test
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml up -d
cargo test --manifest-path crates/quota-router-integration-tests/Cargo.toml \
  -- --ignored layer4_disconnect_heal
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml down
```

## Tests

| Test | What |
|------|------|
| `layer4_2node` | Both containers come up healthy, gossip converges |
| `layer4_disconnect_heal` | Stop node B, A continues, restart B, rejoin |
| `layer4_3node_gossip` | 3 nodes gossip, all stay stable |

## Debugging

```sh
# View logs
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml logs node-a

# Enter a container
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml exec node-a sh

# Check health status
docker compose -f crates/quota-router-integration-tests/tests/layer4/compose-2node.yaml ps
```
