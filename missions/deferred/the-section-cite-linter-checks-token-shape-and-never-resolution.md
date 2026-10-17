---
name: the-section-cite-linter-checks-token-shape-and-never-resolution
description: "The section-cite linter validates the shape of a citation token and never checks that the named section exists in the cited document. This is structural rather than a bug: the extraction regex captures only the section name and never the RFC number, so the linter has no idea which document a cite refers to and cannot resolve one even in principle. All three reject rules are form checks, and multi-word section names are truncated at the first space, so RFC-0862 section Transaction guarantees is examined as the bare token Transaction. Measured: a wholly invented section name passes, and the linter reports PASS with zero rejections on a corpus containing one. This explains why all five phantom-section citations found across the RFC-0959 documents survived. The linter's separate 45 percent coverage limit is already documented in its own comments as a maintainer decision, so this mission is about depth, not scope."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0959-A1
    - RFC-0862
---

# Mission `the-section-cite-linter-checks-token-shape-and-never-resolution`

## Status

Deferred. Widening or deepening a linter is a CI-policy change and the two
existing missions on this linter are both owner decisions. Nothing was actioned
and no workflow file was touched.

## The two gaps are different, and only one is documented

`the-section-cite-linter-never-reads-rust-source` records that
`scripts/lint_section_cites.sh` scans only `*.md`, so Rust source is never
examined. The linter's own comments are explicit and honest about this:

> this linter reads RFC_ROOT, which defaults to rfcs/, while roughly half the
> tracked section references in the repository live in docs/ and missions/
> Widening RFC_ROOT is a maintainer decision

That is **scope**: which files are read. It is documented, measured, and
declined deliberately.

This mission is about **depth**: what happens to the tokens that _are_ read.
They are checked for shape and never for existence, and that limit is not
stated anywhere in the script or its output.

## The extraction has no target document

```bash
grep -nEo '§/?[A-Za-z0-9]+([._/][A-Za-z0-9]+)*|§"[^"]+"' "$rfc_file"
```

The pattern captures the section name only. No RFC number is part of the token,
and the loop below it works on `$cite` and `$line_num`. The four `RFC-NNNN`
strings elsewhere in the script are all inside comments explaining tokenizer
decisions, never in logic.

So when the linter sees `§Transaction`, it does not know whether the claim was
`§Transaction` of RFC-0862, of RFC-0959, or of a document that does not exist.
Resolution is not a rule that was forgotten; it is not expressible in what the
token contains.

## v1.1 — correction: a resolution linter exists, is CI-wired, and measures this

Everything above is scoped to `scripts/lint_section_cites.sh`, and every
statement about it holds. But an earlier draft of this mission generalised it
into "nothing in the toolchain can see this", and **that was wrong.**

`scripts/lint-rfc-anchors.py` exists, resolves section anchors, and is wired
into `.github/workflows/lint.yml`. Its own comments describe the exact hazard
this mission was written about:

> A filesystem walk includes gitignored local scratchpads. In this repository
> that is 711 files under `docs/` contributing roughly 13,460 [headings]

It also has a `--corpus` flag, documented as reporting-only so it cannot turn a
gate red or green. Run here, it reports:

```text
resolving ONLY in untracked files, named sections: 548
resolving ONLY in untracked files, bare numbers : 556
```

So the mechanism I said did not exist does exist, and someone had already built
it and written down the problem it causes.

The finding that survives, restated honestly: `lint_section_cites.sh` is a
**hygiene** linter and cannot resolve, because its token carries no RFC number.
Resolution is a separate concern with a separate tool that already covers it.
The two open missions `the-anchor-linter-is-green-locally-and-red-in-a-clean-checkout`
and `rfcs-cite-sections-of-documents-the-repository-deliberately-does-not-track`
are the authoritative records for that, and this mission should not be read as
contradicting them.

I also filed two missions on this class before finding those two. They are
corrected in place and cross-reference them.

## All three rules are form checks

- **Reject 1**, trailing punctuation — token shape.
- **Reject 2**, over-deep numeric anchors beyond five levels — token shape.
- **Reject 3**, numeric-then-alpha with no matching subsection — the only
  semantic rule, and it consults a _corpus-wide_ set of subsection letters built
  from all headings under `RFC_ROOT`. It asks "is this letter used as a
  subsection anywhere in the corpus", not "does this section exist in the
  document being cited".

Nothing in the script can fail because a cited section is absent.

## Multi-word names lose everything after the first space

The tokenizer stops at whitespace, so:

| In the document                     | Extracted as   | Section actually meant   |
| ----------------------------------- | -------------- | ------------------------ |
| `RFC-0862 §Transaction guarantees`  | `§Transaction` | _Transaction guarantees_ |
| `RFC-0959 §Cache Classification`    | `§Cache`       | _Cache Classification_   |
| `RFC-0959 §Totally Made Up Section` | `§Totally`     | does not exist           |

`§Transaction` is a well-formed bare-alpha name. Reject 1 does not fire, Reject
2 does not fire, Reject 3 requires numeric-then-alpha and does not apply. The
token passes.

Measured on the real corpus:

```text
Summary: SCANNED=247 files, EXAMINED=10536 cite tokens
         ACCEPTED=3986 NON_CANONICAL=6550 REJECTED=0
         COVERAGE=45% of 23352 tracked section references
         unexamined: 12816 outside RFC_ROOT=rfcs
PASS: 0 cite hygiene violations.
```

`REJECTED=0` and `PASS`, on a corpus that contains at least one citation to a
section this audit confirmed does not exist.

## Why this is the thread running through the RFC-0959 sweep

Ten sweeps of the four RFC-0959 documents produced five separate instances of
the same defect — a citation naming something that is not there:

1. `crates/octo-core/src/settlement.rs`, named by the base RFC's §Key Files and
   §Roles
2. `RFC-0862 §Transaction guarantees`, cited by A1's IA-2 and IA-6 as the
   mitigation for partial-state delivery and chain forking
3. `ReceiptReplay`, cited by base TV1 as an error variant the tree explicitly
   documents as never having been one
4. `crates/octo-core/tests/fixtures/cache_key_tv3.json`, base TV3's only
   expected value, in a crate with no `tests` directory
5. `RFC-0105 §DQA Serialization` — **caught and corrected by RFC-0959-v27**

None of the four could have been caught by this linter, and none is in Rust
source, so neither existing mission would have found them either. Instance 5 is
the proof that the class is detectable by hand against the tree; the tool
contributes nothing to it.

## What a resolution check would need

Not proposed as an implementation — this is CI policy and the owner decides.
Recording only what the obstacle is, so the decision is informed:

- The token must carry its target. That means the extraction has to capture the
  `RFC-NNNN` prefix, which changes what every existing rule receives.
- Section names must be matched whole. Multi-word names need the quoted form,
  or a name table built from the target's headings.
- The target has to be located, and RFC numbering is **not** one-to-one — five
  numbers are each claimed by two files, and RFC-0959 has four. Resolution by
  number alone will pick the wrong document.

That last point is the one that makes the check harder than it looks, and it is
the same reason `RFC-0862` needed the path-based disambiguation in RFC-0959's
dependency table.

## What the owner should do

- **Record the depth limit in the script's header,** next to the scope limit it
  already states. A linter that says what it does not check is doing the job the
  existing comments show its author cares about.
- **Decide whether resolution is wanted at all.** It is a different tool from a
  hygiene linter, and the numbering collisions make it a larger piece of work
  than the check itself suggests.
- **If not, rely on v27's discipline** — an author reading their own citations
  against the tree — and point new RFC authors at
  `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not` as
  the worked example.

## Reproduction

```bash
# the extraction captures no RFC number
grep -nEo '§/?[A-Za-z0-9]+([._/][A-Za-z0-9]+)*|§"[^"]+"' scripts/lint_section_cites.sh

# the only four RFC numbers in the script are all in comments
grep -nEo 'RFC-[0-9]+' scripts/lint_section_cites.sh

# the three rules, and what each examines
grep -nE 'Reject [0-9]+:' scripts/lint_section_cites.sh

# the tokenizer drops everything after the first space
printf '%s\n' 'RFC-0862 §Transaction guarantees' \
              'RFC-0959 §Totally Made Up Section' \
  | grep -Eo '§/?[A-Za-z0-9]+([._/][A-Za-z0-9]+)*'

# and the verdict on the real corpus
./scripts/lint_section_cites.sh

# the scope limit the script already documents, for contrast
grep -n 'roughly half the tracked' scripts/lint_section_cites.sh
```

## Cross-references

- Mission `the-section-cite-linter-never-reads-rust-source` — the scope gap, and
  the parent finding this one extends from form to resolution
- Mission `rfc-citations-in-rust-source-name-sections-that-do-not-exist` — 17
  instances found by hand in Rust because the linter never reads Rust; none of
  them would have been caught by depth either
- Mission `rfc-0959-a1-cites-a-transaction-guarantee-section-that-does-not-exist`
  — instance 2, the clearest live example
- Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`
  — instance 5, the one that was caught, by hand
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  this linter cannot see, ten instances, and the reason a resolution check is
  worth considering at all
