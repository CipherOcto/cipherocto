---
name: rfc-0960-pins-six-companion-versions-and-every-one-is-behind
description: "RFC-0960's Dependency Validation table records the Accepted version of all six companion RFCs. Every one of the six is behind what the companion's own Status header says: RFC-0961 v2.0 against v2.1, RFC-0962 v2.0 against v2.1, RFC-0963 v2.0 against v3.1, RFC-0964 v1.1 against v1.2, RFC-0965 v1.1 against v2.0, RFC-0967 v1.0 against v1.1. The cause is visible in the tree: a corpus-wide status sync referenced as M37 updated the Status headers of RFC-0961, RFC-0962, RFC-0964, RFC-0965 and RFC-0967, and RFC-0960 carries no reference to it. RFC-0960 is the document that records those six versions, and it is the one the sync did not reach. The hard-block claim itself is unaffected, since all six companions are genuinely Accepted. Version metadata is also expressed four different ways across the RFC-0959 and RFC-0960 families."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0960
---

# Mission `rfc-0960-pins-six-companion-versions-and-every-one-is-behind`

## Status

Deferred. Six table cells and a Status header are an RFC edit, and the
version-bookkeeping convention is the owner's call. Nothing was actioned.

## Finding

RFC-0960 §Dependency Validation records, for each companion, the version that
was Accepted in lockstep on 2026-07-23. All six pins are behind:

| Companion | RFC-0960 pins | Its own Status header says | Behind by |
| --------- | ------------- | -------------------------- | --------- |
| RFC-0961  | v2.0          | Accepted **v2.1**          | 1         |
| RFC-0962  | v2.0          | Accepted **v2.1**          | 1         |
| RFC-0963  | v2.0          | Accepted **v3.1**          | 3         |
| RFC-0964  | v1.1          | Accepted **v1.2**          | 1         |
| RFC-0965  | v1.1          | Accepted **v2.0**          | 1         |
| RFC-0967  | v1.0          | Accepted **v1.1**          | 1         |

Not one is current. RFC-0963 is the furthest behind, three minor versions.

## The cause is visible in the tree

Five of the six companions carry a Status header that names the sync:

> Accepted v2.1 — Status header synced with §Version History v2.1-Resolved
> (Risk-closure round) per **M37 corpus-wide sync check**

RFC-0961, RFC-0962, RFC-0964, RFC-0965 and RFC-0967 each carry exactly one
`M37` reference. **RFC-0960 carries none.**

So this is not drift that accumulated unnoticed. A corpus-wide status sync ran,
reached five of the six companions, and did not reach RFC-0960 — which is the
one document whose dependency table exists to record those companions' versions.
The sync updated the sources and missed the index.

RFC-0963 does not name M37 in its header, but its Status line records its own
history precisely ("v2.0 Accepted 2026-07-23; v3.1 §9 amendment 2026-08-21"),
which is why RFC-0960's v2.0 pin for it is three versions stale.

## What is not broken

The table's substantive claim is that all six companions were promoted in
lockstep and that the hard-blocks are resolved. **That still holds** — every one
of the six is Accepted. The `YES → resolved` markers are accurate.

This is bookkeeping, not a gate failure, and it should not be read as one. The
finding is narrower: a status sync that exists in the corpus was applied
everywhere except to the document that indexes the synced values.

## RFC-0965 has three versions in play at once

Worth separating out, because it is not just a stale pin:

- RFC-0960's table says **v1.1**
- `0965-capability-extension-format`'s Status header says **Accepted v2.0**
- an amendment file in the same directory is named `0965-v21-payment-caveat-asset-binding.md`

Three files, three figures. The filename encodes a fourth distinct convention
(`v21` rather than `v2.1`).

## Version metadata has four shapes across two RFC families

| Where               | Convention                                                  |
| ------------------- | ----------------------------------------------------------- |
| RFC-0960 base       | `## Status` header, free text — "Accepted v3.0"             |
| RFC-0960 amendments | YAML frontmatter — `rfc:`, `version:`, `status:`, `amends:` |
| RFC-0959 amendments | `## 0. Status` header                                       |
| RFC-0959 base       | `## Status` header — "Accepted", **no version at all**      |

Only one of the four is machine-readable. A tool asking "what version is
RFC-0960" gets a frontmatter answer from v3.5/v3.6/v3.7 and a prose answer from
the base, and a tool asking the same of RFC-0959 gets no version from the base
at all.

RFC-0959's base is the awkward case, and it is worth being precise about what it
does and does not record. The `## Status` header is bare "Accepted". The version
_is_ in the document — but only inside a prose Note that opens "**Note (v2.0,
2026-08-19):**". So the version is not absent from RFC-0959; it is absent from
the place anything would look for it, and it is embedded in a sentence that also
explains a wire-format change. A machine reading the header gets no version; a
machine reading the whole document gets one it has to parse out of prose. Neither
is the frontmatter form the RFC-0960 amendments already use.

## Two claims of mine I dropped, and why

Both looked like findings and neither survived checking against how the project
actually works.

**The base saying `Accepted v3.0` is not stale.** The alignment mission frames
it explicitly:

> RFC-0960 v3.5 (canonical Accepted 2026-08-23) adds Vault Path Taxonomy
> amendment on top of **RFC-0960 v3.0 substrate** (landed via
> `0960-vault-substrate-amendment` 2026-08-18)

So the base document stays at its own accepted version and amendments are
layered documents on top. `Accepted v3.0` is the base's version, correctly.

**v3.7's `amends: RFC-0960` is not a broken chain.** v3.6 says
`amends: RFC-0960 v3.5` while v3.7 says `amends: RFC-0960`, which looks like it
skips v3.6. But v3.7 is a vault-balance-projection amendment and v3.6 is a
burn-event migration — parallel topics, not sequential ones. The two are
independent amendments off the same base, which is a different thing from a
broken chain. A reader cannot tell parallel from sequential without opening the
documents, which is a fair criticism but not a defect.

## What the owner should do

- **Update the six pins.** Six cells, and the fix is mechanical. RFC-0963 is the
  one worth checking rather than assuming, since it moved three versions.
- **Extend the M37 sync to RFC-0960**, or note in RFC-0960 that it was
  deliberately excluded. What is worth avoiding is the current state, where a
  sync is referenced by its participants and not by the document that records
  their results.
- **Pick one convention for version metadata.** Four shapes across two RFC
  families means no tool can answer "what version is this" without special
  cases, and the frontmatter form is already the most useful of the four.
- **Leave the hard-block markers.** They are correct.

## Reproduction

```bash
R=rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md

# what RFC-0960 pins
grep -oE 'RFC-09(6[1-5]|67) +\| Companion +\| \*\*Accepted v[0-9.]+' "$R" \
  | grep -oE 'RFC-[0-9]+|v[0-9.]+$' | paste - -

# what each companion says about itself
for f in 0961-ciphero-sql-language-spec 0962-consensus-session-protocol \
         0963-resource-shard-routing 0964-constraint-encoding-standard \
         0965-capability-extension-format 0967-policy-object-graph; do
  printf 'RFC-%s -> ' "${f:0:4}"
  grep -m1 -A2 '^## Status' "rfcs/accepted/economics/$f.md" \
    | grep -oE 'Accepted v[0-9.]+' | head -1
done

# the sync that reached five of six
grep -c 'M37' rfcs/accepted/economics/096[0-9]-*.md

# the four conventions
head -6 rfcs/accepted/economics/0960-v37-vault-balance-projection-substrate.md \
  | grep -E '^(rfc|version|status):'
head -22 rfcs/accepted/economics/0959-v28-settlement-cost-dqa-migration.md \
  | grep -m1 '^## 0. Status'
head -8 "$R" | grep -m1 '^Accepted'
head -8 rfcs/accepted/economics/0959-ask-settlement-chain.md | grep -m1 '^Accepted'

# RFC-0959's version: present, but only inside a prose Note
head -8 rfcs/accepted/economics/0959-ask-settlement-chain.md \
  | grep -oE '\*\*Note \(v[0-9.]+'

# and the convention that is not a defect
grep -n 'RFC-0960 v3.0 substrate' missions/claimed/0960-alignment-coordination.md
```

## Cross-references

- Mission `rfc-0960-closes-its-review-at-r28-and-records-sixteen-rounds` — the
  same document, where a recorded round count and a recorded version are both
  behind the claim made about them
- Mission `two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review`
  — RFC-0960's other bookkeeping gap, in the same document
- Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`
  — v27's discipline, which includes checking what the substrate says rather
  than what this document last recorded
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  this belongs to in miniature: a table of facts that no longer matches reality,
  where nothing checks
