# RFC-0964 (Economics): Constraint Encoding Standard

## Status

Draft

> **Note:** Companion RFC to RFC-0960 §5 (Constraints as policy modules). Defines canonical binary encoding for the 23-variant Constraint set identified by Phase 3 research (`docs/research/2026-07-22-external-capability-based-spend-systems.md` §7.7). Encoding is BLAKE3-based, length-prefixed, version-tagged, and ZK-circuit-friendly. Builds on RFC-0126 (deterministic serialization) and EIP-712 (typed-data cross-chain interop).

## Version History

| Version | Date | Author | Note |
|---------|------|--------|------|
| v1.0 | 2026-07-23 | @cipherocto + @mmacedoeu | Initial draft. |

## Authors

- Author: @cipherocto (Constraint encoding work)
- Contributor: @mmacedoeu (RFC-0964 protocol extraction)

## Maintainers

- Maintainer: @mmacedoeu
- Maintainer: @cipherocto

## Summary

A `Constraint` is one of 23 tagged-union variants. Canonical encoding:

```
Constraint encoding:
    discriminator_byte (1 byte) || length_prefix (4 bytes BE) || variant_payload

variant_payload:
    fields in canonical order (length-prefixed strings, BE-encoded numbers, etc.)

Constraint hash:
    blake3(0x01 || constraint_encoding)  // 0x01 = "constraint" domain separator
```

Three artifacts:

1. **`Constraint` enum** — 23-variant tagged union (Time, SpendCap, Destination, CoSigning, Caller, UseCount, Delegation, Composition, Vesting, Compliance groups).
2. **`ConstraintSet`** — ordered list of Constraints; canonical encoding is concatenation of constraint encodings.
3. **`constraint_hash`** — `BLAKE3(0x01 || canonical_ser(constraint_set))` for content-addressing.

Cross-chain interop: each variant carries an EIP-712-style `typed_data_hash` so an Ethereum verifier can validate a constraint without re-implementing the parser.

## Dependencies

### Required RFCs

| RFC | Status | Reason |
|-----|--------|--------|
| RFC-0960 | Draft (companion) | Defines §5 Constraint set as policy modules |
| RFC-0126 | Accepted (v2.5.1) | Canonical serialization for all numeric + structured fields |
| RFC-0853 | Draft | BLAKE3 primitive source for `constraint_hash` |
| RFC-0957 | Draft | Macaroon caveat substrate (constraints are encoded as caveats) |

### Companion RFCs (Planned)

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0965 | Builds on | Capability extension format (caveat DSL consumer) |
| RFC-0961 | Builds on | CIPHERO_SQL `AllowIf` constraint embeds a procedure reference |
| RFC-0962 | Builds on | ConsensusSession envelope references constraints in capability binding |

### Dependency Validation

| Dependency | Type | Current Status (2026-07-23) | Hard-block? |
|------------|------|------------------------------|-------------|
| RFC-0960 | Requires | Draft (companion) | YES |
| RFC-0126 | Requires | Accepted | No |
| RFC-0853 | Requires | Draft | YES |
| RFC-0957 | Requires | Draft | YES |

**DAG check:** `0964 ← {0960, 0126, 0853, 0957}` — acyclic.

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Bit-identical encoding across implementations | Two encoders of the same Constraint produce identical bytes |
| G2 | ZK-circuit-friendly | Field ordering + length prefixes match R1CS/PLONK/STWO circuit shapes |
| G3 | Forward-compatible | Adding a new variant requires a new discriminator byte; old parsers reject unknown bytes (fail-closed) |
| G4 | Cross-chain interop | EIP-712 `typed_data_hash` field on every variant |
| G5 | Compact encoding | Average constraint ≤ 64 bytes; max ≤ 256 bytes |
| G6 | Type-safe deserialization | Unknown variants rejected at parse time, not at evaluation time |

## Motivation

### Why a separate encoding RFC?

The Capability (RFC-0957 + RFC-0965) carries constraints as caveats. Each constraint has:
- A **semantic meaning** ("spend no more than X per Y window").
- A **canonical encoding** (bytes that hash to the same value on every node).
- A **verification algorithm** (does this constraint allow the proposed operation?).

RFC-0964 defines (1) and (2). Verification algorithms live in the verifier (one per Constraint variant, in the constraint pipeline) and are out of scope for this RFC.

### Why BLAKE3 over SHA-256?

CipherOcto's other primitives (RFC-0853) use BLAKE3 keyed mode. Consistency: every commitment in the system uses the same hash family. Performance: BLAKE3 is ~3-5× faster than SHA-256 on modern CPUs.

### Why EIP-712 typed-data hash?

Cross-chain interop. An Ethereum verifier (e.g., a Solidity contract enforcing a CipherOcto capability) can validate the constraint's `typed_data_hash` without re-implementing the parser. EIP-712 is the de-facto typed-data standard on Ethereum; reusing it gives us off-chain verifiability for free.

## Specification

### 0. Wire-format envelope tag (namespace prefix)

To disambiguate Constraint envelopes from other tagged-union envelopes on the wire
(Caveat per RFC-0965, Permission per RFC-0965 §3.2, etc.), every Constraint
envelope is preceded by a **namespace tag byte**:

```text
Constraint envelope (wire):
    namespace_tag:        u8                   // 0x01 = Constraint
    constraint_envelope:  ConstraintEnvelope   // §2 below
```

Namespace tag values (reserved range `0x01-0x1F` for RFC-0964/0965 stack):

| Tag | Meaning |
|---|---|
| 0x00 | forbidden |
| 0x01 | **Constraint** (this RFC) |
| 0x02 | **Caveat** (RFC-0965) |
| 0x03 | **PermissionKind** (RFC-0965 §3.2) |
| 0x04 | **ReservationState** (RFC-0960 §2.3) |
| 0x05 | **Capability** (RFC-0965 §6) |
| 0x06 | **ConsensusSession** (RFC-0962 §4) |
| 0x07-0x1F | reserved for future stack expansion |
| 0x20-0xFF | application-specific |

Receivers MUST read the namespace tag first and dispatch to the correct parser.
A receiver that sees an unknown tag (e.g. 0x07) MUST fail-closed.

**Discriminator bytes within a Constraint envelope** (§1 below) are local to
the Constraint namespace; they do NOT share an address space with Caveat
discriminators (RFC-0965) or any other tagged union. A byte 0x05 inside a
Constraint envelope means `MaxPerTx`; a byte 0x05 inside a Caveat envelope
means `After` (deprecated time-bound, RFC-0957).

### 1. Constraint variant enumeration

The canonical 23-variant set from Phase 3 research, grouped by category:

| Discriminator | Variant | Group |
|---|---|---|
| 0x01 | `ValidRange` | Time |
| 0x02 | `NotBefore` | Time |
| 0x03 | `UnlockAfter` | Time |
| 0x04 | `Period` | Time |
| 0x05 | `MaxPerTx` | SpendCap |
| 0x06 | `PerAssetSpendingCap` | SpendCap |
| 0x07 | `RateLimit` | SpendCap |
| 0x08 | `AllowedDestinations` | Destination |
| 0x09 | `DeniedDestinations` | Destination |
| 0x0A | `IntentBound` | Destination |
| 0x0B | `MultiSig` | CoSigning |
| 0x0C | `RequireReceiptSignatureBy` | CoSigning |
| 0x0D | `CallerBound` | Caller |
| 0x0E | `MaxUses` | UseCount |
| 0x0F | `SingleUse` | UseCount |
| 0x10 | `AllowIf` | Delegation |
| 0x11 | `VerifierRequired` | Delegation |
| 0x12 | `WrappedOnly` | Composition |
| 0x13 | `SponsoredBy` | Composition |
| 0x14 | `CoordinatorCanSubmit` | Composition |
| 0x15 | `LinearRelease` | Vesting |
| 0x16 | `CliffVesting` | Vesting |
| 0x17 | `LiquidityLock` | Vesting |
| 0x18 | `GovernanceLock` | Vesting |
| 0x19 | `ComplianceHold` | Compliance |

Total: 25 entries (23 variants + 0x00 reserved for "unknown"). Discriminator `0x00` is forbidden; parsers MUST reject `0x00` (fail-closed).

### 2. Top-level encoding

```text
Constraint encoding (one constraint):
    discriminant_byte:   u8                       // 1 byte, ∈ [0x01, 0x19]
    length_prefix:       [u8; 4]                  // 4 bytes BE; length of variant_payload
    variant_payload:     [u8; length_prefix]      // variant-specific encoding
```

Total: 5 bytes overhead + variant_payload bytes.

### 3. Variant payload encodings

All numbers are big-endian (BE) per RFC-0126. All byte strings are length-prefixed (4 bytes BE length, then bytes). All timestamps are `u64` unix seconds.

#### 3.1 Time group

```text
ValidRange:
    valid_after_unix:    u64 BE
    valid_until_unix:    u64 BE
    // 16 bytes payload

NotBefore:
    not_before_unix:     u64 BE
    // 8 bytes payload

UnlockAfter:
    unlock_at_block:     u64 BE
    // 8 bytes payload

Period:
    max_per_period:      u128 BE              // 16 bytes
    period_duration_secs: u64 BE              // 8 bytes
    // 24 bytes payload
```

#### 3.2 SpendCap group

```text
MaxPerTx:
    amount_micro:        u128 BE              // micro-units
    asset_id:            [u8; 32]            // asset identifier
    // 48 bytes payload

PerAssetSpendingCap:
    caps:                Vec<(asset_id, amount_micro)>
    // Each entry: asset_id (32 bytes) || amount_micro (16 bytes)
    // Vec length prefix: 4 bytes BE
    // Average: ~50 bytes payload for 1 asset; ~250 bytes for 5 assets

RateLimit:
    max_per_window:      u128 BE
    window_duration_secs: u64 BE
    asset_id:            [u8; 32]
    // 56 bytes payload
```

#### 3.3 Destination group

```text
AllowedDestinations:
    dids:                Vec<DID>             // each DID: 4 bytes BE length || UTF-8 bytes
    // Vec length prefix: 4 bytes BE
    // Average: ~30 bytes for 1 DID; ~200 bytes for 5 DIDs

DeniedDestinations:
    // Same shape as AllowedDestinations

IntentBound:
    message_template:    Bytes                // 4 bytes BE length || bytes
    // Average: 100-500 bytes (template strings)
```

#### 3.4 CoSigning group

```text
MultiSig:
    n:                   u32 BE               // threshold
    signers:             Vec<DID>             // each DID: 4 bytes BE length || UTF-8 bytes
    // ~70 bytes for 2-of-3 (n=2, signers=[3 DIDs])

RequireReceiptSignatureBy:
    did:                 DID                  // 4 bytes BE length || UTF-8 bytes
    // ~30 bytes payload
```

#### 3.5 Caller group

```text
CallerBound:
    holder:              DID                  // 4 bytes BE length || UTF-8 bytes
    // ~30 bytes payload
```

#### 3.6 UseCount group

```text
MaxUses:
    count:               u32 BE
    // 4 bytes payload

SingleUse:
    // 0 bytes payload (discriminator alone)
```

#### 3.7 Delegation group

```text
AllowIf:
    predicate_hash:      [u8; 32]             // blake3(canonical_ser(predicate))
    step_budget:         u32 BE
    // 36 bytes payload

VerifierRequired:
    circuit_id:          [u8; 32]             // BLAKE3(canonical_ser(circuit))
    // 32 bytes payload
```

#### 3.8 Composition group

```text
WrappedOnly:
    // 0 bytes payload

SponsoredBy:
    vault_id:            [u8; 32]
    // 32 bytes payload

CoordinatorCanSubmit:
    coordinator:         DID                  // 4 bytes BE length || UTF-8 bytes
    // ~30 bytes payload
```

#### 3.9 Vesting group

```text
LinearRelease:
    start_unix:          u64 BE
    end_unix:            u64 BE
    cliff_unix:          u64 BE
    // 24 bytes payload

CliffVesting:
    until_unix:          u64 BE
    pct:                 u32 BE               // basis points (0-10000)
    period_secs:         u64 BE
    // 16 bytes payload

LiquidityLock:
    until_unix:          u64 BE
    // 8 bytes payload

GovernanceLock:
    while_vote_active:   bool                 // 1 byte (0x00 or 0x01)
    // 1 byte payload
```

#### 3.10 Compliance group

```text
ComplianceHold:
    threshold:           u128 BE
    delay_secs:          u64 BE
    // 24 bytes payload
```

### 4. ConstraintSet encoding

```text
ConstraintSet encoding:
    version_tag:         u8                   // 0x01 (current)
    constraint_count:    u32 BE               // number of constraints
    constraints:         [Constraint; count]  // concatenated constraint encodings
```

Constraint ordering is preserved exactly. Two `ConstraintSet`s with the same constraints in different orders have **different encodings** and thus different `constraint_hash`es. This is intentional: it preserves canonical ordering for deterministic evaluation.

### 5. Constraint hash

```text
constraint_hash(constraint_set: ConstraintSet) -> [u8; 32]:
    return BLAKE3(0x01 || canonical_ser(constraint_set))
```

The `0x01` prefix is the domain separator. Distinct from RFC-0853's other domain separators (e.g., `0x02` for shard roots, `0x03` for session envelopes).

### 6. EIP-712 typed-data hash

For cross-chain verifiability:

```text
typed_data_hash(constraint: Constraint) -> [u8; 32]:
    let domain_separator = BLAKE3(0x02 || canonical_ser({
        name: "CipherOcto.Constraint",
        version: "1",
        chain_id: <network_chain_id>,
        verifying_contract: <capability_id>,
    }))

    let message_hash = BLAKE3(0x03 || constraint_encoding)

    return BLAKE3(0x04 || domain_separator || message_hash)
```

`typed_data_hash` is what an Ethereum contract calls to verify a constraint. Solidity equivalent:

```solidity
function verifyConstraint(
    bytes32 constraintTypedDataHash,
    bytes calldata constraintEncoding
) external pure returns (bool) {
    // Recompute typed_data_hash from encoding
    // Compare against provided hash
    // Return true iff match
}
```

Cross-chain verifiers don't need to re-implement the parser; they only need to recompute the typed-data hash.

### 7. Worked example: `RateLimit` constraint

Encoding a "max 1000 OCTO_W per hour" constraint:

```rust
let constraint = Constraint::RateLimit {
    max_per_window: 1_000_000_000,  // 1000 OCTO_W in micro-units
    window_duration_secs: 3600,     // 1 hour
    asset_id: [0xab; 32],            // OCTO_W asset ID
};

let bytes = constraint.encode();
// = [0x07, 0x00, 0x00, 0x00, 0x38,  // discriminator 0x07, length 0x38 = 56
//    0x00, 0x00, 0x00, 0x00, 0x3B, 0x9A, 0xCA, 0x00,  // 1_000_000_000 BE
//    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0E, 0x10,  // 3600 BE
//    0xab, 0xab, ... (32 bytes of asset_id)]
// Total: 5 + 56 = 61 bytes
```

Decoding:

```rust
let parsed = Constraint::decode(&bytes).expect("decode");
assert_eq!(parsed, constraint);
assert_eq!(parsed.discriminant(), 0x07);
```

Hash:

```rust
let h = constraint.hash();
// = BLAKE3(0x01 || bytes)
// 32 bytes
```

### 8. Worked example: `ConstraintSet`

A capability with `MaxPerTx(50 OCTO_W)` + `ValidRange(2026-07-22, 2027-01-01)`:

```rust
let constraints = ConstraintSet::new(vec![
    Constraint::MaxPerTx {
        amount_micro: 50_000_000,
        asset_id: [0xab; 32],
    },
    Constraint::ValidRange {
        valid_after_unix: 1_753_142_400,  // 2026-07-22 00:00 UTC
        valid_until_unix: 1_786_272_000,  // 2027-01-01 00:00 UTC
    },
]);

let bytes = constraints.encode();
let h = constraints.hash();
```

Both constraints are encoded in order, then hashed with the `0x01` domain separator.

### 9. Catalog schema

```sql
CREATE TABLE constraint_definitions (
    constraint_hash    BYTES PRIMARY KEY,       -- BLAKE3(0x01 || canonical_ser(constraint_set))
    constraint_set     BLOB NOT NULL,           -- canonical_ser encoding
    definition         TEXT NOT NULL,           -- human-readable form
    created_at_unix    BIGINT NOT NULL,
    usage_count        BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX ix_constraint_usage ON constraint_definitions (usage_count DESC);
```

Constraint definitions are content-addressed: two capabilities with the same constraints share the same `constraint_hash` row. This enables deduplication + fast lookup.

### 10. Wire format

For consensus messages (RFC-0862 + RFC-0962):

```text
constraint_envelope := {
    constraint_hash:    [u8; 32],
    constraint_set:     ConstraintSet,        // re-encoding for verification
    typed_data_hash:    [u8; 32],
    signature:          Ed25519Signature,     // optional; for EIP-712 verification
}
```

Receivers verify by:
1. `constraint_hash == BLAKE3(0x01 || canonical_ser(constraint_set))` — encoding consistency.
2. `typed_data_hash == BLAKE3(0x04 || domain_separator || BLAKE3(0x03 || constraint_encoding))` — EIP-712 consistency.
3. `signature.verify(signer_pubkey, typed_data_hash)` — if signature present.

## Open Questions

| # | Question | Resolution Target |
|---|----------|-------------------|
| 1 | How many variants can fit in the 0x01-0xFF range? | 254 max; we're at 25. Plenty of headroom for v1.x. |
| 2 | Can a ConstraintSet reference another ConstraintSet (e.g., "all of set A plus all of set B")? | Future variant `Composite(AND(set_a, set_b))`; defer to v1.1 |
| 3 | Should `GovernanceLock` include the proposal_id? | Currently bool-only; v1.1 may add proposal_id binding |
| 4 | How is `AllowIf` predicate serialized? | Reference to a CIPHERO_SQL procedure (RFC-0961); `predicate_hash` is `BLAKE3(canonical_ser(proc_id))` |
| 5 | Can a capability carry 1000+ constraints? | Technically yes; performance-wise, evaluation cost is O(N) per check. Caps to be determined. |
| 6 | What about constraint intersection/union for cross-capability composition? | Out of scope; capabilities compose via parent_capability reference |

## Out of Scope

- **Constraint verification algorithms.** This RFC defines encoding only. Verifier lives in the constraint pipeline (separate crate).
- **Constraint UI / authoring tools.** Encoding is canonical; authoring tools are application-level.
- **Backward-compat migration paths.** New variants are fail-closed; old nodes reject them. No migration shim.

## Status

This RFC = Constraint encoding standard. Status: Draft. Companion RFCs 0960, 0961, 0962, 0965 in flight. Awaiting review and promotion to Accepted.

Once Accepted, the `cipherocto-constraint` crate implements:
- `Constraint` enum (25 variants incl. reserved)
- `encode()` / `decode()` methods (canonical_ser)
- `constraint_hash()` (BLAKE3 with domain separator)
- `typed_data_hash()` (EIP-712-compatible)
- Cross-chain verifier interface (for Solidity / other-chain contracts)
