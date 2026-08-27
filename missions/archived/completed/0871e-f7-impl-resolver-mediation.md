# 0871e-f7-impl-resolver-mediation — Resolver-node DID write mediation

**Status:** claimed (2026-08-11); substrate `DidWriteCoordinator` trait LANDED (commit `607621c6` in `octo-ident` per mission `0871e-f7-cross-instance-did-coordination`). Implementation LANDED 2026-08-11.
**Substrate:** RFC-0862 v1.3 (Accepted 2026-08-11, commit `62ed3af1`) §DidWriteCoordinator + RFC-0010 v1.3 `DidRegistry` (LANDED 2026-08-11, commit `71f8d745`)
**Parent:** `0871e-f7-cross-instance-did-coordination` (substrate mission)

## Scope

Wire `Arc<dyn DidWriteCoordinator>` into the
`octo-identity-resolver-node` crate so the `IDENTITY_REGISTER` +
`IDENTITY_REVOKE` mediation flow lives at the resolver-node boundary
(Layer C). Production HA / sharded deployments inject a concrete
`WriterElection`-backed coordinator (RFC-0862 v1.4 amendment);
single-instance deployments may legitimately leave the slot `None`
(writes are refused, fail-closed per RFC-0862 v1.3 R12).

### Mission scope (resolver-node mediation)

1. **`IDENTITY_REGISTER` + `IDENTITY_REVOKE` payload kinds** —
   UUIDs allocated in the identity sub-namespace
   (`0x0009:0001:...:0002` + `0x0009:0001:...:0003`) in
   `crates/octo-protocol/src/payload_kind.rs`. **LANDED** — 2 new
   payload kinds + 3 distinct-test vectors
   (`identity_register_uuid_matches_rfc_0862_v13`,
   `identity_revoke_uuid_matches_rfc_0862_v13`,
   `identity_payload_kinds_are_distinct`).
2. **`RegisterHandler` + `RevokeHandler`** in
   `crates/octo-identity-resolver-node/src/handlers/registration.rs`
   (NEW). Mediation flow:
   1. Validate canonical DID via
      `octo_ident::CanonicalCodec::parse(s, false)`.
   2. Decode canonical wire form → `RawDid::hash`.
   3. Consult injected `Arc<dyn DidWriteCoordinator>`. Refuse with
      `CoordinatorUnavailable` when `None` (fail-closed).
   4. Coordinator returns OK → delegate to local
      `DidRegistry::register` / `revoke`.
   5. Return `RegisterResponse` / `RevokeResponse` with the canonical
      DID + chain ID for caller observability.
   **LANDED** — 5 test vectors
   (`register_request_borsh_round_trip`,
   `revoke_request_borsh_round_trip`,
   `register_handler_refuses_without_coordinator`,
   `revoke_handler_refuses_without_coordinator`,
   `register_handler_rejects_invalid_did`,
   `revoke_handler_rejects_invalid_did`).
3. **`write_coordinator` + `chain_id` slots** on
   `IdentityResolverNodeConfig`. `write_coordinator` defaults to
   `None` (fail-closed writes); `chain_id` defaults to
   `DEFAULT_CHAIN_ID = "cipherocto-mainnet"`. Same DI shape as the
   `registry` slot. **LANDED** — `Cargo.toml` no new deps; `octo-ident`
   already a workspace dep.
4. **`IdentityResolverNode::handle_envelope`** — converted to `async`
   (mediation path requires `.await` on the coordinator). New
   dispatch arms for `IDENTITY_REGISTER` + `IDENTITY_REVOKE`. Existing
   `IDENTITY_RESOLVE` path unchanged (sync registry lookup). **LANDED**.
5. **`IdentityResolveError::Coordinator` + `CoordinatorUnavailable`
   variants** added; both tunnel to `ProtocolError::AuthorizationFailed`
   at the dispatch boundary (coordinator failures share the
   "cannot authenticate the request to write" security class with
   storage failures). **LANDED**.

### Layer discipline

Per [[cipherocto-design-principles]] §Layer discipline:
- `octo-ident` (Layer B) — `DidWriteCoordinator` trait + `ChainId` (LANDED via substrate)
- `octo-identity-resolver-node` (Layer C) — coordinator mediator (THIS MISSION)
- `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry` stays pure local persistence (no coordinator dep)
- `octo-sync` (Layer B-substrate; crate to be added per RFC-0862 v1.4 amendment) — concrete `WriterElection`-backed coordinator impl (FOLLOW-ON: `0871e-f7-coordinator-impl`)

The coordinator is injected via `Arc<dyn DidWriteCoordinator>` at
the resolver-node construction boundary. The trait is sealed
(`mod sealed { pub trait DidWriteCoordinatorSealed {} }`) so only
the substrate crate (future `octo-sync`) can implement it; downstream
crates cannot invent parallel coordinator interfaces.

### Wire form

`IDENTITY_REGISTER` request: borsh `(canonical_did: String, public_key: [u8; 32], revoked: bool)`.
`IDENTITY_REVOKE` request: borsh `(canonical_did: String)`.
`RegisterResponse` / `RevokeResponse`: borsh `(canonical_did: String, chain_id: String)`.

The wire form uses raw fields rather than embedding `DidDocument` to
keep `octo-ident::DidDocument` free of borsh derives (Layer B
substrate stays decoupled from any specific wire codec).

## Test Vectors (6 new TV, all green)

In `crates/octo-identity-resolver-node/src/handlers/registration.rs::tests`:

- `register_request_borsh_round_trip`
- `revoke_request_borsh_round_trip`
- `register_handler_refuses_without_coordinator` (fail-closed TV)
- `revoke_handler_refuses_without_coordinator` (fail-closed TV)
- `register_handler_rejects_invalid_did`
- `revoke_handler_rejects_invalid_did`

In `crates/octo-protocol/src/payload_kind.rs::tests`:

- `identity_register_uuid_matches_rfc_0862_v13`
- `identity_revoke_uuid_matches_rfc_0862_v13`
- `identity_payload_kinds_are_distinct` (3-way: RESOLVE / REGISTER / REVOKE)

Total: 9 new TV across 2 crates.

## Cross-instance TV (deferred)

Cross-instance integration TV (4 planned — atomic register, leader
failover, WAL replay, fail-closed) are DEFERRED to follow-on
mission `0871e-f7-coordinator-impl`. The mediation logic above is
already correct; the missing piece is a concrete `WriterElection`-
backed coordinator impl + multi-instance test harness.

## Validation

- `cargo fmt --all -- --check` ✓
- `cargo clippy -p octo-identity-resolver-node --all-targets -- -D warnings` ✓
- `cargo clippy -p octo-protocol --all-targets -- -D warnings` ✓
- `cargo test --lib -p octo-identity-resolver-node` ✓ (17 passed; 0 failed)
- `cargo test --lib -p octo-protocol` ✓ (60 passed; 1 PRE-EXISTING failure in `capability_payload_kinds_are_distinct`, out of scope per session memory)

## Layer direction

- `octo-ident` (Layer B) — `DidWriteCoordinator` trait + `ChainId`
- `octo-identity-resolver-node` (Layer C) — mediator (THIS MISSION)
- `octo-sync` (Layer B-substrate; crate to be added per RFC-0862 v1.4 amendment) — concrete coordinator impl
- `quota-router-storage` (Layer B-adjacent) — `StoolapDidRegistry` stays pure local persistence (no coordinator dep)

## Cross-references

- [[rfc-0010-v13-storage-extension]] — v1.3 `DidRegistry` substrate
- [[mission-0871e-f7-cross-instance-did-coordination]] — substrate mission
- [[mission-0871b-storage-backend]] — sibling mission
- [[cipherocto-design-principles]] — Layer A additive-only rule
- `rfcs/accepted/networking/0862-writer-election-bootstrap-v130.md` — RFC-0862 v1.3 (Accepted 2026-08-11)

## Follow-on

- **`0871e-f7-coordinator-impl`** — concrete `WriterElection`-backed
  coordinator impl in new `octo-sync` crate. Gated on RFC-0862 v1.4
  amendment + `octo-sync` workspace landing. Needs multi-instance
  test harness for the 4 cross-instance TV.

## Claimant

@mmacedoeu

## Pull Request

#