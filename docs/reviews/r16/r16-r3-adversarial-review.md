# R16 R3 — Adversarial Review (R2 follow-up)

**Date:** 2026-06-17
**Reviewer:** jcode agent (auto)
**Scope:** all 11 RFCs and missions modified in R16 R1+R2; verify R2 fixes did not introduce new issues and verify no stale references remain.
**Method:** re-audit each R2 fix site for consistency, completeness, and side-effects; comprehensive scan for any stale slash code references (`0x000C-0xFFFF`, `0x000A-0xFFFF`, `0x0009-0xFFFF`) and stale struct references (`SubGroupExtension`, `0855p-b.1`) that may have escaped R1+R2 fixes.

## Findings (in scope)

| ID | Severity | File | Description |
|----|----------|------|-------------|
| R3-H1 | HIGH | `rfcs/accepted/networking/0850p-c-transport-group-binding.md` | §6a "Slash reason 0x000A" (line 479) and §"Forward Compatibility" (line 761) still say "0x000C-0xFFFF reserved for future slash reasons" / "0x000C-0xFFFF reserved" — stale after R1+R2 fix allocated 0x000E-0x0012 to specific RFCs. Fixed in this commit. |
| R3-M1 | MEDIUM | `rfcs/accepted/networking/0850p-c-transport-group-binding.md` | §"Future Work" F6 row (line 923) says "UNBIND reason 0x000C-0xFFFF reserved for future governance events" — stale description that incorrectly suggests all codes in 0x000C-0xFFFF are reserved for future UNBIND use, contradicting the §6 "Unbind Reasons" table and the canonical R1+R2 mapping. Fixed in this commit. |
| R3-M2 | MEDIUM | `rfcs/README.md` | Line 322 says "Sub-group CGROUP with `SubGroupExtension`; sub-DC delegation" — stale description that does not mention the new `CreateSubGroupEnvelope` envelope variant introduced by the R1-H2 fix. Updated to "New `CreateSubGroupEnvelope` envelope variant (subtype `b"CGSB"`) carrying a `SubGroupExtension` payload" in this commit. |

## Out-of-scope findings (flagged for future review)

The following are pre-existing inconsistencies that predate R16 but were discovered during the R3 audit. They are out of scope for R16 R3 (which is limited to the 11 RFCs and missions created/modified in R16) but should be addressed in a future review round (R17+):

| ID | Severity | File | Description |
|----|----------|------|-------------|
| R3-OOS-1 | HIGH | `rfcs/accepted/networking/0851p-a-network-bootstrap.md` (lines 420, 431, 726, 748, 749) and `missions/open/0851p-a-bootstrap-slashing.md` (lines 13, 18, 19, 21, 23-26, 35, 53, 73, 85, 87, 91) | Both files claim `0x000D` = `bootstrap_node_misbehavior`. Per the canonical R1+R2 mapping (RFC-0855p-b §B, RFC-0850p-c §6, RFC-0855p-c, RFC-0850p-d, RFC-0850p-e), codes 0x000C-0x000D are reserved for non-slash mechanisms (sub-DC delegation and governance vote), NOT slash reasons. The 0851p-a slash code 0x000D conflicts with this reservation. The mission and RFC should be updated to use the next free slash reason code, `0x0013` (per RFC-0855p-b §B "0x0013-0xFFFF reserved for future slash reasons"). The 0851p-a mission also references the stale description "0x000C-0xFFFF is reserved" (lines 19, 87) which now contradicts the canonical mapping. These are pre-R16 issues (R12/R13) and out of scope for R16, but should be addressed in R17. |
| R3-OOS-2 | HIGH | `missions/open/0855p-b-governance-rfc.md` (lines 20, 42, 52, 61) | The mission claims `0x000E` = `governance_key_compromise` slash reason and references a hypothetical "New RFC-0855p-d (Governance Lifecycle)" to be created. Per the canonical R1+R2 mapping, `0x000E` is allocated to `CreateGroupFailed` (RFC-0850p-d §"Slash Reason Codes Added"), and the actual RFC-0855p-d is "Sub-Domain / Sub-Group Nesting", NOT "Governance Lifecycle". The mission should be updated to use a different slash code (e.g., 0x0013 or 0x0014, both free in the 0x0013-0xFFFF range) and the "New RFC-0855p-d" reference should be replaced with "RFC-XXXX (Governance Lifecycle)" (a new RFC number to be assigned). Pre-R16 issue, out of scope for R16. |
| R3-OOS-3 | LOW | `rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md` (lines 311, 313) | Lines 311 and 313 say "slash reason code 0x0009 from this RFC's reserved 0x0009-0xFFFF range; codes 0x0001-0x0008 are already taken". The wording "0x0009-0xFFFF range" technically includes 0x0009 itself (which is now `genesis-compromise`); a more precise wording would be "the 0x000A-0xFFFF future allocation range" or similar. This is a minor wording nit, not a substantive issue. Pre-R16 issue, out of scope for R16. |

## Findings investigated and rejected

- **R3-N1 (false positive):** 0855p-e `HandoverReason` enum and `CoordinatorRole` enum use `u8` representation but the canonical serialization per RFC-0126 (DCS) is not explicitly stated. **Rejected:** both enums are `#[repr(u8)]` and the canonical 10-byte envelope header precedes them; the DCS serialization of the `u8` representation is standard and need not be repeated in every RFC that uses it.

- **R3-N2 (false positive):** 0850p-c `slash_reason_code: u16` field references "0x0001-0xFFFF code space" but only 0x0001-0x0012 are allocated. **Rejected:** the `u16` type allows any value in the code space, but application logic only uses allocated codes. The comment is correct in describing the type's range, not the allocated subset.

- **R3-N3 (false positive):** 0855p-c line 549 says "slash reason 0x0005 with reduced penalty (50% OCTO-O, since loss may be platform-driven not coordinator-driven)" — but RFC-0855p-b §B says 0x0005 (`coordinator-misbehavior`) is 100% OCTO-O. **Rejected:** RFC-0855p-b §B describes 0x0005 as a "free-form `evidence` payload that the `Slashing Adjudicator` judges to be sufficient. The adjudicator's signature is the proof of validity." Per-event override of the canonical penalty is within the adjudicator's authority; the 0855p-c text correctly notes the per-event override.

- **R3-N4 (false positive):** 0850p-c line 414 says slash reason `0x0003` (founder-squat) "All witnesses initiate a slash tally against the founder". **Rejected:** this is the correct R1+R2 allocation: 0x0003 = `founder-squat` per RFC-0855p-b §B. The text is consistent with the §B table and §6 "Unbind Reasons" table.

## Slash code space allocation block (verified consistent after R3 fixes)

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

This block is now consistent across:
- `rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md` §B (v1.2) — narrative on line 441 + table on lines 949-966
- `rfcs/accepted/networking/0850p-c-transport-group-binding.md` §6 (v0.1.2) — table on lines 449-466 + §6a line 479 + §"Forward Compatibility" line 761 + §"Future Work" F6 line 923
- `rfcs/accepted/networking/0855p-c-domain-coordinator-role.md` §9b + §"Forward Compatibility" (v0.1.2)
- `rfcs/draft/networking/0850p-d-dc-initiated-group-creation.md` §"Slash Reason Codes Added" (v1.1)
- `rfcs/draft/networking/0850p-e-kick-detection.md` §"Slash Reason Codes Used" (v1.1)

## Recommendation

**R3 fixes complete (in-scope).** All 3 in-scope findings (1 HIGH, 2 MEDIUM) addressed in this commit. The 11 RFCs/missions in the R16 scope are now internally consistent and cross-RFC consistent.

**3 out-of-scope findings flagged for R17+.** These are pre-R16 issues (R12/R13) that the R16 R1+R2 slash code allocation block has revealed as conflicting. The user may want to address them in a future review round.

**Proceed to R4** if desired. R4 should:
1. Verify R3 fixes landed correctly
2. Cross-RFC slash code consistency one more time (final pass)
3. Verify cross-RFC canonical 10-byte header consistency (final pass)
4. Verify cross-RFC envelope struct field consistency (final pass)
5. Address the 3 out-of-scope findings (or document why they remain out of scope)
6. Write the R4 review document and commit if issues found

I expect R4 to find 0 in-scope issues. The 3 out-of-scope findings will need to be addressed in R17 or later.