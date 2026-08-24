---
name: 0903-D1-substrate-landing
description: NEW mission owning LiteLLM persistence substrate for RFC-0903-D1 v1.0 — 5 tables (litellm_users, litellm_keys, scim_users, scim_groups, scim_group_members) + v006-v010 schema migrations + canonical 16-byte asset_id derivation per RFC-0010 §Data Model + RFC-0008 §RFC-0008 Execution Class Mapping table. Surfaces LiteLLM substrate gap (Cat E from audit 2026-08-24) + mission mislabel (`0903-d-budget-enforcement` body = "Key Cache (L1)" not budget-enforcement — historical file preserved per historical-mission-preservation; new mission owns correct substrate).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0903-d-budget-enforcement
    - 0903-virtual-api-key
    - 0862-c-cross-process-atomicity
    - RFC-0903-D1
    - RFC-0903
    - RFC-0010
    - RFC-0008
status: OPEN

**Retro-supersession (2026-08-24 Session 2 RFC-0903-D1 substrate audit):** Substrate LANDING DEFERRED — schema migration numbering conflict. `crates/quota-router-storage/migrations/v006` through `v010` already occupied by substrate indexes from prior missions:
- `v006__create_outbox.sql` (quota-router-storage substrate, occupied)
- `v007__create_spend_ledger.sql` (quota-router-storage substrate, occupied)
- `v008__create_did_registry.sql` (quota-router-storage substrate, occupied)
- `v009__add_service_endpoints_and_controllers.sql` (octo-reputation substrate, occupied)
- `v010__add_verification_methods_and_capability_delegations.sql` (octo-reputation substrate, occupied)

Per RFC-0903-D1 v1.0 §2 substrate mandate, the LiteLLM persistence migrations need migration numbers `v011__create_litellm_users.sql` through `v015__create_scim_group_members.sql` (next free numbers after v010). Mission text preserved with original `v006-v010` per historical-mission-preservation + R19 scope discipline. Stoolap registry impls + 25 TV byte-exact fixtures ALSO NOT LANDED — substrate landing requires coordinated migration renumbering + 5 registry trait impls + test fixture generation, beyond single-session scope. Per claim-and-implement scope, substrate landing remains OPEN for follow-up commit when migration renumbering can be coordinated with quota-router-storage + octo-reputation owners. Cross-RFC harmonization + Layer B additive-only compliance verification per RFC-0206 §4 remains IN SCOPE. NO PUSH per `feedback_initiation_user_only`.
---

# Mission `0903-D1-substrate-landing` v1.0 — OPEN 2026-08-24

## Context

RFC-0903-D1 v1.0 (canonical Accepted per `rfcs/accepted/economics/0903-d1-litellm-persistence.md` YAML `version: 1.0` + `status: Accepted`) defines LiteLLM persistence substrate as 5 tables (`litellm_users` + `litellm_keys` + `scim_users` + `scim_groups` + `scim_group_members`) backed by §3 Execution Class Mapping per RFC-0008. Mission audit 2026-08-24 surfaced 2 gap categories:

1. **Cat E — LiteLLM substrate PENDING** (no dedicated 0903-D1 mission exists; substrate landing is implied by `0903-d-budget-enforcement` mission text but body is "Key Cache (L1)" — filename mislabel per historical-mission-preservation preserved; new mission owns correct substrate)
2. **Cat B — no dedicated 0903-D1 mission** (RFC-0903-D1 v1.0 has no mission claiming it; `0903-d-budget-enforcement` predates the v1.0 amendment filing)

This mission owns the LiteLLM persistence substrate landing work — 5 tables + 5 migrations + canonical 16-byte asset_id derivation for `litellm_keys.key_hash` (per RFC-0010 §Data Model + RFC-0008 §RFC-0008 Execution Class Mapping).

## Substrate work scope (NEW — owned by this mission)

### Step 1: v006-v010 schema migrations LANDED verification

```bash
ls crates/quota-router-storage/migrations/v006*.sql \
   crates/quota-router-storage/migrations/v007*.sql \
   crates/quota-router-storage/migrations/v008*.sql \
   crates/quota-router-storage/migrations/v009*.sql \
   crates/quota-router-storage/migrations/v010*.sql
```

Expected: 5 SQL files for `litellm_users` (v006) + `litellm_keys` (v007) + `scim_users` (v008) + `scim_groups` (v009) + `scim_group_members` (v010). All 5 LANDED per RFC-0903-D1 v1.0 §2.

### Step 2: Stoolap-based LiteLLM registry impl

Implement `LitellmUserRegistry` + `LitellmKeyRegistry` + `ScimUserRegistry` + `ScimGroupRegistry` + `ScimGroupMemberRegistry` (5 trait impls in `crates/octo-quota-router/src/storage/` per RFC-0206 §4 Layer B additive-only pattern).

Per RFC-0903-D1 v1.0 §3 Execution Class Mapping:

| Operation                       | Execution Class (RFC-0008) | Substrate                                                       |
| ------------------------------- | -------------------------- | --------------------------------------------------------------- |
| `LitellmUserRegistry::register` | Class A (RFC-0008)         | `crates/octo-quota-router/src/storage/litellm_user_registry.rs` |
| `LitellmUserRegistry::resolve`  | Class A (RFC-0008)         | same                                                            |
| `LitellmKeyRegistry::rotate`    | Class A (RFC-0008)         | `crates/octo-quota-router/src/storage/litellm_key_registry.rs`  |
| `ScimUserRegistry::provision`   | Class A (RFC-0008)         | `crates/octo-quota-router/src/storage/scim_user_registry.rs`    |

### Step 3: Canonical 16-byte asset_id derivation

Per RFC-0010 §Data Model (canonical 16-byte BE scale 12 DqaEncoding), `litellm_keys.key_hash` derives via:

```rust
let key_hash: [u8; 16] = blake3::derive_key(
    "cipherocto/litellm-key/v1/",
    b"|".join([&user_id.to_le_bytes()[..], &key_fingerprint]).as_slice()
);
```

Domain separator canonical `octo/`-prefix pattern per F-R8-DOMSEP-PREFIX-DRIFT hygiene convention. (NOTE: RFC-0903-D1 v1.0 §5 may use `cipherocto/`-prefix per RFC-0104 DFP domain separator policy — verify per RFC-0903-D1 v1.0 §5 EXACT string before commit.)

### Step 4: Test fixtures + TV byte-exact

5 TV per registry (1 happy path + 1 idempotency + 1 rotation + 1 SCIM provisioning + 1 cross-table integrity) — 25 TV total byte-exact against canonical 16-byte asset_id derivation.

```bash
cargo test -p octo-quota-router --test tv_0903_d1_litellm_persistence
# Expected: 25/25 PASS
```

### Step 5: Layer discipline verification

Per RFC-0008 §Specification:

- `octo-quota-router` (Layer B-adjacent, RFC-0008 Class A execution) — additive only
- `octo-ident` (Layer B frozen) — UNCHANGED (canonical 16-byte asset_id derivation uses existing `ChainId::as_bytes` + `NamespaceVariant` substrate)
- `octo-policy` (Layer B) — UNCHANGED (no policy changes for RFC-0903-D1 v1.0)

## Inline retrofix candidate (audit 2026-08-24)

### Retrofix candidate: `0903-d-budget-enforcement` mission mislabel

**Defect:** Filename says `budget-enforcement` but body says "Mission: Key Cache (L1)" — references RFC-0903 Virtual API Key, NOT RFC-0903-D1 LiteLLM persistence.

**Evidence:**

1. `cat missions/claimed/0903-d-budget-enforcement.md | head -5` → "Mission: Key Cache (L1)" — references RFC-0903 (Virtual API Key) not RFC-0903-D1 (LiteLLM persistence).
2. RFC-0903-D1 v1.0 §0 Status declares separate dedicated mission required.

**Fix decision:** Per historical-mission-preservation discipline, archived `0903-d-budget-enforcement.md` left as-is (filename + body content both predate RFC-0903-D1 v1.0 amendment filing; represents committed work at its filing time). New `0903-D1-substrate-landing.md` mission owns the LiteLLM persistence substrate (correct substrate scope + correct RFC amendment reference).

**Status:** No inline retro-supersession applied to `0903-d-budget-enforcement` — historical file preserved.

## Acceptance Criterion

- 5 schema migrations LANDED (v006-v010)
- 5 Stoolap registry impls LANDED
- 25 TV byte-exact PASS
- `cargo clippy -p octo-quota-router --all-targets -- -D warnings` clean
- `cargo fmt --all -- --check` clean
- AC gate: `rg 'CREATE TABLE litellm_users' crates/quota-router-storage/migrations/` → 1 hit
- AC gate: `rg 'CREATE TABLE scim_group_members' crates/quota-router-storage/migrations/` → 1 hit
- AC gate: `cargo test -p octo-quota-router --test tv_0903_d1_litellm_persistence 2>&1 | tail -3` → "test result: ok. 25 passed; 0 failed"
- Cross-RFC cite validation: Guard 2 PASS

## Files / Artifacts

- New: `crates/quota-router-storage/migrations/v006__create_litellm_users.sql`
- New: `crates/quota-router-storage/migrations/v007__create_litellm_keys.sql`
- New: `crates/quota-router-storage/migrations/v008__create_scim_users.sql`
- New: `crates/quota-router-storage/migrations/v009__create_scim_groups.sql`
- New: `crates/quota-router-storage/migrations/v010__create_scim_group_members.sql`
- New: `crates/octo-quota-router/src/storage/litellm_user_registry.rs`
- New: `crates/octo-quota-router/src/storage/litellm_key_registry.rs`
- New: `crates/octo-quota-router/src/storage/scim_user_registry.rs`
- New: `crates/octo-quota-router/src/storage/scim_group_registry.rs`
- New: `crates/octo-quota-router/src/storage/scim_group_member_registry.rs`
- New: `crates/octo-quota-router/tests/tv_0903_d1_litellm_persistence.rs`

## Cross-references

- RFC-0903-D1 v1.0 (canonical Accepted — `rfcs/accepted/economics/0903-d1-litellm-persistence.md`)
- RFC-0903 (parent Virtual API Key RFC)
- RFC-0010 (canonical DID codec — `ChainId::as_bytes` 32-byte form for canonical 16-byte asset_id derivation)
- RFC-0008 (Deterministic AI Execution Boundary — §RFC-0008 Execution Class Mapping table)
- RFC-0206 (Value Transfer Surface — §4 Layer B additive-only pattern)
- Mission `0903-d-budget-enforcement` (claimed — historical mislabel; preserved as-is)
- Mission `0903-virtual-api-key` (claimed — parent RFC-0903 substrate)
- Mission `0862-c-cross-process-atomicity` (LANDED `5fce8604` — cross-process fs2 lock substrate for SCIM provisioning atomicity)
- Sibling coordination: `0903-D1-alignment-coordination` (NEW coordination summary)

## Out of scope

- Budget enforcement substrate for RFC-0903 (owned by `0903-d-budget-enforcement` — historical file preserved)
- Inline retro-supersession of `0903-d-budget-enforcement` (per historical-mission-preservation + R19 scope discipline)
- RFC-0903-D1 v1.0 §5 SCIM provisioning atomicity cross-process handler (owned by `0862-c-cross-process-atomicity` mission LANDED)
- RFC-0903-D1 v1.0 §6 LiteLLM proxy route registration (separate mission TBD per RFC-0903-D1 v1.0 Dependencies)
- Cross-RFC harmonization edits (research doc + companion RFC cross-refs) per `vault-monetary-research-consequence` Phase 5 (separate phase)

## Dependencies

- `0903-d-budget-enforcement` (claimed — historical mislabel; substrate scope misattributed but file preserved)
- `0903-virtual-api-key` (claimed — parent RFC-0903 substrate)
- `0862-c-cross-process-atomicity` (LANDED `5fce8604` — cross-process atomicity)
- RFC-0903-D1 v1.0 (canonical Accepted)
- RFC-0903 (parent Virtual API Key RFC)
- RFC-0010 (canonical DID codec substrate)
- RFC-0008 (Execution Class Mapping)
- RFC-0206 §4 (Layer B additive-only pattern)

## Version History

| Version | Date       | Change                                                                                                                                                                                                                                                                                                                                                                              |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per RFC-0903-D1 v1.0 mission audit 2026-08-24. NEW mission owning LiteLLM persistence substrate (5 tables + 5 migrations + canonical 16-byte asset_id derivation + 25 TV). Closes Cat E (LiteLLM substrate PENDING) + Cat B (no dedicated 0903-D1 mission). Historical `0903-d-budget-enforcement` mission preserved per historical-mission-preservation discipline. |
