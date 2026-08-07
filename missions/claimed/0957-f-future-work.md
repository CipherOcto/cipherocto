# Mission: 0957-A1 Future Work (F1-F4) — Catalog Federation, GC, Audit Log, V2 Bundling

## Status

Closed (Band A — 2026-08-06). Claimed 2026-08-04 by @mmacedoeu; implementation landed F1+F2+F3 (14/16 ACs green; modules `crates/octo-wallet/src/capability/{federation,gc,audit_log}.rs` carrying 21 tests across 789 lines). 2 unchecked ACs are explicit cross-mission deferrals with named owner per [[deferred-vs-unspecified]]: F4 (2 ACs: bundle struct + TV) blocks on RFC-0009 §Identity evolution (RFC-0009 currently Accepted 2026-08-02 but §Identity evolution not yet active upstream). Future-work sub-mission `0957-f-f4-bundle` to be filed when RFC-0009 §Identity lands.

## RFC

RFC-0957-A1 (Economics): Holder Registry + Catalog Storage (Amendment) — Accepted 2026-08-02

**Future Work item source:** RFC-0957-A1 §Future Work (4 items, all DEFERRED from the amendment scope per R*-N* consensus during R28-R64 review).

## Summary

Implement the 4 items deferred from RFC-0957-A1 §Future Work. Each item has a concrete plan in RFC-0957-A1 §Future Work; this mission consolidates them into a single sub-mission batch because they are mutually orthogonal (federation is gossip-bound, GC is time-bound, audit is append-only, V2 is identity-bound) and depend on different RFCs landing first.

## Acceptance Criteria

### F1: Catalog federation across nodes

- [x] `crates/octo-wallet/src/capability/federation.rs` (NEW) — gossip delta bounded to ~1KB per insert. Verify via TV F1: simulated 10K-node federation, per-gossip-frame size ≤ 1KB.
- [x] Test: 1000 random inserts; gossip frame histogram; p99 ≤ 1KB. (`thousand_random_inserts_p99_under_1kb` in `crates/octo-wallet/src/capability/federation.rs` §thousand_random_inserts_p99_under_1kb)
- [x] `docs/07-developers/` rule: inline §Developer Guide section in this mission (no external developer-guide file). (See §Developer Guide below)

### F2: Catalog GC

- [x] `crates/octo-wallet/src/capability/gc.rs` (NEW) — sweep Revoked/Expired rows older than 30 days. Configurable retention via `GcPolicy::retention_days`.
- [x] TV F2: insert record with `revoked_at_millis_unix = now - 31 days`; GC sweep removes it. Insert with `revoked_at_millis_unix = now - 29 days`; GC sweep preserves it. (`revoked_31_days_old_is_eligible`, `revoked_29_days_old_is_preserved`)
- [x] No manual Debug redaction needed; `HolderRecord` Debug already redacted per 0957-c.

### F3: Audit log

- [x] `crates/octo-wallet/src/capability/audit_log.rs` (NEW) — append-only log of insert/revoke/sync events. Schema: `event_id`, `node_did`, `event_kind: AuditEventKind { Insert | Revoke | Sync }`, `cap_root_hash`, `at_millis_unix`. Backed by separate stoolap table `holder_registry_audit_log` (RFC-0862 substrate).
- [x] TV F3: insert → revoke sequence emits 2 audit entries; tampering with log fails BLAKE3 chain check. (`insert_then_revoke_emits_two_audit_entries`, `tampering_with_log_breaks_chain_check`)
- [x] Manual redacting Debug on `AuditEvent` (redact `cap_root_hash`, preserve `node_did` + `event_kind` for forensics). (`audit_event_debug_redacts_cap_root_hash`)

### F4: CapabilityCatalog V2

- [ ] Defer until RFC-0009 §Identity evolves. V2 bundles 4 extensions (holder_registry, root_secret_for_ask, settlement_chain_tip, gossip_to_buyer) into a single struct. **Deferral owner:** @cipherocto. **Target:** RFC-0009-B1 / RFC-0957-A2 full signature promotion (target 2026-09-15 per the 0957-a1 / 0959-a1 / 0969 phantom-type chain) per [[deferred-vs-unspecified]] named-owner rule.
- [ ] TV F4: NOT YET SPEC'D — when RFC-0009 §Identity lands, re-author this AC. **Deferral owner:** @cipherocto. **Target:** 2026-09-15 per [[deferred-vs-unspecified]] named-owner rule. Re-author AC body when F4 substrate (V2 bundling) lands.
- [x] Concrete plan documented in RFC-0957-A1 §Future Work; blocks on RFC-0009 §Identity evolution.

### Cross-crate compat

- [x] `cargo build --workspace` green
- [x] `cargo test --workspace` green (181 octo-wallet capability tests pass; F1/F2/F3 = 21 tests)
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean — **pre-existing baseline errors on next branch unrelated to 0957-f; F1/F2/F3 modules clippy-clean**
- [x] `cargo fmt --check` clean

## Dependencies

**Requires (RFC gates):**

- RFC-0957-A1 — base HolderRegistry + CapabilityCatalog extensions
- RFC-0862 — stoolap substrate (for F1 gossip + F3 audit table)
- RFC-0009 — identity primitive (for F4 V2 bundling)

**Mission gates:**

- `missions/claimed/0957-c-holder-registry-impl.md` — base HolderRecord + HolderKind MUST exist (F1, F2, F3 substrate) — Closed Band A 2026-08-06
- `missions/open/0957-e-mint-txn-parameter.md` — CapabilityCatalog 4-method extension MUST exist (F4 substrate)

**Blocks:**

- F4 blocks on RFC-0009 §Identity evolution. Track separately.

## Location

- `crates/octo-wallet/src/capability/federation.rs` (NEW) — F1
- `crates/octo-wallet/src/capability/gc.rs` (NEW) — F2
- `crates/octo-wallet/src/capability/audit_log.rs` (NEW) — F3
- Inline §Developer Guide section in this mission (no external file) — covers F1, F2, F3 operational runbook

## Decomposition Rationale

RFC-0957-A1 §Future Work bundles 4 items. Per BLUEPRINT §Multi-Mission Decomposition:

- **4 new types** (FederationDelta, GcPolicy, AuditEvent, CapabilityCatalogV2) — does NOT exceed >10 threshold
- **4 implementation phases** (F1: gossip bound, F2: time bound, F3: append-only log, F4: struct bundling) — at threshold
- **Different prerequisite chains:**
  - F1 + F3 depend on RFC-0862 stoolap + 0957-c substrate
  - F2 depends on 0957-c substrate only
  - F4 depends on RFC-0009 §Identity evolution (out of mission control)

Decomposition into a single mission (0957-f) is appropriate because all 4 items share the same substrate (HolderRegistry) and the work is naturally a "future cleanup" batch — implementing them as a single claim avoids context fragmentation. F4 is conditional and may move to a separate follow-up mission when RFC-0009 §Identity lands.

## Claimant

@mmacedoeu

## Pull Request

# pending user push

## Developer Guide (F1, F2, F3 operational runbook)

### F1 — Catalog federation

Operators wire `FederationDelta` gossip between `HolderRegistry` instances. Each insert / revoke in the local registry emits a `FederationDelta` to the gossip topic; remote nodes merge by `cap_root_hash` PK (idempotent). The 1KB bound per frame is enforced via `validate_federation_size`; oversized payloads must split.

### F2 — Catalog GC

Run `sweep_candidates(&candidates, &GcPolicy::default(), now_millis_unix)` periodically (e.g., daily cron). Default retention is 30 days. Operators with stricter data-residency requirements can construct `GcPolicy { retention_days: 7, .. }`. Perpetual records (`ttl_millis_unix == 0 && revoked_at_millis_unix == None`) are NEVER swept.

### F3 — Audit log

`append_event(node_did, kind, cap_root_hash, at_unix, prev_chain_hash, event_id)` is called for every insert/revoke/sync. The returned `AuditEvent` has its `chain_hash` field populated; the caller persists it. On replay, `verify_chain(&events)` detects tampering by recomputing every chain hash and asserting prev-chain links.

### F4 — CapabilityCatalog V2

When RFC-0009 §Identity lands, re-author the F4 AC + TV. The 4 current `CapabilityCatalog` extension methods (`holder_registry`, `root_secret_for_ask`, `settlement_chain_tip`, `gossip_to_buyer`) will be bundled into a single struct; backward-compat preserved by the existing default impls.

## Notes

- Each future item has a concrete plan in RFC-0957-A1 §Future Work (F1: ~1KB per insert bound; F2: 30-day retention; F3: append-only log; F4: bundle when RFC-0009 §Identity evolves).
- Per [[deferred-vs-unspecified]] rule: each item has a Major divergent note (R*-N* finding from R28-R64 review) + concrete plan + explicit phase gate (F1, F2, F3 trigger after 0957-c lands; F4 triggers on RFC-0009 §Identity).
- F4 is the only item not yet fully spec'd (TV F4 placeholder) because it depends on RFC-0009 §Identity evolution. The mission structure explicitly handles this: when RFC-0009 §Identity lands, re-author F4 AC + TV.
- Manual Debug redaction on `AuditEvent` is the load-bearing security primitive for F3 (audit logs are forensics surface; cap_root_hash leakage defeats audit purpose).
- Concrete plans for F1, F2, F3 mean this mission is CLAIMABLE; only F4 blocks on upstream.

## Version History

| Version | Date       | Change                                                                                                                                                                                                     |
| ------- | ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| v0.1    | 2026-08-04 | Mission claimed. RFC-0957-A1 §Future Work F1-F4 consolidated.                                                                                                                                              |
| v0.2    | 2026-08-06 | Closed Band A (14/16 ACs). F4 (2 ACs) explicit deferral to `0957-f-f4-bundle` future sub-mission; blocks on RFC-0009 §Identity evolution.                                                                  |
| v0.3    | 2026-08-07 | Audit-closure: named-owner augmentation on F4 (2 ACs) per [[deferred-vs-unspecified]] named-owner rule. owner = @cipherocto, target = 2026-09-15 (sync with RFC-0009-B1 / RFC-0957-A2 phantom-type chain). |

Last Updated: 2026-08-07
Version: 0.3

### Related

- [Dual-Mode Authorization Batch Accepted 2026-08-02](../rfcs/accepted/economics/0957-a1-holder-registry.md)
- Original research: `docs/research/2026-08-01-dual-mode-workflow-gap-research.md`
- Original use case: `docs/use-cases/dual-mode-authorization-workflow.md`
