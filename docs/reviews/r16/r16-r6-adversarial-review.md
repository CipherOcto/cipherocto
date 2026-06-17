# R16 R6 — Adversarial Review (R5 follow-up)

**Date:** 2026-06-17
**Reviewer:** jcode agent (auto)
**Scope:** all 11 RFCs and missions modified in R16 R1+R2+R3+R4+R5. Verify R5 fix landed correctly; perform final comprehensive scan for any new in-scope issues.
**Method:** comprehensive structural scan; cross-RFC slash code consistency final pass; canonical 10-byte header verification; struct reference verification; mission/RFC consistency check; 0xF0xx kick detection code verification; obsolete RFC reference check.

## Findings (in scope)

**No new in-scope issues found.** R6 found 0 new in-scope issues. Per the user's loop termination rule ("loop should end when a new review find no issues"), the R16 review series is now **CLOSED**.

## R5 fix verification (verified correct)

The R5-M1 fix landed correctly:
- `missions/open/0850p-f-group-decommission.md` Phase 1 lines 30-31 now use `b"UALL"` and `b"UAAC"` (canonical 4-byte ASCII tags) instead of `0x15` and `0x16` (stale 1-byte tags).
- The fix is documented in the line text with the R16 R5-M1 fix attribution.

## Final cross-RFC consistency verification (all consistent)

### Slash code space allocation block (final form, verified in all 5 new RFCs and 3 affected accepted RFCs)

```
0x0001-0x0009  : 0855p-b §B (slash reasons: double-sign, liveness-failure, founder-squat, censorship, coord-misbehavior, key-compromise, banning-legit-member, vote-buying, genesis-compromise)
0x000A-0x000B  : 0850p-c §6 (transport-level: PlatformMigration, is_reconnect_lie)
0x000C-0x000D  : RESERVED (NOT slash reasons; sub-DC delegation/governance)
0x000E         : 0850p-d §"Slash Reason Codes Added" (CreateGroupFailed)
0x000F         : 0850p-d §"Slash Reason Codes Added" (CgGroupSpam)
0x0010         : 0850p-d §"Slash Reason Codes Added" (FalseWitness; reused by 0850p-e)
0x0011         : 0850p-e §"Slash Reason Codes Used" (SelfKicked)
0x0012         : 0855p-c §9b "Cross-Platform Slash" (CrossPlatformWitnessCollusion)
0x0013-0x7FFF  : reserved for future slash reasons
0x8000-0xFFFF  : 0855p-c §9b platform-tagged slash indicator (0x8000 | base_reason)
```

Verified consistent across all 5 new RFCs (0850p-d, 0850p-e, 0850p-f, 0855p-d, 0855p-e) and the 3 affected accepted RFCs (0855p-b, 0850p-c, 0855p-c).

### Canonical 10-byte header (verified in all 16 defined envelope structs)

```
envelope_type: [u8; 4]  // b"DOT1" (4-byte ASCII)
envelope_subtype: [u8; 4]  // b"<TAG>" (4-byte ASCII, unique per envelope)
version: u16  // 0x0001
```

All 16 envelope structs in the 5 new RFCs use this format with unique 4-byte ASCII subtype tags. No collisions.

### Kick detection reason code space (verified in 0850p-e)

```
0xF0xx namespace: kick detection layer codes (out of slash reason code space 0x0001-0xFFFF)
0xF001 = StatusTimeout
0xF002 = WitnessObservation
0xF003 = DcObservation
```

Verified: 0850p-e §"Reason Codes for KICK_DETECTED" uses the 0xF0xx namespace correctly (R1-M4 fix migrated from the colliding 0x1001-0x1003 range).

### GroupRegistry.unbound_quarantine (verified consistent)

- Defined in `rfcs/accepted/networking/0850p-c-transport-group-binding.md` §B (v0.1.2)
- `UnboundQuarantineEntry` struct with `unbound_at_epoch`, `recovery_window_epochs`, `original_binding`
- Referenced in `rfcs/draft/networking/0850p-e-kick-detection.md` State Machine table
- Referenced in `missions/open/0850p-e-kick-detection.md` Phase 2

### Mission/RFC consistency (verified)

All 5 mission files use the canonical 4-byte ASCII subtype tags and reference the canonical 10-byte header per RFC-0850p-c §A. No stale 1-byte subtype references remain.

### Version History completeness (verified)

- 0850p-d: v1.0 → v1.1 (R1+R2 fixes recorded)
- 0850p-e: v1.0 → v1.1 (R1 fixes recorded)
- 0850p-f: v0.1 → v0.2 (R1 fixes recorded)
- 0855p-d: v0.1 → v0.2 (R1 fixes recorded)
- 0855p-e: v0.1 → v0.2 (R1+R2 fixes recorded)
- 0850p-c: v0.1.0 → v0.1.1 → v0.1.2 → v0.1.3 (R10-batch, R1, R3 fixes recorded)

## Out-of-scope findings (4 total, flagged for R17+)

The following are pre-existing inconsistencies that predate R16. They are out of scope for R16 but should be addressed in a future review round (R17+):

| ID | Severity | File | Description |
|----|----------|------|-------------|
| R3-OOS-1 | HIGH | `rfcs/accepted/networking/0851p-a-network-bootstrap.md` and `missions/open/0851p-a-bootstrap-slashing.md` | Both files claim `0x000D` = `bootstrap_node_misbehavior`, which conflicts with the canonical R1+R2 reservation of 0x000C-0x000D for non-slash mechanisms. Should be updated to `0x0013` in R17. |
| R3-OOS-2 | HIGH | `missions/open/0855p-b-governance-rfc.md` | Mission claims `0x000E` = `governance_key_compromise` (conflicts with R1+R2 allocation of 0x000E to `CreateGroupFailed` per 0850p-d) and references a hypothetical "New RFC-0855p-d (Governance Lifecycle)" that does not exist (the actual RFC-0855p-d is "Sub-Domain / Sub-Group Nesting"). Should be fixed in R17. |
| R3-OOS-3 | LOW | `rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md` (lines 311, 313) | Minor wording nit. The phrase "slash reason code 0x0009 from this RFC's reserved 0x0009-0xFFFF range" is technically correct (0x0009 is in the 0x0009-0xFFFF range and is allocated as `genesis-compromise`) but mildly confusing because 0x0009 is "allocated", not "reserved". A more precise wording would be "slash reason code 0x0009 (defined in this RFC; 0x0001-0x0008 are the earlier allocations and 0x000A-0xFFFF is the future allocation range)". Pre-R16 issue. |
| R5-OOS-4 | HIGH | `missions/open/0855p-c-cross-domain-slash.md` (lines 17-21, 32, 52, 85, 87) | Mission claims `0x000F` = `domain_coordinator_misbehavior` (with sub-codes `.01`-`.04`). Per the canonical R1+R2 mapping, `0x000F` is allocated to `CgGroupSpam` (RFC-0850p-d §"Slash Reason Codes Added"). Should be updated to `0x0013` or `0x0014` in R17. Pre-R16 issue. |

## Loop closure

**Per the user's loop termination rule, the R16 review series is now CLOSED.**

R6 is the **terminal review round** — it found 0 new in-scope issues. The loop terminates here.

The 6-round R16 series has produced 5 fix commits:

| Round | Commit | Findings | Status |
|-------|--------|----------|--------|
| R1 | `b5fe3d0` | 11 (2 CRITICAL, 4 HIGH, 4 MEDIUM, 1 LOW) | Fixed and committed |
| R2 | `8ad9a02` | 11 (1 CRITICAL, 1 HIGH, 4 MEDIUM, 5 LOW) | Fixed and committed |
| R3 | `cb25196` | 3 in-scope (1 HIGH, 2 MEDIUM) + 3 OOS | Fixed and committed |
| R4 | `a299b5f` | 1 LOW (version history entry) | Fixed and committed |
| R5 | `130640b` | 1 MEDIUM (last stale 1-byte subtype) | Fixed and committed |
| R6 | (this commit) | 0 in-scope issues | **LOOP CLOSED** |

## Final state of the R16 review series

**In-scope items (11 total):** all consistent, all R-round fixes applied, all version histories complete, all slash code references aligned with the canonical allocation block, all envelope structs use the canonical 10-byte header, all 4-byte ASCII subtype tags unique.

**Out-of-scope items (4 total, flagged for R17+):**
- 2 HIGH (0851p-a slash code 0x000D conflict; 0855p-b-governance-rfc mission 0x000E conflict and non-existent RFC reference)
- 1 HIGH (0855p-c-cross-domain-slash mission 0x000F conflict)
- 1 LOW (0855p-b wording nit)

**No further R-rounds are needed within the R16 scope.**

**User action items:**
1. Review the 6 commits (b5fe3d0, 8ad9a02, cb25196, a299b5f, 130640b, plus this commit)
2. Review the 6 review documents in `docs/reviews/r16/` (r1, r2, r3, r4, r5, r6)
3. Decide whether to address the 4 OOS findings in a future R17 review round
4. Push the local commits to `origin/next` when ready