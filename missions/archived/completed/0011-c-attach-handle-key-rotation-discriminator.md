---
name: 0011-c-attach-handle-key-rotation-discriminator
description: Land the KeyId discriminator + KeySet registry + sign_v2 + verify_v2 + UnknownKeyId variant substrate in octo-runtime Layer B (gated on octo-attach-key-rotation feature)
metadata:
  node_type: substrate-runtime-extension
  type: layer-b-substrate-extension
  originSessionId: d23cf564-d553-4e7d-be82-070883125eed
  created: 2026-09-17
  v: "1.0"
  depends_on:
    - RFC-0011
    - RFC-0011-c
    - RFC-0015
    - RFC-0015-a
    - mission 0011-c-octo-runtime-attachhandle-substrate
    - mission 0011-c-octowallet-agents-substrate
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-17
completed_at: 2026-09-17
completed_by: mmacedoeu
implementation_commit: e9aca538
review_rounds: 0
dry_closure_audit: docs/audits/2026-09-17-0011-c-phase-d2-1-key-rotation-discriminator-dry-closure.md
---

# 0011-c-attach-handle-key-rotation-discriminator — KeyId + KeySet + sign_v2 + verify_v2 + UnknownKeyId

**Status:** Completed
**Substrate:** RFC-0011-c §F.5.1 — Key rotation discriminator (D2.1 additive slice)
**Parent:** RFC-0011-c (agent lifecycle amendment of RFC-0011)
**Depends on:**

- Mission `0011-c-octo-runtime-attachhandle-substrate` — §F.5 v1 single-pubkey surface pre-existing
- Mission `0011-c-octowallet-agents-substrate` — Phase D1 predecessor; this mission unblocks the Phase D2 standing-direction blocker
- RFC-0015-a §6.5 — paired-acceptance bridge cite for `octo-attach-key-rotation` cfg-gate

## Status

Completed (RFC-0011-c §F.5.1 D2.1 discriminator-only additive slice; Phase D2.1 of AttachHandle follow-on cycles A/B/C/D1). Resolves the standing-direction "Phases B/C/D complete without further deferrals" — D2.1 = discriminator mechanism; D2.2 = policy + `key_set` population (post-PQC direction).

## Substrate (RFC-0011-c §F.5.1)

Per RFC-0011-c §F.5.1 + RFC-0015-a §6.5 paired-acceptance bridge. Algorithm-independent discriminator mechanism (Layer B substrate-faithful per [[cipherocto-design-principles]] §Stable Abstractions Principle). NO `key_set` population policy — that lands as D2.2 post-PQC direction.

## Parent

RFC-0011-c (agent lifecycle amendment; Phase D2.1 of AttachHandle follow-on cycles). Cycles A/B/C/D1 closed prior; this is the cycle-D2.1 discriminator-only closure.

## Depends on

See YAML frontmatter `depends_on` block above. Hard sequencing: Phase A/B/C/D1 AttachHandle follow-on cycles → Phase D2.1.

## Acceptance Criteria

- [x] `crates/octo-runtime/src/handle/key_id.rs` (NEW) defines `pub type KeyId = u32` + `pub struct KeySet { keys: BTreeMap<KeyId, [u8; 32]>, grace_keys: BTreeMap<KeyId, [u8; 32]> }`
- [x] `KeySet::new / insert / move_to_grace / lookup / grace_key / grace_period / known_key_ids / len / is_empty` (9 methods) all land; pubkey retained in grace_keys for grace-fallback verify
- [x] 5 KeySet tests land (insert/lookup roundtrip, move_to_grace removes from active, re-promote clears grace, known_key_ids includes both, empty lookup miss) — all gated on `octo-attach-key-rotation` feature
- [x] `pub mod key_id;` + `pub use key_id::{KeyId, KeySet};` added to `crates/octo-runtime/src/handle/mod.rs`
- [x] `pub fn canonical_payload_bytes_v2` lands in `signing.rs` (additive: v1 bytes + 4-byte big-endian `key_id` suffix)
- [x] `pub fn sign_attach_handle_payload_v2` lands in `signing.rs` (additive: composes `canonical_payload_bytes_v2` then delegates to `IdentityKey::sign`)
- [x] `pub fn verify_attach_handle_payload_v2` lands in `signing.rs` (3-step lookup: active → grace fallback → `UnknownKeyId`)
- [x] Private `verify_with_msg` helper lands in `signing.rs` (precomputed-message re-use across grace iteration)
- [x] 5 NEW sign/verify tests land (sign_v2_includes_key_id_in_canonical_bytes, sign_v2_happy_path, verify_v2_grace_period_accepts_rotated_key, verify_v2_unknown_key_id_returns_error, verify_v2_active_lookup_mismatch_returns_bad_signature) — all gated on `octo-attach-key-rotation` feature
- [x] `AttachError::UnknownKeyId { key_id, known_keys }` additive variant lands in `error.rs` (cfg-gated)
- [x] 1 Display test (`unknown_key_id_display_includes_id_and_known`) lands (cfg-gated)
- [x] `octo-attach-key-rotation = []` feature added to `crates/octo-runtime/Cargo.toml` (default OFF)
- [x] RFC-0011-c §F.5.1 amendment lands (substrate specification mirrors code surface)
- [x] `cargo fmt --all -- --check` clean
- [x] `cargo clippy -p octo-runtime --lib --all-features -- -D warnings` clean
- [x] `cargo test -p octo-runtime --lib` (feature OFF default): 91 of 91 pass (86 baseline + 5 KeySet tests; v2 tests + UnknownKeyId Display compile-out)
- [x] `cargo test -p octo-runtime --lib` (feature ON): 97 of 97 pass (86 baseline + 5 KeySet + 1 UnknownKeyId Display + 5 v2 sign/verify)

### Type Coverage

| RFC-0011-c type                            | Sub-step                              | Notes                                                                                                                                                                              |
| ------------------------------------------ | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pub type KeyId`                           | Sub-step 1 (discriminator)            | Layer B; `u32` covers 4B key generations; `NonZeroU32` not used because `key_id == 0` is canonical-acceptance bootstrap slot per RFC-0015-a §6.5                                  |
| `pub struct KeySet`                        | Sub-step 2 (registry)                 | Layer B; `keys: BTreeMap<KeyId, [u8; 32]>` + `grace_keys: BTreeMap<KeyId, [u8; 32]>`; pubkey retained in grace_keys so the v2 grace fallback can attempt a per-pubkey verify          |
| `KeySet::new / insert / move_to_grace`     | Sub-step 3 (mutators)                 | Layer B; `move_to_grace` atomically moves pubkey from active to grace; `insert` clears grace on re-promotion                                                                       |
| `KeySet::lookup / grace_key / grace_period`| Sub-step 4 (lookups)                  | Layer B; active lookup parallel to grace lookup; `grace_period` returns ascending-order Vec for stable verify iteration                                                          |
| `KeySet::known_key_ids / len / is_empty`   | Sub-step 5 (diagnostics)              | Layer B; `known_key_ids` = active + grace union; `len / is_empty` count active-only                                                                                                 |
| `canonical_payload_bytes_v2`               | Sub-step 6 (canonical bytes)          | Layer B; additive: v1 canonical bytes ++ `key_id.to_be_bytes()` (4-byte big-endian suffix); single source of truth for sign_v2 + verify_v2                                          |
| `sign_attach_handle_payload_v2`            | Sub-step 7 (sign)                     | Layer B; composes `canonical_payload_bytes_v2` + `IdentityKey::sign` per RFC-0015-a Appendix A; Signature captured via `to_bytes()` (avoids Layer A type leak per §F.5 stable-abstraction pattern) |
| `verify_attach_handle_payload_v2`          | Sub-step 8 (verify)                   | Layer B; 3-step lookup: active → grace fallback → `UnknownKeyId`; v1 verify body shape factored into private `verify_with_msg` helper                                            |
| `AttachError::UnknownKeyId`                | Sub-step 9 (error envelope)           | Layer B; additive typed-discriminator variant per [[cipherocto-design-principles]] §Extension over enumeration; CLI mapping is follow-on (defaults to `Internal(reason)` wildcard) |
| 10 NEW tests                               | Sub-step 10 (TV)                      | Layer B; 5 KeySet + 5 sign/verify + 1 UnknownKeyId Display; all gated on `octo-attach-key-rotation` feature                                                                        |
| `octo-attach-key-rotation` Cargo feature   | Sub-step 11 (cfg-gate)                | Build-system; RFC-0015-a §6.5 paired-acceptance bridge; default OFF preserves v1 byte-identical baseline                                                                          |

## Implementation Guide

See `docs/07-developers/octo-runtime-implementation-guide.md` §AttachHandle Token Substrate for the v1 single-pubkey surface; the v2 additions are pure cfg-gated extensions layered on top. The KeySet pattern mirrors the `RevocationStore` pattern (per-extension-crate + registry; here KeySet is a single-crate Layer B registry because the policy is OUT OF SCOPE — see D2.2 DEFERRED section).

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

**Algorithm-independent discriminator** — D2.1's wire form + canonical signed bytes + lookup logic are forward-compatible regardless of which signature algorithm(s) the `key_set` eventually holds (classic Ed25519 / hybrid Ed25519+PQC / PQC-only). PQC direction only affects the `key_set` population policy (D2.2).

**Grace fallback is per-pubkey verify, not skip-verify** — the v2 verifier iterates the grace map and attempts `ed25519-dalek::verify` against each rotated pubkey. If any grace pubkey verifies the signature, the token is accepted. The grace window is therefore bounded by the rotation policy (D2.2 OUT OF SCOPE), not by the verify path itself.

**Design deviation from plan**: `grace_ids: BTreeSet<KeyId>` → `grace_keys: BTreeMap<KeyId, [u8; 32]>`. Reason: the grace fallback needs the rotated pubkey to attempt per-pubkey verify; a `BTreeSet<KeyId>` would lose the pubkey on `move_to_grace`, making the grace iteration a no-op. The deviation surfaces a useful substrate invariant: **the grace map MUST carry the rotated pubkey**, NOT just the key id.

The 11 NEW tests assert:
1-5. KeySet mutators + lookups + diagnostics (insert, move_to_grace, re-promote, known_key_ids, empty)
6. v2 canonical bytes include the key_id (4-byte big-endian suffix)
7. v2 sign + verify against active lookup succeeds
8. v2 grace fallback accepts a rotated key (proves grace carries the pubkey)
9. v2 unknown key_id returns `UnknownKeyId` with the populated `known_keys` set
10. v2 active lookup mismatch with a multi-key set rejects as `BadSignature` (NOT `UnknownKeyId` — proves lookup path is per-pubkey)
11. `UnknownKeyId` Display includes key_id + known_keys (operator-readable diagnostic)

## Risk

- **NONE** to baseline — substrate-faithful additive variant. Default build (feature OFF) is byte-identical to pre-D2.1 baseline (91/91 lib tests pass byte-identical; D2.1 surface compiles-out).
- **DEFERRED Phase D2.2** — `key_set` population policy (which keys exist, when they rotate, grace-period bounds, PQC algorithm choice, wire-form extension if needed, CLI mapping for `UnknownKeyId`). Lands post-PQC direction.
- **NO Layer A change** — D2.1 introduces zero new Layer A primitives. `KeyId = u32` is a Layer B typed discriminator (not a key). `KeySet` is Layer B state. The Ed25519 verify path is unchanged (still `ed25519_dalek::verify` via the `IdentityKey::sign` Layer A primitive).

## Scope

Land the KeyId discriminator + KeySet registry + sign_v2 + verify_v2 + UnknownKeyId variant substrate per RFC-0011-c §F.5.1. **DEFERRED Phase D2.2**: `key_set` population policy (post-PQC direction).

## Sub-steps

1. **`pub type KeyId = u32` + `pub struct KeySet`** — `crates/octo-runtime/src/handle/key_id.rs` (Layer B; NEW). `KeySet` carries `keys: BTreeMap<KeyId, [u8; 32]>` + `grace_keys: BTreeMap<KeyId, [u8; 32]>` (the pubkey is retained in grace for per-pubkey verify). Module rustdoc cites RFC-0011-c §F.5.1 + RFC-0015-a §6.5 paired-acceptance bridge + the algorithm-independent discriminator principle.

2. **`KeySet` impl** — same file (Layer B). 9 methods: `new / insert / move_to_grace / lookup / grace_key / grace_period / known_key_ids / len / is_empty`. Module rustdoc clarifies the `grace_keys` design rationale (pubkey retention invariant).

3. **`pub mod key_id;` + re-exports** — `crates/octo-runtime/src/handle/mod.rs` (Layer B). Re-export `KeyId` + `KeySet` at module root so `octo_runtime::handle::KeyId / KeySet` paths work.

4. **`canonical_payload_bytes_v2`** — `crates/octo-runtime/src/handle/signing.rs` (Layer B; cfg-gated). Composes `canonical_payload_bytes` (v1, single source of truth per RFC-0016-a §6.10) + `key_id.to_be_bytes()` 4-byte suffix. Additive — v1 bytes unchanged.

5. **`sign_attach_handle_payload_v2`** — same file (Layer B; cfg-gated). Composes `canonical_payload_bytes_v2` + `IdentityKey::sign` (Layer B substrate per RFC-0015-a Appendix A). Signature captured via `to_bytes()` (avoids Layer A type leak).

6. **`verify_attach_handle_payload_v2`** — same file (Layer B; cfg-gated). 3-step lookup: active → grace fallback → `UnknownKeyId`. Private `verify_with_msg` helper factors the v1 verify body shape so the v2 verifier can re-use the canonical bytes across the grace iteration.

7. **`AttachError::UnknownKeyId` variant** — `crates/octo-runtime/src/handle/error.rs` (Layer B; cfg-gated). Additive typed-discriminator variant per [[cipherocto-design-principles]] §Extension over enumeration. CLI mapping is follow-on (defaults to `Internal(reason)` wildcard via the existing `From<AttachError>` arm).

8. **`octo-attach-key-rotation` Cargo feature** — `crates/octo-runtime/Cargo.toml`. Default OFF. When OFF, the entire D2.1 surface (KeySet + sign_v2 + verify_v2 + UnknownKeyId + their tests) compiles-out via `#[cfg(feature = "octo-attach-key-rotation")]` — the v1 baseline is byte-identical.

9. **11 NEW tests** — across `key_id.rs` (5), `signing.rs` (5), `error.rs` (1). All gated on the feature. Test vectors cover: KeySet mutators + lookups + diagnostics + empty state; v2 canonical bytes include key_id; v2 happy path; v2 grace fallback; v2 unknown key_id; v2 active lookup mismatch returns `BadSignature`; UnknownKeyId Display.

10. **RFC §F.5.1 amendment** — `rfcs/accepted/process/0011-c-agent-lifecycle.md` §F.5.1 NEW section. Substrate specification mirrors code surface. Cites RFC-0015-a §6.5 paired-acceptance bridge + RFC-0016-a §6.10 canonical-bytes invariant. Explicit DEFERRED section for D2.2.

## Cargo deps

```toml
# crates/octo-runtime/Cargo.toml — additive per RFC-0011-c §F.5.1
[features]
# ... existing features ...
octo-attach-key-rotation = []
```

No new external crates required; D2.1 is pure substrate-extension on the existing `octo-runtime` Layer B + `octo-wallet` Layer B `IdentityKey::sign` Layer A surface.

## Test Vectors (per RFC-0011-c §F.5.1)

11 NEW tests covering the v2 surface:

| #              | Substrate coverage                                                                                |
| -------------- | ------------------------------------------------------------------------------------------------- |
| TV-KR-D2-K1    | `key_set_insert_lookup_roundtrip` — insert + lookup returns same pubkey                            |
| TV-KR-D2-K2    | `key_set_move_to_grace_removes_from_active` — after move_to_grace, lookup returns None             |
| TV-KR-D2-K3    | `key_set_re_promote_clears_grace` — insert of grace'd id removes from grace                       |
| TV-KR-D2-K4    | `key_set_known_key_ids_includes_both` — known_key_ids = active + grace union                     |
| TV-KR-D2-K5    | `key_set_empty_lookup_miss` — new() lookup returns None                                           |
| TV-KR-D2-S1    | `sign_v2_includes_key_id_in_canonical_bytes` — v2 bytes = v1 ++ key_id_be(7)                      |
| TV-KR-D2-S2    | `sign_v2_happy_path` — sign_v2 + verify_v2 against key_set[key_id] succeeds                       |
| TV-KR-D2-S3    | `verify_v2_grace_period_accepts_rotated_key` — sign with id=1; key_set moves id=1 to grace; verify_v2 succeeds via grace fallback |
| TV-KR-D2-S4    | `verify_v2_unknown_key_id_returns_error` — sign with id=99; key_set contains 1, 2; verify returns UnknownKeyId |
| TV-KR-D2-S5    | `verify_v2_active_lookup_mismatch_returns_bad_signature` — multi-key set, active lookup mismatch rejects as BadSignature (not UnknownKeyId) |
| TV-KR-D2-E1    | `unknown_key_id_display_includes_id_and_known` — Display includes key_id + known_keys set          |

## Layer direction (RFC-0011-c §9.1 Architecture + per [[cipherocto-design-principles]])

- `octo-runtime` (Layer B) — `KeyId` + `KeySet` + `sign_v2 / verify_v2 / canonical_payload_bytes_v2` + `AttachError::UnknownKeyId` + 11 tests
- `octo-wallet` (Layer B) — UNCHANGED; `IdentityKey::sign` (RFC-0015-a Appendix A) is the sole Layer B dependency
- `ed25519-dalek` (Layer A frozen) — UNCHANGED; re-exported via `octo_wallet::ed25519_dalek`
- NO new Layer C/D/E crates introduced

## Validation

```bash
cargo fmt --all -- --check                                                # clean
cargo clippy -p octo-runtime --lib --all-features -- -D warnings           # clean
cargo test -p octo-runtime --lib                                            # 91/91 PASS (feature OFF default)
cargo test -p octo-runtime --lib --features octo-attach-key-rotation        # 97/97 PASS (feature ON)
cargo build --workspace --all-targets                                       # EXIT=0
```

## Backward compat

- Additive only: D2.1 surface (KeySet + sign_v2 + verify_v2 + UnknownKeyId) is hidden when `octo-attach-key-rotation` feature is OFF (default). v1 single-pubkey path is byte-identical to baseline.
- No new CLI exit code changes; no new CLI variants; no `OctoCliError` envelope changes.
- No `WalletError` envelope changes.
- No Layer A crypto primitive changes.

## Cross-references

- RFC-0011-c §F.5.1 — Key rotation discriminator (NEW)
- RFC-0011-c §F.5 — signing surface (v1 single-pubkey baseline)
- RFC-0015-a §6.5 — Layer A Paired-Acceptance Bridge (cfg-gate cite)
- RFC-0015-a Appendix A — `IdentityKey::sign` substrate
- RFC-0016-a §6.10 — canonical-bytes invariant (carries through to v2)
- [[cipherocto-design-principles]] — Layer A/B stability contract + extension over enumeration + per-extension crate + registry pattern
- [[0011-c-octowallet-agents-substrate-phase-d1-dry-closure-2026-09-17]] — Phase D1 predecessor
- [[0011-c-phase-c-cross-process-revocation-dry-closure]] — Phase C pattern (substrate-faithful additive variant)
- [[0011-c-transport-extension-phase-b-dry-closure-2026-09-17]] — Phase B pattern (3 Layer D extension crates)
- [[0011-c-attachhandle-followon-phase-a]] — Phase A pattern (variant arithmetic + slot allocation)

## Why gate

No release gate. D2.1 is substrate-faithful additive; the default build (feature OFF) is byte-identical to baseline. The v2 surface is opt-in via the `octo-attach-key-rotation` Cargo feature, which is OFF until D2.2 ships the `key_set` population policy post-PQC direction.

## Closure audit

See `docs/audits/2026-09-17-0011-c-phase-d2-1-key-rotation-discriminator-dry-closure.md` for the multi-round DRY review chain summary + substrate-faithful verification + algorithm-independent discriminator justification.

## Memory card

See `~/.claude/projects/.../memory/0011-c-attach-handle-key-rotation-discriminator-phase-d2-1-dry-closure-2026-09-17.md` for closure card.

## Claimant

@unassigned
