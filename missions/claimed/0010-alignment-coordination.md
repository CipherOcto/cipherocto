---
name: 0010-alignment-coordination
description: Coordination summary for RFC-0010 mission alignment per audit 2026-08-24. Documents 5 inline retrofix categories surfaced by RFC-0010 v1.6 amendment spec audit (3 status-stale + 1 path-drift + 1 bare-version-pin) + 1 pre-existing substrate defect tracked (F-P5.2-3 RETAIN — borsh generic bounds in rich_did_document_tv) + 1 new sibling mission (`0010-f8-rich-did-documents-clippy-fix`). NO scope of its own — pure cross-RFC alignment documentation; existing 0010-* missions preserved untouched per historical-mission-preservation discipline except for inline retrofixes documented below per R19 scope discipline.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0010-c-32-byte-addendum
    - 0010-f2-multi-chain-did-resolution
    - 0010-f2-registry-namespacing
    - 0010-f2-multi-chain-routing
    - 0010-f8-rich-did-storage
    - 0010-f8-rich-did-documents-clippy-fix
    - 0206-009-adapter-crate-creation
    - RFC-0010
status: OPEN
---

# Mission `0010-alignment-coordination` v1.0 — OPEN 2026-08-24

## Context

RFC-0010 (canonical Accepted 2026-07-27 + v1.6 amendment 2026-08-19 per `rfcs/accepted/process/0010-canonical-did-codec.md` Status header) is the canonical DID codec RFC. RFC-0010-v17 amendment (per `rfcs/accepted/process/0010-v17-chain-id-registration-authority.md` YAML `version: 1.9.2`) extends with chain-id registration authority substrate. Mission audit 2026-08-24 surfaced 5 retrofix categories for existing 0010-* missions (3 status-stale + 1 path-drift + 1 bare-version-pin) + 1 new sibling mission to fix pre-existing borsh generic bounds defect (`0010-f8-rich-did-documents-clippy-fix`).

This mission captures the audit findings + references the 1 new sibling mission that owns the clippy-fix work. **This mission is documentation-only** — it does not edit any existing 0010-* mission file beyond inline retrofixes documented below per historical-mission-preservation discipline (existing OPEN/CLAIMED/LANDED mission state represents committed work at its filing time and is preserved where possible; only stale placeholders and clear contradictions receive inline retrofixes per R19 scope discipline).

## Inline retrofixes applied (2026-08-24 audit)

### Retrofix 1: `0010-c-32-byte-addendum` status stale

**Defect:** Mission status `OPEN` claims `ChainId::as_bytes()` + 3 TV fixtures + RFC-0010 v1.6 VH row are PENDING. All LANDED.

**Evidence:**

1. `crates/octo-ident/src/chain.rs:137` — `pub fn as_bytes(&self) -> [u8; 32]` (BLAKE3 derivation via domain separator `b"cipherocto/chain/v1/"` per Layer A frozen substrate pattern).
2. `crates/octo-ident/tests/tv_0010_chain_id_32byte.rs` — 3 TV (`as_bytes_is_deterministic_across_n_calls`, `as_bytes_known_vector_matches_blake3_256`, `canonical_bytes_17_and_as_bytes_32_coexist`) PASS verified via `cargo test -p octo-ident --test tv_0010_chain_id_32byte`.
3. `rfcs/accepted/process/0010-canonical-did-codec.md:1017` — RFC-0010 v1.6 VH row documents `ChainId::as_bytes()` 32-byte addendum + cross-ref to mission `0010-c-32-byte-addendum` + 3 TV byte-exact fixtures.
4. `cargo test -p octo-ident --lib` 69/69 PASS (no regression in `canonical_bytes()` 17-byte form).
5. `cargo clippy -p octo-ident --test tv_0010_chain_id_32byte -- -D warnings` clean.
6. `cargo fmt --all -- --check` clean.

**Fix:** Inline retro-supersession note added to frontmatter (combined drift into single quote for readability). Mission body preserved verbatim per historical-mission-preservation + R19 scope discipline. AC-7 (all-targets clippy) noted as blocked by pre-existing defect in `rich_did_document_tv` (per F-P5.2-3 RETAIN).

### Retrofix 2: `0010-f2-multi-chain-did-resolution` status stale + bare version pin

**Defect:** Mission status `claimed` claims substrate PENDING (commit `f6478bda` not yet mentioned). 5 inline bare version pin violations (RFC-0010 v1.3 / v1.4) per CLAUDE.md §RFC Reference Conventions.

**Evidence:**

1. `git log --oneline -1 f6478bda` → `feat(octo-ident): 0010-f2-multi-chain-did-resolution (RFC-0010 v1.4)` — LANDED 2026-08-11.
2. `crates/octo-ident/src/chain.rs:81` `pub struct ChainId(pub String)` + `:220` `pub struct ChainNamespace` + `:308` `pub enum NamespaceVariant` + `:55` `CIPHEROCTO_MAINNET_TAG` const + `:257` `ChainNamespace::canonical_bytes()` 17-byte form.

**Fix:** Inline retro-supersession note added to Status block (combined status + bare pin correction into single quote). Mission body preserved verbatim per historical-mission-preservation + R19 scope discipline. Bare pins retro-corrected in header `Substrate:` line only.

### Retrofix 3: `0010-f2-registry-namespacing` status stale + path drift + bare version pin

**Defect:** Mission status `open` claims substrate PENDING. Path references `crates/quota-router-storage/src/stoolap_did_registry.rs` (stale — substrate moved to `octo-ident-storage` per `0206-009` adapter crate pattern). 5 inline bare version pin violations (RFC-0010 v1.4).

**Evidence:**

1. `git log --oneline -1 a7efaabb` → `feat(octo-ident,quota-router-storage): 0010-f2-registry-namespacing — multi-chain registry column` — LANDED 2026-08-11.
2. `crates/octo-ident/src/registry.rs:71` `pub trait DidRegistry` gains 2 ADDITIVE methods (`register_in_chain` at `:199` + `resolve_in_chain` at `:219`) with default impls.
3. `crates/octo-ident-storage/src/did_registry.rs` — `MAINNET_CHAIN_ID_BYTES` const + 4 SQL filter updates (re-exported from `crates/octo-ident-storage/src/lib.rs:34`).
4. `crates/quota-router-storage/migrations/v011__add_chain_id_namespace.sql` — ADD COLUMN + UNIQUE INDEX migration LANDED.
5. `crates/octo-ident-storage/tests/chain_namespace.rs` — 1 TV `register_in_chain_isolates_dids_across_chains` LANDED.

**Fix:** Inline retro-supersession note added to Status block (combined status + path + bare pin correction into single quote). Mission body preserved verbatim per historical-mission-preservation + R19 scope discipline.

### Retrofix 4: `0010-f2-multi-chain-routing` status stale + bare version pin

**Defect:** Mission status `open` claims wire-protocol + handler + TV PENDING. 3 inline bare version pin violations (RFC-0010 v1.4).

**Evidence:**

1. `crates/octo-protocol/src/payload_kind.rs` — `IDENTITY_RESOLVE_WITH_CHAIN` payload kind UUID `0x0009:0001:...:0005` + `IDENTITY_RESOLVER_PAYLOAD_KINDS` array + `identity_payload_kinds_are_distinct` test (5 distinct kinds).
2. `crates/octo-identity-resolver-node/src/handlers/resolve_with_chain.rs` (NEW) — `ResolveWithChainHandler` + `ResolveWithChainRequest` + borsh (de)serialization.
3. `crates/octo-identity-resolver-node/src/node.rs` — `handle_envelope` dispatch arm.
4. `crates/octo-identity-resolver-node/tests/resolve_with_chain.rs` (NEW) — 1 TV `resolve_with_chain_isolates_dids_across_chains` (102 lines, dispatch + handler + mainnet vs partner chain isolation).

**Fix:** Inline retro-supersession note added to Status block. Mission body preserved verbatim per historical-mission-preservation + R19 scope discipline.

### Retrofix 5: `0010-f8-rich-did-storage` status stale + path drift

**Defect:** Mission status `claimed` claims v009 + v010 PENDING. Path references `crates/quota-router-storage/src/stoolap_did_registry.rs` (stale — substrate moved to `octo-ident-storage` per `0206-009`).

**Evidence:**

1. `crates/quota-router-storage/migrations/v009__add_service_endpoints_and_controllers.sql` + `v010__add_verification_methods_and_capability_delegations.sql` — both LANDED.
2. `crates/octo-ident-storage/src/did_registry.rs:162-173` — borsh serialization of 4 rich fields (`service_endpoints`, `controllers`, `verification_methods`, `capability_delegations`).
3. `crates/octo-ident-storage/src/did_registry.rs:202-204,229-231` — SQL bind params for 4 new BLOB columns.
4. `cargo test -p octo-ident-storage --lib` 20/20 PASS.

**Fix:** Inline retro-supersession note added to Status block. Mission body preserved verbatim per historical-mission-preservation + R19 scope discipline.

## Gaps surfaced by RFC-0010 audit

### Gap 1: Pre-existing borsh generic bounds defect in `rich_did_document_tv`

**Coverage gap:** 6 E0277 compile errors per `cargo clippy -p octo-ident --all-targets -- -D warnings`. TV file `crates/octo-ident/tests/rich_did_document_tv.rs` (LANDED via commit `a5ffd8ef`) has missing BorshDeserialize derive on round-trip test wrapper type. Blocks `0010-c-32-byte-addendum` AC-7 (all-targets clippy) indirectly via shared all-targets clippy invocation.

**Owned by mission:** `0010-f8-rich-did-documents-clippy-fix` (sibling; 1-line borsh derive fix).

### Gap 2: Bare version pin residual

Multiple inline bare version pin references remain in mission body text (not retro-corrected). Per CLAUDE.md §RFC Reference Conventions, only inline retro-supersession notes + status block quotes were corrected; deep body retro-correction is out of R19 scope.

**Owned by:** future batch retrofix cycle (deferred per R19).

## Sibling mission cross-references

- `0010-f8-rich-did-documents-clippy-fix` — primary substrate ownership for borsh generic bounds defect (1-line fix + clippy validation)

## Acceptance Criterion

- 5 inline retrofixes applied to `0010-c` + `0010-f2-multi-chain-did-resolution` + `0010-f2-registry-namespacing` + `0010-f2-multi-chain-routing` + `0010-f8-rich-did-storage` per audit findings
- 1 sibling mission filed (`0010-f8-rich-did-documents-clippy-fix`) + cross-references 1 retrofix mission + 1 dependency mission via `depends_on` chain
- AC gate: `ls missions/claimed/0010-*.md` → 10 files (9 existing + 1 new clippy-fix)
- AC gate: `rg 'Retro-supersession \(2026-08-24 audit\)' missions/claimed/0010-c-32-byte-addendum.md missions/claimed/0010-f2-multi-chain-did-resolution.md missions/claimed/0010-f2-registry-namespacing.md missions/claimed/0010-f2-multi-chain-routing.md missions/claimed/0010-f8-rich-did-storage.md` → 5 hits (1 retro-supersession note per retrofixed mission)
- AC gate: `rg 'pub trait DidRegistry' crates/octo-ident/src/registry.rs` → 1 hit (substrate anchor for retrofix 3)
- AC gate: `rg 'IDENTITY_RESOLVE_WITH_CHAIN' crates/octo-protocol/src/payload_kind.rs` → 1 hit (substrate anchor for retrofix 4)
- Cross-RFC cite validation: Guard 2 PASS for all 5 retrofixed + 1 new mission files
- Prettier clean
- No new INVALID cites introduced

## Files / Artifacts

- Edit: `missions/claimed/0010-c-32-byte-addendum.md` (frontmatter retro-supersession note)
- Edit: `missions/claimed/0010-f2-multi-chain-did-resolution.md` (Status block retro-supersession note + Substrate header bare pin correction)
- Edit: `missions/claimed/0010-f2-registry-namespacing.md` (Status block retro-supersession note combining status + path + bare pin)
- Edit: `missions/claimed/0010-f2-multi-chain-routing.md` (Status block retro-supersession note combining status + bare pin)
- Edit: `missions/claimed/0010-f8-rich-did-storage.md` (Status block retro-supersession note combining status + path)
- New: `missions/claimed/0010-f8-rich-did-documents-clippy-fix.md` (pre-existing defect substrate ownership)
- New: `missions/claimed/0010-alignment-coordination.md` (this file)

## Cross-references

- RFC-0010 (canonical Accepted 2026-07-27 + v1.6 amendment 2026-08-19 — `rfcs/accepted/process/0010-canonical-did-codec.md`)
- RFC-0010-v17 (YAML `version: 1.9.2` — `rfcs/accepted/process/0010-v17-chain-id-registration-authority.md`)
- RFC-0206 §4 (Layer B additive-only migration rule — applies to all v009/v010/v011 schema migrations)
- Mission `0010-a-canonical-did-codec-crate` (LANDED Band A 2026-08-06 — codec substrate)
- Mission `0010-b-canonical-did-codemod` (LANDED Band A 2026-08-06 — codemod substrate)
- Mission `0010-c-32-byte-addendum` (retrofix target 1)
- Mission `0010-d-wallet-audience-validation` (LANDED `d9070a78` 2026-08-09 — F4 wallet audience)
- Mission `0010-f2-multi-chain-did-resolution` (retrofix target 2)
- Mission `0010-f2-registry-namespacing` (retrofix target 3)
- Mission `0010-f2-multi-chain-routing` (retrofix target 4)
- Mission `0010-f8-rich-did-documents` (LANDED `a5ffd8ef` 2026-08-11 — rich DidDocument substrate; pre-existing borsh defect source)
- Mission `0010-f8-rich-did-storage` (retrofix target 5)
- Mission `0010-f8-rich-did-documents-clippy-fix` (sibling — borsh defect substrate ownership)
- Mission `0206-009-adapter-crate-creation` v1.0 (LANDED per MEMORY — `octo-ident-storage` adapter crate)
- Mission `0871b-storage-backend` (LANDED `71f8d745` 2026-08-11 — DidRegistry substrate DAG predecessor)
- Sibling coordination: `0959-alignment-coordination` + `0960-alignment-coordination` + `0967-A1-alignment-coordination` (cross-RFC harmonization pattern)

## Out of scope

- Retroactive supersession of older 0010-* missions beyond the 5 inline retrofixes (per R19 scope discipline)
- Deep body bare version pin retro-correction (out of R19; deferred to future batch cycle)
- RFC-0010 v1.7 chain-id registration authority substrate landing (separate mission TBD per RFC-0010-v17 YAML frontmatter)
- RFC-0010 §Future Work F1 (W3C DID method registration), F3 (capability key derivation), F4 (already LANDED via 0010-d), F5, F6, F7 (cross-instance coordination) — out of scope
- 108 byte-exact vault_id TV (LANDED via `0960-vault-substrate-amendment`; out of scope)
- Cargo command text rewrites (e.g., `cargo test -p cipherocto-policy` → `cargo test -p octo-policy`) in retrofix target missions — historical mission text preserved verbatim; only retro-supersession notes added per R19
- Cross-RFC harmonization edits (research doc + companion RFC cross-refs) per `vault-monetary-research-consequence` Phase 5 (separate phase)

## Dependencies

- All 5 retrofix target missions (parent coverage)
- `0010-f8-rich-did-documents-clippy-fix` (sibling — borsh defect substrate ownership)
- `0206-009-adapter-crate-creation` (LANDED — `octo-ident-storage` adapter crate pattern)
- RFC-0010 (canonical Accepted state + v1.6 amendment)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                   |
| ------- | ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| v1.0    | 2026-08-24 | Initial filing per RFC-0010 mission audit 2026-08-24. 5 inline retrofix categories (3 status-stale + 1 path-drift + 1 bare-version-pin) + 1 sibling mission for pre-existing borsh defect substrate ownership. Pure coordination; no new substrate code in this mission. |
