---
name: rfc-0959-key-files-table-points-ten-rows-at-nothing
description: "RFC-0959 section Key Files to Modify lists eleven paths. Ten of them do not exist. Only crates/octo-wallet/src/bin/octo-wallet.rs resolves. Every marketplace pointer names crates/octo-core/src, but that crate's modules are identity, role, capability, routing and config, and the Ask, settlement, cache and axis code all landed in quota-router-storage instead. The sharpest row is crates/octo-core/config/pricing-axes.toml: no pricing-axes.toml exists anywhere in the tree and octo-core has no config directory, so acceptance gate G5's promise that adding an axis needs only a TOML entry and a parser bump with no code change is not merely unwired, the file it names was never created and the only axis definition in the codebase is a Rust string constant, DEFAULT_MVP_TOML, read exclusively by tests. The axis module's own doc claims it loads that file at boot, and it does not: load_from_path has zero non-test callers and the named file does not exist. Its second clause, that every axis id is validated as snake_case, is true and green, and an earlier draft of this mission wrongly repeated an error from a sibling mission by reading zero callers outside the module as no callers at all. One row also embeds an absolute machine-local path under the author's home directory in an accepted RFC."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0900
---

# Mission `rfc-0959-key-files-table-points-ten-rows-at-nothing`

## Status

Deferred. The table describes an intended layout that was never adopted; whether
to repoint it or drop it is an RFC-owner call, and creating the missing TOML is
a G5 decision already queued. Nothing was actioned.

Sixth systematic sweep of a structure RFC-0959 points at, after the roles
table, the acceptance gates, the implementation assumptions, the test vectors
and the adversary analysis.

## Finding

§Key Files to Modify is eleven rows naming the files RFC-0959 expects to create
or change. **Ten of the eleven paths do not exist.** One resolves.

| RFC names                                              | Status     | What is actually there                                 |
| ------------------------------------------------------ | ---------- | ------------------------------------------------------ |
| `crates/octo-core/src/ask.rs`                          | missing    | `quota-router-storage/src/ask.rs`                      |
| `crates/octo-core/src/settlement.rs`                   | missing    | same file — settlement types sit beside `Ask`          |
| `crates/octo-core/src/cache.rs`                        | missing    | `quota-router-storage/src/cache_key.rs`                |
| `crates/octo-core/src/axis_registry.rs`                | missing    | `quota-router-storage/src/axis_registry_toml.rs`       |
| `crates/octo-core/config/pricing-axes.toml`            | missing    | **nothing — see below**                                |
| `crates/quota-router-core/src/marketplace.rs`          | missing    | `quota-router-core/src/marketplace/` (a directory)     |
| `crates/quota-router-core/src/anti_fraud.rs`           | missing    | `quota-router-storage/src/anti_fraud.rs` (other crate) |
| `crates/octo-wallet/src/bin/octo-wallet.rs`            | **exists** | —                                                      |
| `crates/quota-router-core/src/bin/quota-router-cli.rs` | missing    | no `bin/` directory in that crate                      |
| stoolap `migrations/asks.sql`                          | missing    | `quota-router-storage/src/migrations.rs`               |
| `rfcs/draft/economics/0900-ai-quota-marketplace.md`    | missing    | it is at `rfcs/accepted/…`, promoted since             |

Five of the six missing marketplace files point into `crates/octo-core/src/`.
That crate's actual modules are `identity`, `role`, `capability`, `routing` and
`config` — none of them marketplace-related. The code did not land in the crate
the RFC nominated; it landed in `quota-router-storage`.

This is not a case of paths drifting as a codebase reorganises. The named files
were never created, in a crate that never received the work.

## The TOML row, and what it means for G5

`crates/octo-core/config/pricing-axes.toml` is named in three separate places in
RFC-0959:

- this table, as a file to create
- §Design Goals **G5**, whose metric is "adding a new axis requires only TOML
  entry + parser bump; no code change for known axes"
- §Test Vectors TV2, whose cost comment defers to "axis rates per
  `pricing-axes.toml` at test time"

**No `pricing-axes.toml` exists anywhere in the tree,** and `octo-core` has no
`config/` directory. The file was never created.

What does exist is `DEFAULT_MVP_TOML` — a `&str` constant in
`axis_registry_toml` holding the same three MVP axes. Its only consumers are two
tests. `load_from_path`, the function that would read the file, has **zero
non-test callers**, and the `load_axis_registry_from_path` alias re-exported
from the crate root has none either.

This sharpens the G5 finding already recorded. The gates mission noted that
nobody calls the loader and asked who should. The answer is that there is
nothing to call it with: **the extension mechanism G5 certifies does not exist in
any form.** The only axis definitions in the codebase are inside a Rust `const`,
so adding an axis today requires editing Rust — precisely the code change G5 says
is unnecessary.

G5 is not "unwired". It describes a file that has never existed.

## The module doc describes work it does not do

`axis_registry_toml`'s header states two things about itself:

> Loads `crates/octo-core/config/pricing-axes.toml` at boot, validates that
> every axis ID is snake_case (RFC-0959 §Data Structures: kebab-case and
> mixed-case rejected fail-closed)

The **first** clause is false. The file is not loaded at boot, and
`load_from_path` has zero non-test callers.

The **second** clause is true, and this mission's first draft got it wrong. The
loader calls the validator directly:

```rust
for ax in parsed.axes {
    if !is_snake_case(&ax.id) {
        return Err(AxisRegistryTomlError::NotSnakeCase(ax.id));
    }
```

with green tests `rejects_mixed_case_axis_id` and
`rejects_kebab_case_axis_id` covering both cases the RFC names. The mistake was
reading "zero callers outside its own module" as "no callers" — the loader is
the parser, and it lives in the same module. This correction was forced by a
sibling mission that had made the identical error.

So the doc is half-right, and the wrong half is the one about loading a file
that does not exist.

## An absolute path from someone's home directory

One row is an absolute, machine-local path:

```
/home/mmacedoeu/_w/databases/stoolap/src/storage/migrations/asks.sql
```

An accepted specification naming a file under one contributor's home directory
is unusable as a change instruction for anyone else, and the file is not at that
path in any case — the `asks` table is created from `migrations.rs`.

## What the RFC owner should do

- **Repoint the table or delete it.** It is a plan, not a record, and it has
  been mistaken for a record often enough that a second reader made the same
  wrong inference this audit did.
- **Decide G5 properly.** Either the axis registry is configuration — in which
  case `pricing-axes.toml` needs creating and `load_from_path` needs a caller
  at boot — or it is code, in
  which case G5 should stop claiming otherwise. The current state, where the
  answer is neither, satisfies neither reading.
- **Fix the first clause of the `axis_registry_toml` module doc.** It promises
  to load a file that does not exist, at boot, in a path that does not exist.
  The validation clause beneath it is accurate and should stay.
- **Make the stoolap row relative**, the way every other row is.

## Reproduction

```bash
# ten of eleven rows do not exist
for f in crates/octo-core/src/ask.rs crates/octo-core/src/settlement.rs \
  crates/octo-core/src/cache.rs crates/octo-core/src/axis_registry.rs \
  crates/octo-core/config/pricing-axes.toml \
  crates/quota-router-core/src/marketplace.rs \
  crates/quota-router-core/src/anti_fraud.rs \
  crates/quota-router-core/src/bin/quota-router-cli.rs \
  rfcs/draft/economics/0900-ai-quota-marketplace.md; do
  [ -e "$f" ] && echo "EXISTS  $f" || echo "missing $f"
done
ls crates/octo-core/src/          # identity, role, capability, routing, config

# the TOML file was never created
find . -name 'pricing-axes*.toml' -not -path './target/*' || echo absent
grep -n 'Loads .*pricing-axes.toml' crates/quota-router-storage/src/axis_registry_toml.rs

# and nothing loads it
grep -rn 'load_axis_registry_from_path' --include=*.rs crates/ | grep -v 'lib.rs' \
  || echo "no callers"
grep -n 'DEFAULT_MVP_TOML' --include=*.rs -r crates/ | grep -v axis_registry_toml.rs

# the validation clause is accurate: the loader calls the validator, and the
# two cases the RFC names are covered by green tests
grep -n 'is_snake_case(&ax.id)' -B 2 crates/quota-router-storage/src/axis_registry_toml.rs
cargo test -p quota-router-storage --lib axis_registry_toml

# where the code actually landed
find crates -name 'ask.rs' -o -name 'cache_key.rs' -o -name 'anti_fraud.rs' \
  | grep quota-router
ls -d crates/quota-router-core/src/marketplace
```

## Cross-references

- RFC-0959 §Key Files to Modify — the section swept
- RFC-0959 §Design Goals G5 — the gate this table's TOML row certifies
- Mission `four-of-rfc-0959s-ten-implementation-assumptions-fail` — IA-6, whose
  v1.1 amendment corrected the same `is_snake_case` misreading this mission
  initially repeated
- Mission `four-of-rfc-0959s-six-acceptance-gates-are-not-met` — G5 recorded as
  "who calls the loader"; answered here, nothing does and there is no file
- Mission `the-rfc-0959-role-table-is-not-a-map-of-the-implementation` — the
  same failure in the roles table: pointers into a layout that was not adopted
- Mission `ask-axis-ids-are-never-validated-against-the-axis-registry` — the
  registry whose only definition is a Rust constant
