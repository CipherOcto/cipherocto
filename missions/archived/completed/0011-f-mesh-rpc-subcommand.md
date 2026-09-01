---
name: 0011-f-mesh-rpc-subcommand
description: Land `octo mesh rpc` subcommand (remote RPC invocation via RFC-0871 envelope) per RFC-0011-f §Subcommand Taxonomy
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-f author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-f
    - RFC-0855
    - RFC-0855p-b
    - RFC-0855p-c
    - RFC-0871
    - RFC-0957
    - RFC-0010
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
    - mission 0011-f-mesh-peer-subcommands
    - mission 0011-f-mesh-forward-subcommand
status: Completed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
completed_at: 2026-09-01
---

# 0011-f-mesh-rpc-subcommand — `octo mesh rpc` subcommand (remote RPC invocation)

**Status:** Open — substrate prereqs landed (RFC-0855 + RFC-0855p-b + RFC-0855p-c + RFC-0871 + RFC-0957 Accepted). No formal release gate; implementation may proceed when cross-mission ordering permits per [[feedback_initiation_user_only]] + [[git-workflow]]. The mission depends on `0011-f-mesh-peer-subcommands` (peer table for `peer_did` resolution) and `0011-f-mesh-forward-subcommand` (substrate `NodeEnvelope` construction pattern).
**Substrate:** RFC-0011-f §Subcommand Taxonomy (rpc entry), RFC-0871 envelope shape, RFC-0855 + RFC-0855p-b + RFC-0855p-c peer lifecycle hooks
**Parent:** RFC-0011-f
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root
- Mission `0011-identity-commands` — `active_signer()` for envelope signature
- Mission `0011-capability-commands` — capability caveat gating per RFC-0957
- Mission `0011-policy-commands` — `body` redaction pass extended for envelope payload bytes
- Mission `0011-f-mesh-peer-subcommands` — local peer table for `peer_did` → `peer_node_id` resolution
- Mission `0011-f-mesh-forward-subcommand` — substrate `NodeEnvelope` construction + dispatch pattern (rpc reuses `forward`'s `NodeEnvelope` construction + `HsmAdapter::sign` + `NodeTransport::send_best` path)
  **Blocks:** none (terminal mission in the mesh chain; no further cross-mission dependencies)

## Status

Open — substrate prereqs (RFC-0871 + RFC-0855 + RFC-0855p-b + RFC-0855p-c Accepted) verified per RFC-0011-f §Dependencies The lifecycle-hook RFCs (RFC-0855p-b coordinator lifecycle, RFC-0855p-c domain coordinator role) inform `TrustLevel::classify` semantics but are not consumed directly by this mission — the rpc mission uses the `octo_mesh::rpc` substrate call which delegates trust-level derivation to the peer mission.

## RFC

RFC-0011-f §Subcommand Taxonomy `octo mesh rpc` entry (rfcs/draft/process/0011-f-mesh-operations.md)

## Dependencies

See YAML frontmatter `depends_on` block above. RFC-0855p-b / RFC-0855p-c are Accepted and the lifecycle hooks are substrate-visible via the peer mission's `TrustLevel::classify` integration; this mission inherits the lifecycle hook semantics transitively. No formal release gate on RFC-0855p-b / RFC-0855p-c — those are already Accepted.

## Acceptance Criteria

- [x] `octo mesh rpc` implemented + unit-tested (TV-RPC-1..4 pass)
- [x] `octo_mesh::rpc` substrate function implemented + unit-tested (`[ADD]` #5 per RFC-0011-f §Subcommand Taxonomy)
- [x] RFC-0871 `NodeEnvelope` constructed with `payload_kind = PAYLOAD_KIND_RPC_DISPATCH` (RFC-allocated UUID per RFC-0871 §Data Structures `PayloadKindId` namespace) + `payload = borsh::serialize(&(method, params))`
- [x] Envelope signed via `HsmAdapter::sign` (same path as RFC-0871 §Algorithms "Envelope send" steps 1-5)
- [x] Request sent via `NodeTransport::send_best` + reply awaited via substrate request/reply pattern (correlation via `envelope_id`)
- [x] RFC-0010 canonical DID validation on `<PEER_DID>` arg (exit 4 on shape violation)
- [x] Method name substrate-dispatched via target's `SpecializedNode::handle_payload` (RFC-0871 §Specialized Node Lifecycle); no central enum per §Architectural Principles; unknown method → substrate `MeshError::UnknownMethod` → exit 17
- [x] `--params <JSON>` parsed as `serde_json::Value`; size clamp ≤64 KiB (RFC-0011 parser clamps pattern)
- [x] Two-step `--confirm` + `--confirm-acknowledge` gate wired per RFC-0011-f §Security Considerations + RFC-0011 §Security Considerations 1a (pastejacking defense)
- [x] `--dry-run` envelope header preview (correlation_id, target_did, method, params_hash) BEFORE signs and sends per RFC-0011-f §Subcommand Taxonomy "Dry-run" row
- [x] Request + response receipts persisted to `$OCTO_HOME/mesh/rpc-receipts.log` with redacted `params` / `response_payload` per RFC-0011-f §Subcommand Taxonomy "Side effects" row
- [x] Substrate timeout ceiling default 30s; CLI exit 20 on `RpcTimeout` per RFC-0011-f §Subcommand Taxonomy "Exit codes" row
- [x] `OctoCliError::RpcTimeout` variant implemented + unit-tested per RFC-0011-f §Error Handling
- [x] Redaction: `params` JSON MAY contain secret material depending on RPC method; redactor applies to nested secret fields per RFC-0011 §Redaction Layer
- [x] Output envelope `schema_version: 3` per RFC-0011-f §Output Envelope
- [x] Cross-mission AC: rpc command integrates with peer mission's local peer table for `peer_node_id` resolution AND forward mission's `NodeEnvelope` construction pattern
- [x] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [x] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [x] Cargo test -p octo-cli --lib --tests green
- [x] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-f type             | Sub-step                          | Notes                                                                                                                                                                           |
| --------------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rpc`                       | Sub-step 1 (substrate `[ADD]` #5) | Layer C; `octo_mesh::rpc(peer_did: &Did, method: &str, params: &serde_json::Value) -> Result<serde_json::Value, MeshError>`; constructs envelope + signs + sends + awaits reply |
| `RpcOutput`                 | Sub-step 2 (output types)         | Layer C/D; `peer_did: Did` + `method: String` + `request_envelope_id: Hex32` + `response_envelope_id: Hex32` + `response_payload: serde_json::Value` + `round_trip_ms: u64`     |
| `PAYLOAD_KIND_RPC_DISPATCH` | Sub-step 3 (RFC-allocated UUID)   | Layer 1; `PayloadKindId` UUID per RFC-0871 §Data Structures `PayloadKindId` namespace; assigned via RFC-0871 amendment process (substrate-allocated; CLI does not own the UUID) |
| `OctoCliError::RpcTimeout`  | Sub-step 4 (error variants)       | Layer C/D; `peer: String` + `method: String` + `timeout_ms: u64`; exit 20 per RFC-0011-f §Error Handling                                                                        |
| `MeshError::UnknownMethod`  | Sub-step 5 (substrate error)      | Layer C; substrate returns this for unknown `payload_kind` discriminators per RFC-0011-f §RPC Surface; CLI maps to exit 17                                                      |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Phase 7 mesh extension (rpc chapter) for Rust snippets + clap wiring patterns. Per RFC-0011-f §Key Files to Modify "SUBSTRATE" section, this mission creates `crates/octo-mesh/src/rpc.rs` and extends `crates/octo-mesh/src/lib.rs`. The CLI side extends `crates/octo-cli/src/commands/mesh.rs`, `crates/octo-cli/src/output.rs`, `crates/octo-cli/src/error.rs`, `crates/octo-cli/src/redact.rs`.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

- **No central enum for RPC methods** — per RFC-0011-f §RPC Surface + §Architectural Principles "Extension over enumeration", method names are substrate-defined via RFC-allocated `payload_kind` UUIDs (RFC-0871 §Data Structures `PayloadKindId`). The CLI does NOT carry a central enum of valid methods; new methods land via substrate additions without CLI changes. Unknown methods fail at the substrate layer with `MeshError::UnknownMethod` (CLI exit 17).
- **Payload-Kind UUID allocation** — `PAYLOAD_KIND_RPC_DISPATCH` is RFC-allocated via the RFC-0871 amendment process (substrate-allocated); the CLI does not own or define the UUID. The CLI references the substrate constant by name (no inline magic-number UUIDs).
- **`HsmAdapter::sign` integration** — same path as RFC-0871 §Algorithms "Envelope send" steps 1-5; the CLI delegates envelope construction + signing to substrate; CLI owns operator-facing flag parsing + output rendering only.
- **Request/reply correlation** — substrate maintains correlation via `envelope_id`; both `request_envelope_id` and `response_envelope_id` are surfaced in `RpcOutput` for operator audit.
- **Redaction of nested secret fields** — `params` JSON may contain `api_key`, `bearer`, etc. fields; the redactor applies per RFC-0011 §Redaction Layer (field-name redactor covers 11 names).

## Scope

Land 1 rpc subcommand per RFC-0011-f §Subcommand Taxonomy `octo mesh rpc` entry. The substrate `[ADD]` surface for this mission is function 5 of the 5 listed in RFC-0011-f §Subcommand Taxonomy (terminal function in the mesh `[ADD]` surface).

## Sub-steps

Per RFC-0011-f §Implementation Phases "Phase 7" + §Key Files to Modify:

1. **Substrate `crates/octo-mesh/src/rpc.rs` (NEW)** — `rpc()` wrapper with request/reply correlation tracking per RFC-0011-f §Key Files to Modify.
2. **Substrate `crates/octo-mesh/src/lib.rs` (EXTEND)** — export `rpc` per RFC-0011-f §Subcommand Taxonomy entry 5 (`[ADD]` surface).
3. **CLI `crates/octo-cli/src/commands/mesh.rs` (EXTEND)** — add `MeshAction::Rpc` impl per RFC-0011-f §Binary Surface.
4. **CLI `crates/octo-cli/src/output.rs` (EXTEND)** — add `RpcOutput` per RFC-0011-f §Output Envelope (`schema_version: 3`).
5. **CLI `crates/octo-cli/src/error.rs` (EXTEND)** — add `OctoCliError::RpcTimeout` per RFC-0011-f §Error Handling.
6. **CLI `crates/octo-cli/src/redact.rs` (EXTEND)** — extend pattern set for `params` JSON nested secret fields (RFC-0011 §Redaction Layer field-name table covers `api_key`, `bearer`, `secret`, etc.).
7. **Test files `crates/octo-cli/tests/mesh_rpc_*.rs` (NEW)** — 4 test vectors per RFC-0011-f §Test Vectors rpc group.
8. **Doc `docs/07-developers/octo-cli-implementation-guide.md` (EXTEND)** — Phase 7 mesh extension chapter (rpc section).

### Cargo deps

Per RFC-0011-f §Key Files to Modify "SUBSTRATE" + "CLI" sections:

```toml
# crates/octo-cli/Cargo.toml additions (CLI binding layer)
octo-protocol = { path = "../octo-protocol" }       # Layer 1 stable (RFC-0871 NodeEnvelope + PAYLOAD_KIND_RPC_DISPATCH)
octo-cap-macaroon = { path = "../octo-cap-macaroon" } # Layer B (RFC-0957 capability verification)
octo-mesh = { path = "../octo-mesh" }                # Layer C substrate (this mission)
octo-ident = { path = "../octo-ident" }              # Layer 1 stable (RFC-0010 canonical codec)
serde_json = "1"                                      # JSON parsing for params + response_payload
borsh = "1"                                           # NodeEnvelope payload canonical encoding (RFC-0871 §Determinism Requirements #1)
```

## Test Vectors (per RFC-0011-f §Test Vectors — rpc + envelope-shape groups)

4 TV (TV-RPC-1..4) — drawn from RFC-0011-f §Test Vectors sketches TV-7 (rpc-success-round-trip), TV-8 (rpc-timeout), TV-12 (rpc-method-not-registered), plus the rpc correlation match vector from the envelope-shape group.

| TV       | Group          | Sketch                                                                                                                                                                                                                                                                  |
| -------- | -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TV-RPC-1 | rpc            | `rpc-success-round-trip` — peer_a serves `quota.drain_queue` with `{queue_id:"stuck-1"}`; exit 0; `RpcOutput { peer_did, method, request_envelope_id, response_envelope_id, response_payload, round_trip_ms }`; request + response receipts persisted                   |
| TV-RPC-2 | rpc            | `rpc-timeout` — peer_a unreachable; substrate default 30s timeout → exit 20 `RpcTimeout { peer, method, timeout_ms: 30000 }`; receipt persisted with timeout status                                                                                                     |
| TV-RPC-3 | rpc            | `rpc-method-not-registered` — peer_a does NOT serve `nonexistent.method`; request envelope constructed and sent; target rejects with `ProtocolError::UnknownPayloadKind`; CLI exit 17 (`MeshError::UnknownMethod` mapped per RFC-0011-f §Error Handling)                |
| TV-RPC-4 | envelope shape | `rpc-request-response-correlation-match` — request `envelope_id` correlates to response `envelope_id` via substrate-defined correlation field per RFC-0871 §Algorithms step 6 + reply path; both `request_envelope_id` and `response_envelope_id` appear in `RpcOutput` |

## Layer direction (per RFC-0011-f §Rationale "Why Layer C/D placement" + [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `MeshAction::Rpc` dispatch + `RpcOutput` + 1 new `OctoCliError` variant + redaction pattern extension
- `octo-mesh` (Layer C) — extended with `[ADD]` `rpc` function
- `octo-protocol` (Layer 1) — REUSED via `NodeEnvelope` + `NodeTransport::send_best` + `PAYLOAD_KIND_RPC_DISPATCH` UUID; no new Layer-1 types
- `octo-cap-macaroon` (Layer B) — REUSED for capability verification (RFC-0957 `Audience` caveat); no new Layer-B types
- `octo-ident` (Layer 1) — REUSED only via `octo_ident::CanonicalCodec::parse(s, allow_legacy=false)`; no new Layer-1 types

## Validation

```bash
cargo fmt --all -- --check                                              # clean
cargo clippy --workspace --all-targets --features full -- -D warnings  # clean
cargo test -p octo-cli --lib --tests                                    # green
cargo test -p octo-mesh --lib --tests                                   # green
```

## Backward compat

- Additive only: `MeshAction` enum gains one new `Rpc` variant per RFC-0011-f §Compatibility "Additive compatibility"; no existing variant is modified
- `OutputEnvelope<T>` struct is unchanged; `data: T` parameter gains 1 new payload type (`RpcOutput`) per RFC-0011-f §Compatibility
- `OctoCliError` enum gains 1 new variant (`RpcTimeout`); existing variants unchanged per RFC-0011-f §Error Handling
- Exit-code table reserves codes 17-30 for mesh errors; 17 (shared with forward `InvalidTtlHops`) + 20 are used per RFC-0011-f §Exit Codes
- Redaction layer extended for nested secret fields in `params` JSON; the existing field-name redactor (11 names) is REUSED (no new pattern additions); `params` JSON nested fields are scanned against the existing field-name table per RFC-0011-f §Subcommand Taxonomy "Redaction" row

## Cross-references

- RFC-0011-f §Subcommand Taxonomy `octo mesh rpc` entry — primary substrate
- RFC-0011-f §Binary Surface — clap `MeshAction::Rpc` enum variant + args + flags
- RFC-0011-f §Output Envelope — `RpcOutput` payload + `schema_version: 3`
- RFC-0011-f §Error Handling — 1 new `OctoCliError` variant (`RpcTimeout`)
- RFC-0011-f §Exit Codes — codes 17 (shared) + 20 assigned
- RFC-0011-f §Roles and Authorities — `--confirm` + `--confirm-acknowledge` two-step gate + capability gating per G7
- RFC-0011-f §RPC Surface — no central enum for RPC methods rationale
- RFC-0011-f §Implicit Assumptions Audit — Peer DID canonical form + RFC-0871 envelope version + Capability present + Local peer table permissions + Mesh routing substrate reachable
- RFC-0011-f §Security Considerations — payload injection via RPC (two-step confirm gate) + envelope body leak via logs
- RFC-0011-f §Adversary Analysis A4 — RPC method probing for unhandled payload kinds
- RFC-0011-f §RFC-0008 Execution Class Mapping — `rpc` is Class B (consensus-impacting remote invocation)
- RFC-0011-f §Performance Targets — rpc round-trip local <200ms p95; cross-region <2s p95; substrate timeout default 30s per `MeshError::RpcTimeout`
- RFC-0011-f §Compatibility — additive compat, schema_version discipline, forward compatibility for new RPC methods via `payload_kind` UUIDs
- RFC-0011-f §Rationale "Why no central enum for RPC methods" — RFC-allocated `payload_kind` UUIDs discipline
- RFC-0011-f §RFC-0871 Envelope Mapping (CLI View ↔ Substrate) — `request_envelope_id` + `response_envelope_id` mappings
- RFC-0011 §Subcommand Taxonomy — parent substrate
- RFC-0011 §Security Considerations 1a — pastejacking defense (`--confirm-acknowledge` pattern)
- RFC-0011 §Redaction Layer — field-name redactor (11 names) covers `params` nested secrets
- RFC-0871 §Data Structures — `NodeEnvelope` shape + `PayloadKindId` namespace
- RFC-0871 §Algorithms "Envelope send" — steps 1-5 (sign via `HsmAdapter`)
- RFC-0871 §Algorithms step 6 — "dispatch payload to payload_kind handler" + reply path correlation
- RFC-0871 §Specialized Node Lifecycle — `SpecializedNode::handle_payload` dispatch entry point
- RFC-0871 §Security Considerations "Unknown payload kind" — fail-closed on unknown `payload_kind` discriminators
- RFC-0871 §Adversary Analysis A5 — fail-closed on unknown payload kinds (referenced via A4)
- RFC-0871 §Compatibility "additive payload_kind UUIDs" — new RPC methods land without CLI changes
- RFC-0855 — Mission Overlay Networks peer model
- RFC-0855p-b — Coordinator Lifecycle (peer lifecycle hooks; informs `TrustLevel::classify` via peer mission)
- RFC-0855p-c — Domain Coordinator Role (peer-as-coordinator case)
- RFC-0957 §Attenuation Invariant — capability `Audience` caveat binding to single `OverlayIdentity`
- RFC-0957 — Macaroon substrate (capability caveat gating for `rpc` per G7)
- RFC-0010 §2 ledger_chain_registry Table Codec — `did:octo:z<base58btc>` wire form validation
- RFC-0008 — Execution Class mapping (`rpc` = Class B per RFC-0011-f §RFC-0008 Execution Class Mapping)
- RFC-0011-d — Role Provisioning (NOT consumed by this mission; this mission is not role-gated per RFC-0011-f §Roles and Authorities)
- [[cipherocto-design-principles]] — Layer A/B stability contract

## Why 1 release cycle gate (rpc mission)

No formal release gate. RFC-0855 + RFC-0855p-b + RFC-0855p-c lifecycle hooks are substrate-visible and Accepted; this mission inherits `TrustLevel::classify` semantics transitively via the peer mission's substrate. RFC-0871 envelope shape is Layer 1 stable and Accepted. The cross-mission substrate ordering (peer mission lands before rpc mission; forward mission lands before rpc mission per `0011-f-mesh-forward-subcommand` §Substrate reuse) is the only sequencing constraint. Per RFC-0011-f §Implementation Phases "Phase 7 dependencies", RFC-0855p-b lifecycle hooks are required for full `TrustLevel::classify` wiring in the peer mission; if acceptance were delayed, rpc ships with `trust_level: Untrusted` as the only valid value for all peers (per RFC-0011-f §Implementation Phases "Phase 7 dependencies" row 1). Since RFC-0855p-b is Accepted, this fallback does not apply.

## Claimant

@unassigned
