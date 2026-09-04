---
name: rfc-0011-d-m10-m11-mission-closeout-2026-09-03
description: RFC-0011-d M10 + M11 mission close-out 2026-09-03 with INLINE drift-fix
metadata:
  type: project
---

# RFC-0011-d M10 + M11 Mission Close-Out — CLOSED 2026-09-03

`next` HEAD after R1 hygiene commit c0e86dc8 + substrate commit 96bccc4b.
Both missions `Open` → `Completed` + drift-fixed INLINE per user direction
(`in place, not separated amendments`).

## Mission transitions

```
missions/open/0011-d-M10-phase2-coordinator-domain-coordinator.md      →  missions/archived/completed/
missions/open/0011-d-M11-phase2-domain-coordinator-platform-binding.md →  missions/archived/completed/
```

Both frontmatter updates: `v: "1.3" → "1.4"`, `status: Open → Completed`,
`completed: 2026-09-03`, `commit: 96bccc4b`. All 17 ACs (M10) + 14 ACs (M11)
marked `[x]`.

## Drift-fix INLINE summary (10 findings)

| ID                        | Sev      | Drift                                                                 | Fix                                                                                                                                             |
| ------------------------- | -------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| F-M11-sig                 | CRITICAL | M11 §Substrate signature `role_binding: &RoleBinding` (pre-drift-fix) | `operator_did: &str` (matches `crates/octo-network/src/dc/admin_attest.rs:330` canonical substrate)                                             |
| F-M11-test-name           | MEDIUM   | `bind_domain_coordinator_rolls_back_on_nonce_replay` (speculative)    | `bind_domain_coordinator_rolls_back_on_stale_proof` (actual; substrate uses stale-proof check) + added missing `rejects_non_canonical_did` test |
| F-M10-AC-list             | MEDIUM   | M10 ACs named speculative CLI test fixtures                           | Aligned to actual 17 test fn names (10 substrate + 7 CLI)                                                                                       |
| F-M10-partial-prereq      | MEDIUM   | "Partial-prereq guard" defense-in-depth AC retained                   | Removed (gate CLEARED 2026-09-02; no regression risk; no speculative defensive code per `[[cipherocto-design-principles]]`)                     |
| F-M10-error-variant       | LOW      | `RoleNotSelectable` reuse claimed                                     | `RoleError::GroupBindingRejected { reason }` documented (6th variant; exit code 36)                                                             |
| F-M10-export-count        | LOW      | "2 new exports"                                                       | 3 exports (`select_coordinator` + `select_domain_coordinator` + `build_handover_request` helper per §Interface Segregation)                     |
| F-M10-arg-path            | LOW      | `--platform-admin-proof <path>`                                       | `--platform-admin-proof <json>` (inline JSON per Layer C surface)                                                                               |
| F-M10-3-coordinator-roles | MEDIUM   | 3 coordinator roles not enumerated                                    | `domain-coordinator` + `mission-coordinator` + `witness-coordinator` documented with class + slashing rules                                     |
| F-M11-non-exhaustive-ctor | LOW      | `#[non_exhaustive]` mentioned without constructor                     | `PlatformAdminProof::new` documented (drift-fix C closure)                                                                                      |
| F-M10-pubkey-from-did     | LOW      | `pubkey_from_did` helper not enumerated                               | Phase 1 helper + 4 regression tests documented                                                                                                  |

## Test summary

| Mission | New tests | Substrate                           | CLI                           |
| ------- | --------- | ----------------------------------- | ----------------------------- |
| M10     | 17        | 10 (octo-role/select.rs)            | 7 (octo-cli/commands/role.rs) |
| M11     | 6         | 6 (octo-network/dc/admin_attest.rs) | —                             |

Total: 23 new tests (M10 + M11 combined).

## Audit trail

- `docs/audits/2026-09-03-0011-d-M10-M11-mission-closeout.md` — close-out audit (gitignored scratchpad)
- `docs/audits/2026-09-03-rfc-0011-d-m10-m11-substrate-truth-reconciliation.md` — substrate truth reconciliation audit (gitignored scratchpad)
- `memory/rfc-0011-d-m10-m11-substrate-impl-2026-09-03.md` — substrate impl closure card
- `memory/rfc-0011-d-m10-m11-mission-closeout-2026-09-03.md` — THIS close-out closure card

## User-owned follow-on actions

- `git push origin next` + PR `next → main` (covers commits c0e86dc8 + 96bccc4b + mission close-out)
- RFC-0011-d v1.7.1 PR review (7-day RFC review window)
- 11 atomic RFC-0011-d missions now COMPLETE; RFC-0011 amendment chain (a/b/e/f/g) independent

NO PUSH.

## Cross-RFC invariants preserved

- `RecorderDid` canonical keying (RFC-0968 §28.4 amend 22)
- `HARD_THRESHOLD = 5` (slash_store + dc_store)
- `0x0100` cross-domain slash code (RFC-0855p-c §9c)
- `CoordinatorRole` enum (`MissionCoordinator=0x00`, `DomainCoordinator=0x01`, `WitnessCoordinator=0x02`)
- `HandoverReason::Voluntary = 0x00` for self-selecting coordinators
- `MAX_ATTEST_AGE_EPOCHS = 100` (RFC-0855p-c §5a)
- `pubkey_from_did` Phase 1 form (`did:octo:0x<hex>`); production swap point for `octo_ident::WireDid` per RFC-0010
- `parse_hash32_hex` canonical lowercase alphabet (RFC-0010 §OctoID Codec)

## Why

Goal: "proceed to mission close out" — M10 + M11 missions transitioned
`Open` → `Completed` + drift-fixed INLINE per established pattern
(`in place, not separated amendments`). 10 drift findings closed across both
YAMLs. Layer direction preserved. Cross-RFC invariants intact. RFC-0011-d
Phase 2 entrypoints now substrate-complete + mission lifecycle complete.

**How to apply:** when next RFC-0011 amendment lands (RFC-0011-a vault ops,
RFC-0011-b reputation, RFC-0011-e vault, RFC-0011-f mesh, RFC-0011-g governance),
follow the same drift-fix-then-close-out workflow. Mission YAMLs must
match canonical substrate truth at closure time; do not defer drift-fixes to
separate amendments.
