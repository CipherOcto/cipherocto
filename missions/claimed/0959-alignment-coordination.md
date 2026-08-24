---
name: 0959-alignment-coordination
description: Coordination summary for RFC-0959 v2.0 mission alignment per audit 2026-08-24. Documents 3 inline retrofix categories surfaced by RFC-0959 v2.0 spec audit + scope of 2 new sibling missions for 11-step recon remaining scope (0959-c1-wire-A-substrate-verify + 0959-c1-wire-B-rfc-tv). NO scope of its own — pure cross-RFC alignment documentation; existing 0959-* missions preserved untouched per historical-mission-preservation discipline except for inline retrofixes documented below.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-24T00:00:00.000Z
  v: "1.0"
  depends_on:
    - 0959-a-ask-pricing-stoolap
    - 0959-a1-market-delivery
    - 0959-b-market-delivery-impl
    - 0959-b1-mdelivery-types-drift
    - 0959-c-delivery-gossip-integration
    - 0959-c1-gossip-error-variants
    - 0959-c1-wire-format-amendment
    - 0959-c2-cross-node-delivery
    - 0959-c3-octo-transport-wiring
    - 0959-c4-composite-catalog
    - 0959-placeholder-identity-binding
    - RFC-0959
status: OPEN
---

# Mission `0959-alignment-coordination` v1.0 — OPEN 2026-08-24

## Context

RFC-0959 v2.0 (canonical Accepted 2026-08-19) extends v1.0 with wire-format amendment (`cost_vault_id` + `chain_id` on `SettlementEnvelope` + cross-chain settlement reject per §20.7). Mission audit 2026-08-24 surfaced 3 retrofix categories for existing 0959-* missions + 2 new sibling missions to cover remaining 11-step recon scope.

This mission captures the audit findings + references the 2 new sibling missions that own the remaining substrate alignment work. **This mission is documentation-only** — it does not edit any existing 0959-* mission file beyond inline retrofixes documented below per historical-mission-preservation discipline (existing OPEN/LANDED mission state represents committed work at its filing time and is preserved where possible; only stale placeholders and clear contradictions receive inline retrofixes per R19 scope discipline).

## Inline retrofixes applied (2026-08-24 audit)

### Retrofix 1: `0959-b1-mdelivery-types-drift` commit placeholder

**Defect:** Mission Status block + AC-D1 + AC-D2 closure text all reference `<TBD>` commit hash placeholder.

**Evidence:** `git log --oneline -- crates/octo-wallet/src/capability/market_delivery.rs` shows `eec6fb51 feat(capability): 0959-b1 AC-D1 + AC-D2 — DeliveryError 14 variants + RoleTag RFC-0971 alignment` is the actual commit.

**Fix:** Inline edit replaces 3x `<TBD>` → `eec6fb51`. No semantic change.

### Retrofix 2: `0959-c2-cross-node-delivery` test file drift

**Defect:** Mission AC text references `cargo test -p octo-wallet --test cross_node_delivery` (single file); ground state has 2 test files (`cross_node_delivery.rs` original + `cross_node_delivery_transport.rs` added by `0959-c3-octo-transport-wiring` Band A closure).

**Evidence:** `ls crates/octo-wallet/tests/cross_node*` returns 2 files; both test suites green (4/4 + 6/6).

**Fix:** Inline retro-supersession note added to `Notes` section documenting the production-wiring test file addition by `0959-c3-octo-transport-wiring.md` Band A closure. Mission AC text left intact (4/4 in-process harness tests still pass per AC text). Per R19 scope discipline, retro-supersession note preferred over AC text edit.

### Retrofix 3: `0959-c3-octo-transport-wiring` substrate path correction

**Defect:** Mission Summary text + Cargo dep references `octo_transport::NodeTransport`; actual substrate home = `crates/octo-cap-macaroon-transport/src/lib.rs:73` (`pub struct TransportDeliveryCatalog`) in `octo_cap_macaroon_transport` crate (Layer B extension glue crate per RFC-0957-A1 §Layer Discipline).

**Evidence:** `rg 'pub struct TransportDeliveryCatalog' crates/` → 1 hit at `crates/octo-cap-macaroon-transport/src/lib.rs:73`. Cargo dep actually added is `octo-cap-macaroon-transport` not `octo-transport`.

**Fix:** Inline substrate correction note added to Summary step 1; RFC list extended with RFC-0959 v2.0 reference (cross-link to `0959-c1-wire-format-amendment`). Mission slug + Summary text retained for historical preservation per R19.

## Gaps surfaced by RFC-0959 v2.0 audit

### Gap 1: `SettlementEnvelope` v2.0 struct field extension missing

RFC-0959 v2.0 Status header documents `cost_vault_id: Option<[u8; 32]>` + `chain_id: Option<[u8; 32]>` on `SettlementEnvelope` per §8.4.1 + §20.7. The substrate `SettlementEnvelope` struct at `crates/quota-router-storage/src/ask.rs:984` carries `cost: Dqa` (v1.0) but NOT the v2.0 fields. The DAO `settlement_event_repo.rs:56` carries `cost_vault_id` + `chain_id` (column-level) but the struct itself needs field extension.

**Coverage gap:** consumers deserializing v2.0 envelopes cannot match canonical substrate form without struct field extension.

**Owned by mission:** `0959-c1-wire-A-substrate-verify` (sibling; struct field extension + `compute_settlement_hash` v2.0 preimage + `SettlementError::ChainMismatch` + `verify_settlement_chain_match` function).

### Gap 2: `settlement_verify.rs` module missing

RFC-0959 v2.0 §Settlement-Time Vault Row Lookup (NEW subsection per recon step 8) requires a `verify_settlement_chain_match(envelope, vault_lookup)` function that reuses `octo_cap_macaroon::VaultLookup` trait. Module does NOT exist on disk.

**Coverage gap:** RFC documents the 3-step algorithm in prose but no Rust implementation exists.

**Owned by mission:** `0959-c1-wire-A-substrate-verify` (sibling; module creation + Cargo dep + lib.rs pub use).

### Gap 3: migrations/v016 status + 25 byte-exact TV missing

Per recon 2026-08-19: v016 migration `settlement_chain_vault.sql` should exist (recon says LANDED via `0900-d-chain-aware-slash-ledger` follow-on); `crates/quota-router-storage/tests/tv_0959_settlement_wire.rs` (25 byte-exact fixtures) does NOT exist on disk.

**Coverage gap:** consumers cannot verify byte-exact v1.0/v2.0 envelope hash preimage without TV file.

**Owned by mission:** `0959-c1-wire-B-rfc-tv` (sibling; v016 verification/creation + 25 TV file + RFC-0959 v2.0 amendment documentation).

## Sibling mission cross-references

- `0959-c1-wire-A-substrate-verify` — primary substrate ownership for `SettlementEnvelope` v2.0 struct + `settlement_verify.rs` module
- `0959-c1-wire-B-rfc-tv` — primary documentation + test-vector ownership for RFC-0959 v2.0 amendment + 25 byte-exact TV + migrations/v016 verification

## Acceptance Criterion

- 3 inline retrofixes applied to `0959-b1` + `0959-c2` + `0959-c3` per audit findings
- 2 sibling missions filed + cross-reference each other via `depends_on` chain
- AC gate: `ls missions/claimed/0959-c1-wire-{A,B}*.md missions/claimed/0959-alignment-coordination.md` → 3 files
- AC gate: `rg 'eec6fb51' missions/claimed/0959-b1-mdelivery-types-drift.md` → ≥3 hits (commit hash retrofix)
- AC gate: `rg 'cross_node_delivery_transport.rs.*added by.*0959-c3' missions/claimed/0959-c2-cross-node-delivery.md` → 1 hit (retro-supersession note)
- AC gate: `rg 'substrate correction.*2026-08-24' missions/claimed/0959-c3-octo-transport-wiring.md` → 1 hit (substrate path correction note)
- Cross-RFC cite validation: Guard 2 PASS for all 3 new mission files
- Prettier clean
- No new INVALID cites introduced

## Files / Artifacts

- Edit: `missions/claimed/0959-b1-mdelivery-types-drift.md` (commit hash retrofix)
- Edit: `missions/claimed/0959-c2-cross-node-delivery.md` (retro-supersession note)
- Edit: `missions/claimed/0959-c3-octo-transport-wiring.md` (substrate path correction + RFC-0959 v2.0 cross-link)
- New: `missions/claimed/0959-c1-wire-A-substrate-verify.md`
- New: `missions/claimed/0959-c1-wire-B-rfc-tv.md`
- New: `missions/claimed/0959-alignment-coordination.md` (this file)

## Cross-references

- RFC-0959 v2.0 (canonical Accepted; documents wire-format amendment + 3 NEW subsections)
- RFC-0957 (VaultLookup trait reuse per §Verify-Time Extension)
- RFC-0010 v1.6 (chain_id canonical 32-byte form)
- RFC-0960 (vault substrate for cost_vault_id derivation)
- RFC-0967-A1 v1.9.2 §2.5 (Layer B intra-dep justification)
- RFC-0206 §4 (Layer B additive-only migration rule)
- Mission `0959-c1-wire-format-amendment` (parent — owns 11-step recon scope)
- All 11 existing 0959-* missions (preserved untouched per historical-mission-preservation discipline except for inline retrofixes)

## Out of scope

- Retroactive supersession of older 0959-* missions beyond the 3 inline retrofixes (per R19 scope discipline)
- `policy_kind_authority` substrate migration (owned by `0105-v3-policy-kind-authority-landing` per RFC-0967-A1 §2.5)
- `kind_uuid_registry` 30-UUIDv5 namespace seeding (separate future mission per RFC-0967-A1 §2.6)
- Live DID provisioning for treasury + corp_admin signers (separate onboarding flow)
- Cross-RFC byte-0 overwrite drift resolution (owned by RFC-0206 fix-all cascade)

## Dependencies

- All 11 existing 0959-* missions (parent coverage)
- RFC-0959 v2.0 (canonical Accepted state)

## Version History

| Version | Date       | Change                                                                                                                                                                                                    |
| ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v1.0    | 2026-08-24 | Initial filing per RFC-0959 v2.0 mission audit 2026-08-24. 3 inline retrofix categories + 2 sibling missions for remaining 11-step recon scope. Pure coordination; no new substrate code in this mission. |
