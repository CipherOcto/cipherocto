---
name: 0205-stoolap-quarterly-reviews
description: Open 2026-08-19; RFC-0205 §Future Work phantom pointer 4/4 — per-Phase-2 quarterly split-review audit log. Quarterly audit of fork divergence + consumer-set + signing-key rotation, logged in `docs/audits/stoolap-quarterly-reviews.md` per TV-0205-09.
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T22:50:00.000Z
---

# Mission `0205-stoolap-quarterly-reviews` — OPEN 2026-08-19

## Scope

Per-Phase-2 quarterly split-review audit of the Stoolap fork substrate.
Covers:

- Fork divergence (`git rev-list --count upstream/main..feat/blockchain-sql`;
  threshold per RFC-0205 §Error Handling)
- Consumer-set audit (which Layer B crates consume via
  `octo_storage_core::Database`; check against `crates/` inventory)
- Signing-key rotation check (§HW Key Custody 90-day rotation
  schedule; flag any key approaching expiry)
- Bump audit (which `octo-stoolap-frozen-vN` tags landed this quarter;
  verify tag signatures per TV-0205-05)
- CVE audit (any unresolved upstream CRITICAL CVEs this quarter)

## Acceptance Criterion

Audit log file `docs/audits/stoolap-quarterly-reviews.md` exists and
has one entry per 90-day cycle post-v0; each entry covers all 5 audit
points above; entry is signed by 1-of-3 quorum steward + linked to
the corresponding TV-0205-09 audit invocation.

## Cross-references

- RFC-0205 §Future Work (this mission is the bullet's real pointer)
- RFC-0205 §Release-Tag Pin Policy (quarterly window = 90 days)
- RFC-0205 §HW Key Custody (signing-key rotation check)
- TV-0205-09 (quarterly audit log CI gate)
- TV-0205-10 (upstream-bump audit log CI gate)

## Out of scope

- Fork feature upstreaming (owned by `0205-stoolap-fork-feature-upstreaming`)
- Fork retirement (owned by `0205-stoolap-fork-retirement`)
- Release process (owned by `0205-octo-stoolap-frozen-release-process`)
