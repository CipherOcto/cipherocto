---
name: rfc-0910-key-files-table-names-a-crate-that-does-not-exist-and-its-phase-1-tracker-never-closed
description: "RFC-0910 is the marketplace corpus's one RFC whose test vectors all execute and match, and it is also the RFC that most completely fails to describe what was built. section Key Files to Modify names four paths and not one resolves: two RFC rows point at rfcs/draft when the RFCs were promoted to accepted and final, and two code rows point into crates/quota-router, a crate that does not exist anywhere in the workspace. The same document names the correct crate elsewhere, in its Test Vectors note. There is no tokenizer.rs in any crate; the tokenizer code lives inside pricing.rs, and the RFC's own SpendReceipt import example points at a crate::tokenizer module that was never created, for a TokenSource enum that actually lives in the keys module. The phase tracker records Phase 1 as not implemented in two separate checklists, in a document whose acceptance list ticks all ten of Phase 1's components immediately above the box that denies it, and whose archive mission 0910-a closed Phase 1 on 2026-04-27. The RFC was then revised twice, on 2026-04-27 and 2026-04-29, and neither revision marked Phase 1 done. get_canonical_tokenizer is described three different ways, in the RFC, in the archived mission, and in the code, and no two agree; the RFC separately contradicts itself on whether the function uses prefix heuristics at all. The Phase 1 to Phase 2 migration note calls the changeover a data entry task of roughly 15 to 20 rows; the exact-match table has 26 entries, and an exact-match-only Phase 2 cannot reproduce an unbounded prefix fallback, which by execution changes the tokenizer for every unlisted o-series variant."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0910
    - 0910-a-pricing-table-registry
---

# Mission `rfc-0910-key-files-table-names-a-crate-that-does-not-exist-and-its-phase-1-tracker-never-closed`

## Status

Deferred. Every fix below is either an RFC-owner decision about what a §Key
Files table in an accepted document is for, or a status correction to two
checklists. No code change is proposed and nothing was actioned. The archived
mission `0910-a` was read, never edited.

Seventh systematic sweep of a self-describing structure in an accepted
economics RFC, after RFC-0900's listing lifecycle, RFC-0918's phase table,
RFC-0955's sibling status, RFC-0960's substrate anchors and security
mechanisms, RFC-0630's consensus types, and RFC-0934's budget properties.

## Why this RFC is the important one

RFC-0910 is the counter-example that stops the pattern from reading as uniform.
Everything in it that describes _executed_ behaviour is correct, and correct to
the byte: the pricing-hash test vector, the cost vector, both tokenizer IDs, and
all eight end-to-end assignment rows resolve in the implementation and match.
It was measured by running it, not by reading it.

Everything in it that describes _intended_ behaviour — a file to modify, a
phase to complete, a migration to perform — is wrong, and wrong in the same
recursive way. A reader who trusts §Test Vectors will be right; a reader who
trusts §Key Files to Modify will find nothing at all.

The mission for a sweep like this is therefore not "the RFC is stale". It is:
**the sections that were updated against reality are correct, and the sections
nobody re-read after the code landed are fiction.** That distinction is only
visible from a document where both kinds coexist.

## Finding 1 — §Key Files to Modify: four rows, zero resolve

| RFC names                                                     | Status  | What is actually there                             |
| ------------------------------------------------------------- | ------- | -------------------------------------------------- |
| `rfcs/draft/economics/0910-pricing-table-registry.md`         | missing | the RFC is at `rfcs/accepted/economics/`, promoted |
| `rfcs/draft/economics/0909-deterministic-quota-accounting.md` | missing | RFC-0909 is at `rfcs/final/economics/`, promoted   |
| `crates/quota-router/src/pricing.rs`                          | missing | `crates/quota-router-core/src/pricing.rs`          |
| `crates/quota-router/src/tokenizer.rs`                        | missing | **no `tokenizer.rs` exists in any crate**          |

The `crates/quota-router` directory does not exist. This is not a path that
drifted by one segment — the named crate was renamed or split into
`quota-router-core`, and every code row in the table kept the old name.

**The same document already knows the new name.** The Test Vectors note credits
the hash vector as verified against `crates/quota-router-core/src/pricing.rs`,
in the `compute_pricing_hash_tests` module. Both halves of that claim are
correct: the path resolves, and the module exists. So one section was updated
after the crate split and the section immediately below it was not.

**No `tokenizer.rs` was ever written.** The tokenizer functions are defined in
`pricing.rs` beside the pricing code. The table's fourth row names a split that
was never performed.

That same error propagates into the specification body. The `SpendReceipt`
example imports `TokenSource` from `crate::tokenizer`; there is no such module,
and `TokenSource` is defined in the crate's `keys` module.

## Finding 2 — Phase 1 is denied twice, by a document that ticks all of it

Two checklists record Phase 1 as outstanding:

- §Implementation Phases lists six Phase 1 items, all unticked.
- The acceptance list has an unticked "Phase 1 implemented" box.

Ten boxes immediately above that unticked one are ticked, and they are
Phase 1's components: `register()` immutability, `get_pricing()`, `get_by_hash()`,
`get_versions()`, `get_version()`, `compute_pricing_hash()`, `compute_cost()`,
`get_canonical_tokenizer()`, `tokenizer_version_to_id()`, and both BLAKE3 test
vectors. The document ticks every part and then denies the whole, one line
below.

**The denial is false, and this was measured rather than argued.**
`cargo test -p quota-router-core --lib pricing` returns 32 passed and 0 failed,
including a test that asserts the RFC's exact pricing hash
`4a065c51147d4730379d600c4a491778b98f66a8e381c5dfdf51f42052c32f60` and a test
that asserts the cost vector's `Ok(6000)`.

**The sequence is the finding.** The archived mission `0910-a` completed
Phase 1 on 2026-04-27 with every acceptance criterion ticked, including that
same hash vector. The RFC's Version History then records v30 on 2026-04-27 and
v31 on 2026-04-29. Two revisions landed after the mission closed, and neither
one marked Phase 1 implemented, in a document whose next section reports the
correct crate path.

This is the inverse of the archived `0934-a` case, where a mission checked off
enforcement the code does not have. Here a mission checked off work the code
does have, and the RFC never learned about it.

## Finding 3 — the tokenizer dispatch is described three ways, and the RFC denies itself

`get_canonical_tokenizer` is an exact-match table **followed by** a prefix
fallback. Three documents describe it, and no two agree.

| Source                                 | Account                                                                        |
| -------------------------------------- | ------------------------------------------------------------------------------ |
| RFC-0910 §Implementation Phases        | "4-character prefix dispatch" — `gem-`, `gpt-`, `o1`, `o1-m`, `o1-p`, `clau`   |
| mission `0910-a` §Implementation Notes | "Tokenizer 4-char prefix dispatch" — adds `o1-` and `o3-`, still seven         |
| the implementation                     | `gemini-`, `gpt-`, `claude-`, `mistral-`, `llama-`, `o1-m`, `o1-p`, `o1`, `o3` |

Every account is wrong in a different direction. The RFC omits `mistral`,
`llama` and `o3` entirely; writes `gem-` where the code tests `gemini-`; and
lists `o1`, which is two characters, as a four-character prefix. The mission
adds an `o3-` arm the code does not have, with a trailing hyphen the code does
not test. Neither document mentions the exact-match table in the context where
prefix dispatch is being described, and that table holds **26** entries — every
o-series, Gemini, Mistral and LLaMA model the RFC's §Tokenizer Assignment Table
lists.

**The RFC contradicts itself outright.** §Tokenizer Lookup Function says
"Known models use an exact-match lookup table (no prefix heuristics)". Four
other places — the database schema comment, the Tokenizer Assignment Table
note, the Phase 1 box, and the Phase 1 to 2 migration note — say Phase 1 uses
in-memory prefix dispatch and does not query the table. Both statements are
false about the real function, which does the table first and falls back to
prefixes.

**One test vector cites the wrong mechanism.** The `claude-3-opus` tokenizer
row is annotated as verified by four-character prefix `clau`. But
`claude-3-opus` is an entry in the exact-match table, so it resolves before any
prefix is tested. The annotation describes a path that never runs for that
model. The prefix string is also wrong against the RFC's own code listing, which
tests `claude-` and not `clau`.

The "4-character" wording is inherited, not authored fresh. Version History v17
introduced it, v18 hardened a stale schema comment from "first-character" to
"4-character", and v20 later added the case-insensitive fallback. Each revision
fixed the defect in front of it. Nobody revisited the phrasing once the
exact-match table existed.

**The fallback branch is entirely unexercised.** Every tokenizer test in the
crate names a model that is already in the exact-match table, so the prefix
fallback never executes under test. This was proved by mutation rather than by
inspection: rewriting the o-series arm to return the wrong tokenizer left the
suite at 32 passed and 0 failed. The wrong behaviour — every unlisted o-series
variant silently billed against the wrong vocabulary — is not caught by any
test in the workspace.

## Finding 4 — the Phase 1 to Phase 2 migration note changes billing

The migration note is the sharpest claim in the document, because Phase 2 is
the phase that replaces the lookup path:

> Migration requires populating one row per supported model name —
> approximately 15-20 rows for the built-in model set. Wildcard/glob patterns
> are NOT supported in Phase 2. This is a data entry task, not a code change.

Two claims, both wrong.

**The row count.** The exact-match table has 26 entries, not 15 to 20.

**"A data entry task, not a code change."** The prefix fallback covers an
unbounded set of names — anything starting with `gpt-`, `claude-`, `mistral-`,
`llama-`, `gemini-`, `o1` or `o3`. A finite exact-match table cannot reproduce
an unbounded rule, so Phase 2 is a semantic change, and the note does not say
so.

**Which models it changes was measured.** Resolution for names absent from the
exact table, from the live implementation:

| Model                                                        | Resolves today                |
| ------------------------------------------------------------ | ----------------------------- |
| `o1-turbo`                                                   | `tiktoken-o200k_base`         |
| `o1-2025-01`                                                 | `tiktoken-o200k_base`         |
| `o3-pro-max`                                                 | `tiktoken-o200k_base`         |
| `gpt-5`, `claude-4-opus`, `gemini-3-pro`, `mistral-medium-3` | `tiktoken-cl100k_base-v1.2.3` |

Only the o-series diverges. Every other family's unlisted variants happen to
coincide with `DEFAULT_TOKENIZER`, so exact-match-only would silently agree
with them. The o-series returns `o200k_base` and would instead return
`cl100k_base` — a different vocabulary, therefore different token counts for
the same request, therefore a different computed cost, on precisely the newest
model family and the one most likely to gain variants.

That is the failure this RFC exists to prevent. Its determinism requirements
are stated in terms of identical token counts across routers, and the migration
note describes the one step that would break them as bookkeeping.

## What the RFC owner should do

- **Repoint or delete §Key Files to Modify.** Three of the four rows are a
  rename away from correct and two of them need a status promotion recorded.
  The fourth names a module split that was never performed, so it needs a
  decision rather than a repoint.
- **Fix the `crate::tokenizer` import in the `SpendReceipt` example.** It
  points at a module that does not exist, for a type that lives in `keys`.
- **Tick Phase 1, or delete the phase tracker.** It currently denies a phase
  that the acceptance list above it ticks in full and that the test suite
  demonstrates. Leaving both is worse than either state alone.
- **Rewrite the prefix description once**, from the implementation, and delete
  the "4-character" phrasing wherever it survives. It appears in the schema
  comment, the assignment table note, the Phase 1 box and the migration note,
  and it is wrong in all four.
- **Rework the migration note.** It has to state that Phase 2 replaces a prefix
  fallback with exact matching, enumerate the o-series explicitly, and either
  accept the o-series divergence as intended or specify how it is preserved.
  This is a design decision, not an editorial one.
- **Add a test that reaches the fallback.** No test currently executes the
  prefix path, so any of its arms can be wrong without a failure.

## Reproduction

```bash
# all four Key Files rows fail to resolve
for f in rfcs/draft/economics/0910-pricing-table-registry.md \
  rfcs/draft/economics/0909-deterministic-quota-accounting.md \
  crates/quota-router/src/pricing.rs \
  crates/quota-router/src/tokenizer.rs; do
  [ -e "$f" ] && echo "EXISTS  $f" || echo "missing $f"
done
ls -d crates/quota-router 2>&1 | tail -1   # no such directory
find crates -name 'pricing.rs' -o -name 'tokenizer.rs'   # only quota-router-core/src/pricing.rs

# the same document names the right crate elsewhere
grep -n 'crates/quota-router-core/src/pricing.rs' \
  rfcs/accepted/economics/0910-pricing-table-registry.md

# TokenSource is not in a tokenizer module
grep -rn 'enum TokenSource' crates/ --include=*.rs

# Phase 1 denied by two checklists, ticked in ten parts by a third
grep -n 'Phase 1 implemented' rfcs/accepted/economics/0910-pricing-table-registry.md

# and it is in fact implemented: 32 passed, including the RFC's own hash vector
cargo test -p quota-router-core --lib pricing

# the migration note's row count
sed -n '/const EXACT_TABLE/,/^    \];/p' crates/quota-router-core/src/pricing.rs \
  | grep -c '^        ("'      # 26, not "approximately 15-20"

# the prefixes the two documents name that the code does not test
grep -n 'starts_with' crates/quota-router-core/src/pricing.rs

# the mutation that no test catches: rewrite the o-series arm to return the
# wrong tokenizer, then run the suite. It stays at 32 passed, 0 failed.
```

## Cross-references

- RFC-0910 §Key Files to Modify — the section swept, four rows, none resolving
- RFC-0910 §Test Vectors — the same document's accurate half, verified by
  execution; the contrast is the point of this mission
- RFC-0910 §Implementation Phases and the acceptance list — the two places
  Phase 1 is denied
- RFC-0910 §Tokenizer Lookup Function — "no prefix heuristics", contradicted by
  four other sections of the same document
- RFC-0910 §Tokenizer Assignment Table — the `claude-3-opus` row annotated with
  a prefix that never runs for that model
- Mission `0910-a-pricing-table-registry` (archived) — closed Phase 1 on
  2026-04-27; read, not edited
- Mission `rfc-0959-key-files-table-points-ten-rows-at-nothing` — the same
  section name failing for a different reason, since there the files were never
  created at all, and here the work landed in a crate the table never renamed
- Mission `the-budget-mission-checks-off-atomic-enforcement-the-code-does-not-have`
  — the inverse case, where a mission ticks what the code lacks
- Mission `rfc-0955-calls-its-accepted-sibling-draft-and-gates-phase-5-on-a-promotion-already-made`
  — a phase gate standing open on conditions already met

## Errors made while producing this mission

Recorded because two of them nearly produced false findings.

- The prefix list was first read as six four-character strings. `o1` is two
  characters, and the first draft of the comparison table repeated the RFC's
  "4-character" framing while trying to refute it. The framing has to be quoted
  before it can be doubted.
- An intermediate reading of the archived mission's acceptance criterion
  implied the mission described prefix dispatch only. The criterion reads
  "exact match + 4-char prefix fallback dispatch", which is accurate; the error
  is in the Implementation Notes block beneath it, which is not.
- The initial claim that the fallback branch is untested was asserted from
  reading five test names. It was then proved by mutation, which is the only
  reason it is in this mission at all. The mutation had to produce the specific
  wrong behaviour, so it rewrote one arm to a demonstrably wrong tokenizer
  rather than deleting the branch outright.
- The line-ref check run as part of validating this mission returned **10,695
  instances across 326 files**, where it had previously been recorded as zero.
  The earlier figure was not wrong so much as unscoped: the check had been run
  over files this work touches, and never over the tree. Both numbers were
  true and only one of them was being quoted as if it were the other. The bulk
  of the repository-wide total sits in `docs/reviews/` and `docs/audits/`,
  which the blueprint already treats as ephemeral and not committed, plus
  `docs/plans/`, `docs/research/` and `memory/`. Six live missions written
  earlier in this audit did carry violations — twelve instances, in quoted
  panic transcripts, quoted source excerpts, and one call-site table that used
  line numbers as identifiers. Those were fixed rather than excused: the
  transcripts keep their paths and note that line and column are elided under
  the rule, the excerpts keep both sites and are distinguished by label, and
  the call-site table is now keyed by symbol. No archived record was touched.
