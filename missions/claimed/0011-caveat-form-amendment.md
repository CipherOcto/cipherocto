---
name: 0011-caveat-form-amendment
description: "Land the Layer B substrate change for RFC-0011 Caveat Form Amendment: add `visit_str` arm to `dqa_serde::field::deserialize` so the canonical 64-hex form emitted by `Caveat::canonical_ser` for `AmountMax` re-parses through the input form; create `hex_id_32` adapter module accepting both 64-hex (canonical) and 32-element byte array (migration); apply the adapter to `Caveat::Vault([u8; 32])`. Round-trip holds for the lossless arms only: the `Payment` canonical form is a partial projection and does NOT reparse, and four of the five hex-emitting arms still reject their own hex. Both are recorded in the RFC §Known deviations and pinned by `tv_cf_18` and `tv_cf_20`/`tv_cf_21`. Requires the amendment RFC acceptance per RFC-0011 §Caveat Catalog + §Hex32 newtype."
metadata:
  node_type: mission
  type: substrate-conformance
  originSessionId: 6b66c09a-4979-47ba-b92f-e3a757ecff92
  created: 2026-10-02
  v: "1.0"
  depends_on:
    - 0011-vault-asset-chain-id-hex
    - rfcs/draft/0011-caveat-form-amendment.md
status: CLAIMED
---

# Mission `0011-caveat-form-amendment` v1.0 — CLAIMED 2026-10-02, implementation complete 2026-10-03

**Owner:** substrate (Layer B, `octo-cap-macaroon` — the spec review during
Phase 2 established the crate is Layer B and **not** in the Layer A frozen
list; an earlier revision of this line said Layer A, which would have licensed
skipping the freeze discipline).
**Phase:** Phase 1 of the Caveat Form Alignment plan.
**Companion:** `missions/claimed/0011-vault-asset-chain-id-hex.md` (Layer B, paired).

## Status

Implementation complete as of 2026-10-03. All acceptance criteria for the
mission's scope are met and every gate is green; the mission is **not** closed,
because the two promotion gates below have not fired.

Landed:

- Phase 1 — `visit_str` arm on `dqa_serde::field`, the `hex_id_32` adapter, its application to `Caveat::Vault`, and the inversion of `guide_canonical_form_is_not_reparseable` → `guide_canonical_form_is_reparseable` (`next 590ddbff`, review fixes `next 747f15d7`).
- Phase 2 review follow-ups (`next 4423454e`, `next a5cd3fc8`): the adapter's original vectors were built from a single repeated byte, a fixed point of every byte permutation, so no vector could detect a byte-order defect — reversing the byte order inside `hex_id_32::serialize` left all 13 vectors and all 257 lib tests green. `tv_cf_14` (position-sensitive, non-gated, so it runs in the ordinary CI gate), `tv_cf_15` (array length rejection) and `tv_cf_16` (uppercase-input leniency) close that, each verified against a mutation. The `AssetId` / `ChainId` / `VaultId` doc paragraphs also claimed the newtype "serialises as hex" without qualifying that the Borsh form is unaffected by the flag; corrected.

Open gates before this mission can close:

1. The amendment RFC is still `Draft`. `docs/BLUEPRINT.md` §RFC Process requires a minimum 7-day feedback window (step 3) and a discussion PR (step 2); the RFC was filed 2026-10-02 and no PR has been opened.
2. The post-implementation multi-round adversarial review closure pair (R-DRY) has not run. Per `docs/BLUEPRINT.md` §Mission Lifecycle the mission file stays in `missions/claimed/` until it does.

Layer note: the spec review during Phase 2 established that `octo-cap-macaroon`
is **Layer B**, not Layer A — it is not in the Layer A frozen list, and the
"Layer A frozen" header in `src/substrate.rs` is a pre-existing inaccuracy left
out of scope. The `Layer A` label in this mission's `Owner` line, in its
frontmatter description, and in the RFC header was corrected by the adversarial
review on 2026-10-03. The `src/substrate.rs` header itself is the one part
still outstanding, since changing it is a code change outside this mission's
declared scope.

## Context

Per `rfcs/draft/0011-caveat-form-amendment.md` §Caveat Form Amendment, the
canonical hex form emitted by `Caveat::canonical_ser` for `AmountMax`,
`Payment.budget`, and `Vault` MUST round-trip through the same `Caveat`
enum's `Deserialize` impl. The drift audit
`docs/audits/2026-09-30-open-limitations-drift-audit.md` §9 records the
closed-at-canonical-form portion of the asymmetry; this mission closes the
round-trip portion at the source.

The drift is two-fold:

1. `dqa_serde::field`'s deserializer implements `visit_bytes` and `visit_seq`
   and not `visit_str`. The canonical form emits hex; the input form accepts
   only a 16-element byte array.
2. `Caveat::Vault([u8; 32])` derives a default 32-element byte-array form;
   the canonical form emits hex; the guide's
   `guide_canonical_form_is_not_reparseable` pins the rejection.

## Scope

### Step 1: Add `visit_str` arm to `dqa_serde::field::deserialize`

Edit `crates/octo-cap-macaroon/src/dqa_serde.rs`. The `Visitor` impl gains a
`visit_str` arm that hex-decodes the input and delegates to
`dqa_from_bytes`. A paired `visit_string` arm delegates to `visit_str`. The
existing `visit_bytes` + `visit_seq` arms are preserved (the byte-array form
remains valid input during the migration window).

### Step 2: Create `hex_id_32` adapter module

New file `crates/octo-cap-macaroon/src/hex_id_32.rs`. The module exposes
`pub fn serialize<S>` and `pub fn deserialize<'de, D>`. The serializer emits
a 64-char lowercase hex string via `hex::encode`. The deserializer dispatches
via `deserialize_any` to a `Visitor` with `visit_str` (canonical hex form,
preferred) and `visit_seq` (legacy 32-element array form, preserved for
migration). Both arms reject inputs that are not exactly 32 bytes.

### Step 3: Apply `hex_id_32` to `Caveat::Vault`

Edit `crates/octo-cap-macaroon/src/caveat/mod.rs`. The `Caveat::Vault([u8; 32])`
arm gains `#[serde(with = "crate::hex_id_32")]` alongside its existing
`#[serde(rename = "vault")]`.

### Step 4: Export `hex_id_32` from the crate root

Edit `crates/octo-cap-macaroon/src/lib.rs`. Add `pub mod hex_id_32;` (or a
`pub use` re-export) so the Layer B adapter module can reuse `hex_id_32`'s
`deserialize` in its newtype-specific adapters without re-signalling the
32-byte constraint.

### Step 5: Add the 9 new test vectors

Add 9 new test functions to `crates/octo-cap-macaroon` (across the existing
`dqa_serde` and `caveat/mod` test modules plus a new
`tests/tv_cf_dqa_hex_round_trip.rs` integration file):

| Vector                                                      | Where                                 | What it pins                                                            |
| ----------------------------------------------------------- | ------------------------------------- | ----------------------------------------------------------------------- |
| `tv_cf_01_dqa_hex_round_trip`                               | `tests/tv_cf_dqa_hex_round_trip.rs`   | 64-hex `Dqa` decodes to the same value as the byte-array form.          |
| `tv_cf_02_dqa_hex_rejects_odd_length`                       | same                                  | odd-length hex rejects.                                                 |
| `tv_cf_03_dqa_hex_rejects_non_hex_chars`                    | same                                  | non-hex characters reject.                                              |
| `tv_cf_04_hex_id_32_round_trip_string`                      | `tests/tv_cf_hex_id_32_round_trip.rs` | 64-char hex round-trips through `hex_id_32`.                            |
| `tv_cf_05_hex_id_32_accepts_legacy_array_form`              | same                                  | 32-element array form parses.                                           |
| `tv_cf_06_hex_id_32_rejects_short_hex`                      | same                                  | short hex rejects.                                                      |
| `tv_cf_07_canonical_amount_max_reparses_through_input_form` | `caveat/mod.rs` lib test              | `Caveat::AmountMax`'s `canonical_ser` re-parses through the input form. |
| `tv_cf_08_canonical_vault_reparses_through_input_form`      | same                                  | `Caveat::Vault`'s `canonical_ser` re-parses through the input form.     |
| `tv_cf_09_legacy_vault_array_form_still_parses`             | same                                  | legacy 32-element array form on `Caveat::Vault` continues to parse.     |

### Step 6: Invert the guide vector

In `crates/octo-cli/src/commands/capability.rs`, rename
`guide_canonical_form_is_not_reparseable` to
`guide_canonical_form_is_reparseable` and invert the assertion. The doc
comment records the inversion.

## Layer model

- `octo-cap-macaroon` (Layer B) — RFC-driven and additive; the change is a
  new `visit_str` arm + a new module + a `#[serde(with = ...)]` on one enum arm
  - 3 new tests. No field removals, no variant additions. It is **not** frozen,
    which is why AC-9 below is a change-scope check rather than a freeze check.
- `octo-cli` (Layer C) — one test vector inverted in Step 6.

## Acceptance Criterion

The mission is closed when the following are true (each AC is paired with its
negative control):

- **AC-1:** `dqa_serde::field::deserialize` accepts a 64-hex string and
  decodes to the same `Dqa` as the 16-byte byte-array form. Pinned by
  `tv_cf_01_dqa_hex_round_trip`.
- **AC-1-NC (negative control):** reverting the `visit_str` arm makes AC-1
  fail. `tv_cf_01_dqa_hex_round_trip` is green with the `visit_str` arm
  present and failing without it.
- **AC-2:** `Caveat::Vault` accepts both forms on input — the 64-hex form
  (canonical) and the 32-element array form (legacy, preserved for
  migration). Pinned by `tv_cf_08` + `tv_cf_09`.
- **AC-2-NC (negative control):** removing the `#[serde(with = ...)]` from
  `Caveat::Vault` makes AC-2 fail. `tv_cf_09_legacy_vault_array_form_still_parses`
  passes under both conditions (the legacy array form is always accepted
  without the adapter), so the failure mode for AC-2-NC is the loss of the
  hex-form acceptance — `tv_cf_08_canonical_vault_reparses_through_input_form`
  fails when the adapter is removed.
- **AC-3:** `cargo test -p octo-cap-macaroon --lib` green (257 lib tests,
  including the 3 in-crate `tv_cf_07..09`).
- **AC-4:** `cargo test -p octo-cap-macaroon --tests` green (14 integration
  vectors under default features, 16 under `--features hex-ids`).

  Counts corrected 2026-10-03. These ACs previously read 259 and 6, written
  when the amendment carried 13 vectors. The adversarial review added six
  (`tv_cf_14..16`, `tv_cf_18..21`), and the two feature configs are mutually
  exclusive per file via `cfg` attributes. Measured per file, not estimated.

- **AC-5:** `cargo test -p octo-cli --lib guide_canonical_form_is_reparseable`
  green (the inverted vector).
- **AC-6:** `cargo clippy -p octo-cap-macaroon --all-targets -- -D warnings`
  clean.
- **AC-7:** `cargo clippy -p octo-cli --all-targets -- -D warnings` clean.
- **AC-8:** `cargo fmt --all -- --check` clean.
- **AC-9:** change-scope check: `git diff crates/octo-cap-macaroon` shows
  only `dqa_serde.rs`, `hex_id_32.rs` (new), `caveat/mod.rs` (one enum arm
  attribute + 3 tests), `caveat/payment.rs` (one adapter now delegating to
  `hex_id_32`), `lib.rs` (one `pub mod`), and the new test files. Recorded as a
  scope check rather than a freeze check because the crate is Layer B.

## Files / Artifacts

- Edit: `crates/octo-cap-macaroon/src/dqa_serde.rs` (add `visit_str` + `visit_string`)
- New: `crates/octo-cap-macaroon/src/hex_id_32.rs` (32-byte hex adapter)
- Edit: `crates/octo-cap-macaroon/src/lib.rs` (export `hex_id_32`)
- Edit: `crates/octo-cap-macaroon/src/caveat/mod.rs` (`#[serde(with = ...)]`
  on `Caveat::Vault` + 3 lib tests)
- New: `crates/octo-cap-macaroon/tests/tv_cf_dqa_hex_round_trip.rs` (3 tests)
- New: `crates/octo-cap-macaroon/tests/tv_cf_hex_id_32_round_trip.rs` (3 tests)
- Edit: `crates/octo-cli/src/commands/capability.rs` (invert guide vector)

## Cross-references

- RFC-0011 §Caveat Catalog — the canonical envelope `{ type, value }` and
  the `AmountMax`/`Payment.budget`/`Vault` 16-byte `DqaEncoding` hex form.
- RFC-0011 §Hex32 newtype — the 32-byte hex newtype this amendment extends
  to substrate-owned serialization paths.
- `rfcs/draft/0011-caveat-form-amendment.md` — the amendment RFC.
- `docs/plans/2026-10-02-caveat-form-alignment.md` Phase 1 — the plan.
- `docs/audits/2026-09-30-open-limitations-drift-audit.md` §9 — the closed
  residual that this mission closes.
- Companion mission `0011-vault-asset-chain-id-hex` (paired Layer B).

## Out of scope

- `hex-ids` feature flag and the `AssetId` / `ChainId` / `VaultId` newtype
  hex adapters (owned by `0011-vault-asset-chain-id-hex`).
- `CaveatSummaryView` field rename `{ kind, body }` → `{ type, value }` for
  the CLI envelope (owned by Phase 3 of the plan).
- The `guide_canonical_form_is_reparseable` test is a Layer C CLI test; the
  underlying substrate fix it exercises is this mission's scope.

## Dependencies

- `missions/claimed/0011-vault-asset-chain-id-hex.md` — paired Layer B mission
  (the Layer B adapters reuse this mission's `hex_id_32`).
- `rfcs/draft/0011-caveat-form-amendment.md` — the amendment RFC.

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                        |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-10-02 | Initial filing per Caveat Form Alignment plan Phase 0. Substrate-owned serialization paths for the 16-byte `DqaEncoding` payload (`dqa_serde::field`) and 32-byte ids (`hex_id_32`). Paired with Layer B mission `0011-vault-asset-chain-id-hex` for the newtype hex feature. |
