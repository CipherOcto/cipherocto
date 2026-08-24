---
name: 0105-v3-private-namespace-rollout
description: Land RFC-0105 v3.0+ §2.2 private-namespace substrate coverage: add TV-P1..N byte-exact private-asset fixtures for `PRIVATE-{chain_id_32B-hex}-{asset_name}` variants to `crates/octo-vault/tests/test_vectors.rs`; verify `AssetId::derive("PRIVATE-{hex}-{name}")` byte-equality with `octo_determin::asset_id_for`; add `tv_p1_private_namespaces_round_trip` parallel to existing `tv_d9_vectors_cover_role_tokens_exactly_once`; document private-asset substrate form in `crates/octo-vault/src/lib.rs` doctring. RFC-0105 v3.4 substrate anchor chain unbroken.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-23T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0105-v-asset-id-addendum
    - 0105-v2-role-token-canonicalization
    - RFC-0105
status: OPEN
---

# Mission `0105-v3-private-namespace-rollout` v1.0 — OPEN 2026-08-23

## Context

RFC-0105 v3.0 introduced two asset namespaces (sovereign `OCTO-*` vs private `PRIVATE-{chain_id_32B-hex}-{asset_name}`) without breaking the existing substrate derivation path. RFC-0105 v3.4 (canonical Accepted 2026-08-23) inherits v3.0's framing. Existing substrate coverage:

- `octo_determin::asset_id_for(role_token)` (Layer A frozen external git-deps crate) accepts arbitrary `&str` input.
- `AssetId::derive(role_token)` wrapper at `crates/octo-vault/src/lib.rs:140` delegates to substrate; no code change needed to accept `PRIVATE-*` strings.
- Test vector registry at `crates/octo-vault/tests/test_vectors.rs:316` covers the 9 sovereign `OCTO-*` tokens via `tv_d9_vectors_cover_role_tokens_exactly_once`. No private-namespace test vectors exist on disk.

Gap: no byte-exact verification for private-namespace asset_id derivation across multi-`chain_id` + multi-`asset_name` combinations. Consumers implementing `AssetId::derive("PRIVATE-{hex}-{name}")` cannot verify against canonical substrate output without an explicit test vector suite.

## Scope

### Step 1: Substrate form docstring update

`crates/octo-vault/src/lib.rs` `AssetId::derive` doctring (around `lib.rs:140`) — extend to mention both sovereign (`OCTO-*`) and private (`PRIVATE-{chain_id_32B-hex}-{asset_name}`) coverage per RFC-0105 economics amendment (sovereign + private namespace framing). Cite RFC-0105 cross-reference to make the substrate form explicit.

### Step 2: TV-P1 byte-exact fixtures

Add `tv_p1_private_namespaces_round_trip` test function to `crates/octo-vault/tests/test_vectors.rs` parallel to existing `tv_d9_vectors_cover_role_tokens_exactly_once` (line 316). Fixture set:

- 4 `chain_id` variants — 32-byte hex (use canonical fixtures from RFC-0010 v1.7 §2 + v1.9.2 §2):
  - 0x01 Rfc: example chain (deterministic 32-byte hex pinned in TV)
  - 0x02 User: example chain (deterministic 32-byte hex pinned in TV)
  - edge: 32-zero chain_id (chain_id = all zeros — must still derive deterministically; cross-RFC byte-0 overwrite absence confirmed)
  - edge: 0xff-padded chain_id (deterministic stress; cross-RFC byte-0 overwrite absence confirmed)
- 5 `asset_name` variants: `USDC`, `CUSTOM-ASSET`, `Pi-Coin-Test`, `a` (single char), `trailing-dash-` (1 char boundary)
- = 20 fixture combinations = 20 byte-exact asset_id sequences

Helper function `assert_asset_id_derivation` (parallel to TV-D9 helper) verifies `AssetId::derive(role_token)` byte-equal `octo_determin::asset_id_for(role_token)` byte-equal pinned fixture asset_id.

### Step 3: Cross-crate byte-equality check

Add explicit `octo_vault_private_asset_id_derivation_matches_octo_determin` test (parallel to existing `asset_id_for_matches_octo_vault_asset_id_derive` at `crates/octo-vault/tests/test_vectors.rs:333`) to confirm the substrate wrapper path for PRIVATE-* strings produces byte-identical output to `octo_determin::asset_id_for` directly.

### Step 4: RFC-0105 version history

`rfcs/accepted/economics/0105-v34-private-asset-namespace.md` §6 Version History — add v3.5 row documenting:

- 20 TV-P1 fixture addition
- Substrate form doctring update covering private namespace
- No spec text change (test-only addition; substrate unchanged)

## Acceptance Criterion

- 20 byte-exact TV-P1 fixtures added + pinned in `crates/octo-vault/tests/test_vectors.rs::tv_p1_private_namespaces_round_trip`
- `tv_p1_private_namespaces_round_trip` covers 4 chain_id variants × 5 asset_name variants = 20 fixtures
- `octo_vault_private_asset_id_derivation_matches_octo_determin` cross-crate byte-equality test added
- `crates/octo-vault/src/lib.rs:140` doctring updated to document private-namespace coverage
- RFC-0105 §6 VH v3.5 row added (test-only — no spec text change)
- AC gate: `rg 'PRIVATE-' crates/octo-vault/tests/test_vectors.rs` ≥ 20 hits
- AC gate: `rg 'tv_p1_private_namespaces_round_trip' crates/octo-vault/tests/test_vectors.rs` → 1 hit (function def)
- AC gate: `rg 'PRIVATE-' crates/octo-vault/src/lib.rs` ≥ 1 hit (doctring)
- `cargo test -p octo-vault --test test_vectors` → 35/35 green (existing 15 + 20 TV-P1)
- `cargo test -p octo-vault --doc` → all doctests green (incl. extended doctring)
- `cargo clippy --workspace --all-targets --features full -- -D warnings` green (per `quota-router-core-feature-mutex`)
- `cargo fmt --all -- --check` green

## Files / Artifacts

- Edit: `crates/octo-vault/src/lib.rs` (AssetId::derive doctring — line 140 vicinity)
- Edit: `crates/octo-vault/tests/test_vectors.rs` (TV-P1 fixtures + test functions)
- Edit: `rfcs/accepted/economics/0105-v34-private-asset-namespace.md` §6 VH v3.5 row

## Cross-references

- RFC-0105 (sovereign-namespace substrate form per economics amendment)
- RFC-0105 (private-namespace derivation rule + cross-RFC drift note per economics amendment)
- RFC-0105 §Asset ID Derivation (canonical substrate anchor — `AssetId::derive` + `octo_determin::asset_id_for`)
- RFC-0010 §2 (chain_id 32-byte form for `chain_id_32B-hex` segment)
- Mission `0105-v-asset-id-addendum` (parent — establishes substrate `asset_id_for` + TV-D9)
- Mission `0105-v2-role-token-canonicalization` (sibling — canonical form hyphen)
- Mission `0105-v3-policy-kind-authority-landing` (sibling — separate table migration)

## Out of scope

- `policy_kind_authority` substrate migration (owned by `0105-v3-policy-kind-authority-landing`)
- Cross-RFC byte-0 overwrite drift resolution (owned by RFC-0206 fix-all cascade; substrate is source of truth per RFC-0105 economics amendment cross-RFC drift note)
- Private-asset registration flow (owned by RFC-0010 §4 + downstream corporate-chain onboarding missions)
- Substrate `asset_id_for` implementation change (Layer A frozen; no change)
- New role-token enumeration extension (defer to RFC-0105 economics-amendment future iteration if needed)

## Dependencies

- `0105-v-asset-id-addendum` (substrate `asset_id_for` + TV-D9 must exist)
- `0105-v2-role-token-canonicalization` (canonical hyphen form for `OCTO-*` fixtures)
- RFC-0105 (canonical Accepted state per long-horizon plan v1.6 Phase 4 Tier 1 promotion sequence)

## Version History

| Version | Date       | Change                                                                                                                                                                                                   |
| ------- | ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-23 | Initial filing per cross-RFC harmonization close-out RFC-0105 v3.4 mission audit. Substrate coverage for private-namespace (RFC §2.2) via 20 TV-P1 byte-exact fixtures; doctring extension; VH v3.5 row. |
