# R16 R4 — Final Verification Review (R3 follow-up)

**Date:** 2026-06-17
**Reviewer:** jcode agent (auto)
**Scope:** final verification pass on all 11 RFCs and missions modified in R16 R1+R2+R3. Verify R3 fixes landed correctly; perform one more cross-RFC consistency check; verify all envelope structs use the canonical 10-byte header; verify all referenced structs are defined.
**Method:** comprehensive structural scan; cross-RFC slash code consistency final pass; canonical 10-byte header verification across all 16 defined envelope structs; struct reference verification.

## Findings (in scope)

| ID | Severity | File | Description |
|----|----------|------|-------------|
| R4-L1 | LOW | `rfcs/accepted/networking/0850p-c-transport-group-binding.md` | Version History v0.1.2 entry did not mention the R3 fix. Added v0.1.3 entry documenting the R3-H1 and R3-M1 fixes (the R3-M2 fix was in `rfcs/README.md`, not in this RFC). |

## Findings investigated and rejected

- **R4-N1 (false positive):** 0850p-f allocates subtype tags `b"UADN"` and `b"UAAU"` but the struct definitions (`UnbindAllDoneEnvelope`, `UnbindAllAuditEnvelope`) are not yet provided. **Rejected:** 0850p-f is an early-stage stub (v0.1 → v0.2) and explicitly documents in line 72 that the struct definitions are "to be added in the next iteration of this RFC". The subtype tags are reserved to prevent future conflicts; this is a deliberate stub-state, not an issue.

- **R4-N2 (false positive):** The 0850p-c §A "Canonical Envelope Serialization" appendix describes the canonical header as "Header: `envelope_type (4 bytes) || envelope_subtype (4 bytes) || version (2 bytes, big-endian)`" but does not explicitly say the type is ASCII. **Rejected:** the same line says `envelope_type = b"DOT1"` (which is a 4-byte ASCII literal in Rust syntax) and the 5 new RFCs (0850p-d, 0850p-e, 0850p-f, 0855p-d, 0855p-e) all use `// b"DOT1"` and `// b"<TAG>"` comments confirming the ASCII type. The 0850p-c §A is implicitly ASCII via the `b"DOT1"` notation.

- **R4-N3 (false positive):** 0855p-e `SlashEvent.slash_reason_code` field has comment "per RFC-0855p-b §B code space 0x0001-0xFFFF" but only 0x0001-0x0012 are allocated. **Rejected:** the comment correctly describes the type's range (u16, allowing 0x0001-0xFFFF), not the allocated subset. Application logic only uses allocated codes.

- **R4-N4 (false positive):** 0850p-e's `PlatformKickEvent` enum coexists with 0855p-c's `PlatformEvent::KickedFromGroup`. **Rejected:** they serve different purposes (verified in R1-N1): `PlatformEvent::KickedFromGroup` is the adapter-internal event (data-carrying, RFC-0855p-c §3); `PlatformKickEvent` is the kick-detection-layer's higher-level classification (tag-only, RFC-0850p-e). The "Per-Adapter Wiring" subsections in 0850p-e document the mapping from one to the other.

## Cross-RFC consistency final pass (verified consistent)

### Slash code space allocation block (final form)

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

Verified consistent across all 5 RFCs and the 3 accepted RFCs (0855p-b, 0850p-c, 0855p-c) after R3-H1 and R3-M1 fixes.

### Canonical 10-byte header (verified consistent)

All 16 defined envelope structs use the canonical 10-byte header per RFC-0850p-c §A:

| RFC | Envelope structs | Subtype tags |
|-----|------------------|--------------|
| 0850p-d | CreateGroupEnvelope, CreateGroupAckEnvelope, CreateGroupDoneEnvelope, CreateGroupFailEnvelope, InviteEnvelope, UnbindAllEnvelope, UnbindAllAckEnvelope | `b"CGRO"`, `b"CGAC"`, `b"CGDA"`, `b"CGFA"`, `b"INVT"`, `b"UALL"`, `b"UAAC"` |
| 0850p-e | SelfKickedEnvelope, KickDetectedEnvelope, MemberRemovedEnvelope, RejoinRequestEnvelope, RejoinGrantEnvelope | `b"SFCK"`, `b"KFDT"`, `b"MREM"`, `b"RJRQ"`, `b"RJGT"` |
| 0850p-f | (none — early stub; 4 subtype tags reserved: `b"UALL"`, `b"UAAC"` shared with 0850p-d; `b"UADN"`, `b"UAAU"` new in 0850p-f) | — |
| 0855p-d | CreateSubGroupEnvelope | `b"CGSB"` |
| 0855p-e | HandoverRequestEnvelope, HandoverAckEnvelope, HandoverDoneEnvelope | `b"HORQ"`, `b"HOAK"`, `b"HODN"` |

All 16 structs include:
- `pub envelope_type: [u8; 4]` with `// b"DOT1"`
- `pub envelope_subtype: [u8; 4]` with `// b"<TAG>"`
- `pub version: u16` with `// 0x0001`

### Struct definitions (verified complete)

All 16 envelope structs are defined with their full body fields. All non-envelope structs and enums referenced in the 5 new RFCs are defined:

- 0850p-d: `ProposedGroupMetadata`, `GroupVisibility`, `UnbindReason`, `WitnessAssertion` (all defined)
- 0850p-e: `PlatformKickEvent` (defined); `Platform` (defined in 0850p-c); `PlatformEvent::KickedFromGroup` (defined in 0855p-c §3)
- 0855p-d: `SubGroupExtension` (defined)
- 0855p-e: `HandoverReason`, `CoordinatorRole`, `SlashTally`, `SlashEvent` (all defined in 0855p-e itself; R1-H5 inlined `SlashTally`/`SlashEvent` from non-existent RFC-0855p-b.1; R1-L3 inlined `CoordinatorRole`)

### GroupRegistry.unbound_quarantine (verified consistent)

- Defined in `rfcs/accepted/networking/0850p-c-transport-group-binding.md` §B "GroupRegistry Local State" (v0.1.2)
- `UnboundQuarantineEntry` struct defined with `unbound_at_epoch: u64`, `recovery_window_epochs: u64`, `original_binding: GroupBinding`
- Referenced in `rfcs/draft/networking/0850p-e-kick-detection.md` State Machine table (lines 203+) — uses `recovery_window_epochs = REJOIN_GRANT_TIMEOUT = 50`
- Referenced in `missions/open/0850p-e-kick-detection.md` Phase 2 — uses `BTreeMap<(MissionId, DomainId, Platform), UnboundQuarantineEntry>` with explicit move semantics

### Mission files cross-reference (verified consistent)

All 5 mission files (`missions/open/0850p-d-*.md`, `0850p-e-*.md`, `0850p-f-*.md`, `0855p-d-*.md`, `0855p-e-*.md`) reference the canonical 4-byte ASCII subtype tags and the canonical 10-byte header format. No stale 1-byte subtype references remain.

## Recommendation

**R16 multi-round review complete.** All 4 rounds (R1, R2, R3, R4) completed with all in-scope findings addressed. The 11 RFCs and missions are now:

1. **Internally consistent** — no contradictions within any single RFC or mission
2. **Cross-RFC consistent** — slash codes, canonical header, envelope structs, and struct references all match across all 5 new RFCs and the 3 affected accepted RFCs
3. **Mission/RFC consistent** — mission files reference the same struct definitions, subtype tags, and field types as their parent RFCs
4. **Version History complete** — all 5 new RFCs have R1 and R2 fix entries; 0850p-c has R1, R2 (via v0.1.2), and R3 (via v0.1.3) fix entries
5. **Slash code allocation block consistent** — the canonical mapping 0x0001-0x0012 (with 0x000C-0x000D reserved for non-slash) is now reflected in all relevant tables, narratives, and forward-compatibility notes

**Round-by-round summary:**

| Round | Findings | Fixed in commit | Review document |
|-------|----------|-----------------|-----------------|
| R1 | 11 (2 CRITICAL, 4 HIGH, 4 MEDIUM, 1 LOW) | `b5fe3d0` "R16 R1: migrate to canonical 10-byte header, coordinate slash code block" | `r16-r1-adversarial-review.md` |
| R2 | 11 (1 CRITICAL, 1 HIGH, 4 MEDIUM, 5 LOW) | `8ad9a02` "R16 R2: fix contradictions and stale references from R1 fix" | `r16-r2-adversarial-review.md` |
| R3 | 3 in-scope (1 HIGH, 2 MEDIUM) + 3 OOS | `cb25196` "R16 R3: clean up stale 0x000C-0xFFFF references and update 0855p-d summary" | `r16-r3-adversarial-review.md` |
| R4 | 1 LOW (version history entry) | (this commit) | `r16-r4-adversarial-review.md` |

**Out-of-scope findings (flagged for R17+):**

- R3-OOS-1 (HIGH): 0851p-a-network-bootstrap.md RFC + 0851p-a-bootstrap-slashing.md mission claim `0x000D` for `bootstrap_node_misbehavior`, which conflicts with the canonical R1+R2 reservation of 0x000C-0x000D for non-slash mechanisms. Should be updated to `0x0013` in R17.
- R3-OOS-2 (HIGH): 0855p-b-governance-rfc.md mission claims `0x000E` for `governance_key_compromise` (conflicts with R1+R2 allocation of 0x000E to `CreateGroupFailed`) and references a hypothetical "New RFC-0855p-d (Governance Lifecycle)" that does not exist (the actual RFC-0855p-d is "Sub-Domain / Sub-Group Nesting"). Should be fixed in R17.
- R3-OOS-3 (LOW): Minor wording nit in 0855p-b lines 311/313 ("0x0009 from this RFC's reserved 0x0009-0xFFFF range" is technically correct but mildly confusing). Pre-R16 issue.

**R16 review series is now closed.** No further R-rounds are needed within the R16 scope. The 3 out-of-scope findings should be addressed in a future review (R17+).

**User action items:**
1. Review the 4 R-round commit subjects and bodies (b5fe3d0, 8ad9a02, cb25196, plus this commit)
2. Review the 4 review documents in `docs/reviews/r16/` (r1, r2, r3, r4)
3. Decide whether to address the 3 OOS findings in a future review round (R17)
4. Push the local commits to `origin/next` when ready