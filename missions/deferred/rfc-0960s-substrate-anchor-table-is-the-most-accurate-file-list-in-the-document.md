---
name: rfc-0960s-substrate-anchor-table-is-the-most-accurate-file-list-in-the-document
description: "RFC-0960 §Substrate anchors (LANDED) names four files. All four exist, and v013's four structural claims — the composite primary key, the unique index, the DQA(12) balance column, and the absence of parent_vault_id — hold exactly. That makes it the most accurate file-level claim list in the document, and worth measuring against: RFC-0960 §Key Files elsewhere in the same RFC names eleven paths of which ten do not exist. Two claims inside the accurate table still fail. v014's transfer-event columns are described as foreign-key references and carry no REFERENCES clause. And the 108-fixture vault_id matrix is described as byte-exact when its anti-drift guard compares the derivation against itself, which was proven by mutating the BLAKE3 domain separator and watching the guard pass unchanged."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0960
---

# Mission `rfc-0960s-substrate-anchor-table-is-the-most-accurate-file-list-in-the-document`

## Status

Deferred. Two schema descriptions and one fixture claim, all RFC edits. The
mutation experiment was reverted; no source file is modified.

## Finding

`### Substrate anchors (LANDED)` names four files and makes specific structural
claims about each. It is worth measuring this table rather than reading it,
because it is the only place in RFC-0960 that promises something checkable about
the substrate at file granularity — and the answer is better than almost
anything else this sweep has found in the document.

**All four paths exist. All of v013's claims hold exactly. Two claims inside the
table still fail.**

| Anchor                                         | Exists | Claims hold             |
| ---------------------------------------------- | ------ | ----------------------- |
| `migrations/v013__create_vaults.sql`           | yes    | **all four**            |
| `migrations/v014__create_transfer_events.sql`  | yes    | **no** — FK claim fails |
| `src/lib.rs::vault_id` + `vault_id_unchecked`  | yes    | **yes**                 |
| `tests/test_vectors.rs::tv_v1_vault_id_matrix` | yes    | **no** — not byte-exact |

The contrast with the rest of the document is the point. RFC-0960's §Key Files
section names eleven paths and ten of them do not exist. This table names four and
all four do. Same document, same author, same review process — one section
describes what landed and the other describes what was planned. The section that
describes reality is the accurate one.

## What v013 gets exactly right

Four claims, four correct:

| Claim                                    | Verified                                                                    |
| ---------------------------------------- | --------------------------------------------------------------------------- |
| PK = `(chain_id, owner_did, asset_id)`   | `PRIMARY KEY (chain_id, owner_did, asset_id)`                               |
| `vaults_vault_id_idx` UNIQUE INDEX       | `CREATE UNIQUE INDEX IF NOT EXISTS vaults_vault_id_idx ON vaults(vault_id)` |
| `balance DQA(12)`                        | `balance DQA(12) NOT NULL`                                                  |
| no `parent_vault_id` column (§20.8 lock) | absent, and explicitly annotated as dropped                                 |

The last one deserves credit beyond correctness. The migration does not merely
lack the column; it records _why_:

> -- VaultHierarchy dropped (phantom type, see §20.10); no parent_vault_id.

A negative constraint enforced by removal, with the reason kept next to the
removal. That is the practice the rest of the corpus is trying to reach.

## Defect 1: v014's columns are not foreign keys

The table describes the transfer-event migration as:

> `from_vault_id` + `to_vault_id` **FK references**, chain-prefixed transfer
> semantics per §20.7

They are not foreign keys. The migration declares:

```
from_vault_id    BLOB(32) NOT NULL,
to_vault_id      BLOB(32) NOT NULL,
```

and nothing else. There is no `REFERENCES vaults(vault_id)` clause, and no
`FOREIGN KEY` anywhere in the file. The two columns are unconstrained `BLOB(32)`
values that the database will accept for vaults that do not exist.

This is not house style being described loosely. Other migrations in this
repository do declare real references — `quota-router-storage` has two files
using `REFERENCES` / `FOREIGN KEY`. The omission is specific to `octo-vault`.

It is also worth knowing whether the omission was deliberate. There is a recorded
rationale elsewhere in the tree for skipping foreign keys under a "fork
constraint", and `octo-vault`'s migration header does carry a fork-related
constraint — but it is about `NO inline -- comments mid-statement`, which has
nothing to do with referential integrity. So the file documents a different
parser limitation and says nothing about the constraints the RFC attributes to
it.

For an append-only transfer event log this matters more than it would elsewhere.
An event can name a `from_vault_id` that was never created, and the database will
not object. RFC-0960's §Security Considerations Threat 4 is about event-log
integrity; this is the layer where a divergence would land.

## Defect 2: the 108-fixture matrix is not byte-exact

The table calls `tv_v1_vault_id_matrix` a "108 **byte-exact** vault_id matrix
fixture". It is 108 fixtures. They are not byte-exact, and the test named as
protecting them cannot detect drift.

The file is unusually honest about this, which is what made it findable. Its own
header says:

> Anti-drift guard (per AC-6): each fixture's `vault_id` field is computed via
> `vault_id_unchecked_fixt` (which calls the canonical `vault_id_unchecked`) and
> asserted equal to its own `vault_id` field on every test run. If the derivation
> ever drifts, the test catches it at compile + test time **without needing 108
> hand-pinned hex constants**.

And the guard is named for what it does: `tv_v1_vault_id_matrix_matches_self_derivation`.

Reading the code confirms the shape. `vault_id_unchecked_fixt` calls
`vault_id_unchecked` and unwraps the bytes. The fixture builder stores that same
call in each fixture's `vault_id` field. The test then re-derives with the same
function on the same inputs and asserts equality. **Expected equals actual by
construction.**

### Proved by mutation

Inspection of a self-comparison proves it is circular, but does not prove the
guard is _incapable_ of catching drift. So I mutated it. The canonical
derivation's domain separator in `vault_id_unchecked` was changed from
`cipherocto/vault/v1/` to `cipherocto/vault/v2/MUTATED/`, which changes every
`vault_id` in the tree, and the suite was run:

```
test tv_v1_vault_id_vectors_match_pinned_hex ... FAILED
test tv_v1_vault_id_matrix_matches_self_derivation ... ok
```

The anti-drift guard passed against a deliberately corrupted derivation. The
mutation was reverted immediately and the suite returns to 15 passed, 0 failed.

### My hypothesis was wrong about the rest of the file

I expected the whole file to be in this state and expected to file the §Test
Vectors finding again. That was wrong, and the failure is worth recording. The
same file contains `TV_V1_VECTORS` — **ten** hand-pinned vectors with literal hex
constants — asserted by `tv_v1_vault_id_vectors_match_pinned_hex`. That test
caught the mutation immediately, with an exact expected/actual diff:

> actual: `2c2fa66c9053fecc2ba6b28421e7b270a3cf484d5ba95817a2d9d5db139c3217`
> expected: `2d5b470ba690b80ea35d5a60c864014304fd72a52093c0ad66ba9290d153691c`

So `octo-vault` has real byte-exact coverage, and it is better coverage than the
RFC credits: the ten pinned vectors include the empty-DID, empty-chain,
unicode-DID and cross-chain boundary cases, which are precisely the inputs a
derivation is most likely to get wrong.

**The RFC cites the wrong half of the file.** It points at the 108-row matrix and
calls it byte-exact, while the genuinely byte-exact vectors sit under a different
test name in the same file. The guarantee exists; the citation points past it.

## The two untracked references

Of the seven paths under §Internal research, five are tracked and two are not:

| Path                                                                  | Status         |
| --------------------------------------------------------------------- | -------------- |
| five `docs/research/2026-07-22-*` documents                           | tracked        |
| `docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md`     | **gitignored** |
| `docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md` | **gitignored** |

Both exist locally and are ignored by rule (`.gitignore` covers `docs/reviews/`
and `/docs/plans/`). RFC-0960 cites the review for four specific normative
outcomes — §9.5 `parent_vault_id` removal, §20.3 Model B lattice, §20.7
intra-chain transfer semantics, §20.8 no-organizational-intermediates lock — and
the plan for two more. This is already recorded for RFC-0960 elsewhere, so it is
not filed again here. It is noted because it bounds the table above: the
`parent_vault_id` claim in v013 is verifiable in the migration itself, which is
why it could be confirmed without the review that mandates it.

## Errors I made while measuring this

Three, recorded because each nearly shipped a wrong claim.

**I assumed the whole test file lacked pinned vectors.** The 108-fixture matrix
is not byte-exact, and I had that from reading the code — but I generalised it
to the file. It was wrong: the same file holds ten hand-pinned vectors, and the
mutation proved it by having them catch the drift. Had I filed the finding
without running the experiment, I would have recorded "no byte-exact coverage in
`octo-vault`", which is false, and I would have reinforced a prior mission's
finding that is too broad. **This is the argument for mutation over inspection:**
the self-comparison was already a proof of circularity, and it still would not
have told me what else the file contained.

**I mis-transcribed the expected hex from the test output**, dropping a digit
(`...ba9269d...` for `...ba9290d...`). Quoting a value from tool output and
re-typing it introduces exactly the kind of error this whole audit is about. The
fix is to compare any quoted constant back against its source, which is one grep
away.

**I invented a plausible mission name for a cross-reference** —
`rfc-0960-test-vectors-name-four-scenarios-and-not-one-has-an-expected-value` —
which does not exist. The real one is `rfc-0959-test-vectors-contain-no-expected-values`.
Same error as the §Open Questions sweep earlier in this session: naming a
mission you have not checked is the defect you are auditing.

## What the owner should do

- **Fix the v014 row.** Either drop "FK references" or add the constraints. The
  first is a sentence; the second is a migration and a compatibility decision,
  which is the owner's.
- **Repoint the test_vectors row** at `tv_v1_vault_id_vectors_match_pinned_hex`
  and say "ten hand-pinned boundary vectors", which is both true and more
  impressive than what it currently claims. If the 108-matrix is meant to be a
  byte-exact guarantee, that is a real piece of work — 108 pinned hex constants
  and a generator that emits them — and worth deciding deliberately rather than
  by inheritance.
- **Fix the `§20.3.1` and `§20.3.2` citations** in §Internal RFCs. Both are in
  the anchor linter's baseline, so both are known-dangling.
- **Leave the table otherwise alone.** It is the accurate section. More
  RFCs should cite substrate files the way this one does.
- **Note the `octo-vault` FK question for the Stoolap fork constraint.** If
  referential integrity is genuinely unavailable in the fork, that belongs in the
  migration header next to the parser constraint that is already there.

## Reproduction

```bash
# All four claimed paths exist
for p in crates/octo-vault/migrations/v013__create_vaults.sql \
         crates/octo-vault/migrations/v014__create_transfer_events.sql \
         crates/octo-vault/src/lib.rs \
         crates/octo-vault/tests/test_vectors.rs; do
  [ -e "$p" ] && echo "EXISTS  $p" || echo "MISSING $p"
done

# v013: all four claims hold
grep -nE 'PRIMARY KEY|UNIQUE INDEX|balance|parent_vault_id' \
  crates/octo-vault/migrations/v013__create_vaults.sql

# Defect 1: no REFERENCES anywhere in v014, though other crates use it
grep -cE 'REFERENCES|FOREIGN KEY' crates/octo-vault/migrations/v014__create_transfer_events.sql
grep -rlE 'REFERENCES|FOREIGN KEY' crates/*/migrations/*.sql

# Defect 2: the guard compares the derivation against itself
grep -n -A3 'fn vault_id_unchecked_fixt' crates/octo-vault/tests/test_vectors.rs
grep -n 'matches_self_derivation' crates/octo-vault/tests/test_vectors.rs
grep -n 'without needing 108 hand-pinned hex' crates/octo-vault/tests/test_vectors.rs

# ...and the file really does hold 10 pinned vectors, which is why the
# mutation below was caught by a different test
grep -cE 'name: "TV-V1-[0-9]' crates/octo-vault/tests/test_vectors.rs
grep -nE 'const TV_V1_0[1-3]:' crates/octo-vault/tests/test_vectors.rs

# MUTATION EXPERIMENT — revert before doing anything else
cp crates/octo-vault/src/lib.rs /tmp/lib.rs.bak
python3 - <<'PY'
p = 'crates/octo-vault/src/lib.rs'
s = open(p).read()
old = '    h.update(b"cipherocto/vault/v1/");'
assert s.count(old) == 1
open(p, 'w').write(s.replace(old, '    h.update(b"cipherocto/vault/v2/MUTATED/");'))
PY
cargo test -p octo-vault --test test_vectors 2>&1 | tail -20
# observed: tv_v1_vault_id_vectors_match_pinned_hex FAILED
#           tv_v1_vault_id_matrix_matches_self_derivation ok
cp /tmp/lib.rs.bak crates/octo-vault/src/lib.rs
cargo test -p octo-vault --test test_vectors 2>&1 | tail -3
git diff --stat crates/octo-vault/src/lib.rs   # must be empty

# The two untracked references
git check-ignore -v docs/reviews/2026-08-15-storage-layer-restructuring-analysis.md \
  docs/plans/2026-08-16-storage-layer-restructuring-execution-plan.md

# The two known-dangling citations in this section
grep -nE '20\.3\.1|20\.3\.2' scripts/lint-rfc-anchors-baseline.txt
```

## Cross-references

- Mission `two-accepted-rfcs-take-their-normative-spec-from-a-gitignored-review`
  — the two untracked paths above, already recorded for this RFC
- Mission `rfc-0959-test-vectors-contain-no-expected-values` — the same
  missing-expected-value class in another document. The finding here is
  narrower and partly **contradicts** it: `octo-vault` does hold ten real pinned
  vectors, so the class is not universal
- Mission `rfc-0960-security-section-names-a-constraint-evaluator-nobody-built`
  — Threat 4, the event-log integrity claim that the unconstrained FKs bear on
- Mission `rfc-0960-closes-its-review-at-r28-and-records-sixteen-rounds` — the
  review round count this document records
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class,
  and the reason this section is worth measuring: it is the one place in the
  document where describing the substrate turned out to be accurate
