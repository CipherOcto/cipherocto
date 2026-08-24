---
name: 0010-f8-rich-did-documents-clippy-fix
description: Fix pre-existing borsh generic bounds defect in `crates/octo-ident/tests/rich_did_document_tv.rs` blocking `cargo clippy -p octo-ident --all-targets -- -D warnings`. Surfaced by RFC-0010 mission audit 2026-08-24; tracked separately per F-P5.2-3 RETAIN framework (substrate fabrication category — out of R19 inline-retrofix scope for retrofix cycle but blocks AC-7 of mission `0010-c-32-byte-addendum` indirectly via shared all-targets clippy invocation). 6 compile errors per `rustc --explain E0277` — borsh `from_slice<T: BorshDeserialize>` generic bound mismatch on round-trip types. Owner: layered substrate fix; mission owns the clippy-clean closure pass.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0010-f8-rich-did-documents
    - RFC-0010
status: OPEN
---

# Mission `0010-f8-rich-did-documents-clippy-fix` v1.0 — OPEN 2026-08-24

## Context

Mission `0010-f8-rich-did-documents` (LANDED 2026-08-11, commit `a5ffd8ef`) shipped `crates/octo-ident/tests/rich_did_document_tv.rs` (NEW file, 5 byte-exact TV for `DidDocument` rich fields). The TV file has pre-existing borsh generic bounds defect surfaced during RFC-0010 mission audit 2026-08-24:

```
error[E0277]: the trait bound is not satisfied
   --> /home/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/borsh-1.5.0/src/de/mod.rs:976:22
    |
976 | pub fn from_slice<T: BorshDeserialize>(v: &[u8]) -> Result<T> {
    |                      ^^^^^^^^^^^^^^^^ required by this bound in `from_slice`

error: could not compile `octo-ident` (test "rich_did_document_tv") due to 6 previous errors
```

Per F-P5.2-3 RETAIN framework (substrate fabrication category), this defect is out of R19 inline-retrofix scope for retrofix cycle but tracked separately. Mission `0010-c-32-byte-addendum` AC-7 (`cargo clippy --all-targets -p octo-ident -- -D warnings`) is blocked by this defect — `cargo clippy -p octo-ident --test tv_0010_chain_id_32byte -- -D warnings` PASSES, confirming 0010-c substrate is clean and the defect is isolated to `rich_did_document_tv`.

## Scope

### Step 1: Diagnose borsh generic bounds mismatch

Inspect `crates/octo-ident/tests/rich_did_document_tv.rs` for 6 round-trip types missing `borsh::BorshDeserialize` derive:

- `ServiceEndpoint` — `derive(borsh::BorshSerialize, borsh::BorshDeserialize)` at `crates/octo-ident/src/rich_document.rs:71` ✓ has both
- `VerificationMethod` — same at `:181` ✓
- `VerificationMethodKind` — same at `:216` ✓
- `ControllerReference` — same at `:253` ✓
- `CapabilityDelegation` — same at `:277` ✓

5 rich types already have borsh derives. Likely culprit = TEST wrapper type (e.g., `RichDocumentFixture`) or feature-gated derive misconfiguration on `octo-ident/borsh` feature flag.

### Step 2: Apply borsh derive fix

Add `BorshDeserialize` derive to whichever round-trip type(s) in the TV file are missing it. Likely 1-line fix per missing derive + import statement update if `borsh` not in scope.

### Step 3: Verify clippy clean

```bash
cargo clippy -p octo-ident --all-targets -- -D warnings
# Expected: clean (no errors, no warnings)
```

### Step 4: Verify TV passes

```bash
cargo test -p octo-ident --test rich_did_document_tv
# Expected: 5/5 PASS (TV-1..TV-5 from mission 0010-f8-rich-did-documents LANDED)
```

## Layer model

- `octo-ident` (Layer B) — test-only fix; production code unchanged. Per Layer B additive-only rule, no breaking changes to public API.

## Acceptance Criterion

- `cargo clippy -p octo-ident --all-targets -- -D warnings` clean (no E0277 errors)
- `cargo test -p octo-ident --test rich_did_document_tv` 5/5 PASS
- `cargo test -p octo-ident --lib` 69/69 PASS (no regression in canonical codec + chain namespace tests)
- `cargo test -p octo-ident --test tv_0010_chain_id_32byte` 3/3 PASS (no regression in 0010-c TV fixtures)
- `cargo fmt --all -- --check` clean
- AC gate: `rg 'E0277|BorshDeserialize' crates/octo-ident/tests/rich_did_document_tv.rs` → 0 E0277 references + ≥1 BorshDeserialize derive per round-trip type
- AC gate: `cargo clippy -p octo-ident --all-targets -- -D warnings 2>&1 | grep -c 'error\[E'7` → 0

## Files / Artifacts

- Edit: `crates/octo-ident/tests/rich_did_document_tv.rs` (add BorshDeserialize derive to missing round-trip type + import statement if needed)

## Cross-references

- Mission `0010-f8-rich-did-documents` (LANDED 2026-08-11 `a5ffd8ef` — substrate author)
- Mission `0010-c-32-byte-addendum` (AC-7 blocked by this defect; resolution unblocks 0010-c AC-7 closure)
- Mission `0010-alignment-coordination` (coordination summary parent)
- `crates/octo-ident/src/rich_document.rs` (5 rich types — all have borsh derives; culprit = TV file test wrapper type)
- `crates/octo-ident/Cargo.toml` (`borsh` feature gate at `:33` per mission `0010-f8-rich-did-documents` LANDED)

## Out of scope

- Substrate fabrication defects in other crates (RETAIN framework)
- Borsh upgrade from 1.5.0 to newer version (separate version-bump mission)
- Public API changes to `DidDocument` or rich types
- Cargo.toml `borsh` feature gate reconfiguration (currently functional; the defect is in TV file)

## Dependencies

- `0010-f8-rich-did-documents` (LANDED 2026-08-11 — TV file substrate)
- RFC-0010 (canonical Accepted 2026-07-27 + v1.6 amendment 2026-08-19)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                             |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per RFC-0010 mission audit 2026-08-24. Pre-existing borsh generic bounds defect in `rich_did_document_tv` blocking all-targets clippy on `octo-ident`. Owned by layered substrate fix; tracked separately from inline retrofix cycle per F-P5.2-3 RETAIN framework. |
