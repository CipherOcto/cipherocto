---
name: a-third-of-rfc-0960s-error-code-registry-traces-to-a-round-that-does-not-exist
description: "RFC-0960's Central Error Code Registry lists 34 codes with a Defined in column whose stated purpose is to let reviewers trace provenance. Eleven of those 34 cite R25, and no R17 through R25 review round exists anywhere in the RFC corpus, so a third of the registry cannot be traced to anything. The registry's naming convention is also absent from the codebase: zero of its 35 code strings appear in any Rust file, so a reviewer consulting it for cross-RFC discoverability would grep for E_REPLAY_DETECTED and find nothing where the code says ReplayDetected. Under the naming the code actually uses, four sampled concepts are present and nine are not found. The table header scopes itself to RFC-0960 through RFC-0965 while two of its rows belong to RFC-0967."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.1"
  depends_on:
    - RFC-0960
    - RFC-0967
---

# Mission `a-third-of-rfc-0960s-error-code-registry-traces-to-a-round-that-does-not-exist`

## Status

Deferred. Renaming 34 error codes across six accepted RFCs is a breaking
change to a cross-RFC surface, and whether R17–R25 exist somewhere unrecorded
is a question only the RFC author can answer. Nothing was actioned.

## Finding

RFC-0960 §Central Error Code Registry lists every error code the RFC-0960 stack
emits, "for cross-RFC discoverability", with a provenance column:

> The "Defined in" column links each code to the R-round that introduced it, so
> reviewers can trace provenance.

Thirty-four rows. **Eleven of them cite `R25`.**

The document has review rounds R1 through R16. There is no R17, R18, … R25.
Nor is there one anywhere else in `rfcs/` — a search for an R17-to-R25 heading
across the whole corpus returns nothing, and the string `R25` appears in this
file _only_ inside those eleven table cells.

So for a third of the registry, the traceability the section exists to provide
terminates in a round that was never recorded.

## Wider than this table

The R25 citations are the visible part of a larger gap. RFC-0960's Version
History states that **R1 through R28** multi-round review closed with R28 a
clean round, and the implementation mission repeats that figure — but only
sixteen round sections exist. R17 through R28 are recorded nowhere, so the
clean-round claim that justified promotion cannot be inspected either.

Worked through in `rfc-0960-closes-its-review-at-r28-and-records-sixteen-rounds`,
which also covers §Reference Implementation naming a companion crate that does
not exist. This mission stays scoped to the registry.

## The eleven

`E_GAS_LIMIT_EXCEEDED`, `E_BRANCH_NOT_FOUND`, `E_MERGE_CONFLICT_UNRESOLVED`,
`E_MERGE_SIGNATURE_MISSING`, `E_MV_STATE_HASH_MISMATCH`,
`E_AS_OF_QUERY_FAILED`, `E_DETERMINISTIC_PROFILE_NO_ORDER_BY`,
`E_DETERMINISTIC_PROFILE_INVALID_COLLATE`, `E_POLICY_NOT_FOUND`,
`E_POLICY_ATTENUATION_INVALID`, `E_DDL_NOT_ACTIVATED`.

They are also the **newest** codes in the table — the ones carrying no
"(renamed from …)" note, drawn from RFC-0960 §14/§15/§17/§18, RFC-0967 §8 and
RFC-0964 §3.11. Every other row traces to a round the document contains (R3,
R4-F9, R7-F1, R7-F5, R8-F1, R8-F3, R8-F5).

The pattern is consistent with rounds R17–R25 having happened and never having
been written down — the same sixteen-round review process described in
`four-of-rfc-0960s-sixteen-self-review-rounds-re-found-one-unpropagated-change`,
extended without extending the log. Whether that is what happened is the RFC
author's to say; what is checkable is that the provenance is currently
unresolvable for those eleven.

## The naming convention is not the one in use

The registry's codes are `E_SCREAMING_SNAKE`. **None of the 35 distinct strings
appears in any `.rs` file in the tree.**

That is not a harmless convention difference, because the section states its
purpose as cross-RFC discoverability and closes:

> Reviewers should consult this registry when implementing error handling
> across RFC boundaries.

A reviewer following that instruction greps for `E_REPLAY_DETECTED` and finds
nothing. The code says `ReplayDetected`. Following the registry's own guidance
leads away from the implementation rather than to it.

Under the naming the code actually uses, the picture is mixed rather than
absent. Sampling thirteen of the registry's concepts:

| Present in Rust      | Files |
| -------------------- | ----- |
| `ReplayDetected`     | 23    |
| `SignatureInvalid`   | 16    |
| `PolicyNotFound`     | 5     |
| `ChainDepthExceeded` | 1     |

| Not found under either name                                     | —   |
| --------------------------------------------------------------- | --- |
| `CapabilityRevoked`, `CapabilityExpired`, `CapabilityExhausted` | 0   |
| `GasLimitExceeded`, `BranchNotFound`, `AsOfQueryFailed`         | 0   |
| `DdlNotActivated`, `WalSegmentMismatch`, `LocalChainForked`     | 0   |

The right-hand column is stated as what it is: **not found under the registry
name or its obvious transliteration.** That is not proof the behaviour is
absent — an error may be named something less literal (`CapRevoked`), or the
condition may be prevented rather than reported. It does mean the registry
cannot be used to check the implementation, which is the only thing a reviewer
would want it for.

## Scope mismatch in the header

The heading reads:

> ## Central Error Code Registry (RFC-0960/0961/0962/0963/0964/0965)

and two rows — `E_POLICY_NOT_FOUND` and `E_POLICY_ATTENUATION_INVALID` — name
`RFC-0967 §8`. Small, but it means the header's scope is narrower than the
table's contents, in the one section whose job is to be authoritative about
what is where.

## An error of mine, recorded

My first pass at the implementation half reported that **zero** of the
concepts existed, because a `cd` in my command left the shell in a stray
`/home/mmacedoeu/_w/cipherocto` directory that contains only an empty `crates`
folder. Every count in that pass was wrong — `ReplayDetected`, which I had read
out of `market_delivery.rs` earlier in this same audit, came back as 0 files.

Caught by cross-checking a known-present symbol against the search rather than
trusting the search. It is the third search-error this audit has recorded, after
a `head`-truncated grep and a `crates/`-only grep, and all three produced the
same shape: **a negative result from a search that did not run where I thought
it did.**

The corrected numbers are the ones above.

## What the owner should do

- **Either record R17–R25 or re-point those eleven rows.** A provenance column
  that is unresolvable for a third of its entries is worse than no column,
  because it invites a reviewer to trust a link.
- **Decide the naming convention, then make the registry match it.** Either
  rename the codes to the convention the codebase uses, or state plainly that
  the `E_*` names are wire-level identifiers distinct from Rust variant names —
  in which case the registry should carry both columns, because the second is
  what a reviewer greps for.
- **Widen the heading to include RFC-0967**, or move those two rows.
- **No code change is implied by the right-hand table above.** It is a sampling
  result, and the useful next step is a check of those nine concepts against
  whatever the implementation does call them.

## Reproduction

```bash
R=rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md

# the provenance column that cannot be resolved
grep -cE '^\| .E_[A-Z_]+.*\| R25 ' "$R"
grep -oE '^\| .E_[A-Z_]+.*\| R25 ' "$R" | grep -oE 'E_[A-Z_]+'

# the rounds that do exist, and the ones that do not
grep -oE '^## R[0-9]+' "$R" | grep -oE '[0-9]+' | sort -n | tr '\n' ' '
grep -rE '^#+ R(1[7-9]|2[0-5])\b' --include='*.md' rfcs/ || echo "no R17-R25 anywhere"
grep -c 'R25' "$R"        # 11: every occurrence is a table cell

# the naming convention in use
grep -oE '`E_[A-Z_]+`' "$R" | tr -d '`' | sort -u | while read -r c; do
  grep -rq "$c" --include='*.rs' crates/ 2>/dev/null && echo -n "$c "
done
echo   # nothing printed = zero exact hits

# the concepts under Rust naming
for c in ReplayDetected SignatureInvalid PolicyNotFound ChainDepthExceeded \
         CapabilityRevoked GasLimitExceeded DdlNotActivated LocalChainForked; do
  printf '%-26s %s\n' "$c" "$(grep -rl "$c" --include='*.rs' crates/ | wc -l)"
done

# the header scope
grep -n 'RFC-0960/0961/0962/0963/0964/0965' "$R"
grep -cE '^\| .E_[A-Z_]+.*\| RFC-0967' "$R"
```

## Cross-references

- Mission `four-of-rfc-0960s-sixteen-self-review-rounds-re-found-one-unpropagated-change`
  — the same document's review log, which stops at R16 and is where R17–R25
  would have been recorded
- Mission `two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review`
  — RFC-0960 in Grade 3 there, so this mission covers the half of the document
  that is present rather than the half that is not
- Mission `the-section-cite-linter-checks-token-shape-and-never-resolution` —
  the class of a citation that cannot be resolved, here entirely within one
  document so no cross-file search is needed to show it
- Mission `rfc-0959-claims-a-determinism-verification-that-does-not-exist` —
  the same shape at document level: a section's stated purpose is traceability,
  and its contents do not deliver it
