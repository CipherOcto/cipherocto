---
name: replay-defense-is-a-no-op-in-the-default-cli-configuration
description: "quota-router settle-replay opens a fresh in-memory consumed_receipt_index on every invocation unless --db-path is given, so the same envelope verifies successfully an unlimited number of times in the default configuration while printing verify OK. v1.2 withdraws this mission's own claim that the mechanism is sound. It is not: the index is keyed solely on the envelope nonce, and the nonce is an unconstrained field its author supplies, so the same economic event replays indefinitely by varying it. Measured on the built binary with a persistent --db-path, the strongest configuration available: three submissions of an identical event (same ask_id, axes_consumed, cost, timestamp, holder_did) differing only in nonce all returned verify OK with index_len growing 1, 2, 3 and zero rejections. The RFC's design assumes only the router can mint an envelope, since SettlementEnvelope has no signature field that assumption does not hold. Separately, contains_nonce and contains_settlement_hash fail open, returning not-present when the query errors."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.2"
  depends_on:
    - RFC-0959
---

# Mission `replay-defense-is-a-no-op-in-the-default-cli-configuration`

## Status

Deferred — the default is a CLI policy call and the fail-open fix needs a
breaking signature change. Neither is the auditor's to make. Evidence
measured against the real binary; the measurement probes were removed and
the tree matches its prior state.

**v1.1** withdrew a phantom section citation this mission introduced. See
the note in §Finding.

**v1.2 withdraws this mission's own claim that "the mechanism is sound".** It
is not sound against any replay that is not byte-identical, because the only
thing it indexes is a nonce the envelope's author chooses. The strongest
available configuration — a persistent `--db-path` — still accepts an
unbounded number of settlements for one economic event. See
§Replay defense keys on a field the author controls below.

## Finding

RFC-0959 §Algorithms (Receipt build) exists to stop a settlement
envelope being accepted twice, and §Data Structures defines the
`ConsumedReceiptIndex` it uses. The v1.0 status note records that as one
of the R3 criticals: _"no nonce prevents replay without consumed-receipt
state."_

> **Corrected at v1.1.** This mission originally cited "RFC-0959
> §Replay Defense". RFC-0959 has no such section — the string appears in
> no heading. The replay machinery lives in §Data Structures
> (`ConsumedReceiptIndex`, `SettlementError::ReceiptReplay`) and
> §Algorithms (`Receipt build`, `Verify receipt`), plus §Security
> Considerations under `Replay attacks`. That is a phantom citation
> introduced by this audit, corrected in
> `rfc-citations-in-rust-source-name-sections-that-do-not-exist`.

`settle_replay` creates the consumed-receipt state like this:

```rust
let repo = match db_path {
    Some(p) => ConsumedReceiptRepository::open_path(p)...,
    None    => ConsumedReceiptRepository::open_in_memory()...,
};
```

`open_in_memory()` is a brand-new database with an empty
`consumed_receipt_index`. So with no `--db-path`, every invocation starts
with no history and cannot recognise anything as already consumed.

The feature is opt-in. The insecure configuration is the default.

## Measurement — the real binary, not a test double

Same envelope file, same binary, run twice:

| Config                  | Run 1                | Run 2                                            |
| ----------------------- | -------------------- | ------------------------------------------------ |
| default, no `--db-path` | `verify: OK`, exit 0 | `verify: OK`, exit 0                             |
| `--db-path`             | `verify: OK`, exit 0 | `Error: nonce already consumed (replay)`, exit 1 |

```text
# default
settlement_hash = 1962f5fe...d80f4
nonce           = 5555...5555
index_len       = 1
verify: OK (hash matches + nonce inserted)
# identical second invocation: byte-identical output, exit 0 again
```

```text
# with --db-path
verify: OK (hash matches + nonce inserted)
Error: settle_replay failed: nonce already consumed (replay)
```

So against byte-identical resubmission the check is correct, and what is
broken in the default is that it selects a store which makes even that
vacuous.

That framing was the limit of the original finding, and v1.2 shows it was
too generous.

## Replay defense keys on a field the author controls

`ConsumedReceiptIndex` is a `HashMap<[u8; 32], ()>` keyed on the envelope's
`nonce`. `nonce` is a plain field of `SettlementEnvelope`, supplied by
whoever authors the envelope, and nothing constrains it. `settle` recomputes
`settlement_hash` over whatever JSON it is handed, so changing the nonce
yields a valid hash for an otherwise unchanged envelope.

Measured on the built binary against **one persistent store** — the strongest
configuration the CLI offers, and the one the v1.0/v1.1 measurements credit
with working:

```text
# three envelopes, identical except nonce; each pre-stamped by `settle`
verify: OK (hash matches + nonce inserted)   nonce=09..09   index_len = 1   exit=0
verify: OK (hash matches + nonce inserted)   nonce=01..01   index_len = 2   exit=0
verify: OK (hash matches + nonce inserted)   nonce=02..02   index_len = 3   exit=0
```

Same `ask_id`, same `axes_consumed`, same `cost`, same `timestamp_unix`, same
`holder_did` in all three. Three different `settlement_hash` values, because
the nonce is bound into the hash. Zero rejections. The nonce is not a replay
_key_; it is a replay _escape hatch_.

RFC-0959 does state the property it wants, in its acceptance criteria: "Same
`(event_tuple, nonce)` from same router MUST yield distinct `receipt_id`;
consumed-receipt index rejects duplicates deterministically" (G6). Note what
that guarantee is about — it is scoped to an _identical_ `(event, nonce)`
pair. It presupposes that a nonce is minted once per event by the party
authorising the settlement, and that nobody else can produce an envelope at
all.

That presupposition fails here, and it fails because of a separate finding:
**`SettlementEnvelope` has no signature field.** There is no router
signature on the envelope, so there is no party whose authority constrains
which nonces can be minted for a given event. Anyone can author an envelope
for any event and generate as many nonces as they like.

So the three defects compose into one:

1. no signature on the envelope — anyone can author one;
2. the hash binds the nonce, and `settle` recomputes it — so a fresh nonce
   is trivially self-certifying;
3. replay defense keys on that nonce — so a fresh nonce is also a fresh,
   unchallenged settlement for the same event.

Combined with the payee not being bound into the hash (mission
`settlement-hash-does-not-bind-the-cost`), each of those settlements can also
name a different recipient while carrying a hash that matches the original.

This is why "the mechanism is sound" was wrong. The mechanism is sound only
against an attacker who resubmits bytes verbatim, which is the weakest
adversary worth defending against.

## The output says the opposite of the truth

Both successful runs print:

```text
verify: OK (hash matches + nonce inserted)
```

On the second default run, a nonce _was_ inserted — into an empty
throwaway database. Nothing was checked, because there was nothing to
check against. A reader of that output has every reason to conclude
replay defense ran and passed.

This is the same shape as the trust-but-verify findings elsewhere in
this audit: an artefact whose surface asserts a guarantee no executed
code established.

The `--db-path` help text does say persistence requires it. What is
missing is the consequence — that the default provides **no** replay
protection for a feature whose entire purpose is replay protection.

## Second finding: both `contains_*` queries fail open

```rust
pub fn contains_nonce(&self, nonce: &[u8; 32]) -> bool {
    match self.db.query("SELECT 1 FROM consumed_receipt_index WHERE nonce = ? LIMIT 1", ...) {
        Ok(mut iter) => iter.next().is_some(),
        Err(_) => false,          // <-- error reads as "not consumed"
    }
}
```

`false` means "not consumed", so a database error is reported as a clean
nonce. Measured against a database whose schema was never migrated:

```text
underlying_query_is_err=true
contains_nonce_reports_not_consumed=true
hash_query_is_err=true
contains_settlement_hash_reports_absent=true
```

`contains_settlement_hash` has the identical arm.

**How far this reaches today:** less than it looks, and it should not be
overclaimed. Inside `verify_and_insert` the fail-open read is followed by
`insert(...)?`, which propagates a genuine database failure, so a total
outage still surfaces as `Err`. The fail-open only decides the answer
when the read fails and the write succeeds, or for any caller that uses
`contains_nonce` on its own. It is a latent hazard in a `pub` API rather
than a live exploit — but it is the wrong default for a security gate,
and it is the same "I/O error becomes the benign answer" shape as the
slashing-store bug fixed earlier in this audit.

## Third finding: two `# Errors` sections describe an error channel that does not exist

Both functions are documented:

```rust
/// # Errors
/// Returns `RepoError::Db` on query failure.
pub fn contains_nonce(&self, nonce: &[u8; 32]) -> bool {
```

The signature returns `bool`. There is no error channel, and rustdoc
does not lint a `# Errors` section on a non-`Result` function, so this
sits there indefinitely. The documentation promises exactly the safety
property the body lacks.

`contains_nonce`'s doc also claims it is "Used by `verify_and_insert`
and by the on-demand CLI fast-path". The CLI fast-path caller does not
exist — the only caller in the tree is `verify_and_insert`.

## What the RFC owner needs to decide

- Should `settle-replay` require `--db-path`, or refuse to claim a replay
  verdict without a persistent store? Right now the safe configuration is
  the one you have to know to ask for.
- Should the success message distinguish "checked against persistent
  state" from "checked against a fresh empty index"? A verifier that
  cannot tell those apart will over-trust it.
- Should `contains_nonce` / `contains_settlement_hash` return
  `Result<bool, RepoError>` so a query failure cannot read as "not
  present"? This is a breaking signature change on a `pub` API, which is
  why it is deferred rather than done.
- Should the legacy in-memory `settle_replay_from_json` path — which
  constructs a fresh `ConsumedReceiptIndex` per call and so can never
  detect a replay either — be removed, or made to state that it is a hash
  check only?
- Should the RFC's replay-defense requirement be restated so that a
  verifier which cannot persist state does not present itself as a
  verifier?
- **The load-bearing question, now that the nonce is known to be
  author-controlled:** what makes a nonce unforgeable? As written, nothing.
  Either the envelope gains a signature so only the settling router can mint
  one, or the replay key moves off an author-supplied field and onto
  something derived from the economic event (an RFC-0959 `receipt_id`,
  which is specified but not implemented on this surface — see
  `the-settlement-receipt-has-no-receipt-id-and-two-disjoint-stacks-exist`).
  Persisting the index, which the v1.0 and v1.1 findings are about, does not
  help by itself: a persistent index of attacker-chosen nonces still accepts
  every replay.
- G6 as written is scoped to an identical `(event_tuple, nonce)` pair and so
  cannot catch this. Should it be restated over the _event_ rather than the
  pair, or should uniqueness of nonce-per-event become an enforced
  precondition rather than an assumption?

## Reproduction

```bash
# the default opens a fresh in-memory index every invocation
grep -n -A12 'pub fn settle_replay' crates/quota-router-cli/src/commands.rs

# the fail-open arm on both queries
grep -n -A10 'pub fn contains_nonce' crates/quota-router-storage/src/consumed_receipt_repo.rs

# an Errors section on a function returning bool
grep -n -B4 'pub fn contains_nonce' crates/quota-router-storage/src/consumed_receipt_repo.rs

# the CLI fast-path caller the doc claims
grep -rn 'contains_nonce' --include=*.rs crates/

# replay end to end on the real binary
cargo build -p quota-router-cli --bin quota-router
./target/debug/quota-router settle-replay --from env.json   # OK
./target/debug/quota-router settle-replay --from env.json   # OK again, default config
./target/debug/quota-router settle-replay --from env.json --db-path /tmp/r.db   # OK
./target/debug/quota-router settle-replay --from env.json --db-path /tmp/r.db   # replay rejected
```

## Cross-references

- RFC-0959 §Data Structures — the `ConsumedReceiptIndex` contract
- RFC-0959 §Algorithms — `try_insert` returning `ReceiptReplay`
- RFC-0959 §Security Considerations — `Replay attacks`
- Mission `settlement-hash-does-not-bind-the-cost` — the other live
  settlement-integrity finding; both are in the same verification path
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the
  in-memory / persisted DAO pair that replaced `ConsumedReceiptIndex`
- `StoolapSlashStore` — the same I/O-error-becomes-benign-answer shape was
  fixed there (a swallowed store write left an in-memory ban that vanished
  on reload); the fix introduced an explicit store-error variant and a
  rollback. `contains_nonce` is the un-fixed instance of that class.
