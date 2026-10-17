# Stoolap integration: the fork boundary

Rules for working with the embedded Stoolap database fork. Code comments cite
these as `[[stoolap-general-purpose-db]]` or `[[stoolap-fork-persistence]]`.

These rules exist because CipherOcto vendors a fork of Stoolap for its
state-machine store. The fork is a **dependency with its own history**, and the
mistake this file is written against is treating CipherOcto's schema as if it
lived inside Stoolap.

## The one rule that matters

**CipherOcto's schema stays CipherOcto-side. Stoolap's schema stays Stoolap-side.**

Migrations, tables and column types that CipherOcto owns live in
`crates/quota-router-storage/migrations/` and are applied by CipherOcto code.
They do not go into the fork, and the fork does not learn about them.

This is a fork-hygiene rule, not a style preference: a change to the fork that
carries CipherOcto's schema cannot be upstreamed, and a change to CipherOcto
that requires a fork edit makes the next upstream merge a conflict resolution
exercise instead of a rebase.

## Where things live

| Thing                                    | Home                                       |
| ---------------------------------------- | ------------------------------------------ |
| CipherOcto migrations                    | `crates/quota-router-storage/migrations/`  |
| Digest-packing and Felt conventions      | Stoolap fork (CASM / proof-system concern) |
| Storage-layer policies, schema ownership | CipherOcto                                 |

## Testing against the fork

A test may not require a running Stoolap WAL stream to exercise CipherOcto-side
logic. Bridge and adapter tests construct their inputs directly; spinning up a
fork instance belongs in integration tests that are explicitly scoped to it.

The reason is that the fork is a large surface with its own build requirements.
Requiring it for a unit test means the test suite's health depends on a component
that is not what the test is about, and a failure in either is indistinguishable
from the other at the CI boundary.

## When you must touch the fork

Touching the fork is legitimate — the fork exists to carry CipherOcto's needs.
Two conditions:

1. The change is genuinely a proof-system or digest-packing concern, not a
   CipherOcto schema concern that leaked the wrong way.
2. The change is recorded as fork-local so the next upstream merge can classify
   it.

`crates/zk-circuit/` is the home for CASM compilation, which is a proof-system
concern orthogonal to SQL. Code in that crate cites this rule by name.

## Related

- [`design-principles.md`](./design-principles.md) §No parallel abstractions —
  the general rule this is a specific instance of. Stoolap is the storage
  primitive; CipherOcto does not grow a second one alongside it.
- [`zk-stwo-cairo-executable-recipe.md`](./zk-stwo-cairo-executable-recipe.md) —
  how a Cairo circuit is compiled into an STWO proof, including the toolchain
  versions and the two undocumented traps.
