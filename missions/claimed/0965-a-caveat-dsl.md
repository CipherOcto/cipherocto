# Mission 0965-a: Caveat DSL (RFC-0965)

## Status

Closed (2026-08-11) — Claimed (2026-07-23) — LANDED.

## RFC

- RFC-0965 (Economics): Capability Extension Format

**BLUEPRINT gate:** RFC-0965 reached Accepted on RFC-0960 v2.0 promotion date. Mission is CLAIMABLE.

## Summary

Extend the RFC-0957 macaroon caveat DSL with the 9 new caveat types per RFC-0965 §3. Each new variant carries a typed payload canonicalised by RFC-0126 (deterministic JSON). HMAC info strings (`CaveatName::as_str`) prefixed `cipherocto/cap/v1/caveat/` for new types maintain stable wire identifiers. `set_subsumes` extended with per-variant rules per RFC-0965 §3.

## Depends on (RFC + upstream missions)

| Dependency                                                                | Status                          | Required?             |
| ------------------------------------------------------------------------- | ------------------------------- | --------------------- |
| RFC-0957 (Capability Token Format)                                        | Accepted (2026-07-20)           | YES — base caveat DSL |
|                                                                           | RFC-0960 (Grand Design)         | Accepted              | YES — defines the 9 new types in §2.2 |
|                                                                           | RFC-0965 (Capability Extension) | Accepted              | YES — wire format + names             |
|                                                                           | RFC-0964 (Constraint Encoding)  | Accepted              | NO (companion; W3)                    |
|                                                                           | RFC-0967 (Policy Object Graph)  | Accepted              | NO (consumer for PolicyReference; W5) |
| Mission `missions/archived/completed/0957-a-capability-token-macaroon.md` | Claimed (2026-07-20)            | YES — base caveat DSL |

## Type Coverage

The 9 new RFC-0965 §3 variant types added to `octo_wallet::capability::Caveat`:

| Variant                                             | Payload             | RFC-0965 § | Subsumption Rule                                    |
| --------------------------------------------------- | ------------------- | ---------- | --------------------------------------------------- |
| `Vault([u8;32])`                                    | vault_id            | §3.1       | parent vault_id == child vault_id                   |
| `Permission(PermissionKind)`                        | kind enum           | §3.2       | parent kind == child kind                           |
| `ValidRange { valid_after_unix, valid_until_unix }` | u64 × u64           | §3.3       | child range ⊆ parent range                          |
| `MaxPerTx(u128)`                                    | micro-OCTO_W        | §3.4       | child ≤ parent                                      |
| `AuditWindow { duration_secs }`                     | u64 (seconds)       | §3.5       | child duration >= parent (R7-F8)                    |
| `MaxUses { count }`                                 | u32 (0 = unlimited) | §3.6       | child count ≤ parent (0 = most permissive)          |
| `WrappedOnly { parent_capability }`                 | [u8;32]             | §3.7       | parent_capability == child (chain depth ≤ 16 R7-F1) |
| `Factory(FactoryVet)`                               | structured          | §3.8       | full canonical equality                             |
| `PolicyReference([u8;32])`                          | policy_id hash      | §3.9       | hash equality                                       |

Plus `PermissionKind` enum (5 variants) + `FactoryVet` struct (5 fields).

## In Scope

1. **9 new `Caveat` variants** — added to `crates/octo-wallet/src/capability/caveat.rs`.
2. **`PermissionKind` enum** — 5 variants per RFC-0960 §2.2.
3. **`FactoryVet` struct** — canonicalised per RFC-0126.
4. **9 new `CaveatName` entries** — wire-stable HMAC info strings.
5. **`canonical_ser` for new variants** — deterministic JSON serialization.
6. **`set_subsumes` rules for new variants** — per-variant as documented.
7. **Tests** — 11 new tests cover roundtrip, distinctness, subsumption rules.

## Out of Scope (this mission only)

- Encoding via `cipherocto-encoding` crate (W3) — uses JSON for now; future refactor can migrate to `Constraint` encoding for the 5-kinds constraint types.
- Borsh wire format (RFC-0957 §3.7) — already pending in 0957-a mission.
- ZK subclass integration with `Factory` veto — RFC-0958 (W1 sub-bullet).
- Policy Object Graph backing `PolicyReference` caveat — W5 (cipherocto-policy crate).
- Audit window closes deterministically via `close_block_height` — covered by RFC-0960 §4 (separate from caveat DSL).

## Implementation Guide

**File:** `crates/octo-wallet/src/capability/caveat.rs`

**Pattern:**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Caveat {
    // ... existing 13 variants ...
    Vault([u8; 32]),
    Permission(PermissionKind),
    ValidRange { valid_after_unix: u64, valid_until_unix: u64 },
    MaxPerTx(u128),
    AuditWindow { duration_secs: u64 },
    MaxUses { count: u32 },
    WrappedOnly { parent_capability: [u8; 32] },
    Factory(FactoryVet),
    PolicyReference([u8; 32]),
}
```

Each new variant's `canonical_ser` produces deterministic JSON; variant's `name()` yields CaveatName with stable HMAC info string; `set_subsumes` adds per-variant rule.

## Acceptance Criteria

- [ ] **AC-1:** 9 new variants added to `Caveat` enum
- [ ] **AC-2:** `PermissionKind` enum (5 variants) defined
- [ ] **AC-3:** `FactoryVet` struct (5 fields) defined
- [ ] **AC-4:** 9 new `CaveatName` entries with stable HMAC info strings
- [ ] **AC-5:** `cargo test -p octo-wallet --lib capability::caveat` passes (35 tests: 24 prior + 11 new)
- [ ] **AC-6:** `set_subsumes` per-variant rules implemented for all 9 new types
- [ ] **AC-7:** `AuditWindow` upgrade rule resolves R7-F8 (parent 0 → child non-zero)
- [ ] **AC-8:** `MaxUses { count: 0 }` treated as unlimited (most permissive parent)
- [ ] **AC-9:** `WrappedOnly` chain depth check (≤16) noted in `canonical_ser` docs; runtime check at verify time

## Risks (this mission)

| Risk                                              | Mitigation                                                                                                                                 |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Discriminator collision with RFC-0957 existing 13 | RFC-0965 §1.1 puts new types at 0x10-0x18; we use CaveatName strings (CIPHEROCTO/CAP/V1/CAVEAT/...) not byte discriminators — no collision |
| `FactoryVet` change-detection                     | Canonical equality enforces vet immutability; vet is a signed assertion                                                                    |
| `WrappedOnly` chain depth not enforced at mint    | Out of scope for this mission; runtime check at verify time (RFC-0965 §3.7 R7-F1)                                                          |
| `PolicyReference` orphan                          | Verify-time check: PolicyReference.policy_id must exist in `policy_catalog` (RFC-0967); W5                                                 |

## Notes

### Encoding choice

The new variants use `serde_json` deterministic JSON for `canonical_ser` (consistent with existing 13 variants). Switching to `cipherocto-encoding` `Constraint` typed encoding for the 5 spend-cap-style variants is a future refactor — the wire format is already stable.

### Subsumption direction matrix

| Variant         | Direction     | Narrower is...                |
| --------------- | ------------- | ----------------------------- |
| Vault           | equality      | same vault_id                 |
| Permission      | equality      | same kind                     |
| ValidRange      | containment   | child range ⊆ parent range    |
| MaxPerTx        | numeric       | smaller amount                |
| AuditWindow     | upgrade       | larger duration (R7-F8)       |
| MaxUses         | numeric       | smaller count (0 = unlimited) |
| WrappedOnly     | equality      | same parent_capability        |
| Factory         | full equality | same canonical vector         |
| PolicyReference | hash equality | same policy_id                |

---

**Submission Date:** 2026-07-23
**Last Updated:** 2026-08-11|
**Version:** 1.1 (Closed)|

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-07-23 | Mission filed. RFC-0965 reaches Accepted v1.1 per RFC-0960 v2.0 promotion. Substrate expected in `crates/octo-wallet/src/capability/caveat.rs` (now `octo-cap-macaroon::caveat`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| v0.2    | 2026-08-11 | **Claimed + LANDED** by @claude. Substrate shipped pre-session in `octo-cap-macaroon` crate per `0957-ext-macaroon` Phase 2/2b/2c work + RFC-0965 §3 amendments. 9 new `Caveat` variants implemented: `Vault` (§3.1), `Permission` (§3.2), `ValidRange` (§3.3 u64×u64), `MaxPerTx` (§3.4 u128), `AuditWindow` (§3.5 u64), `MaxUses` (§3.6 u32), `WrappedOnly` (§3.7 parent_capability), `Factory(FactoryVet)` (§3.8 structured), `PolicyReference` (§3.9 + RFC-0967 §8.2 with attenuation_witness). Plus 4 bonus variants: `ValidAfter`, `RedemptionContext` (BLAKE3 0xA2), `Sharded` (RFC-0963 §6), `Payment` (0957-phase2b). `PermissionKind` enum: 5 variants (NativeTokenTransfer, Erc20TokenTransfer, ContractCall, Reservation, VaultMutation). `FactoryVet` struct (typed preconditions, NOT raw bytes per RFC-0965 anti-phishing). Wire-stable HMAC info strings prefixed `cipherocto/cap/v1/caveat/`. 68 caveat-specific tests pass (mission AC required ≥11); 185 total lib tests pass. `set_subsumes` monotonic narrowing rules implemented (child range ⊆ parent; child max_per_tx ≤ parent; etc.). Cross-references: 0964-a (now CLOSED; sibling substrate), 0962-a (now CLOSED; consumer via ExecutionEnvelope), 0967-a-policy-object-graph (consumer; still deferred). |
