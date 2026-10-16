---
name: rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not
description: "RFC-0959-v27 is the only one of the four RFC-0959 documents that verifies its claims against the substrate, corrects its own errors with named review rounds, and marks what is pending confirmation. Checked against the tree: its diagnostic claims all hold. The cost_micro_octo_w column is BLOB NOT NULL exactly as quoted, MigrationError has exactly the two variants it names, dqa_assign_to_column exists, and its R5 correction that RFC-0105 has no DQA Serialization section while RFC-0126 does is correct. Its own substrate-status paragraph is the one part that has drifted, moving from max v016 to max v020 in quota-router-storage and v014 to v015 in octo-vault, which is precisely the drift it instructed implementers to re-verify rather than trust. The phantom section citation it caught and fixed is the same defect class found unfixed in RFC-0959 base, in RFC-0959-A1 and in two mission files."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959-v27
    - RFC-0105
    - RFC-0126
---

# Mission `rfc-0959-v27-checks-its-own-substrate-claims-and-the-other-three-do-not`

## Status

Nothing to action. v27's only drift is its own migration-numbering status
paragraph, which is dated substrate state rather than a specification, and it
already tells implementers not to trust it.

Tenth systematic sweep, and the last of the four RFC-0959 documents.

## Finding

A sweep that only reports gaps leaves a misleading picture, so this one records
the document that does the thing the other three do not.

**RFC-0959-v27 verifies its own claims against the substrate, and it is
mostly right.** Nine of its ten checkable assertions were tested against the
tree. Eight hold exactly:

| v27's claim                                                | Verified                                |
| ---------------------------------------------------------- | --------------------------------------- |
| `cost_micro_octo_w BLOB NOT NULL`, 16-byte BE u128         | yes, at v004 as quoted                  |
| `MigrationError` has only `Storage` and `UnknownMigration` | yes, exactly those two                  |
| `dqa_assign_to_column` exists for column-scale assignment  | yes                                     |
| `DqaEncoding` is 16 bytes; RFC-0126 carries the wire form  | yes                                     |
| RFC-0105 has **no** `§DQA Serialization` heading           | yes — the heading is absent             |
| RFC-0126 has `§DQA Serialization`                          | yes                                     |
| `dqa_from_bytes` parses `DqaEncoding`, not raw u128 BE     | yes, and the doc is right to avoid it   |
| Stoolap integer-scale `DQA(0)` support is unconfirmed      | still unconfirmed; still marked pending |

The ninth is the substrate-status paragraph, and it has drifted:

> current `crates/quota-router-storage/migrations/` max is v016;
> `crates/octo-vault/migrations/` max is v014

On disk the maxima are now **v020** (`policy_registry_columns_v2`) and **v015**
(`vault_balance_projection_cache`). The v019 slot it reserves for a policy DDL
rollback has been taken by `v019__litellm_user_vault_link.sql`.

That is not a finding against v27. It is the drift v27 predicted, in the exact
place it warned about, in the paragraph it annotated:

> Mission implementer MUST verify the actual on-disk max version before
> allocating a migration number; renumbering vs substrate reality may be required
> if v015–v018 land with different numbering than RFC-0206 v3.0 §4 planned.

v015–v018 landed with different numbering. v27 said to check. The lesson is the
document's, working as designed.

## One place v27's reasoning is right for a slightly different reason

v27 warns that routing a raw u128 BE cost through `dqa_from_bytes` "rejects
every raw u128 BE row with `DqaError::InvalidEncoding`", and prescribes a direct
u128 BE → range-check → `Dqa::new` path instead.

The prescription is right. The stated reason is a side effect rather than a
designed guard. `dqa_from_bytes` parses the `DqaEncoding` layout — eight bytes
of big-endian i64, one scale byte, seven reserved bytes — and rejects when the
reserved bytes are non-zero. It does not know it is looking at a u128. A raw
16-byte BE cost of `1_000_000` puts `0F 42 40` into the reserved region and is
rejected; so is `1`, and `256`, and anything else with a non-zero low byte.

The exception is a value whose low 56 bits are all zero — a multiple of 2^56,
roughly 72 million OCTO-W in MicroOCTO_W units. Such a row would decode without
error, and the first eight bytes would be read as the value, so it would come
back as **zero** rather than as the amount.

So the hazard v27 identifies is real, and its guard is incidental. Nothing in
`dqa_from_bytes` is checking that it was handed the right thing. That strengthens
v27's recommendation rather than weakening it: the conversion it prescribes is
not merely more correct, it is the only one that does not depend on a rejection
that happens to fire.

## What makes v27 different

**It marks its own uncertainty as uncertainty.** Three separate places carry an
explicit "pending substrate confirmation" or "RFC-defined extension pending
landing" note, and the SQL block in §3.2 says outright that it is "illustrative
of the RFC interpretation, NOT a recommendation".

**It records that a previous version of itself was wrong.** The R5 correction
states plainly that the R3 fix-all referenced a phantom `RFC-0105 §DQA
Serialization` cite, and re-anchors to RFC-0126. It also corrects a stale column
name inherited from a research doc (`te.amount_dqamicros` → `te.amount`) and
attributes the correction to the substrate as authoritative.

**It names the reader's next action.** Each divergence header ends with a MUST
or SHOULD for whoever lands the work.

**It says when a claim is a choice.** The `DQA(12)` scale-semantics discussion
spells out both interpretations, shows that interpretation (b) is "silently
wrong for any reader who follows §3.2 DDL alone", and declines to pick without
substrate input.

## Why it matters to the rest of this audit

The phantom-section-citation class has now appeared five times:

1. `crates/octo-core/src/settlement.rs` — named in the base RFC's §Key Files and
   §Roles, does not exist
2. `RFC-0862 §Transaction guarantees` — cited by A1's IA-2 and IA-6, does not
   exist
3. `ReceiptReplay` — cited by base TV1 as an error variant, explicitly never was
   one
4. `crates/octo-core/tests/fixtures/cache_key_tv3.json` — base TV3's sole
   expected value, in a crate with no tests directory
5. `RFC-0105 §DQA Serialization` — **caught and corrected by v27**

The fifth is the proof that the class is detectable by ordinary reading against
the tree. One author in this document set did exactly that, found one, fixed it,
and wrote down that they had been wrong.

The other three RFC-0959 documents each still carry at least one uncaught
instance.

## What the owner should do

- **Use v27 as the template for the others.** Its structure — a divergence
  header per substrate conflict, an explicit pending marker, a named next action,
  and a version history that records what a previous round got wrong — costs
  nothing and is the only thing standing between the document and a reader's
  wasted afternoon.
- **Refresh v27's substrate-status paragraph** when migrations land, since it is
  the one part it marks as dated.
- **Point the base RFC and A1 at v27.** Both cite `canonical_ser` and DQA
  semantics; v27 has already resolved where that anchor actually lives, and both
  would benefit from the pointer.

## Reproduction

```bash
# v27's substrate claims, all still true
grep -n 'cost_micro_octo_w' crates/quota-router-storage/migrations/v004__create_settlement_events.sql
grep -n 'pub enum MigrationError' -A 12 crates/quota-router-storage/src/migrations.rs
grep -rn 'fn dqa_assign_to_column' --include=*.rs determin/ crates/

# the R5 correction: the phantom cite really is absent, and the real anchor present
grep -nE '^#{2,4} ' rfcs/accepted/numeric/0105-*.md | grep -i serial \
  || echo "RFC-0105 has no serialization section — v27 is right"
grep -nE '^#{2,4} ' rfcs/accepted/numeric/0126-*.md | grep -i 'DQA Serialization'

# the one part that drifted, exactly where v27 said to check
ls crates/quota-router-storage/migrations/ | sort | tail -1
ls crates/octo-vault/migrations/ | sort | tail -1

# and the discipline, visible in the document itself
grep -c 'pending substrate\|RFC-defined extension\|MUST ' \
  rfcs/accepted/economics/0959-v27-burn-event-wire-form.md
```

## Cross-references

- Mission `rfc-0959-a1-cites-a-transaction-guarantee-section-that-does-not-exist`
  — the second instance of the uncaught phantom-citation class, in the sibling
  amendment
- Mission `rfc-0959-test-vectors-contain-no-expected-values` — the base RFC's
  fixture path that does not exist, the fourth instance
- Mission `rfc-0959-key-files-table-points-ten-rows-at-nothing` — the first
  instance, and the same §Key Files table in the base RFC
- Mission `the-settlement-hash-that-binds-cost-is-built-and-not-on-the-live-path`
  — v28, the fourth RFC-0959 document, and the only settlement hash that binds
  cost; it does not claim to have verified against the substrate, so the v27
  habit is worth extending to it
- Mission `four-of-rfc-0959s-ten-implementation-assumptions-fail` — IA-5's
  `canonical_ser` assumption, the same substrate question v27 answers with a
  verified anchor
