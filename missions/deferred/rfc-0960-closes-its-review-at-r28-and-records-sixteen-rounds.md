---
name: rfc-0960-closes-its-review-at-r28-and-records-sixteen-rounds
description: "RFC-0960's Version History states that R1 through R28 multi-round adversarial review closed with R28 a clean round of zero actionable defects, which is the statement that justified promoting the RFC to Accepted. The document records sixteen round sections, R1 through R16. Rounds R17 through R28 have no sections anywhere in the corpus, so the clean-round claim that closed the review cannot be inspected. The implementation mission repeats the R1 to R28 figure independently, so the rounds are attested by two sources and recorded in neither. The same document's Reference Implementation section names a companion crate cipherocto-vault that does not exist, appears exactly once in the whole RFC corpus, and is not mentioned by the grand-design document the implementation mission actually delivered, because that mission is scoped to navigation only."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0960
    - RFC-0964
---

# Mission `rfc-0960-closes-its-review-at-r28-and-records-sixteen-rounds`

## Status

Deferred. Recording twelve missing review rounds is the RFC author's history to
supply, and a companion-crate promise is a scope question for whoever owns
RFC-0960's implementation. Nothing was actioned.

## Finding

RFC-0960's Version History records the promotion:

> **Promoted Draft → Accepted.** R1-R28 multi-round adversarial review closed
> with R28 clean round (**zero actionable defects**). Six companion RFCs
> (0961/0962/0963/0964/0965/0967) promoted in lockstep on the same date.

**The document contains sixteen round sections, R1 through R16.** R17 through
R28 are not there, and a search for an `R17`-to-`R28` heading anywhere in
`rfcs/` returns nothing.

So the sentence that justified promotion — _R28 was a clean round, zero
actionable defects_ — cannot be inspected by anyone. It is the load-bearing
claim of the RFC's acceptance, and it terminates in a round that was never
written down.

## Attested twice, recorded nowhere

The implementation mission `0960-a-grand-design-reference` states the same
figure independently, in its BLUEPRINT gate note:

> RFC-0960 reached Accepted v2.0 on 2026-07-23 (**R1-R28** multi-round review
> closed; six companion RFCs promoted in lockstep).

Two independent documents agree that twenty-eight rounds happened. Neither
contains rounds seventeen through twenty-eight. Whatever else is true, the
review log is incomplete by twelve rounds, and the missing twelve are the ones
that produced the final verdict.

The rounds that _are_ referenced beyond R16 are R17 (the "strategic reframe
(R17+)" that inverted the architecture around the deterministic WAL, cited in
the Version History and again at the head of §Specification), R25 (eleven error
codes, covered in
`a-third-of-rfc-0960s-error-code-registry-traces-to-a-round-that-does-not-exist`),
and R28 (the clean round, and the risk-closure round that resolved the last
Open Question). **R17 is the significant one** — it is the reframe that made the
WAL the primary protocol primitive, and the specification section opens by
describing it while the round that decided it is not recorded.

## §Reference Implementation names a crate that does not exist

> **Reference Implementation**
> Companion crate: `cipherocto-vault` — implements `Vault`, `Capability`,
> `Reservation`, `Settlement`, `Constraint` (23 variants), and the audit window
> state machine.

There is no `cipherocto-vault` in the tree. Four vault crates exist —
`octo-vault`, `octo-vault-storage`, `octo-cap-macaroon-vault`,
`octo-cap-macaroon-vault-storage` — none of which is that crate, and none of
which implements the six listed primitives.

What makes this worth more than a stale name: **`cipherocto-*` is a real naming
convention here.** `cipherocto-encoding`, `cipherocto-fs-perms` and
`cipherocto-zkp-canonical` are all real crates. So the RFC names an artifact in
the project's own convention that was simply never built — which is harder to
spot than a misspelling, and will read as real to anyone skimming.

`cipherocto-vault` appears **exactly once in the entire RFC corpus**: here.

## Why it was never built is the more useful part

The mission that implements RFC-0960 exists, and its deliverable landed:
`docs/architecture/grand-design.md`, 31KB. But it is scoped to navigation:

> The doc is **navigation only** — every section points to the canonical
> RFC-0960 §1–§18 for normative spec. Re-specification is rejected per
> BLUEPRINT.md "Use Cases = intent, RFCs = design, Missions = execution"
> separation.

And that delivered document mentions `cipherocto-vault` **zero times**.

So the RFC promises a companion crate; the mission scoped to implement it
declined to write code; and the document that was delivered does not carry the
promise forward. That is a scope mismatch between a specification and its
implementation mission, not a build failure — and it is invisible from either
side alone. The RFC asserts a reference implementation, the mission delivers a
map, and no document reconciles them.

## Two hypotheses of mine that did not reproduce

Recorded because both would have made clean-sounding findings.

**The 23-vs-25 constraint count is not a drift.** The implementing mission's
retro-note says "mission spec §3 says 23 variants; doc §3 says 25 variants"
and calls it a count drift with the doc as ground truth. But RFC-0964 — which
the delivered document itself names as canonical for variant enumeration — says:

> Currently **25 used (23 constraints + 2 reserved)**

So 25 is the discriminator count including two reserved slots, and 23 is the
constraint count excluding them. Both numbers are correct under their own
convention. The delivered document's "Constraint Set (25 variants)" heading is
the looser phrasing of the two, not a correction of a wrong one.

**The primitives are not all missing.** `Reservation` is in
`octo-settlement-core` and `Constraint` is in `cipherocto-encoding`. They are
real and implemented — just not in the crate the RFC names, and not in
`octo-vault`, which is a balance-projection and vault-operations crate. My
first pass searched only `octo-vault` and would have reported them absent.

## What the owner should do

- **Supply R17–R28, or state the real round count.** R17 is the priority: it is
  the reframe the whole specification is built on. If the rounds exist in a
  session log, the R-round section should point at it the way §Test Strategy
  points at its fixtures.
- **Reconcile §Reference Implementation with what was actually built.** Either
  name the crates that carry the primitives (`octo-settlement-core`,
  `cipherocto-encoding`, `octo-vault`), or mark the section as aspirational.
  As written it is the only place in the corpus that promises a
  `cipherocto-vault`, and the delivered document does not carry it.
- **Make the R-round count in the Version History match the log.** "R1-R28" is
  repeated in the RFC, in the implementation mission's gate note, and in
  v2.0-Accepted. Three copies of a figure the document does not support.

## Reproduction

```bash
R=rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md

# the promotion claim
grep -n 'R1-R28' "$R"

# the rounds that exist
grep -oE '^## R[0-9]+' "$R" | grep -oE '[0-9]+' | sort -n | tr '\n' ' '

# the rounds that do not
grep -rE '^#+ R(1[7-9]|2[0-8])\b' --include='*.md' rfcs/ || echo "no R17-R28 anywhere"
grep -oE '\bR(1[7-9]|2[0-8])\b' "$R" | sort -u | tr '\n' ' '

# the second attestation, from the implementation mission
grep -n 'R1-R28' missions/claimed/0960-a-grand-design-reference.md | cut -c1-160

# the crate that does not exist, named once
grep -rn 'cipherocto-vault' --include='*.md' rfcs/
ls crates | grep '^cipherocto'
grep -c 'cipherocto-vault' docs/architecture/grand-design.md

# and what the mission was scoped to do
grep -n 'navigation only' missions/claimed/0960-a-grand-design-reference.md | cut -c1-160

# the count difference that is not a difference
grep -n 'Constraint Set (25 variants)' docs/architecture/grand-design.md
grep -n '23 constraints + 2 reserved' rfcs/accepted/economics/0964-constraint-encoding-standard.md
```

## Cross-references

- Mission `a-third-of-rfc-0960s-error-code-registry-traces-to-a-round-that-does-not-exist`
  — R25 is one instance of this gap; that mission is the narrow version of this
  one
- Mission `four-of-rfc-0960s-sixteen-self-review-rounds-re-found-one-unpropagated-change`
  — the sixteen recorded rounds and what R12–R15 show about them; read with
  this, the log is incomplete rather than merely repetitive
- Mission `two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review`
  — RFC-0960 in Grade 3 there, so the §20.3 anchor and this missing log are the
  same gap in different places: what RFC-0960 needs is not in the repository
- Mission `rfc-0959-claims-a-determinism-verification-that-does-not-exist` —
  the same shape at document level in a different RFC
