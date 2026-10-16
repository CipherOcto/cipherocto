---
name: two-of-rfc-0960s-four-integration-scenarios-name-surface-that-does-not-exist
description: "RFC-0960's Test Strategy names four integration scenarios. Two are genuinely implemented: the eleven-step exercise is a forty-three kilobyte test that drives real crates and does make a real Reservation at step six, and the audit window appears in three source files. Two name surface that does not exist. MultiSettlement appears in four Accepted RFCs and in no Rust file anywhere, while the related MultiEnvelope concept does exist in three files, so the sub-envelope half of the same feature landed and the settlement half did not. The Enterprise migration scenario depends on CIPHERO_SUBSCRIPTION, which is a DDL statement in RFC-0961, itself a six-hundred-and-fifty-five line Accepted language specification with no parser, lexer or grammar implementation in the tree and no mission to build one. RFC-0960 is not at fault for the second: it says explicitly that it only specifies the high-level mode and delegates the dialect."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0960
    - RFC-0961
    - RFC-0962
---

# Mission `two-of-rfc-0960s-four-integration-scenarios-name-surface-that-does-not-exist`

## Status

Deferred. Naming an integration test for surface that is not built is not a
defect in the test strategy — it is the strategy pointing at work not yet done.
Whether `MultiSettlement` is in scope, and who builds RFC-0961's dialect, are
both owner calls. Nothing was actioned.

## Finding

RFC-0960 §Test Strategy lists four integration scenarios:

| Scenario               | Named test                                              | Status                       |
| ---------------------- | ------------------------------------------------------- | ---------------------------- |
| 11-step exercise       | Step 6 escrow becomes a real `Reservation` row          | **implemented**              |
| Enterprise migration   | PostgreSQL logical replication → `CIPHERO_SUBSCRIPTION` | **dependency unimplemented** |
| Cross-shard settlement | `MultiSettlement` atomicity test                        | **surface absent**           |
| Audit window           | Settled → Frozen → Dispute → Rollback                   | **implemented**              |

Two of four are real. The two that are not are worth separating, because one is
a gap in someone else's RFC and one is a gap in this one.

## The two that hold

**The eleven-step exercise is the strongest piece of evidence in this part of
the tree.** `quota-router-core/tests/eleven_step.rs` is 43KB, imports
`octo_ident`, `quota_router_core`, `quota_router_sm_engine`,
`quota_router_storage`, `cipherocto_encoding`, `octo_determin` and
`octo_policy`, and carries a function documented as:

> Step 6: OCTO-W escrow pre-auth (**real Reservation** per RFC-0960 §2.3).

It is invoked from two places in the pipeline, so the scenario RFC-0960 names
is not aspirational — it runs, on real crates, and the Reservation is real.
That is worth recording plainly, because the rest of this mission is about what
a specification names versus what exists, and this is a case where the two
agree.

The audit window appears in three source files under both spellings.

## Cross-shard settlement names a type that does not exist

`MultiSettlement` is named in **four Accepted RFCs** — RFC-0960, RFC-0961,
RFC-0962 and RFC-0963 — and in **no Rust file in the tree**, under any casing or
separator form.

The instructive part is what _did_ land. The same feature has a second half that
does exist: `MultiEnvelope` appears in three Rust files. RFC-0960's own error
registry records the rename chain — `E_MULTI_ENVELOPE_TIMEOUT` is annotated
"(renamed from MultiSession in RFC-0962)" — so the envelope side was renamed,
specified and implemented, while the settlement side was specified four times
over and never implemented.

That asymmetry is the finding. Multi-settlement is not an abandoned idea; it is
a live cross-RFC concept with four documents depending on it and no
implementation, while its sibling concept in the same feature area shipped.

## Enterprise migration depends on a dialect nobody has built

The scenario routes PostgreSQL logical replication through
`CIPHERO_SUBSCRIPTION`, a DDL statement defined in RFC-0961. That is not a Rust
symbol, so its absence from `.rs` files is expected and is not counted as a
gap here.

The gap is one level up. RFC-0961 is an **Accepted, 655-line, 54-section**
specification of a deterministic SQL dialect — grammar, parse-time determinism
checks, runtime determinism verification. There is no parser, lexer or grammar
implementation for it anywhere in the tree, and the only hit for the dialect name
in Rust is a passing mention in an envelope module.

Three missions mention RFC-0961, and none of them is "build the engine": the
grand-design reference, a vault substrate amendment, and the execution envelope.

**To be fair to RFC-0960, this is not its error.** It says:

> Stoolap already gives us DDL, indexes, views, foreign keys, window functions,
> joins. Add `CIPHERO_SQL` mode that forbids: …
>
> `CIPHERO_SQL` spec in RFC-0961 (companion). **This RFC only specifies the
> high-level mode.**

RFC-0960 delegates correctly. What is missing is an owner for RFC-0961's
implementation, and RFC-0960's §Test Strategy is simply the place where that
missing owner becomes visible — a test strategy that names a scenario its own
specification has explicitly deferred to an unimplemented companion.

## What the owner should do

- **Decide whether `MultiSettlement` is still in scope.** Four Accepted RFCs
  reference it. Either it is scheduled and the test scenario is a legitimate
  forward reference, or it has been superseded by the `MultiEnvelope` shape and
  those four documents should be corrected.
- **Give RFC-0961 an implementation mission.** It is Accepted, it is the largest
  unwritten substrate in this stack, and RFC-0960 §Test Strategy already depends
  on it. Nothing else in the corpus will surface it as sharply.
- **Leave the eleven-step scenario alone.** It is what a test strategy entry
  looks like when it is true, and it is the best available argument for keeping
  the section.

## Reproduction

```bash
R=rfcs/accepted/economics/0960-grand-design-vaults-capabilities-reservations.md
T=crates/quota-router-core/tests/eleven_step.rs

# the scenario table
grep -n -A 6 '### Integration tests' "$R"

# implemented: the eleven-step exercise, with a real Reservation
ls -la "$T" | awk '{print $5" bytes"}'
grep -n 'real Reservation' "$T"
grep -c 'step6_escrow_preauth' "$T"

# implemented: the audit window
grep -rl 'AuditWindow' --include='*.rs' crates/ | wc -l

# absent: MultiSettlement, four Accepted RFCs and no Rust
grep -rl 'MultiSettlement' --include='*.md' rfcs/accepted/
grep -rl 'MultiSettlement' --include='*.rs' crates/ || echo "no Rust anywhere"
grep -rliE 'multi.?settlement' --include='*.rs' crates/ || echo "no form of it in Rust"

# the sibling that did land
grep -rl 'MultiEnvelope' --include='*.rs' crates/ | wc -l

# unimplemented dependency: the dialect
wc -l < rfcs/accepted/economics/0961-ciphero-sql-language-spec.md
grep -rl -i 'ciphero_sql' --include='*.rs' crates/
grep -rn 'CIPHERO_SUBSCRIPTION' --include='*.md' rfcs/accepted/ | cut -c1-120

# and RFC-0960 declining to own it
grep -n 'only specifies the high-level mode' "$R"
```

## Cross-references

- Mission `rfc-0960-closes-its-review-at-r28-and-records-sixteen-rounds` — the
  same document's §Reference Implementation, which names a companion crate that
  also does not exist; both are specifications pointing at unbuilt surface
- Mission `a-third-of-rfc-0960s-error-code-registry-traces-to-a-round-that-does-not-exist`
  — the same RFC stack, and the registry records the `MultiSession` →
  `MultiEnvelope` rename this mission uses as its evidence
- Mission `four-of-rfc-0960s-sixteen-self-review-rounds-re-found-one-unpropagated-change`
  — R-round context for the document these findings are in
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  `MultiSettlement` and the CIPHERO_SQL dialect both join, and the eleven-step
  exercise is the counter-example worth keeping in view
