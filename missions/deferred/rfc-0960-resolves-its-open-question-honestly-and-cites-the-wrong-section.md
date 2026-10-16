---
name: rfc-0960-resolves-its-open-question-honestly-and-cites-the-wrong-section
description: "RFC-0960 §Open Questions and §Resolved Decisions are the strongest-written part of the document, and the resolution they record is real: the hierarchical vault policy lattice is built in all three of its legs, and the section explains its own earlier deferral as a review note rather than hiding it. Six companion section citations resolve cleanly. Two defects survive that. The WrappedOnly leg is cited as RFC-0965 §3.5, which is AuditWindow; WrappedOnly is §3.7. And both RFC-0960 and RFC-0964 say the Constraint set has 23 variants when their own tables enumerate 25 and the code declares 25. The first is a class no tooling catches: a section citation that resolves but points at the wrong target."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0960
---

# Mission `rfc-0960-resolves-its-open-question-honestly-and-cites-the-wrong-section`

## Status

Deferred. A section reference and a variant count are RFC edits. Nothing was
actioned.

## Finding

This is the finding that credits RFC-0960 where it is strongest, and then finds
two things. Both halves matter, because the section is short.

## What §Resolved Decisions gets right

RFC-0960 carried one genuinely open question — the hierarchical vault policy
lattice — and deferred it with the note "Defer to v1.1 — capability-security
lattice well-studied (KeyKOS, E, Capsicum)". The resolved row then does three
things well:

1. **It reverses the deferral with a reason, not a shrug.** "The 'defer to v1.1'
   annotation was a review-note expressing 'this is well-studied, no urgency' —
   not a missing primitive." That is a reader being told precisely what the
   earlier annotation had meant, and it is the kind of sentence that stops a
   deferral from calcifying into a permanent gap.

2. **It names three concrete mechanisms** rather than asserting coverage. That
   makes the claim falsifiable, which is why this sweep could check it at all.

3. **All three legs are built.** Verified independently:

| Leg as cited                                                      | Implementation                                                                                    |
| ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| RFC-0957 §Attenuation — `parent_capability` chain, child ⊆ parent | `set_subsumes(parent: &[Caveat], child: &[Caveat])`, with `parent_caveat_implies` per caveat kind |
| RFC-0965 — `WrappedOnly` caveat for hierarchical composition      | `Caveat::WrappedOnly { parent_capability }`, with subsumption, serde and a discriminator          |
| RFC-0967 §5 — `PolicyGraph` subgraph relation                     | `is_subgraph` and `is_subgraph_graph` in `octo-policy`                                            |

This is the second time in this audit that a section's self-description has
survived contact with the tree — the first was RFC-0960's sibling v27 sweep. It
is worth saying plainly: **this resolution is not a bookkeeping entry. The
lattice exists.**

The companion citations in the closing line also all resolve. That paragraph
names six sections across RFC-0961, RFC-0962, RFC-0963, RFC-0964, RFC-0965 and
RFC-0967, and none of them appears in the anchor linter's baseline. Checked
against the tool's own data rather than by reimplementing it.

## Defect 1: `WrappedOnly` is cited two sections away

The resolved row attributes the second leg like this:

> RFC-0965 §3.5 adds `WrappedOnly` caveat for hierarchical composition

RFC-0965 §3.5 is `AuditWindow (0x14)`. `WrappedOnly` is **§3.7 (0x16)**. The
cited section exists, is real, and is about something else.

The mechanism the citation names is implemented, so this is not a claim failure.
It is a navigation failure, and it is the interesting kind: **the citation is
perfectly well-formed.** `scripts/lint-rfc-anchors.py` finds heading `3.5` in
RFC-0965, because heading `3.5` exists in RFC-0965. It is satisfied, correctly,
by the linter's own rules. RFC-0965 is named. The section number exists. The
citation is not in the baseline and never will be.

This is adjacent to a recorded mission, and worth distinguishing from it. The
anchor-linter coverage gap is about citations that name the _wrong document_ — a
heading of the right name in the wrong file satisfies them. This is the
remaining case: the citation names the **right** document, the **right** format
and a **real** section number, and is simply the wrong number. No shape-based
check can catch that, because there is nothing malformed to catch. `§3.5` is a
valid reference to §3.5. Only a reader — or a semantic check that compares the
subject named in the sentence against the subject of the section it points to —
can tell.

The distinction matters for what to build. The recorded gap would be fixed by
making the linter check _which document_ a citation names. This one cannot be
fixed that way, and would need the heuristic noted below. A dangling anchor
announces itself; a real-section-wrong-target citation hides behind a green
check, which is the worse of the two.

## Defect 2: "23 variants" contradicts the table that defines them

RFC-0960's Reference Implementation line describes the substrate as
implementing `Constraint` "(23 variants)". RFC-0964 says "23-variant" three
times. The actual set has **25**.

Three independent measurements agree on 25: the enum's variant declarations,
its discriminant arms, and its decode arms. The set is exactly
`0x01`–`0x19`.

The sharpest part is that RFC-0964 contradicts _itself_:

- Its own canonical variant table has **25 rows**, `0x01` through `0x19`.
- Its own v1.2-Resolved version-history entry says "254-variant budget confirmed
  (**25 used** / 229 free)".
- Its prose says **23** in three separate places.

And RFC-0960 repeats the stale 23. So this is not "the count drifted and nobody
noticed" — the correct number is present twice inside the document that states
the wrong one three times.

Reassuringly, the encoding itself is exact. Diffing RFC-0964's table names
against the code enum returns **identical** — 25 names, no difference, and the
discriminants line up. Only the prose count is stale. That is the cheapest
possible kind of correction: one number, three places in one RFC and one in
another.

## Scope note: the lattice is built over caveats, not constraints

The resolved row describes RFC-0957 §Attenuation as "child capability's
caveats/constraints ⊆ parent's". The implementation is
`set_subsumes(parent: &[Caveat], child: &[Caveat])` — **caveats only**.
`Constraint` values are not an input to it.

This is not a contradiction of the resolution, which cites the `parent_capability`
chain specifically. It is worth recording because it connects to the §Security
Considerations sweep in the same RFC: there is no constraint evaluator anywhere
in the tree, so the constraint half of "caveats/constraints ⊆ parent's" has no
mechanism, while the caveat half is genuinely enforced. **The lattice holds for
the type it was built on.**

## What the owner should do

- **Fix `§3.5` → `§3.7`** in RFC-0960's resolved row. One word.
- **Fix the count: 23 → 25**, in RFC-0964's three prose mentions and RFC-0960's
  Reference Implementation line. RFC-0964's table and version history are
  already right, so this only needs the prose brought in line with the document
  it sits in.
- **Consider a semantic citation check, separately from the recorded one.** The
  recorded anchor-linter gap — a citation satisfied by a heading in the wrong
  document — is fixable by checking which document a citation names. This case is
  not, because the citation is well-formed. The only realistic check is
  heuristic: flag a numeric citation whose target heading's subject does not
  appear anywhere near the citing sentence. That will produce false positives and
  is an owner call on tooling scope. **Proposing, not implementing**, and it
  should be considered alongside, not as part of, the existing fix.
- **Leave the rest of §Resolved Decisions alone.** It is the section that got this
  right, and the reversals it performs are the model for the other open questions
  in the corpus.

## Reproduction

```bash
# CONTROL — a section name known to exist
grep -nE '^#+ .*Attenuation' rfcs/accepted/economics/0957-capability-token-format.md

# Defect 1 — the cited section is AuditWindow; WrappedOnly is two sections on
grep -nE '^#### 3\.(5|6|7) ' rfcs/accepted/economics/0965-capability-extension-format.md

# ...and the anchor linter passes it, because §3.5 is a real heading
grep -n '3.5' scripts/lint-rfc-anchors-baseline.txt | grep 0965
# empty — the citation is not even baselined as dangling

# Defect 2 — three independent counts of the enum
awk '/^pub enum Constraint \{/,/^\}/' crates/cipherocto-encoding/src/lib.rs \
  | grep -oE '^    [A-Z][A-Za-z]+' | wc -l
grep -cE '^\s+Self::[A-Za-z]+.*=> 0x' crates/cipherocto-encoding/src/lib.rs
grep -cE '^\s+0x[0-9A-Fa-f]+ =>' crates/cipherocto-encoding/src/lib.rs

# ...against the RFC's own three mentions
grep -c '23-variant' rfcs/accepted/economics/0964-constraint-encoding-standard.md

# ...and the RFC's own table, which is already right
grep -oE '^\| 0x[0-9A-F]+ \| `[A-Za-z]+`' \
  rfcs/accepted/economics/0964-constraint-encoding-standard.md \
  | grep -oE '`[A-Za-z]+`' | tr -d '`' | sort > /tmp/rfc_names.txt
awk '/^pub enum Constraint \{/,/^\}/' crates/cipherocto-encoding/src/lib.rs \
  | grep -oE '^    [A-Z][A-Za-z]+' | tr -d ' ' | sort > /tmp/code_names.txt
diff /tmp/rfc_names.txt /tmp/code_names.txt && echo IDENTICAL

# ...and RFC-0964's version history, which also says 25
grep -o '25 used' rfcs/accepted/economics/0964-constraint-encoding-standard.md

# The three lattice legs, all real
grep -n 'pub fn set_subsumes' crates/octo-cap-macaroon/src/caveat/mod.rs
grep -rn --include=*.rs 'WrappedOnly { parent_capability' \
  crates/octo-cap-macaroon/src/caveat/mod.rs | head -1
grep -rn --include=*.rs 'pub fn is_subgraph' crates/octo-policy/src/lib.rs

# The six companion citations resolve — none is in the baseline
grep -cE '11\.1|15\.1|Resolved Decisions' \
  scripts/lint-rfc-anchors-baseline.txt
```

## Cross-references

- Mission `rfc-0960-security-section-names-a-constraint-evaluator-nobody-built`
  — the same document, and the constraint half of the lattice that has no
  mechanism
- Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`
  — v27's discipline; this section is where it paid off
- Mission `rfc-0960-pins-six-companion-versions-and-every-one-is-behind` — the
  version pins in the §Open Questions table, stale in the same document
- Mission `the-section-cite-linter-checks-token-shape-and-never-resolution` — the
  linter this extends, and the limitation this sits beyond
- Mission `the-anchor-linter-checks-that-a-section-exists-somewhere-not-in-the-rfc-it-cites`
  — the adjacent gap this is explicitly **not** the same as: that one is about
  the wrong document, this one is about the wrong number in the right document
