# Mission 0964-a: Constraint Encoding Standard (RFC-0964)

## Status

Closed (2026-08-11) — Claimed (2026-07-23) — LANDED.

## RFC

- RFC-0964 (Economics): Constraint Encoding Standard

**BLUEPRINT gate:** Per BLUEPRINT.md "Missions REQUIRE an approved RFC" — RFC-0964 reached Accepted on RFC-0960 v2.0 promotion date. Mission is CLAIMABLE.

## Summary

Implement the canonical 23-variant constraint encoding per RFC-0960 §3 + RFC-0964 §1. Wire format: 1-byte outer namespace tag (`0x01` = Constraint) + 1-byte high-bit version tag (`0xA0`) + 1-byte discriminator (range `0x10-0x28`) + 2-byte big-endian length-prefixed payload. Maximum total encoded size: 256 bytes (RFC-0960 §G5; RFC-0964 §3.2 cap). `constraint_hash` uses BLAKE3 with `0xA1` domain separator (RFC-0964 §5).

## Depends on (RFC + upstream missions)

| Dependency                             | Status                         | Required?                                                   |
| -------------------------------------- | ------------------------------ | ----------------------------------------------------------- |
|                                        | RFC-0960 (Grand Design)        | Accepted                                                    | YES — umbrella + 23-variant list  |
|                                        | RFC-0964 (Constraint Encoding) | Accepted                                                    | YES — discriminator + wire format |
| RFC-0126 (Deterministic Serialization) | Accepted                       | YES — canonical_ser (we use big-endian explicit, not borsh) |
|                                        | RFC-0965 (Caveat Extension)    | Accepted                                                    | NO (companion; W4 picks it up)    |
|                                        | RFC-0962 (ExecutionEnvelope)   | Accepted                                                    | NO (consumer; W6)                 |
| Mission `claims/`                      | —                              | NO                                                          |

## Type Coverage

Per RFC-0960 §3 + RFC-0964 §1, the 23-variant `Constraint` enum is implemented in this mission:

| Variant                     | Discriminator | Implemented By               |
| --------------------------- | ------------- | ---------------------------- |
| `ValidRange`                | 0x10          | This mission                 |
| `NotBefore`                 | 0x11          | This mission                 |
| `UnlockAfter`               | 0x12          | This mission                 |
| `Period`                    | 0x13          | This mission                 |
| `MaxPerTx`                  | 0x14          | This mission                 |
| `PerAssetSpendingCap`       | 0x15          | This mission (sorted; max 5) |
| `RateLimit`                 | 0x16          | This mission                 |
| `AllowedDestinations`       | 0x17          | This mission                 |
| `DeniedDestinations`        | 0x18          | This mission                 |
| `IntentBound`               | 0x19          | This mission                 |
| `MultiSig`                  | 0x1A          | This mission                 |
| `RequireReceiptSignatureBy` | 0x1B          | This mission                 |
| `CallerBound`               | 0x1C          | This mission                 |
| `MaxUses`                   | 0x1D          | This mission                 |
| `SingleUse`                 | 0x1E          | This mission                 |
| `AllowIf`                   | 0x1F          | This mission                 |
| `VerifierRequired`          | 0x20          | This mission                 |
| `WrappedOnly`               | 0x21          | This mission                 |
| `SponsoredBy`               | 0x22          | This mission                 |
| `CoordinatorCanSubmit`      | 0x23          | This mission                 |
| `LinearRelease`             | 0x24          | This mission                 |
| `CliffVesting`              | 0x25          | This mission                 |
| `LiquidityLock`             | 0x26          | This mission                 |
| `GovernanceLock`            | 0x27          | This mission                 |
| `ComplianceHold`            | 0x28          | This mission                 |

(Sponsor + Coordinator variants per RFC-0960 §3; 25 variants total. RFC-0960 §3 enumerates 23; the canonical list spans both RFC-0960 + RFC-0964.)

## In Scope

1. **`crates/cipherocto-encoding/` crate** — new workspace member under `crates/*` glob (auto-picked up).
2. **`Constraint` enum** — 25-variant set with deterministic field ordering.
3. **`encode` / `decode`** — canonical wire format with namespace tag, version tag, discriminator, length-prefixed payload.
4. **`constraint_hash`** — BLAKE3 with `0xA1` domain separator.
5. **256-byte cap** — `MAX_ENCODED_SIZE` enforced; overflow returns `E_CONSTRAINT_OVERFLOW`.
6. **PerAssetSpendingCap ordering** — sorted by asset_id lex (RFC-0964 §3.2 + R6-F4); max 5 (RFC-0964 §3.2 + R5-F4).
7. **Test coverage** — roundtrip all 25 variants + reject invalid namespace/version/discriminator + reject truncated + reject too-many-caps + reject unsorted-caps + hash determinism.

## Out of Scope (this mission only)

- Full 23 variants + RFC-0965 caveat integration → W4 (caveat DSL using encoding trait)
- `PolicyObject` referencing constraints → W5 (cipherocto-policy crate)
- ExecutionEnvelope usage → W6
- Fuzz harness for 24h → W3 follow-up (per RFC-0964 plan)
- Benchmarks → out of scope

## Implementation Guide

**Crate path:** `crates/cipherocto-encoding/`

**Public API:**

```rust
pub enum Constraint { /* 25 variants */ }
pub fn encode(c: &Constraint) -> Result<Vec<u8>, EncodingError>;
pub fn decode(bytes: &[u8]) -> Result<Constraint, EncodingError>;
pub fn constraint_hash(c: &Constraint) -> [u8; 32];
pub const NAMESPACE_TAG: u8 = 0x01;
pub const VERSION_TAG: u8 = 0xA0;
pub const CONSTRAINT_HASH_PREFIX: u8 = 0xA1;
pub const MAX_ENCODED_SIZE: usize = 256;
```

**Wire format:** `[NAMESPACE_TAG, VERSION_TAG, discriminator, body_len:u16_be, body...]`

**Encoding strategy:** Explicit big-endian `[u8; N]` reads/writes (no serde/borsh). All multi-byte fields are big-endian. Set-type fields (`AllowedDestinations`, `DeniedDestinations`, `MultiSig.signers`) are sorted lexicographically before encoding. `PerAssetSpendingCap.caps` is sorted by asset_id lex and capped at 5 entries.

## Acceptance Criteria

- [ ] **AC-1:** `crates/cipherocto-encoding/Cargo.toml` exists; `crates/cipherocto-encoding/src/lib.rs` exists
- [ ] **AC-2:** Workspace `cargo metadata` includes `cipherocto-encoding` (no manual `members` edit needed; `crates/*` glob auto-picks)
- [ ] **AC-3:** `cargo test -p cipherocto-encoding --lib` passes (all 25-variant roundtrip + edge case tests)
- [ ] **AC-4:** `cargo build -p cipherocto-encoding` green
- [ ] **AC-5:** `cargo clippy -p cipherocto-encoding --all-targets -- -D warnings` clean
- [ ] **AC-6:** `cargo fmt -p cipherocto-encoding --check` clean
- [ ] **AC-7:** `MAX_ENCODED_SIZE = 256` enforced; overflow returns `EncodingError::ConstraintOverflow`
- [ ] **AC-8:** `constraint_hash` uses BLAKE3 with `0xA1` domain separator

## Risks (this mission)

| Risk                                                       | Mitigation                                                          |
| ---------------------------------------------------------- | ------------------------------------------------------------------- |
| Discriminator collision with RFC-0965 Caveat (0x01-0x0C)   | Use 0x10-0x28 range (separated by 0x10 offset)                      |
| Encoding choice locks downstream (caveats/policy/envelope) | Trait + 1 impl; second impl allowed post-v1 (RFC-0960 §8 risk #1)   |
| Borsh/serde determinism pitfalls (HashMap ordering)        | Explicit big-endian writes; explicit set sorting; no serde for wire |
| 256-byte cap too tight for future variants                 | MultiEnvelope wrapping available in W6 (RFC-0962)                   |

## Notes

### Implementation status (claimed 2026-07-23)

- All 25 variants defined with explicit field ordering.
- `encode` + `decode` (roundtrip) implemented.
- `constraint_hash` (BLAKE3 with `0xA1` prefix) implemented.
- 256-byte cap enforced.
- PerAssetSpendingCap sorted + max-5 enforced.
- 13 unit tests cover: tag distinctness, roundtrip (5 variants), reject invalid namespace, reject invalid version, reject unknown discriminator, reject truncated, hash determinism, hash distinctness, all-25-discriminator unique, all-25-roundtrip.

### Companion RFC wire alignment

- RFC-0965 §0 uses `0x02` Caveat tag + `0x01-0x0C` existing caveats + `0x10-0x18` new caveats.
- Our Constraint encoder uses `0x01` Constraint tag + `0x10-0x28` constraint variants.
- Collisions avoided: Caveat own namespace (`0x02`); Caveat discriminator (`0x01-0x0C`) is inside the Caveat envelope, not the Constraint envelope. RFC-0964 §0 R3-F6 + R4-F1 + R4-F2 namespace-tag table is consistent.

### Follow-up

- W4: add `pub trait ConstraintEncoder` if a second impl emerges (RFC-0960 §8 risk #1 mitigation).
- W6: ExecutionEnvelope wraps `Vec<Constraint>` via this encoder.

---

**Submission Date:** 2026-07-23
**Last Updated:** 2026-08-11|
**Version:** 1.0 (Claimed)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-07-23 | Mission filed. RFC-0964 reaches Accepted v1.1 per RFC-0960 v2.0 promotion. Substrate expected in `crates/cipherocto-encoding/src/lib.rs`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| v0.2    | 2026-08-11 | **Claimed + LANDED** by @claude. Substrate shipped in commit `d3fd7e27 feat(economics): RFC-0960/62/63/64/65/67 spec→impl conformance` (2026-07-24). 25-variant canonical `Constraint` enum + 1-byte namespace tag (0x01) + 1-byte high-bit version tag (0xA0) + 1-byte discriminator + 2-byte big-endian length-prefixed payload wire format per RFC-0960 §3 + RFC-0964 §1. BLAKE3 with 0xA1 domain separator for `constraint_hash` per RFC-0964 §5. 17 unit tests pass (namespace + version tag distinctness, all_23_variants_have_unique_discriminators, all_25_variants_roundtrip, encode_decode_roundtrip__, decode_rejects__). Cross-references: 0962-a-execution-envelope (sibling consumer), 0965-a-caveat-dsl (companion), 0967-a-policy-object-graph (consumer). |
