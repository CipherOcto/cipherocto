---
name: the-section-cite-linter-never-reads-rust-source
description: "scripts/lint_section_cites.sh scans only *.md, so 6,528 section-citation tokens in tracked .rs files are never examined while the linter reports green. Eight dangling RFC-0900 citations in marketplace source were found by hand and corrected."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.1"
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

## More hand-verified instances — marketplace core (added at v1.1)

A second hand-verification pass over the marketplace facade found **four**
further false citations, all in `quota_router_core::marketplace`. These are
added to the eight above as authoritative findings, in the same way: each
checked against the real heading list, no extractor involved.

| Was                                                    | Now                                      | Why                                                                                                                                                                                                                                            |
| ------------------------------------------------------ | ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `RFC-0105 §Numeric Substrate`                          | `RFC-0105 §Scale Alignment Rules`        | RFC-0105 has no section by that name; its headings are Data Structures, APIs/Interfaces, SQL Integration, Scale Alignment Rules, Arithmetic Algorithms. The scale-0 rule the comment meant is Scale Alignment Rules                            |
| `RFC-0862 §StoolapSpendLedger pattern`                 | `RFC-0862 §StoolapSpendLedger substrate` | trailing prose dropped; the real heading is `### StoolapSpendLedger substrate (v2.0, additive on v1.4.0)`                                                                                                                                      |
| `RFC-0965 §3.9`                                        | `RFC-0967 §8`                            | neither RFC-0965 nor RFC-0967 has a §3.9. RFC-0965 numbers 0–8 with 2.1–2.4 and 4.1; RFC-0967 numbers 1–8. `Caveat::PolicyReference` is defined in RFC-0967 §8, and RFC-0965 defines the capability envelope it attaches to                    |
| "the documented ≤1k-provider target (Gap 5 perf note)" | removed                                  | RFC-0900 contains no provider-count target, no performance section, and no "Gap 5 perf note". Found while auditing the `AskRead::cheapest` candidate limit — see mission `ask-repo-cheapest-silently-ignores-asks-outside-its-candidate-limit` |

Also re-confirmed **valid** in the same pass: `RFC-0862 §StoolapSpendLedger
substrate`, `RFC-0967 §5` ("## 5. Attenuation: subgraph relation"),
`RFC-0900 §Escrow Flow`, `§Dispute Resolution`, `§Dispute Evidence Challenge`,
`§Market Operations`, `§Slashing Model`.

### A caution for whoever automates this: RFC-0862 is two documents

While checking `RFC-0862 §StoolapSpendLedger`, `find rfcs -name "0862*.md"`
returned a file with no spend-ledger content at all. The reason is worth
recording on its own, because it is not a glob artefact:

```text
0862-writer-election-bootstrap-v130.md  "# RFC-0862 v1.4.0 — Writer Election + Bootstrap Integration"
                                        19 mentions of StoolapSpendLedger
0862-stoolap-data-sync.md              "# RFC-0862 (Networking): Stoolap Data Sync Protocol"
                                        0 mentions
0862p-a-writer-election-bootstrap.md   "# RFC-0862p-a — Writer Election Bootstrap"
                                        0 mentions
```

**Two distinct documents are both titled RFC-0862** — one about writer
election and bootstrap, one about the Stoolap data sync protocol — with no
discriminator in the number. The `StoolapSpendLedger` section lives in the
_Writer Election_ document.

So a bare `RFC-0862` citation is ambiguous: a reader following it has a
coin-flip chance of landing in a document that never mentions the subject.
Any automated resolver that keys on the numeric prefix will pick
whichever file sorts first and silently produce a false negative here.
`0968-A1`, `0957-A`, `0959-A` and similar amendments offer the same trap
in milder form.

This is worth an owner decision in its own right: either RFC-0862 is two
RFCs that need distinct numbers, or the data-sync document should be
renamed to an amendment suffix.

Ten hand-verified findings now: eight from the first pass, four from this
one, overlapping on none.

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
