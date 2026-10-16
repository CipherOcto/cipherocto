---
name: rfc-0909-downgraded-its-own-cross-router-requirement-and-left-the-must-in-the-invariant
description: "RFC-0909 sits in rfcs/final/, which the blueprint defines as Implemented and stable. Its own final revision did the opposite. The v69 changelog states it reclassified the cross-router token_source requirement from MUST to SHOULD, and it added the sections that carry the weaker wording, but three surviving passages still assert the requirement it downgraded, including a MUST in the numbered Economic Invariants list. The document therefore states its single most important claim six ways at three different strengths, two of which are flat denials of the other four. The test vectors are nonetheless correct: TV1 through TV4 were executed and all four match, and the provider/model normalization requirement added at v67 is implemented and tested. The vectors also have a gap that matters more than it looks. No vector isolates token_source, because TV2 varies request_id at the same time, so the table cannot demonstrate the divergence that the Token Source Divergence Handling section depends on. Measured directly, holding every other input at its TV1 value and changing only token_source produces an event_id that appears in none of the four vectors. Two stale cross-references remain: RFC-0910 is listed as a Draft, which is the only such reference in the whole accepted and final corpus, and the RFC-0903-B1 amendment path points into rfcs/draft when that document lives in rfcs/accepted."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0909
    - 0909-a-compute-event-id
---

# Mission `rfc-0909-downgraded-its-own-cross-router-requirement-and-left-the-must-in-the-invariant`

## Status

Deferred. The primary fix is an RFC-owner decision about which strength is
correct and therefore which passages have to change; that is not an editorial
call and is not made here. Two of the smaller findings are straightforward
documentation corrections the owner can take directly. No code change is
proposed and nothing was actioned. The archived `0909-*` missions were read,
never edited.

Eighth systematic sweep of a self-describing structure in an accepted or final
economics RFC, after RFC-0900's listing lifecycle, RFC-0918's phase table,
RFC-0955's sibling status, RFC-0960's substrate anchors and security mechanisms,
RFC-0630's consensus types, RFC-0934's budget properties, and RFC-0910's Key
Files table and phase tracker.

## Why this one lands harder than the others

`rfcs/final/` holds exactly two economics RFCs, RFC-0903 and RFC-0909. The
blueprint defines that tier as **"Implemented and stable"**, and the transition
rule into it is "Implemented". So the directory a document sits in is itself a
claim about it.

RFC-0909's most recent revision is a **self-downgrade**. Its v69 changelog row
records that it reclassified the cross-router `token_source` requirement from
MUST to SHOULD, and added the sections carrying the weaker wording. That is a
revision taking a promise away, in a document whose directory asserts
stability — and the revision was applied to the new prose but not to the
numbered invariants or to three older passages.

## Finding 1 — one requirement, six statements, three strengths

| #   | Location                                                | What it says                                                                                                                 |
| --- | ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 1   | §Idempotent Event Recording, "Cross-router determinism" | "Two routers processing identical requests independently produce identical `SpendEvent` records"                             |
| 2   | §Economic Invariants, invariant 6                       | "token_source **MUST** be identical across routers for a given request_id"                                                   |
| 3   | §Canonical Token Accounting, critical rule              | "Two routers processing the same request on the same router instance **MUST** produce identical token counts"                |
| 4   | §Ledger-Based Architecture, key points                  | "Deterministic event_id — SHA256 hex hash ensures same request = same event **across routers**"                              |
| 5   | §Definition: Deterministic Accounting                   | "This definition explicitly **excludes**: Cross-router execution — different router instances may produce different outputs" |
| 6   | §Known Limitations, token source divergence             | "the system does **NOT** guarantee cross-router deterministic token counts"                                                  |

Five of six assume the property. Two deny it. Invariant 6 is the sharpest,
because §Economic Invariants opens with "The following invariants **MUST** hold
at all times" — so the one place a reader would look for a definitive answer
still carries the requirement the RFC's own last revision retired.

The two CONSISTENCY GOAL blocks in §Canonical Token Accounting are a third
strength again, SHOULD, and they say substantially the same thing twice.

Entry 3 is also internally incoherent on its own: "two routers" processing the
same request "on the same router instance" describes one router, and it states
the requirement in terms of the thing §Definition excludes.

**The inconsistency was introduced by the fix, not inherited by it.** Compare
the v69 changelog row against what survived:

> v69: reclassify token_source MUST as CONSISTENCY GOAL (SHOULD); add Token
> Source Divergence Handling section; add Definition of Deterministic (RFC-0909
> Scope); add Strict Mode option; update Known Limitation to make
> race-dependent outcome explicit

Every one of those additions is present and correctly worded. What is missing
is the corresponding edit to the three places the reclassification was supposed
to reach. A reader of the changelog would reasonably conclude the downgrade was
complete.

## Finding 2 — the test vectors are correct, and this was checked by running them

This is the part of the RFC that holds, and it holds exactly.

- All four cross-router vectors were executed. **11 passed, 0 failed**, with
  each test asserting the RFC's published hash character for character.
- `test_normalize_provider_model` and `test_normalize_provider_model_unicode_nfc`
  are green, so the normalization requirement added at v67 — lowercase ASCII plus
  Unicode NFC on `provider` and `model` — is implemented and covered, which is
  what makes the vectors meaningful in the first place.
- The BLAKE3 tokenizer vector named in Approval Criteria matches the one
  RFC-0910 publishes.

The vectors prove what they can prove: `compute_event_id` is a deterministic
pure function of its arguments.

## Finding 3 — but the vectors cannot demonstrate the property v69 downgraded

§Test Vectors for Cross-Router Determinism says the vectors "verify that
`compute_event_id()` produces identical output across all router
implementations". That overstates what four vectors can establish, for a reason
worth stating precisely.

Identical **outputs** require identical **inputs**. `token_source` is one of
those inputs. The vectors vary four things across four rows:

| Vector | Varies                              |
| ------ | ----------------------------------- |
| TV1    | baseline                            |
| TV2    | `request_id` **and** `token_source` |
| TV3    | `key_id`                            |
| TV4    | `pricing_hash`                      |

**No row isolates `token_source`.** TV2 changes it, but it changes `request_id`
in the same row, so the resulting hash difference cannot be attributed. The one
input whose cross-router agreement was downgraded is the one input the table
cannot demonstrate, and it is therefore the input the table cannot protect.

This was measured rather than argued. Holding every other input at its TV1
value and changing only `token_source`:

| Inputs                           | `event_id`                                                         |
| -------------------------------- | ------------------------------------------------------------------ |
| TV1 inputs, `ProviderUsage`      | matches the RFC's TV1 exactly                                      |
| TV1 inputs, `CanonicalTokenizer` | `72dd2d57d6f0a274faa1594bb2fb5a6e60265d7c8729355155b38e8c3a17c362` |

They differ, and the second value appears in **none** of the four published
vectors.

So the prose is right and the table is incomplete: §Token Source Divergence
Handling asserts that routers deriving different `token_source` "compute
different `event_id` values", and that is true. It is simply the one claim in
that section with no vector behind it, and it is the claim the whole
first-writer-wins resolution model rests on.

## Finding 4 — two stale cross-references

**RFC-0910 is listed as a Draft.** §Dependencies calls it
"RFC-0910: Pricing Table Registry (Draft — for immutable pricing tables)". It is
Accepted at v31. The changelog shows the label was pinned deliberately as
"Draft v2" at v52 and never revisited.

This is ordinary staleness, and worth saying so: RFC-0909's last revision
predates RFC-0910's promotion, so the label was correct when written. What
makes it worth fixing is that `rfcs/final/` is the tier that promises
stability, and a sweep of every RFC in `accepted/` and `final/` found this to be
the **only** stale Draft label in the corpus — it is an isolated instance, not
a class, and should be fixed as one line.

**The RFC-0903-B1 path is wrong.** §Relationship to RFC-0903 closes with a
pointer to `rfcs/draft/economics/0903-B1-schema-amendments.md`. That document
is at `rfcs/accepted/economics/`.

## Finding 5 — the dependency rationale names the wrong mechanism

§Dependencies lists RFC-0126 as "Accepted v2.5.1 — **for canonical JSON
serialization**".

RFC-0909 says the opposite about this RFC's central use of it, twice. In
§Canonical Token Accounting and again in the `compute_pricing_hash` doc comment:

> RFC-0126 §JSON Allowed Contexts forbids JSON for Merkle tree leaves. Since
> `pricing_hash` feeds into `event_id` (a Merkle leaf input), it MUST use DCS
> binary encoding per Entry 16 (Part 3).

RFC-0910 makes the same correction from the other side. So the dependency line
credits RFC-0126 with supplying JSON canonicalization for a path the RFC
prohibits using JSON on, and the correction that fixed the actual code was
never carried back to the line that motivated the dependency.

## Finding 6 — two ticked boxes a repository cannot verify

§Approval Criteria ticks "NTP clock synchronization deployed across all router
instances". No repository can establish this; it is an operational fact about a
deployment, not a property of the code. The same list ticks the multi-tenant
collision mitigation while its own parenthetical records that "the RFC
identifies the options but does not specify the implementation details" — so the
box is ticked for having _described_ a mitigation rather than for having one.

Neither is dangerous, but both are boxes whose meaning cannot be checked by
reading the tree, and they sit in the one list a reader treats as a completion
record.

## What the RFC owner should do

- **Choose the strength, then apply it everywhere.** Either cross-router
  `token_source` agreement is a goal or it is not. The answer determines
  whether invariant 6 changes or the two denials do. This is the decision; the
  rest is bookkeeping.
- **Add a fifth vector that varies only `token_source`.** The value is already
  known, and a vector that isolates the downgraded input is what turns
  §Token Source Divergence Handling from an assertion into something the table
  demonstrates.
- **Retitle the vector table.** "For Cross-Router Determinism" overstates it.
  The vectors demonstrate a deterministic pure function, and the L2/L3 note in
  §Verification Methodology already draws that distinction correctly — which is
  the strongest evidence that the framing is the accidental part.
- **Fix the two stale references**: RFC-0910's status label, and the RFC-0903-B1
  path.
- **Fix the RFC-0126 dependency rationale** to name DCS Entry 16 rather than
  canonical JSON.
- **Reword the two approval boxes** so a reader can tell a documented option from
  a deployed control.

## Reproduction

```bash
# the vectors are real and pass
cargo test -p quota-router-core --lib compute_event_id     # 11 passed

# the MUST that v69 retired is still in the invariant list
grep -n 'MUST be identical across routers' \
  rfcs/final/economics/0909-deterministic-quota-accounting.md

# and the changelog row that retired it
grep -n 'reclassify token_source MUST' \
  rfcs/final/economics/0909-deterministic-quota-accounting.md

# the two passages that deny the property
grep -n 'explicitly \*\*excludes\*\*\|does NOT guarantee cross-router' \
  rfcs/final/economics/0909-deterministic-quota-accounting.md

# no vector isolates token_source: TV2 changes request_id in the same row
grep -n 'same as TV1 but' crates/quota-router-core/src/keys/mod.rs

# hold TV1's inputs, vary only token_source, and compare against all four
# published vectors. The CanonicalTokenizer value matches none of them.

# stale references
grep -n 'RFC-0910.*Draft' rfcs/final/economics/0909-deterministic-quota-accounting.md
ls rfcs/draft/economics/0903-B1-schema-amendments.md    # absent
ls rfcs/accepted/economics/0903-B1-schema-amendments.md # present

# RFC-0910's Draft label is the only one in accepted/ and final/
grep -rln 'RFC-0910' rfcs/accepted rfcs/final | \
  xargs grep -lE 'RFC-0910[^|]*\(Draft|RFC-0910.*Draft v'

# the dependency line names JSON, the body forbids it
grep -n 'canonical JSON serialization' \
  rfcs/final/economics/0909-deterministic-quota-accounting.md
```

## Cross-references

- RFC-0909 §Economic Invariants — invariant 6, the surviving MUST
- RFC-0909 §Test Vectors for Cross-Router Determinism — the table swept, correct
  but unable to isolate the input v69 downgraded
- RFC-0909 §Definition: Deterministic Accounting — the exclusion that
  contradicts the invariant
- RFC-0909 §Known Limitations — the second denial, also added at v69
- RFC-0909 Version History, v69 — the self-downgrade and its incomplete
  propagation
- RFC-0909 §Verification Methodology — the L2/L3 note that draws the distinction
  the vector table's title obscures
- Mission `0910-a-pricing-table-registry` (archived) and mission
  `rfc-0910-key-files-table-names-a-crate-that-does-not-exist-and-its-phase-1-tracker-never-closed`
  — RFC-0910 is the document this RFC mislabels, and it independently repeats
  the same "exact-match table then prefix fallback" description under a third
  set of wrong prefix names
- Mission `two-crates-define-spend-ledger-with-incompatible-schemas` — the other
  live mission touching `token_source`, scoped to the schema split rather than
  to the determinism claim
- Mission `rfc-0955-calls-its-accepted-sibling-draft-and-gates-phase-5-on-a-promotion-already-made`
  — the same stale-status failure in a sibling RFC, which makes the corpus-wide
  sweep that produced the "only one instance" count worth keeping

## Errors made while producing this mission

- The first draft of this mission claimed the four test vectors isolate
  `token_source` and that TV1 and TV2 differ only in it. They do not — TV2
  changes `request_id` in the same row. The claim was the more interesting one
  and it was wrong, and it was caught by reading the test bodies rather than the
  RFC's table. The finding that replaced it is that no vector isolates the
  input, which is the same observation with the sign flipped.
- Eight four-backtick lines appear to close three-backtick fences, which reads
  as malformed markdown. Tested and dismissed: CommonMark closes a fence opened
  with three backticks using a line of four or more, and `prettier --check`
  passes on the file. Recorded because the visual impression was strong and the
  conclusion was wrong.
- The stale RFC-0910 Draft label was initially framed as a corpus-wide pattern.
  It is not. A sweep of every RFC under `accepted/` and `final/` found exactly
  one, which changes it from a trend into a one-line fix and is the reason this
  mission is framed the way it is.
