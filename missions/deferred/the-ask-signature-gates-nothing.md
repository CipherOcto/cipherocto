---
name: the-ask-signature-gates-nothing
description: "The Ask signature is produced but never enforced anywhere in production. AskSigned::verify has zero production callers - its only two call sites are in crates/octo-cli/tests/security_regressions.rs and crates/octo-wallet/tests/ask_publish_cli.rs. AskSigned appears in non-test code only where the wallet constructs it and prints it to stdout. The signature is then structurally absent from every downstream boundary: AskWrite::put and Marketplace::put both take a bare Ask, the Ask struct has no signature field, and the asks table (migration v001, with no later ALTER TABLE asks) has no signature column. So an Ask is stored, indexed, priced and selected on the strength of an unsigned asker_did string. The same migration family stores router_signature and registrant_signature as NOT NULL with length checks, so the omission is an outlier rather than a house style."
metadata:
  node_type: mission
  type: research
  created: 2026-10-15T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0959
    - RFC-0009
---

# Mission `the-ask-signature-gates-nothing`

## Status

Deferred — carrying a signature past the CLI requires a schema change (the
`asks` table), a trait change (`AskWrite::put` / `Marketplace::put` take a
bare `Ask`), and a decision about whether the router verifies on ingest or on
use. All three are RFC-owner calls. The hazard is documented on
`AskWrite::put`, where the signature is dropped.

This is the synthesis of the identity thread: mission
`a-signed-ask-does-not-prove-who-published-it` found that `verify` does not
bind the DID to the key. This mission finds that `verify` is never called at
all.

## Finding

The wallet signs every Ask. Nothing checks the signature, and the signature is
structurally dropped at every boundary after it.

Five independent layers, each sufficient on its own:

| Layer               | What it is                                                                                  | Signature?                     |
| ------------------- | ------------------------------------------------------------------------------------------- | ------------------------------ |
| `octo-wallet`       | builds `AskSigned` via `sign`, serialises to stdout                                         | produced                       |
| `AskSigned::verify` | the only verification function                                                              | **never called in production** |
| `AskWrite::put`     | storage write trait — takes `&Ask`                                                          | **absent from the signature**  |
| `Ask` struct        | `asker_did`, `model`, `rates`, `nonce`, `expires_at_unix`                                   | **no field**                   |
| `asks` table (v001) | `ask_id`, `asker_did`, `model`, `rates_json`, `nonce`, `expires_at_unix`, `created_at_unix` | **no column**                  |

### The verifier has no production caller

Every reference to `AskSigned` outside test directories and its own module:

```text
crates/octo-wallet/src/bin/octo-wallet.rs        # constructs (sign) + prints
crates/quota-router-storage/src/lib.rs           # the pub re-export
```

Both call sites for `verify` are test files: `crates/octo-cli/tests/security_regressions.rs`
and `crates/octo-wallet/tests/ask_publish_cli.rs`.

The security-regression test deserves a note, because it looks like coverage
and is not. It finds `fn verify(` and asserts the **parameter type** of
`asker_public_key` is a slice rather than a fixed-width alias. It is a
linter-shaped check that no longer has anything to report; it never calls
`verify` and never establishes that anyone does.

### The signature has nowhere to survive to

`AskWrite::put(&self, ask: &Ask)` and `Marketplace::put(&self, ask: &Ask)` both
take the unsigned struct. `Ask` has no signature field, so there is nothing to
forward even if a caller wanted to. The `asks` table (migration v001) has no
signature column, and no later migration adds one — no `ALTER TABLE asks`
exists anywhere in the tree.

So the terminal state is: an Ask is persisted, indexed, priced and selected
on the strength of an unsigned `asker_did` string. `asker_did` is what
`list_by_asker`, reputation, slashing and escrow key on.

### This is an outlier, not a house style

The same migration family stores signatures, and enforces them:

```sql
-- v004, settlement_events
router_signature    BLOB NOT NULL,

-- v017, policy registry
operator_signature  BLOB NOT NULL,
CHECK (length(operator_signature) = 64),
registrant_signature BLOB NOT NULL,
CHECK (length(registrant_signature) = 64)
```

`settlement_events` is the table for the settlement path, which is to say the
adjacent subsystem made the opposite choice deliberately. Nothing in the Ask
schema reflects that Asks were intended to be signed at all.

## The binding check exists only inside a test

Because `verify` does not bind the DID to the key, `ask_publish_cli.rs`
performs the check by hand as a "sanity" step:

```rust
let derived_did = format!("did:octo:b{}", hex::encode(verifying_key.to_bytes()));
assert_eq!(
    signed.payload.asker_did, derived_did,
    "asker_did must match derived DID from seed"
);
```

Two things follow from that snippet.

1. It is the only place in the tree where the property is asserted at all.
2. It **re-implements the CLI's derivation by hand** instead of calling a
   shared function, so the test and the CLI share a copy of the format rather
   than a definition. If the derivation is corrected to the canonical form
   (mission `a-signed-ask-does-not-prove-who-published-it`), this assertion
   has to be found and updated by hand, and nothing points at it.

It also cannot detect the impersonation case it is nominally guarding: it
compares the wallet's output against the wallet's own derivation, so it holds
exactly when the wallet is honest. A producer that is not the wallet is
outside what this test can see.

## Why this was not simply wired

Verification has to happen somewhere, and each candidate placement is a
different decision:

- **On ingest** (`put` verifies before writing) is the strongest, but `put`
  takes a bare `Ask` with no signature, so it needs the schema and trait
  change first. It also fails closed for a publisher whose key is not yet
  known to the router.
- **On use** (`select_ask` / settlement verifies before honouring) needs the
  signature to reach the index, which means the same schema change, plus the
  question of what an unverifiable Ask does to selection.
- **At the router** (the RFC-0862 sync layer verifies on receipt) is the
  natural home for a signature check in a federated design, and is where
  `consumed_receipt_repo` style trust boundaries usually sit. But that layer
  has no Ask ingest today.

None of these can be chosen from the code. The RFC names the signed envelope;
it does not say which side verifies.

## What the RFC owner needs to decide

- Where is the Ask signature verified — on ingest, on use, or at the sync
  boundary? The three have different availability and trust properties.
- Does the `asks` table gain a signature column, or is verification purely
  transient (verify on arrival, store the bare Ask)? The first is
  auditable after the fact; the second is smaller but loses the evidence.
- If stored, is `asker_did` re-derived and checked at that point, closing both
  this mission and the identity-binding gap in one place?
- Should `Marketplace::put` and `AskWrite::put` accept `AskSigned` instead of
  `Ask`? That is a breaking change across every caller, but it is the only
  option that makes the signature structurally unavoidable rather than
  optional.
- Is there a reason Asks were exempt from the signature-column treatment
  given to `settlement_events` and the policy registry? If yes, it is not
  written down anywhere this audit can find.

## Reproduction

```bash
# the verifier's only two call sites, both tests
grep -rn 'AskSigned' --include=*.rs crates/ \
  | grep -v '/tests/' | grep -v 'src/ask.rs'

# the signature is absent from the write boundary
grep -n -A4 'fn put(&self, ask: &Ask)' crates/quota-router-storage/src/ask_repo.rs
grep -n 'pub fn put(&self, ask: &Ask)' crates/quota-router-core/src/marketplace/mod.rs

# Ask carries no signature field
grep -n -A12 'pub struct Ask {' crates/quota-router-storage/src/ask.rs

# the table has no signature column, and none was ever added
cat crates/quota-router-storage/migrations/v001__create_asks_table.sql
grep -rn 'ALTER TABLE asks' crates/*/migrations/*.sql

# the contrast: signatures ARE stored elsewhere
grep -rn 'router_signature\|registrant_signature' \
  crates/quota-router-storage/migrations/*.sql

# the hand-rolled binding check, test-only
grep -n -A6 'asker_did must match derived DID' \
  crates/octo-wallet/tests/ask_publish_cli.rs

# the security-regression check that looks like coverage but is a type check
grep -n -A14 'were the two sites' crates/octo-cli/tests/security_regressions.rs
```

## Cross-references

- Mission `a-signed-ask-does-not-prove-who-published-it` — the same thread one
  step earlier: `verify` does not bind the DID to the key. This mission finds
  `verify` is never called
- RFC-0959 §Roles — the signed-Ask model and the `seller_signature ≡ Asker
signature` claim nothing currently enforces
- RFC-0009 §Verification — the format check (a) DID format, (b) signature
  validity, that the Ask path skips entirely
- Mission `the-ask-lifecycle-state-machine-has-no-representation-in-the-ask-type`
  — the Ask carries no state and no binding; this is the same structural
  absence on the storage side
- Mission `the-settlement-path-records-but-never-settles` — the adjacent
  subsystem, whose `router_signature` column _is_ `NOT NULL`, is exactly the
  counter-example
- Mission `ask-publish-fails-with-its-own-default-nonce` — the command that
  produces these signatures
