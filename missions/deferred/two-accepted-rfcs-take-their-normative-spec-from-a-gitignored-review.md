---
name: two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review
description: "Twelve Accepted RFCs cite files under docs/reviews, which .gitignore excludes in its entirety and which BLUEPRINT mandates be ephemeral and uncommitted. Usage falls into three grades. Five use it correctly, stating that review artifacts are ephemeral and that the summary lands in the RFC's Version History. Five use it as the place the audit trail lives, so a reader cannot reproduce the findings behind an accepted allocation. Two take the normative specification from it: RFC-0960, the grand-design vault document, cites the storage-layer restructuring review for the 20.3 lattice and calls that review the amendment driver, and RFC-0900 anchors its slash ledger substrate to section 20.3 of the same file. The lattice both are pointing at is already recorded normatively in a tracked migration, octo-vault's v013, which declares chain_id NOT NULL and PRIMARY KEY of chain_id, owner_did and asset_id, so the fix is to cite that instead. RFC-0861 additionally carries three relative markdown links into ../reviews, a directory that does not exist in the repository."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0960
    - RFC-0900
    - RFC-0855p-b
    - RFC-0861
---

# Mission `two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review`

## Status

Deferred. Re-anchoring two Accepted RFCs is an RFC edit, and the slash-ledger
primary-key question is already recorded separately. Nothing was actioned and no
file was moved.

## Finding

`docs/reviews/` is excluded from version control outright — `.gitignore` line
43 — and BLUEPRINT is explicit that this is intended:

> **Review files go in `docs/reviews/`** — ephemeral scratchpads, NOT committed
> to git

Twelve Accepted RFCs cite into that directory anyway. Not all of that is wrong,
and the distinction matters for triage, so the usage is graded.

## Grade 1 — correct, and the pattern to copy

Five Accepted RFCs state the policy and then follow it:

> Review files go in `docs/reviews/` (ephemeral, not committed). Final summary
> lands in §Version History. — RFC-0959

> Review artifacts in `docs/reviews/0012-audit-substrate/` (gitignored); summary
> in §Version History — RFC-0012, and the same shape in RFC-0013 and RFC-0014

> Round-1 adversarial review notes are filed under `docs/reviews/rfc-0871/`
> (not committed per `docs-reviews-temporary`) — RFC-0871

The RFC carries the conclusion and the review carries the working. Nothing
depends on a file nobody else has.

## Grade 2 — the audit trail is not reproducible

Five Accepted RFCs point a reader at gitignored space for detail they need:

- **RFC-0855p-b** cites `docs/reviews/r16/r16-r1-adversarial-review.md §2` as
  **"the canonical mapping"** for the slash reason code space, three times,
  including in its Version History.
- **RFC-0855p-c** cites the same review as the rationale for reassigning
  `0x000F` to `0x0100 = DomainCoordinatorMisbehavior` — a security-relevant
  allocation whose authority is a file outside the repository.
- **RFC-0850p-c** and **RFC-0850p-e** cite the same file for the same table.
- **RFC-0862** documents 60 findings across 12 rounds as living in
  `docs/reviews/rfc-0862-adversarial-review-r{1..12}.md`.
- **RFC-0973** records 24 rounds and roughly 300 findings, with per-round
  detail archived in the same place.

These RFCs do summarise their conclusions in the body, so a reader is not
blocked. But the supporting evidence cannot be checked, and for the slash-code
family the _canonical mapping_ is defined by reference to it.

## Grade 3 — the specification itself lives there

Two RFCs do not merely cite a review for detail. They take the normative
substrate specification from a gitignored file.

**RFC-0960**, the grand-design document for vaults, capabilities and
reservations (one of four files carrying that number; the citation is in the
grand-design base):

> §20.3 lattice (review): `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`
> §20.3 + §20.7 + §20.8 + §9.5
>
> `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md` — §9.5
> `parent_vault_id` removal + §20.3 Model B lattice + §20.7 intra-chain transfer
> semantics + §20.8 no-organizational-intermediates lock (**v3.0 amendment
> driver**)

A grand-design RFC names a gitignored scratchpad as the document that drove its
amendment, and as the source of four normative subsections.

**RFC-0900** anchors its slash ledger substrate to §20.3 of the same file, twice
— worked through in
`rfc-0900-anchors-its-substrate-specification-to-a-gitignored-scratchpad`.

So it is one review scratchpad carrying the substrate specification for two
Accepted RFCs, one of them a parent document.

## The fix is already sitting in the repository

The lattice both RFCs cite is recorded normatively, in a tracked file:

```sql
-- crates/octo-vault/migrations/v013__create_vaults.sql
chain_id        BLOB(32) NOT NULL,
...
PRIMARY KEY (chain_id, owner_did, asset_id)
```

That is exactly the §20.3 Model B shape — `chain_id` **NOT NULL**, a real
`PRIMARY KEY`, not a unique index. It is in git, it is the artifact that
actually creates the vault table, and a reader can check it.

So re-anchoring is not a research task. RFC-0960 and RFC-0900 can cite
`v013__create_vaults.sql`, or restate the lattice inline as RFC-0900 already
does for the `ChainId` type and the amount columns.

This also sharpens the slash-ledger question. The vault migration is the
pattern, and it uses `NOT NULL` plus a `PRIMARY KEY`. The slash ledger's v015
delivers a nullable column inside a UNIQUE index. **The RFC cites the review;
the migration it should have copied is in the tree and is stricter.**

## Three broken links, separately

RFC-0861 carries relative markdown links:

```markdown
[`docs/reviews/coordinator-admin-impl-adversarial-review-r1.md`](../reviews/coordinator-admin-impl-adversarial-review-r1.md)
```

Three occurrences, naming two files — `coordinator-admin-impl-adversarial-review-r1.md`
twice and `...-r5.md` once, described as "source of all 17 findings" and as the
"R5 closure summary".

`../reviews/` from `rfcs/accepted/networking/` resolves to
`rfcs/accepted/networking/reviews/`, which does not exist. Every one renders
broken on any clone and on any hosted view — a different defect from the
citation one, and one that a link checker would catch if one ran.

## What the owner should do

- **Re-anchor RFC-0960 and RFC-0900 to `v013__create_vaults.sql`** or to an
  inline statement of the lattice. Both are one-line edits against a file that
  is already in the repository.
- **Decide whether a gitignored file may be normative at all.** The BLUEPRINT
  rule exists for a reason; these two RFCs are the cases where it was set aside
  without a recorded exception. Either permit it explicitly and say where the
  review lands, or forbid it and fix the two.
- **Fold the Grade 2 slash-code rationale into the RFCs' own bodies.** The
  `0x000F → 0x0100` reassignment in particular should say why in the document
  that carries the constant.
- **Fix RFC-0861's three relative links** — two of them name the same file —
  point them at tracked paths, or render them as plain text naming the ephemeral
  file.
- **Sweep for `docs/plans/` and `docs/audits/`,** which `.gitignore` excludes on
  the same two lines nearby. The RFC-0900 case suggests review scratchpads are a
  natural source of citations and the corpus has not been checked for the other
  two.

## Reproduction

```bash
# the exclusion and the policy behind it
grep -n '^docs/reviews/' .gitignore
git check-ignore -v docs/reviews/r16/r16-r1-adversarial-review.md
git ls-files docs/reviews/ | wc -l            # 0
grep -n 'Review files go in' docs/BLUEPRINT.md

# every Accepted RFC that cites into it
grep -rl 'docs/reviews' --include=*.md rfcs/accepted/ | sort

# grade 3: normative use
grep -n 'docs/reviews' rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md \
  | cut -c1-200
grep -n 'docs/reviews' rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md | cut -c1-200

# grade 2: canonical mapping resting on an untracked file
grep -c 'canonical mapping' rfcs/accepted/networking/0855p-b-coordinator-lifecycle.md

# the target, and the lattice the reviews are cited for
grep -rnE '^#+ *20\.3' --include='*.md' docs/reviews/

# already recorded normatively, in a tracked migration
grep -nE 'chain_id.*NOT NULL|PRIMARY KEY' crates/octo-vault/migrations/v013__create_vaults.sql

# and the broken relative links
grep -n '(\.\./reviews/' rfcs/accepted/networking/0861-coordinator-admin-trait-refinements.md
ls rfcs/accepted/networking/reviews/ 2>/dev/null || echo "target directory absent"
```

## Cross-references

- Mission `rfc-0900-anchors-its-substrate-specification-to-a-gitignored-scratchpad`
  — the RFC-0900 half, in detail; this mission shows it is a corpus pattern and
  not a one-off
- Mission `the-section-cite-linter-checks-token-shape-and-never-resolution` —
  the class. A linter that resolved cites would find none of this either, since
  every instance points at a real file that simply is not in the repository
- Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`
  — v27's discipline, and the reason RFC-0959 appears in Grade 1 rather than
  Grade 3
- Mission `rfc-0900-specifies-a-listing-lifecycle-that-does-not-exist-anywhere`
  — the same RFC-0900 sweep, whose praise of the slash ledger section is
  qualified by the Grade 3 anchor
