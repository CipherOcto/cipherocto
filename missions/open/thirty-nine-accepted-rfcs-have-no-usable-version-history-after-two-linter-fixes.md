---
name: thirty-nine-accepted-rfcs-have-no-usable-version-history-after-two-linter-fixes
description: "lint_no_vh_accepted.sh reported PASS while examining 7 of 155 Accepted RFCs, because its detection regex required Accepted on the same line as the status heading and the corpus writes it on the following lines. Its CHECKED counter incremented once per markdown file rather than per Accepted RFC, so the summary read CHECKED=247 against PASS=7 and the gap looked like unexamined work rather than a silent skip. After the detection fix 51 Accepted RFCs were reported, and after a second fix recognising numbered Version History headings the real figure is 39: 6 have no Version History section at all and 33 have a section with no data rows."
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0001
---

# Mission `thirty-nine-accepted-rfcs-have-no-usable-version-history-after-two-linter-fixes`

## Status

Open. Both detection defects are fixed. The 39 findings are recorded here and
**not** resolved — fixing an Accepted RFC's Version History means writing
history for it, which is author work, not mechanical repair.

## The linter that should have caught this

`scripts/lint_no_vh_accepted.sh` is Guard 4, and its own header says
`BLOCKING promotion: YES`. It ran on this tree and printed:

```
Summary: CHECKED=247 (Accepted RFCs scanned)
         PASS=7 FAIL=0 MISSING_VH=0
PASS: all Accepted RFCs have ## Version History.
```

`PASS=7 + FAIL=0 + MISSING_VH=0` is 7. The summary claimed 247 were scanned.
The 240-file gap was the tell, and it was invisible in the output because the
two counters counted different populations.

There are **155** Accepted RFCs in the tree. The linter examined **7**.

## Why it examined 7

```bash
grep -qE '^## (Status|## Status|Status:).*Accepted' "$rfc_file"
```

`Accepted` must appear on the same line as the heading. The corpus writes:

```markdown
## Status

Accepted (v2.0) — promoted 2026-08-20 by user instruction ...
```

So the match failed for 148 of 155 files, they hit `[ "$is_accepted" -eq 0 ] &&
continue`, and were never examined. The linter reported a clean pass over a
population it had mostly skipped.

The second counter is what made it invisible. `CHECKED` incremented once per
markdown file _found_, before any filtering, while `PASS` incremented only for
detected Accepted RFCs. A skip therefore widened the gap between two numbers
that are printed together, and neither number said how many were eligible.

Fixed. The linter now reads the Status block rather than the heading line, and
separates `SCANNED` (files found) from `CHECKED` (Accepted RFCs examined). It
now reports `SCANNED=247 markdown files, CHECKED=155 Accepted RFCs`.

## The 39 findings

| Class                                                  | Count | Meaning                                            |
| ------------------------------------------------------ | ----- | -------------------------------------------------- |
| `MISSING_VH` — no Version History or Changelog section | 6     | the section is absent entirely                     |
| `MISSING_VH_ROWS` — section present, no data rows      | 33    | the heading exists and the table under it is empty |

`PASS=116`.

**Correction: the first row was 18, not 6.** Twelve of those eighteen carry a
fully populated Version History under a **numbered** heading — `## 6. Version
History` — and the linter required the heading to be unnumbered. They were
reported as having no Version History at all.

Measured across the whole tree before changing anything: allowing an optional
section number recognizes 14 files and loses none, so the change is strictly
additive. Verified in both directions by mutation — adding a numbered Version
History to a file that lacked one is now accepted, and deleting a populated
one is still reported.

Six rather than eighteen is the real size of this problem. Half of what looked
like missing history was a linter that could not read a numbered heading.

### 6 with no Version History section

```
draft/process/0001-mission-lifecycle.md
accepted/process/0002-agent-manifest.md
accepted/process/0011-s-oct-cli-network-phase-11.md
accepted/process/0011-t-oct-cli-network-phase-12.md
accepted/process/0011-u-oct-cli-network-phase-13.md
accepted/process/0011-v-oct-cli-network-phase-14.md
```

`0001-mission-lifecycle.md` is a draft that declares itself Accepted in its
Status block. That is a status/placement inconsistency in its own right and is
called out rather than folded in.

The remaining twelve from the earlier list of eighteen carry a populated
Version History under a numbered heading and are no longer findings:

```
accepted/process/0010-v17-chain-id-registration-authority.md
accepted/process/0206-v30-value-transfer-surface.md
accepted/economics/0105-v34-private-asset-namespace.md
accepted/economics/0105-v35-payment-caveat-asset-generality.md
accepted/economics/0903-d1-litellm-persistence.md
accepted/economics/0959-v27-burn-event-wire-form.md
accepted/economics/0959-v28-settlement-cost-dqa-migration.md
accepted/economics/0960-v35-vault-path-taxonomy.md
accepted/economics/0960-v36-burn-event-dqa-migration.md
accepted/economics/0960-v37-vault-balance-projection-substrate.md
accepted/economics/0965-v21-payment-caveat-asset-binding.md
accepted/economics/0967-a1-policy-registry.md
```

### 33 with an empty Version History

A representative sample, all under `rfcs/accepted/`:

```
process/0011-k-oct-cli-network-phase-3.md
process/0011-m-oct-cli-network-phase-5.md
process/0011-p-oct-cli-network-phase-8.md
process/0012-v3-audit-substrate-amendment.md
process/0014-v2-settlement-substrate-amendment.md
process/0015-a-wallet-agent-write-path.md
process/0015-wallet-agent-operations.md
process/0016-a-audit-receipt-write-path.md
```

The full list is the current output of `scripts/lint_no_vh_accepted.sh`; it is
not duplicated here so it cannot drift.

## The third defect: a section sign in the heading

Two files write the heading as `## §Version History`. The pattern
`^## (Version History|Changelog)$` did not allow a leading `§`, so both were
reported as having no Version History when they have populated ones.

This is the same shape as the citation work in
`scripts/validate_cites.sh`: this repository writes `§section_name`, and
patterns written against a bare `section_name` silently miss the `§` form.

The first attempt at the fix used `§? *`, which cannot match a plain heading —
the star repeats the space, and the sign is optional, so zero repetitions means
zero spaces. It collapsed `PASS` from 104 to 2. Explicit alternation is correct.

## Acceptance criteria

- [ ] Each of the 6 files gains a Version History section, or a decision is
      recorded for why an Accepted RFC does not need one.
- [ ] Each of the 33 files gains at least one data row, or is exempted.
- [ ] `0001-mission-lifecycle.md` is resolved: either promoted out of `draft/`
      or its Status block corrected.
- [ ] Any exemption is expressed as a mechanism the linter honours, not as a
      deletion of the check.
- [ ] If the linter is wired, it reports zero on the tree at the moment of
      adoption.

## Notes

Verification of the linter fix was done by mutation, both directions:

- reverting the detection regex drops findings from 51 to 12, which accounts
  for the difference exactly
- deleting every Version History data row from a known Accepted RFC is still
  detected as `MISSING_VH_ROWS`

The first positive control replaced a single data row rather than removing all
of them, so it correctly did not fire. That was a bad control, not a bad fix;
it was rewritten to remove every numeric row, and then fired.

This is the third instance in this repository of a gate whose counters
disagreed with each other and whose output looked clean because the
disagreement was arithmetic rather than visible:

| Script                      | Symptom                                       |
| --------------------------- | --------------------------------------------- |
| `lint_no_vh_accepted.sh`    | CHECKED counted files, PASS counted eligible  |
| `mutation-verify-guards.sh` | an unmatched filter yields `passed=0` quietly |
| `lint_status_vh_sync.sh`    | newest version read as the first row          |

A counter that does not match the population it describes is the failure mode
worth hunting for next.
