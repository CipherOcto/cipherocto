---
name: 0205-stoolap-fork-feature-upstreaming
description: Open 2026-08-19; RFC-0205 §Future Work phantom pointer 1/3 — upstream-contribution strategy for fork-only Stoolap features. Sister mission to `0900-d2-stoolap-fork-dqa-driver-upstreaming.md` (DQA driver track); this mission covers the broader set of fork-only features that should be merged back to upstream Stoolap.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T22:50:00.000Z
---

# Mission `0205-stoolap-fork-feature-upstreaming` — OPEN 2026-08-19

## Scope

Track upstream-contribution strategy for fork-only Stoolap features that
CipherOcto needs to maintain but that could plausibly be merged back to
upstream Stoolap. Distinct from `missions/claimed/0900-d2-stoolap-fork-dqa-driver-upstreaming.md`,
which covers the DQA-driver upstreaming track; this mission covers the
broader set of fork-only features (16-byte MVCC DQA extension,
`encode_decimal_lexicographic`, `Value::quant`, `as_dqa`, etc.).

## Acceptance Criterion

Per feature, file an upstream PR against the Stoolap repository with a
linked CipherOcto RFC explaining why the feature is fork-only; track
acceptance + merge in `docs/audits/stoolap-upstream-prs.md`; quarterly
review of unresolved PRs.

## Cross-references

- RFC-0205 §Future Work (this mission is the bullet's real pointer)
- `missions/claimed/0900-d2-stoolap-fork-dqa-driver-upstreaming.md`
  (sibling DQA-driver track)
- RFC-0105 (DQA substrate)

## Out of scope

- DQA driver track (owned by 0900-d2)
- Fork retirement (owned by `0205-stoolap-fork-retirement`)
- Release process (owned by `0205-octo-stoolap-frozen-release-process`)
