---
name: the-section-cite-linter-never-reads-rust-source
description: "scripts/lint_section_cites.sh scans only *.md, so 6,528 section-citation tokens in tracked .rs files are never examined while the linter reports green. Eight dangling RFC-0900 citations in marketplace source were found by hand and corrected."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0003
---

# Mission `the-section-cite-linter-never-reads-rust-source`

## Status

Evidence recorded. Eight citations corrected. Linter scope change **proposed,
not implemented** — it is a policy decision, see below.

## The blind spot

`scripts/lint_section_cites.sh` enumerates its corpus with:

```bash
find "$RFC_ROOT" -type f -name '*.md' -print0
```

It therefore never opens a `.rs` file. Measured across tracked files:

| Corpus | §-tokens | Examined by the linter |
| ------ | -------- | ---------------------- |
| `*.md` | 23,013   | yes                    |
| `*.rs` | 6,528    | **no**                 |

Roughly 22% of every section citation in the repository is invisible to the
check that exists to validate section citations.

## Confirmed instances (hand-verified)

While auditing `marketplace::orderbook` I found the code citing
`RFC-0900 §Order Book`. RFC-0900 has no such section. Verified against its
complete heading list:

```
Status · Version History · Summary · Motivation · Specification (Core Concepts,
Token Economics, Routing Protocol, Market Operations) · Implementation ·
Settlement Model (Registry Decision, Escrow Flow, Dispute Resolution, Dispute
Evidence Challenge, Slashing Model, Slash Ledger Substrate) · Security ·
Related Use Cases · State Machines (Listing/Purchase/Dispute Lifecycle) ·
Observability · Security & Privacy · Related RFCs · References · Open Questions
```

Eight dangling citations, all corrected:

| Was                                  | Count | Now                          | Why                                                                                                                                              |
| ------------------------------------ | ----- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `RFC-0900 §Order Book`               | 2     | `RFC-0900, Gap 5.1`          | no such section; the gap is defined in the version-history row                                                                                   |
| `RFC-0900 §Order Book` (task market) | 1     | `RFC-0918 §Order Book`       | RFC-0918 _does_ have that section, and it is the right domain                                                                                    |
| `RFC-0900 §Reputation System`        | 3     | `RFC-0968`                   | RFC-0900 mentions reputation only in its version-history row, one table cell, and a Related RFCs entry. RFC-0968 is the reputation registry spec |
| `RFC-0900 §Settlement`               | 1     | `RFC-0900 §Settlement Model` | trailing word dropped from the real heading                                                                                                      |
| `RFC-0900 §Slashing Model defaults`  | 1     | `RFC-0900 §Slashing Model`   | trailing word                                                                                                                                    |

Also confirmed **valid** and left alone: `§Escrow Flow`, `§Dispute Resolution`,
`§Dispute Evidence Challenge`, `§Market Operations`, `§Slashing Model`.

## An order-of-magnitude signal — NOT authoritative

A crude heuristic pass over tracked `.rs` suggests **744 of 1,156** RFC
section citations are dangling (412 resolve).

Do not act on that number. The extractor was hand-written, it over-captures
multi-word headings (`RFC-0011 §changelog v2` is almost certainly `§Changelog`
plus trailing prose), and reimplementing a linter to measure it is exactly
the mistake this repo's tooling exists to prevent. The eight corrected above
were each verified by hand against the heading list and are the only
authoritative findings in this mission.

The real count requires the real parser.

## Proposal — not implemented

Extend `lint_section_cites.sh` to include `*.rs` in its corpus, and
distinguish citations that resolve from those that do not.

This is **owner policy**, deliberately not implemented here:

- it would turn a green check red across unrelated crates
- 744-vs-412 (or whatever the real number is) needs triage before it can gate
- the correct target section for a dangling cite is sometimes an authoring
  decision, not a mechanical rewrite

A safe sequencing would be: land the linter in report-only mode, triage the
output, fix, and only then make it blocking.

## Reproduction

```bash
# the linter's corpus
grep -n 'find .*RFC_ROOT' scripts/lint_section_cites.sh

# uncounted citations
git ls-files '*.rs' | xargs grep -c '§' | awk -F: '{s+=$2} END {print s}'

# verify a specific heading
grep -nE '^#{2,4} ' rfcs/accepted/economics/0900-ai-quota-marketplace.md
```

## Cross-references

- RFC-0003 (process/meta) — citation conventions
- `CLAUDE.md` §RFC Reference Conventions
- Mission `marketplace-escrow-is-not-wired-into-the-marketplace-facade` — the
  audit that surfaced the order-book citation
