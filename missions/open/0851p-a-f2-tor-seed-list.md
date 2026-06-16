# Mission: 0851p-a F2 — Tor-only seed list option

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0851p-a (Networking): Network Bootstrap — §"Future Work" F2

## Summary

Add a `bootstrap_mode = TorOnly` configuration option. In this mode, the bootstrap adapter connects to a `.onion` seed list service (operated by the foundation or a trusted party) over Tor, hiding the gateway's IP address from the seed list operator. This protects the gateway from network-level correlation attacks (e.g., a malicious seed list operator deanonymizing DOT users).

## Design

1. Add `bootstrap_mode: TorOnly | TorWithIpFallback | Direct` to `BootstrapConfig`.
2. When `bootstrap_mode = TorOnly`:
   - The adapter starts a Tor SOCKS5 client (using `arti` crate, the Rust Tor implementation).
   - It connects to a hard-coded `.onion` seed list service (e.g., `dotseeds...onion`).
   - The seed list service responds with the current seed list, also over Tor.
   - There is NO IP fallback; if Tor is down, bootstrap fails.
3. When `bootstrap_mode = TorWithIpFallback`:
   - First attempts Tor; falls back to direct IP if Tor fails.
   - Logs a warning when fallback is used.
4. When `bootstrap_mode = Direct`:
   - Current behavior (direct IP connection to seed list).

The Tor exit node sees only the gateway's Tor circuit, not its real IP. The seed list service sees only the Tor exit node's IP, not the gateway's.

## Acceptance Criteria

- [ ] `BootstrapConfig::bootstrap_mode` field
- [ ] `crates/octo-bootstrap/src/transport/tor.rs` — Tor SOCKS5 client wrapper around `arti`
- [ ] `.onion` seed list service spec in `docs/operations/tor-seed-service.md`
- [ ] Unit tests: TorOnly mode fails without Tor, succeeds with Tor
- [ ] Integration test: gateway IP not visible to seed list operator (verified via network trace)
- [ ] Documentation: how to set up a `.onion` seed list service using `tor` or `arti-server`

## Mitigates

D-NB-7 (network-level deanonymization via seed list operator)

## Deadline

Post-launch
