# RFC-0011-h: `octo network` Operations Substrate

## Status

Draft (2026-09-18)

> **Amendment chain:** Subordinate amendment to RFC-0011. Closes the gap DEFERRED from 0011-deprecation-stub-removal (2026-09-17): `octo network bootstrap` + `octo network status` deferred to a future amendment — this is that amendment, scoped to substrate-faithful slice of the full network CLI gap. Substrate-faithful scope bounded by what `octo-network` actually exposes today; substrate-missing slices become explicit companion missions per §Substrate-Additions Companion Missions.

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

Adds `Commands::Network { NetworkAction }` to `octo` CLI (RFC-0011). 17 substrate-faithful subcommands delegate to real `octo-network` types/methods verified by R1 dry-review ground-truth grep 2026-09-18 + R2 substrate-faithfulness re-verification.

5 functional families; `bind-envelope` straddles Bootstrap + Discovery + Envelope by substrate module (NOT by functional family — it lives in `mon/bind_envelope.rs` which crosses module boundaries):

| Family                | Subcommands                                                          | Substrate                                                                |
| --------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Bootstrap + Discovery | `peers`, `identity`, `mode`, `authority`, `discovery`                | `mon/bootstrap.rs`, `mon/discovery.rs`, `gdp/cache.rs`, `dot/gateway.rs` |
| Trust + Reputation    | `trust-graph`, `slash`                                               | `mon/trust_graph.rs`, `reputation/slash_store.rs`                        |
| Coordinator           | `coordinator`                                                        | `mon/coordinator.rs`, `octo_coordinator_types`                           |
| Governance            | `governance`                                                         | `mon/governance.rs`, `mon/governance_rotation.rs`                        |
| Envelope              | `bind-envelope` (all 4 ops: `show`, `rebind-{prepare,commit,abort}`) | `mon/bind_envelope.rs`                                                   |

Every subcommand is Layer C façade over real `octo-network` (Layer B) call. No CLI reach into substrate internals; all cross typed substrate boundary per [[cipherocto-design-principles]] §Stable Abstractions Principle.

Subcommands documented "DEFERRED (substrate-additions prerequisite)" NOT in binary surface; available only after paired companion mission closes its DRY CLOSED gate.

## Dependencies

**Requires:**

- RFC-0011 — `octo` CLI Substrate (binary surface, output envelope, redaction, error envelope, exit codes, confirmation flag matrix)
- RFC-0008 — Deterministic AI Execution Boundary
- RFC-0010 — Canonical DID Codec (peer DID wire form at every substrate boundary; RFC-0010-v17 §2 ledger_chain_registry Table is the substrate-side chain-id authority)

**Governing networking RFCs (substrate citations; this amendment does NOT amend them):**

- RFC-0851 — Gateway Discovery Protocol (substrate for `peers`, `identity`, `discovery`)
- RFC-0851p-a — Network Bootstrap Protocol (substrate for `mode`, `authority`, `slash`, `bind-envelope`, `discovery`)
- RFC-0855 — Mission Overlay Networks (substrate for `bind-envelope rebind-*`)
- RFC-0855p-c — Domain Coordinator Role (substrate for `coordinator`)
- RFC-0855p-b — Coordinator Lifecycle (slash reason code allocation; cross-platform witness vote aggregation substrate lives in `mon/slash_aggregation.rs` per RFC-0850p-c — SEPARATE substrate concern per [[cipherocto-design-principles]] §Separation of concerns, NOT touched by this amendment)
- RFC-0861 — Coordinator Admin Trait Refinements (substrate for `coordinator admin`)
- RFC-0862 — Writer Election Bootstrap (substrate for `governance tally` via `VotingTally`)
- RFC-0871 — Specialized Node Protocol Envelope (substrate for `bind-envelope`)

> **DAG invariant:** RFC-0010 → RFC-0011 → RFC-0011-h; each networking RFC → RFC-0011-h. No 2-cycle sibling required.

## Design Goals

| Goal | Target                                                                                                   | Metric                                                                                                                                                                          |
| ---- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G1   | Operator-driven bootstrap/status without substrate reach-arounds                                         | `octo network mode show` + `octo network mode set` + `octo network authority show` surface bootstrap state; `octo network peers list` surfaces `GatewayCache` without file-edit |
| G2   | All subcommands delegate through typed substrate boundary                                                | Every CLI handler calls real `octo-network` Layer B function; zero direct reach into private items                                                                              |
| G3   | RFC-0011 future-amendment band (additive exit slot extension via this RFC)                               | New exit slots 79-90; additive under `#[non_exhaustive]`                                                                                                                        |
| G4   | All mutating subcommands gated through `require_confirm` + `--confirm-acknowledge` + `--dry-run` default | RFC-0011 confirmation-gate pattern (extended to network subcommands per this RFC)                                                                                               |
| G5   | All network errors redacted in `OutputEnvelope.error`                                                    | RFC-0011 redaction layer (peer DIDs surface via `redacted: true` markers; envelope payload bytes never surface per this RFC)                                                    |
| G6   | Deterministic per-invocation output                                                                      | All 17 subcommands Class C (read-only / local-payload-builder); no Class B consensus-impacting operations                                                                       |
| G7   | One mission per subcommand-cluster + explicit substrate-additions companion missions                     | Per [[no-phantom-mission-pointers]]; mission YAML paired with each subcommand's `## Subcommand Taxonomy` row                                                                    |

## Motivation

0011-deprecation-stub-removal (2026-09-17) removed `octo init` / `octo join` / `octo status`. User accepted deferral of `octo network bootstrap` + `octo network status` to a future amendment (User Decision 2026-09-17; `0011-deprecation-stub-removal` §Out of Scope). This amendment is that future amendment, scoped to the substrate-faithful slice only; `bootstrap` + `status` remain gated on companion mission `0011-h-s-a-bootstrap-orchestrator` per §Substrate-Additions Companion Missions.

Phase 0 recon (`docs/audits/2026-09-18-network-cli-gap-recon.md`) identified 21 GAPs between `octo-network` substrate and current `Commands` enum (which has NO `Network { NetworkAction }` variant). R1 + R2 substrate-faithfulness reviews (2026-09-18) confirmed 17 substrate-faithful subcommands map to real `octo-network` substrate; the remaining GAPs are substrate-missing and become explicit companion missions (§Substrate-Additions Companion Missions).

3 concrete operator gaps:

1. **Bootstrap config observability.** Operators edit `$OCTO_HOME/octotransport/bootstrap.toml` for `BootstrapMode` + `SeedListAuthority` but have no CLI surface. Substrate `BootstrapMode` enum + `verify_authority(SeedListAuthority, epoch)` + `SlashedSeedBlacklist` exist; CLI binding does not.
2. **Network state observability.** Operators have no CLI surface for `GatewayCache` entries, trust-graph topology, slash reputation stats, coordinator lifecycle, governance tally. They grep substrate logs. Substrate functions exist for all 17 substrate-faithful subcommands.
3. **Mutating operations unsafe.** Substrate-faithful slice has 3 mutating subcommands (the `bind-envelope rebind-{prepare,commit,abort}` payload builders); every one must mirror RFC-0011 §Confirmation Flag Matrix; every mutating subcommand defaults to `--dry-run`. (`slash mark` was originally counted but is read-only inquiry against `SlashReputationStoreCompat` aggregate; CLI never mints slashes — minting is Class B per RFC-0008 §RFC-0008 Execution Class Mapping and out of scope for this amendment.)

## Roles and Authorities

### Role/Authority Coverage Table

| Role                  | Identifier                                                                    | Authority Scope                                                      | Source/Ref                                                                 |
| --------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| Operator (CLI caller) | OCTO_HOME path + capability caveats                                           | read (all 17); write (`bind-envelope rebind-{prepare,commit,abort}`) | RFC-0011 §Roles and Authorities + RFC-0011-d §Roles and Authorities        |
| Bootstrap Authority   | `SeedListAuthority::{Foundation, Dao}`                                        | gate authority state display                                         | RFC-0851p-a                                                                |
| Coordinator           | `CoordinatorRecord` (octo-coordinator-types)                                  | gate `coordinator show` (read-only; admin deferred per RFC-0861)     | RFC-0855p-c                                                                |
| Governance Voter      | `VotingTally` (struct defined in `crates/octo-network/src/mon/governance.rs`) | gate `governance tally` read                                         | substrate-canonical (no governing RFC); spec surface deferred to substrate |

### Per-Subcommand Capability Caveat Matrix

| Subcommand                       | Human                                                                                                                                               | CI  | Dev | Auditor  | Authority Role      | Notes                                                                                                                                                                              |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- | --- | --- | -------- | ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bootstrap`                      | DEFERRED — substrate-additions prerequisite (BootstrapOrchestrator missing)                                                                         | no  | no  | no       | Bootstrap Authority | see `0011-h-s-a-bootstrap-orchestrator` companion; migration: reroute via `octo network peers list` + `octo network trust-graph render` for equivalent visibility                  |
| `status`                         | DEFERRED — substrate-additions prerequisite (BootstrapState accessor missing)                                                                       | no  | no  | no       | Bootstrap Authority | see `0011-h-s-a-bootstrap-orchestrator` companion; migration: same fallback as `bootstrap`                                                                                         |
| `peers list` / `peers get`       | yes                                                                                                                                                 | yes | yes | yes      | Operator            | read-only                                                                                                                                                                          |
| `identity show`                  | yes                                                                                                                                                 | yes | yes | redacted | Operator            | peer DID redacted in Audit                                                                                                                                                         |
| `mode show`                      | BLOCKED (pending G1 `0011-h-s-a-bootstrap-orchestrator`; `BootstrapConfig::from_toml` missing in `mon/bootstrap.rs` per substrate-audit 2026-09-18) | yes | yes | yes      | Operator            | read-only once loader lands                                                                                                                                                        |
| `mode set`                       | DEFERRED — substrate-additions prerequisite (`BootstrapConfig::save_toml` missing)                                                                  | no  | no  | no       | Bootstrap Authority | see `0011-h-s-a-bootstrap-orchestrator` companion; migration: operators must manually edit `$OCTO_HOME/octotransport/bootstrap.toml` and restart until `save_toml` lands           |
| `authority show`                 | yes                                                                                                                                                 | yes | yes | yes      | Bootstrap Authority | read-only                                                                                                                                                                          |
| `authority rotate`               | DEFERRED — substrate-additions prerequisite (rotate_post_fork missing)                                                                              | no  | no  | no       | Bootstrap Authority | see `0011-h-s-a-seed-list-authority-rotate` companion; migration: operators must manually edit `OCTO_HOME/octotransport/bootstrap.toml` and restart until rotation primitive lands |
| `slash excluded` / `slash stats` | yes                                                                                                                                                 | yes | yes | redacted | Operator            | DID redacted in Audit                                                                                                                                                              |
| `slash mark`                     | CLI cannot mint SlashEnvelopes (substrate-only mint path)                                                                                           | no  | no  | no       | n/a                 | out of scope per RFC-0008 §RFC-0008 Execution Class Mapping                                                                                                                        |
| `slash list` / `slash show`      | DEFERRED — substrate-additions prerequisite (SlashStore missing)                                                                                    | no  | no  | no       | Operator            | see `0011-h-s-a-slash-store` companion; migration: use `slash stats` for aggregate view + `slash excluded <did>` for per-DID exclusion check                                       |
| `trust-graph render`             | yes                                                                                                                                                 | yes | yes | yes      | Operator            | read-only                                                                                                                                                                          |
| `coordinator show`               | BLOCKED (pending G12b `0011-h-s-a-coordinator-record-loader`; `CoordinatorRecord::load` static method missing per substrate-audit 2026-09-18)       | yes | yes | redacted | Coordinator         | coordinator DID redacted in Audit; once loader lands                                                                                                                               |
| `coordinator admin`              | DEFERRED — substrate-additions prerequisite (coordinator admin substrate missing)                                                                   | no  | no  | no       | Coordinator         | see `0011-h-s-a-coordinator-admin-trait` companion (delivers the substrate surface)                                                                                                |
| `governance tally`               | BLOCKED (pending G3b; per §Substrate-Additions)                                                                                                     | yes | yes | yes      | Governance Voter    | read-only once helper lands                                                                                                                                                        |
| `governance rotation status`     | yes                                                                                                                                                 | yes | yes | yes      | Governance Voter    | read-only                                                                                                                                                                          |
| `governance vote`                | OUT                                                                                                                                                 | no  | no  | no       | n/a                 | lives in RFC-0011-g                                                                                                                                                                |
| `bind-envelope show`             | yes                                                                                                                                                 | yes | yes | redacted | Operator            | payload bytes never surfaced                                                                                                                                                       |
| `bind-envelope rebind-prepare`   | BLOCKED (pending G21; per §Substrate-Additions)                                                                                                     | no  | yes | no       | Operator            | `--dry-run` default + `--confirm-acknowledge` required; reversible; payload-builder only                                                                                           |
| `bind-envelope rebind-commit`    | BLOCKED (pending G21; per §Substrate-Additions)                                                                                                     | no  | yes | no       | Operator            | `--dry-run` default + `--confirm-acknowledge` + `--confirm` SECOND flag (pastejacking defense per §Subcommand Taxonomy row); irreversible                                          |
| `bind-envelope rebind-abort`     | yes (write; reversible)                                                                                                                             | no  | yes | no       | Operator            | `--dry-run` default + `--confirm-acknowledge` required; reversible                                                                                                                 |
| `discovery advertisement show`   | yes                                                                                                                                                 | yes | yes | yes      | Operator            | read-only                                                                                                                                                                          |
| `discovery invitation show`      | yes                                                                                                                                                 | yes | yes | yes      | Operator            | read-only                                                                                                                                                                          |

### Out-of-scope Roles

- **P2P gossip peers** — peer-to-peer propagation substrate exists (`mon/gossip.rs`), but peer-to-peer gossip NOT operator-facing. CLI exposes `gossip stats` DEFERRED until `Gossip::stats()` substrate lands.
- **Routing substrate operators** — `routing send` (onion relay, RFC-0863) + `router status` (quota marketplace, RFC-0870) DEFERRED until `NetworkSender` + `QuotaRouterNode` substrate land. Both are substrate-facing, not operator-facing CLI surfaces.
- **Election voting operators** — `election show` + `cast_vote` DEFERRED until `WriterElection` substrate lands (RFC-0862 substrate not yet implemented; G18 `0011-h-s-a-writer-election` companion mission).
- **Slash evidence creation** — operators cannot create `SlashEnvelope`s via CLI; `slash` subcommands are read-only per RFC-0008 §RFC-0008 Execution Class Mapping.

## Specification

### System Architecture

```mermaid
graph TB
    Op[Operator<br/>octo network ...] --> Cli[octo-cli<br/>Layer C<br/>commands/network.rs]
    Cli --> Req[require_confirm<br/>RFC-0011 §Confirmation Flag Matrix]
    Req --> Facade[octo-network facade<br/>Layer B]
    Facade --> Substrate[mon/bootstrap.rs<br/>mon/bind_envelope.rs<br/>mon/discovery.rs<br/>mon/trust_graph.rs<br/>mon/governance.rs<br/>mon/governance_rotation.rs<br/>mon/coordinator.rs<br/>gdp/cache.rs<br/>dot/gateway.rs<br/>reputation/slash_store.rs<br/>reputation/dc_store.rs<br/>dc/slash_bridge.rs]
    Substrate --> Types[GatewayCacheEntry<br/>GatewayIdentity<br/>BootstrapMode<br/>SeedListAuthority<br/>SlashedSeedBlacklist<br/>BindEnvelope<br/>RebindEnvelope (umbrella:<br/>Prepare&#124;Commit&#124;Abort)<br/>MissionAdvertisement<br/>MissionInvitation<br/>TrustGraph<br/>SlashReputationStoreCompat<br/>CoordinatorRecord<br/>VotingTally<br/>GovernanceRotation]
    Types --> Codec[RFC-0010<br/>Canonical DID Codec]

    Note[SlashedSeedBlacklist<br/>marked DEPRECATED<br/>in this amendment;<br/>read methods only;<br/>see mon/bootstrap.rs §module doc<br/>(struct definition per substrate `SlashedSeedBlacklist`)]

    RFC0011h[RFC-0011-h<br/>this amendment] -.->|defines| Cli
    RFC0010 -.->|canonical codec| Codec
    RFC0851pA[RFC-0851p-a] -.->|substrate| Substrate
    RFC0851[RFC-0851] -.->|substrate| Substrate
    RFC0855[RFC-0855 + amendments] -.->|substrate| Substrate
    RFC0855pB[RFC-0855p-b] -.->|slash reason codes| Types
    RFC0855pC[RFC-0855p-c] -.->|substrate| Substrate
    RFC0861[RFC-0861] -.->|substrate when AdminTrait added| Substrate
    RFC0862[RFC-0862] -.->|VotingTally reused| Types
    RFC0871[RFC-0871] -.->|BindEnvelope| Types
```

### Binary Surface

Additive extension to `crates/octo-cli/src/lib.rs` `Commands` enum:

```rust
#[derive(Subcommand, Debug)]
#[non_exhaustive]
pub enum Commands {
    // ... existing variants (Whoami, Identity, Capability, Policy, Role,
    // Reputation, Mesh, Vault, Agent, Governance, Audit) ...

    /// Network operations subcommands (RFC-0011-h).
    ///
    /// Substrate-faithful slice of 17 subcommands delegating to
    /// `octo-network` Layer B functions. Defer-to-substrate-additions
    /// subcommands tracked in §Substrate-Additions Companion Missions.
    Network {
        /// Network subcommand.
        #[command(subcommand)]
        action: NetworkAction,
    },
}
```

`NetworkAction` enum (10 wrapper variants, `#[non_exhaustive]` per RFC-0011 §Error Handling additive contract; 17 substrate-faithful leaf subcommands via nested sub-actions per §Subcommand Taxonomy). Each nested action enum below carries its own `#[non_exhaustive]` per F-14 pattern (GovernanceAction / AgentAction / AuditAction precedent at `crates/octo-cli/src/commands/{governance,agent,audit}.rs`):

```rust
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkAction {
    /// Known gateway peers (RFC-0851).
    Peers { #[command(subcommand)] action: PeersAction },
    /// Show this node's gateway identity (RFC-0851).
    Identity { #[command(subcommand)] action: IdentityAction },
    /// Inspect / set bootstrap mode (RFC-0851p-a).
    Mode { #[command(subcommand)] action: ModeAction },
    /// Inspect the seed-list authority state (RFC-0851p-a).
    Authority { #[command(subcommand)] action: AuthorityAction },
    /// Slash reputation (RFC-0855p-b).
    Slash { #[command(subcommand)] action: SlashAction },
    /// Render the web-of-trust graph (RFC-0851p-a).
    TrustGraph { #[command(subcommand)] action: TrustGraphAction },
    /// Domain coordinator state (RFC-0855p-c + RFC-0861).
    Coordinator { #[command(subcommand)] action: CoordinatorAction },
    /// Governance tally + rotation status (RFC-0862 substrate reuse).
    Governance { #[command(subcommand)] action: NetworkGovernanceAction },
    /// Bind envelopes + rebind payload builders (RFC-0871).
    BindEnvelope { #[command(subcommand)] action: BindEnvelopeAction },
    /// Mission discovery advertisements (RFC-0851).
    Discovery { #[command(subcommand)] action: DiscoveryAction },
}

// Each nested enum below is `#[non_exhaustive]` per F-14 (extension over enumeration
// invariant; future subcommands land additively without central enum edits).
#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PeersAction { List { /* flags */ }, Get { /* flags */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdentityAction { Show { /* flags */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModeAction { Show { /* flags */ }, Set { /* flags */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthorityAction { Show { /* flags */ }, Rotate { /* flags */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SlashAction { Excluded { /* flags */ }, Stats { /* flags */ }, List { /* flags */ }, Show { /* flags */ }, Mark { /* flags */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustGraphAction { Render { /* flags */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CoordinatorAction { Show { /* flags */ }, Admin { /* flags */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkGovernanceAction { Tally { /* flags */ }, Rotation { /* subcommand */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BindEnvelopeAction { Show { /* flags */ }, RebindPrepare { /* flags */ }, RebindCommit { /* flags */ }, RebindAbort { /* flags */ } }

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiscoveryAction { Advertisement { /* subcommand */ }, Invitation { /* subcommand */ } }
```

### Subcommand Taxonomy

Each row maps: substrate function → governing RFC section → mission YAML.

| Subcommand                     | Sub-action                                                                                                                                                                                                                                                        | Substrate function(s)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | Governing RFC §                                                                                                   | Mission YAML                                                  | Phase | Authority Role      |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- | ----- | ------------------- |
| `peers list`                   | (read)                                                                                                                                                                                                                                                            | `GatewayCache::iter()`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | RFC-0851 §10 Gateway Cache                                                                                        | `0011-h-network-peers-identity` §peers-list                   | 1     | Operator            |
| `peers get <gateway_id>`       | (read)                                                                                                                                                                                                                                                            | `GatewayCache::get(gateway_id)`; `<gateway_id>` accepts 64-char hex (32-byte blake3 hash per RFC-0850 §3.2); CLI parser maps to `[u8; 32]`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | RFC-0851 §10 Gateway Cache                                                                                        | `0011-h-network-peers-identity` §peers-get                    | 1     | Operator            |
| `identity show`                | (read)                                                                                                                                                                                                                                                            | Local public-key read → `GatewayIdentity::new(pk, net_id, gateway_class, epoch)`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | RFC-0851 §1 Gateway Identity                                                                                      | `0011-h-network-peers-identity` §identity                     | 1     | Operator            |
| `mode show`                    | (read; BLOCKED — pending G1 `0011-h-s-a-bootstrap-orchestrator` closure; `BootstrapConfig::from_toml` missing in `crates/octo-network/src/mon/bootstrap.rs` per L2 substrate audit 2026-09-18)                                                                    | Local config read → `BootstrapMode` enum (enum exists; `BootstrapConfig::from_toml` parser missing in substrate TODAY; `mode show` exposes enum value once loader lands)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | RFC-0851p-a §3 Mode A                                                                                             | `0011-h-network-mode` §mode-show                              | 2     | Operator            |
| `authority show`               | (read)                                                                                                                                                                                                                                                            | `verify_authority(SeedListAuthority, current_epoch)`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | RFC-0851p-a §1 BootstrapNode Registry                                                                             | `0011-h-network-authority` §authority-show                    | 2     | Bootstrap Authority |
| `slash excluded <did>`         | (read)                                                                                                                                                                                                                                                            | `SlashReputationStoreCompat::is_excluded(did)`; `<did>` accepts 104-char hex (52-byte `RecorderDid` per `crates/octo-reputation/src/types.rs`); CLI parser maps to `RecorderDid`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | RFC-0855p-b                                                                                                       | `0011-h-network-slash-stats` §slash-excluded                  | 2     | Operator            |
| `slash stats`                  | (read)                                                                                                                                                                                                                                                            | `SlashReputationStoreCompat::{did_count, total_slashes, global_slash_count}` (see G6b for aggregation rationale; cross-platform witness aggregation lives in `mon/slash_aggregation.rs` per RFC-0850p-c, SEPARATE substrate concern per [[cipherocto-design-principles]] §Separation of concerns). `per_did` accessor DEFERRED to G6 `0011-h-s-a-slash-store` companion mission per §Substrate-Additions (substrate lacks `iter_dids()` accessor verified 2026-09-18)                                                                                                                                                                                                                                                                                                                                                                                                                                | RFC-0855p-b                                                                                                       | `0011-h-network-slash-stats` §slash-stats                     | 2     | Operator            |
| `trust-graph render`           | --depth 1-100; --format ascii or dot                                                                                                                                                                                                                              | `TrustGraph::render(&self, format: GraphFormat) -> String`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | RFC-0851p-a §10 Error Handling                                                                                    | `0011-h-network-trust-graph`                                  | 1     | Operator            |
| `coordinator show`             | (read; BLOCKED — pending G12b `0011-h-s-a-coordinator-record-loader` closure)                                                                                                                                                                                     | `CoordinatorRecord` load via `octo-coordinator-types` (canonical type at `octo_coordinator_types::state::CoordinatorRecord`); substrate currently exposes no public `CoordinatorRecord::load(coordinator_id)` entry point — see `0011-h-s-a-coordinator-record-loader` companion mission in §Substrate-Additions Companion Missions for the loader addition                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | RFC-0855p-c + RFC-0861                                                                                            | `0011-h-network-coordinator` §coordinator-show                | 3     | Coordinator         |
| `governance tally`             | (read; BLOCKED — pending G3b `0011-h-s-a-voting-tally-canonical-bytes` closure)                                                                                                                                                                                   | `VotingTally::{total_for, total_against, into_canonical}`. `into_canonical(self, proposal_id, issuer, decision, state, voting_opens_at_millis, voting_closes_at_millis, total_eligible_weight) -> GovernanceProposal` per substrate signature at `crates/octo-network/src/mon/governance.rs` — receiver is `self` (consuming; not `&mut self`); 7 separate args (NOT a single `GovernanceProposal`); return is `GovernanceProposal` (NOT `Vec<u8>`); CLI MUST call before any subsequent field access would be a use-after-move substrate violation. The 7 args are sourced from substrate GovernanceProposal state. `canonical_bytes_hash` field BLOCKED per §Substrate-Additions G3b: the canonical-bytes hashing helper `governance_proposal_canonical_bytes(&GovernanceProposal) -> [u8; 32]` is gap not yet landed in substrate; CLI surfaces `[REDACTED:canonical_bytes_hash]` until G3b lands | substrate-canonical (VotingTally struct defined in `crates/octo-network/src/mon/governance.rs`; no governing RFC) | `0011-h-network-governance` §governance-tally                 | 3     | Governance Voter    |
| `governance rotation status`   | (read)                                                                                                                                                                                                                                                            | `GovernanceRotation::{has_quorum, migration_deadline, in_migration_window(current_epoch: u64)}`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | RFC-0862                                                                                                          | `0011-h-network-governance` §governance-rotation              | 3     | Governance Voter    |
| `bind-envelope show`           | (read)                                                                                                                                                                                                                                                            | `BindEnvelope::{canonical_bytes, is_participant}`. Single canonical cite for blake3 + no-leak invariant: §Security Considerations.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | RFC-0871 §Data Structures                                                                                         | `0011-h-network-bind-envelope` §bind-envelope-show            | 4     | Operator            |
| `bind-envelope rebind-prepare` | (write; --dry-run default; --confirm-acknowledge; --no-dry-run to apply; BLOCKED — pending G21 `0011-h-s-a-attached-handle-key-rotation` closure)                                                                                                                 | `RebindEnvelope::Prepare(RebindPrepare)` builder (umbrella enum at `crates/octo-network/src/mon/bind_envelope.rs`). Key-id surface BLOCKED per §Substrate-Additions G21 (substrate currently lacks `AttachedHandleKeyRotation` trait + `current_key_id()` method; D2.1 only landed primitives per RFC-0011-c §F.5.1) --confirm double-gate NOT required (prepare only reserves resources and is fully reversible via substrate accept-rollback per RFC-0871 §Algorithms); --confirm-acknowledge sufficient alone per pastejacking-defense risk model                                                                                                                                                                                                                                                                                                                                                 | RFC-0871                                                                                                          | `0011-h-network-bind-envelope` §bind-envelope-rebind          | 4     | Operator            |
| `bind-envelope rebind-commit`  | (write; --dry-run default; --confirm-acknowledge; --confirm SECOND flag for irreversible state mutation per §Security Considerations pastejacking-defense bullet; --no-dry-run to apply; BLOCKED — pending G21 `0011-h-s-a-attached-handle-key-rotation` closure) | `RebindEnvelope::Commit(RebindCommit)` builder (umbrella enum at `crates/octo-network/src/mon/bind_envelope.rs`). Key-id surface BLOCKED per §Substrate-Additions G21 (substrate currently lacks `AttachedHandleKeyRotation` trait + `current_key_id()` method; D2.1 only landed primitives per RFC-0011-c §F.5.1)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | RFC-0871                                                                                                          | `0011-h-network-bind-envelope` §bind-envelope-rebind          | 4     | Operator            |
| `bind-envelope rebind-abort`   | (write; --dry-run default; --confirm-acknowledge; --no-dry-run to apply; reversible via substrate accept-rollback per RFC-0871 §Algorithms; NOT pastejacking-mitigated above prepare)                                                                             | `RebindEnvelope::Abort(RebindAbort { domain_id, reason, dissenters, signature })` builder (umbrella enum at `crates/octo-network/src/mon/bind_envelope.rs`). --confirm double-gate NOT required (abort ROLLS BACK the prepare-reserve state; substrate idempotency check per RFC-0871 §Algorithms means double-abort is a no-op); --confirm-acknowledge sufficient alone per pastejacking-defense risk model                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | RFC-0871                                                                                                          | `0011-h-network-bind-envelope` §bind-envelope-rebind          | 4     | Operator            |
| `discovery advertisement show` | (read; --hops u16 0-65535 default 0)                                                                                                                                                                                                                              | `MissionAdvertisement::{advertisement_hash, is_encrypted, is_ttl_exceeded(current_hops: u16)}` — `current_hops` is a METHOD ARGUMENT only (the struct has NO `current_hops` field per substrate `MissionAdvertisement` definition). `--hops` accepts u16 range 0-65535 (clap rejects >65535 with exit 2 `ClapParse` per `is_ttl_exceeded` method signature); default `--hops 0`; `--hops` arg is the sole input to `is_ttl_exceeded` evaluation                                                                                                                                                                                                                                                                                                                                                                                                                                                      | RFC-0851 §8 Discovery Lifecycle                                                                                   | `0011-h-network-discovery` §discovery-advertisement           | 5     | Operator            |
| `discovery invitation show`    | (read)                                                                                                                                                                                                                                                            | `MissionInvitation::to_signing_bytes`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | RFC-0851 §8 Discovery Lifecycle                                                                                   | `0011-h-network-discovery` §discovery-invitation              | 5     | Operator            |
| `mode set`                     | (write; BLOCKED — pending G1 `0011-h-s-a-bootstrap-orchestrator` closure; `BootstrapConfig::save_toml` missing in `crates/octo-network/src/mon/bootstrap.rs` per L2 substrate audit 2026-09-18)                                                                   | `BootstrapConfig::save_toml(path)` writer. Companion mission `0011-h-s-a-bootstrap-orchestrator` per §Substrate-Additions G1 row closes this gate                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | RFC-0851p-a §3 Mode A                                                                                             | `0011-h-network-mode` §mode-set                               | 2     | Bootstrap Authority |
| `authority rotate`             | (write; BLOCKED — pending G8 `0011-h-s-a-seed-list-authority-rotate` closure; `SeedListAuthority::rotate_post_fork` missing in substrate)                                                                                                                         | `SeedListAuthority::rotate_post_fork(new_epoch) → SeedListAuthority` builder. Companion mission `0011-h-s-a-seed-list-authority-rotate` per §Substrate-Additions G8 row closes this gate                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | RFC-0851p-a §1 BootstrapNode Registry                                                                             | `0011-h-network-authority` §authority-rotate                  | 2     | Bootstrap Authority |
| `slash mark`                   | (out of scope — CLI cannot mint SlashEnvelopes)                                                                                                                                                                                                                   | Substrate-only mint path; CLI cannot produce SlashEnvelopes. Operators calling pre-landing hit clap `UnrecognizedSubcommand` exit 2. Future amendment would need to extend RFC-0008 §RFC-0008 Execution Class Mapping                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                | RFC-0008 §RFC-0008 Execution Class Mapping                                                                        | (out of scope per RFC-0008 §RFC-0008 Execution Class Mapping) | n/a   | n/a                 |
| `slash list` / `slash show`    | (read; BLOCKED — pending G6 `0011-h-s-a-slash-store` closure; `SlashStore::list` + `peer_reputation(did)` missing in substrate)                                                                                                                                   | `SlashStore::{list(filter), peer_reputation(did)}` readers. Companion mission `0011-h-s-a-slash-store` per §Substrate-Additions G6 row closes this gate. Operators calling pre-landing use `slash stats` + `slash excluded <did>` for equivalent visibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | RFC-0855p-b + RFC-0860                                                                                            | `0011-h-network-slash-stats` §slash-list-show                 | 2     | Operator            |
| `coordinator admin`            | (write; BLOCKED — pending G12 `0011-h-s-a-coordinator-admin-trait` closure; coordinator admin substrate missing)                                                                                                                                                  | Coordinator admin operations surface (rotate, suspend, reactivate) lands via companion mission `0011-h-s-a-coordinator-admin-trait` per §Substrate-Additions G12 row. RFC-0861 governs unrelated GroupHandle adapter contract refinements; coordinator admin actions do not yet have a governing RFC (substrate surface comes first; governing RFC amendment follows per BLUEPRINT ordering).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | substrate-canonical (lands via G12 mission)                                                                       | `0011-h-network-coordinator` §coordinator-admin               | 3     | Coordinator         |
| `bootstrap`                    | DEFERRED — substrate-additions prerequisite (`BootstrapOrchestrator` missing)                                                                                                                                                                                     | `BootstrapOrchestrator::start_bootstrap(BootstrapConfig)` + `status()` (per G1 row). Operators calling pre-landing hit clap `UnrecognizedSubcommand` exit 2; reroute via `0011-h-s-a-bootstrap-orchestrator` companion mission                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | RFC-0851p-a §3 Mode A                                                                                             | `0011-h-network-bootstrap` (deferred)                         | 6     | Bootstrap Authority |
| `status`                       | DEFERRED — substrate-additions prerequisite (`BootstrapState` missing)                                                                                                                                                                                            | `BootstrapOrchestrator::status() → BootstrapState` (per G1 row). Operators calling pre-landing hit clap `UnrecognizedSubcommand` exit 2; fall back to `octo network peers list` + `octo network trust-graph render` for equivalent visibility                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | RFC-0851p-a §3 Mode A                                                                                             | `0011-h-network-status` (deferred)                            | 6     | Bootstrap Authority |

### Output Envelope

Every subcommand emits `OutputEnvelope<T>` where `T` is `schemars::JsonSchema` struct:

```rust
// octo network peers list (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkPeersListOutput {
    pub peers: Vec<GatewayCacheProjection>,  // CLI-side read-only DTO; substrate-faithful fields derived from GatewayCacheEntry per substrate `GatewayCacheEntry` (gdp/cache.rs)
    pub total_count: usize,
}

// octo network trust-graph render (read)
#[derive(Serialize, JsonSchema)]
pub struct TrustGraphOutput {
    pub format: GraphFormat,                  // Ascii | Dot per substrate GraphFormat enum
    pub depth_applied: u32,                   // echoed for verification (clamped per §Security Considerations --depth clamp bullet)
    pub body: String,                         // substrate `TrustGraph::render(&self, format: GraphFormat) -> String` rendered graph
}

// octo network mode show (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkModeShowOutput {
    pub mode: BootstrapMode,                  // Direct | TorOnly | TorWithIpFallback
    pub source_path: String,                  // local config file path
}

// octo network authority show (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkAuthorityShowOutput {
    pub authority: SeedListAuthority,         // Foundation | Dao
    pub current_epoch: u64,
    pub valid: bool,                          // maps verify_authority() Ok(())
    pub message: Option<String>,              // Display of SeedAuthorityError when verify_authority() Err
}

// octo network slash stats (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkSlashStatsOutput {
    pub distinct_did_count: usize,            // substrate SlashReputationStoreCompat::did_count()
    pub total_slashes: u64,                   // substrate SlashReputationStoreCompat::total_slashes()
    pub per_did: Vec<DidSlashCount>,          // DEFERRED — substrate lacks per-DID iterator (no `iter_dids()` accessor verified 2026-09-18; surface returns empty vec until G6 `0011-h-s-a-slash-store` companion lands the iterator); aggregate-only surface today
}

// substrate-faithful type def for NetworkPeersListOutput::peers element
#[derive(Serialize, JsonSchema)]
/// CLI-side read-only projection of `GatewayCacheEntry` for `peers list` output.
/// NOT a substrate surface; substrate has `GatewayCacheEntry` per substrate `GatewayCacheEntry` (gdp/cache.rs).
/// CLI never mints this type — builders read from substrate and map into this projection.
pub struct GatewayCacheProjection {
    pub gateway_id: String,                   // 64-char lowercase hex of substrate GatewayIdentity bytes
    pub gateway_class: String,                // substrate GatewayClass Display form (e.g. "Edge"); full variant set per `crates/octo-network/src/dot/gateway.rs` enum GatewayClass: Edge|Relay|Consensus|Archive|Stealth|Translation. (NOT to be confused with BootstrapMode: Direct|TorOnly|TorWithIpFallback per substrate `BootstrapMode` enum (mon/bootstrap.rs).)
    pub first_seen_epoch: u64,                // substrate GatewayCacheEntry::first_seen
}

// substrate-faithful type def for NetworkSlashStatsOutput::per_did element
#[derive(Serialize, JsonSchema)]
pub struct DidSlashCount {
    pub redacted_did: String,                 // literal `"[REDACTED:did]"` placeholder; never the raw DID per RFC-0011 §Redaction layer
    pub slash_count: u32,                     // substrate SlashReputationStoreCompat::global_slash_count(did) (per-DID accessor present; per-DID iteration requires `iter_dids()` accessor which is DEFERRED to G6 `0011-h-s-a-slash-store` companion mission per §Substrate-Additions)
}

// octo network coordinator show (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkCoordinatorShowOutput {
    pub coordinator_peer_id: CoordinatorId,           // redacted in Audit (CoordinatorRecord::coordinator_peer_id)
    pub state: CoordinatorLifecycle,                  // CoordinatorRecord::state (not lifecycle)
    pub source: CoordinatorSource,                    // CoordinatorRecord::source
    pub term_start_epoch: u64,                        // CoordinatorRecord::term_start_epoch (not tenure_start_epoch; not Option)
    pub term_end_epoch: u64,                          // CoordinatorRecord::term_end_epoch (paired with term_start for operator visibility)
    pub coordinator_term_id: [u8; 32],                // CoordinatorRecord::coordinator_term_id
    pub slash_count: u32,                             // CoordinatorRecord::slash_count
    pub last_heartbeat_epoch: u64,                    // CoordinatorRecord::last_heartbeat_epoch
    pub heartbeat_interval: u64,                      // CoordinatorRecord::heartbeat_interval
    pub octo_o_stake_locked: u64,                     // CoordinatorRecord::octo_o_stake_locked
}

// octo network governance tally (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkGovernanceTallyOutput {
    pub votes_for: BTreeMap<[u8;32], u64>,    // substrate-canonical field per substrate `VotingTally::votes_for` field
    pub votes_against: BTreeMap<[u8;32], u64>, // substrate-canonical field per substrate `VotingTally::votes_against` field
    pub canonical_bytes_hash: String,         // blake3 hex via `octo_cap_macaroon::blake3_hash(&governance_proposal_canonical_bytes(&proposal))` — HYPOTHETICAL caller chain (the `governance_proposal_canonical_bytes` helper is BLOCKED per §Substrate-Additions G3b; helper absent in substrate TODAY, verified 2026-09-18). `OutputType` itself is BLOCKED: this field is rendered as `"[REDACTED]"` until G3b lands. Once G3b lands, the helper signature is `fn governance_proposal_canonical_bytes(p: &GovernanceProposal) -> [u8; 32]` consuming the Layer A frozen `GovernanceProposal` at `crates/octo-governance-core/src/proposal.rs` (NOT Layer B).
}

// octo network bind-envelope show (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkBindEnvelopeShowOutput {
    pub domain_id: String,                    // bind envelope domain identifier (canonical hex form)
    pub platform: String,                     // substrate-faithful to BindEnvelope::platform (e.g. "whatsapp", "matrix", "telegram")
    pub group_id: String,                     // substrate-faithful to BindEnvelope::group_id (physical group identifier per platform)
    pub participant_filter: Option<Vec<String>>, // substrate-faithful to BindEnvelope::participant_filter (public field per `crates/octo-network/src/mon/bind_envelope.rs`; Option<Vec<String>>, None = no filter)
    pub member_count_at_bind: u16,            // substrate-faithful to BindEnvelope::member_count_at_bind (group size at binding time per RFC-0855p-c)
    pub signature_redacted: bool,             // true: signature bytes never echoed (no-payload-bytes-leak invariant)
    pub canonical_bytes_hash: String,         // blake3 hex via `octo_cap_macaroon::blake3_hash(&env.canonical_bytes())`
}

// octo network bind-envelope rebind-prepare (write)
#[derive(Serialize, JsonSchema)]
pub struct NetworkRebindPrepareOutput {
    pub domain_id: String,                    // bind envelope domain identifier (canonical hex form)
    pub new_bind: BindEnvelope,               // proposed replacement substrate struct (RebindPrepare::new_bind)
    pub deadline_epoch: u64,                  // deadline after which prepare expires
    pub payload: String,                      // canonical JSON; operator review surface
    pub dry_run: bool,                        // true when --dry-run (no substrate mutation)
}

// octo network bind-envelope rebind-commit (write)
#[derive(Serialize, JsonSchema)]
pub struct NetworkRebindCommitOutput {
    pub domain_id: String,                    // bind envelope domain identifier (canonical hex form)
    pub new_bind: BindEnvelope,               // committed replacement substrate struct (RebindCommit::new_bind)
    pub prepared_evidence_hex: String,        // hex-encoded substrate `RebindCommit::prepared_evidence` (sorted platform/signature pairs; WITNESS data, not envelope payload — no-payload-bytes-leak invariant covers `BindEnvelope::canonical_bytes` only)
    pub dry_run: bool,                        // true when --dry-run (no substrate mutation)
}

// octo network bind-envelope rebind-abort (write)
#[derive(Serialize, JsonSchema)]
pub struct NetworkRebindAbortOutput {
    pub domain_id: String,                    // bind envelope domain identifier (canonical hex form)
    pub reason: RebindAbortReason,            // abort reason enum: VoteAbort | Timeout | LostTieBreak
    pub dissenters: Vec<String>,              // platforms that voted abort (or timed out); sorted
    pub signature_redacted: bool,             // true: signature bytes never echoed (no-payload-bytes-leak invariant)
    pub aborted: bool,                        // CLI surface derivation: `aborted = !dry_run` (true = substrate RebindAbort accepted, CLI surfaces operator-facing "aborted" wording; false = dry-run preview only — substrate call deferred, abort not yet accepted)
    pub dry_run: bool,                        // true when --dry-run (no substrate mutation)
}

// octo network discovery advertisement show (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkDiscoveryAdvertisementOutput {
    pub advertisement_hash: String,           // hex-encoded [u8; 32] per MissionAdvertisement::advertisement_hash
    pub encrypted: bool,
    pub ttl_exceeded: bool,                   // is_ttl_exceeded(current_hops)
    pub hops_arg: u16,                        // source: --hops clap arg only (default 0); no envelope-header current_hops in substrate struct
}

// octo network discovery invitation show (read)
#[derive(Serialize, JsonSchema)]
pub struct NetworkDiscoveryInvitationOutput {
    pub mission_id_hex: String,                // 76-char lowercase hex of substrate MissionInvitation::mission_id (MissionId struct, 38 bytes canonical form)
    pub invitee_gateway_id_hex: String,        // 64-char lowercase hex of substrate MissionInvitation::invitee_gateway_id ([u8; 32])
    pub coordinator_gateway_id_hex: String,    // 64-char lowercase hex of substrate MissionInvitation::coordinator_gateway_id ([u8; 32])
    pub logical_timestamp: u64,                // substrate MissionInvitation::logical_timestamp
    pub signing_bytes_hex: String,            // hex-encoded Vec<u8> from MissionInvitation::to_signing_bytes
}
```

### Error Handling

See also §Exit Codes for the operator-facing code-to-symbol mapping. Slots 80 and 81 are RESERVED; see footnote [†] below.

| Substrate error                                                                                                                                                                                                                                       | CLI exit code slot | `OctoCliError` variant                   | Redaction level         |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ | ---------------------------------------- | ----------------------- |
| `GatewayCache::get` returns `None`                                                                                                                                                                                                                    | 79                 | `NetworkPeerNotFound` (NEW)              | `gateway_id` redacted   |
| <sup>[†](#slot-reserve-footnote)</sup> 80                                                                                                                                                                                                             | (RESERVED)         | n/a                                      | n/a                     |
| <sup>[†](#slot-reserve-footnote)</sup> 81                                                                                                                                                                                                             | (RESERVED)         | n/a                                      | n/a                     |
| Local config parse failure                                                                                                                                                                                                                            | 82                 | `NetworkConfigParseFailed` (NEW)         | file path redacted      |
| Local public-key unavailable                                                                                                                                                                                                                          | 83                 | `NetworkLocalKeyUnavailable` (NEW)       | n/a                     |
| `CoordinatorRecord` not found                                                                                                                                                                                                                         | 84                 | `NetworkCoordinatorNotFound` (NEW)       | coordinator_id redacted |
| `TrustGraph` depth below 1 (`--depth 0`)                                                                                                                                                                                                              | 85                 | `NetworkGraphDepthBelowRange` (NEW)      | n/a                     |
| DID codec rejection (RFC-0010)                                                                                                                                                                                                                        | 86                 | `NetworkInvalidDid` (NEW)                | DID redacted            |
| `--confirm-acknowledge` missing on write, OR for `rebind-commit` only `--confirm` missing alongside `--confirm-acknowledge` present (CLI gate; fires before substrate dispatch per RFC-0011 §Confirmation Flag Matrix)                                | 87                 | `NetworkConfirmRequired` (NEW)           | n/a                     |
| `--dry-run` explicit + operator denied                                                                                                                                                                                                                | 88                 | `NetworkDryRunDenied` (NEW)              | n/a                     |
| Substrate state not loaded (companion)                                                                                                                                                                                                                | 89                 | `NetworkSubstrateUnavailable` (RESERVED) | n/a                     |
| CI mode detected on rebind-* write path (env-var `OCTO_CLI_CI=1` or non-TTY stdin); CI mode gate is evaluated BEFORE the `--confirm-acknowledge` check, so in CI mode the operator always sees exit 90 regardless of `--confirm-acknowledge` presence | 90                 | `NetworkCIRebindDenied` (NEW)            | n/a                     |

All 9 NEW `OctoCliError` variants (slots 79 + 82-88 + 90) + 2 RESERVED slots (80, 81) + slot 89 RESERVED (no fallback; see foot-note [†] above). Additive under `#[non_exhaustive]` per RFC-0011 §Error Handling + [[cipherocto-design-principles]] §Extension over enumeration. Slot range 79-90 claimed within parent RFC-0011 §Exit Codes future-amendment band (79-99).

RFC-0011-c/d/e/f/g all below slot 79; slots 79 + 82-90 free at Draft time. Slots 80 + 81 RESERVEd at Draft because substrate `BindEnvelope::canonical_bytes()` returns infallible `Vec<u8>` (no overflow path; substrate has no fallible variant to map a slot 80 `NetworkEnvelopeEncodeFailed` against) AND `RebindAbortReason` is a closed 3-variant enum (no invalid variant path; cannot synthesize a slot 81 `NetworkRebindReasonInvalid` from substrate state). Per substrate-faithfulness audit 2026-09-18, slots revert to candidate status if either substrate surface lands a fallible path in a future amendment.

### Exit Codes

| Code | Symbol                                                 | Description                                                                          |
| ---- | ------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| 0    | `Ok`                                                   | Success                                                                              |
| 2    | `UnrecognizedSubcommand`                               | clap default                                                                         |
| 65   | `StaleStub`                                            | RFC-0011                                                                             |
| 79   | `NetworkPeerNotFound`                                  | `GatewayCache::get` returns `None` (gateway_id redacted)                             |
| 80   | <sup>[†](#slot-reserve-footnote-exit)</sup> (RESERVED) | No substrate failure path; see footnote [†]                                          |
| 81   | <sup>[†](#slot-reserve-footnote-exit)</sup> (RESERVED) | No substrate failure path; see footnote [†]                                          |
| 82   | `NetworkConfigParseFailed`                             | Local config parse failure (file path redacted)                                      |
| 83   | `NetworkLocalKeyUnavailable`                           | Local public-key unavailable                                                         |
| 84   | `NetworkCoordinatorNotFound`                           | `CoordinatorRecord` not found (coordinator_id redacted)                              |
| 85   | `NetworkGraphDepthBelowRange`                          | `TrustGraph` depth below 1 (per §Adversarial Review directionality)                  |
| 86   | `NetworkInvalidDid`                                    | DID codec rejection per RFC-0010 (Canonical DID Codec; DID redacted in variant name) |
| 87   | `NetworkConfirmRequired`                               | `--confirm-acknowledge` OR `--confirm` (rebind-commit only) missing on write path    |
| 88   | `NetworkDryRunDenied`                                  | `--dry-run` explicit + operator denied                                               |
| 89   | `NetworkSubstrateUnavailable`                          | Substrate state not loaded (RESERVED — gated on companion mission landing)           |
| 90   | `NetworkCIRebindDenied`                                | CI mode detected on rebind-* write path (env-var `OCTO_CLI_CI=1` or non-TTY stdin)   |
| ≥128 | `SubstrateErrorPanic`                                  | RFC-0011 §Exit Codes                                                                 |

## Performance Targets

| Metric                                        | Target | Notes                                                   |
| --------------------------------------------- | ------ | ------------------------------------------------------- |
| `peers list` wall-clock                       | <500ms | Linear in `GatewayCache::iter()`; expected ≤256 entries |
| `peers get` wall-clock                        | <50ms  | Single HashMap lookup                                   |
| `identity show` wall-clock                    | <100ms | Local public-key file read                              |
| `mode show` wall-clock                        | <50ms  | Single TOML read                                        |
| `authority show` wall-clock                   | <10ms  | Pure function call (no I/O)                             |
| `slash stats` wall-clock                      | <100ms | O(distinct DID count)                                   |
| `slash excluded` wall-clock                   | <50ms  | HashMap probe                                           |
| `trust-graph render ascii`                    | <2s    | 100-node graph; per RFC-0851p-a §10 Error Handling      |
| `trust-graph render dot`                      | <2s    | 100-node graph                                          |
| `coordinator show`                            | <100ms | Local record load                                       |
| `governance tally`                            | <50ms  | O(1) tally reads                                        |
| `governance rotation status`                  | <10ms  | Pure field read                                         |
| `bind-envelope show`                          | <50ms  | In-memory bind envelope read                            |
| `bind-envelope rebind-{prepare,commit,abort}` | <200ms | Pure payload builder; no I/O                            |
| `discovery advertisement show`                | <50ms  | Local advertisement cache lookup                        |
| `discovery invitation show`                   | <50ms  | Local invitation cache lookup                           |

## Implicit Assumptions Audit

| Assumption                                        | Where Relied Upon                               | Blast Radius if False                                    | Mitigation / Status                                                                                                                                                                                                                                                                                                                     |
| ------------------------------------------------- | ----------------------------------------------- | -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Substrate state is current                        | `peers list`, `trust-graph`, `coordinator show` | Stale data presented as fresh; operator misreads network | Substrate is source of truth; staleness applies at substrate level                                                                                                                                                                                                                                                                      |
| `OCTO_HOME/octotransport/bootstrap.toml` exists   | `mode show`, `authority show`                   | CLI returns 82 (`NetworkConfigParseFailed`)              | Documented in `octo network mode show --help`                                                                                                                                                                                                                                                                                           |
| Peer DID codec is RFC-0010 canonical              | All peer-DID-sourcing subcommands               | Legacy DID form could bypass codec gate                  | `octo_ident::CanonicalCodec::parse(s, allow_legacy=false)` at every substrate boundary (RFC-0011-f G2 pattern; codec spec lives in RFC-0010-v17 §2 ledger_chain_registry Table)                                                                                                                                                         |
| Operator is trusted within OCTO_HOME              | `--confirm-acknowledge` write path              | Compromised OCTO_HOME = operator compromise              | CLI confirms via `--confirm-acknowledge` flag per RFC-0011 §Confirmation Flag Matrix                                                                                                                                                                                                                                                    |
| `octo-coordinator-types` runtime is loaded        | `coordinator show`                              | Coordinator lifecycle state unavailable                  | Substrate exposes `CoordinatorRecord` type at `octo_coordinator_types::state::CoordinatorRecord`; missing `CoordinatorRecord::load(coordinator_id)` static method → exit 89 `NetworkSubstrateUnavailable` (gated on `0011-h-s-a-coordinator-record-loader` companion mission which adds the loader to `octo-coordinator-types` Layer B) |
| `octo-network::reputation::*` substrate is loaded | `slash stats`, `slash excluded`                 | Stats unavailable                                        | Substrate loads `SlashReputationStoreCompat` via public façade `octo_network::reputation::*`; in-memory state per substrate `slash_store.rs`; if not loaded → exit 89 `NetworkSubstrateUnavailable` (gated on `0011-h-s-a-slash-store-loader` companion mission)                                                                        |

## Security Considerations

Narrative threat model. Operator-facing decision matrix in §Adversarial Review below. Canonical cite for pastejacking defense, canonical_bytes_hash no-payload-bytes-leak invariant, rebind nonce/timestamp gate, depth clamp, and key-compromise mitigation lives in the §Adversarial Review table.

- **Envelope payload bytes never echoed (canonical invariant).** `bind-envelope show` returns metadata + `canonical_bytes_hash` only; raw `Vec<u8>` from `BindEnvelope::canonical_bytes()` NEVER surface in `OutputEnvelope` or logs. Invariant: `no-payload-bytes-leak`. Test vector: `tv-network-bind-envelope-show-1`.
- **Mutating subcommands gated + pastejacking defense.** All 3 `bind-envelope rebind-{prepare,commit,abort}` write subcommands require `--dry-run` default + `--confirm-acknowledge`; `rebind-commit` adds `--confirm` SECOND flag per RFC-0011 §Confirmation Flag Matrix human column for high-blast-radius writes (pastejacking defense) per §Subcommand Taxonomy rebind-commit row.
- **Redaction placeholder form.** RFC-0011 §Redaction layer `[REDACTED:<kind>]` placeholders cover `[REDACTED:did]`, `[REDACTED:coordinator_id]`, `[REDACTED:gateway_id]`, `[REDACTED:file_path]`, `[REDACTED:domain_id]`. Audit mode applies same placeholders.
- **`--allow-ci-rebind` escape hatch (CI only).** Hidden from `--help` per RFC-0011 §Test Vectors convention; surfaces in `octo network ... --help-all` for emergency CI override only; bypasses the pastejacking-defense CI gate documented in §CI Mode Rationale. Stable contract: experimental flag, surfaces in changelog as such, removed before v1.0. Operators MUST NOT script against this flag; substrate-level double-confirm gate is the canonical pastejacking defense.
- **Witness integrity + freshness + input gates.** Substrate nonce + monotonic timestamp gate per RFC-0871 §Data Structures governs rebind ordering (CLI builders emit substrate-derived freshness, operators CANNOT inject arbitrary values). Double-commit rejected via substrate idempotency per RFC-0871 §Algorithms (idempotency check on rebind coordinator `mark_committed`). Verify substrate confirms at `crates/octo-network/src/mon/rebind.rs` `RebindCoordinator::mark_committed(&mut self) -> ()` (infallible transition) + `RebindCoordinator::abort(&mut self, reason: RebindAbortReason) -> CoordinatorState` (infallible state-return per L1 substrate-faithfulness audit 2026-09-18). Substrate does NOT expose an `accept` method nor a Result-returning variant; both substrate methods are infallible state-transition primitives. Double-commit rejected via substrate idempotency check in `mark_committed` (infallible: returns `()` even on duplicate; CLI abort preview stays accurate). Rollback path returns the NEW `CoordinatorState` after the abort transition; CLI MAY inspect the returned state to confirm rollback completed. Tally freshness is substrate-owned via `VotingTally` (the struct definition is in `crates/octo-network/src/mon/governance.rs`); CLI never caches across invocations. RFC-0011-g governs the CLI attestation + vote calls, not the underlying `VotingTally` type surface. The `--depth` clamp 1-100 on `trust-graph render` is substrate OOM defense; `--depth 0` → exit 85; `--depth 1000000` → clamp 100 with warning.
- **Substrate-gated authority surfaces.** `SeedListAuthority::rotate_post_fork` gates at substrate layer per RFC-0851p-a; CLI does not bypass. Slash envelopes are substrate-generated only — operators cannot mint via CLI (`slash mark` is read-only inquiry). `authority show` surfaces `SeedAuthorityError::SeedListAuthorityDeprecated` Display verbatim when present (no secret material).
- **CLI scope boundaries + key compromise mitigation.** No private key access — `identity show` surfaces public-key-derived `GatewayIdentity` only. No multiaddr allowlist collision — `peers add` stays in `octo mesh peer add` per RFC-0011-f. Signing-identity key compromise mitigated by HSM wallet per RFC-0015-a §6.5 paired-acceptance bridge + `0011-h-s-a-attached-handle-key-rotation` companion substrate enabling key rotation invalidating stale signatures.

## Adversarial Review

| Threat                                                                                      | Impact                                                 | Mitigation                                                                                                                                                                                                  |
| ------------------------------------------------------------------------------------------- | ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Operator edits `bootstrap.toml` to corrupted value                                          | MEDIUM — CLI surfaces corrupt value; no network effect | `mode show` returns parsed enum value (3 valid variants) OR `NetworkConfigParseFailed` exit 82                                                                                                              |
| Operator runs `bind-envelope rebind-commit` on wrong target                                 | HIGH — irreversible state change if substrate commits  | `--dry-run` default + TWO confirm flags (`--confirm` + `--confirm-acknowledge`) + `--no-dry-run` triple + audit-log entry per RFC-0011-a §7.4 Substrate [ADD] signatures                                    |
| Operator runs `trust-graph render --depth 1000000`                                          | MEDIUM — substrate OOM                                 | `--depth` clamp 1-100; values below range (`--depth 0`) → exit 85; values above range (e.g. `--depth 1000000`) → clamped to 100 with warning; clap u32 parse error for negative values (exit 2 `ClapParse`) |
| `coordinator show` leaks coordinator ID in Audit mode                                       | LOW — DIDs are public; redaction is for consistency    | Per RFC-0011 §Redaction layer: `coordinator_id` surface redacted via `[REDACTED:coordinator_id]` in Audit mode                                                                                              |
| `slash stats` leaks peer DIDs in Audit mode                                                 | LOW — DIDs are public; redaction is for consistency    | Per-did counts surface with `[REDACTED:did]` placeholder in Audit mode                                                                                                                                      |
| `bind-envelope show` leaks payload bytes via `canonical_bytes_hash` collision or off-by-one | HIGH — payload disclosure                              | `canonical_bytes_hash` is blake3 hex only; raw `canonical_bytes()` NEVER called from CLI; redaction test vectors assert no payload byte leak                                                                |
| Adversary fabricates `BindEnvelope` payload via CLI mutation                                | MEDIUM — operator confusion                            | CLI builders construct payloads from typed substrate structs; CLI cannot inject arbitrary bytes                                                                                                             |
| Adversary chains `rebind-prepare → rebind-commit` rapidly                                   | LOW — substrate owns lifecycle                         | Substrate enforces ordering; CLI emits payloads only; substrate decides whether to accept                                                                                                                   |
| `governance tally` reads stale tally                                                        | LOW — substrate owns freshness                         | Tally substrate owned by `VotingTally` (struct defined in `crates/octo-network/src/mon/governance.rs`); CLI surfaces current state only                                                                     |     |
| `authority show` exposes deprecated authority                                               | LOW — informational                                    | `valid: bool=false` case explicitly surfaces `message: Option<String>` carrying the `Display` of `SeedAuthorityError::SeedListAuthorityDeprecated`; no network effect                                       |
| Adversary replays `bind-envelope rebind-prepare` payload via nonce/timestamp forgery        | HIGH — unintended rebind if accepted                   | Substrate nonce + timestamp gate per RFC-0871 §Data Structures; CLI surface emits payloads with substrate-derived freshness, never operator-supplied timestamp                                              |
| Adversary double-commits `bind-envelope rebind-commit`                                      | HIGH — duplicate state mutation                        | Substrate idempotency check per RFC-0871 §Algorithms on rebind coordinator accept; CLI cannot bypass via rapid chained invocations                                                                          |
| Signing-identity key compromise during rebind-*                                             | HIGH — adversary mints rebind under stolen key         | HSM wallet required per RFC-0015-a §6.5 paired-acceptance bridge; key rotation via `0011-h-s-a-attached-handle-key-rotation` companion substrate; rotation invalidates stale signatures                     |

### CI Mode Rationale (rebind-* subcommands)

| Subcommand            | CI behavior  | Rationale                                                                                                                                                                                                     |
| --------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `rebind-prepare`      | DENY default | CI gate fires regardless of `--confirm-acknowledge` presence; the only CI bypass is `--allow-ci-rebind` (DEBUG-ONLY escape hatch per §Security Considerations); CLI surfaces exit 90 `NetworkCIRebindDenied`. |
| `rebind-commit`       | DENY default | Irreversible state mutation; CI cannot satisfy double-confirm gate (`--confirm-acknowledge` + `--confirm`). CLI surfaces exit 90 `NetworkCIRebindDenied`.                                                     |
| `rebind-abort`        | DENY default | Roll-back state operation; substrate may interpret double-abort as no-op per RFC-0871 §Algorithms idempotency, but CI pastejacking risk remains. CLI surfaces exit 90 `NetworkCIRebindDenied`.                |
| `bind-envelope show`  | ALLOW in CI  | Read-only; safe for CI.                                                                                                                                                                                       |
| `peers/identity/etc.` | ALLOW in CI  | All read paths; per RFC-0008 §RFC-0008 Execution Class Mapping (read-side determinism).                                                                                                                       |

> **CI detection:** `OCTO_CLI_CI=1` env-var (set by most CI providers automatically; CLI double-checks with `[ -t 0 ]` on stdin). When set OR stdin is non-TTY, CI mode engages for all CI-DENY subcommands. CLI flag `--allow-ci-rebind` (DEBUG-ONLY, never documented in `--help`) is reserved for emergency operator override; surfaces in changelog as experimental and removed before v1.0.

The 3 CI-DENY subcommands reuse the existing CLI confirm-gate pattern. Exit code 90 (`NetworkCIRebindDenied`) maps to `OctoCliError::NetworkCIRebindDenied` (slot 90) per §Error Handling table above. No new substrate required; pure CLI gate per RFC-0011 §Confirmation Flag Matrix.

## Compatibility

**Backward:** `Commands::Network { NetworkAction }` is purely additive under `#[non_exhaustive]`. No existing CLI command renamed/removed. `octo mesh peer` (RFC-0011-f) remains mesh peer-table surface; this amendment adds `octo network peers` for gateway-cache surface (different substrate function — `octo_mesh::peers` vs `octo_network::gdp::cache::GatewayCache::iter`).

**Forward:** Future amendments may add `NetworkAction` variants additively (e.g., `slash mark`, `mode set`, `authority rotate` once companion substrate-additions missions close).

**Post-v2.0 stub-removal script breakage:** `octo network bootstrap` and `octo network status` are DEFERRED. Operators calling either receive clap `unrecognized subcommand` (exit 2). Migration: reroute to the `0011-h-s-a-bootstrap-orchestrator` companion mission (bootstrap) and to per-substrate `octo network peers list` + `octo network trust-graph render` (status-equivalent visibility).

**Cross-binary:** Operators using Python SDK / HTTP proxy (when RFC-0917 lands) will get same substrate-faithful slice via REST/gRPC. CLI is operator escape hatch. Full schema parity is OUT OF SCOPE for this RFC; cross-binary parity owned by RFC-0917.

## Test Vectors

Each subcommand has ≥2 test vectors per RFC-0011 §Test Vectors. Per RFC-0011-f pattern (lowercase `tv-network-<subcommand>-<num>`):

| Test vector ID                              | Subcommand                                                                                                                                                        | Scenario                                                                                                                                                                                        |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `tv-network-peers-list-1`                   | `peers list`                                                                                                                                                      | Empty cache returns `(empty)` body                                                                                                                                                              |
| `tv-network-peers-list-2`                   | `peers list`                                                                                                                                                      | 3-entry cache returns 3 summaries                                                                                                                                                               |
| `tv-network-peers-get-1`                    | `peers get <id>`                                                                                                                                                  | Existing entry returns full summary                                                                                                                                                             |
| `tv-network-peers-get-2`                    | `peers get <id>`                                                                                                                                                  | Missing entry returns exit 79 (`NetworkPeerNotFound`)                                                                                                                                           |
| `tv-network-identity-show-1`                | `identity show`                                                                                                                                                   | Local key available; constructs `GatewayIdentity`                                                                                                                                               |
| `tv-network-identity-show-2`                | `identity show`                                                                                                                                                   | Local key missing; returns exit 83 (`NetworkLocalKeyUnavailable`)                                                                                                                               |
| `tv-network-mode-show-1`                    | `mode show`                                                                                                                                                       | Valid TOML → `BootstrapMode::Direct` (default)                                                                                                                                                  |
| `tv-network-mode-show-2`                    | `mode show`                                                                                                                                                       | Invalid TOML → exit 82 (`NetworkConfigParseFailed`)                                                                                                                                             |
| `tv-network-authority-show-1`               | `authority show`                                                                                                                                                  | Foundation authority, pre-epoch → `Valid`                                                                                                                                                       |
| `tv-network-authority-show-2`               | `authority show`                                                                                                                                                  | Foundation authority, post-epoch → `Deprecated`                                                                                                                                                 |
| `tv-network-slash-excluded-1`               | `slash excluded <did>`                                                                                                                                            | DID with 3 slashes → `true`                                                                                                                                                                     |
| `tv-network-slash-excluded-2`               | `slash excluded <did>`                                                                                                                                            | DID with 0 slashes → `false`                                                                                                                                                                    |
| `tv-network-slash-stats-1`                  | `slash stats`                                                                                                                                                     | Empty reputation store → `distinct_did_count = 0`                                                                                                                                               |
| `tv-network-slash-stats-2`                  | `slash stats`                                                                                                                                                     | 5 DIDs / 12 total slashes → correct aggregate                                                                                                                                                   |
| `tv-network-redact-did-1`                   | (Audit mode redaction)                                                                                                                                            | `slash stats` in Audit mode → per-did counts surface literal `[REDACTED:did]` placeholders (asserts form per RFC-0011 §Redaction layer; raw DID byte sequence never appears in output envelope) |
| `tv-network-redact-coord-1`                 | (Audit mode redaction)                                                                                                                                            | `coordinator show` in Audit mode → `coordinator_peer_id` surfaces as `[REDACTED:coordinator_id]` placeholder                                                                                    |
| `tv-network-redact-gateway-1`               | (Audit mode redaction)                                                                                                                                            | `peers get` on missing entry → exit 79 error message carries `[REDACTED:gateway_id]` placeholder (never raw hex)                                                                                |
| `tv-network-trust-graph-render-1`           | `trust-graph render`                                                                                                                                              | 5-node graph + `--format ascii` → renders ASCII                                                                                                                                                 |
| `tv-network-trust-graph-render-2`           | `trust-graph render`                                                                                                                                              | 5-node graph + `--format dot` → renders DOT                                                                                                                                                     |
| `tv-network-trust-graph-render-3`           | `trust-graph render`                                                                                                                                              | `--depth 0` → exit 85                                                                                                                                                                           |
| `tv-network-trust-graph-render-4`           | `trust-graph render`                                                                                                                                              | `--depth 1000000` → clamped to 100 with warning                                                                                                                                                 |
| `tv-network-coordinator-show-1`             | `coordinator show`                                                                                                                                                | Active coordinator + lifecycle::Active                                                                                                                                                          |
| `tv-network-coordinator-show-2`             | `coordinator show`                                                                                                                                                | No coordinator → exit 84 (`NetworkCoordinatorNotFound`)                                                                                                                                         |
| `tv-network-governance-tally-1`             | `governance tally`                                                                                                                                                | Empty tally → `total_for = 0, total_against = 0`                                                                                                                                                |
| `tv-network-governance-tally-2`             | `governance tally`                                                                                                                                                | 7-for / 3-against tally → correct counts + canonical bytes hash                                                                                                                                 |
| `tv-network-governance-rotation-1`          | `governance rotation status`                                                                                                                                      | Pre-migration window → `in_migration_window = false`                                                                                                                                            |
| `tv-network-governance-rotation-2`          | `governance rotation status`                                                                                                                                      | In-migration window → `in_migration_window = true` + `migration_deadline_epoch` populated                                                                                                       |
| `tv-network-bind-envelope-show-1`           | `bind-envelope show`                                                                                                                                              | 3-participant envelope; canonical_bytes_hash present, no payload bytes surfaced                                                                                                                 |
| `tv-network-bind-envelope-show-2`           | `bind-envelope show`                                                                                                                                              | Missing envelope → substrate not-found error                                                                                                                                                    |
| `tv-network-bind-envelope-rebind-prepare-1` | `bind-envelope rebind-prepare`                                                                                                                                    | `--dry-run` default → `dry_run = true`                                                                                                                                                          |
| `tv-network-bind-envelope-rebind-prepare-2` | `bind-envelope rebind-prepare`                                                                                                                                    | `--confirm-acknowledge` + `--no-dry-run` + valid signer → `dry_run = false` + canonical `RebindPrepare` bytes + signature surfaced                                                              |
| `tv-network-bind-envelope-rebind-commit-1`  | `bind-envelope rebind-commit`                                                                                                                                     | `--dry-run` default → `dry_run = true`                                                                                                                                                          |
| `tv-network-bind-envelope-rebind-commit-2`  | `bind-envelope rebind-commit`                                                                                                                                     | Missing `--confirm-acknowledge` → exit 87 (`NetworkConfirmRequired`)                                                                                                                            |
| `tv-network-bind-envelope-rebind-commit-3`  | `bind-envelope rebind-commit`                                                                                                                                     | `--confirm-acknowledge` but missing `--confirm` SECOND flag → exit 87 (`NetworkConfirmRequired`) — pastejacking defense                                                                         |
| `tv-network-bind-envelope-rebind-commit-4`  | `bind-envelope rebind-commit`                                                                                                                                     | `--confirm-acknowledge` + `--confirm` + `--no-dry-run` + valid signer → `dry_run = false` + committed `RebindCommit` success                                                                    |
| `tv-network-bind-envelope-rebind-abort-1`   | `bind-envelope rebind-abort`                                                                                                                                      | `--reason BogusValue` → clap rejects (closed `RebindAbortReason` enum has 3 fixed variants: `VoteAbort`, `Timeout`, `LostTieBreak`) → exit 2 `UnrecognizedSubcommand`                           |
| `tv-network-bind-envelope-rebind-abort-2`   | `bind-envelope rebind-abort`                                                                                                                                      | `--reason timeout` + `--confirm-acknowledge` → success                                                                                                                                          |
| `tv-network-bind-envelope-rebind-prepare-4` | `bind-envelope rebind-prepare`                                                                                                                                    | Missing `--confirm-acknowledge` → exit 87 (`NetworkConfirmRequired`)                                                                                                                            |
| `tv-network-bind-envelope-rebind-abort-4`   | `bind-envelope rebind-abort`                                                                                                                                      | Missing `--confirm-acknowledge` → exit 87 (`NetworkConfirmRequired`)                                                                                                                            |
| `tv-network-discovery-advert-1`             | `discovery advertisement show`                                                                                                                                    | Existing advertisement + TTL OK → shown                                                                                                                                                         |
| `tv-network-discovery-advert-2`             | `discovery advertisement show`                                                                                                                                    | TTL exceeded → `is_ttl_exceeded = true` surfaced                                                                                                                                                |
| `tv-network-discovery-invite-1`             | `discovery invitation show`                                                                                                                                       | Existing invitation shown                                                                                                                                                                       |
| `tv-network-discovery-invite-2`             | `discovery invitation show`                                                                                                                                       | Missing invitation → not-found                                                                                                                                                                  |
| `tv-network-discovery-advert-3`             | `discovery advertisement show`                                                                                                                                    | `--hops 65536` → clap u16 parse error (exit 2); overflow rejected pre-dispatch                                                                                                                  |
| (REMOVED)                                   | (was `bind-envelope show`)                                                                                                                                        | Substrate `BindEnvelope::canonical_bytes()` is infallible (`Vec<u8>` return, not `Result`); overflow test path fabricated, REMOVED per substrate-faithfulness audit 2026-09-18                  |
| `tv-network-did-1`                          | `peers get <gateway_id>`                                                                                                                                          | Non-canonical DID form → exit 86 (`NetworkInvalidDid`) (DID redacted in variant name)                                                                                                           |
| `tv-network-bind-envelope-rebind-prepare-3` | `octo network bind-envelope rebind-prepare --envelope-id <redacted>` in CI mode (`OCTO_CLI_CI=1` set, `--confirm-acknowledge`, `--no-dry-run`)                    | exit 90 (`NetworkCIRebindDenied`); substrate `RebindCoordinator::mark_committed` not reached because CLI gate fires first per §CI Mode Rationale                                                |
| `tv-network-bind-envelope-rebind-commit-5`  | `octo network bind-envelope rebind-commit --envelope-id <redacted>` in CI mode (`OCTO_CLI_CI=1` set, `--confirm-acknowledge` + `--confirm`, `--no-dry-run`)       | exit 90 (`NetworkCIRebindDenied`); substrate `RebindCoordinator::mark_committed` not reached because CLI gate fires first per §CI Mode Rationale                                                |
| `tv-network-dry-run-deny-1`                 | `bind-envelope rebind-abort`                                                                                                                                      | `--dry-run` explicit + operator denied at prompt → exit 88 (`NetworkDryRunDenied`)                                                                                                              |
| `tv-network-bind-envelope-rebind-abort-3`   | `octo network bind-envelope rebind-abort --envelope-id <redacted> --reason <redacted>` in CI mode (`OCTO_CLI_CI=1` set, `--confirm-acknowledge` + `--no-dry-run`) | exit 90 (`NetworkCIRebindDenied`); substrate `RebindCoordinator::abort` not reached because CLI gate fires before substrate dispatch per §CI Mode Rationale                                     |

## Alternatives Considered

| Approach                                                            | Pros                                                                 | Cons                                                                                               |
| ------------------------------------------------------------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| **A: Umbrella amendment with substrate-faithful slice (this RFC).** | Single source of truth; one DRY cycle; per-extension substrate guard | Smaller scope than 20-GAP vision; deferred subcommands need own missions                           |
| B: Per-subcommand amendments (`0011-h1`, `0011-h2`, ...)            | Smaller individual reviews                                           | 15 separate RFC DRY cycles; harder to keep cross-citations consistent                              |
| C: Extend `octo mesh` to absorb network surface                     | Single CLI namespace for "mesh-shaped" things                        | Conflates RFC-0011-f peer table with RFC-0851p-a bootstrap; breaks substrate-faithfulness boundary |
| D: Defer to RFC-0917 only (Python SDK or HTTP proxy available)      | No CLI work; substrate automation only                               | Operators have no escape hatch; CLI is operator surface per CLAUDE.md §Branch Strategy             |

**Selected: A.** Substrate-faithful umbrella pattern. Rejected alternatives E (20-subcommand umbrella) per [[substrate-faithfulness-verification]] (fabricated ~19 substrate types/methods) and F (substrate-additions only, no umbrella CLI) per horizon-too-long (no CLI surface until ALL companion missions close). Both rejected at R1 substrate audit 2026-09-18. Companion-mission pattern retained: 5-GAP deferred slice (G1 `bootstrap`/`mode set`, G6 `slash list/show`, G8 `authority rotate`, G12 `coordinator admin`, G15 `gossip stats`) — note: G7 and G19 are SUBSUMED into G1 and G9 respectively per §Substrate-Additions; they are not standalone GAPs — each becomes its own substrate-additions mission YAML; once any closes, follow-on CLI mission extends the umbrella.

## Substrate-Additions Companion Missions

> **Draft-time pointer convention:** Mission YAMLs cited in this section are PLANNED companions. Each opens its own DRY CLOSURE gate when the corresponding substrate-additions work enters Phase 1 of its own RFC/mission cycle. RFC-0011-h Draft status does not require these files to exist; they materialize when each companion mission is CLAIMED per [[no-phantom-mission-pointers]].

Each entry is substrate-first mission YAML that lands missing substrate type/method, gated BEFORE corresponding CLI mission. Each `depends_on:` RFC-0011-h + listed networking RFC(s).

| GAP  | Substrate-additions companion mission                           | Adds to substrate                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | Required by CLI mission (this RFC)                                                                                                             |
| ---- | --------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| G1   | `0011-h-s-a-bootstrap-orchestrator`                             | `BootstrapOrchestrator` struct + `start_bootstrap(BootstrapConfig) → BootstrapOutcome` + `status() → BootstrapState` + `BootstrapConfig::{from_toml, save_toml}` + `BootstrapConfig::bootstrap_mode` accessor                                                                                                                                                                                                                                                                                                                                                                                          | `octo network bootstrap` (deferred); `octo network status` (deferred)                                                                          |
| G2   | (subsumed by G1)                                                | —                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | —                                                                                                                                              |
| G3b  | `0011-h-s-a-voting-tally-canonical-bytes`                       | Substrate-additions companion for `governance tally`: add free fn `governance_proposal_canonical_bytes(&GovernanceProposal) -> [u8; 32]` in `octo-governance` Layer B facade. `GovernanceProposal` itself is Layer A frozen (`crates/octo-governance-core/src/proposal.rs`); the helper is a free fn that does NOT mutate the type, preserving the Layer A frozen crypto contract via the underlying `blake3_hash` reference. `mon/governance.rs` Layer B imports the type and provides `VotingTally::into_canonical` that converts tally state to a `GovernanceProposal`. Verified absent 2026-09-18. | `octo network governance tally` (this amendment) — helper must close BEFORE CLI mission opens                                                  |
| G6   | `0011-h-s-a-slash-store`                                        | Extend existing `SlashReputationStoreCompat` Layer B façade (`crates/octo-network/src/reputation/slash_store.rs`); add `list(reason_filter: Option<u16>) → Vec<SlashEnvelopeSummary>` + `show(slash_id: [u8;32]) → Option<SlashEnvelope>`. No new parallel CRUD façade. `mon/slash_aggregation.rs` is SEPARATE per [[cipherocto-design-principles]] §Separation of concerns.                                                                                                                                                                                                                           | `octo network slash list/show` (deferred)                                                                                                      |
| G6b  | `0011-h-s-a-slash-store-loader`                                 | `query_attestations → SlashReputationStoreCompat` persistence→façade hydration path (cross-restart only; read methods substrate-faithful today)                                                                                                                                                                                                                                                                                                                                                                                                                                                        | `octo network slash stats` + `slash excluded` (this amendment) — loader must close BEFORE CLI mission opens for cross-restart persistence only |
| G7   | (subsumed by G1)                                                | —                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | `octo network mode set` (deferred)                                                                                                             |
| G8   | `0011-h-s-a-seed-list-authority-rotate`                         | `SeedListAuthority::rotate_post_fork(new_authority, governance_quorum_proof) → Result<SeedListAuthority, SeedAuthorityError>`                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | `octo network authority rotate` (deferred)                                                                                                     |
| G9   | `0011-h-s-a-slash-bridge-trait`                                 | `SlashBridge` trait + `list() → Vec<BridgedSlash>` + `propagate_to(slash_envelope_id: [u8;32]) → Result<BridgeReceipt, BridgeError>`                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | `octo network slash-bridge list/propagate` (deferred)                                                                                          |
| G10  | `0011-h-s-a-quota-router-node`                                  | `QuotaRouterNode` struct + `status() → RouterStatus` + `peer_capacity(peer_node_id) → u64` (RFC-0870 substrate)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | `octo network router status/peers` (deferred)                                                                                                  |
| G11  | `0011-h-s-a-specialized-node-record`                            | `SpecializedNodeRecord::load(node_id) → Option<SpecializedNodeRecord>` + `bind_to_did(holder_did: &Did)`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `octo network node show/bind` (deferred)                                                                                                       |
| G12  | `0011-h-s-a-coordinator-admin-trait`                            | Coordinator admin substrate (rotate, suspend, reactivate) + `CoordinatorRecord::load(coordinator_id: &CoordinatorId)` (post-G12b landing). Companion mission `0011-h-s-a-coordinator-admin-trait` is the substrate deliverable; governing RFC for these admin actions lands in a follow-up amendment per BLUEPRINT ordering (substrate-first)                                                                                                                                                                                                                                                          | `octo network coordinator admin` (deferred; CLI surface also gated on the RFC amendment)                                                       |
| G12b | `0011-h-s-a-coordinator-record-loader`                          | `CoordinatorRecord::load(coordinator_id: &CoordinatorId) -> Option<CoordinatorRecord>` additive to `octo-coordinator-types` Layer B; verified absent 2026-09-18                                                                                                                                                                                                                                                                                                                                                                                                                                        | `octo network coordinator show` (this amendment) — loader must close BEFORE CLI mission opens                                                  |
| G13  | `0011-h-s-a-reputation-store`                                   | `ReputationStore` struct + `list(filter: ReputationFilter) → Vec<PeerReputation>` + `peer_reputation(did)` (RFC-0860 substrate)                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | `octo network reputation show/list` (deferred)                                                                                                 |
| G14  | `0011-h-s-a-topology-render`                                    | `Topology` struct + `render(format: GraphFormat) → String` impl                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | `octo network topology` (deferred)                                                                                                             |
| G15  | `0011-h-s-a-gossip-stats`                                       | `Gossip::stats() → GossipStats` impl (substrate-side anti-entropy counter)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | `octo network gossip --stats` (deferred)                                                                                                       |
| G16  | `0011-h-s-a-envelope-inspector` + `0011-h-s-a-forward-envelope` | `EnvelopeInspector::inspect(envelope_id) → EnvelopeMeta` + `ForwardEnvelope::build(...) → ForwardEnvelope`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | `octo network envelope inspect/forward` (deferred)                                                                                             |
| G17  | `0011-h-s-a-heartbeat-probe`                                    | `Heartbeat::probe(peer_did: &Did) → HeartbeatProbeResult` impl                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | `octo network heartbeat` (deferred)                                                                                                            |
| G18  | `0011-h-s-a-writer-election`                                    | `WriterElection` struct (RFC-0862) + `state() → ElectionState` + `cast_vote(candidate_id, weight)`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | `octo network election show/cast-vote` (deferred)                                                                                              |
| G19  | (subsumed by G9)                                                | —                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | —                                                                                                                                              |
| G20  | `0011-h-s-a-network-sender`                                     | `NetworkSender` trait + `SendContext` + `send(envelope) → Result<SendReceipt, SendError>` (RFC-0863 substrate). Per per-extension crate pattern (trait in Layer B `octo-network`, per-transport impl crates in Layer D, registry lookup at runtime) per [[cipherocto-design-principles]] §User extensibility                                                                                                                                                                                                                                                                                           | `octo network routing send` (deferred)                                                                                                         |
| G21  | `0011-h-s-a-attached-handle-key-rotation`                       | `AttachedHandleKeyRotation` trait + `current_key_id() -> KeyId` + `rotate_key(new_key_id: KeyId, signed_rotation_intent: SignedIntent<RotationPayload>) -> Result<KeyId, RotationError>` (RFC-0011-c §F.5.1 D2.2 forward projection — D2.1 only landed the `KeyId` + `KeySet` primitives behind `octo-attach-key-rotation` cfg). 1 `RotationError` variant (`UnknownKeyId`). Required by `bind-envelope rebind-prepare` + `bind-envelope rebind-commit` payload builders.                                                                                                                              | `octo network bind-envelope rebind-prepare/rebind-commit` (deferred)                                                                           |

**Substrate-first ordering invariant:** Each CLI mission YAML `depends_on:` above the substrate-additions companion mission. Per [[cipherocto-design-principles]] §No premature coupling + [[feedback-no-fabricated-commit-rule]], CLI cannot promise substrate call that doesn't exist. Each companion mission closes its own DRY CLOSED gate before its corresponding CLI mission opens.

## Implementation Phases

### Phase 1: Peers + Identity + Trust Graph (highest priority — gateway-cache + read-only observability)

- [ ] Mission `0011-h-network-peers-identity` (peers + identity)
  - [ ] `peers list` (read); `peers get <gateway_id>` (read); `NetworkPeersListOutput` + `NetworkPeerGetOutput` (new envelopes)
  - [ ] `identity show` (read); local public-key read → `GatewayIdentity::new`; `NetworkIdentityShowOutput`
  - [ ] 3 `OctoCliError` variants (`NetworkPeerNotFound`, `NetworkLocalKeyUnavailable`, `NetworkInvalidDid`)
  - [ ] Test vectors tv-network-peers-list-1/2 + tv-network-peers-get-1/2 + tv-network-identity-show-1/2
- [ ] Mission `0011-h-network-trust-graph`
  - [ ] `trust-graph render` (read); `--depth 1-100`; `--format ascii|dot`; `TrustGraphOutput`
  - [ ] 1 `OctoCliError` variant (`NetworkGraphDepthBelowRange`)
  - [ ] Test vectors tv-network-trust-graph-render-1/2/3/4

### Phase 2: Mode + Authority + Slash Stats

- [ ] Mission `0011-h-network-mode`
  - [ ] `mode show` (read); local config parse → `BootstrapMode`; `NetworkModeShowOutput`
  - [ ] 1 `OctoCliError` variant (`NetworkConfigParseFailed`)
  - [ ] Test vectors tv-network-mode-show-1/2
- [ ] Mission `0011-h-network-authority`
  - [ ] `authority show` (read); `verify_authority(SeedListAuthority, current_epoch)`; `NetworkAuthorityShowOutput`
  - [ ] 0 new `OctoCliError` variants
  - [ ] Test vectors tv-network-authority-show-1/2
- [ ] Mission `0011-h-network-slash-stats`
  - [ ] `slash excluded <did>` (read); `slash stats` (read); `NetworkSlashStatsOutput` + `NetworkSlashExcludedOutput` (new envelopes)
  - [ ] 0 new `OctoCliError` variants
  - [ ] Test vectors tv-network-slash-excluded-1/2 + tv-network-slash-stats-1/2

### Phase 3: Coordinator + Governance

- [ ] Mission `0011-h-network-coordinator`
  - [ ] `coordinator show` (read); `CoordinatorRecord` load from `octo-coordinator-types`; `NetworkCoordinatorShowOutput`
  - [ ] 1 `OctoCliError` variant (`NetworkCoordinatorNotFound`)
  - [ ] Test vectors tv-network-coordinator-show-1/2
- [ ] Mission `0011-h-network-governance`
  - [ ] `governance tally` (read); `governance rotation status` (read); `NetworkGovernanceTallyOutput` + `NetworkGovernanceRotationOutput` (new envelopes)
  - [ ] 0 new `OctoCliError` variants
  - [ ] Test vectors tv-network-governance-tally-1/2 + tv-network-governance-rotation-1/2

### Phase 4: Bind Envelope (read + payload builders)

- [ ] Mission `0011-h-network-bind-envelope`
  - [ ] `bind-envelope show` (read); `BindEnvelope::{canonical_bytes, is_participant}` with blake3 hash surfaced via `NetworkBindEnvelopeShowOutput.canonical_bytes_hash`; raw bytes NEVER echoed
  - [ ] `bind-envelope rebind-prepare` (write; `--dry-run` default; BLOCKED — pending G21 `0011-h-s-a-attached-handle-key-rotation` closure); `RebindPrepare` builder; `NetworkRebindPrepareOutput`
  - [ ] `bind-envelope rebind-commit` (write; `--dry-run` default; BLOCKED — pending G21 `0011-h-s-a-attached-handle-key-rotation` closure); `RebindCommit` builder; `NetworkRebindCommitOutput` (new envelope)
  - [ ] `bind-envelope rebind-abort` (write; `--dry-run` default); `RebindAbort { domain_id, reason, dissenters, signature }` builder per `crates/octo-network/src/mon/bind_envelope.rs`; `NetworkRebindAbortOutput` (new envelope)
  - [ ] 3 `OctoCliError` variants (`NetworkConfirmRequired`, `NetworkDryRunDenied`, `NetworkCIRebindDenied`); slots 80, 81 RESERVED per Phase 4 substrate-faithfulness audit; slot 89 RESERVED globally for substrate-not-loaded (covers Phases 2-4 BLOCKED rows) per audit 2026-09-18
  - [ ] Test vectors tv-network-bind-envelope-show-1/2 + tv-network-bind-envelope-rebind-prepare-1/2 + tv-network-bind-envelope-rebind-commit-1/2/3/4 + tv-network-bind-envelope-rebind-abort-1/2 (commit-3 covers missing `--confirm` SECOND flag exit 87 pastejacking defense; commit-4 covers triple-flag valid signer success)

### Phase 5: Discovery

- [ ] Mission `0011-h-network-discovery`
  - [ ] `discovery advertisement show` (read); `discovery invitation show` (read); `NetworkDiscoveryAdvertisementOutput` + `NetworkDiscoveryInvitationOutput` (new envelopes)
  - [ ] 0 new `OctoCliError` variants
  - [ ] Test vectors tv-network-discovery-advert-1/2/3 + tv-network-discovery-invite-1/2

### Phase 6: Closure artifacts

- [ ] Drift-closure mission `0011-h-0851p-a-seed-health-check-archive` — archive `missions/claimed/0851p-a-seed-health-check.md` (frontmatter `status: Claimed` + §Status prose "LANDED 2026-08-13 (drift-closure)" mismatch per [[memory-is-never-status-ground-truth]])
- [ ] File the 2 missing follow-on missions: `missions/open/0851p-a1-prometheus-metric-export.md` + `missions/open/0851p-a2-operator-guide.md`
- [ ] Final audit doc + memory card + MEMORY.md index entry
- [ ] Per-RFC amendments and Draft promotions deferred to §Future Work items F8 + F9; each requires its own DRY cycle. Out of scope for THIS amendment's DRY CLOSED gate; cross-cited only.

## Key Files to Modify

| File                                                          | Change                                                                                                                    |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `rfcs/accepted/process/0011-h-oct-cli-network-subcommands.md` | NEW (this RFC, promoted from `rfcs/draft/process/`)                                                                       |
| `crates/octo-cli/src/lib.rs`                                  | ADD `Commands::Network { NetworkAction }` variant                                                                         |
| `crates/octo-cli/src/commands/mod.rs`                         | ADD `pub mod network;` + `pub use network::NetworkAction;`                                                                |
| `crates/octo-cli/src/commands/network.rs`                     | NEW (~500 lines; clap derive + handlers + output envelopes + tests)                                                       |
| `crates/octo-cli/src/error.rs`                                | ADD 12 `OctoCliError` variants (slots 79-90; slots 80, 81, 89 RESERVED; 9 NEW + 3 RESERVED; `#[non_exhaustive]` additive) |
| `crates/octo-cli/src/output.rs`                               | NEW envelope types per Phase 1-5                                                                                          |
| `crates/octo-cli/tests/network.rs`                            | NEW (~400 lines; test vectors per subcommand)                                                                             |
| `rfcs/accepted/networking/<each-RFC>`                         | ADD §Network CLI Surface section (Phase 2.2 per-RFC amendments)                                                           |

## Future Work

- **F1 — Per-extension crates for network adapters.** Per [[cipherocto-design-principles]] §User extensibility, future network adapter extensions MAY live as separate Layer D crates. CLI MAY gain `--adapter <name>` flags. Out of scope.
- **F2 — Network streaming tail mode.** `octo network envelope inspect --follow` style streaming tail. Per RFC-0011-a pattern (streaming surface for `audit watch`). Out of scope.
- **F3 — Marketplace / pricing integration.** Quota router marketplace pricing (RFC-0900 series). Out of scope.
- **F4 — CLI automation hooks.** RFC-0917 Python SDK + HTTP proxy as programmatic surface. CLI is operator escape hatch.
- **F5 — Multi-network topology.** Operator selects single network identity per CLI invocation (per `octo_network::config::active_network()` accessor). Aggregating multiple networks in one invocation is a future amendment.
- **F6 — Network simulation / replay.** Substrate-side replay surface for testing. Out of scope.
- **F7 — Federation across CipherOcto instances.** Per RFC-0863 + RFC-0871. Out of scope.
- **F8 — Per-RFC `§Network CLI Surface` amendments.** RFC-0851, RFC-0851p-b, RFC-0855, RFC-0855p-c, RFC-0855p-b, RFC-0861, RFC-0862, RFC-0871, RFC-0008 each gain a `§Network CLI Surface` section cross-citing this amendment. Each amendment is its own DRY cycle. Out of scope for this amendment.
- **F9 — Draft→Accepted promotions for 10 networking RFCs.** RFC-0850p-d, RFC-0850p-e, RFC-0850p-f, RFC-0852, RFC-0854, RFC-0856, RFC-0857, RFC-0858, RFC-0859, RFC-0860 — each requires its own DRY cycle before promotion. Out of scope for this amendment.

## Rationale

Substrate-faithful umbrella pattern chosen because:

1. **One CLI namespace for network family.** Per RFC-0011-a through RFC-0011-g pattern, each CLI surface gets top-level namespace. `octo network` is natural namespace for substrate-faithful slice.

2. **Substrate-faithful boundary.** CLI never reaches into substrate internals; every subcommand crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle + §Attenuation invariants cross boundaries.

3. **Substrate-first ordering via companion missions.** Each deferred subcommand has explicit substrate-additions companion mission in §Substrate-Additions Companion Missions. CLI missions gated on companion mission closure per [[feedback-no-fabricated-commit-rule]] + [[never-guess-hard-check]].

4. **No central enum edit for extension-bearing types.** `NetworkAction` is `#[non_exhaustive]`; future amendments add variants additively. Per [[cipherocto-design-principles]] §Extension over enumeration.

5. **Layer discipline preserved.** All additions are Layer C (CLI). Zero Layer A (crypto + canonical) changes. Substrate Layer B unchanged by this amendment (substrate-additions companion missions land Layer B changes; each is own DRY cycle).

6. **R1 dry-review lesson encoded.** Original R1 draft produced ~30 CRIT substrate-faithfulness findings. Per [[substrate-faithfulness-verification]], substrate must be verified by grep BEFORE RFC is drafted, not after. Current rewrite grounded in actual `octo-network` cargo grep output (verified 2026-09-18).

## Version History

| Version | Date       | Changes                                                                     |
| ------- | ---------- | --------------------------------------------------------------------------- |
| 0.1     | 2026-09-18 | Initial Draft. 17 subcommands, 5 families, 19 companion missions.           |
| 0.2     | 2026-09-18 | R5.5: fabricated fn refs; wire-format notes.                                |
| 0.3     | 2026-09-18 | R6.5: cite sweep + slot table + adversarial.                                |
| 0.4     | 2026-09-18 | R9.5: blake3 cite + GAPs + Security + Layer.                                |
| 0.5     | 2026-09-18 | R10.5: dry_run rename + matrix rows + bind-chain integrity + F5 hedge trim. |
| 0.6     | 2026-09-18 | R11.5: RebindAbort fields + BLOCKED + blake3 canonicalize.                  |
| 0.7     | 2026-09-18 | R13.5: substrate-faithful fields + blake3 cite + new_bind struct.           |
| 0.8     | 2026-09-18 | R14.5: type defs + Authority col + slot 85 + commit-3/4 TV.                 |

## Related RFCs

**Required:**

- RFC-0011 — `octo` CLI Substrate
- RFC-0008 — Deterministic AI Execution Boundary
- RFC-0010 — Canonical DID Codec
- RFC-0851 — Gateway Discovery Protocol
- RFC-0851p-a — Network Bootstrap Protocol
- RFC-0855 — Mission Overlay Networks
- RFC-0855p-b — Coordinator Lifecycle
- RFC-0855p-c — Domain Coordinator Role
- RFC-0861 — Coordinator Admin Trait Refinements
- RFC-0862 — Writer Election Bootstrap
- RFC-0871 — Specialized Node Protocol Envelope

**Optional (cross-cited in use cases, not directly substrate for subcommands):**

- RFC-0851p-b — DotDomain Bootstrap Mode (cross-cited in `docs/use-cases/social-platform-transport-layer.md` per §Related Use Cases)
- RFC-0852 — Deterministic Gossip Protocol
- RFC-0853 — Overlay Cryptography
- RFC-0854 — Deterministic Proof Substrate
- RFC-0856 — Deterministic Route Selection
- RFC-0857 — Deterministic Overlay Mempool
- RFC-0858 — Onion Relay Routing
- RFC-0859 — Proof-Carrying Envelopes
- RFC-0860 — Proof of Relay
- RFC-0863 — General-Purpose Network Integration
- RFC-0870 — Distributed Quota Router Network

**Amendment chain:**

- RFC-0011-a — audit subcommands
- RFC-0011-b — reputation subcommands
- RFC-0011-c — agent lifecycle
- RFC-0011-d — role provisioning
- RFC-0011-e — vault operations
- RFC-0011-f — mesh operations
- RFC-0011-g — governance subcommands
- **RFC-0011-h — network operations**

## Related Use Cases

- `docs/use-cases/dot-network-bootstrap.md` — RFC-0851p-a intent
- `docs/use-cases/node-operations.md` — RFC-0851 + RFC-0855 + RFC-0871 intent
- `docs/use-cases/bandwidth-provider-network.md` — RFC-0855p-d intent
- `docs/use-cases/compute-provider-network.md` — RFC-0870 intent
- `docs/use-cases/storage-provider-network.md` — RFC-0862 intent
- `docs/use-cases/enhanced-quota-router-gateway.md` — RFC-0870 intent
- `docs/use-cases/reputation-persistence.md` — RFC-0860 intent
- `docs/use-cases/social-platform-transport-layer.md` — RFC-0850p-c + RFC-0851p-b intent
- `docs/use-cases/wallet-as-specialized-node.md` — RFC-0871 intent
- `docs/use-cases/decentralized-mission-execution.md` — RFC-0855 intent
- `docs/use-cases/mission-coordinator-lifecycle.md` — RFC-0855p-b intent
- `docs/use-cases/telegram-auth-onboarding.md` — RFC-0850ab-a intent
- `docs/use-cases/stoolap-data-sync-via-cipherocto-network.md` — RFC-0862 intent

## Appendices

### B. Layer Direction Verification

Per [[cipherocto-design-principles]] §Stable Abstractions Principle + §No premature coupling:

- CLI (`octo-cli`, Layer C) → substrate (`octo-network` Layer B; `octo-mesh` Layer C per RFC-0011-f): ✅ via typed façade calls
- Substrate (Layer B) → CLI (Layer C): ❌ never (substrate does not know about CLI)
- CLI (Layer C) → persistence adapters (Layer D): ❌ CLI does not reach into Layer D directly
- Substrate (Layer B) → Layer D: ✅ substrate façade calls Layer D adapters

No reverse dependencies. No central enum edits. No per-extension crate pattern violations.

### C. Mission YAML Pattern Reference

Each CLI mission YAML in `missions/open/0011-h-network-*.md` follows RFC-0011-c companion mission pattern (single mission, multiple ACs, 2-round DRY minimum). Two pattern variants:

#### Pattern A — Substrate-faithful CLI mission (no BLOCKED deps)

Example: `0011-h-network-mode` (covers only `mode show`; substrate exists; no BLOCKED dep):

```markdown
---
name: 0011-h-network-mode
description: octo network mode show subcommand per RFC-0011-h Phase 2
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: ...
  created: 2026-09-18
  v: "1.0"
  depends_on:
    - RFC-0011-h
    - RFC-0851p-a
status: Open
---
```

#### Pattern B — CLI mission with BLOCKED substrate-additions companion

When a CLI subcommand needs substrate that does NOT yet exist, the mission YAML adds an inline `depends_on:` note pointing to the substrate-additions companion mission. The companion mission is itself Pattern C (below) and must close its DRY CLOSED gate before this CLI mission opens.

```markdown
---
name: 0011-h-network-bootstrap
description: octo network bootstrap subcommand per RFC-0011-h Phase 6
metadata:
  node_type: substrate-cli
  type: cli-substrate
  originSessionId: ...
  created: 2026-09-18
  v: "1.0"
  depends_on:
    - RFC-0011-h
    - RFC-0851p-a
    - mission 0011-h-s-a-bootstrap-orchestrator (BLOCKED — must close first)
status: Open
---
```

#### Pattern C — Substrate-additions companion mission (Layer B)

Each entry in §Substrate-Additions Companion Missions takes this shape. Single mission, multiple ACs, 2-round DRY minimum. The mission lands the missing substrate type/method BEFORE its paired CLI mission opens.

````markdown
---
name: 0011-h-s-a-bootstrap-orchestrator
description: Substrate additions for BootstrapOrchestrator + BootstrapConfig + BootstrapState per RFC-0011-h §Substrate-Additions
metadata:
  node_type: substrate-cli
  type: substrate-additions
  originSessionId: ...
  created: 2026-09-18
  v: "1.0"
  depends_on:
    - RFC-0011-h
    - RFC-0851p-a
    - mission 0851p-a-base-bootstrap-orchestrator (archived)
status: Open
---

## 0011-h-s-a-bootstrap-orchestrator — Substrate additions for `BootstrapOrchestrator`

### Status

Open (2026-09-18) — Substrate-additions prerequisite for RFC-0011-h Phase 1 (bootstrap) + Phase 2 (mode set)

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G1

## Summary

Adds to `crates/octo-network/src/mon/bootstrap.rs`:

```rust
pub struct BootstrapOrchestrator { /* substrate-owned state */ }
pub enum BootstrapState { Idle, Running, Complete, Failed }
pub struct BootstrapOutcome { acquired_peers: u32, elapsed_ms: u64, seed_health: SeedHealth }
pub struct BootstrapConfig { pub bootstrap_mode: BootstrapMode, /* ... */ }

impl BootstrapOrchestrator {
    pub fn start_bootstrap(&mut self, config: BootstrapConfig) -> Result<BootstrapOutcome, BootstrapError>;
    pub fn status(&self) -> BootstrapState;
}
impl BootstrapConfig {
    pub fn from_toml(path: &Path) -> Result<Self, TomlError>;
    pub fn save_toml(&self, path: &Path) -> Result<(), TomlError>;
}
```

## Acceptance Criteria

- [ ] New types ADDED to `octo-network` Layer B (zero Layer A change)
- [ ] `BootstrapOrchestrator::start_bootstrap` deterministic given identical substrate state
- [ ] `BootstrapConfig::{from_toml, save_toml}` round-trip stable
- [ ] ≥3 unit tests + ≥2 integration tests
- [ ] `Cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-network --lib` green

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands. Substrate `BootstrapMode` enum exists per `BootstrapMode` (RFC-0851p-a §3 Mode A).

## Out of Scope

- `octo network mode set` — DEFERRED per `0011-h-s-a-bootstrap-orchestrator` companion mission (substrate-additions prerequisite).

## Notes

Phase 2 of RFC-0011-h. Pair with `0011-h-network-authority` + `0011-h-network-slash-stats` for Phase 2 closure.
````

Example showing the structure for one of the substrate-helper companions (e.g. `0011-h-s-a-voting-tally-canonical-bytes`) follows the same Pattern C shape but ACs are substrate-side (Layer B additions to `octo-governance` facade, with re-exports of Layer A frozen types from `octo-governance-core`).

Each CLI mission YAML in `missions/open/0011-h-network-*.md` follows Pattern A or Pattern B above (CLI-side missions), each substrate-additions companion YAML in `missions/open/0011-h-s-a-*.md` follows Pattern C.

### D. Drift-Checkpoint Cross-References

See §Implementation Phases Phase 6 (single canonical closure artifact list per R24.5 dedup sweep).

### E. Substrate-Faithful Amendment Trail

This RFC's R1 dry-review ground-truth grep (2026-09-18) verified every cited substrate type/method against `crates/octo-network/src/`. R1 substrate-faithfulness audit produced ~30 CRIT findings against original 19-subcommand draft; this 17-subcommand rewrite reflects actual substrate. Future amendments that add `NetworkAction` variants MUST re-verify substrate citations via cargo grep before drafting, per [[substrate-faithfulness-verification]].

```

```
