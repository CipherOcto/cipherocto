# RFC-0014-v2 — Settlement Substrate Amendment v2

| Field        | Value                                                                                       |
| ------------ | ------------------------------------------------------------------------------------------- |
| Status       | Draft                                                                                       |
| Version      | v2.0.0-draft                                                                                |
| Layer        | A (substrate-frozen)                                                                        |
| Authors      | CipherOcto core team                                                                        |
| Maintainers  | octo-settlement-core maintainers                                                            |
| Parent RFC   | RFC-0014 (accepted)                                                                         |
| Supersedes   | RFC-0014 §Data Structures (none yet; v2 codifies the extension surface)                     |
| Companion    | RFC-0012-v2 (audit substrate amendment v2), RFC-0015-a + RFC-0016-a (write-path amendments) |
| Target crate | `octo-settlement-core` v2.0.0 (semver-major)                                                |

## Summary

RFC-0014-v2 is a **Layer A substrate amendment** to `octo-settlement-core` that:

1. **Pins the typed-discriminator extension pattern** for new `Receipt` semantics — extensions land via `ask_id` typed-discriminator namespaces per `cipherocto/settlement/extension/<kind>/v1/`, NOT via central `Receipt` field edits.
2. **Pins the canonical 6-field `Receipt` struct** — no new fields; substrate-frozen.
3. **Pins `ReceiptId(pub u64)` newtype** — type-safe wrapper around `Receipt.receipt_id: u64` for façade-level use; canonical hash unchanged.
4. **Pins `AppendOnlyReceiptSink::append` invariants** — strict `receipt_id` monotonicity, canonical `settlement_hash` verification, atomic persistence (parallel to RFC-0012-v2 §S2).
5. **Pins `SettlementError` canonical form** — adapter-specific failures map to `SettlementError::SinkSpecific(String)`; no new variants.

Per CLAUDE.md §Architectural Principles + §Extension over enumeration, RFC-0014-v2 enables future receipt extension kinds (AskSettled, AskPartial, AskRejected, AskRefunded, etc) without modifying `octo_settlement_core::Receipt` (which is substrate-frozen).

## Authors

- CipherOcto Architecture Working Group

## Maintainers

- CipherOcto Architecture Working Group

## Dependencies

- **RFC-0014** (parent, accepted) — defines `Receipt`, `Ask`, `AskState`, `Reservation`, `ReservationState`, `SettlementStore`, `AppendOnlyReceiptSink`, `SettlementError`, `verify_receipt_chain`, `receipt_id_for`.
- **RFC-0012-v2** (sibling, draft) — pins audit substrate extension pattern; cross-RFC consistency on canonical extension kind table.
- **RFC-0016-a** (write-path amendment, draft) — requires RFC-0014-v2 for `ReceiptId` newtype + `ask_id` typed-discriminator pattern.
- **RFC-0015-a** (write-path amendment, draft) — cross-RFC: agent transition receipt uses `ask_id = BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/")` typed-discriminator.

## Design Goals

**G1.** Enable future receipt extension kinds without modifying the substrate 6-field `Receipt` struct.
**G2.** Preserve byte-identical canonical-bytes serialization form for existing receipts.
**G3.** Maintain `#[non_exhaustive]` discipline on `Receipt` so substrate-stored receipts remain forward-compatible with future extensions.
**G4.** Provide type-safe `ReceiptId` newtype for façade-level use without changing canonical substrate `Receipt.receipt_id: u64`.
**G5.** Lock the substrate contract surface so adapter implementations (`StoolapReceiptSink` in `octo-settlement/src/storage/stoolap.rs`) have no ambiguity on what they must enforce.

## Motivation

RFC-0014 (accepted) defines a 6-field `Receipt` struct (`receipt_id`, `ask_id`, `settlement_hash`, `router_id`, `router_sig`, `timestamp_unix`). The substrate-frozen nature means:

- Adding fields (e.g. `model`, `cost_dqa`, `capability_root`, `subject_did`, `status`) requires semver-major bump + migration of every existing consumer.
- Future receipt semantics (AskPartial settlements, AskRejected settlements, refund receipts) need a substrate-faithful encoding.

RFC-0014-v2 codifies the typed-discriminator pattern via `ask_id` extension namespaces so write-path amendments can reference a pinned canonical substrate form without forcing field additions to `Receipt`.

## Roles and Authorities

| Role                                        | Authority                                                                                                                                                                                  |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `octo-settlement-core` (substrate, Layer A) | Owns `Receipt`, `Ask`, `AskState`, `Reservation`, `ReservationState`, `SettlementStore`, `AppendOnlyReceiptSink`, `SettlementError`, `ReceiptId`, `verify_receipt_chain`, `receipt_id_for` |
| `octo-settlement` (façade, Layer B)         | Re-exports substrate canonical types + implements `StoolapReceiptSink` (Layer D adapter) + provides `ReceiptSummary` projection + `ReceiptStatus` enum                                     |
| Domain callers (Layer B / C / D)            | Construct `Receipt` instances via typed-discriminator helpers (e.g. `receipt_for_ask_settled(...)` in `octo-settlement/src/receipt.rs`)                                                    |

**Façade projection concerns (NOT substrate):**

- `ReceiptStatus` enum — façade-side (Layer B); recovers status from `ask_id` typed-discriminator at projection time.
- `ReceiptSummary` projection struct — façade-side (Layer B); reads canonical `Receipt` + extension payload.

## Specification

### §S1 — Typed-discriminator extension pattern (canonical)

§S1 pins the extension pattern as the canonical mechanism for new receipt semantics.

**Pattern:** A new receipt kind `K` is encoded as:

- `Receipt.ask_id = BLAKE3-256("cipherocto/settlement/extension/<K>/v1/" || canonical_ask_id_bytes)`

The `ask_id` field is a 32-byte BLAKE3-256 digest. By extending the input with a namespace string prefix, the digest acts as a typed-discriminator: the first 32 bytes of input are the extension namespace identifier, the remainder is the canonical ask identifier.

**Why this works:** The `ask_id` field is already present on `Receipt` (RFC-0014 §Data Structures). Domain crates construct `ask_id` by hashing namespace + canonical ask_id bytes. Substrate accepts `ask_id` as opaque 32-byte digest (no domain logic). The substrate `verify_receipt_chain` function does NOT interpret `ask_id` semantics — it only validates `receipt_id` monotonicity + `settlement_hash` integrity + `timestamp_unix` monotonicity.

**Note on input boundary:** the BLAKE3-256 input is variable-length (`namespace_string || canonical_ask_id_bytes`); there is no fixed 32-byte boundary between the namespace and the ask_id bytes. Readers recover extension kind K by enumerating candidate namespace strings (or by convention lookup at the façade).

**Extension kind table (canonical):**

| Extension kind             | `ask_id` prefix (BLAKE3-256 input)                                                                             | Substrate version                           |
| -------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| AskSettled                 | `cipherocto/settlement/extension/ask-settled/v1/` (RFC-0014 existing; `ask_id` = digest of canonical ask only) | RFC-0014                                    |
| **AskPartial**             | `cipherocto/settlement/extension/ask-partial/v1/`                                                              | **RFC-0014-v2**                             |
| **AskRejected**            | `cipherocto/settlement/extension/ask-rejected/v1/`                                                             | **RFC-0014-v2**                             |
| **AgentTransitionReceipt** | `cipherocto/settlement/extension/agent-transition-receipt/v1/`                                                 | **RFC-0014-v2** (cross-RFC with RFC-0015-a) |

**Extension kind enumeration is exhaustive for v2.0.0.** Future extensions (v2.1+) are added to this table via subsequent RFCs and do NOT require substrate field additions.

### §S2 — `Receipt` 6-field canonical form

§S2 pins the existing 6-field `Receipt` struct as the substrate canonical form:

```rust
// crates/octo-settlement-core/src/receipt.rs (§S2 pinned — no change from RFC-0014)
pub struct Receipt {
    pub receipt_id: u64,
    pub ask_id: [u8; 32],
    pub settlement_hash: [u8; 32],
    pub router_id: String,
    pub router_sig: Vec<u8>,
    pub timestamp_unix: u64,
}
```

**No new fields.** RFC-0014-v2 explicitly rejects adding fields to `Receipt`:

- Adding `model: Option<String>` would require migration of every existing persisted receipt (semver-major + data migration).
- Adding `status: ReceiptStatus` would couple substrate to façade projection enum.
- Adding `capability_root: Option<[u8; 32]>` would change canonical hash computation (`verify_receipt_chain` inputs change → existing chains fail verification).

§S1 typed-discriminator pattern achieves extension semantics without substrate field additions.

### §S3 — `ReceiptId(pub u64)` newtype

§S3 introduces `ReceiptId` as a type-safe wrapper around `u64`:

```rust
// crates/octo-settlement-core/src/receipt.rs (§S3 — new newtype)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiptId(pub u64);

impl ReceiptId {
    /// Construct from raw monotonic `receipt_id` value.
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Extract the raw monotonic value (for substrate persistence).
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}
```

**Substrate contract:**

- `Receipt.receipt_id: u64` field remains substrate-frozen (canonical hash unchanged).
- `ReceiptId` newtype is substrate-side because `receipt_id` is a canonical substrate identifier.
- Façade-level callers use `ReceiptId` for type-safety (e.g. `ReceiptId` parameters to `get_receipt(id: ReceiptId)`); substrate persists raw `u64`.
- Conversion: `ReceiptId::new(receipt.receipt_id)` (façade → substrate boundary).

### §S4 — `AppendOnlyReceiptSink::append` invariants (canonical)

§S4 pins the following substrate-level invariants that all `AppendOnlyReceiptSink` implementations MUST enforce (parallel to RFC-0012-v2 §S2):

1. **`&mut self` requirement** — already substrate. The trait takes `&mut self` to enforce type-level append-only.

2. **Strict receipt_id monotonicity** — `receipt.receipt_id` MUST equal `last_receipt_id() + 1`. On gap, return `SettlementError::SequenceGap { receipt_id, prev: last_receipt_id }`.

3. **Idempotent re-append** — `receipt.receipt_id == last_receipt_id()` MUST return `SettlementError::AlreadyExists(receipt_id)`. NOT a success.

4. **Canonical `settlement_hash` verification** — implementations MUST recompute `settlement_hash` via `octo_settlement_core::receipt_id_for(&receipt)` and verify `receipt.settlement_hash == computed`. On mismatch, return `SettlementError::ChainIntegrity { receipt_id: receipt.receipt_id }` (NOT `SinkSpecific`; the substrate-canonical error for chain-integrity failures is `ChainIntegrity` per RFC-0014 §Error Type).

5. **Atomic persistence** — implementations MUST persist the receipt in a transaction-scoped atomic write. Adapter impls (e.g. `StoolapReceiptSink`) MUST wrap persistence in a Stoolap `Transaction`.

6. **`SinkSpecific` boundary** — adapter-specific failures MUST map to `SettlementError::SinkSpecific(String)`. No raw error chains, no adapter-type names leaking past the substrate boundary.

**Substrate contract surface (v2.0.0):**

```rust
// crates/octo-settlement-core/src/sink.rs (§S4 pinned)
pub trait AppendOnlyReceiptSink {
    fn append(&mut self, receipt: &Receipt) -> Result<(), SettlementError>;
    fn last_receipt_id(&self) -> Result<Option<u64>, SettlementError>;
}
```

### §S5 — `SettlementError` canonical form

§S5 pins the **existing** `SettlementError` cross-trait envelope form:

```rust
// crates/octo-settlement-core/src/error.rs (§S5 pinned — substrate-faithful)
#[derive(Debug, Error)]
pub enum SettlementError {
    #[error("ask not found: {0:?}")]
    AskNotFound([u8; 32]),

    #[error("ask {0:?} already consumed")]
    AlreadyConsumed([u8; 32]),

    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidTransition { from: String, to: String },

    #[error("receipt_id sequence gap: {receipt_id} after {prev}")]
    SequenceGap { receipt_id: u64, prev: u64 },

    #[error("chain integrity violation at receipt_id {receipt_id}")]
    ChainIntegrity { receipt_id: u64 },

    #[error("receipt_id {0} already persisted")]
    AlreadyExists(u64),

    #[error("sink-specific error: {0}")]
    SinkSpecific(String),
}
```

**Adapter implementations MUST NOT add new variants.** RFC-0014-v2 explicitly rejects:

- New error variants in concrete impls.
- `From<AdapterError> for SettlementError` that leaks adapter-specific structure.
- `SettlementError` payload fields that reference adapter types.

Adapter-specific error chains MUST be scrubbed at the adapter boundary per §SC2 + the canonical scrubber declared in §S5.1.

#### §S5.1 — Canonical adapter-error scrubber (Layer B placement per RFC-0011-a)

§S5.1 RESCINDED the Layer A scrubber declaration per R31 layer-model finding (scrubber is a Layer B concern per RFC-0011-a). The canonical scrubber now lives at `octo-settlement::scrub::scrub_adapter_error` (Layer B façade):

```rust
// crates/octo-settlement/src/scrub.rs (§S5.1 — Layer B façade placement)
//
// LAYER NOTE: scrubber is Layer B per RFC-0011-a §7.7 Redaction model.
// Substrate (Layer A) MUST NOT host scrubbing logic because adapter-error
// patterns are adapter-side concern, not substrate contract.
//
// Adapter implementations MUST call this function before wrapping any
// adapter error into SettlementError::SinkSpecific(String) or any other
// variant that carries adapter-derived content.

/// Canonical adapter-error scrubber. Maps adapter-specific error chains
/// (Stoolap transaction IDs, IO path fragments, table-name leaks) to
/// a substrate-canonical scrubbed String.
///
/// Pattern list (substrate-canonical for v2.0.0, 6 patterns):
/// 1. Hex digest ≥32 chars (transaction IDs, hashes) → `<redacted-hex-N>`
/// 2. Absolute file paths (`/.../...`) → `<redacted-path>`
/// 3. Table-name references (`table 'X'`, `relation "X"`) → `<redacted-table>`
/// 4. Stoolap/SQL error code prefixes (`SQLSTATE_XXXXX`) → `<redacted-sql-state>`
/// 5. `std::io::Error` chain fragments (`os error N`) → `<redacted-io>`
/// 6. Adapter-type names (e.g. `StoolapTransactionError`) → `<redacted-adapter>`
pub fn scrub_adapter_error(s: &str) -> String {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    let mut out = s.to_string();

    // Pattern 1: hex digests ≥32 chars (case-insensitive)
    let hex_re = once_cell::sync::Lazy::<regex::Regex>::new(|| {
        regex::Regex::new(r"\b[0-9a-fA-F]{32,}\b").unwrap()
    });
    out = hex_re.replace_all(&out, |_caps: &regex::Captures| {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("<redacted-hex-{}>", n)
    }).into_owned();

    // Pattern 2: absolute file paths
    let path_re = once_cell::sync::Lazy::<regex::Regex>::new(|| {
        regex::Regex::new(r"(?:/[A-Za-z0-9_.-]+){2,}").unwrap()
    });
    out = path_re.replace_all(&out, "<redacted-path>").into_owned();

    // Pattern 3: table-name references
    let table_re = once_cell::sync::Lazy::<regex::Regex>::new(|| {
        regex::Regex::new(r#"(?:table|relation)\s+['"]([A-Za-z0-9_]+)['"]"#).unwrap()
    });
    out = table_re.replace_all(&out, "<redacted-table>").into_owned();

    // Pattern 4: SQLSTATE prefixes
    let sqlstate_re = once_cell::sync::Lazy::<regex::Regex>::new(|| {
        regex::Regex::new(r"SQLSTATE_[A-Z0-9]+").unwrap()
    });
    out = sqlstate_re.replace_all(&out, "<redacted-sql-state>").into_owned();

    // Pattern 5: io error chain fragments
    let io_re = once_cell::sync::Lazy::<regex::Regex>::new(|| {
        regex::Regex::new(r"os error \d+").unwrap()
    });
    out = io_re.replace_all(&out, "<redacted-io>").into_owned();

    // Pattern 6: adapter-type names (extensible set)
    const ADAPTER_TYPES: &[&str] = &[
        "StoolapTransactionError",
        "StoolapConnectionError",
        "StoolapDatabaseError",
    ];
    for ty in ADAPTER_TYPES {
        out = out.replace(ty, "<redacted-adapter>");
    }

    out
}
```

The 6-pattern list is canonical for v2.0.0. Future RFCs MAY extend the list; downstream adapters MUST call this function rather than implementing their own scrubber (per RFC-0011-a §7.7 redaction discipline).

**Cargo.toml dependency at Layer B façade** (`crates/octo-settlement/Cargo.toml`):

```toml
[dependencies]
regex = { version = "1.10" }
once_cell = { version = "1.19" }
```

### §S6 — Canonical-hash construction (keyed BLAKE3)

§S6 pins the existing canonical-hash construction for receipts:

```
settlement_hash = BLAKE3-256-keyed(
    key = [0; 32],
    input = CHAIN_DOMAIN_SEPARATOR || receipt_id_be_u64 || ask_id || router_id_utf8 || router_sig || timestamp_unix_be_u64
)
```

Where `CHAIN_DOMAIN_SEPARATOR = b"cipherocto/reservation/v1/"` (byte-pinned constant exported from `octo-settlement-core::chain`).

**Critical invariants:**

- `settlement_hash` is the OUTPUT of the BLAKE3 hash; it is NOT part of the input (including it would create a circular self-dependency).
- BLAKE3 streaming obviates an explicit `router_sig_len` prefix — the streaming hash absorbs variable-length bytes directly.
- The BLAKE3 key is `[0; 32]` (zero-filled 32-byte constant). This is INTENTIONAL — the substrate is Layer A frozen (RFC-0014 §Data Structures) and cross-replica consensus requires the receipt hash to be deterministic for any replica that holds the same `Receipt` inputs (no per-deployment key material). Production deployments needing keyed-hash defense-in-depth SHOULD wrap this function (via `KeyedHasher` trait on Layer C) and inject real key material from an HSM / Vault / secrets manager at startup.
- Extensions do NOT add new bytes to the canonical hash input. Extension payload (e.g. AskPartial supplemental amount, AskRejected reason) is encoded in domain-crate-side canonical hash inputs and surfaced via façade projection (RFC-0016-a).

**Substrate code surface (substrate-faithful):**

```rust
// crates/octo-settlement-core/src/chain.rs (§S6 — substrate-faithful)
pub const CHAIN_DOMAIN_SEPARATOR: &[u8] = b"cipherocto/reservation/v1/";

pub fn receipt_id_for(receipt: &Receipt) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(&[0; 32]);
    hasher.update(CHAIN_DOMAIN_SEPARATOR);
    hasher.update(&receipt.receipt_id.to_be_bytes());
    hasher.update(&receipt.ask_id);
    hasher.update(receipt.router_id.as_bytes());
    hasher.update(&receipt.router_sig);
    hasher.update(&receipt.timestamp_unix.to_be_bytes());
    *hasher.finalize().as_bytes()
}
```

### §S7 — Cross-RFC consistency with RFC-0012-v2 (façade-level pairing, NOT substrate shared hash input)

§S7 establishes cross-RFC invariants with RFC-0012-v2. **Important:** the audit `cap_root_hash` and the receipt `ask_id` use **different BLAKE3 inputs** (audit is namespace-only; receipt is namespace || canonical_ask_id). The pairing is a **façade convention**, not a cryptographic binding at the substrate hash layer.

| RFC-0012-v2 audit event                                                                                                     | RFC-0014-v2 receipt extension kind                                                                                                       | Pairing invariant (façade-side)                                                                                                                                                                                                                                                                                                                                         |
| --------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AuditEventKind::Insert` + `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")` (namespace only) | `Receipt.ask_id = BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/" \|\| canonical_ask_id)` (namespace + ask_id) | Façade helper `audit_event_for_agent_transition_receipt(receipt: &Receipt) -> AuditEvent` reconstructs the audit event with `prev_chain_hash = receipt_id_for(receipt)` so the audit event's `prev_chain_hash` binds it to the specific receipt's hash. Substrate-level pairing is via the `prev_chain_hash` field on the audit event, not via shared namespace prefix. |
| `AuditEventKind::Revoke` + `cap_root_hash = BLAKE3-256("cipherocto/audit/extension/redaction/v1/")` (namespace only)        | `Receipt.ask_id = BLAKE3-256("cipherocto/settlement/extension/ask-rejected/v1/" \|\| canonical_ask_id)` (namespace + ask_id)             | Façade helper `audit_event_for_ask_rejected(receipt: &Receipt) -> AuditEvent` similarly binds via `prev_chain_hash = receipt_id_for(receipt)`.                                                                                                                                                                                                                          |

**Cross-RFC consistency rule:** For each (audit extension kind, receipt extension kind) pair, the namespace strings are coordinated via RFC-0012-v2 + RFC-0014-v2 paired acceptance. The substrate-level binding is via `prev_chain_hash = settlement_hash` on the audit event; the façade helper sets this field at construction time. Future extensions follow the same pattern.

## Implicit Assumptions

- **A1. Keyed BLAKE3 with zero key intentional.** `settlement_hash = BLAKE3-256-keyed([0; 32], ...)` uses a zero key intentionally to enable cross-replica determinism without key-management infrastructure. Blast radius if misused: keyed mode is reserved for future HSM/Vault key injection per RFC-0009 §Identity substrate. Mitigation: zero-key usage documented at §S6 + substrate comment; future key-injection migrations noted in §Future Work.
- **A2. Settlement-hash non-circularity.** `settlement_hash` is computed from `Receipt` fields but is NOT a field on `Receipt` itself — it is a separate 32-byte output. Blast radius if violated: circular hashing would make `verify_receipt_chain` unsound. Mitigation: substrate `compute_settlement_hash` excludes `settlement_hash` from its input by construction; TV-SET-v2-2 verifies round-trip.
- **A3. ask_id namespace prefix recovery.** The BLAKE3-256 input is variable-length (`namespace_string || canonical_ask_id_bytes`); readers recover extension kind K by enumerating candidate namespace strings. Blast radius if enumeration fails: extension kind unrecoverable, projection fails closed. Mitigation: canonical extension kind table at §S1; façade-side lookup helper for known kinds.
- **A4. `router_sig` non-verification at substrate.** Substrate does NOT verify `router_sig`; verification is layer B / domain concern per RFC-0014 §Security Considerations. Blast radius: forged receipts persist into chain. Mitigation: domain-crate `verify_router_sig(receipt, router_pubkey)` call at the domain boundary; substrate accepts only the bytes.
- **A5. `timestamp_unix` monotonicity.** Substrate enforces strict `timestamp_unix` monotonicity on append (parallel to `event_id`). Blast radius: clock drift between replicas triggers spurious SequenceGap. Mitigation: external time-source synchronization (NTP) + adapter-side timestamp monotonicity check.

## Determinism Requirements

Per RFC-0008 Execution Class Mapping, all substrate operations in this RFC are **Class A** (deterministic, byte-identical across replicas):

- **`compute_settlement_hash`** — keyed BLAKE3-256 over canonical input; pure function of `Receipt` fields; deterministic across replicas.
- **`verify_receipt_chain`** — deterministic replay of `compute_settlement_hash` over each persisted receipt; byte-identical verification result.
- **`receipt_id_for`** — keyed BLAKE3-256 domain-separator over `request_canonical_bytes`; deterministic across replicas.
- **Typed-discriminator construction** — `BLAKE3-256("cipherocto/settlement/extension/<K>/v1/" || canonical_ask_id_bytes)` is a pure function of `<K>` + canonical ask_id bytes; deterministic.
- **`receipt_id` monotonicity ordering** — strict total order; deterministic across replicas.
- **`timestamp_unix` wall-clock vs logical ordering** — wall-clock values; monotonicity precondition assumes synchronized time source (NTP).

Cross-replica consensus invariant: any two replicas observing the same receipt sequence MUST produce identical `settlement_hash` values. Failure indicates either (a) a serialization drift (adapter-side bug) or (b) a field-level non-determinism (e.g. timestamp drift between insert + compute).

## Performance Targets

- **Append latency ceiling:** 1 ms p99 on commodity SSD (single-receipt append with monotonicity check + settlement_hash recompute + persistence).
- **Chain verification O(n):** `verify_receipt_chain(n)` MUST complete in O(n) time over `n` persisted receipts; no quadratic scans.
- **Adapter throughput floor:** 1,000 appends/second sustained single-writer; 100 appends/second under 4-way concurrent append with monotonicity serialization.
- **`compute_settlement_hash` cost:** keyed BLAKE3-256 over ~120-byte input; expected <10 µs per call on commodity CPU.
- **`ReceiptId` newtype zero-overhead:** newtype wraps `u64`; no runtime cost beyond the wrapper (verified by `cargo build --release` symbol inspection).
- **Storage adapter memory ceiling:** sink implementations MUST NOT hold more than 64 KiB per append in transient buffers.

## Acceptance Criteria

The RFC is Accepted when ALL of the following are true:

- **AC-1.** `octo_settlement_core::Receipt` retains its 6-field public surface (no field additions; semver-major bump from v1.x to v2.0.0).
- **AC-2.** `octo_settlement_core::SettlementError` retains its 7-variant form (no new variants; existing `SinkSpecific` carries adapter-scrubbed strings only).
- **AC-3.** `compute_settlement_hash(&Receipt) == BLAKE3-256-keyed([0;32], CHAIN_DOMAIN_SEPARATOR || receipt_id_be_u64 || ask_id || router_id_utf8 || router_sig || timestamp_unix_be_u64)` — verified by `verify_receipt_chain` round-trip.
- **AC-4.** Strict `receipt_id == last_receipt_id() + 1` enforced; re-append returns `AlreadyExists`; gap returns `SequenceGap`.
- **AC-5.** Tampered `settlement_hash` returns `ChainIntegrity { receipt_id }` (NOT `SinkSpecific`); substrate canonical invariant.
- **AC-6.** `ReceiptId(pub u64)` newtype re-exported at `octo_settlement::ReceiptId`; canonical hash unchanged.
- **AC-7.** Adapter implementations call `octo_settlement::scrub::scrub_adapter_error` before wrapping into `SinkSpecific`; raw error chains never reach substrate.
- **AC-8.** Paired acceptance with RFC-0012-v2 (2-cycle atomic promotion gate).
- **AC-9.** All Test Vectors in §Test Vectors produce expected outputs (verified by `cargo test -p octo-settlement`).
- **AC-10.** `StoolapReceiptSink` (or equivalent concrete impl) implements `AppendOnlyReceiptSink` with monotonicity + atomic persistence; no adapter panic on fsync failure (atomic-or-rollback contract).

## 2-Cycle Atomic Promotion Tag

Per BLUEPRINT.md §RFC Process item 5 + §2-Cycle Atomic Promotion gate:

- **Sibling:** RFC-0012-v2 (audit substrate amendment, draft)
- **Reviewer board:** 5-len (correctness / security / layer-model / hygiene / spec-completeness)
- **Pairing invariant:** §S7 cross-RFC pairing via `prev_chain_hash = receipt_id_for(receipt)` requires both substrate amendments to land together. RFC-0015-a + RFC-0016-a acceptance gated on this 2-cycle.
- **Atomic promotion gate:** Both RFCs transition Draft → Accepted in the same PR. Neither may be Accepted without the other.

## Security Considerations

**SC1. Typed-discriminator collision resistance.** Extension kinds are encoded as `BLAKE3-256(namespace_string || canonical_ask_id)`. BLAKE3-256 collision resistance is 2^128 operations (birthday bound on 256-bit output). Preimage resistance is 2^256. Cross-extension-kind collisions are cross-prefix second-preimage attacks (~2^256 with one fixed prefix; ~2^128 birthday for attacker-chosen both prefixes).

**SC2. `SinkSpecific` payload scrubbing.** Adapter-specific error messages MUST be scrubbed at the adapter boundary before wrapping into `SettlementError::SinkSpecific`. No raw error chains, no adapter-type names, no leaked path fragments. Canonical scrubber is declared at `octo-settlement::scrub::scrub_adapter_error` (Layer B façade per §S5.1 — 6-pattern list).

**SC3. Settlement hash integrity.** `settlement_hash` field MUST match the canonical computation (substrate-side `verify_receipt_chain` enforces). §S4.4 enforces this on every append; §S6 canonical-bytes form is the substrate-level guarantee.

**SC4. Receipt_id monotonicity.** Strict `receipt_id == last_receipt_id() + 1` requirement prevents both gaps and replay. Re-append returns `AlreadyExists` (NOT silent success) per §S4.3.

**SC5. Atomic persistence.** §S4.5 requires transaction-scoped atomic write. Adapter impls without atomic persistence (e.g. file append without fsync) MUST NOT satisfy this contract.

**SC6. Router signature verification.** Substrate does NOT verify `router_sig` (signature verification is layer B / domain concern per RFC-0014 §Security Considerations). Substrate only persists the bytes; verification happens at domain call boundary (e.g. `octo-settlement::verify_router_sig(receipt, router_pubkey)`).

## Adversary Analysis

**A1. Receipt chain tampering.** Adversary modifies a persisted `Receipt` (e.g. flips `router_id` bytes, modifies `settlement_hash`). `verify_receipt_chain` returns an error for any tampered receipt. Mitigated by §S6 canonical-bytes + §S4.4 settlement hash integrity check.

**A2. Replay attack.** Adversary re-submits last persisted receipt. §S4.3 returns `AlreadyExists`; no duplicate persistence. Adversary cannot bypass via different `receipt_id` because §S4.2 enforces strict monotonicity.

**A3. Sequence gap injection.** Adversary skips `receipt_id` (e.g. submits 5 after 3, skipping 4). §S4.2 returns `SequenceGap { receipt_id: 5, prev: 3 }`. Caller cannot inject gaps.

**A4. Typed-discriminator spoofing.** Adversary constructs receipt with `ask_id = BLAKE3-256("cipherocto/settlement/extension/ask-settled/v1/" || ask)` but with settlement_hash computed from non-canonical partial settlement data. Detection: substrate verifies settlement_hash via §S4.4; if settlement_hash mismatch, append fails. Cross-RFC: domain crate canonicalizes the settlement payload and includes it in `settlement_hash` input.

**A5. Adapter error chain leakage.** Adversary inspects `SettlementError::SinkSpecific(String)` payload to extract adapter-internal structure. Mitigated by §SC2 scrubbing at adapter boundary.

**A6. Atomic persistence failure.** Adapter impl persists receipt but fsync fails. §S4.5 requires transaction-scoped atomic write + fsync; adapter impl without fsync fails SC5 conformance.

**A7. Router signature forgery.** Adversary constructs receipt with valid `receipt_id` (monotonic), valid `settlement_hash` (canonical computation), but forged `router_sig`. Substrate accepts (signature verification is domain concern per SC6). Detection: domain crate calls `verify_router_sig(receipt, router_pubkey)` before treating receipt as authoritative; fails closed.

## Economic Analysis

**E1. Substrate storage cost.** Typed-discriminator pattern reuses existing `ask_id` field. No substrate storage cost increase. Extension payload is encoded in domain-crate-side canonical hash inputs and surfaced via façade projection.

**E2. Adapter implementation cost.** Existing `StoolapReceiptSink` requires no changes for v2.0.0 conformance. §S4 invariants are already substrate-level.

**E3. Domain crate migration cost.** Existing callers that construct `Receipt` directly (e.g. `octo-settlement-core::ask`) must migrate to typed-discriminator helpers for new extension kinds. RFC-0016-a + RFC-0015-a pin the helper signatures.

**E4. `ReceiptId` newtype cost.** Façade-level type-safety improvement at zero substrate cost (newtype wraps existing `u64` field; canonical hash unchanged).

## Compatibility

**C1. Wire format.** `canonical_bytes` unchanged. Existing persisted receipt chains remain verifiable via `verify_receipt_chain`.

**C2. Source compatibility.** `Receipt` retains its 6-field public surface. `SettlementError` retains its 7-variant form. New `ReceiptId` newtype is additive.

**C3. Adapter compatibility.** Existing `StoolapReceiptSink` impl remains RFC-0014-v2 conformant. No adapter rewrite required.

**C4. Crate version.** `octo-settlement-core` advances from v1.x to v2.0.0 (semver-major). Per CLAUDE.md §Layer A stability rules, this is a one-time major bump for the typed-discriminator + sink invariant codification.

## Test Vectors

### TV-SET-v2-1: Typed-discriminator construction

```
input: extension_kind = "ask-partial", canonical_ask_id = "0x..."
expect: Receipt { ask_id: BLAKE3-256("cipherocto/settlement/extension/ask-partial/v1/" || canonical_ask_id), ... }
```

### TV-SET-v2-2: ReceiptId newtype round-trip

```
input: receipt.receipt_id = 42
expect: ReceiptId::new(42).get() == 42
        ReceiptId::new(42) == ReceiptId::new(42)
        ReceiptId::new(42) != ReceiptId::new(43)
```

### TV-SET-v2-3: Sink append accepts canonical settlement_hash

```
input: receipt with receipt.settlement_hash = compute_settlement_hash(&receipt)
expect: append returns Ok(())
        last_receipt_id() returns Some(receipt.receipt_id)
```

### TV-SET-v2-4: Sink append rejects tampered settlement_hash

```
input: receipt with receipt.settlement_hash = [0x00; 32] (not equal to compute_settlement_hash)
expect: append returns Err(SettlementError::ChainIntegrity { receipt_id: receipt.receipt_id })
```

### TV-SET-v2-5: Sink append rejects sequence gap

```
input: append receipt_id=1, then receipt_id=3 (skipping 2)
expect: first append returns Ok(())
        second append returns Err(SettlementError::SequenceGap { receipt_id: 3, prev: 1 })
```

### TV-SET-v2-6: Sink append rejects re-append (idempotency)

```
input: append receipt_id=1, then append receipt_id=1 again
expect: first append returns Ok(())
        second append returns Err(SettlementError::AlreadyExists(1))
```

### TV-SET-v2-7: Cross-RFC pairing (agent transition receipt ↔ audit event)

```
input: canonical_ask_id_bytes = "did:example:agent/123" (UTF-8 bytes)
       receipt.ask_id = BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/" || canonical_ask_id_bytes)
expect: audit_event_for_agent_transition_receipt(receipt).cap_root_hash ==
        BLAKE3-256("cipherocto/audit/extension/agent-transition/v1/")
        audit_event_for_agent_transition_receipt(receipt).event_kind == AuditEventKind::Insert
```

### TV-SET-v2-8: Typed-discriminator construction (ask-rejected)

```
input: extension_kind = "ask-rejected", canonical_ask_id_bytes = "did:example:ask/456"
expect: Receipt { ask_id: BLAKE3-256("cipherocto/settlement/extension/ask-rejected/v1/" || canonical_ask_id_bytes), ... }
```

### TV-SET-v2-9: Typed-discriminator construction (agent-transition-receipt)

```
input: extension_kind = "agent-transition-receipt", canonical_ask_id_bytes = "did:example:agent/789"
expect: Receipt { ask_id: BLAKE3-256("cipherocto/settlement/extension/agent-transition-receipt/v1/" || canonical_ask_id_bytes), ... }
```

### TV-SET-v2-10: Sink append atomic persistence

```
input: receipt with receipt.settlement_hash = receipt_id_for(&receipt)
       simulated crash injected mid-transaction (Stoolap Transaction wrapper abandoned)
expect: post-recovery last_receipt_id() returns None OR Some(prev_receipt_id)
        (NOT Some(receipt.receipt_id) — partial persistence must not be observable)
```

### TV-SET-v2-11: ask_id namespace || ask_id_bytes boundary

```
input: ask_id = BLAKE3-256("cipherocto/settlement/extension/ask-partial/v1/" || canonical_ask_id_bytes)
expect: 32-byte digest; namespace prefix recovered by enumeration
        (variable-length input; no fixed 32-byte boundary between namespace and ask_id bytes)
```

### TV-SET-v2-12: ReceiptId const fn round-trip

```
input: ReceiptId::new(42)
expect: ReceiptId::new(42).get() == 42
        ReceiptId::new(42) == ReceiptId::new(42)
        ReceiptId::new(42) != ReceiptId::new(43)
```

### TV-SET-v2-13: Receipt canonical_bytes round-trip

```
input: Receipt { receipt_id: 1, ask_id: [0; 32], settlement_hash: [0; 32], router_id: "r1", router_sig: "sig", timestamp_unix: 1700000000 }
expect: canonical_bytes(&receipt) yields deterministic byte sequence
        compute_settlement_hash(&receipt) is stable across calls
```

### TV-SET-v2-14: verify_receipt_chain accepts canonical chain

```
input: persist receipts 1, 2, 3 with settlement_hash = compute_settlement_hash(&receipt) for each
expect: verify_receipt_chain returns Ok(())
        all chain links validated
```

### TV-SET-v2-15: verify_receipt_chain rejects tampered settlement_hash

```
input: persist receipt with settlement_hash = compute_settlement_hash(&receipt)
       flip one byte in settlement_hash field after persistence
expect: verify_receipt_chain returns Err(ChainIntegrity { receipt_id })
```

### TV-SET-v2-16: SequenceGap at receipt_id boundary (1, 3 skip 2)

```
input: append receipt_id=1, then receipt_id=3
expect: first Ok(())
        second returns Err(SettlementError::SequenceGap { receipt_id: 3, prev: 1 })
```

### TV-SET-v2-17: Replay (idempotency) at receipt_id 1

```
input: append receipt_id=1, then append receipt_id=1 again
expect: first Ok(())
        second returns Err(SettlementError::AlreadyExists(1))
```

### TV-SET-v2-18: Cross-RFC pairing AskRejected + Redaction (§S7 §Table C)

```
input: audit event with cap_root_hash = BLAKE3-256("cipherocto/audit/extension/redaction/v1/")
       paired with receipt having ask_id = BLAKE3-256("cipherocto/settlement/extension/ask-rejected/v1/" || canonical_ask_id)
expect: audit_event_for_ask_rejected(receipt) returns AuditEvent with cap_root_hash matching above
        prev_chain_hash = receipt_id_for(receipt) (cross-RFC binding via §S7 pairing)
```

### TV-SET-v2-19: scrub_adapter_error all 6 patterns (already covered in RFC-0012-v2 TV-AUD-v2-18 through v2-23)

```
input: same scrubber tests as RFC-0012-v2 (canonical Layer B scrubber applies to both AuditError::SinkSpecific and SettlementError::SinkSpecific)
expect: identical pattern coverage
```

### TV-SET-v2-20: StoolapReceiptSink concurrent receipt_id collision detection

```
input: two appenders simultaneously call append(receipt_id=1) on empty table
expect: at most one returns Ok(())
        the other returns Err(SettlementError::SequenceGap) or Err(SettlementError::AlreadyExists)
```

### TV-SET-v2-21: Router signature forge attempt (SC6 domain-only verification)

```
input: receipt with valid receipt_id, valid settlement_hash, but forged router_sig
expect: substrate append returns Ok(())
        (substrate does NOT verify router_sig per SC6)
        domain caller MUST invoke verify_router_sig(receipt, router_pubkey) at the wire boundary
```

### TV-SET-v2-22: SettlementError::SinkSpecific payload byte cap

```
input: SinkSpecific("a".repeat(10000)) (10 KiB adapter error)
expect: payload retained verbatim (substrate does NOT truncate)
        adapter-side scrub_adapter_error MUST be called before wrapping
```

### TV-SET-v2-23: Receipt 6-field surface preservation

```
input: Receipt { receipt_id, ask_id, settlement_hash, router_id, router_sig, timestamp_unix }
expect: 6 fields total; no field additions allowed at v2.0.0
        semver-major bump required for any field addition
```

### TV-SET-v2-24: SettlementError 7-variant surface preservation

```
input: SettlementError { AskNotFound, AlreadyConsumed, InvalidTransition, SequenceGap, ChainIntegrity, AlreadyExists, SinkSpecific }
expect: 7 variants total; no new variants allowed at v2.0.0
        existing SinkSpecific carries adapter-scrubbed strings only
```

### TV-SET-v2-25: keyed BLAKE3 zero key determinism

```
input: compute_settlement_hash(&receipt) called on two replicas with identical receipt bytes
expect: identical 32-byte digest output
        (zero key [0; 32] intentional per A1; no key management required for cross-replica consensus)
```

### TV-SET-v2-26: cross-prefix second-preimage resistance

```
input: known ask_id = BLAKE3-256("cipherocto/settlement/extension/ask-partial/v1/" || ask)
       adversary attempts to find ask_id' with different extension kind namespace that produces same digest
expect: ~2^256 operations required (one fixed prefix)
        ~2^128 birthday for attacker-chosen both prefixes
```

### TV-SET-v2-27: timestamp_unix wall-clock monotonicity precondition

```
input: append receipt with timestamp_unix = 1700000000
       then append receipt with timestamp_unix = 1699999999 (1 second earlier)
expect: substrate enforces strict timestamp monotonicity
        (per A5; second append returns Err(SettlementError::SequenceGap) or similar)
```

### TV-SET-v2-28: Receipt chain O(n) verification

```
input: persist N receipts with valid settlement_hash each
expect: verify_receipt_chain completes in O(N) time
        no quadratic scans (per Performance Targets)
```

### TV-SET-v2-29: ReceiptId newtype zero runtime overhead

```
input: ReceiptId::new(42) vs raw u64 = 42
expect: identical machine code generated by `cargo build --release`
        newtype is zero-cost abstraction
```

### TV-SET-v2-30: Adapter atomic persistence precondition

```
input: StoolapReceiptSink append in Stoolap Transaction wrapper
       fsync fails at adapter boundary
expect: adapter rolls back Transaction (atomic-or-rollback contract per AC-10)
        no partial persistence observable to subsequent last_receipt_id() calls
```

## Alternatives Considered

**Alt-A: Add `ReceiptStatus` as substrate field.** Rejected per CLAUDE.md §Extension over enumeration + §Substrate-faithful — substrate-frozen struct cannot grow fields. §S1 typed-discriminator via `ask_id` namespace achieves extension semantics without field additions.

**Alt-B: Add `model`, `cost_dqa`, `capability_root`, `subject_did` as `Receipt` fields.** Rejected per §S2 explicit prohibition — adding fields forces semver-major + data migration of every existing persisted receipt.

**Alt-C: Add `ReceiptSummary` projection at substrate.** Rejected — projection structs are façade concerns (Layer B). Substrate stays free of projection logic.

**Alt-D: Add `ReceiptStatus` enum at substrate.** Rejected — status is recovered from `ask_id` typed-discriminator at façade boundary; substrate stays free of status interpretation logic.

**Alt-E: Pin §S4 invariants at adapter layer only.** Rejected — substrate-level invariants ensure adapter impls cannot accidentally weaken the contract. Adapter-level enforcement is unenforced.

## Implementation Phases

### Phase 1: Substrate `octo-settlement-core` v2.0.0 release

- Bump crate version to 2.0.0
- Add §S1 extension kind table to substrate doc-comment
- Add §S2 `Receipt` field boundary doc-comment
- Add §S3 `ReceiptId` newtype (additive — does NOT change `Receipt.receipt_id: u64`)
- Add §S4 invariant doc-comments to `AppendOnlyReceiptSink` trait
- Add §S5 `SettlementError` variant boundary doc-comment
- Add §S6 canonical-bytes form doc-comment
- Add §S7 cross-RFC consistency doc-comment

### Phase 2: Façade `octo-settlement` v1.x → v2.0.0 release

- Bump crate version to 2.0.0
- Add `octo-settlement-core` v2.0.0 dep
- Re-export `ReceiptId` newtype
- Add typed-discriminator helper functions at façade root (e.g. `receipt_for_ask_partial(...)`, `receipt_for_agent_transition_receipt(...)`)
- Add façade-side `ReceiptStatus` enum (recovered from `ask_id` typed-discriminator)
- Add façade-side `ReceiptSummary` projection struct

### Phase 3: Adapter `StoolapReceiptSink` conformance verification

- Verify existing adapter impl satisfies §S4.5 atomic persistence
- Verify existing adapter impl scrubs errors per §SC2
- Add test vectors TV-SET-v2-3 through TV-SET-v2-6

### Phase 4: Cross-crate migration

- RFC-0016-a amendment acceptance depends on Phase 1+2+3 completion
- RFC-0015-a amendment acceptance depends on Phase 1+2+3 completion

## Key Files to Modify

- `crates/octo-settlement-core/src/receipt.rs` — add `ReceiptId` newtype (§S3) + doc-comment updates for §S1 + §S2
- `crates/octo-settlement-core/src/sink.rs` — doc-comment updates for §S4 invariants
- `crates/octo-settlement-core/src/error.rs` — doc-comment updates for §S5 variant boundary
- `crates/octo-settlement-core/src/chain.rs` — doc-comment updates for §S6 canonical-bytes form
- `crates/octo-settlement-core/Cargo.toml` — version bump 1.x → 2.0.0
- `crates/octo-settlement/src/lib.rs` — version bump + `ReceiptStatus` + `ReceiptSummary` projection
- `crates/octo-settlement/Cargo.toml` — `octo-settlement-core` dep bump to 2.0.0

## Future Work

**FW1. RFC-0014-v3.** Subsequent typed-discriminator extensions (e.g. AskRefunded, AskPartialSupplement, AskSettledWithRebate) follow the §S1 pattern. Each extension kind is added to the table via a new RFC; no substrate field additions.

**FW2. Cross-crate extension helpers.** Domain crates (e.g. `octo-wallet`, `octo-vault`) provide typed-discriminator construction helpers (`receipt_for_ask_settled`, `receipt_for_ask_partial`). Façade `octo-settlement` re-exports for cross-crate use.

**FW3. ReceiptStatus state machine.** Façade-side `ReceiptStatus` enum + transition rules (e.g. `Partial` → `Ok` after supplemental settlement). Future RFC pins the state machine; not in RFC-0014-v2 scope.

**FW4. Audit-receipt pairing helpers.** Cross-crate helper `audit_event_for_agent_transition_receipt(receipt) -> AuditEvent` pins the §S7 pairing invariant at the implementation level.

## Rationale

RFC-0014-v2 codifies the typed-discriminator extension pattern as the canonical substrate-faithful mechanism for new receipt semantics. This achieves three goals:

1. **Layer A frozen preservation** — `Receipt` stays 6-field; `SettlementError` stays 3-variant. No substrate churn from extension additions.
2. **Forward compatibility** — `#[non_exhaustive]` discipline means downstream consumers continue to compile (substrate-stored receipts remain forward-compatible).
3. **Type safety** — typed-discriminator namespaces are BLAKE3-256 digests (2^128 collision operations birthday bound; cross-prefix second-preimage attacks at ~2^256 with one fixed prefix, ~2^128 birthday for attacker-chosen both prefixes); cross-extension-kind spoofing requires BLAKE3 cross-prefix collision attack.

The `ReceiptId` newtype adds façade-level type safety without changing canonical substrate form. `ReceiptStatus` + `ReceiptSummary` move to façade (Layer B) where projection logic naturally lives.

The cost is a doc-comment-driven extension pattern that domain crates must follow. This is acceptable per CLAUDE.md §Extension over enumeration — the substrate stays minimal; extension mechanics live at the façade/domain boundary.

## Version History

| Version      | Date       | Author               | Notes                                                                                                                                                  |
| ------------ | ---------- | -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| v2.0.0-draft | 2026-09-11 | CipherOcto core team | Initial draft. Pins typed-discriminator pattern via `ask_id` namespaces, `ReceiptId` newtype, sink invariants, cross-RFC consistency with RFC-0012-v2. |

## Related RFCs

- RFC-0014 (accepted) — parent RFC; defines substrate `Receipt`, `Ask`, `Reservation`, `SettlementStore`, `AppendOnlyReceiptSink`, `SettlementError`.
- RFC-0012-v2 (draft) — sibling substrate amendment for audit extension pattern.
- RFC-0015-a (draft) — wallet agent write-path amendment; requires RFC-0014-v2 for `ReceiptId` + `ask_id` typed-discriminator.
- RFC-0016-a (draft) — audit receipt write-path amendment; requires RFC-0014-v2 for `ReceiptId` newtype + `ReceiptSummary` projection.
- RFC-0011-a (accepted) — wallet subcommands; cross-RFC reference for scrubber location (Layer B façade, not Layer A substrate).
- RFC-0959 (accepted) — settlement data structures + state machines (parent design).

## Related Use Cases

- **UC-SET-001 — Agent lifecycle settlement.** When an agent transitions state (RFC-0015-a `transition_agent`), a corresponding `AgentTransitionReceipt` is settled with `ask_id` typed-discriminator namespace `cipherocto/settlement/extension/agent-transition-receipt/v1/`. The audit event for the transition (RFC-0012-v2 `AuditEventKind::Insert` + cap_root_hash namespace) is paired with the receipt via `prev_chain_hash = receipt_id_for(receipt)`.
- **UC-SET-002 — Ask rejection.** When a downstream router rejects an ask (e.g. insufficient capacity), a corresponding `AskRejected` receipt is settled with `ask_id` namespace `cipherocto/settlement/extension/ask-rejected/v1/`. The audit event for the rejection (RFC-0012-v2 `AuditEventKind::Revoke` + Redaction namespace) is paired similarly.
- **UC-SET-003 — Partial settlement supplement.** When a settlement is partially filled and supplemented (e.g. multi-hop routing), a corresponding `AskPartial` receipt is settled with `ask_id` namespace `cipherocto/settlement/extension/ask-partial/v1/`. The partial supplemental amount is encoded in domain-crate-side canonical hash inputs; the substrate does NOT inspect the extension payload.
- **UC-SET-004 — Receipt projection at façade.** When a CLI consumer requests `octo audit list` or `octo audit show <receipt_id>` (RFC-0016-a), the façade projects the canonical `Receipt` into `ReceiptSummary` via typed-discriminator lookup on `ask_id`. The projection is read-only; substrate persistence is unchanged.

## Appendices

### Appendix A: Extension kind namespace registry

| Extension kind         | Namespace string                                                                                                                       | `ask_id` prefix bytes (BLAKE3-256 input prefix) |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| AskSettled             | `cipherocto/settlement/extension/ask-settled/v1/` (RFC-0014 existing; `ask_id` = digest of canonical ask only — namespace is implicit) | (RFC-0014 existing)                             |
| AskPartial             | `cipherocto/settlement/extension/ask-partial/v1/`                                                                                      | RFC-0014-v2 new                                 |
| AskRejected            | `cipherocto/settlement/extension/ask-rejected/v1/`                                                                                     | RFC-0014-v2 new                                 |
| AgentTransitionReceipt | `cipherocto/settlement/extension/agent-transition-receipt/v1/`                                                                         | RFC-0014-v2 new (cross-RFC with RFC-0015-a)     |

### Appendix B: Substrate-vs-façade boundary

| Concern                                | Substrate (Layer A)           | Façade (Layer B)                               |
| -------------------------------------- | ----------------------------- | ---------------------------------------------- |
| `Receipt` struct                       | owned                         | re-export                                      |
| `ReceiptId` newtype                    | owned                         | re-export                                      |
| `SettlementError` enum                 | owned                         | re-export                                      |
| `AppendOnlyReceiptSink` trait          | owned                         | re-export + concrete `StoolapReceiptSink` impl |
| `verify_receipt_chain`                 | owned                         | re-export                                      |
| `receipt_id_for`                       | owned                         | re-export                                      |
| `ReceiptStatus` enum                   | NOT owned (façade projection) | owned (Layer B projection)                     |
| `ReceiptSummary` projection            | NOT owned (façade projection) | owned (Layer B projection)                     |
| Typed-discriminator helpers            | doc-comment only              | owned (`receipt_for_*` functions)              |
| Storage adapter (`StoolapReceiptSink`) | NOT owned                     | owned (Layer B → Layer D adapter)              |

RFC-0014-v2 explicitly pins this boundary. Substrate stays free of projection logic, status interpretation, and storage adapter logic — all three are Layer B (or Layer D) concerns per CLAUDE.md §Layer direction rule.

### Appendix C: Cross-RFC consistency table

| RFC-0012-v2 audit event kind                                                   | RFC-0014-v2 receipt extension kind                                                        | Façade pairing helper                                             |
| ------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| `AgentTransition` (Insert + `cipherocto/audit/extension/agent-transition/v1/`) | `AgentTransitionReceipt` (`cipherocto/settlement/extension/agent-transition-receipt/v1/`) | `audit_event_for_agent_transition_receipt(receipt) -> AuditEvent` |
| `Redaction` (Revoke + `cipherocto/audit/extension/redaction/v1/`)              | `AskRejected` (`cipherocto/settlement/extension/ask-rejected/v1/`)                        | `audit_event_for_ask_rejected(receipt) -> AuditEvent`             |

Cross-RFC pairing rules:

- Audit event + receipt share a canonical hash input prefix (the extension namespace string).
- Façade-side helper functions recover the pairing invariant at the implementation level.
- Substrate stays free of cross-RFC pairing logic — both substrates accept opaque 32-byte digests.
