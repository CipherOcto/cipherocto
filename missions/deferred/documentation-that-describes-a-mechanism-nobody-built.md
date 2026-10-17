---
name: documentation-that-describes-a-mechanism-nobody-built
description: "Across the marketplace audit, ten independent findings share one shape: a specification or doc comment describes a mechanism in enough detail to read as implemented, and no code path implements it. This mission inventories the ten, states the class, and argues the failure mode is materially different from a missing control, because a documented-but-absent control survives code review, passes as a citation, and cannot be found by grepping for the missing symbol. The class spans the Ask lifecycle, the settlement hash, signature verification, the jurisdiction gate, the index rebuild, persisted storage layout, and the CLI surface. It is recorded as the audit's most actionable output because it points at a class rather than an instance."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0862
---

# Mission `documentation-that-describes-a-mechanism-nobody-built`

## Status

Deferred — this is an audit synthesis, not a defect, so there is nothing to
fix in the code. The proposed remedy is a check, and building a checker is
explicitly out of scope for this audit: an earlier attempt at a citation
classifier in this same audit was wrong three times before being abandoned.
See §What this deliberately does not do.

## Finding

Ten findings in this marketplace audit are not ten bugs. They are ten
instances of one shape:

> A specification or doc comment describes a mechanism **in enough detail to
> read as implemented**, and no code path implements it.

The distinction from an ordinary missing control is the whole point, and it
is what makes these findings expensive.

## Why this shape is worse than a missing control

A control that was never written is **visible in review**. A reviewer asks
"where is this enforced?", greps for the function, finds nothing, and the
gap is closed in the same sitting.

A documented-but-absent control does the opposite. It:

- **reads as implemented** to anyone who reads the doc rather than the code;
- **cannot be found by grepping for the missing symbol**, because the symbol
  in the documentation is an RFC concept, not a Rust identifier — `soft_delay`,
  `Rebuilding`, `receipt_id`;
- **survives citation checks**, because the citation is accurate; the RFC does
  say what the doc says it says;
- **cannot be caught by the tests it appears to have**, because the tests test
  the part that exists.

That last point is the one that bit hardest here, and it recurred: a test
named for the missing property, asserting something weaker, passing.

## The ten instances

| #   | Documented mechanism                                                                                      | Where documented                          | Reality                                          | Mission                                                                 |
| --- | --------------------------------------------------------------------------------------------------------- | ----------------------------------------- | ------------------------------------------------ | ----------------------------------------------------------------------- |
| 1   | `soft_delay` warm-up gate on `Published --> Active`                                                       | RFC-0959 §Ask state machine               | absent from every `.rs` file                     | `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type` |
| 2   | `cost` bound into the settlement hash ("R3 fix")                                                          | RFC-0959 §Settlement hash                 | absent from the live preimage                    | `settlement-hash-does-not-bind-the-cost`                                |
| 3   | Payee and payer bound into the settlement                                                                 | implied by a settlement's purpose         | absent from the preimage; measured               | same                                                                    |
| 4   | "Recipients verify signature before marketplace insert"                                                   | RFC-0959 §Security Considerations         | no verification caller in production             | `the-ask-signature-gates-nothing`                                       |
| 5   | `JurisdictionMismatch` → HTTP 451                                                                         | RFC-0959 §Data Structures                 | variant declared, never constructed; no 451      | `an-ask-declared-for-one-jurisdiction-is-served-to-any-other`           |
| 6   | Index rebuild on RFC-0862 sync event; `Rebuilding`/`Stale` states                                         | RFC-0959 §Roles + §Lifecycle Requirements | no sync adapter, no states, no divergence metric | `the-marketplace-index-has-no-rebuild-path-and-no-staleness-check`      |
| 7   | `node_type` "remains unchanged as persisted storage layout"                                               | RFC-0959 IA-7 amendment                   | in no migration in the tree                      | `rfc-0959-defines-one-ask-and-the-code-has-two-with-different-fields`   |
| 8   | `octo-wallet ask publish/list/show/revoke`                                                                | RFC-0959 §Roles                           | only `publish` exists; other three exit 2        | `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type` |
| 9   | `ask_id` as a single "Wire/storage form" address                                                          | RFC-0959 §Data Structures invariant       | two derivations, unequal; no bridge              | `the-same-ask-has-two-different-content-addresses`                      |
| 10  | A store-error was discarded because "a follow-on observability hook can detect via `load_all` divergence" | `quota-router-core` `slashing.rs` comment | **already found and fixed** by the team          | —                                                                       |

The tenth is the important one for calibration. It is the same shape, it was
found independently by the codebase itself, and it was **fixed** — with the
error propagated and the in-memory penalty rolled back. So this class is
detectable and fixable by someone who looks, which is the main reason to
report it as a class.

Note also that #1 and #6 are _not_ findable by grepping for a plausible
identifier. `soft_delay` is an RFC concept; there is no Rust symbol a reader
would expect to find. `Rebuilding` is a mermaid state name.

## What the codebase gets right

The balance matters, because a synthesis that only counts failures invites
the reading that the documentation is uniformly unreliable. It is not.

Concretely accurate documentation found during this audit, in the same
crates:

- the `select_ask` doc after correction — names the two gates it applies and
  the one it does not;
- the `prune_expired_and_overflow` doc after correction — states the `0`
  sentinel requirement and warns that `u64::MAX` clears the index;
- the `Compute cost` doc — correctly flags the registry-default fallback as a
  deferred RFC decision rather than a settled behaviour;
- the `settlement_cost` `# Panics` section, and the recorded proof that one
  of its overflow routes is blocked by `dqa_mul` canonicalisation;
- the whole `dqa_add`/`dqa_mul` canonicalisation analysis, which records both
  the live panic and the route that is provably unreachable.

The failure mode is concentrated in one place: **RFC-level prose asserting an
operational guarantee, which the implementation absorbed as a doc comment
without a corresponding gate.** Code-local documentation is largely sound,
because someone who wrote it was looking at the code.

That distinction suggests where the remedy belongs, and it is not
"document less".

## The one counter-example that reframes it

Two findings in this audit were the _opposite_ failure: a citation naming
something that does not exist, so a reader concluded a control was missing
when it was not.

- `AskSigned::verify` — my first move on finding no file at the RFC's stated
  path was to conclude the Anti-Fraud Monitor was unimplemented. It is
  implemented; the RFC names the wrong **crate**.
- Ten sites citing `RFC-0959 §CLI` and seven citing `§Cache Classification`,
  neither of which is a section of RFC-0959. Both corrected; the counts are
  recorded in
  `rfc-citations-in-rust-source-name-sections-that-do-not-exist`.

So the two failure modes compound. A miscited location produces a false
absence; an accurately-cited-but-unimplemented mechanism produces a false
presence. Both resist grepping, in opposite directions.

## What this deliberately does not do

This audit has already tried to build a detector for this class and failed.
An automated citation classifier was written to sweep RFC section references
for phantoms; it reported false positives on valid citations **three times**
before being abandoned in favour of hand verification. A checker for
"documented mechanism with no implementation" would be strictly harder —
it requires deciding what counts as an implementation.

The inventory above is hand-verified, one mission at a time, and is therefore
a lower bound on the class rather than a count.

## What the owner can do without a checker

- **Grep the RFC for prescriptive verbs.** "MUST", "reject", "verify",
  "returns HTTP", "bound into hash", "rejects duplicates" — each is a
  candidate instance. RFC-0959 §Security Considerations and §Acceptance
  criteria are the densest areas, and both produced findings here.
- **Read the §Roles table column by column.** Every row names an authority
  and a lifecycle. #5, #6 and #8 all came from that one table.
- **Treat "advisory only" and "future work" as load-bearing.** RFC-0959 is
  careful about these, and they are how a reader tells a real guarantee from a
  deferred one.
- **Prefer the RFC's acceptance criteria over its prose.** G6 and A5 were
  sharper than the state machines around them.

## Cross-references

- `the-settlement-path-records-but-never-settles` — the audit's earlier
  synthesis and severity calibration, for the standing caveat that most of this
  is latent
- `rfc-citations-in-rust-source-name-sections-that-do-not-exist` — the
  opposite failure mode, and the citation sweep that followed it
- The ten missions named in the inventory table above
- Mission `settlement-hash-does-not-bind-the-cost` — #2 and #3, and the one
  finding in this set whose failure I had to correct upward after measuring
