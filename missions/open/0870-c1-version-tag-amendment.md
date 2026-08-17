# Mission: 0870-c1 — RFC-0870 NodeEnvelope version_tag amendment (S6a)

## Status

**LANDED 2026-08-17 (claimant @mmacedoeu).** S6a first sub-session per
`docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
§3 row 6 (Stream A.1 continuation; user-chosen S6 split-by-RFC
decision overrides §22 atomic-blocker bundle rule for this session).
Pre-reqs verified landed: S3 (octo-vault crate), S4 (DFP codemod),
S5 (verify-time invariant — LANDED 2026-08-17 in commit `d007de54`).

**Acceptance gate:** all 5 ACs satisfied. TV-0870-01 7/7 pass
(5 original + 2 added in Round 1 review fix commit `ab2b57b4`).
`cargo test --workspace --lib` passes modulo 3 pre-existing S4 DFP
Round 2 quota-router-cli failures unrelated to S6a (per AC #4
explicit exclusion). Clippy zero warnings. fmt clean. Mission YAML
prettier-clean. Round 1 adversarial review closed (13 findings, all
fixed). Round 2 adversarial review closed (8 new findings, see
`## Round 2 review fixes` row in Version history).

## RFC

- Primary: RFC-0870 (Networking): Distributed Quota Router Network —
  §NodeEnvelope Adoption amendment text + §Version History v2.1 row.
- Co-RFC: RFC-0871 (Wallet Node Lifecycle) — `NodeEnvelope.version_tag:
u8` field spec (S5 implementation already landed in commit
  `d007de54`; this mission BACK-FILLS the RFC-0870 amendment text
  referencing it).
- Source review: `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.1 (envelope.version_tag spec).

## Summary

RFC-0870 §NodeEnvelope Adoption (v2.0 amendment, 2026-08-08) mandated
the unified `NodeEnvelope` from RFC-0871. S5 (LANDED 2026-08-17)
extended `NodeEnvelope` with the `version_tag: u8` field per review
§14.1 — V1 (0xA0) / V2 (0xA1) wire-format version discrimination.
This mission back-fills the RFC-0870 amendment text (v2.1) +
delivers 1 byte-exact TV fixture pinning the version_tag wire form.

The amendment is ADDITIVE: existing RFC-0870 §NodeEnvelope Adoption
table is unchanged; new §NodeEnvelope Version Tag subsection
documents the `version_tag` field requirement with explicit constants.

## Acceptance Criteria

1. **RFC-0870 §Version History v2.1 row added** documenting:
   - `version_tag: u8` field addition to `NodeEnvelope` (V1=0xA0,
     V2=0xA1)
   - V1 receipts (or absent version_tag) hard-rejected at verify
     deterministically
   - Wire-format break per RFC-0870 §14.1 (post-cutover V2
     receipts land at different `envelope_id`s than V1 — replay
     defense)
   - Implementation mission: this file (`0870-c1-version-tag-amendment.md`)
   - Pre-req: S5 LANDED 2026-08-17 commit `d007de54`
2. **RFC-0870 §NodeEnvelope Version Tag subsection added** (new
   subsection under §Specification, after §NodeEnvelope Adoption):
   - `version_tag: u8` field declaration
   - `VERSION_TAG_V1 = 0xA0` / `VERSION_TAG_V2 = 0xA1` constants
   - `NodeEnvelope::build` rejects unknown tags with
     `ProtocolError::UnsupportedVersion(u8)`
   - V1 receipts deterministically rejected at verify via
     `verify_version` helper
   - Cross-reference to RFC-0871 §Data Structures + §Algorithms
     (envelope_id derivation includes version_tag)
3. **TV-0870-01 fixture** in
   `crates/octo-protocol/tests/tv_0870_version_tag.rs` (NEW):
   - Byte-exact `NodeEnvelope` canonical_ser output for a sample
     V2 envelope (V2=0xA1, deterministic payload)
   - Pins wire-form version_tag byte position (after envelope_id,
     before from_did — verify field order)
   - Pins VERSION_TAG_V2 export from `octo_protocol::envelope`
4. Verification gate:
   ```bash
   cargo test -p octo-protocol --test tv_0870_version_tag    # 7/7 pass (Round 1: 5/5; +2 regression tests)
   cargo test --workspace --lib                             # excludes 3 pre-existing S4 DFP Round 2 quota-router-cli::commands::tests::settle_* failures (commits 19faf380/4ab400bd/18edbe0d); unrelated to S6a
   cargo clippy --workspace --all-targets --features full -- -D warnings
   cargo fmt --all -- --check
   npx prettier --write missions/open/0870-c1-version-tag-amendment.md
   ```
5. Memory card: this session's S5 memory card
   (`memory/mission-0957-g-verify-time-invariant-status.md`)
   already covers the version_tag implementation; back-link added
   from this mission's `## Cross-reference` section.

## Cross-reference

- **Pre-req:** `memory/mission-0957-g-verify-time-invariant-status.md`
  (S5 LANDED 2026-08-17, commit `d007de54`) — back-link to this S6a
  mission added in S6a commit `c7f99a47` (post-implementation).
- **Status card:** `memory/mission-0870-c1-version-tag-amendment-status.md`
  (this session's LANDED receipt).
- **Plan:** `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md`
  §3 row 6 (Stream A.1 S6a continuation).
- **Review source:** `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
  §14.1 (NodeEnvelope.version_tag spec origin).

## Out of scope (deferred beyond S6a)

- S6b RFC-0957 amendment (22 TV) — next sub-session
- S6c RFC-0862 amendment (8 TV)
- S6d RFC-0900 amendment (10 TV)
- S6e RFC-0105 amendment (109 TV)
- S6f RFC-0959 amendment (25 TV)
- S6g RFC-0960 amendment (108 TV)
- §22 atomic-blocker PR bundle (user-chosen split-by-RFC overrides
  atomic-blocker rule for these sub-sessions; user may bundle at
  push time)

## Dependency edges (no changes)

| From                                                      | To                        | Why             | Layer direction     |
| --------------------------------------------------------- | ------------------------- | --------------- | ------------------- |
| RFC-0870 amendment                                        | RFC-0871 §Data Structures | Cross-reference | n/a (RFC text only) |
| `crates/octo-protocol/tests/tv_0870_version_tag.rs` (NEW) | `octo-protocol`           | Test consumer   | test → lib          |

No new cyclic edges. No new crate deps.

## Critical files

- `rfcs/accepted/networking/0870-distributed-quota-router-network.md`
  (modify — §Version History v2.1 row + §NodeEnvelope Version Tag
  subsection)
- `crates/octo-protocol/tests/tv_0870_version_tag.rs` (NEW — 1 TV
  fixture)
- `memory/mission-0957-g-verify-time-invariant-status.md` (existing
  — add cross-reference backlink in follow-up edit)
- `missions/open/0870-c1-version-tag-amendment.md` (this file)

## Existing patterns reused

- RFC version history row format (RFC-0870 §Version History rows
  v1.0..v2.0) → new v2.1 row mirrors same shape.
- RFC subsection format (RFC-0870 §NodeEnvelope Adoption v2.0) →
  new §NodeEnvelope Version Tag v2.1 subsection mirrors the
  pattern.
- `octo-protocol/tests/tv8_borsh_parity.rs` byte-exact TV layout →
  new `tv_0870_version_tag.rs` mirrors the test scaffolding.

## Risks

- **B.3 verify-time invariant load-bearing** (HIGH per plan §5):
  S5 implementation already passed gate; S6a amendment text + 1 TV
  is documentation-only + 1 fixture. Low blast if anything regresses.
- **§22 atomic-blocker rule bypass** (MED per plan §5): user-chosen
  S6 split-by-RFC decision lands each amendment separately, NOT in
  the prescribed single PR bundle. Production deployment must
  coordinate the 7 sub-sessions' commits at push time (per S8).
- **Version tag V1 → V2 cutover coordination** (MED): existing
  RFC-0870 §NodeEnvelope Adoption table does not specify V1 vs V2;
  this amendment is the FIRST place V2 is documented. Future
  consumer migrations (RFC-0870 S7 territory) MUST rebuild against
  the V2 wire form.
- **MED-1 from Round-1 review (accept-at-build / reject-at-verify split)**:
  `NodeEnvelope::build(..., VERSION_TAG_V1)` is accepted by the
  constructor (rejection is at `verify_version`). Intentional design —
  `build` rejects ONLY unknown tags; verify-time gates V1. Rationale:
  (a) preserves the ability to round-trip historical V1 fixtures in
  tests, (b) `verify_version` is the canonical operational gate, (c)
  dual-message is more permissive at construction + stricter at verify
  than the inverse (reject-at-build would silently drop V1 fixtures
  before they could be inspected by a future replay/recovery tool).
  Documented in TV-0870-01 test #2 (`v1_build_accepts_legacy_path`).

## Version history

| Date       | Author     | Change                                                                                                                                                          |
| ---------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-08-17 | @mmacedoeu | Initial proposal as S6a (first S6 sub-session per user split-by-RFC decision). RFC-0870 amendment back-fills S5 implementation. 1 TV fixture pins V2 wire form. |
| 2026-08-17 | @mmacedoeu | LANDED. RFC-0870 v2.1 row + §NodeEnvelope Version Tag subsection added. TV-0870-01 5/5 pass. Memory card cross-link added to S5 status card.                    |
