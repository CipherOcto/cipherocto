# Mission: 0851p-a — Trust UX (web-of-trust visualization)

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0851p-a (Networking): Network Bootstrap — §"Future Work"

## Summary

A `dot-trust graph` CLI command that renders the web-of-trust graph (the `signed_by` relationships between peers) as ASCII art or DOT (Graphviz) format for operator inspection. This is an operator/UX tool, not a security feature — it helps operators visualize who trusts whom, identify isolated clusters, and find "celebrity" peers (high in-degree = many peers trust them).

## Design

1. New binary `octo-trust` (or subcommand `octo-cli trust graph`).
2. Reads the local trust store (peers and their `signed_by` chains).
3. Computes the trust graph.
4. Renders as:
   - ASCII art for terminal display (limited to ~50 nodes)
   - DOT (Graphviz) format for large graphs (pipe to `dot -Tpng` for visualization)
5. Options:
   - `--root <peer_id>` — show only the trust tree rooted at `peer_id`
   - `--depth <N>` — limit tree depth (default 3)
   - `--format ascii|dot` — output format (default ascii)

## Acceptance Criteria

- [ ] `crates/octo-bootstrap-cli/src/trust_graph.rs` — graph renderer
- [ ] `dot-trust graph` CLI command
- [ ] `--root`, `--depth`, `--format` options
- [ ] Unit tests: ASCII output for 5-node graph, DOT output for 100-node graph
- [ ] Documentation: example output in `docs/operations/trust-graph.md`

## Mitigates

Operational visibility (not a security issue).

## Deadline

Post-launch
