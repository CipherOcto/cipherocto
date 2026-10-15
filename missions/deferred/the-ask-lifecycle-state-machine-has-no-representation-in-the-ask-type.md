---
name: the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type
description: "RFC-0959 specifies an 8-state Ask lifecycle with a transition table giving determinism, side effects and signing requirements per transition, but the Ask type has no state field and no AskState type exists. Only Expired and Pruned are represented at all (Active implicitly, as presence in the index); Published, Selected, Settled and Revoked have none. Measured: select_ask honours exactly two of the machine's gates (expiry and budget); the Published -> Active soft_delay warm-up has no implementation and cannot have one, because Ask does not carry published_at; and the Settled -> Revoked transition's stated side effect ('future select_ask skips') is unimplementable because nothing records a revoke. Measured on the built binary: of the four subcommands the RFC's CLI Operator row names, only publish exists — list, show and revoke all exit 2. Actioned: insert passed u64::MAX as the expiry cutoff, so the 100,001st insert deleted the entire index instead of trimming to the cap; the existing prune_cap_evicts_oldest_first test passes under that mutation because 100 inserts never reach the branch."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0009
---

# Mission `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`

## Status

Deferred — representing lifecycle state changes the on-disk Ask wire form
(and therefore `ask_id`, since the id is a hash over the canonical payload),
and `Revoked` needs a mechanism the RFC does not yet specify: an unsigned
"do not select" entry cannot be trusted if any peer can add one, so a
revocation envelope needs its own signature and storage. Both are RFC-owner
design decisions, not marketplace-local fixes. One real defect and three
documentation defects were **actioned**; see §Actioned.

## Finding

RFC-0959 §Ask state machine specifies a lifecycle with eight states and a
transition table that, for each transition, names the trigger, whether it is
deterministic, its side effects, and what signs it. The `Ask` type has **no
field recording which state an Ask is in**, and no `AskState` / `AskLifecycle`
type exists anywhere in the crate.

The whole `Ask` struct:

```rust
pub struct Ask {
    pub asker_did: AskerDid,
    pub model: ModelRef,
    pub rates: ModelRateTable,
    pub nonce: [u8; 16],
    pub expires_at_unix: u64,
}
```

Five fields, none of which is a state. So the machine is not
_mis-implemented_ — it is **unrepresentable**, and each transition has to be
satisfied by an ad-hoc proxy. Two of those proxies exist (`expires_at_unix`
for `Expired`, physical removal for `Pruned`); the rest have none.

### Transition-by-transition

| Transition                         | Proxy in the data                | Faithful?                                                                  |
| ---------------------------------- | -------------------------------- | -------------------------------------------------------------------------- |
| `Draft -> Published`               | `AskSigned` exists               | No — `Draft` is off-chain by design, so this is right                      |
| `Published -> Active` (soft_delay) | **none**                         | No — see §The warm-up gate that does not exist                             |
| `Active -> Selected`               | `select_ask` returns the ask     | No — a return value is not a reservation; nothing records the selection    |
| `Selected -> Settled`              | settlement receipt in the ledger | Partly — recorded against the _receipt_, never against the `Ask`           |
| `Settled -> Active`                | **none**                         | No                                                                         |
| `Active -> Expired`                | `expires_at_unix < now_unix`     | Yes                                                                        |
| `Settled -> Revoked`               | **none**                         | No — the RFC names the mechanism and it does not exist                     |
| `Expired/Revoked -> Pruned`        | `prune_expired` / `remove`       | Yes for `Expired`; unreachable for `Revoked` because there is no `Revoked` |

## The warm-up gate that does not exist

The transition table's side effect for `Published -> Active` is "Visible to
`select_ask()`". `select_ask` honours exactly two of the machine's gates:

1. **Expiry** — `prune_expired(now_unix)` runs first.
2. **Budget ceiling** — `dqa_cmp(cost, budget_ceiling) > 0` skips.

There is no soft-delay filter, and none is possible: `Ask` does not carry
`published_at_unix`. That field lives on `AskUnsignedPayload`, the pre-mint
form, and the post-mint `Ask` the index holds drops it. The same structural
gap already forced a documented no-op one function over — `jurisdiction`
matching, where the source comment says the post-mint form "does not carry
jurisdiction ... so `jurisdiction_matches()` returns true for all entries".
The soft-delay gate is a second instance of the same loss, and unlike
jurisdiction it is a **security-relevant** gate: the RFC calls it
"router warm-up", and its absence means an Ask is selectable the instant it
enters the index.

## `Settled -> Revoked` cannot be implemented as specified

The transition table gives the trigger and the side effect:

| From    | To      | Trigger                                      | Side Effects                                   | Signing                               |
| ------- | ------- | -------------------------------------------- | ---------------------------------------------- | ------------------------------------- |
| Settled | Revoked | Asker `octo-wallet ask revoke --ask-id <id>` | Index marks revoked; future `select_ask` skips | Revoke envelope signed by IdentityKey |

Three problems, in increasing order of severity:

1. **The command does not exist.** Measured below.
2. **Nothing records `Settled`,** so there is no way to know which asks are
   eligible for the `Settled -> Revoked` transition. The transition names a
   _source state_ that the data type cannot represent.
3. **The side effect has no substrate.** "Index marks revoked; future
   `select_ask` skips" requires a revoke flag the index can filter on.
   `select_ask` filters on expiry and budget — plus jurisdiction, though that
   filter is already inert for the same structural reason (the post-mint
   `Ask` drops the field, so `jurisdiction_matches` returns true for every
   entry). It never consults a revocation marker, because no such marker is
   ever written.

Problem 3 is the one that survives even if the CLI is added tomorrow: adding
`ask revoke` without adding state would produce a command that reports
success while `select_ask` keeps returning the revoked ask.

This matters more than the usual "unwired facade" framing because revocation
is the asker's only **self-help** remedy. Every other marketplace control
here is about what a _router_ will accept. Revocation is about what a _seller_
can escape: an asker who publishes a badly-priced Ask and realises it cannot
withdraw it except by waiting out the TTL it chose — the RFC itself notes TTL
is Asker-chosen, "max recommended 30 days".

## Measured — the built binary

`octo-wallet ask --help` lists one subcommand:

```text
$ octo-wallet ask --help
Usage: octo-wallet ask <COMMAND>

Commands:
  publish  Publish a signed Ask (RFC-0959 §Roles; mission 0959-a AC)
  help     Print this message or the help of the given subcommand(s)
```

The other three, each invoked directly:

```text
$ octo-wallet ask revoke --ask-id 00
error: unrecognized subcommand 'revoke'
exit=2

$ octo-wallet ask list
error: unrecognized subcommand 'list'
exit=2

$ octo-wallet ask show --ask-id 00
error: unrecognized subcommand 'show'
exit=2
```

So of the four subcommands RFC-0959 §Roles names for the `CLI Operator`
("human invoking `octo-wallet ask publish/list/show/revoke`"), three are not
implemented.

Note that these exit **2**, not 1 — a clean clap rejection, not a crash and
not a silent success. That is the right behaviour for a missing command and
worth saying plainly: nothing here fails open.

## Measured — publish reaches nothing

The surviving subcommand terminates at stdout. A successful publish, with a
32-byte seed file and a correct 32-hex nonce:

```text
$ octo-wallet ask publish --node-type wholesale --model openai/gpt-4 \
    --ttl-unix 1900000000 --seed ./seed.bin \
    --axes input_tokens_per_1k:30000 --jurisdiction US \
    --nonce-hex 42424242424242424242424242424242 > ask.json
exit=0
```

stdout is a single `{"ask_id": [...], "payload": {...}, "signature": [...]}`
envelope. The invocation **created no files** (beyond the shell redirect),
opened no database, and took no address — there is no `--out`, no
`--db-path`, and no peer or daemon flag on the subcommand. It does not touch
`MarketplaceIndex`, and it does not call `AskRepository::put`.

That is the same unwired gap mission
`marketplace-index-is-a-parallel-unwired-abstraction` records for the index
from the consumer side. This mission measures it from the **producer** side,
which is the half that matters for the lifecycle: the state machine has no
transition into it either.

## Actioned in this mission

### A real defect: the over-cap insert emptied the whole index

`insert` has no clock argument, so its over-cap branch passes a sentinel as
the expiry cutoff. It passed `u64::MAX` — which to `prune_expired`, whose
predicate is `expires_at_unix < now_unix`, matches **every** ask in the
index. So the 100,001st insert did not trim the index to the cap; it deleted
all 100,000 entries and returned the index empty.

Measured on three year-2100 asks (none expired by any wall-clock standard), so
the only thing that can remove an entry is the count cap:

```text
removed=3 remaining=0
```

Fixed to pass `0`, which is the opposite sentinel: `expires_at_unix < 0` is
never true for a `u64`, so the time filter is disabled and only the count cap
applies. This is the documented intent — `insert` deliberately has no clock,
and expiry is applied by `select_ask` — so the fix removes no capability.

Verified by mutation: reintroducing `u64::MAX` fails
`over_cap_insert_evicts_one_not_everything` (`left: 0, right: 3`) while every
other test in the module keeps passing.

**The existing `prune_cap_evicts_oldest_first` did not catch this, and still
does not.** It inserts 100 asks against a 100,000 cap, so `insert` never
reaches the over-cap branch; it then calls `prune_expired(0)` and asserts
nothing was removed. It passes under the mutation above. Its own comment
admits the batch was reduced "to verify the eviction logic" — but it verifies
no eviction logic at all. It was a check over a population it never reached,
with a name describing behaviour it does not exercise.

To make the branch reachable at all, `ACTIVE_ASK_CAP` became a private
`cap` field on the index, defaulting to the same constant, plus a
test-only `with_cap` constructor. This changes no public signature: `new()`
still yields an index capped at `ACTIVE_ASK_CAP`, and no production caller
exists.

Moving the cap into a field created a trap that is worth recording, because
it is the kind of thing a later change reintroduces silently. A derived
`Default` sets `cap: 0`, and `Default` is a **public** trait impl — so
`MarketplaceIndex::default()`, reachable from any crate, would have produced
an index that prunes on every single insert and never retains anything. The
derive was removed and `Default` now delegates to `new()`.
`default_cap_matches_production_cap` pins this, and fails under the mutation
(`left: 0, right: 100000`) — the control is behavioural as well as
structural, asserting that a default-built index actually retains an ask.

Two regression tests were added. `over_cap_insert_evicts_one_not_everything`
pins the fix, with distinct ascending TTLs so "earliest expiry" is a strict
ordering rather than a `min_by_key` tie. `over_cap_prune_still_honours_a_real_clock`
is its control: it calls `prune_expired_and_overflow` with a genuine clock
and asserts the stale ask still goes, so the bug cannot be reintroduced as
"always pass a real clock" — which would look like a fix while silently
moving expiry into the hot insert path.

### Documentation defects

Three doc comments asserted guarantees the code does not provide:

- `select_ask`'s doc said "Select the cheapest **active** Ask", implying a
  state gate it does not apply. It now names the two gates it does apply,
  and states that the `soft_delay` gate is absent and why it cannot be
  present.
- The module doc's "Cache invalidation" paragraph claimed over-cap inserts
  "trigger FIFO pruning by `published_at_unix` ASC". `Ask` does not carry
  `published_at_unix`, and the eviction is by smallest `expires_at_unix`.
- The `indexed` field's doc claimed "`published_at_unix` is part of the
  value so FIFO pruning can sort efficiently without a separate
  `Vec<AskId>`". The value is a bare `Ask`, which has no such field — the
  sort is a linear `min_by_key` over the whole map.

None changed behaviour; all three cost a reader real time to discover.

The lifecycle gaps themselves are **not** actioned.

## Why this was not "just wired up"

The obvious fix — add a `state` field and a `revoke` subcommand — is three
decisions that belong to the RFC owner, and each has a cost that is easy to
underestimate:

1. **Adding a state field to `Ask` changes `ask_id`.** The id is
   `BLAKE3(canonical_ser(payload))`. A mutable state field cannot live in the
   hashed payload, so it needs a side table keyed by `ask_id`, and every
   consumer has to read both. This is the same shape as the jurisdiction loss
   above, and it is why the soft-delay gate could not simply be added.
2. **A revocation entry is an authenticated write.** If revocation is a row
   an index simply skips on, then anyone able to write that row can silence
   any competitor's Ask. The RFC names the signing requirement ("Revoke
   envelope signed by IdentityKey") but does not specify the envelope's
   position in the sync or storage model.
3. **The order of transitions is a policy choice.** If `Settled -> Revoked`
   is genuinely available only from `Settled`, then an Ask that has never
   been selected cannot be revoked at all — which is the case an asker most
   wants (published, never used, mis-priced). The state machine as drawn
   cannot express "withdraw an unused Ask". Revocation likely needs an
   `Active -> Revoked` edge as well, and that is an RFC amendment.

Point 3 is the one I would not have predicted from reading the RFC, and it is
the reason this is a design question rather than a missing function.

## What the RFC owner needs to decide

- Where does lifecycle state live, given `ask_id` is a hash over the payload
  and must stay stable as state changes? Side table, or accept an
  un-hashed field in a wrapper struct?
- Is `Revoked` a state on `Ask`, or a separate envelope the index consumes?
  If separate, what authenticates it and how does it propagate under the
  RFC-0862 sync model?
- Should `Active -> Revoked` be added so an unused Ask can be withdrawn? As
  drawn, revocation is only reachable from `Settled`.
- Should `Published -> Active`'s soft_delay gate be implemented, and if so
  does `Ask` need `published_at_unix` restored to the post-mint form?
- Should RFC-0959 gain a §CLI section stating which subcommands exist? The
  §Roles row currently names four and three are absent, and the phantom
  `§CLI` citations this audit already corrected were code that believed such
  a section existed.
- Should the three missing subcommands be implemented, or should the §Roles
  row be narrowed to the surface that exists?

## Reproduction

```bash
cargo build -p octo-wallet --bin octo-wallet

# the RFC names four subcommands for the CLI Operator role
grep -n 'octo-wallet ask publish/list/show/revoke' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# the state machine
grep -n -A14 '#### Ask state machine' \
  rfcs/accepted/economics/0959-ask-settlement-chain.md

# only publish exists
./target/debug/octo-wallet ask --help
./target/debug/octo-wallet ask revoke --ask-id 00; echo "exit=$?"
./target/debug/octo-wallet ask list; echo "exit=$?"
./target/debug/octo-wallet ask show --ask-id 00; echo "exit=$?"

# Ask has no state field, and no AskState type exists
grep -n -A12 'pub struct Ask {' crates/quota-router-storage/src/ask.rs
grep -rn 'AskState\|AskLifecycle\|enum.*Revoked' --include=*.rs crates/

# select_ask gates on expiry and budget only — no revoked, no soft_delay
grep -n -A50 'pub fn select_ask' crates/quota-router-storage/src/marketplace.rs

# soft_delay is named nowhere in Rust — the gate has no implementation
grep -rn 'soft_delay' --include=*.rs crates/

# the actioned defect: insert's over-cap branch and its sentinel
grep -n -B12 'prune_expired_and_overflow(0)' \
  crates/quota-router-storage/src/marketplace.rs

# the regression and its control
cargo test -p quota-router-storage --lib \
  marketplace::tests::over_cap_ -- --nocapture

# the cap-as-field trap: Default must not yield cap == 0
cargo test -p quota-router-storage --lib \
  marketplace::tests::default_cap_matches_production_cap

# the pre-existing test that does not reach the branch it is named for:
# 100 inserts against a 100_000 cap never trip `count > cap`
grep -n -A18 'fn prune_cap_evicts_oldest_first' \
  crates/quota-router-storage/src/marketplace.rs

# the only references to the index outside its own file
grep -rn 'MarketplaceIndex' --include=*.rs crates/ \
  | grep -v 'src/marketplace.rs'
```

## Cross-references

- RFC-0959 §Ask state machine — the specification this mission measures
- RFC-0959 §Roles — the `CLI Operator` row naming four subcommands, and the
  `Marketplace Index` row whose state machine this one extends
- RFC-0009 — the IdentityKey that the revoke envelope is specified to be
  signed with
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the same
  unwired index measured from the consumer side; this mission measures the
  missing producer
- Mission `the-marketplace-facade-has-no-production-caller` — why neither
  index nor this state machine is on a live path today
- Mission `raising-an-asks-rate-silently-leaves-the-old-price-in-the-book` —
  supersession semantics, the adjacent unresolved question about withdrawing
  a live listing
- Mission `the-settlement-path-records-but-never-settles` — why nothing
  records the `Selected -> Settled` transition against an `Ask`
- Mission `ask-publish-fails-with-its-own-default-nonce` — the one other
  defect in this command that is live rather than latent, and the reason the
  measurements above need explicit flags
