---
name: 0011-f-mesh-peer-subcommand
description: Land `octo mesh peer` subcommands (list/add/remove) per RFC-0011-f §Subcommand Taxonomy
metadata:
  node_type: substrate-cli
  type: cli-substrate-extension
  originSessionId: RFC-0011-f author session
  created: 2026-08-31
  v: "1.0"
  depends_on:
    - RFC-0011-f
    - mission 0011-core-output-envelope-redaction
    - mission 0011-identity-commands
    - mission 0011-capability-commands
    - mission 0011-policy-commands
  release_gate:
    require: "RFC-0011-d Phase 1 reached Accepted"
    released_version: TBD
status: Open
---

# 0011-f-mesh-peer-subcommand — `octo mesh peer` subcommands (list/add/remove)

**Status:** Open — release-gated on RFC-0011-d Phase 1 reaching Accepted (the role-provisioning substrate that gates `peer add` / `remove` fleet-wide per RFC-0011-f §Roles and Authorities "Role-provisioning rationale"). Implementation kickoff user-gated per [[feedback_initiation_user_only]] + [[git-workflow]] once the gate clears.
**Substrate:** RFC-0011-f §Subcommand Taxonomy (peer list/add/remove), RFC-0855 peer model, RFC-0010 canonical DID codec
**Parent:** RFC-0011-f
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root (per RFC-0011 §Binary Surface)
- Mission `0011-identity-commands` — `active_signer()` exposure for DID wire form validation
- Mission `0011-capability-commands` — capability caveat dispatch (no capability required for `peer list`/`add`/`remove` per RFC-0011-f §Roles and Authorities)
- Mission `0011-policy-commands` — `body` redaction pass shared via `OctoCliRedactor`
  **Blocks:** `0011-f-mesh-forward-subcommand`, `0011-f-mesh-rpc-subcommand` (cross-mission substrate ordering; mesh peer is the substrate the forward/rpc missions depend on for `peer_did` resolution)

## Status

Open — release-gated. RFC-0011-d Phase 1 (role-provisioning substrate) must reach Accepted before `peer add` / `remove` ship. Per RFC-0011-f §Roles and Authorities "Role-provisioning rationale", the CLI surfaces mutating peer operations with `--confirm` only in v1.0; RFC-0011-d adds role-gated admission as Phase 1 follow-on.

## RFC

RFC-0011-f §Subcommand Taxonomy `octo mesh peer list` / `octo mesh peer add` / `octo mesh peer remove` entries (rfcs/draft/process/0011-f-mesh-operations.md)

## Dependencies

See YAML frontmatter `depends_on` block above. Hard sequencing: core substrate → identity/capability/policy missions → mesh peer (this mission) → mesh forward / mesh rpc (follow-on missions). The role-provisioning release gate on RFC-0011-d Phase 1 is the cross-cutting dependency that gates mutating subcommands; `peer list` (read-only) is release-gated only on the substrate landing, not on RFC-0011-d.

## Acceptance Criteria

- [ ] `octo mesh peer list` implemented + unit-tested (TV-PEER-LIST-1..4 pass)
- [ ] `octo mesh peer add` implemented + unit-tested (TV-PEER-ADD-1..4 pass)
- [ ] `octo mesh peer remove` implemented + unit-tested (TV-PEER-REMOVE-1..4 pass)
- [ ] `PeerSummary` + `TrustLevel` + `EndpointUri` + `PeerFilter` types implemented in `octo-mesh` substrate (`[ADD]` per RFC-0011-f §Key Files to Modify)
- [ ] `octo_mesh::list_peers` / `add_peer` / `remove_peer` substrate functions implemented + unit-tested (3 of 5 `[ADD]` functions; the remaining 2 land in the forward / rpc missions)
- [ ] Atomic peer-table persistence (`$OCTO_HOME/mesh/peers.toml` with 0700 permissions; write to `.tmp` + fsync + rename) implemented + unit-tested
- [ ] RFC-0010 canonical DID validation at CLI dispatch (`octo_ident::CanonicalCodec::parse(s, allow_legacy=false)`); legacy form rejected with exit 4
- [ ] Endpoint URI scheme allowlist (`tcp://`, `quic://`, `bluetooth://`) enforced substrate-side; CLI exit 17 on disallowed scheme
- [ ] `--confirm` + `--dry-run` flags wired per RFC-0011 §Confirmation Flag Matrix
- [ ] `OctoCliError::InvalidEndpointScheme` variant implemented + unit-tested
- [ ] `OctoCliError::IdentityNotFound` mapping for invalid DID shape verified (exit 4)
- [ ] Output envelope `schema_version: 3` per RFC-0011-f §Output Envelope
- [ ] Redaction: no envelope bytes (peer list/add/remove are non-secret; endpoint URIs are public per RFC-0011-f §Subcommand Taxonomy)
- [ ] Cross-mission AC: peer commands integrate with core mission's `OutputEnvelope<T>` + `OctoCliError` + clap root
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-f type    | Sub-step                          | Notes                                                                                                                                                      |
| ------------------ | --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `list_peers`       | Sub-step 1 (substrate `[ADD]` #1) | Layer C; `octo_mesh::list_peers(filter: &PeerFilter) -> Result<Vec<PeerSummary>, MeshError>`; AND across `filter_trust` levels (empty = all peers)         |
| `add_peer`         | Sub-step 2 (substrate `[ADD]` #2) | Layer C; `octo_mesh::add_peer(peer_did: &Did, endpoint: &EndpointUri) -> Result<(), MeshError>`; atomic write + DID shape validation + endpoint allowlist  |
| `remove_peer`      | Sub-step 3 (substrate `[ADD]` #3) | Layer C; `octo_mesh::remove_peer(peer_did: &Did) -> Result<(), MeshError>`; idempotent (returns `Ok(())` if peer not present; does NOT contact the peer)   |
| `PeerSummary`      | Sub-step 4 (output types)         | Layer C/D; `peer_did` + `endpoint` + `trust_level` + `last_seen_unix` + `capabilities` per RFC-0011-f §Peer Summary Shape                                  |
| `TrustLevel`       | Sub-step 5 (output types)         | Layer C/D; 3-variant enum (`Trusted` / `Verified` / `Untrusted`); classification table per RFC-0011-f §Peer Summary Shape "TrustLevel::classify" semantics |
| `EndpointUri`      | Sub-step 6 (output types)         | Layer C; wrapper around the URI string with scheme allowlist enforcement                                                                                   |
| `PeerFilter`       | Sub-step 7 (output types)         | Layer C; CLI-side struct with `trust_levels: Vec<TrustLevel>`; AND semantics across levels                                                                 |
| `PeerListOutput`   | Sub-step 8 (output types)         | Layer C/D; CLI-output wrapper (`peers: Vec<PeerSummary>` + `total_count` + `filtered_count`)                                                               |
| `PeerAddOutput`    | Sub-step 9 (output types)         | Layer C/D; `peer_did` + `endpoint` + `trust_level` + `added_at_unix`                                                                                       |
| `PeerRemoveOutput` | Sub-step 10 (output types)        | Layer C/D; `peer_did` + `removed: bool` + `removed_at_unix`                                                                                                |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Phase 7 mesh extension (peer chapter) for Rust snippets + clap wiring patterns. Per RFC-0011-f §Key Files to Modify "SUBSTRATE" section, this mission creates `crates/octo-mesh/src/{peer.rs,error.rs}` and extends `crates/octo-mesh/src/lib.rs`. The CLI side lands in `crates/octo-cli/src/commands/{peer.rs,mesh.rs}` and extends `crates/octo-cli/src/commands/mod.rs`, `crates/octo-cli/src/output.rs`, `crates/octo-cli/src/error.rs`.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

- **`TrustLevel::classify` aggregation** is in the CLI per RFC-0011-f §Rationale "Why TrustLevel enum is in the CLI (not substrate)"; the substrate carries the underlying signals (RFC-0855p-c `DomainCoordinatorRecord` + RFC-0871 envelope handshake history) separately.
- **RFC-0010 canonical form** — wire form is `did:octo:z<base58btc>` per RFC-0010 §2 ledger_chain_registry Table Codec; legacy `did:octo:b<base32>` form rejected at dispatch with exit 4.
- **Atomic peer-table persistence** mirrors `octo-wallet::WalletStore` discipline per RFC-0011-f §Implicit Assumptions Audit row 7; `$OCTO_HOME/mesh/peers.toml` is 0700-permissioned.
- **No central enum for trust signals** — trust signals are substrate-defined (RFC-0855p-c cross-platform attestation may extend the classification logic without substrate changes per RFC-0011-f §Rationale).

## Scope

Land 3 peer subcommands (`list` / `add` / `remove`) per RFC-0011-f §Subcommand Taxonomy. The substrate `[ADD]` surface for this mission is functions 1-3 of the 5 listed in RFC-0011-f §Subcommand Taxonomy; the remaining 2 (`forward`, `rpc`) land in the cross-mission follow-ons (`0011-f-mesh-forward-subcommand` and `0011-f-mesh-rpc-subcommand`).

## Sub-steps

Per RFC-0011-f §Implementation Phases "Phase 7" + §Key Files to Modify:

1. **Substrate `crates/octo-mesh/src/peer.rs` (NEW)** — `PeerSummary` + `TrustLevel` + `EndpointUri` + `PeerFilter` types + `peers.toml` persistence (atomic write, 0700 perms). Per RFC-0011-f §Key Files to Modify.
2. **Substrate `crates/octo-mesh/src/error.rs` (NEW)** — `MeshError` enum (this mission's CLI-side `OctoCliError` variants map from substrate `MeshError`).
3. **Substrate `crates/octo-mesh/src/lib.rs` (EXTEND)** — export `list_peers` / `add_peer` / `remove_peer` per RFC-0011-f §Subcommand Taxonomy entries 1-3 (`[ADD]` surface).
4. **CLI `crates/octo-cli/src/commands/peer.rs` (NEW)** — `PeerAction` impls (`list` / `add` / `remove`) per RFC-0011-f §Binary Surface.
5. **CLI `crates/octo-cli/src/commands/mod.rs` (EXTEND)** — add `MeshAction::Peer` dispatch.
6. **CLI `crates/octo-cli/src/commands/mesh.rs` (NEW)** — `MeshAction` dispatch glue.
7. **CLI `crates/octo-cli/src/output.rs` (EXTEND)** — add `PeerListOutput` + `PeerAddOutput` + `PeerRemoveOutput` per RFC-0011-f §Output Envelope (`schema_version: 3`).
8. **CLI `crates/octo-cli/src/error.rs` (EXTEND)** — add `OctoCliError::InvalidEndpointScheme` per RFC-0011-f §Error Handling; `IdentityNotFound` already exists from mission `0011-identity-commands`.
9. **CLI `crates/octo-cli/Cargo.toml` (EXTEND)** — add `octo-mesh` + `octo-ident` + `chrono` deps per RFC-0011-f §Key Files to Modify.
10. **Test files `crates/octo-cli/tests/mesh_peer_*.rs` (NEW)** — 12 test vectors per RFC-0011-f §Test Vectors peer groups.
11. **Doc `docs/07-developers/octo-cli-implementation-guide.md` (EXTEND)** — Phase 7 mesh extension chapter (peer section).

### Cargo deps

Per RFC-0011-f §Key Files to Modify "SUBSTRATE" + "CLI" sections:

```toml
# crates/octo-cli/Cargo.toml additions (CLI binding layer)
octo-mesh = { path = "../octo-mesh" }              # Layer C substrate (this mission)
octo-ident = { path = "../octo-ident" }            # Layer 1 stable (RFC-0010 canonical codec)
serde_json = "1"                                    # JSON parsing for params (also used by RPC mission)
chrono = { version = "0.4", features = ["serde"] }  # RFC 3339 UTC timestamps
dirs = "5"                                          # $OCTO_HOME resolution
```

## Test Vectors (per RFC-0011-f §Test Vectors — peer list / peer add / peer remove groups)

12 TV (TV-PEER-LIST-1..4 + TV-PEER-ADD-1..4 + TV-PEER-REMOVE-1..4) — drawn from RFC-0011-f §Test Vectors sketches TV-1 (peer-list-success), TV-2 (peer-add-invalid-did-shape), TV-9 (peer-remove-not-present-idempotent), TV-10 (peer-list-filter-intersection), plus per-subcommand coverage from the 4-per-subcommand RFC floor.

| TV               | Group  | Sketch                                                                                                                       |
| ---------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------- |
| TV-PEER-LIST-1   | list   | `peer-list-success` — 3 peers (1 Trusted / 1 Verified / 1 Untrusted); sorted by (trust ASC, did LEX); exit 0                 |
| TV-PEER-LIST-2   | list   | `peer-list-filter-trusted` — `--filter-trust Trusted` filters to Trusted subset; exit 0                                      |
| TV-PEER-LIST-3   | list   | `peer-list-filter-intersection` — 5 peers (2/2/1); `--filter-trust Trusted --filter-trust Verified` returns 4; AND semantics |
| TV-PEER-LIST-4   | list   | `peer-list-empty` — empty peer table returns `peers: Vec(0)`, `total_count: 0`, `filtered_count: 0`; exit 0                  |
| TV-PEER-ADD-1    | add    | `peer-add-success-untrusted` — canonical DID + `tcp://` endpoint → atomic write + initial `trust_level: Untrusted`; exit 0   |
| TV-PEER-ADD-2    | add    | `peer-add-invalid-did-shape` — legacy `did:octo:b<base32>` form → exit 4 `IdentityNotFound`; no write                        |
| TV-PEER-ADD-3    | add    | `peer-add-invalid-endpoint-scheme` — `file://` endpoint → exit 17 `InvalidEndpointScheme`; no write                          |
| TV-PEER-ADD-4    | add    | `peer-add-confirm-required` — missing `--confirm` → exit 2 `ConfirmationRequired`; no write                                  |
| TV-PEER-REMOVE-1 | remove | `peer-remove-success` — peer present → atomic remove; `removed: true`; exit 0                                                |
| TV-PEER-REMOVE-2 | remove | `peer-remove-not-present-idempotent` — peer absent → `removed: false`; exit 0 (no error)                                     |
| TV-PEER-REMOVE-3 | remove | `peer-remove-confirm-required` — missing `--confirm` → exit 2 `ConfirmationRequired`; no remove                              |
| TV-PEER-REMOVE-4 | remove | `peer-remove-invalid-did-shape` — legacy DID form → exit 4 `IdentityNotFound`; no remove                                     |

## Layer direction (per RFC-0011-f §Rationale "Why Layer C/D placement" + [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `PeerAction` dispatch + 3 output structs (`PeerListOutput` + `PeerAddOutput` + `PeerRemoveOutput`)
- `octo-mesh` (Layer C) — extended with `[ADD]` `list_peers` + `add_peer` + `remove_peer` functions + `PeerSummary` + `TrustLevel` + `EndpointUri` + `PeerFilter` types + `MeshError` enum
- `octo-ident` (Layer 1) — REUSED only via `octo_ident::CanonicalCodec::parse(s, allow_legacy=false)`; no new Layer-1 types
- `octo-wallet` (Layer B) — UNCHANGED; peer table is its own substrate, not part of the wallet store
- `octo-policy` (Layer B) — UNCHANGED; no policy substrate amendments required for this mission

## Validation

```bash
cargo fmt --all -- --check                                          # clean
cargo clippy --workspace --all-targets --features full -- -D warnings  # clean
cargo test -p octo-cli --lib --tests                                # green
cargo test -p octo-mesh --lib --tests                               # green
```

## Backward compat

- Additive only: `Commands` enum gains one new `Mesh { action: MeshAction }` variant; no existing variant is modified per RFC-0011-f §Compatibility "Additive compatibility"
- `OutputEnvelope<T>` struct is unchanged; only `data: T` parameter gains 3 new payload types (`PeerListOutput` + `PeerAddOutput` + `PeerRemoveOutput`) per RFC-0011-f §Compatibility
- `OctoCliError` enum gains one new variant (`InvalidEndpointScheme`); existing variants unchanged
- Exit-code table reserves codes 17-30 for mesh errors; 17 is shared with `InvalidTtlHops` (forward mission); no existing exit code is re-mapped per RFC-0011-f §Exit Codes
- Redaction layer is unchanged; peer subcommands have NO secret material (DID + endpoint URI are public per RFC-0011-f §Subcommand Taxonomy)

## Cross-references

- RFC-0011-f §Subcommand Taxonomy (peer list/add/remove entries) — primary substrate
- RFC-0011-f §Binary Surface — clap `MeshAction` + `PeerAction` enum
- RFC-0011-f §Output Envelope — 3 new payload types + `schema_version: 3`
- RFC-0011-f §Error Handling — new `OctoCliError` variant
- RFC-0011-f §Exit Codes — codes 17-30 reserved
- RFC-0011-f §Roles and Authorities — `--confirm` + Auditor denial (exit 2) for mutating subcommands
- RFC-0011-f §Key Files to Modify — substrate + CLI file map
- RFC-0011-f §Implementation Phases — Phase 7 (this RFC, single phase)
- RFC-0011-f §Compatibility — additive compat, schema_version discipline
- RFC-0011-f §Rationale "Why Layer C/D placement" + "Why TrustLevel enum is in the CLI" — design rationale
- RFC-0011 §Subcommand Taxonomy — parent substrate (binary surface + output envelope + redaction + error)
- RFC-0010 §2 ledger_chain_registry Table Codec — `did:octo:z<base58btc>` wire form validation
- RFC-0855 — Mission Overlay Networks peer model (substrate-level)
- RFC-0855p-b — Coordinator Lifecycle (peer-as-coordinator case; `TrustLevel::classify` input)
- RFC-0855p-c — Domain Coordinator Role (peer-as-domain-coordinator case; `TrustLevel::Trusted` derivation)
- RFC-0871 — Specialized Node Protocol Envelope (substrate reference; not consumed by peer mission directly)
- RFC-0008 — Execution Class mapping (`peer list` / `add` / `remove` = Class C per RFC-0011-f §RFC-0008 Execution Class Mapping)
- RFC-0011-d — Role Provisioning (FUTURE; Phase 1 must reach Accepted before mutating subcommands ship — release gate)
- [[cipherocto-design-principles]] — Layer A/B stability contract

## Why 1 release cycle gate (peer mission)

Per RFC-0011-f §Roles and Authorities "Role-provisioning rationale": "`peer add` / `remove` mutate the operator's local peer table. They are gated by the role-provisioning amendment (RFC-0011-d, future) for fleet-wide consistency; in v1.0 the CLI surfaces them with `--confirm` only and notes in the rationale that RFC-0011-d will add role-gated admission." The release gate on this mission enforces the v1.0 → v1.x transition: `peer list` ships in v1.0 (read-only, no fleet-wide impact); `peer add` / `remove` defer to RFC-0011-d Phase 1 acceptance to ensure fleet-wide consistency gate is in place before mutating operations land.

## Claimant

@unassigned
