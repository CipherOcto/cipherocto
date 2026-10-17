---
name: the-rfc-0959-role-table-is-not-a-map-of-the-implementation
description: "RFC-0959 section Roles exists under a Nothing should be implied rule stating every actor affecting correctness, security, accountability or consensus MUST be named. Sweeping its nine rows against the tree, four name code paths that do not exist or are wrong: the Settlement Engine and Consumed Receipt Index rows both point at crates/octo-core/src/settlement.rs, and that file is absent from a crates/octo-core that contains only capability, config, identity, lib, role and routing, while ConsumedReceiptIndex actually lives in quota-router-storage. The Anti-Fraud Monitor row names quota-router-core/src/anti_fraud.rs and the file is in quota-router-storage. And the CLI Operator row names four subcommands where one exists. The consequence for an auditor is concrete: every severe finding in this thread lives in the code the table does not point to, so using the specification's own organising structure to navigate finds nothing."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0009
---

# Mission `the-rfc-0959-role-table-is-not-a-map-of-the-implementation`

## Status

Deferred — correcting the role table is an RFC edit, and RFCs are the owner's
document. This mission proposes the corrections; it does not make them.
Nothing in the code was changed.

## Finding

RFC-0959's §Roles coverage table is the document's organising structure. Four
of its nine rows name code that is not where they say it is, and two name a
file that does not exist.

### The table's own rule

The table is introduced by:

> **"Nothing should be implied" rule (specification layer): every actor
> affecting correctness, security, accountability, or consensus MUST be
> named.

That is the strongest possible statement about what the table is for. It is
the place a reader is entitled to look first, and it is the place this audit
looked.

### Row-by-row

| Role                     | What the table names                                                                             | Reality                                                                                                                 |
| ------------------------ | ------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| `Asker`                  | publish, sign, **revoke before ttl_unix**                                                        | publish ✓ (broken by default), sign ✓, revoke ✗ — no subcommand                                                         |
| `Role Binding`           | RFC-0971 role binding                                                                            | `role_binding.rs` exists; `Asker` absent from `octo-role`                                                               |
| `Router`                 | marketplace index, capability verify, settlement hash, receipt build, consumed-index maintenance | capability verify ✓ (`ingress/authenticator`), settlement hash ✓, receipt build ✓ but **no production caller**, index ✗ |
| `Provider`               | external                                                                                         | external — nothing to implement                                                                                         |
| `Marketplace Index`      | rebuilt on RFC-0862 sync event                                                                   | no sync adapter exists anywhere; no lifecycle states                                                                    |
| `Settlement Engine`      | **`crates/octo-core/src/settlement.rs`**                                                         | **file absent**                                                                                                         |
| `Consumed Receipt Index` | **`crates/octo-core/src/settlement.rs::ConsumedReceiptIndex`**                                   | **module absent**; the type is in `quota-router-storage`                                                                |
| `Anti-Fraud Monitor`     | `crates/quota-router-core/src/anti_fraud.rs`                                                     | file is in `quota-router-storage`; zero production callers                                                              |
| `CLI Operator`           | `octo-wallet ask publish/list/show/revoke`                                                       | only `publish`; the other three exit 2                                                                                  |

### The settlement rows are the serious ones

Two roles carry the entire settlement integrity story, and both point at a
file that does not exist:

```text
$ ls crates/octo-core/src/
capability.rs  config.rs  identity.rs  lib.rs  role.rs  routing.rs

$ find crates -name settlement.rs
(no results)
```

`crates/octo-core` exists. It has six modules and none of them is settlement.
`ConsumedReceiptIndex` is defined at
`crates/quota-router-storage/src/ask.rs`.

This is a different failure from the Anti-Fraud Monitor row, which names the
wrong _crate_ for a file that exists. Here the _file_ is absent, and so is any
module path a reader would follow.

### Why this matters more than a stale link

This is not cosmetic drift. The severity calibration of this entire audit
depends on a reader being able to find the settlement code, and the RFC's own
table does not take them there.

Every severe finding in this thread lives in `quota-router-storage`:
the settlement hash that omits `cost` and both parties, the replay index
keyed on an author-controlled nonce, the absent `receipt_id`, the envelope
whose `axes_consumed` cannot carry a `cache_key_hash`. A reader who starts
from §Roles, as the document instructs, opens `crates/octo-core` and finds
nothing.

The miscitations and the missing controls compound each other. The controls
are absent **and** the index that would have said so is wrong, so neither the
specification nor a quick navigation surfaces them. That is why this thread
took measurement rather than reading: the table cannot be used as a map.

## A related shape, one level down

The `Ask` data structure has the same property, which is a stronger signal
than a single stale path. RFC-0959 §Data Structures defines one `Ask`; the
tree has two, `AskUnsignedPayload` with eight fields and `Ask` with five, and
the narrower one is what everything reads.

So the drift is not confined to paths. It is the same phenomenon at two
levels:

- the **table** points at modules that do not exist;
- the **type** the table refers to has diverged from the type the code uses.

Both are the signature of a specification that was written complete and an
implementation built bottom-up from types, with the prose contracts retained
and the wiring never following. That is the mechanism behind most of this
audit's findings, and it is worth stating once here rather than leaving each
mission to rediscover it.

## What the RFC owner should do

- Repoint the `Settlement Engine` and `Consumed Receipt Index` rows at
  `quota-router-storage`, or restore a `crates/octo-core/src/settlement.rs`
  if one was intended. The first is a documentation fix; the second is a
  decision about where the settlement substrate belongs.
- Repoint the `Anti-Fraud Monitor` row to `quota-router-storage`.
- Decide whether the `CLI Operator` row should name only the subcommands that
  exist, or whether the missing three are a gap to close. At present the row
  reads as a description of the CLI and is not one.
- Decide whether §Roles should carry a "lives in" column that is checked, or
  whether code paths belong in docs at all. The table currently mixes roles
  with file paths, which is what let it drift silently — `Provider` names no
  path because it is external, and nothing marks that distinction.
- If the table keeps paths, is there a check that they resolve? This audit
  deliberately did not build one: an earlier citation classifier here was
  wrong three times, and path-existence is the easy case. A note in the
  mission conventions saying "paths in §Roles are illustrative" would be
  cheaper and honest.

## Reproduction

```bash
# the table and its rule
grep -n -B4 'Nothing should be implied' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md
grep -n -A16 '### Role/Authority Coverage Table' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# the two settlement rows name a file that does not exist
grep -n 'crates/octo-core/src/settlement.rs' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md
ls crates/octo-core/src/
find crates -name settlement.rs

# the type's real home
grep -rn 'pub struct ConsumedReceiptIndex' --include=*.rs crates/

# the Anti-Fraud Monitor row names the wrong crate
grep -n 'anti_fraud.rs' rfcs/accepted/economics/0959-ask-settlement-chain.md
find crates -name anti_fraud.rs

# the CLI Operator row
grep -n 'octo-wallet ask publish/list/show/revoke' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md
./target/debug/octo-wallet ask --help

# the type-level version of the same drift
grep -n -A12 'pub struct Ask {' crates/quota-router-storage/src/ask.rs
grep -n -A14 'pub struct AskUnsignedPayload {' crates/quota-router-storage/src/ask.rs
```

## Cross-references

- RFC-0959 §Roles — the table this mission measures against
- RFC-0959 §Data Structures — the single `Ask` the tree splits in two
- Mission `documentation-that-describes-a-mechanism-nobody-built` — the class
  this is the navigational face of: accurate citations, absent mechanisms
- Mission `the-marketplace-index-has-no-rebuild-path-and-no-staleness-check` —
  the `Marketplace Index` row, whose RFC-0862 driver has no adapter
- Mission `the-settlement-receipt-has-no-receipt-id-and-two-disjoint-stacks-exist`
  — the `Settlement Engine` and `Consumed Receipt Index` rows, and the code
  they fail to point at
- Mission `a-signed-ask-does-not-prove-who-published-it` — the `Asker` row's
  identity authority, unenforced
- Mission `the-marketplace-has-no-stake-gate-and-the-role-it-would-gate-on-is-not-in-the-role-registry`
  — the `Asker` row's stake authority, with the role absent from the registry
- Mission `rfc-citations-in-rust-source-name-sections-that-do-not-exist` —
  the same drift in § names rather than paths
