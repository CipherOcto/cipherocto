# RFC-0011-w: `octo network` — Slot 91 `NetworkKeyRotationUnknownId` Activation (G21 forward projection)

## Status

Accepted (2026-09-22) — RFC-0011-w activates the slot 91 pre-allocation from RFC-0011-h §Error Handling row 91 + §Exit Codes row 91 + §Substrate-Additions G21 row + [^rotation-error-slot-prealloc] footnote. ONE NEW `OctoCliError` variant (slot 91 mints) + ONE NEW `From<octo_runtime::AttachError>` translation arm (feature-gated per substrate `octo-attach-key-rotation` Cargo feature) + parent RFC-0011-h cross-reference updates. No new CLI subcommand. No substrate change. Paired with new amendment-chain companion mission YAML `0011-h-s-a-network-key-rotation-unknown-id` per [[no-phantom-mission-pointers]]. DRY CLOSURE gate GREEN at R4 (0/0/0/0 findings across 4 rounds) before implementation slice. Implementation slice landed via 4-commit pattern per Phase 5 RFC-0011-m 5-commit precedent: paired-YAML Claimed transition + Cargo.toml feature propagation at `next ef2251a4`, Layer C substrate additions at `next c230dc70`, parent RFC 4 cross-RFC reference updates + paired-YAML Completed transition at `next 31907573`, RFC promotion Draft → Accepted at this commit.

> **Amendment chain:** Fifteenth amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per [[docs-plans-scratchpad]]). Phase 1 = RFC-0011-i. Phase 2 = RFC-0011-j. Phase 3 = RFC-0011-k. Phase 4 = RFC-0011-l. Phase 5 = RFC-0011-m. Phase 6 = RFC-0011-n. Phase 7 = RFC-0011-o. Phase 8 = RFC-0011-p. Phase 9 = RFC-0011-q. Phase 10 = RFC-0011-r. Phase 11 = RFC-0011-s. Phase 12 = RFC-0011-t. Phase 13 = RFC-0011-u. Phase 14 = RFC-0011-v. Phase 15 = RFC-0011-w (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-w activates the slot 91 pre-allocation from RFC-0011-h §Error Handling row 91 + §Exit Codes row 91 + §Substrate-Additions G21 row + [^rotation-error-slot-prealloc] footnote by minting the `OctoCliError::NetworkKeyRotationUnknownId` variant at slot 91 in `crates/octo-cli/src/error.rs`.

The substrate `AttachError::UnknownKeyId` variant is already LANDED at `crates/octo-runtime/src/handle/error.rs` (gated on the `octo-attach-key-rotation` Cargo feature per RFC-0011-c §F.5.1 paired-acceptance bridge with RFC-0015-a §6.5). The amendment adds the translation arm in `From<octo_runtime::AttachError> for OctoCliError` (feature-gated to mirror the substrate's `#[cfg(feature = "octo-attach-key-rotation")]` attribute) and updates the parent RFC-0011-h cross-references to reflect slot 91 activation.

The variant mints unconditionally (slot 91 always exists in the enum per the pre-allocation contract). The translation arm fires only when the `octo-attach-key-rotation` feature is enabled in `octo-runtime`. When the feature is disabled (default `octo-cli` build), the substrate variant does not exist and the existing wildcard arm of `From<AttachError>` collapses any unknown variant to `Internal(reason)` exit 64 (additive-safe wildcard per `#[non_exhaustive]`).

## Dependencies

**Requires:**

- RFC-0011-h (parent RFC; slot 91 pre-allocation contract)
- RFC-0011-c (D2.1 paired-acceptance bridge §F.5.1; `AttachError::UnknownKeyId` substrate variant originates here)
- RFC-0011-l (Phase 4 substrate landing; G21 row substrate references)
- RFC-0015-a (§6.5 paired-acceptance bridge; `KeyId` discriminator surface)

**Optional:**

- RFC-0855p-b (paired with RFC-0011-c §F.5.1 via the D2.1 paired-acceptance bridge; not directly cited but in scope for the substrate lineage)

> **Dependency Validation Rules:**
>
> 1. Dependencies MUST form a DAG (no cycles)
> 2. All "Requires" RFCs MUST be listed as mission prerequisites
> 3. Optional dependencies MUST be documented separately from required
> 4. Dependencies on "Planned" RFCs MUST note the assumption they will be Accepted

## Design Goals

1. **G1 — Mint slot 91 variant.** Define `OctoCliError::NetworkKeyRotationUnknownId` at exit slot 91 in `crates/octo-cli/src/error.rs`. Variant signature mirrors the substrate `AttachError::UnknownKeyId { key_id: KeyId, known_keys: Vec<KeyId> }` with `key_id` redacted in operator-facing render per §Redaction Layer.
2. **G2 — Wire translation arm.** Add `From<octo_runtime::AttachError>` match arm mapping `AttachError::UnknownKeyId { key_id, known_keys }` → `Self::NetworkKeyRotationUnknownId { key_id_hex: hex::encode(key_id.to_be_bytes()), known_keys_band: KnownKeysBand::from_count(known_keys.len()) }`. Arm carries `#[cfg(feature = "octo-attach-key-rotation")]` to mirror the substrate's feature gating.
3. **G3 — Update exit-code table.** Add the `Self::NetworkKeyRotationUnknownId { .. } => 91` arm to `OctoCliError::exit_code` in `crates/octo-cli/src/error.rs`.
4. **G4 — Update parent RFC-0011-h cross-references.** Rewrite four locations in `rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md` to reflect slot 91 activation:
   - §Error Handling summary paragraph: "All 10 defined ... + slot 91 pre-allocated" → "All 11 defined ... (slot 91 activated per RFC-0011-w)"
   - §Exit Codes row 91: `(RESERVED)` → defined `NetworkKeyRotationUnknownId` with substrate reference
   - §Substrate-Additions G21 row right column: append slot 91 activation status
   - [^rotation-error-slot-prealloc] footnote: rewrite to reflect activation contract
5. **G5 — Preserve Layer A frozen contract.** Zero diff to `crates/octo-governance-core`, `crates/octo-audit-core`, `crates/octo-settlement-core` (real Layer A crates in scope per Phase 14 R2.5 lesson; no phantom `octo-vault-core` or `octo-wallet-core` cites).
6. **G6 — Preserve `#[non_exhaustive]` discipline.** The new variant lands additively on the existing `OctoCliError` enum without disturbing the parent RFC-0011-h §Error Handling row 545 summary arithmetic. Per RFC-0011-h G3 row (parent RFC §Substrate-Additions Companion Missions G3 row, line 62), the future-amendment band is slots 79-99 (21 slot positions total). Pre-amendment: 10 defined variants (slots 79, 82-90) + slots 80, 81 RESERVED + slot 91 PRE-ALLOCATED + slots 92-99 RESERVED = 10 + 2 + 1 + 8 = 21 slot positions. Post-amendment: 11 defined variants (slots 79, 82-91) + slots 80, 81 RESERVED + slots 92-99 RESERVED = 11 + 2 + 8 = 21 slot positions (band width unchanged per pre-allocation contract).
7. **G7 — Companion mission YAML pairing.** Pair the amendment with new `0011-h-s-a-network-key-rotation-unknown-id` mission YAML per [[no-phantom-mission-pointers]]. YAML lives in `missions/open/` as `Open` until paired with this RFC's DRY CLOSURE gate; transitions to `Completed` post-closure.
8. **G8 — Test vector coverage.** 4 minimum test vectors:
   - `tv_w_1` — variant construction + `exit_code()` returns 91 + `user_message()` renders the `#[error]` template with `key_id_hex` + `known_keys_band` interpolated per RFC-0011-h §Redaction Layer + `hint()` arm returns the prescribed operational remediation phrase per RFC-0011-c §F.5.1 paired-acceptance bridge (Rust unit test; `cargo test -p octo-cli --lib`)
   - `tv_w_1b` — `KnownKeysBand::from_count(usize)` boundary coverage: 0 → `None` (verifier misconfiguration sentinel), 1 → `Few` (band lower edge), 8 → `Few` (off-by-one at `1..=8` upper edge), 9 → `Many` (off-by-one at `>8` lower edge), 16 → `Many` (interior band value). All 3 quantization bands plus the 8-vs-9 boundary sentinel exercised (Rust unit test; `cargo test -p octo-cli --lib --features octo-attach-key-rotation`, feature-gated to mirror substrate `from_count` gating)
   - `tv_w_2` — `From<AttachError>::from(AttachError::UnknownKeyId)` returns `NetworkKeyRotationUnknownId` with redacted key_id hex (requires `octo-attach-key-rotation` feature enabled in test build)
   - `tv_w_3` — Cross-RFC reference integrity: shell grep verifies all 4 G4 edits landed at their target locations in the parent RFC (per §Test Vectors §`tv_w_3` row)

## Motivation

RFC-0011-h §Error Handling row 91 + §Exit Codes row 91 + §Substrate-Additions G21 row right column pre-allocate slot 91 for the `AttachError::UnknownKeyId` translation per [^rotation-error-slot-prealloc] footnote (the G21 row in §Substrate-Additions Companion Missions is `0011-h-s-a-rebind-coordinator`). The pre-allocation was a §Push complexity to edges decision: reserve the slot at Draft time so the post-G21 amendment only needs to mint the variant without disturbing other slot allocations.

The substrate `AttachError::UnknownKeyId { key_id: KeyId, known_keys: Vec<KeyId> }` is already LANDED at `crates/octo-runtime/src/handle/error.rs`, gated on the `octo-attach-key-rotation` Cargo feature per RFC-0011-c §F.5.1 paired-acceptance bridge with RFC-0015-a §6.5. The substrate returns the unknown `key_id` discriminator and a diagnostic union of active + grace `key_id`s in the verifier's `KeySet`.

Today (without this amendment), `AttachError::UnknownKeyId` falls through to the wildcard arm of `From<AttachError>` and surfaces as `Internal(reason)` exit 64 (additive-safe per `#[non_exhaustive]`). This collapses the typed-discriminator information: the operator sees a generic internal error rather than the dedicated `NetworkKeyRotationUnknownId` exit that maps to a typed remediation hint.

This amendment activates the pre-allocated slot by minting the variant + adding the translation arm. The variant mints unconditionally (so slot 91 always exists in the enum); the translation arm fires only when the substrate feature is enabled.

**Out of scope for this amendment:**

- CLI dispatch surface that forwards `AttachError::UnknownKeyId` to the variant. The current `bind-envelope rebind-{commit,abort}` dispatch arms (substrate `RebindCoordinator::commit_envelope` returns `Option<RebindCommit>` and `abort_envelope` returns `Option<RebindAbort>` per `crates/octo-network/src/mon/rebind.rs:170` + `:215`) — they do not yet route through `AttachError`. NOTE: `prepare_envelope` (substrate line 121) returns `RebindPrepare` (infallible), NOT `Option<_>`; the F1 substrate change applies only to `commit_envelope` + `abort_envelope`. A future amendment will add a CLI caller surface for the key-id translation path; that amendment mints slot 91 reachability (this amendment only mints the variant definition).
- D2.2 population policy. RFC-0011-l Phase 4 §Substrate-Additions G21 row notes "D2.2 population policy deferred post-PQC" — this amendment does NOT advance the D2.2 work item.
- Concrete `Handler` impls (Layer D per-extension crates) for the rebind-* trio. Per [[cipherocto-design-principles]] §User extensibility, concrete impls land in separate per-extension crates; OUT OF SCOPE here.

## Roles and Authorities

- **Operator**: receives the `NetworkKeyRotationUnknownId` exit 91 when invoking any `octo network` subcommand that surfaces an `AttachError::UnknownKeyId` (future caller). Reads the redacted key_id and the count of known keys in the verifier's `KeySet` from the operator envelope. Remediation: rotate the key via `octo network attach handle rotate` (if/when the D2.2 surface lands) or wait for the key to enter the grace-period window per RFC-0011-c §F.5.1.
- **Substrate (`octo-runtime::handle::error::AttachError::UnknownKeyId`)**: source of the typed-discriminator info. Lands at `crates/octo-runtime/src/handle/error.rs` per RFC-0011-c §F.5.1 paired-acceptance bridge.
- **CLI (`OctoCliError::NetworkKeyRotationUnknownId`)**: destination of the translation arm. Lands at slot 91 in `crates/octo-cli/src/error.rs` per this amendment.
- **Per-extension concrete impl crates** (Layer D): OUT OF SCOPE per [[cipherocto-design-principles]] §User extensibility; substrate trait types in Layer B; concrete per-transport / per-handler impl crates in Layer D, separate per-extension missions.

## Detailed Design

### Variant definition (Layer C — `crates/octo-cli/src/error.rs`)

```rust
/// Categorical band of known keys in the verifier's `KeySet` per
/// RFC-0011-h §Redaction Layer. 3-band quantization avoids leaking
/// exact verifier KeySet cardinality across operator boundaries
/// (per [[cipherocto-design-principles]] §Push complexity to edges:
/// redaction happens at the variant boundary, not at substrate).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownKeysBand {
    /// 0 keys in `KeySet` (verifier misconfiguration)
    None,
    /// 1-8 keys in `KeySet` (typical deployment; 1 active + 1-7 grace period)
    Few,
    /// >8 keys in `KeySet` (large deployment or post-rotation grace period)
    Many,
}

impl KnownKeysBand {
    /// Map exact `Vec::len()` to categorical band per §Redaction Layer.
    /// Pure function on `usize`; deterministic per RFC-0008 §Deterministic Constant.
    pub(crate) fn from_count(n: usize) -> Self {
        match n {
            0 => Self::None,
            1..=8 => Self::Few,
            _ => Self::Many,
        }
    }
}

/// `AttachError::UnknownKeyId` translation.
///
/// Substrate `octo_runtime::AttachError::UnknownKeyId` (LANDED at
/// `crates/octo-runtime/src/handle/error.rs` per RFC-0011-c
/// §F.5.1 paired-acceptance bridge; gated on `octo-attach-key-rotation`
/// Cargo feature). Surfaces when the token's `key_id` discriminator
/// is not present in the verifier's `KeySet` (neither in the active
/// lookup table nor in the grace-period window for the claimed
/// `key_id`; see RFC-0011-c §F.5.1 + RFC-0015-a §6.5). The variant
/// mints unconditionally at slot 91 per RFC-0011-w §Motivation; the
/// translation arm fires only when the substrate feature is enabled.
/// Pre-RFC-0011-w, this slot was RESERVED per [^rotation-error-slot-prealloc].
#[error("network key rotation: unknown key_id 0x{key_id_hex} (known_keys band: {known_keys_band:?})")]
NetworkKeyRotationUnknownId {
    /// `key_id` discriminator rendered as 8 hex chars. Substrate
    /// `KeyId = u32` (typed version discriminator for holder signing
    /// key per RFC-0011-c §F.5.1; NOT a Layer A cryptographic secret)
    /// hex-encoded big-endian via `key_id.to_be_bytes()`. Mirrors the
    /// established `hex::encode(declared)` / `hex::encode(actual)`
    /// pattern in the `SessionMismatch` translation arm.
    key_id_hex: String,
    /// Categorical band of known keys in the verifier's `KeySet`
    /// (active + grace period). 3-band quantization per §Redaction
    /// Layer: operator envelope renders the enum tag only (`None` /
    /// `Few` / `Many`) via the `#[error(... {known_keys_band:?})]`
    /// Debug formatter; the bucket thresholds (0 / 1-8 / >8) are
    /// implementation detail of `KnownKeysBand::from_count` (RFC-0011-w
    /// §Detailed Design §Variant block) and are NOT echoed in
    /// operator-facing render (would leak verifier state cardinality
    /// across operator boundaries).
    known_keys_band: KnownKeysBand,
},
```

The variant is defined unconditionally (no `#[cfg(...)]` attribute on the variant itself). This matches the pre-allocation contract: slot 91 always exists in the enum regardless of feature flag state.

### Translation arm (Layer C — `crates/octo-cli/src/error.rs` `From<octo_runtime::AttachError>`)

```rust
impl From<octo_runtime::AttachError> for OctoCliError {
    fn from(e: octo_runtime::AttachError) -> Self {
        match e {
            // ... existing arms (RFC-0011-c §F.4 + §9.8 slot 53-59 + 61) ...
            //
            // RFC-0011-w: slot 91 activation. Feature-gated to mirror
            // substrate `#[cfg(feature = "octo-attach-key-rotation")]`
            // on `AttachError::UnknownKeyId`. When the feature is
            // disabled, the substrate variant does not exist and the
            // existing wildcard arm below catches any unknown variant
            // → `Internal(reason)` exit 64 (additive-safe per
            // `#[non_exhaustive]`).
            #[cfg(feature = "octo-attach-key-rotation")]
            octo_runtime::AttachError::UnknownKeyId {
                key_id,
                known_keys,
            } => Self::NetworkKeyRotationUnknownId {
                key_id_hex: redact_key_id(&key_id),
                known_keys_band: KnownKeysBand::from_count(known_keys.len()),
            },
            // Additive-safe wildcard (existing). Future substrate variants
            // collapse to `Internal(reason)` exit 64.
            _ => Self::Internal(sanitize_substrate_error(&format!(
                "attach substrate error: {e}"
            ))),
        }
    }
}
```

The `redact_key_id` helper is a new private fn in `crates/octo-cli/src/error.rs` that hex-encodes the substrate `KeyId = u32` discriminator (8 hex chars via `hex::encode(key_id.to_be_bytes())`) per RFC-0011-h §Redaction Layer. This matches the existing `hex::encode(declared)` / `hex::encode(actual)` pattern in the `SessionMismatch` translation arm (which hex-encodes the 32-byte `SessionId`); per RFC-0011-c §F.5.1 the substrate `KeyId = u32` has different byte width but the same RFC-0008 §Deterministic Encoding contract applies (big-endian byte order).

### Exit-code arm (Layer C — `crates/octo-cli/src/error.rs` `OctoCliError::exit_code`)

```rust
impl OctoCliError {
    pub fn exit_code(&self) -> u8 {
        match self {
            // ... existing arms ...
            // RFC-0011-w: slot 91 activation per [^rotation-error-slot-prealloc].
            // Variant mints unconditionally; reachable when the
            // `octo-attach-key-rotation` feature is enabled in
            // `octo-runtime` AND a CLI caller surfaces the
            // `AttachError::UnknownKeyId` translation path (future
            // amendment per §Future Work F1).
            Self::NetworkKeyRotationUnknownId { .. } => 91,
        }
    }
}
```

The exit-code arm is unconditional (no `#[cfg(...)]` attribute) — the variant always returns 91 regardless of feature flag state.

### Cross-RFC reference updates (parent RFC text-only edits)

Four edits to `rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md`. The parent RFC remains Accepted; the edits are amendment-level review per docs/BLUEPRINT.md §RFC Process.

**Edit 1 — §Error Handling summary paragraph** (parent RFC §Error Handling summary paragraph; this paragraph is the §Error Handling section's prose summary that mentions "All 10 defined ... + slot 91 pre-allocated per G21" — the table itself has no row 91, the §Exit Codes table is the canonical row-91 location; per the substrate-faithfulness audit at parent RFC §Key Files to Modify the §Error Handling table ends at row 90):

Before:

> All 10 defined `OctoCliError` variants + slot 91 pre-allocated per G21 `0011-h-s-a-rebind-coordinator` companion + slots 80 + 81 RESERVED (no variant at Draft per [^slot80-overflow-rationale] + [^slot81-invalid-variant-rationale]); slot 89 carries the defined variant `NetworkSubstrateUnavailable` (not a slot reservation, gated on companion landing per §Substrate-Additions G1, G3b, G6, G6b, G8, G12, G12b, G22, G23, G24 row set).

After:

> All 11 defined `OctoCliError` variants (slot 91 activated per RFC-0011-w; variant count increment: `NetworkKeyRotationUnknownId` mints unconditionally per pre-allocation contract) + slots 80 + 81 RESERVED (no variant at Draft per [^slot80-overflow-rationale] + [^slot81-invalid-variant-rationale]); slot 89 carries the defined variant `NetworkSubstrateUnavailable` (not a slot reservation, gated on companion landing per §Substrate-Additions G1, G3b, G6, G6b, G8, G12, G12b, G22, G23, G24 row set).

The §Error Handling table itself has no row 91 — only the §Exit Codes table contains the row 91 entry, which is updated per Edit 2 below. The summary paragraph is the canonical §Error Handling reference for the slot 91 pre-allocation → activation transition.

**Edit 2 — §Exit Codes row 91** (§Exit Codes row 91):

Before:

> | 91 | `NetworkKeyRotationUnknownId` (RESERVED) | Pre-allocated per G21 `0011-h-s-a-rebind-coordinator` companion for substrate `AttachError::UnknownKeyId` (octo-runtime/src/handle/error.rs) translation (forward projection per RFC-0011-c §F.5.1). Variant lands post-G21 per [^rotation-error-slot-prealloc] |

After:

> | 91 | `NetworkKeyRotationUnknownId` | Substrate `AttachError::UnknownKeyId` (octo-runtime/src/handle/error.rs) translation per RFC-0011-c §F.5.1 paired-acceptance bridge. Feature-gated translation arm in `From<octo_runtime::AttachError>` for `OctoCliError`; variant mints unconditionally per RFC-0011-w §Motivation. Reachability: requires `octo-attach-key-rotation` Cargo feature enabled in `octo-runtime` AND a CLI caller surface for the key-id translation path (future amendment per RFC-0011-w §Future Work F1). Until F1 lands, variant sits unused in the `#[non_exhaustive]` enum (additive-safe per [[cipherocto-design-principles]] §Extension over enumeration) |

**Edit 3 — §Substrate-Additions G21 row right column** (§Substrate-Additions G21 row right column):

Before:

> | G21 | `0011-h-s-a-rebind-coordinator` | `RebindCoordinator` struct + `prepare_envelope(signature)` + `commit_envelope(signature)` + `abort_envelope(signature)` payload builders at `crates/octo-network/src/mon/rebind.rs` (RFC-0011-l Phase 4 §Substrate-Additions G21 + RFC-0011-c §F.5.1 D2.1 paired-acceptance bridge). Clap arm registration on rebind-* trio via `RebindArmAction` enum + `dispatch_rebind_arm_action(coordinator, arm, key)` helper at `crates/octo-network/src/mon/rebind_arm.rs`. D2.1 discriminator LANDED at `next 01340b93` (D2.2 population policy deferred post-PQC). Slot 91 pre-allocation for `AttachError::UnknownKeyId` translation (octo-runtime/src/handle/error.rs) carried forward per [^rotation-error-slot-prealloc] (deferred-active until a CLI caller surfaces the key-id translation path). | `octo network bind-envelope rebind-{prepare,commit,abort}` (LANDED: clap arm gated pre-G21, exit 2 per [^clap-arm-gated]) |

After: append "Slot 91 activation per RFC-0011-w (variant mints unconditionally; translation arm feature-gated to `octo-attach-key-rotation` per substrate)." to the substrate description cell.

**Edit 4 — [^rotation-error-slot-prealloc] footnote** (§[^rotation-error-slot-prealloc] footnote):

Before:

> [^rotation-error-slot-prealloc]: Slot 91 `NetworkKeyRotationUnknownId` is RESERVED at Draft (no variant today) because substrate `AttachError::UnknownKeyId` (octo-runtime/src/handle/error.rs) from `0011-h-s-a-rebind-coordinator` companion (G21 forward projection) does not yet have a CLI caller. Pre-allocation avoids a runtime slot-arithmetic gap when the rebind-* payload builders (which forward through this error variant) land in post-G21. Per [[cipherocto-design-principles]] §Push complexity to edges, pre-allocating the slot at Draft time means the post-G21 amendment only needs to mint the `OctoCliError::NetworkKeyRotationUnknownId` variant without disturbing other slot allocations. Slot 91 stays RESERVED until the companion mission lands.

After:

> [^rotation-error-slot-prealloc]: Slot 91 `NetworkKeyRotationUnknownId` activated per RFC-0011-w. Substrate `AttachError::UnknownKeyId` (octo-runtime/src/handle/error.rs) per RFC-0011-c §F.5.1 paired-acceptance bridge (gated on `octo-attach-key-rotation` Cargo feature). Variant mints unconditionally in `crates/octo-cli/src/error.rs` at slot 91; translation arm in `From<octo_runtime::AttachError>` for `OctoCliError` carries `#[cfg(feature = "octo-attach-key-rotation")]` to mirror substrate gating. Variant reachability: requires the feature enabled AND a CLI caller surface for the key-id translation path (future amendment per RFC-0011-w §Future Work F1). Until F1 lands, the variant sits unused in the `#[non_exhaustive]` enum (additive-safe per [[cipherocto-design-principles]] §Extension over enumeration). Pre-allocation rationale (historical): per [[cipherocto-design-principles]] §Push complexity to edges, the slot was reserved at RFC-0011-h Draft time so the activation amendment only needed to mint the variant + translation arm without disturbing other slot allocations.

### Feature flag wiring

**Cargo.toml change (new):**

The `octo-attach-key-rotation` feature is defined on `crates/octo-runtime/Cargo.toml` (substrate side) but is NOT propagated to `crates/octo-cli/Cargo.toml`. Without propagation, the translation arm's `#[cfg(feature = "octo-attach-key-rotation")]` attribute never evaluates true in `octo-cli` builds — the substrate variant `AttachError::UnknownKeyId` does not exist in `octo-runtime` from `octo-cli`'s perspective, so the match arm fails to compile when the feature is enabled. Per the substrate-faithfulness audit at [[substrate-faithfulness-verification]] R7.5 lesson, this is a Cargo feature propagation requirement: feature flags do NOT transitively enable downstream (Cargo's optional-dependency model is per-crate).

The amendment adds the following declaration to `crates/octo-cli/Cargo.toml`:

```toml
[features]
# ... existing features ...
octo-attach-key-rotation = ["octo-runtime/octo-attach-key-rotation"]
```

The `octo-runtime` dep in `crates/octo-cli/Cargo.toml` is declared as a REQUIRED dependency (no `optional = true` field), so Cargo's `dep:` feature syntax (which requires the dep to be `optional = true`) is INAPPLICABLE here. The correct syntax is the plain `octo-runtime/octo-attach-key-rotation` clause, which activates the substrate feature on the existing required dep without re-declaring it as optional (per [[substrate-faithfulness-verification]] R3.A HIGH-1 finding; verified at `crates/octo-cli/Cargo.toml`).

After this change, downstream consumers can enable `octo-attach-key-rotation` on `octo-cli` (e.g., `cargo build -p octo-cli --features octo-attach-key-rotation`), which propagates to `octo-runtime` and surfaces the substrate variant + the translation arm. The default `octo-cli` build (no feature flag) compiles cleanly because the translation arm is excluded via `#[cfg]`.

**No new feature flag defined on `octo-cli`.** The amendment adds a feature PROPAGATION, not a feature DEFINITION. The single source of truth for the feature remains `crates/octo-runtime/Cargo.toml`.

**Test vector implications:**

`tv_w_2` (translation arm existence + correct mapping) requires `cargo test -p octo-cli --lib --features octo-attach-key-rotation` to exercise the translation arm. `cargo test -p octo-cli --lib` (default features) exercises `tv_w_1` (variant existence + exit code) and `tv_w_3` (cross-RFC reference integrity) but NOT `tv_w_2`. Both invocations are listed in §Implementation Phases Tasks 9 + 9a + 10 + 10a.

**Substrate-faithfulness audit:**

The translation arm signature mirrors the substrate (with redaction at the edge per [[cipherocto-design-principles]] §Push complexity to edges — redaction happens at the variant boundary, not at substrate):

- Substrate: `AttachError::UnknownKeyId { key_id: KeyId (= u32), known_keys: Vec<KeyId> }` at `crates/octo-runtime/src/handle/error.rs` per RFC-0011-c §F.5.1 paired-acceptance bridge (gated on `octo-attach-key-rotation` Cargo feature)
- CLI variant: `NetworkKeyRotationUnknownId { key_id_hex: String (8 hex chars via `hex::encode(key_id.to_be_bytes())`), known_keys_band: KnownKeysBand (3-band categorical quantization derived from `Vec::len()`) }`

The `KeyId` substrate type is `pub type KeyId = u32;` at `crates/octo-runtime/src/handle/key_id.rs` — a typed version discriminator for a holder signing key (Layer B substrate), NOT a Layer A cryptographic secret. Per RFC-0011-c §F.5.1, `u32` covers 4B key generations; 8 hex chars is the substrate-faithful representation.

The `known_keys_band` field redacts exact `Vec::len()` to 3 categorical bands per §Redaction Layer (avoids leaking verifier KeySet cardinality). The substrate's own `#[error("unknown key_id {key_id} (known: {known_keys:?})")]` Display template leaks both raw `key_id` and full `known_keys` Vec; the CLI translation arm intercepts BEFORE the substrate Display renders (via the `From<octo_runtime::AttachError>` match arm), so operator-facing render is contained at the CLI dispatch boundary.

Per [[substrate-faithfulness-verification]] R7.5 lesson, this is verified by direct grep against `crates/octo-runtime/src/handle/error.rs` substrate variant definition + `crates/octo-runtime/src/handle/key_id.rs` `KeyId` type.

### Lifecycle Requirements

No stateful actors in this RFC. The `OctoCliError::NetworkKeyRotationUnknownId` variant is a static enum tag added unconditionally to the existing `#[non_exhaustive] OctoCliError` enum; the translation arm is a pure static match arm in `From<octo_runtime::AttachError>`. Neither defines a state machine, a stateful lifecycle, or a transition table. The substrate `AttachError::UnknownKeyId` (LANDED) and the future CLI caller surface (F1) each carry their own lifecycles; this amendment is a typed-discriminator projection, not a stateful actor. Justification per docs/BLUEPRINT.md §Specification §Lifecycle Requirements (one-line justification permitted when the RFC has no stateful actors).

### Determinism Requirements

All operations in this amendment are deterministic per RFC-0008 Execution Class C. The variant mint (`pub enum OctoCliError` tag) is a compile-time addition; the exit-code arm (`Self::NetworkKeyRotationUnknownId { .. } => 91`) is a constant projection; the translation arm (`AttachError::UnknownKeyId { key_id, known_keys }` → `NetworkKeyRotationUnknownId { key_id_hex: hex::encode(key_id.to_be_bytes()), known_keys_band: KnownKeysBand::from_count(known_keys.len()) }`) uses deterministic operations (`hex::encode` per RFC-0008 Class C deterministic encoding with stable big-endian byte order; `KnownKeysBand::from_count` is a pure function on `Vec::len()` with deterministic match arms). No consensus-critical, no proof-forensic, no PII-bearing operations are introduced by this amendment. Substrate-faithfulness verified at RFC-0011-w Draft time per Appendix A.

### RFC-0008 Execution Class Mapping

| Operation                                                                                                                        | Class | Rationale                                                                                                                               |
| -------------------------------------------------------------------------------------------------------------------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `OctoCliError::NetworkKeyRotationUnknownId` variant construction                                                                 | C     | Static enum tag; compile-time addition; no runtime effect on consensus, proofs, or verification.                                        |
| `OctoCliError::exit_code()` for `NetworkKeyRotationUnknownId { .. }`                                                             | C     | Constant projection to exit 91; deterministic per RFC-0008 §Deterministic Constant.                                                     |
| `From<octo_runtime::AttachError> for OctoCliError` translation arm (`AttachError::UnknownKeyId` → `NetworkKeyRotationUnknownId`) | C     | Read-only projection from substrate typed-discriminator to operator-facing variant; no mutation, no signing, no cross-process boundary. |
| `redact_key_id(key_id: &KeyId) -> String` (private helper)                                                                       | C     | `hex::encode` is deterministic per RFC-0008; operator-facing render only; does not enter consensus or proof surfaces.                   |

All operations are Class C per docs/BLUEPRINT.md §Specification §RFC-0008 Execution Class Mapping fallback ("If an RFC has no consensus-critical operations, state 'All operations are Class C' explicitly"). The variant is reachable only when (a) the `octo-attach-key-rotation` Cargo feature is enabled in `octo-runtime`, AND (b) a CLI caller surface (F1 future amendment) routes `AttachError::UnknownKeyId` through the translation arm. Until F1 lands, the variant sits unused in the `#[non_exhaustive]` enum (additive-safe per [[cipherocto-design-principles]] §Extension over enumeration).

### Security Considerations

Per docs/BLUEPRINT.md §Specification §Security Considerations (REQUIRED for cryptographic-primitive RFCs AND for RFCs that introduce operator-facing error envelopes):

| Category               | Applicability | Mitigation                                                                                                                                                                                                                                                                                                 |
| ---------------------- | ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Consensus attacks      | N/A           | This RFC introduces a CLI error variant only; no consensus surface, no validator state, no fork-choice input.                                                                                                                                                                                              |
| Economic exploits      | N/A           | This RFC introduces a CLI error variant only; no token movement, no fee path, no staking/slashing surface.                                                                                                                                                                                                 |
| Proof forgery          | N/A           | This RFC introduces a CLI error variant only; no signature verification, no ZK proof surface, no attestation flow.                                                                                                                                                                                         |
| Replay attacks         | N/A           | This RFC introduces a CLI error variant only; no message-passing surface, no nonce tracking, no epoch-bound state.                                                                                                                                                                                         |
| Determinism violations | Applicable    | `hex::encode(key_id.to_be_bytes())` is deterministic per RFC-0008 §Deterministic Encoding (big-endian byte order is stable); `KnownKeysBand::from_count` is a pure function on `Vec::len()`; no `HashMap` iteration, no time-dependent or RNG-dependent operations in the variant construction or Display. |

**Additional security notes:**

- **Operator envelope redaction compliance.** The variant carries `key_id_hex` (8 hex chars) and `known_keys_band` (3-band enum). Neither leaks raw substrate `KeyId` (a public version discriminator, not a cryptographic secret) nor verifier KeySet cardinality (3-band quantization per §Redaction Layer). The substrate `AttachError::UnknownKeyId`'s own `#[error("unknown key_id {key_id} (known: {known_keys:?})")]` Display template leaks both fields, but the CLI translation arm intercepts BEFORE the substrate Display renders via `From<octo_runtime::AttachError>` match arm — operator-facing render is contained at the CLI dispatch boundary.
- **Hex-encoding is NOT redaction** for cryptographic material. Per `KeyId = u32` substrate type, hex-encoding preserves all 32 bits of entropy, but `KeyId` is a version discriminator (per RFC-0011-c §F.5.1) and is NOT a Layer A cryptographic secret. The 8-hex-char representation is reversible but not sensitive. If `KeyId` evolves to opaque bytes post-PQC (per RFC-0011-c §F.5.1 migration notes), redaction review required (F3 future amendment).
- **Variant reachability defense.** `tv_w_1` (variant existence + `exit_code() == 91` + `user_message()` template interpolation) is exercised as a Rust unit test in default `octo-cli` builds (`cargo test -p octo-cli --lib`, no feature flag). `tv_w_3` (cross-RFC reference integrity via 4 per-edit shell greps per §Test Vectors `tv_w_3` row) is exercised as a shell grep in default builds (no feature flag required; runs against the parent RFC file directly). `tv_w_2` (translation arm signature) requires `cargo test -p octo-cli --lib --features octo-attach-key-rotation` per §Detailed Design §Feature flag wiring.
- **Substrate Display leakage containment.** The substrate `AttachError::UnknownKeyId` Display leaks verifier state. The CLI translation arm intercepts this variant BEFORE the substrate Display renders. If a future amendment adds a CLI caller surface that bypasses the translation arm (e.g., direct substrate error logging), the substrate Display leak MUST be addressed at the substrate level — out of scope for this amendment (substrate is owned by RFC-0011-c §F.5.1).

### Adversary Analysis

Per docs/BLUEPRINT.md §Specification §Adversary Analysis (REQUIRED for cryptographic-primitive RFCs AND for RFCs that introduce operator-facing error envelopes). 5-Question Test applied to each design decision:

#### Q1 — Variant field types: substrate-faithful redaction at the variant boundary

| Question       | Answer                                                                                                                                                                                                                                 |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who benefits?  | Operators running `octo network` subcommands that surface `AttachError::UnknownKeyId` (future F1 caller). They get an actionable diagnostic (8 hex chars + 3-band count) without cryptographic-secret leakage.                         |
| What cost?     | Translation arm performs `hex::encode(key_id.to_be_bytes())` + `KnownKeysBand::from_count(known_keys.len())` per unknown-key-id event. Trivial cost (microseconds; deterministic per RFC-0008 §Deterministic Constant).                |
| What gain?     | Operator can grep for `key_id_hex` across multiple invocations to correlate rotation events; 3-band count gives useful diagnostic without leaking verifier cardinality.                                                                |
| Defense cost?  | `KeyId = u32` is a typed version discriminator (Layer B substrate, not Layer A cryptographic secret per RFC-0011-c §F.5.1). Hex-encoding is reversible but not sensitive. `KnownKeysBand` 3-band quantization is the actual redaction. |
| Residual risk? | If `KeyId` evolves to opaque bytes post-PQC, hex-encoding may need additional redaction review (F3 future amendment).                                                                                                                  |

#### Q2 — Feature gating strategy: `#[cfg(feature = "octo-attach-key-rotation")]` on translation arm + Cargo.toml propagation

| Question       | Answer                                                                                                                                                                                                                                                                                                                                            |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who benefits?  | Cargo build system: feature flags propagate predictably per-crate. Default `octo-cli` builds don't pay the cost of compiling the translation arm when the substrate feature is disabled.                                                                                                                                                          |
| What cost?     | One-line addition to `crates/octo-cli/Cargo.toml` (feature propagation). The translation arm is dead code in default builds until F1 caller lands.                                                                                                                                                                                                |
| What gain?     | Compilation correctness across feature states; mirrors substrate's `#[cfg]` attribute pattern; no `cfg_if!` macro dance; substrate-faithful gating.                                                                                                                                                                                               |
| Defense cost?  | Documentation: §Detailed Design §Feature flag wiring + §Test Vectors (tv_w_2 requires `--features octo-attach-key-rotation`); §Implementation Phases Tasks 9 + 9a + 10 + 10a.                                                                                                                                                                     |
| Residual risk? | If a downstream consumer enables `octo-attach-key-rotation` on `octo-cli` without realizing the substrate also requires the feature, compilation fails with E0599 (no matching variant for `AttachError::UnknownKeyId`). This is fail-closed behavior (acceptable per [[cipherocto-design-principles]] §Attenuation invariants cross boundaries). |

#### Q3 — Slot 91 selection: pre-allocated per G21 + [^rotation-error-slot-prealloc] footnote

| Question       | Answer                                                                                                                                                                                                                                                                                                                             |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who benefits?  | Future F1 caller amendment: gets a stable slot for the new variant without disturbing existing slot arithmetic.                                                                                                                                                                                                                    |
| What cost?     | Slot 91 is RESERVED in parent RFC-0011-h §Error Handling row 545 + §Exit Codes row 91 + [^rotation-error-slot-prealloc] footnote at Draft time (slot reserved but unused).                                                                                                                                                         |
| What gain?     | Stable slot arithmetic across pre- and post-amendment states (21 slot positions in band 79-99 unchanged per pre-allocation contract).                                                                                                                                                                                              |
| Defense cost?  | One-line addition to slot arithmetic summary at §Appendix C. Cross-reference updates at parent RFC §Error Handling row 545 + §Exit Codes row 91 + [^rotation-error-slot-prealloc] footnote per §Detailed Design §Cross-RFC reference updates.                                                                                      |
| Residual risk? | None: the variant is unconditionally referenced in the `OctoCliError::exit_code` arm (§Detailed Design §Exit-code arm) which fires on the variant instance, so the variant is reachable in default builds even when the translation arm is feature-gated and F1 has not yet landed. No `dead_code` clippy lint suppression needed. |

#### Q4 — Hex encoding (8 chars for u32): mirrors SessionMismatch pattern

| Question       | Answer                                                                                                                                                                                                                                                                |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who benefits?  | Operator grep-ability: 8 hex chars are stable across invocations (deterministic per RFC-0008 §Deterministic Encoding); consistent with the existing `SessionMismatch` translation arm pattern (which hex-encodes 32-byte `SessionId`); predictable diagnostic output. |
| What cost?     | None (free per RFC-0008 §Deterministic Constant).                                                                                                                                                                                                                     |
| What gain?     | Operator envelope is greppable across multiple invocations for correlation. Mirrors established SessionMismatch pattern (cognitive consistency).                                                                                                                      |
| Defense cost?  | Documentation: §Detailed Design §Redaction helper + §Appendix A substrate-faithfulness audit (verifies `KeyId = u32` substrate-faithful typing per [[substrate-faithfulness-verification]] R7.5 lesson).                                                              |
| Residual risk? | Hex-encoding is NOT redaction for cryptographic secrets; but `KeyId = u32` is a version discriminator, not a cryptographic secret per RFC-0011-c §F.5.1.                                                                                                              |

#### Q5 — Categorical band (3 bands: None / Few / Many): reduces verifier state leakage

| Question       | Answer                                                                                                                                                                                                                                                                 |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who benefits?  | Operator: gets useful diagnostic ("verifier is mid-rotation, lots of keys") without exposing exact cardinality (which could be used to fingerprint the verifier across operator boundaries).                                                                           |
| What cost?     | Loses granularity (operator cannot tell if verifier has 10 vs 50 vs 100 keys).                                                                                                                                                                                         |
| What gain?     | §Redaction Layer compliance: categorical band is industry-standard redaction pattern (Stripe error codes, AWS error codes, GCP error codes all use coarse categorical bands).                                                                                          |
| Defense cost?  | `KnownKeysBand::from_count` is a 4-line pure function; trivial to implement and test. Band thresholds (0, 1-8, >8) chosen to match typical deployments (1 active + 1-7 grace period = Few; >8 indicates high-churn deployment = Many).                                 |
| Residual risk? | An adversary observing the band over time could fingerprint the verifier's rotation cadence (e.g., if band flips Few→Many→Few predictably, this leaks rotation timing). Acceptable: rotation timing is already inferable from epoch transitions per RFC-0011-c §F.5.1. |

#### Q6 — Variant mints unconditionally (no `#[cfg]`): stable slot arithmetic

| Question       | Answer                                                                                                                                                                                                                                                                                                   |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who benefits?  | Build system: variant always exists in the `#[non_exhaustive]` enum regardless of feature state. Slot 91 arithmetic is stable across feature states.                                                                                                                                                     |
| What cost?     | Variant exists in default builds but is unreachable (no translation arm fires without the feature).                                                                                                                                                                                                      |
| What gain?     | Pre-allocation contract preserved; no slot arithmetic gap; `#[non_exhaustive]` discipline preserved; downstream consumers can `match` on the variant unconditionally.                                                                                                                                    |
| Defense cost?  | Documentation: §Motivation + §Implementation Phases Task 1 ("variant mints unconditionally").                                                                                                                                                                                                            |
| Residual risk? | None: the variant is unconditionally referenced in the `OctoCliError::exit_code` arm (§Detailed Design §Exit-code arm) which fires on the variant instance, so the variant is reachable in default builds even when the translation arm is feature-gated. No `dead_code` clippy lint suppression needed. |

#### Q7 — §Redaction Layer compliance: no raw key_id / known_keys leakage in operator envelope

| Question       | Answer                                                                                                                                                                                                                                                                                                          |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Who benefits?  | Log forwarding pipelines: operator envelopes are safe to forward to external logging services (Datadog, Splunk, etc.) without exposing raw cryptographic material or verifier state cardinality.                                                                                                                |
| What cost?     | Slightly more verbose Display (3-band enum renders as enum tag, not raw count).                                                                                                                                                                                                                                 |
| What gain?     | §Redaction Layer compliance; safe log forwarding; compliance posture matches RFC-0011-h §Redaction Layer parent RFC.                                                                                                                                                                                            |
| Defense cost?  | Documented in §Security Considerations + §Adversary Analysis Q5.                                                                                                                                                                                                                                                |
| Residual risk? | Hex-encoded u32 (8 chars) is reversible if attacker has access to the canonical bytes source. `KeyId` is derived from canonical bytes of the signing key per RFC-0011-c §F.5.1. An attacker with access to the canonical bytes source has the key already; the 8-char hex is redundant information. Acceptable. |

#### Summary

All 7 design decisions pass the 5-Question Test with documented costs, gains, defense strategies, and residual risks. No design decision requires revision.

### Companion mission YAML pairing

Per [[no-phantom-mission-pointers]], this amendment pairs with new mission YAML `0011-h-s-a-network-key-rotation-unknown-id.md` at `missions/open/`. The YAML documents:

- §Status header: `Open (2026-09-22)` at amendment Draft time
- §Substrate additions target: variant definition location + translation arm location + exit-code arm location
- §CLI dispatch surface: NONE (this amendment is variant-only; no new CLI subcommand)
- §Depends-on chain: RFC-0011-w (this RFC) + RFC-0011-h (parent pre-allocation) + RFC-0011-c §F.5.1 (paired-acceptance bridge) + RFC-0015-a §6.5 (KeyId discriminator surface)
- §Acceptance criteria: 3 test vectors per G8

YAML transitions: Open → Claimed (paired with amendment substrate slice landing — in this case, just the Rust code commit) → Completed (paired with amendment DRY CLOSURE gate). For amendment scope (variant + translation arm, no substrate change), the Claimed transition aligns with the Rust code commit and the Completed transition aligns with the DRY CLOSURE commit.

## Test Vectors

| ID        | Type                           | Substrate feature               | What it verifies                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| --------- | ------------------------------ | ------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tv_w_1`  | Variant existence              | OFF                             | `OctoCliError::NetworkKeyRotationUnknownId { key_id_hex: String, known_keys_band: KnownKeysBand }` constructs successfully; `exit_code()` returns `91`; `user_message()` renders the `#[error]` template with `key_id_hex` + `known_keys_band` interpolated per RFC-0011-h §Redaction Layer; `hint()` returns the prescribed operational remediation phrase per RFC-0011-c §F.5.1 paired-acceptance bridge (assertion added at R1.5.b per the R1.B MEDIUM finding closure).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `tv_w_1b` | Quantization boundary coverage | ON (`octo-attach-key-rotation`) | `KnownKeysBand::from_count(usize)` boundary assertions at 0 → `None` (verifier misconfiguration sentinel), 1 → `Few` (band lower edge), 8 → `Few` (off-by-one at `1..=8` upper edge), 9 → `Many` (off-by-one at `>8` lower edge), 16 → `Many` (interior band value). All 3 quantization bands plus the 8-vs-9 boundary sentinel exercised per R1.B HIGH finding closure (assertion added at R1.5.b).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `tv_w_2`  | Translation arm                | ON (`octo-attach-key-rotation`) | `From<octo_runtime::AttachError>::from(AttachError::UnknownKeyId { key_id, known_keys })` returns `Self::NetworkKeyRotationUnknownId { key_id_hex: hex::encode(key_id.to_be_bytes()), known_keys_band: KnownKeysBand::from_count(known_keys.len()) }`. Verifies 8-hex-char encoding matches RFC-0008 §Deterministic Encoding (big-endian `u32`); `known_keys_band` matches `Vec::len()` bucketing thresholds.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `tv_w_3`  | Cross-RFC reference integrity  | OFF                             | Shell-grep the parent RFC at `rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md`. The 4-edit list (Edit 1 §Error Handling summary paragraph; Edit 2 §Exit Codes row 91 table cell; Edit 3 §Substrate-Additions G21 row right column; Edit 4 [^rotation-error-slot-prealloc] footnote) does NOT clear the case-sensitive `slot 91.*RESERVED` regex hits at lines 62 (§Substrate-Additions G3 row, documents future-amendment band 79-99 with `slots 92-99 RESERVED`) and 801 ([^non-exhaustive-additive] footnote, documents `slot 91 forward-projected per G21 + slots 80/81 RESERVED`) — these two architectural statements remain semantically correct post-amendment and are out of scope for the 4-edit list per R2.B H1 finding. Line 545 (Edit 1) STILL matches post-amendment because the line still contains `slots 80 + 81 RESERVED` (a substring the regex captures despite slot 91 no longer being RESERVED on that line). The case-sensitive regex is therefore NOT a useful tv_w_3 signal. 4 per-edit greps verify each G4 §Cross-RFC reference update landed at the correct location: (a) Edit 1 — `grep -c 'All 11 defined' rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md` returns ≥1 hit (was 0 pre-amendment); `grep -c 'All 10 defined' rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md` returns 0 hits (was ≥1 pre-amendment). (b) Edit 2 — `grep -nE '\| 91 +\|.*NetworkKeyRotationUnknownId' rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md` returns ≥1 hit at the §Exit Codes row 91 cell (whitespace-tolerant because the row uses 5-space column alignment: ` | 91  | `per parent RFC §Exit Codes Markdown table convention) post-amendment contains`NetworkKeyRotationUnknownId`(not`(RESERVED)`); pre-amendment the line contained `(RESERVED)`. (c) Edit 3 — `grep -nE 'Slot 91 activated per RFC-0011-w' rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md`returns ≥1 hit post-amendment in the G21 row right column (pre-amendment returns 0 hits). (d) Edit 4 —`grep -nE 'Slot 91 is activated per RFC-0011-w' rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md` returns ≥1 hit at the [^rotation-error-slot-prealloc] footnote (the case-sensitive regex matches the "is activated per RFC-0011-w" wording per the parent RFC edit). |

## Alternatives Considered

| Approach                                                                 | Pros                                                                                                                                                                                                                    | Cons                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A — Mint variant + feature-gate translation arm (this RFC)**           | Additive-only; preserves `#[non_exhaustive]` discipline; mirrors substrate feature gating; variant always exists so slot 91 arithmetic is stable across feature states; no CLI dispatch surface required for activation | Translation arm is unreachable in default builds until F1 lands; CLI callers surface `AttachError::UnknownKeyId` only when the `octo-attach-key-rotation` feature is enabled                                                                                                                                                                                                                                                                                                                                                |
| **B — Mint variant + always-on translation arm + CLI dispatch surface**  | Variant becomes reachable immediately; full activation end-to-end                                                                                                                                                       | Requires CLI dispatch surface for the key-id translation path (not in scope today; rebind-* dispatch returns `Option` not `Result<_, AttachError>`); larger amendment scope; needs substrate change to make `commit_envelope` / `abort_envelope` return `AttachError` on key-id miss; violates [[cipherocto-design-principles]] §Separation of concerns (variant minting + dispatch surface are separate concerns)                                                                                                          |
| **C — Update RFC-0011-h directly to land the variant + translation arm** | Single-file RFC change; preserves amendment count                                                                                                                                                                       | Violates [[cipherocto-design-principles]] §Stable Abstractions Principle (RFC-0011-h is Accepted = stable spec); breaks the 14-phase amendment chain pattern; every downstream phase RFC (0011-i through -v) carries a "slot 91 pre-allocated" forward-looking note that becomes stale if the parent RFC changes; violates [[cipherocto-design-principles]] §Separation of concerns (substrate landing + variant minting are separate concerns; substrate landing was Phase 4 RFC-0011-l, variant minting is a NEW concern) |

## Implementation Phases

### Phase 1: Variant mint + translation arm + parent RFC cross-reference updates

- [ ] Task 1: Define `OctoCliError::NetworkKeyRotationUnknownId` variant + `KnownKeysBand` enum in `crates/octo-cli/src/error.rs` (per §Detailed Design variant block)
- [ ] Task 2: Add `Self::NetworkKeyRotationUnknownId { .. } => 91` arm to `OctoCliError::exit_code` (unconditional; no `#[cfg]`)
- [ ] Task 3: Add `#[cfg(feature = "octo-attach-key-rotation")]` `AttachError::UnknownKeyId` translation arm in `From<octo_runtime::AttachError>` mapping to variant with `redact_key_id(&key_id)` + `KnownKeysBand::from_count(known_keys.len())`
- [ ] Task 4: Add `redact_key_id` private helper using `hex::encode(key_id.to_be_bytes())` (8 hex chars per `KeyId = u32`; mirrors SessionMismatch pattern)
- [ ] Task 4a: Add `octo-attach-key-rotation` feature propagation to `crates/octo-cli/Cargo.toml` per §Detailed Design §Feature flag wiring (one-line addition: `octo-attach-key-rotation = ["octo-runtime/octo-attach-key-rotation"]` — no `dep:` prefix because `octo-runtime` is already declared as a required dep at `crates/octo-cli/Cargo.toml`, not `optional = true` per [[substrate-faithfulness-verification]] R3.A HIGH-1 finding)
- [ ] Task 5: Update parent RFC-0011-h §Error Handling summary paragraph, §Exit Codes row 91, §Substrate-Additions G21 row right column, [^rotation-error-slot-prealloc] footnote (4 edits per §Detailed Design §Cross-RFC reference updates)
- [ ] Task 6: Create paired mission YAML `0011-h-s-a-network-key-rotation-unknown-id.md` in `missions/open/` (Open status)
- [ ] Task 7: Add 4 test vectors per G8 (`tv_w_1` + `tv_w_1b` + `tv_w_2` + `tv_w_3`)
- [ ] Task 8: `cargo fmt --all -- --check` clean
- [ ] Task 9: `cargo clippy -p octo-cli --all-targets --features octo-attach-key-rotation -- -D warnings` clean (feature ON build, exercises translation arm)
- [ ] Task 9a: `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (feature OFF build, default octo-cli)
- [ ] Task 10: `cargo test -p octo-cli --lib --features octo-attach-key-rotation` green (2 Rust vectors `tv_w_2` + `tv_w_1b` + zero regressions; feature ON build)
- [ ] Task 10a: `cargo test -p octo-cli --lib` green (1 Rust vector `tv_w_1` + zero regressions; feature OFF build, default octo-cli)
- [ ] Task 10b: Shell grep `tv_w_3` (4 per-edit greps per §Test Vectors `tv_w_3` row) verifies all 4 G4 cross-RFC reference updates landed at their target locations (feature OFF build, runs against parent RFC file directly; NOT a `cargo test` target)
- [ ] Task 11: `cargo build --workspace --all-targets` clean (Layer A frozen check passes — zero diff to governance-core + audit-core + settlement-core)
- [ ] Task 12: Companion mission YAML transition: Open → Claimed (paired with this amendment's code commit)
- [ ] Task 13: 5-len DRY CLOSURE gate on this amendment's RFC text + code commit (R1 + R1.5 + R2 zero rounds = GATE GREEN per Phase 12 RFC-0011-t precedent)
- [ ] Task 14: Companion mission YAML transition: Claimed → Completed (paired with DRY CLOSURE commit)
- [ ] Task 15: RFC promotion Draft → Accepted per docs/BLUEPRINT.md §RFC Process (paired with DRY CLOSURE gate)
- [ ] Task 16: Closure artifacts (audit doc at `docs/audits/2026-09-22-0011-w-slot-91-activation-dry-closure.md` gitignored per [[docs-audits-scratchpad]] + memory card + MEMORY.md index entry)

(No Phase 2; this amendment is single-phase.)

## Key Files to Modify

| File                                                                                                        | Change                                                                                                                                                                                                                                                                                                                                                     |
| ----------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `crates/octo-cli/src/error.rs`                                                                              | ADD `OctoCliError::NetworkKeyRotationUnknownId` variant + `KnownKeysBand` enum; ADD `exit_code` arm returning 91; ADD `From<octo_runtime::AttachError>` translation arm with `#[cfg(feature = "octo-attach-key-rotation")]`; ADD private `redact_key_id` helper                                                                                            |
| `crates/octo-cli/Cargo.toml`                                                                                | ADD `octo-attach-key-rotation` feature propagation clause: `octo-attach-key-rotation = ["octo-runtime/octo-attach-key-rotation"]` (NO `dep:` prefix because `octo-runtime` is already declared as a required dep at `crates/octo-cli/Cargo.toml`; per §Detailed Design §Feature flag wiring + [[substrate-faithfulness-verification]] R3.A HIGH-1 finding) |
| `rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md`                                               | EDIT §Error Handling summary paragraph; EDIT §Exit Codes row 91; EDIT §Substrate-Additions G21 row right column; EDIT [^rotation-error-slot-prealloc] footnote                                                                                                                                                                                             |
| `missions/open/0011-h-s-a-network-key-rotation-unknown-id.md`                                               | NEW paired companion mission YAML (Open status)                                                                                                                                                                                                                                                                                                            |
| `crates/octo-cli/src/error.rs` (tests)                                                                      | ADD 4 unit tests per §Test Vectors (`tv_w_1` + `tv_w_1b` + `tv_w_2` + `tv_w_3`)                                                                                                                                                                                                                                                                            |
| `docs/audits/2026-09-22-0011-w-slot-91-activation-dry-closure.md`                                           | NEW closure audit doc (gitignored per [[docs-audits-scratchpad]])                                                                                                                                                                                                                                                                                          |
| `~/.claude/projects/-home-mmacedoeu--w-ai-cipherocto/memory/0011-w-slot-91-activation-closed-2026-09-22.md` | NEW memory card                                                                                                                                                                                                                                                                                                                                            |
| `~/.claude/projects/-home-mmacedoeu--w-ai-cipherocto/memory/MEMORY.md`                                      | ADD 1-line index entry for the new memory card                                                                                                                                                                                                                                                                                                             |

## Future Work

- **F1 — CLI caller surface for the key-id translation path.** Future amendment adds a CLI dispatch surface that forwards `AttachError::UnknownKeyId` from the rebind-* dispatch arms to the new `NetworkKeyRotationUnknownId` variant. The rebind-* dispatch arms today return `Option<RebindCommit>` / `Option<RebindAbort>` — they do not route through `AttachError`. Substrate change required: `RebindCoordinator::commit_envelope` and `abort_envelope` must return `Result<_, AttachError>` instead of `Option<...>`. The D2.2 population policy (RFC-0011-l Phase 4 §Substrate-Additions G21 row) may advance here. F1 lands when a follow-on amendment activates D2.2 or when a CLI caller surface is needed for operational reasons.
- **F2 — Reachability tracking.** Track when slot 91 becomes reachable (via F1) via the `OctoCliError` variant reachability metrics per the Phase 7 R2 layer discipline reviewer pattern. Slot 91 reachability transitions from "defined but unused" to "defined and reachable" once F1 lands.
- **F3 — Post-PQC migration.** Per RFC-0011-c §F.5.1 + RFC-0015-a §6.5 paired-acceptance bridge, the `KeyId` discriminator surface may evolve post-PQC. Any migration updates the `redact_key_id` helper + the translation arm signature + the test vectors. OUT OF SCOPE for this amendment.

## Rationale

Why this approach over alternatives?

**Why a separate amendment RFC (Option A) vs updating RFC-0011-h directly (Option C)?**

RFC-0011-h is Accepted (`next 8696ec4b`) and is **Layer B RFC-driven, additive only (years-stable)** per the crate stability table in CLAUDE.md §Architectural Principles. Editing an Accepted RFC violates the immutability contract. The 14-phase amendment chain (RFC-0011-i through -v) establishes the precedent: every post-0011-h change lands via a separate amendment RFC. Slot 91 activation continues this pattern.

**Why feature-gate the translation arm vs always-on?**

The substrate `AttachError::UnknownKeyId` is gated on `octo-attach-key-rotation` Cargo feature (defined at `crates/octo-runtime/Cargo.toml`). If the translation arm were always-on in the default `octo-cli` build, it would fail to compile when the feature is disabled (the substrate variant doesn't exist). Feature-gating the translation arm mirrors the substrate's gating pattern and ensures compilation success in both feature states. The `octo-attach-key-rotation` feature must be PROPAGATED to `crates/octo-cli/Cargo.toml` (Cargo features do not transitively enable downstream); this is the §Detailed Design §Feature flag wiring §Cargo.toml change.

**Why mint the variant unconditionally (not feature-gated)?**

The pre-allocation contract (RFC-0011-h [^rotation-error-slot-prealloc]) reserves slot 91 unconditionally — the slot exists in the slot arithmetic regardless of feature state. If the variant were feature-gated, slot 91 would not exist in default builds, breaking the slot arithmetic. Minting the variant unconditionally preserves the slot arithmetic stability.

**Why no CLI dispatch surface (Option A vs Option B)?**

[[cipherocto-design-principles]] §Separation of concerns: variant minting and CLI dispatch surface are separate concerns. The rebind-* dispatch arms today return `Option<...>` not `Result<_, AttachError>` — adding a CLI dispatch surface requires a substrate change to make the dispatch arms route through `AttachError`. This is the F1 scope. The amendment separates the variant activation (this RFC) from the dispatch surface activation (F1 future RFC), per §Separation of concerns.

## Version History

| Version | Date       | Changes                                                                                                                                                          |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.0     | 2026-09-22 | Initial Draft — slot 91 activation per G21 forward projection; ONE NEW variant + ONE NEW translation arm + 4 parent RFC cross-reference updates + 3 test vectors |

## Related RFCs

- RFC-0011-h (parent RFC; slot 91 pre-allocation contract)
- RFC-0011-c (D2.1 paired-acceptance bridge §F.5.1; `AttachError::UnknownKeyId` substrate variant originates here)
- RFC-0011-l (Phase 4 substrate landing; G21 row substrate references)
- RFC-0011-v (Phase 14 substrate landing; Layer A phantom cite hygiene lesson applied to slot 91 cross-references)
- RFC-0015-a (§6.5 paired-acceptance bridge; `KeyId` discriminator surface)
- RFC-0855p-b (paired with RFC-0011-c §F.5.1 via the D2.1 paired-acceptance bridge)

## Related Use Cases

(None at Draft time; slot 91 activation is internal infrastructure; user-facing surface lands via F1.)

## Appendices

### A. Substrate-faithfulness verification (per [[substrate-faithfulness-verification]] R7.5 lesson)

Verified substrate signatures against actual code at RFC-0011-w Draft time:

- `AttachError::UnknownKeyId` at `crates/octo-runtime/src/handle/error.rs` — verified by direct `grep -nE 'UnknownKeyId'` returning the expected match
- `AttachError::UnknownKeyId` fields: `key_id: KeyId`, `known_keys: Vec<KeyId>` — verified by direct read
- `AttachError::UnknownKeyId` cfg attribute: `#[cfg(feature = "octo-attach-key-rotation")]` — verified by direct read
- `KeyId` discriminator: `pub type KeyId = u32;` (typed version discriminator for holder signing key per RFC-0011-c §F.5.1; NOT a 32-byte opaque type) — verified at `crates/octo-runtime/src/handle/key_id.rs` per `grep -nE 'pub type KeyId|pub struct KeyId'`. `u32` covers 4B key generations; hex-encoded as 8 chars via `key_id.to_be_bytes()`.
- `RebindCoordinator::commit_envelope` signature: `pub fn commit_envelope(&self, signature: Vec<u8>) -> Option<RebindCommit>` — verified by direct read at `crates/octo-network/src/mon/rebind.rs` `RebindCoordinator::commit_envelope` (returns `Option`, not `Result<_, AttachError>`; F1 future amendment required)
- `RebindCoordinator::abort_envelope` signature: `pub fn abort_envelope(&self, signature: Vec<u8>) -> Option<RebindAbort>` — verified by direct read at `crates/octo-network/src/mon/rebind.rs` `RebindCoordinator::abort_envelope` (returns `Option`, not `Result<_, AttachError>`; F1 future amendment required)
- `OctoCliError` enum: `#[non_exhaustive]` per RFC-0011-h §Extension over enumeration — verified by direct read
- `From<octo_runtime::AttachError> for OctoCliError`: existing impl at `crates/octo-cli/src/error.rs` `From<octo_runtime::AttachError>` — verified by direct read

### B. Layer A frozen check

Per [[cipherocto-design-principles]] §Stable Abstractions Principle + Phase 14 R2.5 lesson, the amendment preserves Layer A frozen:

- `crates/octo-governance-core`: zero diff (Layer A frozen)
- `crates/octo-audit-core`: zero diff (Layer A frozen)
- `crates/octo-settlement-core`: zero diff (Layer A frozen)
- `crates/octo-vault-core`: NOT in scope (no phantom cite per Phase 14 R2.5 lesson)
- `crates/octo-wallet-core`: NOT in scope (no phantom cite per Phase 14 R2.5 lesson)

Verification: `git diff crates/octo-governance-core crates/octo-audit-core crates/octo-settlement-core` returns 0 hits after amendment commit.

### C. Slot arithmetic summary (post-amendment)

| Slot | Variant                       | Status                                                                                                                        |
| ---- | ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| 79   | `NetworkPeerNotFound`         | DEFINED (Phase 1 RFC-0011-i)                                                                                                  |
| 80   | (RESERVED)                    | RESERVED at Draft per [^slot80-overflow-rationale]; revert to candidate if substrate grows fallible encoding path             |
| 81   | (RESERVED)                    | RESERVED at Draft per [^slot81-invalid-variant-rationale]; revert to candidate if substrate grows `RebindAbortReason` variant |
| 82   | `NetworkConfigParseFailed`    | DEFINED (Phase 2 RFC-0011-j)                                                                                                  |
| 83   | `NetworkLocalKeyUnavailable`  | DEFINED (Phase 1 RFC-0011-i)                                                                                                  |
| 84   | `NetworkCoordinatorNotFound`  | DEFINED (Phase 3 RFC-0011-k)                                                                                                  |
| 85   | `NetworkGraphDepthBelowRange` | DEFINED (Phase 1 RFC-0011-i)                                                                                                  |
| 86   | `NetworkInvalidDid`           | DEFINED (Phase 1 RFC-0011-i)                                                                                                  |
| 87   | `NetworkConfirmRequired`      | DEFINED (Phase 4 RFC-0011-l)                                                                                                  |
| 88   | `NetworkDryRunDenied`         | DEFINED (Phase 4 RFC-0011-l)                                                                                                  |
| 89   | `NetworkSubstrateUnavailable` | DEFINED (Phase 2 RFC-0011-j)                                                                                                  |
| 90   | `NetworkCIDenyDefault`        | DEFINED (Phase 5 RFC-0011-m)                                                                                                  |
| 91   | `NetworkKeyRotationUnknownId` | DEFINED (RFC-0011-w, this amendment)                                                                                          |

**Total: 11 defined variants + 2 RESERVED slots (80, 81) + 8 RESERVED slots (92-99) = 21 slot positions.** All within parent RFC-0011 future-amendment band (79-99) per RFC-0011-h §Exit Codes row 79-99 + G3 row line 62.

> **Pre-existing parent RFC vs actual code drift note (out of scope for RFC-0011-w):** Parent RFC §Error Handling table rows 87 (`NetworkConfirmRequired`) + 90 (`NetworkCIDenyDefault`) claim DEFINED variants, but `crates/octo-cli/src/error.rs` actual code has 8 `Network*` variants (missing slots 87 + 90 per R1.B substrate-faithfulness review). This drift pre-existed RFC-0011-w and is NOT introduced by this amendment. The amendment operates on the parent RFC text as the canonical source-of-truth being amended; resolving the actual code drift is a separate substrate-faithfulness concern tracked in a separate amendment or audit. RFC-0011-w §Appendix A substrate-faithfulness audit confirms all OTHER substrate claims (KeyId type, AttachError::UnknownKeyId signature, octo-attach-key-rotation feature location) are accurate.

---

**Version:** 1.0
**Submission Date:** 2026-09-22
**Last Updated:** 2026-09-22
