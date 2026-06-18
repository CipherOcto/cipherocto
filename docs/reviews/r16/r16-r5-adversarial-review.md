# R16 R5 — Adversarial Review (R4 follow-up)

**Date:** 2026-06-17
**Reviewer:** jcode agent (auto)
**Scope:** all 11 RFCs and missions modified in R16 R1+R2+R3+R4. Verify R4 fix landed correctly; perform one more comprehensive scan for any stale references in the 11 in-scope items.
**Method:** comprehensive scan for any remaining stale 1-byte subtype references, 0x000C-0xFFFF stale references, and obsolete RFC references in the 11 in-scope items.

## Findings (in scope)

| ID | Severity | File | Description |
|----|----------|------|-------------|
| R5-M1 | MEDIUM | `missions/open/0850p-f-group-decommission.md` | Phase 1 acceptance criteria (lines 30-31) still used the OLD 1-byte subtype values `0x15` and `0x16` for `UnbindAllEnvelope` and `UnbindAllAckEnvelope` (R1 fix migrated them to `b"UALL"` and `b"UAAC"` per the canonical 4-byte ASCII tag format). Other phases (Phase 5 line 52, Location line 75) were already updated. The Phase 1 entries were missed by R2. Fixed in this commit. |

## Out-of-scope findings (flagged for future review)

The following are pre-existing inconsistencies that predate R16 but were discovered during the R5 audit. They are out of scope for R16 R5 (which is limited to the 11 RFCs and missions created/modified in R16) but should be addressed in a future review round (R17+):

| ID | Severity | File | Description |
|----|----------|------|-------------|
| R3-OOS-1 (repeated) | HIGH | `rfcs/accepted/networking/0851p-a-network-bootstrap.md` and `missions/open/0851p-a-bootstrap-slashing.md` | Both files claim `0x000D` = `bootstrap_node_misbehavior`, which conflicts with the canonical R1+R2 reservation of 0x000C-0x000D for non-slash mechanisms. Should be updated to `0x0013` in R17. |
| R3-OOS-2 (repeated) | HIGH | `missions/open/0855p-b-governance-rfc.md` | Mission claims `0x000E` = `governance_key_compromise` (conflicts with R1+R2 allocation of 0x000E to `CreateGroupFailed` per 0850p-d) and references a hypothetical "New RFC-0855p-d (Governance Lifecycle)" that does not exist (the actual RFC-0855p-d is "Sub-Domain / Sub-Group Nesting"). Should be fixed in R17. |
| R3-OOS-3 (repeated) | LOW | `rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md` (lines 311, 313) | Minor wording nit. Pre-R16 issue. |
| R5-OOS-4 (new) | HIGH | `missions/open/0855p-c-cross-domain-slash.md` (lines 17-21, 32, 52, 85, 87) | Mission claims `0x000F` = `domain_coordinator_misbehavior` (with sub-codes `.01`-`.04`). Per the canonical R1+R2 mapping, `0x000F` is allocated to `CgGroupSpam` (RFC-0850p-d §"Slash Reason Codes Added"). The 0855p-c mission should be updated to use a different slash code (e.g., `0x0013` or `0x0014`, both free in the 0x0013-0xFFFF range) in R17. Pre-R16 issue (R12/R13). |

## Findings investigated and rejected

- **R5-N1 (false positive):** 0855p-d line 128, 0855p-e lines 148, 180, 191, 201, 0850p-d mission line 32, 0850p-e mission line 30, 0855p-e mission lines 31, 36, 0850p-f mission line 75 reference "0855p-b.1", "extend CreateGroupEnvelope with SubGroupExtension", or "subtype 0x10/0x15/0x16/0x18/0x20/0x30" in comments/version history entries. **Rejected:** all these references are in **version history entries** or **comments explaining the fix** (e.g., "was subtype 0x10 in v1.0; the canonical format is the 4-byte ASCII tag per RFC-0850p-c §A"). They document the fix that was applied, not stale references. This is consistent with the canonical mapping.

- **R5-N2 (false positive):** 0850p-c §6 "Unbind Reasons" table includes both slash codes (0x0001-0x0009 per 0855p-b) and transport-level codes (0x000A-0x000B per 0850p-c) and 0850p-family codes (0x000E-0x0011) and 0855p-c code (0x0012) in one table. **Rejected:** the table is explicitly described as "the unbind reason codes 0x0001-0x000B are a SUPERSET of the slash reason codes from RFC-0855p-b §B" and the column "Slash reason | Unbind reason | Authority | Cooldown" is intentionally shared (line 468 narrative). The codes are all correctly assigned.

## Subtype tag allocation (verified consistent)

All 18 distinct subtype tags allocated across the 5 RFCs are unique (no collisions):

| Subtype tag | RFC | Envelope struct | Status |
|-------------|-----|-----------------|--------|
| `b"CGRO"` | 0850p-d | `CreateGroupEnvelope` | Defined |
| `b"CGAC"` | 0850p-d | `CreateGroupAckEnvelope` | Defined (R2-H1) |
| `b"CGDA"` | 0850p-d | `CreateGroupDoneEnvelope` | Defined |
| `b"CGFA"` | 0850p-d | `CreateGroupFailEnvelope` | Defined |
| `b"INVT"` | 0850p-d | `InviteEnvelope` | Defined |
| `b"UALL"` | 0850p-d | `UnbindAllEnvelope` | Defined |
| `b"UAAC"` | 0850p-d | `UnbindAllAckEnvelope` | Defined |
| `b"UADN"` | 0850p-f | (allocated; struct TBD) | Allocated (early stub) |
| `b"UAAU"` | 0850p-f | (allocated; struct TBD) | Allocated (early stub) |
| `b"SFCK"` | 0850p-e | `SelfKickedEnvelope` | Defined |
| `b"KFDT"` | 0850p-e | `KickDetectedEnvelope` | Defined |
| `b"MREM"` | 0850p-e | `MemberRemovedEnvelope` | Defined |
| `b"RJRQ"` | 0850p-e | `RejoinRequestEnvelope` | Defined |
| `b"RJGT"` | 0850p-e | `RejoinGrantEnvelope` | Defined |
| `b"CGSB"` | 0855p-d | `CreateSubGroupEnvelope` | Defined |
| `b"HORQ"` | 0855p-e | `HandoverRequestEnvelope` | Defined |
| `b"HOAK"` | 0855p-e | `HandoverAckEnvelope` | Defined (R2-L2) |
| `b"HODN"` | 0855p-e | `HandoverDoneEnvelope` | Defined (R2-L2) |

16 envelope structs are fully defined; 2 subtype tags (`b"UADN"`, `b"UAAU"`) are reserved for 0850p-f's early stub.

## Recommendation

**R5 found 1 in-scope issue (R5-M1) — fixed in this commit.** Per the user's loop termination rule ("loop should end when a new review find no issues"), the R16 review series is **NOT yet closed** — the loop continues to R6.

The R5-M1 fix completes the 11 in-scope items' canonical subtype tag migration. All other aspects (slash code block, canonical 10-byte header, struct definitions, cross-RFC consistency) were already verified consistent in R1+R2+R3+R4.

**Out-of-scope findings (4 total flagged for R17+):**
- R3-OOS-1: 0851p-a slash code 0x000D conflict
- R3-OOS-2: 0855p-b-governance-rfc mission stale
- R3-OOS-3: 0855p-b wording nit
- R5-OOS-4 (NEW): 0855p-c-cross-domain-slash mission claims 0x000F for domain_coordinator_misbehavior, conflicts with R1+R2 allocation of 0x000F to CgGroupSpam (pre-R16 issue)

**Loop status:** continue to R6 to verify R5-M1 fix and find 0 issues.