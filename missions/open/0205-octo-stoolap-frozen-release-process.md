---
name: 0205-octo-stoolap-frozen-release-process
description: Open 2026-08-19; RFC-0205 §Future Work phantom pointer 2/3 — tagging + signing convention for `octo-stoolap-frozen-vN` freeze tags. Defines `git tag -s` invocation, FIDO2/YubiKey signing-key custody, `trusted-keys.txt` allowlist, and tag-match byte-equal verification (TV-0205-05 leg (b)).
metadata:
  node_type: mission
  type: project
  originSessionId: 9a316ae1-cb15-46f4-801f-834acacd23ae
  created: 2026-08-19T22:50:00.000Z
---

# Mission `0205-octo-stoolap-frozen-release-process` — OPEN 2026-08-19

## Scope

Define the tagging + signing convention for `octo-stoolap-frozen-vN`
freeze tags used by RFC-0205 to mark each certified frozen rev.
Covers:

- `git tag -s <tag> <sha>` invocation with FIDO2/YubiKey signing
- `trusted-keys.txt` allowlist + `git verify-tag` invocation
  (TV-0205-05 leg (c))
- Tag-match byte-equal verification: `git rev-parse <tag>` MUST equal
  the `<sha-0>` in `crates/octo-storage-core/Cargo.toml` (TV-0205-05
  leg (b))
- Force-push retargeting defense (TV-0205-05 leg (b))

## Acceptance Criterion

Documented in `docs/runbooks/stoolap-steward.md` (per RFC-0205 §Key
Files to Modify checklist): the `git tag -s` + `git verify-tag`
invocations, the `trusted-keys.txt` path, the byte-equal check
command, and the rotation procedure per §HW Key Custody. CI gate:
`.github/workflows/ci.yml` runs TV-0205-05 on every PR that touches
`crates/octo-storage-core/Cargo.toml`.

## Cross-references

- RFC-0205 §Future Work (this mission is the bullet's real pointer)
- RFC-0205 §Release-Tag Pin Policy (tagging policy)
- RFC-0205 §HW Key Custody (key custody + rotation)
- RFC-0205 §Bump Acceptance Criteria (bump procedure)

## Out of scope

- Fork feature upstreaming (owned by `0205-stoolap-fork-feature-upstreaming`)
- Fork retirement (owned by `0205-stoolap-fork-retirement`)
- Quarterly review audits (tracked by `docs/audits/stoolap-quarterly-reviews.md`)
