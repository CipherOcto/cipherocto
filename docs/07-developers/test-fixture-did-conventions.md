# Test Fixture DID Conventions

> **Always use `sample_did(seed)` from `octo_ident::test_helpers` to mint test DIDs; never introduce bare-name `did:octo:*` literals.**

## Why

Bare-name literals (e.g., `"did:octo:alice"`, `"did:octo:node-1"`) carry semantic content that the canonical W3C wire form (`"did:octo:z<base58btc>"`) explicitly rejects. The test-helper minting path is the load-bearing primitive for the codemod's invariant.

## Usage

```rust,ignore
use octo_ident::test_helpers::sample_did;

let alice = sample_did(42);
assert!(alice.starts_with("did:octo:z"));
```

For a typed `WireDid` value:

```rust,ignore
use octo_ident::test_helpers::sample_wire;

let alice = sample_wire(42);
```

## Determinism

`sample_did(seed)` returns a byte-stable DID for a given seed. Two calls with the same seed return equal bytes. Different seeds return different DIDs. Use any `u8` seed (0..=255); SHA-256-derived.

## Recommended Patterns

Prefer named const seeds for readability:

```rust,ignore
const ASKER_SEED: u8 = 1;
const HOLDER_SEED: u8 = 2;
const PROVIDER_SEED: u8 = 3;

let asker = sample_did(ASKER_SEED);
let holder = sample_did(HOLDER_SEED);
let provider = sample_did(PROVIDER_SEED);
```

## Forbidden Patterns

```rust,ignore
// ❌ Bare-name literal — flag AC #5 violation
let did = "did:octo:alice";
assert_eq!(m.asker_did, "did:octo:alice");
```

Use `let did = sample_did(42);` instead.

## JSON Fixtures

JSON test fixtures (e.g., `crates/octo-wallet/tests/fixtures/capability-zk/*.json`) carry bare-name DIDs as named-holder fixtures. These are out of scope for the `.rs` codemod (mission 0010-b) and migrate separately when their owning mission lands (e.g., 0958-a ZK Capability Circuit).

## Related

- `crates/octo-ident/src/test_helpers.rs` — `sample_did` + `sample_wire` implementation
- `crates/octo-ident/src/lib.rs:21` — `pub mod test_helpers;`
- RFC-0010 §Data Structures — canonical W3C wire form
- Mission 0010-b — codemod audit log
