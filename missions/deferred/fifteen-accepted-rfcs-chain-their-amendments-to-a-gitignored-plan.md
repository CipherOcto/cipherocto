---
name: fifteen-accepted-rfcs-chain-their-amendments-to-a-gitignored-plan
description: "Three gitignored trees are cited by Accepted RFCs: docs/reviews by twelve, docs/plans by fifteen, docs/audits by twenty-three distinct files, with no tracked file in any of them. Most citations disclose the gitignored status explicitly, which is honest and does not restore the information. Fifteen Accepted RFCs name one gitignored rollout plan as the spine of their amendment chain. Twenty-three Accepted RFCs cite gitignored audits as the evidence for accepted claims, including RFC-0206's normative count of thirteen consumer crates and RFC-0015-b's acceptance resting on a plateau declaration. One citation is worse than unreproducible: RFC-0958 states a Stoolap fork branch pin commit hash is recorded in a section of a gitignored plan, that section is the Goal section, and the hashes the file does contain are RFC acceptance commits. No commit-level fork pin exists in the RFC or in the workspace manifest, which pins a branch."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0958
    - RFC-0206
    - RFC-0015-b
    - RFC-0011-h
---

# Mission `fifteen-accepted-rfcs-chain-their-amendments-to-a-gitignored-plan`

## Status

Deferred. All of it is RFC and manifest editing. The Stoolap pin is the one
with a technical consequence and it is recorded separately as the headline
below.

This mission answers the sweep its predecessor proposed at the end of
`two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review`.

## Finding

`docs/reviews/` is not the only excluded tree. `.gitignore` also excludes
`/docs/plans/` and `docs/audits/`, all three with zero tracked files and all
three holding substantial local content — 382, 96 and 193 entries respectively
on this machine.

| Tree            | Gitignored at | Distinct files cited by Accepted RFCs |
| --------------- | ------------- | ------------------------------------- |
| `docs/reviews/` | line 43       | 12 RFCs                               |
| `docs/plans/`   | line 46       | 6 files                               |
| `docs/audits/`  | line 51       | 23 files                              |

## Credit where it is due: the disclosure is nearly universal

This is not a project hiding anything. Fifteen of the RFCs naming a plan say so
in the citation itself:

> **Amendment chain:** Third amendment in the `0011-h-multiphase-rollout-plan`
> (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, **gitignored
> scratchpad per `docs-plans-scratchpad`**).

All fifteen of the RFCs that name that plan carry that parenthetical. RFC-0205
prescribes writing its audit artifacts into `docs/audits/` and says the
directory is created empty and populated on demand. RFC-0959 states the policy
and follows it.

So the practice is understood and applied. The problem is narrower and worth
stating precisely: **disclosure does not restore the information.** A reader who
knows a file is missing still cannot see what was in it.

## Grade A — fifteen Accepted RFCs, one unversioned spine

Fifteen Accepted RFCs name `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`
as the document defining their place in an amendment chain — first through
fifteenth amendments, each RFC naming which one it is and what phases it
carries.

The chain is coherent and honestly labelled. It is also the case where the
number matters most: fifteen Accepted RFCs derive their position in a sequence
from a file outside version control. If the plan is revised or dropped, nothing
in the repository records the new ordering, and no diff shows the change.

## Grade B — accepted claims resting on gitignored evidence

Twenty-three distinct `docs/audits/` files are cited. Most are honest closure
records. Three carry weight a reader would want to check:

- **RFC-0206** (storage split): "13 consumer crates directly use
  `stoolap::Database` (per `docs/audits/octo-storage-trait-surface-2026-08-19.md`)"
  — a normative substrate claim, and the count is the RFC's justification for
  its scope. The same audit is cited as "R9 aggregate driving v2.0 wholesale
  rewrite".
- **RFC-0015-b**: "The plateau declaration
  (`docs/audits/2026-09-14-rfc-0015-0016-plateau-declaration.md` §Deferred
  substrate defects)" — an Accepted RFC whose acceptance rests on a document
  nobody else has.
- **RFC-0968**: its whole adversarial-review trigger, "25 CRITICAL findings",
  lives in five gitignored session plans.

RFC-0012, RFC-0013 and RFC-0014 each open by saying they close a gap
"identified in `docs/audits/2026-09-10-rfc-0011-a-g-phantom-substrate-investigation.md`"
— three Accepted RFCs justified by one untracked file.

## Grade C — a pin that does not exist

This is the one instance that is not merely unreproducible.

RFC-0958 carries:

> **Stoolap fork branch pin:** commit hash recorded in
> `docs/plans/2026-07-19-session-05-zk-capability-circuit.md` §1

Three things are wrong:

**The file is gitignored.** Zero tracked files under `docs/plans/`.

**§1 is the Goal section.** The plan's headings are `§0 BLUEPRINT Workflow Gate`,
`§1 Goal`, `§2 Decisions Locked`, `§3 Steps`, `§4 Validation`, `§5 Exit Criteria`,
`§6 Out-of-Scope`, `§7 Risks`. §1 is prose about landing the circuit. It
contains no hash.

**The file records no fork commit at all.** Its only hex tokens are the
acceptance commits of the RFCs it depends on — `b1bf0ed7` for RFC-0957,
`e4e0c7a3` for RFC-0102, `36fc8155` for RFC-0009. None is a Stoolap fork pin.

And nothing else supplies one. The workspace manifest pins a **branch**:

```toml
stoolap = { git = "https://github.com/CipherOcto/stoolap", branch = "feat/blockchain-sql" }
```

directly beneath a comment reading "Pinned commit must match the ask-settlement
migration expectations". A branch is not a commit. RFC-0958 names
`feat/blockchain-sql` in five places and states the hash lives in a plan
section that does not contain it.

**So no commit-level pin of the Stoolap fork exists anywhere in the repository.**
For a proof-systems RFC whose substrate is that fork — CASM compilation, STWO
vendoring, a proof verifiable at any router — the substrate can move under the
specification with no RFC amendment and no diff anywhere in the tree to show it.

That is a different class of finding from the other twenty-nine citations. The
others say "trust this document you cannot read". This one says "trust a pin
that was never written down".

## What the owner should do

- **Write the Stoolap fork commit into RFC-0958 and pin the manifest to it.**
  A `rev =` alongside `branch =` is one line, and it makes the substrate claim
  checkable. Delete or correct the sentence pointing at the plan §1.
- **Bring the 0011-h rollout plan's phase table into a tracked file** — or into
  RFC-0011-h itself. Fifteen Accepted RFCs point at it for their ordering.
- **Inline the three load-bearing `docs/audits/` claims** in RFC-0206, RFC-0015-b
  and RFC-0968: the crate count, the plateau rationale, and the finding counts.
  Each is a paragraph of text that currently lives nowhere else.
- **Consider a tracked summary per audit.** The pattern that works is already in
  RFC-0959 and the RFC-0012/0013/0014 family — conclusion in the RFC, working in
  the scratchpad. Applying it to the grades above closes most of the gap without
  committing anything ephemeral.
- **Leave the disclosed citations alone.** Fifteen RFCs saying "gitignored
  scratchpad" in the citation are doing the right thing; the fix is to stop
  needing the file, not to stop mentioning it.

## Reproduction

```bash
# three excluded trees, none with a tracked file
# (note: /docs/plans/ is written with a leading slash, the other two without)
grep -nE '^/?docs/(reviews|plans|audits)/$' .gitignore
for d in docs/reviews docs/plans docs/audits; do
  printf '%-14s tracked=%s\n' "$d" "$(git ls-files "$d" | wc -l)"
done

# the fifteen-RFC amendment spine, and the disclosure in all fifteen
grep -rl '0011-h-multiphase-rollout-plan' --include=*.md rfcs/accepted/ | wc -l
grep -rl '0011-h-multiphase-rollout-plan' --include=*.md rfcs/accepted/ \
  | xargs grep -c 'gitignored scratchpad' | sort -u

# evidence resting on gitignored audits
grep -rhoE 'docs/audits/[A-Za-z0-9._-]+\.md' --include=*.md rfcs/accepted/ \
  | sort -u | wc -l
grep -n '13 consumer crates' rfcs/accepted/storage/0206-octo-storage-split.md | cut -c1-200
grep -n 'plateau declaration' rfcs/accepted/process/0015-b-substrate-defects.md | cut -c1-200

# Grade C: the pin, the section, and the hashes the file actually holds
grep -n 'Stoolap fork branch pin' rfcs/accepted/proof-systems/0958-zk-capability-subclass.md
grep -n '^## ' docs/plans/2026-07-19-session-05-zk-capability-circuit.md | head -3
grep -oE '.{50}[0-9a-f]{7,40}.{20}' docs/plans/2026-07-19-session-05-zk-capability-circuit.md | head -3

# and the branch that is all the manifest pins — no rev anywhere
grep -n 'CipherOcto/stoolap' Cargo.toml
grep -c 'rev =' Cargo.toml          # 0: every stoolap dep is branch-pinned

# no commit-shaped reference anywhere in RFC-0958.
# (a bare hex grep is not good enough: [0-9a-f]{7,40} also matches
# "formance", "features" and "fix", which is why the count it returns
# is misleading)
grep -niE 'commit [0-9a-f]{7}|rev ?= ?\"[0-9a-f]{7}|pinned at [0-9a-f]{7}' \
  rfcs/accepted/proof-systems/0958-zk-capability-subclass.md || echo "NONE"
```

## Cross-references

- Mission `two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review`
  — the `docs/reviews/` half, and the sweep instruction this mission answers
- Mission `rfc-0900-anchors-its-substrate-specification-to-a-gitignored-scratchpad`
  — RFC-0900's Grade 3 anchor into the same class, on the review tree
- Mission `the-section-cite-linter-checks-token-shape-and-never-resolution` —
  the _hygiene_ linter, which validates token shape and does not resolve.
  **Correction to an earlier draft of this line:** I wrote that there was no
  mechanism for this. That was wrong. `scripts/lint-rfc-anchors.py` resolves
  anchors and its `--corpus` mode counts the ones that resolve only in
  untracked files — 548 named and 556 bare-number, measured
- Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`
  — v27 discloses what is unconfirmed, which is the practice whose absence here
  produced Grade C
