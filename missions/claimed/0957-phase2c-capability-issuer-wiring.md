# 0957-phase2c — Capability Issuer Node wiring

**Status:** LANDED 2026-08-13 (commit b19fe57f). Drift-closed via audit 2026-08-13.
**Substrate:** RFC-0871 §Roles and Authorities + RFC-0957-A1 §Algorithms / §HolderRecord State Machine
**Closes:** 0871d Phase 3 MVP gaps (IdentityKey + HolderRegistry slots; macaroon substrate; `CAPABILITY_LOOKUP` payload kind) + `crates/octo-capability-issuer-node/src/handlers/mod.rs:82` dead-code

## Scope

`CapabilityIssuerNode` currently ships Phase 3 MVP stubs for
`CAPABILITY_ISSUE` + `CAPABILITY_REVOKE` with no macaroon substrate
wiring. Mission 0957-phase2c upgrades the node to a production-shaped
state machine:

1. `CapabilityIssuerNodeConfig` gains `Arc<IdentityKey>` +
   `Arc<dyn HolderRegistry>` slots.
2. `IssueHandler::new()` takes `&IdentityKey` + `&dyn HolderRegistry`;
   the handler mints via `CapabilityToken::mint(...)` + registers via
   `HolderRegistry::insert(record)` (RFC-0957-A1 §HolderRecord State
   Machine transition `Absent → Active`).
3. `RevokeHandler::new()` takes `&dyn HolderRegistry` + `&dyn Clock`;
   the handler calls `HolderRegistry::revoke(cap_root_hash, clock)`
   (RFC-0957-A1 §State Machine transition `Active → Revoked`).
4. New `CAPABILITY_LOOKUP` payload kind + `CapabilityLookupHandler`
   returning the `HolderRecord` for a given 32-byte `cap_root_hash`
   PK (or `None` for absent / revoked records).

## Implementation

1. `crates/octo-protocol/src/payload_kind.rs`: add `CAPABILITY_LOOKUP`
   constant (`0x0009:0005:0000:0000:0000:0000:0000:0003`) + extend
   `CAPABILITY_PAYLOAD_KINDS` slice.
2. `crates/octo-capability-issuer-node/Cargo.toml`: add
   `quota-router-storage` dep (Layer B substrate; the
   `HolderRegistry` trait lives there per `verify.rs` /
   `gateway_authenticator.rs` precedent).
3. `crates/octo-capability-issuer-node/src/handlers/issue.rs`:
   - `IssueHandler<'a>` takes `&'a IdentityKey` + `&'a dyn HolderRegistry`.
   - `handle()`: validate canonical DID; mint via
     `CapabilityToken::mint(&capability, identity, &holder_did, &[])`;
     derive 32-byte `cap_root_hash` via
     `octo_cap_macaroon::compute_capability_id(&token.macaroon)`;
     insert `HolderRecord::from_capability(token, holder_did)` via
     `registry.insert(record)`; emit the real macaroon wire form
     (`octo_cap_macaroon::wire::serialize_wire(&token)`) as the
     response payload (replaces the Phase 3 MVP
     `CIPHEROCTO_ISSUE_V1:*` placeholder).
4. `crates/octo-capability-issuer-node/src/handlers/revoke.rs`:
   - `RevokeHandler<'a>` takes `&'a dyn HolderRegistry` + `&'a dyn Clock`.
   - `handle()`: call `registry.revoke(&cap_root_hash, clock)` where
     `cap_root_hash` is derived from the 16-byte `token_id` (first 16
     bytes of the macaroon capability id) — same path the wallet
     node uses for `HolderRegistry::lookup(cap_root_hash)`.
   - Emit a `RevocationEvent` to the audit log (RFC-0957-A1 §State
     Machine — `Active → Revoked` transition).
5. `crates/octo-capability-issuer-node/src/handlers/lookup.rs` (new):
   - `CapabilityLookupHandler<'a>` takes `&'a dyn HolderRegistry`.
   - Request: `CapabilityLookupRequest { cap_root_hash: [u8; 32] }`.
   - Response: `Option<HolderRecord>` serialized via borsh.
6. `crates/octo-capability-issuer-node/src/handlers/mod.rs`: add
   re-exports for the new lookup handler.
7. `crates/octo-capability-issuer-node/src/node.rs`:
   - `CapabilityIssuerNodeConfig` gains `identity: Arc<IdentityKey>`
     - `registry: Arc<dyn HolderRegistry>` + `clock: Arc<dyn Clock>`.
   - `handle_envelope` routes the three payload kinds to their
     handlers + threads the new config fields.

## Test vector discipline

- All 11 existing `octo-capability-issuer-node` tests pass updated to
  supply the new config slots (IdentityKey + HolderRegistry +
  Clock). The 2 mint + revoke routing tests assert the substrate
  was called (cap_root_hash registered, then revoked) — not just
  that a placeholder response came back.
- 6 new TV:
  - TV1 — `IssueHandler::handle()` with substrate produces a macaroon
    wire form (3-segment base64url-no-pad) as response payload, and
    `HolderRegistry::lookup(cap_root_hash)` returns `Some(record)`
    post-mint.
  - TV2 — `RevokeHandler::handle()` calls `registry.revoke`;
    `HolderRegistry::lookup(cap_root_hash)` post-revoke returns a
    record with `revoked_at_millis_unix = Some(_)`.
  - TV3 — `CapabilityLookupHandler::handle()` returns the
    HolderRecord for an existing cap_root_hash and `None` for an
    absent one.
  - TV4 — `handle_envelope` routes `CAPABILITY_LOOKUP` correctly.
  - TV5 — Non-canonical DID rejected at issue time
    (Phase 3 MVP behaviour preserved).
  - TV6 — `CAPABILITY_LOOKUP` payload kind UUID matches the
    documented `0x0009:0005:0000:0000:0000:0000:0000:0003` slot.

## Depends on

- 0957-phase2a (landed commit `90306f45`) — real wire form substrate
- 0957-phase2b (landed commit `5cda2eb7`) — PaymentCaveat in caveat
  chain (initial caveats for minted tokens)
- `HolderRegistry` production-readiness (already landed in
  `quota-router-storage` per `verify.rs` /
  `gateway_authenticator.rs` / `audit_log.rs` / `federation.rs`
  precedent)

## Blocks

- 0871d Phase 3 MVP gaps closure
- Production capability-issuer end-to-end (mint + verify + revoke + lookup)
- 0871e-phase5b (atomic drain needs cap-issuer-side revocation hook)

## Layer direction

- `octo-capability-issuer-node` (Layer C) → `quota-router-storage`
  (Layer B) → `octo-wallet` (Layer B) ✓
- No reverse dependencies introduced.

## Validation

- `cargo fmt -p octo-capability-issuer-node -p octo-protocol --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib -p octo-capability-issuer-node` (existing 11 +
  6 new = 17)
- `cargo test --lib -p octo-protocol` (UUID byte-exact preserved)
- `cargo test --lib -p octo-wallet` (no regression on the
  HolderRegistry consumers)

## Cross-references

- `[[0957-phase2-unblocker-map]]` — phase2c sub-mission
- `[[cipherocto-design-principles]]` — Layer C specialized-node pattern
- `[[mission-gap-closure-priorities-2026-08-10]]` — Wave 1 plan
- `[[rfc-0957-A1]]` — HolderRegistry / HolderRecord State Machine
