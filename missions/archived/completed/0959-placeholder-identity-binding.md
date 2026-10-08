# 0959-placeholder-identity-binding — HSM-bound RouterNodeId for specialized nodes

**Status:** LANDED 2026-08-13 (commit db75a0e7). Drift-closed via audit 2026-08-13.
**Substrate:** RFC-0009 HSM routing + RFC-0870 announce wire
**Parent:** 0870-b envelope adoption (closed) per [[mission-0870-b-envelope-adoption-status]]

## Scope

After wave 2 commits (`ebdbf4cd` + `0a5570bb`), 4 specialized nodes (wallet-node + capability-issuer-node + reputation-anchor-node + identity-resolver-node) emit `RouterAnnouncePayload` with placeholder `RouterNodeId([0u8; 32])`. The `QuotaRouterNode` (in `crates/quota-router-core/src/node/mod.rs`) already derives `node_id` from its bound `IdentityKey`. The 4 specialized nodes need the same pattern.

1. `crates/octo-capability-issuer-node/src/node.rs` — `CapabilityIssuerNodeConfig` gains `Arc<IdentityKey>` slot (analogous to `WalletNodeConfig.identity`). `broadcast_announce` derives `RouterNodeId::from(identity.public_key_bytes())`. Network-key binding for HMAC: add `network_key: [u8; 32]` slot, sign `RouterAnnouncePayload` via `compute_hmac`.
2. `crates/octo-reputation-anchor-node/src/node.rs` — same pattern.
3. `crates/octo-identity-resolver-node/src/node.rs` — same pattern. (Currently uses placeholder because resolvers are read-only and have no signing identity; production wiring injects via a follow-on mission. For this mission, allow `Option<Arc<IdentityKey>>` + `network_key: [u8; 32]` slots; `None` identity falls back to `[0u8; 32]` placeholder with a TODO marker.)
4. `crates/octo-wallet-node/src/node.rs` — already has `Arc<IdentityKey>`; needs `network_key: [u8; 32]` slot added; `broadcast_announce` populates `RouterNodeId` from identity + signs HMAC.

## Test vector discipline

- 4 new TV (one per node): `broadcast_announce` produces a `RouterAnnouncePayload` with `node_id = RouterNodeId::from(identity.public_key_bytes())` AND `verify_hmac(&network_key)` returns `true`.
- 1 new TV (wallet-node): announce with wrong `network_key` rejects at verify.
- 1 new TV: HMAC coverage includes the new `pricing_policy` field (already validated in wave 2 commit `0a5570bb` but per-node re-verification).

## Depends on

- 0870-b envelope adoption (closed; NodeEnvelope wire format landed)
- 0871e-phase5c pricing policy (closed `0a5570bb`; `RouterAnnouncePayload` has pricing_policy field)
- 0009-a HSM routing (closed; `IdentityKey` is HSM-routable)

## Blocks

- Cross-node announce trust (today's `node_id = [0u8; 32]` is non-routable)
- RFC-0010 v1.3 storage trait extension (DID registry needs trusted node_id to bind DID → node mapping)

## Layer direction

- Specialized nodes (Layer C) gain `IdentityKey` + `network_key` slots
- `IdentityKey` is Layer B (octo-wallet substrate); allowed
- `network_key` is per-deployment configuration (Layer D-adjacent); supplied at node construction

## Validation

- `cargo fmt --all -- --check`
- `cargo clippy -p octo-wallet-node -p octo-capability-issuer-node -p octo-reputation-anchor-node -p octo-identity-resolver-node --all-targets -- -D warnings`
- `cargo test --lib` for the 4 specialized nodes

## Cross-references

- [[wave-3-gaps-2026-08-10]] — gap surface context
- [[mission-0871e-phase5c-status]] — sibling sub-mission (introduced the RouterAnnouncePayload extension)
- [[mission-0009-a-impl-status]] — HSM substrate
- [[cipherocto-design-principles]] — Layer C + HSM routing rule
