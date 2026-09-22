# 0011-h-s-a-network-key-rotation-unknown-id — Substrate additions for NetworkKeyRotationUnknownId slot 91 activation (RFC-0011-w)

## Status

Completed (2026-09-22) — Substrate-additions companion to RFC-0011-w §Substrate-Additions Companion Missions (NEW row added by R1 amendment cycle). Companion mission to RFC-0011-w `octo network` slot 91 `NetworkKeyRotationUnknownId` variant mint + translation arm + exit-code arm + redaction helper. Layer C CLI substrate additions landing in `crates/octo-cli/src/error.rs` per mission YAML substrate additions target. Zero Layer B substrate change (substrate `AttachError::UnknownKeyId` already LANDED at `crates/octo-runtime/src/handle/error.rs` per RFC-0011-c §F.5.1 paired-acceptance bridge).

## RFC

RFC-0011-w §Substrate-Additions Companion Missions (NEW row)

## Summary

Activates slot 91 pre-allocation from RFC-0011-h §Error Handling row 91 + §Exit Codes row 91 + §Substrate-Additions G21 row + [^rotation-error-slot-prealloc] footnote. The substrate `AttachError::UnknownKeyId { key_id: KeyId, known_keys: Vec<KeyId> }` (gated on `octo-attach-key-rotation` Cargo feature per RFC-0011-c §F.5.1 paired-acceptance bridge with RFC-0015-a §6.5) is the source of the typed-discriminator info. Today (without this mission), `AttachError::UnknownKeyId` falls through to the wildcard arm of `From<AttachError>` and surfaces as `Internal(reason)` exit 64 (additive-safe per `#[non_exhaustive]`).

This mission adds the translation arm in `From<octo_runtime::AttachError>` for `OctoCliError` so the substrate variant surfaces as `NetworkKeyRotationUnknownId` exit 91 instead of the generic `Internal` exit 64. Variant mints unconditionally (so slot 91 always exists in the `#[non_exhaustive]` enum regardless of feature state); translation arm fires only when the substrate feature is enabled.

### Substrate additions target

```rust
// crates/octo-cli/src/error.rs (MODIFY — slot 91 variant mint + KnownKeysBand)

/// Categorical band of known keys in the verifier's `KeySet` per
/// RFC-0011-h §Redaction Layer (3 bands; avoids leaking exact
/// verifier KeySet cardinality).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownKeysBand {
    /// 0 keys in `KeySet` (verifier misconfiguration)
    None,
    /// 1-8 keys in `KeySet` (typical deployment)
    Few,
    /// >8 keys in `KeySet` (large deployment or post-rotation grace period)
    Many,
}

impl KnownKeysBand {
    pub(crate) fn from_count(n: usize) -> Self {
        match n {
            0 => Self::None,
            1..=8 => Self::Few,
            _ => Self::Many,
        }
    }
}

/// `AttachError::UnknownKeyId` translation (LANDED at `crates/octo-runtime/src/handle/error.rs` per RFC-0011-c §F.5.1 paired-acceptance bridge; gated on `octo-attach-key-rotation` Cargo feature). Substrate
/// `octo_runtime::AttachError::UnknownKeyId` (LANDED at
/// `crates/octo-runtime/src/handle/error.rs` per RFC-0011-c
/// §F.5.1 paired-acceptance bridge; gated on
/// `octo-attach-key-rotation` Cargo feature). Variant mints
/// unconditionally at slot 91 per RFC-0011-w §Motivation; the
/// translation arm fires only when the substrate feature is enabled.
/// Pre-RFC-0011-w, this slot was RESERVED per
/// [^rotation-error-slot-prealloc].
#[error("network key rotation: unknown key_id 0x{key_id_hex} (known_keys band: {known_keys_band:?})")]
NetworkKeyRotationUnknownId {
    /// `key_id` discriminator rendered as 8 hex chars. Substrate
    /// `KeyId = u32` (typed version discriminator for holder signing
    /// key per RFC-0011-c §F.5.1; NOT a Layer A cryptographic secret)
    /// hex-encoded big-endian via `key_id.to_be_bytes()`. Mirrors the
    /// `hex::encode(declared)` / `hex::encode(actual)` pattern in the
    /// `SessionMismatch` translation arm.
    key_id_hex: String,
    /// Categorical band of known keys in the verifier's `KeySet`
    /// (active + grace period). 3-band quantization per §Redaction
    /// Layer: operator envelope renders the enum tag only (`None` /
    /// `Few` / `Many`) via the `#[error(... {known_keys_band:?})]`
    /// Debug formatter; the bucket thresholds (0 / 1-8 / >8) are
    /// implementation detail of `KnownKeysBand::from_count` and are
    /// NOT echoed in operator-facing render.
    known_keys_band: KnownKeysBand,
},

// crates/octo-cli/src/error.rs (MODIFY — exit-code arm)

impl OctoCliError {
    pub fn exit_code(&self) -> u8 {
        match self {
            // ... existing arms ...
            // RFC-0011-w: slot 91 activation per
            // [^rotation-error-slot-prealloc]. Variant mints
            // unconditionally; reachable when the
            // `octo-attach-key-rotation` feature is enabled in
            // `octo-runtime` AND a CLI caller surfaces the
            // `AttachError::UnknownKeyId` translation path (future
            // amendment per RFC-0011-w §Future Work F1).
            Self::NetworkKeyRotationUnknownId { .. } => 91,
        }
    }
}

// crates/octo-cli/src/error.rs (MODIFY — feature-gated translation arm)

impl From<octo_runtime::AttachError> for OctoCliError {
    fn from(e: octo_runtime::AttachError) -> Self {
        match e {
            // ... existing arms ...

            // RFC-0011-w: slot 91 activation. Feature-gated to
            // mirror substrate `#[cfg(feature =
            // "octo-attach-key-rotation")]` on
            // `AttachError::UnknownKeyId`. When the feature is
            // disabled, the substrate variant does not exist and
            // the existing wildcard arm below catches any unknown
            // variant → `Internal(reason)` exit 64
            // (additive-safe per `#[non_exhaustive]`).
            #[cfg(feature = "octo-attach-key-rotation")]
            octo_runtime::AttachError::UnknownKeyId { key_id, known_keys } => {
                Self::NetworkKeyRotationUnknownId {
                    key_id_hex: redact_key_id(&key_id),
                    known_keys_band: KnownKeysBand::from_count(known_keys.len()),
                }
            }

            // Additive-safe wildcard (existing).
            _ => Self::Internal(sanitize_substrate_error(&format!(
                "attach substrate error: {e}"
            ))),
        }
    }
}

// crates/octo-cli/src/error.rs (NEW — private redaction helper)

/// Redact the substrate `KeyId = u32` discriminator to 8 hex chars via
/// `hex::encode(key_id.to_be_bytes())` per RFC-0011-h §Redaction Layer.
/// Mirrors the existing `hex::encode(declared)` / `hex::encode(actual)`
/// pattern in the `SessionMismatch` translation arm (which hex-encodes
/// the 32-byte `SessionId`); per RFC-0011-c §F.5.1 the substrate `KeyId`
/// has different byte width but the same RFC-0008 §Deterministic
/// Encoding contract applies (big-endian byte order).
fn redact_key_id(key_id: &KeyId) -> String {
    hex::encode(key_id.to_be_bytes())
}
```

```toml
# crates/octo-cli/Cargo.toml (MODIFY — feature propagation)

[features]
# ... existing features ...
octo-attach-key-rotation = ["octo-runtime/octo-attach-key-rotation"]
```

(The `octo-runtime` dep in `crates/octo-cli/Cargo.toml` is declared as a REQUIRED dependency (no `optional = true` field at line 96), so Cargo's `dep:` feature syntax (which requires the dep to be `optional = true`) is INAPPLICABLE here. The correct syntax is the plain `octo-runtime/octo-attach-key-rotation` clause, which activates the substrate feature on the existing required dep without re-declaring it as optional per [[substrate-faithfulness-verification]] R3.A HIGH-1 finding.)

(Stub: full type signatures + ACs land in Phase X of this mission's own RFC/DRY cycle per [[no-phantom-mission-pointers]].)

## Acceptance Criteria

- [ ] `KnownKeysBand` enum (`None | Few | Many`) defined in `crates/octo-cli/src/error.rs` with `from_count(usize)` constructor per RFC-0011-w §Detailed Design variant block
- [ ] Variant `OctoCliError::NetworkKeyRotationUnknownId { key_id_hex: String, known_keys_band: KnownKeysBand }` mints at slot 91 in `crates/octo-cli/src/error.rs` (no `#[cfg(...)]` attribute on the variant — slot 91 always exists per pre-allocation contract); `#[error("network key rotation: unknown key_id 0x{key_id_hex} (known_keys band: {known_keys_band:?})")]` template interpolates both fields per RFC-0011-w §Test Vectors `tv_w_1`
- [ ] `OctoCliError::exit_code()` returns `91` for the new variant (unconditional; no `#[cfg(...)]` attribute on the arm)
- [ ] `From<octo_runtime::AttachError>` translation arm added with `#[cfg(feature = "octo-attach-key-rotation")]` to mirror substrate gating; maps `AttachError::UnknownKeyId { key_id, known_keys }` → `Self::NetworkKeyRotationUnknownId { key_id_hex: redact_key_id(&key_id), known_keys_band: KnownKeysBand::from_count(known_keys.len()) }`
- [ ] Private `redact_key_id(key_id: &KeyId) -> String` helper added in `crates/octo-cli/src/error.rs` using `hex::encode(key_id.to_be_bytes())` (8 hex chars per `KeyId = u32` substrate-faithful representation; mirrors SessionMismatch `hex::encode` pattern per RFC-0011-w §Substrate-faithfulness audit)
- [ ] `octo-attach-key-rotation` feature propagation added to `crates/octo-cli/Cargo.toml` per RFC-0011-w §Detailed Design §Feature flag wiring (`octo-attach-key-rotation = ["octo-runtime/octo-attach-key-rotation"]` — no `dep:` prefix because `octo-runtime` is already declared as a required dep at `crates/octo-cli/Cargo.toml:96`, not `optional = true` per [[substrate-faithfulness-verification]] R3.A HIGH-1 finding; required for translation arm compilation in feature ON builds)
- [ ] 3 test vectors per RFC-0011-w §Test Vectors (`tv_w_1` + `tv_w_2` + `tv_w_3`):
  - `tv_w_1`: variant construction + `exit_code()` returns 91 + `user_message()` renders the `#[error]` template with `key_id_hex` (8 hex chars via `hex::encode(key_id.to_be_bytes())`) + `known_keys_band` (Debug-rendered enum tag `None | Few | Many`) interpolated (feature OFF build; Rust unit test)
  - `tv_w_2`: `From<octo_runtime::AttachError>::from(AttachError::UnknownKeyId { key_id, known_keys })` returns `Self::NetworkKeyRotationUnknownId` with 8-hex-char `key_id_hex` (via `hex::encode(key_id.to_be_bytes())`) and `known_keys_band` matching `KnownKeysBand::from_count(known_keys.len())` per Vec::len bucketing thresholds (feature ON build; Rust unit test)
  - `tv_w_3`: cross-RFC reference integrity — shell grep verifies all 4 G4 §Cross-RFC reference updates landed at their target locations in `rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md` per RFC-0011-w §Test Vectors `tv_w_3` row (4 per-edit greps, one per G4 edit; NOT a Rust unit test, NOT cargo test target). The case-sensitive `slot 91.*RESERVED` regex is documented as NOT a useful tv_w_3 signal because the 4-edit list does not clear the regex hits at lines 62 + 545 (false positive on `slots 80 + 81 RESERVED` substring) + 801 (out-of-scope architectural pre-allocation statement) per R2.B H1 finding. Edit 2 grep uses whitespace-tolerant `\| 91 +\|.*NetworkKeyRotationUnknownId` because the parent RFC §Exit Codes row 91 cell uses 5-space column alignment (`| 91     |` per Markdown table convention; the unanchored single-space regex `^\| 91 \|` fails to match per R3.B HIGH finding).
- [ ] `cargo fmt --all -- --check` clean
- [ ] `cargo clippy -p octo-cli --all-targets --features octo-attach-key-rotation -- -D warnings` clean (feature ON build, exercises translation arm)
- [ ] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (feature OFF build, default octo-cli)
- [ ] `cargo test -p octo-cli --lib --features octo-attach-key-rotation` green (1 Rust vector tv_w_2 + zero regressions; feature ON build)
- [ ] `cargo test -p octo-cli --lib` green (1 Rust vector tv_w_1 + zero regressions; feature OFF build, default octo-cli)
- [ ] Shell grep `tv_w_3` (4 per-edit greps per RFC-0011-w §Test Vectors `tv_w_3` row) verifies all 4 G4 cross-RFC reference updates landed at their target locations (feature OFF build, runs against parent RFC file directly)
- [ ] `cargo build --workspace --all-targets` clean (no downstream breakage)
- [ ] Layer discipline preserved (Layer C CLI substrate additions only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] Layer A frozen check passes: `git diff crates/octo-governance-core crates/octo-audit-core crates/octo-settlement-core` returns 0 hits after mission commit (real Layer A crates in scope per RFC-0011-v R2.5 lesson; no phantom `octo-vault-core` or `octo-wallet-core` cites)
- [ ] RFC-0011-h cross-reference updates land per RFC-0011-w §Detailed Design §Cross-RFC reference updates:
  - §Error Handling summary paragraph (line 545): "All 10 defined ... + slot 91 pre-allocated" → "All 11 defined ... (slot 91 activated per RFC-0011-w)"
  - §Exit Codes row 91: `(RESERVED)` → defined `NetworkKeyRotationUnknownId` with substrate reference
  - §Substrate-Additions G21 row right column: append slot 91 activation status
  - [^rotation-error-slot-prealloc] footnote: rewrite to reflect activation contract
- [ ] `npx prettier --write rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md` applied (reformats table column alignment after G21 row right column expansion)

## Dependencies

Hard sequencing:

1. **RFC-0011-w must be Draft** before this mission's substrate additions land (mission YAML cites real RFC per [[no-phantom-mission-pointers]])
2. **RFC-0011-h must be Accepted** before RFC-0011-w Drafts (parent pre-allocation contract lives in RFC-0011-h §Error Handling row 91 + §Exit Codes row 91 + §Substrate-Additions G21 row + [^rotation-error-slot-prealloc] footnote)
3. **RFC-0011-c must be Accepted** (D2.1 paired-acceptance bridge §F.5.1; `AttachError::UnknownKeyId` substrate variant originates here)
4. **RFC-0011-l must be Accepted** (Phase 4 substrate landing; G21 row substrate references)
5. **RFC-0015-a must be Accepted** (§6.5 paired-acceptance bridge; `KeyId` discriminator surface)

Substrate-first ordering invariant per [[no-phantom-mission-pointers]]: this mission's substrate additions (variant mint + translation arm + exit-code arm + redaction helper) land BEFORE any future CLI caller surface (RFC-0011-w §Future Work F1) that surfaces the key-id translation path.

## Out of Scope

- CLI dispatch surface for the key-id translation path (deferred to RFC-0011-w §Future Work F1; current rebind-* dispatch arms return `Option<RebindCommit>` / `Option<RebindAbort>` not `Result<_, AttachError>` — substrate change required to route through `AttachError`)
- D2.2 population policy (deferred post-PQC per RFC-0011-l Phase 4 §Substrate-Additions G21 row)
- Per-extension concrete impl crates (Layer D; OUT OF SCOPE per [[cipherocto-design-principles]] §User extensibility)
- Wire format versioning (out of RFC-0011-w scope; deferred per RFC-0011-h §Future Work items F8 + F9)
- Post-PQC `KeyId` discriminator migration (out of RFC-0011-w scope; deferred per RFC-0011-c §F.5.1 paired-acceptance bridge migration notes)

## Notes

Stub filed 2026-09-22 per [[no-phantom-mission-pointers]] + RFC-0011-w amendment cycle. The substrate gap was identified at RFC-0011-h Draft time: slot 91 was pre-allocated per [^rotation-error-slot-prealloc] footnote to avoid a runtime slot-arithmetic gap when the post-G21 amendment lands. Without this mission, `AttachError::UnknownKeyId` falls through to the wildcard arm of `From<AttachError>` and surfaces as `Internal(reason)` exit 64, collapsing the typed-discriminator information that the dedicated `NetworkKeyRotationUnknownId` exit was designed to surface.

Variant reachability: requires `octo-attach-key-rotation` Cargo feature enabled in `octo-cli` (which propagates to `octo-runtime`) AND a CLI caller surface for the key-id translation path (RFC-0011-w §Future Work F1). Until F1 lands, the variant sits unused in the `#[non_exhaustive]` enum (additive-safe per [[cipherocto-design-principles]] §Extension over enumeration). Slot 91 arithmetic: 11 defined variants + 2 RESERVED slots (80, 81) + 8 RESERVED slots (92-99) = 21 slot positions within parent RFC-0011 future-amendment band (79-99) per RFC-0011-h §Exit Codes row 79-99 + §Substrate-Additions G3 row.

Substrate-faithfulness verified by direct read at RFC-0011-w Draft time per [[substrate-faithfulness-verification]] R7.5 lesson: `AttachError::UnknownKeyId` signature at `crates/octo-runtime/src/handle/error.rs` exactly matches the translation arm shape in RFC-0011-w §Detailed Design. `KeyId` substrate type is `pub type KeyId = u32;` (typed version discriminator per RFC-0011-c §F.5.1, NOT a 32-byte opaque type) — 8 hex chars via `key_id.to_be_bytes()`. Feature flag wiring verified: `octo-attach-key-rotation` defined on `crates/octo-runtime/Cargo.toml`; propagation to `crates/octo-cli/Cargo.toml` REQUIRED (Cargo features do NOT transitively enable downstream) per RFC-0011-w §Detailed Design §Feature flag wiring + R1.B substrate-faithfulness review.

Feature propagation clause added to `crates/octo-cli/Cargo.toml` (NEW per RFC-0011-w §Implementation Phases Task 4a; `octo-attach-key-rotation = ["octo-runtime/octo-attach-key-rotation"]` — no `dep:` prefix because `octo-runtime` is already declared as a required dep at `crates/octo-cli/Cargo.toml:96`, not `optional = true` per [[substrate-faithfulness-verification]] R3.A HIGH-1 finding). No new feature flag DEFINED on `octo-cli` (single source of truth remains `crates/octo-runtime/Cargo.toml`). No new Cargo.toml dependencies. Layer A frozen preserved per [[cipherocto-design-principles]] §Stable Abstractions Principle.
