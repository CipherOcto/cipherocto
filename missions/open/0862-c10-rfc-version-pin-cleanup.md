# Mission: 0862-c10 — RFC cross-reference version-pin cleanup (4 RFC files, 17 sites)

## Status

**LANDED 2026-08-19 (@mmacedoeu).** 4 RFC files modified; 17 prose
sites cleaned per AC-1..AC-4; AC-5..AC-7 exempts honored (self-refs
in own file + Version History tables).

## What landed (2026-08-19)

- **0112-deterministic-vectors.md** — 5 sites: `RFC-0111 v1.20` → `RFC-0111` (lines 80, 275, 278, 286, 450).
- **0127-dcs-blob-amendment.md** — 6 sites: `RFC-0126 v2.5.1` → `RFC-0126` (lines 47, 125, 150, 240, 535, 536). Version History table v2.6.0 refs at lines 744/749 EXEMPT per AC-6.
- **0113-deterministic-matrices.md** — 2 sites: `RFC-0112 v1.12` + `RFC-0111 v1.20` (lines 224, 1042).
- **0010-canonical-did-codec.md** — 5 sites: `RFC-0862 v1.3` (lines 406, 669, 800, 838, 840). Self-refs `RFC-0010 v1.4/v1.5` (lines 419, 476, 589, 679, 697, 730) + Version History rows (1015, 1016) EXEMPT per AC-5/AC-6.

## Verify (2026-08-19)

- `git diff --stat` → 4 files, 18 insertions / 18 deletions (symmetric — pure text removal).
- `grep -rEn 'RFC-0[0-9]+\s+v[0-9]+\.[0-9]+' rfcs/accepted | grep -v 'Version History'` on the 4 plan files → zero cross-RFC prose pins remain (only exempt self-refs + Version History).
- No Rust changes; no clippy/fmt impact.

## Out-of-plan follow-ons (filed for awareness, NOT this mission)

Round-3 review also surfaced `RFC-0959 v1.0` at `rfcs/accepted/proof-systems/0958-zk-capability-subclass.md:52` and likely more across the RFC corpus. File follow-on sweep mission if desired. Filed per Round-3 adversarial review
finding (defect 6): `RFC-NNNN vN.M` version-pin pattern in cross-RFC
prose violates `CLAUDE.md §RFC Reference Conventions Reaffirmed` ("use
only the number, never include status, version pins, or metadata").
17 sites across 4 RFC files (0112, 0113, 0127, 0010). Self-references
inside an RFC's own Version History are EXEMPT per the rule; RFC-0009
self-refs in `0009-identity-evolution-v12.md` are likewise exempt (the
v1.2 suffix IS the file identity).

## What will land

- **4 RFC files modified**: remove `vN.M` suffix from `RFC-NNNN` in prose.
- **17 sites total** (see Dependency edges for per-file line list).
- **EXEMPT**: `## Version History` tables (where version pins belong).
- **EXEMPT**: `rfcs/accepted/process/0009-identity-evolution-v12.md` (self-refs).
- **No new spec text** — pure format compliance.

## RFC

- Primary: RFC-0011, RFC-0012, RFC-0013, RFC-0127, RFC-0113 (cross-ref format)
- Rule source: `CLAUDE.md §RFC Reference Conventions Reaffirmed` + `docs/BLUEPRINT.md §RFC Process`

## Dependency edges

| From                                                                              | To                | Why                                    |
| --------------------------------------------------------------------------------- | ----------------- | -------------------------------------- |
| `rfcs/accepted/numeric/0112-deterministic-vectors.md` lines 80,275,278,286,450    | Drop `v1.20`      | Cross-RFC ref to RFC-0111 (5 sites)    |
| `rfcs/accepted/numeric/0127-dcs-blob-amendment.md` lines 47,125,150,240,535-539,744,749 | Drop `v2.5.1`/`v2.6.0` | Cross-RFC ref to RFC-0126 (9 sites); self-refs preserved |
| `rfcs/accepted/numeric/0113-deterministic-matrices.md` lines 224,1042             | Drop `v1.12`/`v1.20` | Cross-RFC ref to RFC-0112/RFC-0111 (2 sites) |
| `rfcs/accepted/process/0010-canonical-did-codec.md` line 406                      | Drop `v1.3`       | Cross-RFC ref to RFC-0862 (1 site)     |

No new cyclic edges. Pure format compliance; spec content unchanged.

## Problem

`CLAUDE.md §RFC Reference Conventions Reaffirmed` states:

> "When referencing RFCs in prose, cross-references, changelogs, and
> approval criteria — use only the number. Never include status, version
> pins, or metadata."

The Round-3 review surfaced 17 cross-RFC prose citations that violate
this rule by appending `vN.M` to the RFC number. Examples:
- `RFC-0111 v1.20` (×5 sites in 0112)
- `RFC-0126 v2.5.1` (×9 sites in 0127)
- `RFC-0112 v1.12` (0113:224)
- `RFC-0862 v1.3` (0010:406)

These violate the convention because:
1. Version pins are fragile — they decay as RFCs are amended, leaving
   the prose out-of-date.
2. The convention explicitly forbids them in prose; only the RFC's own
   Status header and Version History table carry version info.
3. Cross-RFC readers chasing the reference look for the RFC number, not
   a specific version; the vN.M suffix adds noise.

## Acceptance Criteria

- AC-1: `rfcs/accepted/numeric/0112-deterministic-vectors.md` — 5 sites
  updated: lines 80, 275, 278, 286, 450. `RFC-0111 v1.20` → `RFC-0111`.
- AC-2: `rfcs/accepted/numeric/0127-dcs-blob-amendment.md` — 9 sites
  updated: lines 47, 125, 150, 240, 535, 536, 539, 744, 749.
  `RFC-0126 v2.5.1` → `RFC-0126`; `RFC-0126 v2.6.0` → `RFC-0126`.
  Self-refs in Version History preserved (no change).
- AC-3: `rfcs/accepted/numeric/0113-deterministic-matrices.md` — 2 sites
  updated: lines 224, 1042. `RFC-0112 v1.12` → `RFC-0112`;
  `RFC-0111 v1.20` → `RFC-0111`.
- AC-4: `rfcs/accepted/process/0010-canonical-did-codec.md` — 1 site
  updated: line 406. `RFC-0862 v1.3` → `RFC-0862`.
- AC-5: NO change to `rfcs/accepted/process/0009-identity-evolution-v12.md`
  (self-refs to own v1.2 identity).
- AC-6: NO change to any RFC Version History table (the only place
  version pins belong).
- AC-7: Verification grep returns zero hits:
  `grep -rEn 'RFC-0[0-9]+\s+v[0-9]+\.[0-9]+' rfcs/accepted | grep -v 'Version History'`

## Out of scope (NOT this mission)

- Defect 7 (underscore role-tokens, 26 sites) — separate mission 0105-v2.
- N4 (ProviderStake serde default) — separate mission 0900-d2.
- 5 REFOOTED R1+R2 defects — no fix needed.
- Any RFC spec text changes — pure format compliance.

## Termination condition

- All 4 RFC files updated per AC-1 through AC-4.
- AC-5 (0009 exempt) + AC-6 (Version History exempt) honored.
- AC-7 verification grep returns zero hits.
- Memory card created + MEMORY.md updated.
- Mission file `git mv` to `missions/claimed/0862-c10-rfc-version-pin-cleanup.md`.
- 1 commit: `chore(rfcs): 0862-c10 — strip cross-RFC vN.M version pins (4 files, 17 sites)`.
- NO push performed — push awaits user instruction per `feedback_initiation_user_only`.