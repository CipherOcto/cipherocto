# Mission: 0957-f-v2-bundle-cutover — V2 Envelope as Primary `response_payload`

## Status

open (2026-08-11).

**Substrate:** Mission `0957-f-v2-bundle-consumer-migration` LANDED (commit `e0c4ad62`)
introduced `CapabilityBundleV2Envelope` + `CIPHEROCTO_V2_BUNDLE_PREFIX` as a
**sidecar** in `HandlerOutput.v2_envelope_bytes`. The primary `response_payload`
still carries V1 wire form (mint: 3-segment base64url; issue: borsh
`IssueResponse` wrapper). All 3 producer sites now build V2 envelopes
alongside. This mission **flips V2 envelope to be the primary
`response_payload`** and removes the V1 sidecar — the V2 envelope is the
authoritative wire form going forward.

## Summary

Two producer handlers still emit V1 wire form on `response_payload`:

| Handler | Current `response_payload` (= V1) | New `response_payload` (= V2 envelope) |
|---------|-----------------------------------|--------------------------------------|
| `MintHandler` (octo-wallet-node) | `minted_wire.into_bytes()` (canonical macaroon wire — 3 base64url segments) | `v2_envelope.canonical_ser()` (16-byte prefix + borsh V2 bundle) |
| `IssueHandler` (octo-capability-issuer-node) | `borsh::to_vec(&IssueResponse { holder_did, token_id, v2_envelope_bytes: sidecar })` | `v2_envelope.canonical_ser()` |

After cutover:
- `HandlerOutput.v2_envelope_bytes` field DELETED (no sidecar; primary = V2 envelope).
- `HandlerOutput::with_v2_envelope()` method DELETED.
- `IssueResponse` struct DELETED (was a V1 wire wrapper; token_id + holder_did recoverable from `CapabilityTokenV2` via `channel_id` + `audience_did`).
- The existing `minted_wire` (V1 macaroon wire form) is still produced internally for internal cacoon_id derivation but is NO LONGER the response payload — it stays as input substrate to the V2 envelope construction (specifically into `holder_record_bytes`).

## Scope (2 producer sites + 2 HandlerOutput modules)

### Site 1: `octo-wallet-node/src/handlers/mint.rs`

**Before:**
```rust
Ok(HandlerOutput::response(
    minted_wire.into_bytes(),
    octo_protocol::payload_kind::WALLET_MINT_CAPABILITY,
)
.with_note(note)
.with_v2_envelope(v2_envelope_bytes))
```

**After:**
```rust
Ok(HandlerOutput::response(
    v2_envelope_bytes.clone(),
    octo_protocol::payload_kind::WALLET_MINT_CAPABILITY,
)
.with_note(note))
```

`minted_wire` continues to be produced inline (line 127) — it remains
the input to `holder_record_bytes` (capability_id derivation) and
spend ledger seeding. The V1 wire form is no longer the response
payload; it stays as internal substrate.

### Site 2: `octo-capability-issuer-node/src/handlers/issue.rs`

**Before:**
```rust
let response = IssueResponse {
    holder_did: req.holder_did.clone(),
    token_id,
    v2_envelope_bytes: v2_envelope_bytes.clone(),
};
let payload = borsh::to_vec(&response).map_err(...)?;
Ok(HandlerOutput::response(payload, CAPABILITY_ISSUE)
    .with_note(...)
    .with_v2_envelope(v2_envelope_bytes))
```

**After:**
```rust
Ok(HandlerOutput::response(
    v2_envelope_bytes.clone(),
    octo_protocol::payload_kind::CAPABILITY_ISSUE,
)
.with_note(note))
```

`IssueResponse` struct deleted from `handlers/issue.rs`.
`IssueResponse` re-export deleted from `handlers/mod.rs`.

### Site 3: `octo-wallet-node/src/handlers/mod.rs` + `octo-capability-issuer-node/src/handlers/mod.rs`

Delete `v2_envelope_bytes: Option<Vec<u8>>` field from `HandlerOutput`.
Delete `with_v2_envelope()` method. Update doc comments to reflect
"V2 envelope is the primary `response_payload` (mission `0957-f-v2-bundle-cutover`)".

## Why no consumer migration is needed

The candidate description named `cross_node_delivery.rs` +
`redemption_subgraph.rs` as prerequisite consumers. Recon
confirmed those tests operate on `MarketDeliveryEnvelope`
(RFC-0959-A1 dual-mode market delivery), NOT on wallet mint
or capability issue response payloads. **No production consumer
currently decodes the V1 wire form on `response_payload`** — the
V1 wire form was the canonical macaroon wire (RFC-0957 §3.7)
used internally (`serialize_wire`/`deserialize_wire` in
`octo_cap_macaroon::wire`) but no inbound/outbound envelope
consumer reads it from the response payload field. The cutover
is therefore producer-only.

(Internal `serialize_wire`/`deserialize_wire` API is preserved
for substrate use — they are NOT the response payload after
cutover.)

## Test updates

### `octo-wallet-node/src/handlers/mint.rs`

| Test | Update |
|------|--------|
| `handle_emits_three_segment_wire_form` (TV1) | Rewrite: assert `response_payload` starts with `CIPHEROCTO_V2_BUNDLE_PREFIX` + decodes to `CapabilityBundleV2Envelope` |
| `wire_roundtrip_preserves_caveats_and_root_id_shape` (TV2) | Rewrite: extract macaroon from V2 envelope via `holder_record_bytes` → assert caveats/root_id |
| `holder_sig_verifies_after_wire_roundtrip` (TV3) | Rewrite: same path as TV2 + verify_holder_sig |
| `wire_only_cap_root_hash_matches_mint_time_derivation` (TV4) | Rewrite: extract macaroon from V2 envelope → derive both hashes → assert match |
| `handle_rejects_non_canonical_did_before_mint` (TV5) | UNCHANGED (negative path) |
| `handle_mints_with_payment_caveat_as_initial_caveat` (TV6) | Rewrite: extract macaroon from V2 envelope → assert Caveat::Payment |
| `handle_mints_without_payment_caveat_has_empty_chain` (TV7) | Rewrite: extract macaroon from V2 envelope → assert empty caveats |
| `handle_with_ledger_seeds_payment_caveat_budget` (TV8) | Rewrite: extract macaroon from V2 envelope → assert ledger balance |
| `handle_surfaces_v2_envelope_alongside_wire_form` (TV9) | Rename to `handle_emits_v2_envelope_as_primary_payload`; drop `response_payload.is_some()` check; drop `v2_envelope_bytes` (deleted field) |

### `octo-capability-issuer-node/src/handlers/issue.rs`

| Test | Update |
|------|--------|
| `issue_request_borsh_round_trip` | UNCHANGED |
| `handle_rejects_invalid_did` | UNCHANGED |
| `handle_rejects_legacy_bare_did` | UNCHANGED |
| `handle_returns_derived_token_id` | Rewrite: decode V2 envelope → assert `token_v2.channel_id == expected` |
| `handle_token_id_varies_with_capability` | Rewrite: same decode + assert token_v2.channel_id varies |
| `issue_response_borsh_round_trip` | DELETE (no more `IssueResponse`) |
| `issue_emits_v2_root_bundle_envelope` | Rename to `handle_emits_v2_envelope_as_primary_payload`; assert `response_payload` IS the envelope (has prefix + decodes) |

## Acceptance Criteria

- [ ] `mint.rs::handle` sets `response_payload = v2_envelope_bytes.clone()` (V2 envelope canonical_ser).
- [ ] `issue.rs::handle` sets `response_payload = v2_envelope_bytes.clone()` (V2 envelope canonical_ser).
- [ ] `HandlerOutput.v2_envelope_bytes` field DELETED in both crate-local mods.
- [ ] `HandlerOutput::with_v2_envelope()` method DELETED in both crate-local mods.
- [ ] `IssueResponse` struct DELETED; re-export removed.
- [ ] All 9 mint tests + 5 issue tests pass (rewritten per above).
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes across `octo-wallet-node` + `octo-capability-issuer-node` + `octo-cap-macaroon`.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test --lib` green for both crates; legacy `wire_v2_roundtrip.rs` (octo-wallet) still passes (tests internal `serialize_wire`/`deserialize_wire`, not the response payload).

## Layer Discipline

- `octo-protocol` (Layer A) — UNCHANGED. V2 envelope is Layer 4 substrate.
- `octo-cap-macaroon` (Layer 4) — UNCHANGED. `CapabilityBundleV2Envelope` + `CIPHEROCTO_V2_BUNDLE_PREFIX` API stable.
- `octo-wallet-node` (Layer C) — HandlerOutput extension removed; mint.rs producer flip.
- `octo-capability-issuer-node` (Layer C) — HandlerOutput extension removed; issue.rs producer flip.

No new production deps. No trait changes. No schema migrations.
No cyclic dep risk.

## Cross-references

- RFC-0009 v1.2 §Implementation Phases Commit 5 — atomic consumer adoption rule
- Mission `0957-f-v2-bundle` (commit `b6bc190b`) — V2 wire substrate
- Mission `0957-f-v2-bundle-consumer-migration` (commit `e0c4ad62`) — V2 envelope sidecar
- `crates/octo-cap-macaroon/src/bundle_v2.rs` — V2 envelope API
- `crates/octo-wallet-node/src/handlers/mint.rs` — Site 1
- `crates/octo-capability-issuer-node/src/handlers/issue.rs` — Site 2

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-11 | open   | Filed after consumer-migration landed; recon confirmed downstream consumers (cross_node_delivery.rs, redemption_subgraph.rs) operate on MarketDeliveryEnvelope not on wallet mint/issue payload — cutover is producer-only |
