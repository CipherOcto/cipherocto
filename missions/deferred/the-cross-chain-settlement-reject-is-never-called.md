---
name: the-cross-chain-settlement-reject-is-never-called
description: "verify_settlement_chain_match implements the RFC-0959 cross-chain settlement reject and is covered by nine tests across two suites, but it has zero production callers. The live settle-replay path accepts an envelope naming a vault that does not exist and a chain_id that could never match a real vault row, reporting verify OK. Measured on the built binary. Both the module doc and the cost_vault_id field doc assert that a marketplace caller wires it. Separately TV-0959-11 through 15 assert a tautology rather than pinning the derivation they claim to pin."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0957
    - RFC-0010
---

# Mission `the-cross-chain-settlement-reject-is-never-called`

## Status

Deferred — wiring an existing verifier into a live path is an owner
decision about where the vault substrate comes from at settlement time,
and the migration gate it would enable changes what the CLI accepts. The
two documentation claims that assert the wiring exists are corrected
here; no behaviour changed. Measurement probes removed, tree matches its
prior state.

This is the clearest instance of this audit's standing theme: a
well-built component that looks like a guarantee, and two documents that
say so in the present tense, with nothing connecting it to production.

## Finding

`quota_router_storage::settlement_verify::verify_settlement_chain_match`
implements RFC-0959's three-step algorithm correctly:

1. `cost_vault_id` absent → `CostVaultIdMissing`
2. no vault row at that key → `CostVaultIdMissing`
3. `vault.chain_id != envelope.chain_id` → `ChainMismatch`

It takes `&dyn octo_cap_macaroon::VaultLookup`, the same trait that backs
capability verify-time, with no shadow impl. That is careful work.

**It has no production callers.** Every call site in the tree is a test:

```text
settlement_verify.rs            -> own #[cfg(test)] module
tests/tv_0959_settlement_wire.rs -> TV-0959-16..25
```

The live path does not reach it. `ConsumedReceiptRepository::verify_and_insert`
performs exactly three steps — recompute hash, check nonce, insert — and
`SettlementEnvelope::verify` (in-memory) performs two: hash, replay. Neither
looks at `cost_vault_id` or `chain_id` beyond hashing them.

## Measurement

An envelope whose `cost_vault_id` is `[0xDE; 32]` — a vault that exists
nowhere — and whose `chain_id` is `[0xAD; 32]`, through the real binary:

```text
$ quota-router settle-replay --from xchain.json
settlement_hash = 25b8ac23c327e26908e1b398e2272bd2267d231dc80ca7faa48744df9451fd60
index_len       = 1
verify: OK (hash matches + nonce inserted)
exit=0
```

If the verifier were wired, step 2 alone would reject this as
`CostVaultIdMissing`. It reports success.

Both fields are in the envelope hash preimage, so the hash stays
self-consistent and step 1 passes too. Nothing downstream objects.

## Two documents assert the wiring exists

`settlement_verify`'s module doc, in the present tense:

> "The **marketplace caller** instantiates `OctoVaultLookup::new(substrate)`
> and passes it as `&dyn VaultLookup` to this verifier."

and again under `# Layer discipline`:

> "The **marketplace caller** wires `OctoVaultLookup::new(substrate)` as the
> `&dyn VaultLookup` arg."

There is no marketplace caller. `OctoVaultLookup` appears eight times
across `quota-router-storage`, `quota-router-core` and
`quota-router-cli` — and **every one of the eight is a doc comment inside
`settlement_verify.rs` itself**. No code in any of those crates names it;
outside its own crate it appears only in that crate's tests.

And on the field itself, `SettlementEnvelope::cost_vault_id`:

> "When `Some`, the settlement-time verifier **MUST** find a vault row at
> this id (via `VaultLookup`) and that row's `chain_id` **MUST** match
> `chain_id` below."

That verifier exists, is correct, and is never invoked. The MUST has no
enforcement anywhere in the tree.

## The TV suite's appearance of coverage

`tv_0959_settlement_wire.rs` presents as 25 byte-exact RFC test vectors.
Families 4 and 5 (TV-16..25) do real work against the real function.

**Family 3, TV-0959-11..15, does not.** Its stated purpose is to "pin the
byte-exact mapping" of `cost_vault_id` to
`octo_vault::vault_id_unchecked(chain_id, owner_did, asset_id)`. What it
actually does:

```rust
let expected = vault_id_unchecked(chain_id, owner_did, asset_id);
let env = SettlementEnvelope { ..., cost_vault_id: Some(expected.0), ... };
assert_eq!(env.cost_vault_id.expect("vault_id"), expected.0, "...");
```

It assigns `expected.0` into the field and then asserts the field equals
`expected.0`. **This assertion cannot fail.** No production derivation of
`cost_vault_id` is called, because none exists. The comment says the bytes
were "captured at TV authoring time via a one-shot `cargo run`", but no
byte literal is pinned in the file, so there is nothing to regress
against — change the derivation and all five tests still pass.

So the suite advertises five vectors covering a derivation, and contains
zero coverage of one.

## What this composes with

Three findings now sit on the same live settlement-verification path, and
each was measured independently:

| Step                             | Implemented?                     | Enforced on the live path?                 |
| -------------------------------- | -------------------------------- | ------------------------------------------ |
| Amount is bound by the hash      | No — `cost` absent from preimage | No, and it is the only integrity mechanism |
| Replay detection                 | Yes                              | Vacuous by default (no `--db-path`)        |
| Vault row exists + chain matches | Yes, correctly                   | Never called                               |

The end state: a settlement can be verified with an arbitrary amount,
replayed any number of times in the default configuration, and drawn
against a vault that does not exist on a chain that never was.

## What the RFC owner needs to decide

- Where does the settlement-time verifier get its `&dyn VaultLookup`? The
  CLI currently opens a `ConsumedReceiptRepository` and has no vault
  substrate, which is probably why the call was never made.
- Should `verify_and_insert` require a vault lookup, making the dependency
  mandatory? That is what makes the `cost_vault_id` MUST real.
- Should a settlement with `cost_vault_id: None` continue to verify? Today
  it does, and `None` is the default on every envelope the CLI produces —
  so the migration gate described in the verifier is never reached.
- Should TV-0959-11..15 be rewritten to pin an actual expected digest, or
  deleted? As written they cost five test names and buy no coverage.
- Should the module doc stop asserting a caller that does not exist?

## Reproduction

```bash
# the verifier and its only callers
grep -rn 'verify_settlement_chain_match' --include=*.rs crates/

# the doc claiming a marketplace caller wires it
grep -n 'marketplace caller' crates/quota-router-storage/src/settlement_verify.rs

# the MUST on the field
grep -n -B2 -A4 'settlement-time verifier' crates/quota-router-storage/src/ask.rs

# the live path never mentions vault or chain
grep -n -A12 'pub fn verify_and_insert' crates/quota-router-storage/src/consumed_receipt_repo.rs

# the tautology
grep -n -A20 'fn tv_0959_11_cost_vault_id_equals_vault_id_unchecked' \
  crates/quota-router-storage/tests/tv_0959_settlement_wire.rs

# end to end on the real binary
cargo build -p quota-router-cli --bin quota-router
./target/debug/quota-router settle-replay --from xchain.json   # verify OK, exit 0
```

## Cross-references

- RFC-0959 §Settlement-Time Vault Row Lookup + §Cross-Chain Settlement
  Reject — the algorithm that is implemented and uncalled
- RFC-0959 §Wire Format v2.0 — the fields whose enforcement is claimed
- RFC-0957 §Verify-Time Extension — the `VaultLookup` trait being reused
- Mission `settlement-hash-does-not-bind-the-cost` — step 1 of the same
  path; the amount is not bound
- Mission `replay-defense-is-a-no-op-in-the-default-cli-configuration` —
  step 2; the replay check is vacuous by default
- Mission `marketplace-index-is-a-parallel-unwired-abstraction` — the same
  parallel-unwired shape, there in the read path
- Mission `the-marketplace-facade-has-no-production-caller` — the standing
  caveat. It is the closest sibling to this finding, and the difference
  matters: that facade is unwired because nothing calls it, whereas this
  verifier is unwired **inside** a path that is called.
