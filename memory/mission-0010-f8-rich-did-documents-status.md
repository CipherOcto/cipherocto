---
name: mission-0010-f8-rich-did-documents-status
description: Mission 0010-f8-rich-did-documents LANDED 2026-08-11. RFC-0010 v1.5 rich DidDocument substrate: service_endpoints + controllers + verification_methods + capability_delegations. DidDocument dropped Copy + Hash. 5 TV. Schema migration v009/v010 deferred.
metadata:
  type: project
  originSessionId: c979a5ea-63a6-4b69-97ac-cd870c8a8f95
---

# Mission 0010-f8-rich-did-documents — Status (2026-08-11)

## What landed

RFC-0010 v1.5 §Rich DID Documents substrate: `DidDocument` gains the
W3C DID Core 1.0 surface beyond the v1.3 MVP minimum.

## Substrate changes (Layer B — `octo-ident`)

**New module: `crates/octo-ident/src/rich_document.rs`**
- `ServiceEndpoint { kind: String, uri: String }` — typed URI + endpoint
  kind tag. `ServiceEndpoint::new` validates: kind non-empty + ≤ 64
  chars + no control chars; URI MUST be absolute (RFC-3986 scheme).
  Bounds: `MAX_SERVICE_ENDPOINTS = 10`.
- `VerificationMethod { kind: VerificationMethodKind, public_key: [u8; 32] }`
  — multi-key DID. Bounds: `MAX_VERIFICATION_METHODS = 2`.
- `VerificationMethodKind` enum (`#[repr(u8)]`, `borsh(use_discriminant=true)`):
  - `Ed25519 = 0x01` (current baseline)
  - `Reserved = 0x00` (catch-all for future PQC per RFC-0853 §F1)
  - `as_byte()` / `from_byte()` — fail-closed round-trip (unknown
    discriminators → `Reserved`).
- `ControllerReference { did: String }` — canonical DID wire form
  reference for hierarchical delegation. Bounds: `MAX_CONTROLLERS = 3`.
- `CapabilityDelegation { token_hash: [u8; 32] }` — BLAKE3 hash of a
  `CapabilityToken` (RFC-0957) for capability delegation attestation.
  Bounds: `MAX_CAPABILITY_DELEGATIONS = 10`.
- `check_controller_cycles` — 3-color DFS (White → Gray → Black)
  cycle detection. Resolver closure stays pure (no IO coupling).
  `BTreeMap<[u8; 32], u8>` for deterministic ordering matching the
  `check_wrapped_chain` pattern in
  `crates/octo-cap-macaroon/src/macaroon.rs`.
- `ControllerCycleError` — `Cycle([u8; 32])` + `InvalidControllerDid(String)`
  + `Resolver(String)`.

**Modified: `crates/octo-ident/src/registry.rs`**
- `DidDocument` extended with 4 `Vec<>` fields:
  - `service_endpoints: Vec<ServiceEndpoint>`
  - `controllers: Vec<ControllerReference>`
  - `verification_methods: Vec<VerificationMethod>`
  - `capability_delegations: Vec<CapabilityDelegation>`
- **`Copy` + `Hash` dropped** (Vec<String> not Copy; Hash impls on Vec
  are unsound across modifications). `Default` added so callers can
  use the `..Default::default()` struct update pattern.
- v1.3 docs (just `public_key` + `revoked`) remain forward-compatible
  via `..Default::default()`.

**Modified: `crates/octo-ident/src/lib.rs`**
- New `pub mod rich_document;` + re-exports for all 4 new types +
  constants + `check_controller_cycles` + `ControllerCycleError`.

**Modified: `crates/octo-ident/src/in_memory_did_registry.rs`**
- `.copied()` → `.cloned()` for `DidDocument` (no longer Copy).
- Test fixtures use `..Default::default()`.

**Modified: `crates/octo-ident/src/write_coordinator.rs`**
- `*document` → `document.clone()` (no longer Copy).
- `sample_doc` uses `..Default::default()`.

**Modified: `crates/octo-ident/Cargo.toml`**
- Dev-dep on `borsh` (=1.5.0) for the new TV integration test
  round-trips.

**17 call-site migrations across 7 crates**: every literal-init site
of `DidDocument { public_key, revoked }` updated to include
`..Default::default()`.

## Canonical-hash invariant (preserved)

`canonical_hash(doc)` was already `BLAKE3(BINDING_DOMAIN || public_key)`
from v1.3. It hashes ONLY the public key — adding service endpoints
does NOT shift the DID identity. This matches the W3C DID Core 1.0
invariant "DID identity ≠ DID document content". TV-1 explicitly
asserts this.

## Test coverage (5 TV per RFC-0010 v1.5 §Test Vectors)

`crates/octo-ident/tests/rich_did_document_tv.rs` (NEW, gated on
`--features borsh`):
- TV-1 rich_document_round_trip — full DidDocument with all 4 v1.5
  fields populated round-trips through borsh; canonical_hash stable
  across rich-field updates.
- TV-2 controller_cycle_rejected — `check_controller_cycles` detects
  A → B → A 2-node cycle via 3-color DFS.
- TV-3 capability_delegation_hash_verifies — 2 distinct
  `CapabilityDelegation` token_hashes round-trip via borsh.
- TV-4 service_endpoint_uri_absolute_only — relative URIs
  (`/foo/bar`, `foo/bar`, `example.com`, empty, `://no-scheme`)
  rejected; absolute URIs (`http://`, `https://`, `cipherocto://`)
  accepted.
- TV-5 verification_method_type_discriminator — `Ed25519` (0x01) +
  `Reserved` (catch-all) round-trip via `as_byte` / `from_byte`; future
  PQC kinds land in `Reserved`.

Plus 14 new unit tests in `rich_document.rs::tests` covering individual
constructors + validation rules.

## Layer discipline (per [[cipherocto-design-principles]])

- `octo-ident` (Layer B) — substrate extension. Trait surface UNCHANGED:
  `DidRegistry::register` / `resolve` / `revoke` / `list` all still
  consume/return `DidDocument` (now extended). The 17 call-site updates
  are mechanical (`..Default::default()`) — no semantic change.
- `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry` source
  code migrated but schema unchanged. JSON columns for the new fields
  land in migration v009 + v010 (separate mission).
- `octo-identity-resolver-node` (Layer C) — consumer updates; resolve
  now returns rich DidDocument, but no handler behavior change.
- `octo-sync` (Layer B-substrate) — `EncodedDidDocument::encode()`
  serializes via borsh which now encodes the extended shape. Tests
  pass (encode → decode round-trip).

## Validation snapshot

| Check | Result |
|-------|--------|
| `cargo build -p octo-ident -p octo-identity-resolver-node -p octo-sync -p quota-router-storage` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p octo-ident -p octo-identity-resolver-node -p octo-sync -p quota-router-storage --all-targets -- -D warnings` | clean |
| `cargo test --lib -p octo-ident` | 65/65 pass (51 prior + 14 new rich_document unit tests) |
| `cargo test --test rich_did_document_tv -p octo-ident --features borsh` | 5/5 pass |
| `cargo test --test chain_namespace_tv -p octo-ident` | 10/10 pass (no regression) |
| `cargo test --lib -p octo-identity-resolver-node` | 24/24 pass (no regression) |
| `cargo test --test cross_domain_chain -p octo-identity-resolver-node` | 7/7 pass (no regression) |
| `cargo test --lib -p octo-sync` | 225/225 pass (no regression) |
| `cargo test --lib -p quota-router-storage` | 181/181 pass (no regression) |

Total: 536 tests pass.

## Implementation gotchas (for follow-on v1.5 work)

- `DidDocument` is **no longer `Copy` or `Hash`** — Vec<String> drops
  both. All `*doc` / `.copied()` sites need `.clone()`.
- `borsh` `#[derive]` on enum with `#[repr(u8)]` requires
  `#[borsh(use_discriminant = true)]` — otherwise the derive fails
  with "You have to specify use_discriminant".
- `VerificationMethodKind::Reserved.as_byte() == 0x00` ALWAYS — the
  source byte is not preserved; `from_byte` collapses all non-Ed25519
  bytes to the canonical `Reserved` variant (fail-closed).
- `check_controller_cycles` uses `BTreeMap` for color tracking (NOT
  `HashMap`) for deterministic ordering across runs.
- `ServiceEndpoint::validate_uri` accepts any RFC-3986 scheme starting
  with ASCII alpha + (alphanumeric | `+` | `-` | `.`) followed by
  `:`. Does NOT validate scheme is registered — accepts any URI shape
  that looks like an absolute URI. Tighter scheme allowlists are a
  consumer concern.
- `canonical_hash` already only hashes `public_key` — TV-1 asserts
  this. Adding any rich field does NOT shift the DID identity.

## Follow-on missions

- `0010-f8-rich-did-storage` — `StoolapDidRegistry` migration v009 +
  v010 with JSON columns for the 4 v1.5 fields. Layer B-adjacent schema
  migration (separate mission per Layer discipline).
- `0010-f8-rich-did-resolution` — `ResolveHandler` + cross-domain
  chain handlers consume the rich fields; service endpoint discovery
  via the new URIs; controller hierarchy traversal via the cycle-checked
  resolver.
- RFC-0010 v1.5 acceptance — RFC document needs to be filed + accepted
  per [[feedback_initiation_user_only]]. Substrate is ready.

## How to apply

- `DidDocument { public_key, revoked, ..Default::default() }` is the
  new minimum literal-init pattern. Old `DidDocument { public_key,
  revoked }` no longer compiles (missing 4 fields).
- For new rich-field tests, gate the file on `--features borsh` (the
  dev-dep `borsh` provides the encode/decode surface).
- For new verification methods, always go through `VerificationMethod::ed25519(pk)`
  or `VerificationMethod::new(VerificationMethodKind::Reserved, pk)`.
  PQC kinds land in `Reserved` for now; v2.0 adds new variants.
- For cycle detection, use `check_controller_cycles(&hash, resolver)`
  with a resolver closure that wraps `Arc<dyn DidRegistry>::resolve`.
  Pure function; no IO coupling.