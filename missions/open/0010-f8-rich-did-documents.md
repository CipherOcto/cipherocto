# 0010-f8-rich-did-documents — Rich DID Document surface

**Status:** unassigned (wave 5; absorbed from RFC-0010 §Storage Extension §Out of scope on 2026-08-10)
**Substrate:** RFC-0010 v1.3 `DidDocument` (32-byte public_key + revoked flag)
**Parent:** RFC-0010 v1.3 §Storage Extension §Out of scope

## Scope

Extend `DidDocument` (RFC-0010 v1.3 §Storage Extension §Data
Structures) with the W3C DID Core 1.0 surface beyond the MVP minimum:

1. **Service endpoints** — `Vec<ServiceEndpoint>` for resolver
   discovery (`homepage`, `inbox`, `capability-registry`).
2. **Controller references** — `Vec<CanonicalDid>` for hierarchical
   DID delegation (parent → child). Cycles rejected (per
   `check_wrapped_chain` cycle detection pattern).
3. **Capability delegation proofs** — `Vec<CapabilityDelegation>`
   embedding a BLAKE3 hash of a `CapabilityToken` (RFC-0957) so the
   DID Document attests to delegated capabilities without
   duplicating the wire form.
4. **Verification methods** — `Vec<VerificationMethod>` for
   multi-key DID (Ed25519 + PQC future). Type discriminator via
   RFC-0853 §F1 hooks.

### RFC amendment (v1.5)

In-place additive amendment to RFC-0010 (mirrors v1.2 → v1.3 → v1.4
pattern):

- Extend `DidDocument` struct with the 4 fields above
- Add `ServiceEndpoint`, `VerificationMethod`, `CapabilityDelegation`
  types to `crates/octo-ident/src/`
- §Future Work F8 added with this mission pointer; F4-F7 status
  unchanged from v1.3 / v1.4

### Migration impact

`StoolapDidRegistry` schema (migration v008) needs extension to
migration v009 + v010:

- **v009** — add `service_endpoints` JSON column + `controllers`
  JSON column + `verification_methods` JSON column
- **v010** — add `capability_delegations` JSON column

JSON columns (rather than separate tables) because:

- Service endpoints / controllers / verification methods are
  read-together with the DID Document
- Per-DID cardinality is bounded (≤ 10 endpoints / ≤ 3 controllers
  per W3C DID Core 1.0 best practice)
- Joins would add latency for the resolver hot path

### Why this is its own mission (not folded into 0871b-storage-backend)

0871b-storage-backend ships the MVP minimum `DidDocument`
(`public_key` + `revoked`). Rich documents are an additive extension
that:

- Touches `octo-ident` types (breaking-ish — needs `#[serde(default)]`
  on all new fields for backward compat)
- Touches `quota-router-storage` schema (two new migrations)
- Has W3C conformance implications (service endpoint URI validation)

The mission split keeps 0871b-storage-backend small (≤ 10 TV) and
lets rich documents ship after the substrate is stable.

## Test Vectors (preview)

- 5 new TV: register-rich-document-round-trip; controller-cycle-
  rejected; capability-delegation-hash-verifies-against-token;
  service-endpoint-uri-validation (rejects non-absolute URIs);
  verification-method-type-discriminator (Ed25519 + future PQC).

## Layer direction

- `octo-ident` (Layer B) — `DidDocument` extension + new types
- `quota-router-storage` (Layer B-adjacent) — schema migrations
  v009 + v010 + `StoolapDidRegistry` updates
- `octo-identity-resolver-node` (Layer C) — consumer updates
  (resolve returns rich document; cross-domain hops pass through
  the rich fields)

## Validation

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```

## Cross-references

- [[rfc-0010-v13-storage-extension]] — v1.3 substrate
- [[mission-0871b-storage-backend]] — substrate mission (DAG predecessor)
- [[mission-0010-f2-multi-chain-did-resolution]] — chain-keyed
  documents (rich documents compose per-chain)
- [[cipherocto-design-principles]] — Extension over enumeration

## Claimant

@unassigned

## Pull Request

#
