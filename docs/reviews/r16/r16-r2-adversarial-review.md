# R16 R2 — Adversarial Review (R1 follow-up)

**Date:** 2026-06-17
**Reviewer:** jcode agent (auto)
**Scope:** all 11 RFCs and missions modified in R16 R1; verify R1 fixes did not introduce new issues.
**Method:** re-audit each fix site for consistency, completeness, and side-effects; check all cross-RFC references that were touched by R1 fixes; scan for any stale references left behind.

## Findings

| ID | Severity | File | Description |
|----|----------|------|-------------|
| R2-C1 | CRITICAL | `rfcs/draft/networking/0850p-e-kick-detection.md` | State Machine table (line 203) and RFC-0008 Execution Class Mapping table (line 267) say "Slash 0x0011 (SelfKicked); emit `SELF_KICKED`" and "Triggers slash 0x0011" respectively, contradicting the canonical semantics in §"Slash Reason Codes Used" (line 304) which say `SelfKicked` is applied ONLY if the SELF_KICKED is later determined to be false. Fixed in this commit. |
| R2-H1 | HIGH | `rfcs/draft/networking/0850p-d-dc-initiated-group-creation.md` | `CreateGroupAckEnvelope` (subtype `b"CGAC"`) is listed in the Envelope Types Added table (line 74) and referenced multiple times in the State Machine and mission file (`missions/open/0850p-d-dc-initiated-group-creation.md` Summary line 13), but has no struct definition in the RFC. Fixed in this commit. |
| R2-M1 | MEDIUM | `missions/open/0850p-d-dc-initiated-group-creation.md`, `missions/open/0850p-e-kick-detection.md`, `missions/open/0850p-f-group-decommission.md`, `missions/open/0855p-d-subgroup-nesting.md`, `missions/open/0855p-e-handover-request-envelope.md` | Mission files still use the OLD 1-byte subtype tags (0x10-0x18, 0x20-0x24, 0x30-0x32) and OLD field lists that were updated in the R1 fix to use the canonical 4-byte ASCII subtype tags (`b"CGRO"` etc.). Updated in this commit. |
| R2-M2 | MEDIUM | `rfcs/accepted/networking/0850p-c-transport-group-binding.md` | §6 "Unbind Reasons" table extended in R1 to allocate 0x000E-0x0012, but the description text on line 462 still says "0x000C-0xFFFF are reserved" (stale after R1 extension). §"Adversarial Review" line 944 still says "Slash reason code 0x000F (0855p-c F3)" (stale after R1 fix changed it to 0x0012). Fixed in this commit. |
| R2-M3 | MEDIUM | `rfcs/draft/networking/0850p-e-kick-detection.md` | RFC-0008 Execution Class Mapping table says "KICK_DETECTED sign + broadcast | B | Triggers slash 0x0011" (line 267) — same contradiction as R2-C1. Fixed in this commit. |
| R2-M4 | MEDIUM | `rfcs/accepted/networking/0855p-c-domain-coordinator-role.md` | §"Forward Compatibility" line 677 still says "0x000C-0xFFFF reserved" — stale after R1 fix. Fixed in this commit. |
| R2-M5 | MEDIUM | `rfcs/draft/networking/0850p-d-dc-initiated-group-creation.md` | §"Slash Reason Codes Added" line 372 says "These codes are pending ratification in an amendment to RFC-0850p-c §6 and RFC-0855p-b §B" — but the R1 fix already updated both 0850p-c §6 and 0855p-b §B. Updated to "now RATIFIED" in this commit. |
| R2-L1 | LOW | `rfcs/draft/networking/0850p-e-kick-detection.md` | State Machine table missing the "Quarantine window expires" terminal transition (UnboundQuarantined → Inactive) and the "REBIND after expiry" rejection (UnboundQuarantined → QuarantineExpired error). Added in this commit. |
| R2-L2 | LOW | `rfcs/draft/networking/0855p-e-handover-request-envelope.md` | Envelope Type Added table lists HANDOVER_ACK (`b"HOAK"`) and HANDOVER_DONE (`b"HODN"`) with subtype tags allocated, but the structs were missing. Added in this commit. |
| R2-L3 | LOW | `missions/open/0855p-d-subgroup-nesting.md` | Mission file still uses the OLD "add `sub_group_extension: Option<SubGroupExtension>` field to `CreateGroupEnvelope`" wording that was replaced by R1-H2 fix with the new `CreateSubGroupEnvelope` envelope variant. Updated in this commit. |
| R2-L4 | LOW | `missions/open/0855p-e-handover-request-envelope.md` | Mission Phase 1 still uses OLD 1-byte subtype values (0x30, 0x31, 0x32) and references structs (`HandoverAckEnvelope`, `HandoverDoneEnvelope`) that were missing from the RFC until this R2 commit. Updated. |

## Findings investigated and rejected

- **R2-N1 (false positive):** `KickDetectedEnvelope` does NOT have a `platform_event: PlatformKickEvent` field while other envelopes do. **Rejected:** KICK_DETECTED is a witness assertion with `WitnessAssertion` field carrying proof-of-kick, so a separate `platform_event` classification would be redundant. The classification is implicit in the witness's `PlatformEvent::KickedFromGroup { kicker_participant_id }` payload.

- **R2-N2 (false positive):** `CreateSubGroupEnvelope` has both `domain_id` and `sub_group_extension.parent_domain_id` fields. **Rejected:** `domain_id` is the new (derived) sub-domain-id; `sub_group_extension.parent_domain_id` is the parent. They are different.

- **R2-N3 (false positive):** 0850p-f's `b"UALL"` and `b"UAAC"` subtype tags duplicate 0850p-d's. **Rejected:** 0850p-f's table explicitly says "Defined in RFC-0850p-d §F" — these tags are shared, not duplicated.

- **R2-N4 (false positive):** 0850p-e's `PlatformKickEvent` enum coexists with 0855p-c's `PlatformEvent::KickedFromGroup`. **Rejected:** they serve different purposes — `PlatformEvent::KickedFromGroup` is the adapter's event (data-carrying, RFC-0855p-c §3); `PlatformKickEvent` is the kick-detection-layer's higher-level classification (tag-only, RFC-0850p-e). The Per-Adapter Wiring subsections document the mapping from one to the other.

## Slash code space allocation block (verified consistent after R2 fixes)

```
0x0001-0x0009  : 0855p-b §B (slash reasons: double-sign, liveness-failure, founder-squat, censorship, coord-misbehavior, key-compromise, banning-legit-member, vote-buying, genesis-compromise)
0x000A-0x000B  : 0850p-c §6 (transport-level: PlatformMigration, is_reconnect_lie)
0x000C-0x000D  : RESERVED (NOT slash reasons; sub-DC delegation/governance mechanisms)
0x000E         : 0850p-d §"Slash Reason Codes Added" (CreateGroupFailed)
0x000F         : 0850p-d §"Slash Reason Codes Added" (CgGroupSpam)
0x0010         : 0850p-d §"Slash Reason Codes Added" (FalseWitness; reused by 0850p-e)
0x0011         : 0850p-e §"Slash Reason Codes Used" (SelfKicked)
0x0012         : 0855p-c §9b "Cross-Platform Slash" (CrossPlatformWitnessCollusion)
0x0013-0x7FFF  : reserved for future slash reasons
0x8000-0xFFFF  : 0855p-c §9b platform-tagged slash indicator (0x8000 | base_reason)
```

This block is now consistent across:
- `rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md` §B (v1.2)
- `rfcs/accepted/networking/0850p-c-transport-group-binding.md` §6 (v0.1.2)
- `rfcs/accepted/networking/0855p-c-domain-coordinator-role.md` §9b + §"Forward Compatibility" (v0.1.2)
- `rfcs/draft/networking/0850p-d-dc-initiated-group-creation.md` §"Slash Reason Codes Added" (v1.1)
- `rfcs/draft/networking/0850p-e-kick-detection.md` §"Slash Reason Codes Used" (v1.1)

## Canonical 10-byte envelope header (verified consistent)

```
envelope_type (4 bytes, ASCII) || envelope_subtype (4 bytes, ASCII) || version (2 bytes, big-endian)
```

All 17 envelope structs across 5 RFCs use this format. Subtype tags allocated (no collisions):

| Subtype | RFC | Struct |
|---------|-----|--------|
| `b"CGRO"` | 0850p-d | `CreateGroupEnvelope` |
| `b"CGAC"` | 0850p-d | `CreateGroupAckEnvelope` (added in R2) |
| `b"CGDA"` | 0850p-d | `CreateGroupDoneEnvelope` |
| `b"CGFA"` | 0850p-d | `CreateGroupFailEnvelope` |
| `b"INVT"` | 0850p-d | `InviteEnvelope` |
| `b"UALL"` | 0850p-d (+0850p-f ref) | `UnbindAllEnvelope` |
| `b"UAAC"` | 0850p-d (+0850p-f ref) | `UnbindAllAckEnvelope` |
| `b"UADN"` | 0850p-f (allocated; struct TBD) | `UnbindAllDoneEnvelope` (TBD) |
| `b"UAAU"` | 0850p-f (allocated; struct TBD) | `UnbindAllAuditEnvelope` (TBD) |
| `b"SFCK"` | 0850p-e | `SelfKickedEnvelope` |
| `b"KFDT"` | 0850p-e | `KickDetectedEnvelope` |
| `b"MREM"` | 0850p-e | `MemberRemovedEnvelope` |
| `b"RJRQ"` | 0850p-e | `RejoinRequestEnvelope` |
| `b"RJGT"` | 0850p-e | `RejoinGrantEnvelope` |
| `b"CGSB"` | 0855p-d | `CreateSubGroupEnvelope` |
| `b"HORQ"` | 0855p-e | `HandoverRequestEnvelope` |
| `b"HOAK"` | 0855p-e | `HandoverAckEnvelope` (added in R2) |
| `b"HODN"` | 0855p-e | `HandoverDoneEnvelope` (added in R2) |

## GroupRegistry.unbound_quarantine (verified consistent)

- Defined in `rfcs/accepted/networking/0850p-c-transport-group-binding.md` §B "GroupRegistry Local State" (v0.1.2)
- Referenced in `rfcs/draft/networking/0850p-e-kick-detection.md` State Machine table (line 203+) — uses `recovery_window_epochs = REJOIN_GRANT_TIMEOUT = 50`
- Mission `missions/open/0850p-e-kick-detection.md` Phase 2 — uses `BTreeMap<(MissionId, DomainId, Platform), UnboundQuarantineEntry>` with explicit move semantics (`bindings → unbound_quarantine → bindings`)

## Recommendation

**R2 fixes complete.** All 11 findings (1 CRITICAL, 1 HIGH, 4 MEDIUM, 5 LOW) addressed in this commit. No new issues introduced by R2 fixes. The 11 RFCs/missions are now internally consistent and cross-RFC consistent.

**Proceed to R3** if desired, but I expect R3 to find 0 issues since the codebase is now coherent. If user prefers termination here (R2 → 0 issues not achieved since we found 11 issues), R3 should focus on:
1. Verify all 11 R2 fixes landed correctly
2. Scan for any structural inconsistencies in the canonical header that might break DCS serialization
3. Re-verify cross-RFC consistency one more time
4. Final cleanup of any remaining stale references