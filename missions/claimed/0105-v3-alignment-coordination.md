---
name: 0105-v3-alignment-coordination
description: Coordination summary for RFC-0105 v3.4 mission alignment per cross-RFC harmonization close-out 2026-08-23. Documents the 3 categories of gaps surfaced by RFC-0105 v3.4 spec audit + scope of 2 new sibling missions (0105-v3-private-namespace-rollout + 0105-v3-policy-kind-authority-landing). NO scope of its own — pure cross-RFC alignment documentation; existing 0105-* missions preserved untouched per historical-mission-preservation discipline.
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

# Mission `0105-v3-alignment-coordination` v1.0 — OPEN 2026-08-23

## Context

RFC-0105 v3.4 (canonical Accepted 2026-08-23) introduces the sovereign/private asset-namespace clarification (per R2 finding resolution in v3.0 + R3 substrate-grounding re-write in v3.1 + R9 fix-all cascade in v3.4). Cross-RFC harmonization close-out 2026-08-23 surfaced 3 categories of mission-alignment gaps that have no existing mission coverage on disk.

This mission captures the audit findings + references the 2 sibling missions that own the substrate alignment work. **This mission is documentation-only** — it does not edit any existing 0105-* mission file, per historical-mission-preservation discipline (existing OPEN/LANDED mission state represents committed work at its filing time and MUST NOT be scrubbed retroactively).

## Gaps surfaced by RFC-0105 v3.4 audit

### Gap 1: Private-namespace test vectors missing

RFC-0105 (economics amendment §2.2) specifies the private-namespace derivation path:

```
PRIVATE-{chain_id_32B-hex}-{asset_name} → BLAKE3-256(b"cipherocto/asset/v1/" ‖ "PRIVATE-{chain_id_32B-hex}-{asset_name}")[:32]
```

This uses the same substrate `asset_id_for` path as sovereign `OCTO-*` tokens — no new substrate code needed. **TV-D9 at `crates/octo-vault/tests/test_vectors.rs:316` covers exactly the 9 sovereign `OCTO-*` tokens** (`tv_d9_vectors_cover_role_tokens_exactly_once`). NO byte-exact fixtures exist for `PRIVATE-{hex}-{name}` variants.

**Coverage gap**: consumers implementing `AssetId::derive("PRIVATE-{hex}-{name}")` cannot verify byte-exact equivalence with `octo_determin::asset_id_for` without explicit test vectors.

**Owned by mission**: `0105-v3-private-namespace-rollout` (sibling; 20 TV-P1 byte-exact fixtures + cross-crate byte-equality check + doctring update + RFC-0105 §6 VH v3.5 row).

### Gap 2: `policy_kind_authority` substrate landing deferred

RFC-0105 (economics amendment Authority-to-Issue table) cites `policy_kind_authority` as the substrate table where issuers register signing authority:

| Namespace             | Authority DID                | Registration path                                        |
| --------------------- | ---------------------------- | -------------------------------------------------------- |
| Sovereign (`OCTO-*`)  | octo treasury DID            | `policy_kind_authority` row registered by octo treasury  |
| Private (`PRIVATE-*`) | corporate chain operator DID | `policy_kind_authority` row registered by chain operator |

RFC-0967-A1 v1.9.2 §2.5 specifies the substrate schema:

```sql
CREATE TABLE policy_kind_authority (
    kind_uuid BLOB(16) NOT NULL PRIMARY KEY,
    required_signer_did BLOB(32) NOT NULL,
    authority_kind TEXT NOT NULL  -- 'octo_treasury' | 'corp_admin'
);
```

RFC-0105 (economics amendment) also notes: "RFC-defined substrate-pending landing via mission `vault-chain-metadata` per research doc §16 + RFC-0206 §Layer B additive-only rule".

**Coverage gap**: `policy_kind_authority` table does NOT exist on disk. Substrate `crates/octo-policy-storage/src/lib.rs` declares only `TABLE_POLICY_OBJECTS = "policy_objects"`. The "mission `vault-chain-metadata` per research doc §16" reference from RFC-0105 Authority-to-Issue table is decomposed (per single-concern discipline) into:

- `0105-v3-policy-kind-authority-landing` (this sibling) — owns `policy_kind_authority` table migration v017 + `register_policy` enforcement + bootstrap seed (2 rows: sovereign + private)
- Separate future mission — owns `chain_metadata` table bridge (FK cross-link to `policy_registry.kind_uuid`)

**Owned by mission**: `0105-v3-policy-kind-authority-landing` (sibling; migration v017 + substrate `register_policy` + 2 bootstrap rows per RFC-0967-A1 §2.5 enforcement rules).

### Gap 3: Existing 0105-* missions preserved

Existing missions in `missions/claimed/0105-*.md` + `missions/archived/0105-*.md`:

| Mission                                       | Status   | Cite state                                                                                                                                 |
| --------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `claimed/0105-v-asset-id-addendum`            | OPEN     | RFC-0105 v1.9 → v2.0 — substrate landed externally (not `crates/octo-determin/src/`); canonical form home is RFC-0105 §Asset ID Derivation |
| `claimed/0105-v2-role-token-canonicalization` | OPEN     | RFC-0105 v2.0 → v2.1; canonical hyphen form                                                                                                |
| `claimed/0105-dqa-literal-syntax`             | LANDED   | RFC-0105 v2.1 → v2.2; DQA typed literal syntax                                                                                             |
| `claimed/0105-dqa-consensus-integration`      | DEFERRED | Blocked by DFP consensus integration; explicit unblocker missions filed                                                                    |
| `claimed/0105-x-s4-deferred-codemod-sites`    | LANDED   | Field types migrated u128 → Dqa across 7 crates                                                                                            |
| `archived/0105-dqa-core-type`                 | LANDED   | Substrate `octo_determin::Dqa` type landed                                                                                                 |
| `archived/0105-dqa-datatype-integration`      | LANDED   | DataType enum integration                                                                                                                  |
| `archived/0105-dqa-expression-vm`             | LANDED   | Expression VM dispatch                                                                                                                     |
| `archived/0105-dqa-free-function-exports`     | LANDED   | Re-export surface                                                                                                                          |
| `archived/0105-dqa-integration-tests`         | LANDED   | Cross-crate integration tests                                                                                                              |
| `archived/0105-dqa-test-vectors`              | LANDED   | Central test vector registry (9 + 100 fixtures)                                                                                            |

Per historical-mission-preservation discipline, none of these are modified by this coordination mission. The RFC-0105 v3.4 evolution is documented exclusively in the 2 new sibling missions. Existing OPEN missions may need retroactive supersession notes if their amendment targets (e.g., RFC-0105 v2.0 row, v2.1 row) have been overtaken by RFC-0105 v3.0+ evolution — but per R19 scope discipline that surface is deferred to separate retroactive-supersession sweep.

## Sibling mission cross-references

- `0105-v3-private-namespace-rollout` — primary substrate ownership for RFC-0105 sovereign/private distinction
- `0105-v3-policy-kind-authority-landing` — primary substrate ownership for RFC-0105 Authority-to-Issue table

## Acceptance Criterion

- This mission lands as documentation-only; NO existing 0105-* mission file modified
- 2 sibling missions filed + cross-reference each other via `depends_on` chain
- AC gate: `ls missions/claimed/0105-v3-*.md` → 3 files (alignment-coordination + private-namespace-rollout + policy-kind-authority-landing)
- AC gate: `rg 'RFC-0105 v3\.4 §2\.2' missions/claimed/0105-v3-*.md` → ≥1 hit (private-namespace-rollout §2.2 anchor)
- AC gate: `rg 'RFC-0105 §3' missions/claimed/0105-v3-*.md` → ≥1 hit (policy-kind-authority-landing §3 anchor)
- Cross-RFC cite validation: Guard 2 PASS for all 3 new mission files
- Prettier clean

## Files / Artifacts

- New: `missions/claimed/0105-v3-alignment-coordination.md` (this file)
- Sibling: `missions/claimed/0105-v3-private-namespace-rollout.md`
- Sibling: `missions/claimed/0105-v3-policy-kind-authority-landing.md`

## Cross-references

- RFC-0105 (sovereign-namespace substrate form per economics amendment)
- RFC-0105 (private-namespace derivation rule per economics amendment)
- RFC-0105 (chain-vs-asset namespace clarification per economics amendment)
- RFC-0105 (Authority-to-Issue table per economics amendment)
- RFC-0105 §Asset ID Derivation (canonical substrate anchor — `AssetId::derive` at `crates/octo-vault/src/lib.rs:140` + external `octo-determin` git-deps crate)
- RFC-0967-A1 v1.9.2 §2.5 (`policy_kind_authority` table schema)
- RFC-0010 v1.9.2 §2 (chain_id 32-byte form + namespace-byte semantics) + §4 (Authority Registration Flow)
- RFC-0206 §4 (Layer B additive-only migration ownership rule)
- Research doc §16 cross-reference citation
- Mission `0105-v-asset-id-addendum` (parent — establishes substrate `asset_id_for`)
- Mission `0105-v2-role-token-canonicalization` (sibling — canonical form)
- Mission `0206-001-substrate-newtype` (parent — substrate Database newtype + migration runner)
- Mission `0206-009-adapter-crates` (parent — adapter crate creation)

## Out of scope

- Retroactive supersession of older 0105-* missions (deferred to separate sweep per R19 scope discipline + historical-mission-preservation principle)
- `policy_registry` table migration (separate future mission; sibling to `0105-v3-policy-kind-authority-landing`)
- `chain_metadata` table bridge (separate future mission per research doc §16 decomposition)
- `kind_uuid_registry` 30-UUIDv5 namespace seeding (per RFC-0967-A1 §2.6; separate future mission)
- Live DID provisioning for treasury + corp_admin signers (substrate migration seeds `zeroblob` placeholders; live DID onboarding is a separate flow)
- Cross-RFC byte-0 overwrite drift resolution (owned by RFC-0206 v3.4+ fix-all; substrate is source of truth per RFC-0105 economics amendment cross-RFC drift note)

## Dependencies

- `0105-v-asset-id-addendum` (substrate `asset_id_for` established)
- `0105-v2-role-token-canonicalization` (canonical hyphen form established)
- RFC-0105 v3.4 (canonical Accepted state)

## Version History

| Version | Date       | Change                                                                                                                                                                             |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-23 | Initial filing per cross-RFC harmonization close-out 2026-08-23 RFC-0105 v3.4 mission audit. Documents 3 gap categories + 2 sibling missions; no existing 0105-* mission modified. |
