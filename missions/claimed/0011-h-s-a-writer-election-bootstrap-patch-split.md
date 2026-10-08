---
name: 0011-h-s-a-writer-election-bootstrap-patch-split
description: Patch-split stub mission for RFC-0862p-a disambiguator creation
type: RFC scaffolding
status: Claimed
created: 2026-09-18
depends_on:
  - rfcs/draft/networking/0862p-a-writer-election-bootstrap.md
  - rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md
related:
  - rfcs/draft/process/0011-h-oct-cli-network-subcommands.md
---

# Mission — 0011-h-s-a-writer-election-bootstrap-patch-split

## Status: Claimed

Stub mission paired with RFC-0862p-a (patch-split disambiguator). Created per RFC-0011-h §Substrate-Faithful disambiguator convention to resolve RFC-0862 / RFC-0862p-a dual-cite ambiguity.

## Scope (minimal)

1. **RFC-0862p-a file exists** at `rfcs/draft/networking/0862p-a-writer-election-bootstrap.md` ✓
2. **RFC-0862 base** remains at `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md` (v1.4.0 amendment content merged; do NOT re-extract)
3. **Disambiguator discipline:** bare `RFC-0862` = base surface; `RFC-0862p-a` = patch-targeted cite. Per RFC-0855p-a/b/c precedent.

## Acceptance criteria

- [ ] AC-1: `rfcs/draft/networking/0862p-a-writer-election-bootstrap.md` exists with §Status = Draft + §Relationship to base + §Scope (patch-targeted cites cross-ref table)
- [ ] AC-2: 6 RFC-0862p-a cites in RFC-0011-h resolve to this file (L51, L125, L151, L199, L695, L837 — per R39.5)
- [ ] AC-3: bare RFC-0862 cites in RFC-0011-h continue to point to base + v1.4.0 amendment (no extract-back from base)

## Out of scope

- Re-extracting v1.4.0 amendment content into this file (content lives in base)
- DRY review loop on the stub itself (file is minimal; not subject to RFC text DRY CLOSURE gate — gate applies to RFC-0011-h only)
- Mission YAML 5-len DRY review (stub status; archive on RFC-0862p-a promotion)

## Notes

Created 2026-09-18 per RFC-0011-h R40.5 fix sweep (user-approved direction: "Create RFC-0862p-a").
