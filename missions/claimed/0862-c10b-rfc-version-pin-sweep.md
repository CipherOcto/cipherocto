# Mission: 0862-c10b — RFC version-pin prose sweep (expanded scope)

## Status

**LANDED 2026-08-19 (@mmacedoeu).** Per Round-3 adversarial review
finding #6: cross-RFC prose contains `RFC-NNNN vN.M` pins that
violate CLAUDE.md §RFC Reference Conventions Reaffirmed rule "use
only the number". 132 prose sites across `rfcs/accepted/`. Plan
documented in `plans/sparkling-mapping-kahan.md` (Round-3 fix plan).
Original plan scope was 4 RFCs / 17 sites (LANDED as `0862-c10`
2026-08-19, commit `feaaa6b0`); this mission expands to **full
workspace sweep** — 116 substitutions across 24 RFC files. Mission
file renamed `0862-c10b` to avoid collision with the already-LANDED
c10 close-out.

## What landed

## RFC

- **Universal rule:** CLAUDE.md §RFC Reference Conventions Reaffirmed:
  "use only the number. Never include status, version pins, or
  metadata. Example: `RFC-0909` not `RFC-0903 (Accepted v63)`.
  **Why:** Status/version in references causes sync bugs and
  verbose noise. Only the RFC's own Status header and version
  history table carry version info."

## Scope

EXEMPT (must keep version pins):

1. **`## Version History` table rows** in each RFC (these ARE the
   canonical version pin locations per CLAUDE.md rule)
2. **`## Status` block `**Version:**` field** (canonical pin
   per-file)
3. **Self-references inside an RFC's own Version History** (the
   row that says "v2.0 → v2.1" naturally references its own
   version)

## Acceptance Criteria

- [x] 116 prose `RFC-NNNN vN.M` pins REMOVED across 24 RFC files in
      `rfcs/accepted/`, leaving only Version History + Status block
      + self-refs
- [x] Each removal preserves the surrounding prose (no other edits)
- [x] No semantic drift — references still identify the target RFC
      unambiguously via the number alone
- [x] `grep -rEn 'RFC-0[0-9]+\s+v[0-9]+\.[0-9]+' rfcs/accepted` returns
      only exempt matches (Version History + Status block + self-refs)

## Cross-reference

- **Audit source:** Round-3 inline adversarial review (8-commit
  scope on `next`: 0155dbb3, 11e9efce, 050c32eb, df55abaa, 2a497f58,
  5b698b72, 0b283d29, 58c4c2ce)
- **Plan:** `plans/sparkling-mapping-kahan.md` (Round-3 fix plan)
- **Sibling:** `0105-v2-role-token-canonicalization`,
  `0900-d2-provider-stake-serde-default` (sibling Round-3 missions)
- **Pattern:** per CLAUDE.md §RFC Reference Conventions Reaffirmed

## Risks

- **Scope creep** (LOW): the replacement is mechanical. Mitigation:
  regex-based per-file sed, no judgement calls.
- **Self-reference ambiguity** (LOW): some "RFC-XXXX v2.1" mentions
  may legitimately refer to the RFC's own version history. Mitigation:
  grep first to identify self-refs, exclude before sed.
- **Cross-RFC semantic drift** (LOW): RFC-0959 has many version pins
  that pin to specific subsections in companion RFCs. Mitigation:
  the version pin doesn't change the reference target — it just adds
  noise. The reader still finds the target RFC.

## Version history

| Date       | Author     | Change |
| ---------- | ---------- | ------ |
| 2026-08-19 | @mmacedoeu | Initial filing per Round-3 review defect #6. Plan scope 4 RFCs / 17 sites undersized the defect; full sweep finds 132 sites. |
| 2026-08-19 | @mmacedoeu | LANDED — 116 substitutions across 24 RFC files. Version History tables + Status blocks + Version History row pins all preserved. Mission file renamed `0862-c10b` to avoid collision with already-LANDED `0862-c10` close-out (`feaaa6b0`). |
