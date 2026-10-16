---
name: rfc-0959-a1-cites-a-transaction-guarantee-section-that-does-not-exist
description: "RFC-0959-A1's implicit assumptions audit cites RFC-0862 section Transaction guarantees as the mitigation for IA-2, whose blast radius is partial-state delivery, and for IA-6, whose blast radius is a settlement chain fork. RFC-0862 has no such section: its top-level headings contain no transaction or atomicity section, and the document explicitly disclaims the property by stating that it introduces no new transaction model. The assumption is very likely true, since the Stoolap fork provides MVCC transactions, so this is a wrong-citation finding rather than a missing-guarantee finding, and the fix is to cite the substrate. Also recorded: A1's key-files table still lists a settlement chain path that its own implementation mission already documented as nonexistent, four of A1's hard-block dependencies are now Accepted while the table records two as Draft, and five accepted RFCs embed an absolute path under one contributor's home directory."
metadata:
  node_type: mission
  type: research
  created: 2026-10-16T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959-A1
    - RFC-0862
    - RFC-0957-A1
---

# Mission `rfc-0959-a1-cites-a-transaction-guarantee-section-that-does-not-exist`

## Status

Deferred. Every item is a citation or table correction in accepted RFCs; none
is mine to make, and none is a code change.

Eighth systematic sweep, and the first to leave the base RFC-0959 for its
amendments.

## Finding

RFC-0959-A1 §Implicit Assumptions Audit lists eight assumptions. Two of them —
the two that carry the atomicity the whole delivery design rests on — name the
same mitigation:

| IA   | Assumption                                               | Blast radius if false                       | Mitigation as written            |
| ---- | -------------------------------------------------------- | ------------------------------------------- | -------------------------------- |
| IA-2 | Stoolap transaction is atomic                            | **Partial-state delivery**                  | RFC-0862 §Transaction guarantees |
| IA-6 | Settlement chain tip is reachable inside the transaction | Cannot compute `prev_chain_hash` atomically | RFC-0862 §Transaction guarantees |

**RFC-0862 has no §Transaction, and no section on transaction atomicity at
all.** Its top-level headings are Status, Authors, Maintainers, Related RFCs,
Summary, Review Trail, Dependencies, Design Goals, Motivation, Roles and
Authorities, Specification, Performance Targets, Implicit Assumptions Audit,
Security Considerations, Adversarial Review, Adversary Analysis, Compatibility,
Test Vectors, Alternatives Considered, Implementation Phases.

Worse than absence, the document disclaims the property. In §Motivation, on
what it reuses from Stoolap:

> Reuses existing Stoolap primitives. No new WAL format; no new snapshot
> format; **no new transaction model.**

RFC-0862 is a data-sync protocol. It is the wrong document to cite for
transaction atomicity, and it explicitly says it introduces none.

## What RFC-0862 does say about transactions

Three things, none of which is the guarantee A1 needs:

- uncommitted transactions are not shipped, and a `Rollback` marker discards
  entries on the reader — a property of _sync_, not of transaction atomicity;
- LSN ranges are captured from Stoolap's commit hook;
- snapshot segments are written to a temp file and atomic-renamed, which the
  threat table logs as row 17, "Snapshot atomicity".

The third is the closest thing, and it is about a filesystem rename on
snapshots. It is not the statement that a multi-step delivery either commits
wholesale or not at all — which is what IA-2's "Partial-state delivery" means
and what A1's TV2 and TV3 rollback vectors exist to test.

## The assumption is probably true, and that is the finding

The Stoolap fork is described by RFC-0862 itself as a complete embedded SQL
database **with MVCC transactions**. So the property IA-2 relies on is very
probably available in the substrate.

That makes this a mis-citation rather than a missing guarantee, and the
distinction matters for how the owner should spend effort. It is not "build
transaction atomicity into the delivery path" — that may already hold. It is
"cite the thing that provides it, so the next reader can check." As written,
the audit trail for the delivery layer's central safety property points at a
section that does not exist, which means nobody checking it can confirm
anything. A reader who tries and fails may reasonably conclude the guarantee is
missing, and go looking to build it.

That failure mode — a citation that cannot be resolved being read as a control
that is absent — is the same one this audit has now found in three documents,
in three forms: `crates/octo-core/src/settlement.rs`, `ReceiptReplay` as an
error variant, and now §Transaction guarantees.

## A1's key-files table contradicts its own implementation mission

A1 §Key Files lists five paths. Two exist. Of the three that do not:

```text
crates/octo-wallet/src/capability/deliver.rs       MISSING
crates/octo-wallet/src/capability/stoolap_txn.rs   MISSING
crates/quota-router-core/src/settlement/chain.rs    MISSING
```

The third is the interesting one, because the implementation mission already
caught it. `0959-b-market-delivery-impl` records, on its own first open item:

> **NOT** `crates/quota-router-core/src/settlement/chain.rs` as original mission
> text claimed — that file path does not exist on disk; actual settlement chain
> lives in `crates/quota-router-storage/src/ask.rs`

So the implementers found the phantom path and wrote the correction into the
mission. The RFC table still lists it as a file to MODIFY.

**And the mission then contradicts itself.** Forty lines further down, the same
file has a §Location block that lists all five phantom paths copied verbatim
from A1's table, including:

```text
- `crates/quota-router-core/src/settlement/chain.rs` (MODIFY) — `append_deal_settled` + chain hash
```

The correction is in the checklist; the uncorrected copy is in the header. So
the phantom path exists in three places — the RFC, the mission header, and the
mission's own corrected checklist — and only the third one is right.

The two NEW files that do not exist — `deliver.rs` and `stoolap_txn.rs` — are
the delivery algorithm and its atomic wrapper, which is the scope of the still
open `0959-b2-deliver-algorithm`. That is honestly tracked, and this table is
consistent with the open work rather than contradicted by it.

## Two corrections in the owner's favour

Worth stating, since this audit has mostly found absences:

**A1's hard-block dependencies are all met.** The table records RFC-0853 and
RFC-0957-A1 as Draft, each with `Hard-block on RFC-0959-A1 acceptance? YES`.
Both are now in `rfcs/accepted/`, along with RFC-0903 and RFC-0955. Whatever the
sequence, the two declared hard-blocks are satisfied.

**A1 disambiguates the RFC-0862 numbering collision by path**, citing
`rfcs/accepted/networking/0862-stoolap-data-sync.md` rather than the bare
number — which is the right response to two files claiming one number, and
better than most of the tree manages.

## Five accepted RFCs embed an absolute home path

Not an A1 problem, but it surfaced while checking A1's dependency table and it
is systemic rather than incidental:

```bash
grep -rl '/home/mmacedoeu/_w/' --include=*.md rfcs/accepted/
```

RFC-0958, RFC-0862, RFC-0850ab-c, RFC-0959 and RFC-0912. In RFC-0959's
key-files table it appears as the instruction to modify a stoolap migration; in
RFC-0862 it appears as the location of the Stoolap fork itself, which is load
bearing — that RFC's entire subject is a fork living outside this repository, and
it identifies it by a path under one contributor's home directory. A relative
pointer plus a fork URL would be the same information and would survive the
contributor moving machines.

## What the RFC owner should do

- **Re-cite IA-2 and IA-6** to whatever actually guarantees transaction
  atomicity — the Stoolap substrate, RFC-0200, or a new statement in RFC-0862.
  Whichever it is, the point is that a reader must be able to resolve it.
- **Apply the mission's chain-path correction to A1 §Key Files,** or delete the
  table per the parallel finding on the base RFC.
- **Refresh the dependency status column** in A1 and the base RFC. Both record
  dependencies as Draft that are now Accepted, which erodes the value of a
  column whose whole purpose is telling a reader what is still outstanding.
- **Replace absolute home paths in the five RFCs that carry them,** RFC-0862
  first.

## Reproduction

```bash
# the citation, twice
grep -n 'RFC-0862 §Transaction guarantees' \
  rfcs/accepted/economics/0959-a1-market-delivery.md

# and the section it names
grep -nE '^#{2,4} ' rfcs/accepted/networking/0862-stoolap-data-sync.md \
  | grep -iE 'transact|atomic' || echo "no transaction or atomicity section"

# what RFC-0862 does say, including the disclaimer
grep -n 'no new transaction model' rfcs/accepted/networking/0862-stoolap-data-sync.md

# the hard-blocks, now satisfied
for r in 0853 0957-a1 0903 0955; do
  find rfcs/accepted -iname "$r-*.md" -printf '%f\n' | head -1
done

# A1's key files, and the correction already in the tree
for f in crates/octo-wallet/src/capability/deliver.rs \
         crates/octo-wallet/src/capability/stoolap_txn.rs \
         crates/quota-router-core/src/settlement/chain.rs; do
  [ -e "$f" ] && echo "EXISTS $f" || echo "missing $f"
done
# the correction the implementers made, and the uncorrected copy of it
# that survives 40 lines later in the same file
grep -n 'chain.rs' missions/claimed/0959-b-market-delivery-impl.md

# the absolute-path pattern
grep -rl '/home/mmacedoeu/_w/' --include=*.md rfcs/accepted/
```

## Cross-references

- RFC-0959-A1 §Implicit Assumptions Audit — the section swept
- Mission `the-rfc-0959-role-table-is-not-a-map-of-the-implementation` — the
  base RFC's roles table pointing at a file that does not exist
- Mission `rfc-0959-key-files-table-points-ten-rows-at-nothing` — the same
  key-files defect on the base RFC, and the recommendation to delete both tables
- Mission `rfc-0959-test-vectors-contain-no-expected-values` — the same
  unresolvable-citation pattern in the base RFC's test vectors
- Mission `0959-d-market-delivery-roll-up-is-green-on-work-measurement-contradicts`
  — open, and already recording that A1's GREEN roll-up contradicts measurement;
  IA-2 and IA-6 are the assumptions that roll-up leaned on
- Mission `rfc-0959-claims-a-determinism-verification-that-does-not-exist` —
  A1 §Determinism violations has its own verification claim, unswept
