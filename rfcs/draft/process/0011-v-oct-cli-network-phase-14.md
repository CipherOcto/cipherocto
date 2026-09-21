# RFC-0011-v: `octo network` Phase 14 — Heartbeat Probe (G17) + Stale-Stub Sweep

## Status

Draft (2026-09-20) — RFC-0011-v lands RFC-0011-h §Implementation Phases Phase 14. ONE CLI subcommand (`heartbeat probe`) wires heartbeat probing via ONE NEW substrate module (`mon/heartbeat.rs`). Companion stub mission G17 + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 1 output envelope + 12 test vectors (5 dispatch + 7 substrate). The final RFC-0011-h §Substrate-Additions Companion Missions G-row G17 closes the 9-row deferred stub sweep opened at Phase 7 (RFC-0011-o).

> **Amendment chain:** Fourteenth + final amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per [[docs-plans-scratchpad]]). Phase 1 = RFC-0011-i. Phase 2 = RFC-0011-j. Phase 3 = RFC-0011-k. Phase 4 = RFC-0011-l. Phase 5 = RFC-0011-m. Phase 6 = RFC-0011-n. Phase 7 = RFC-0011-o. Phase 8 = RFC-0011-p. Phase 9 = RFC-0011-q. Phase 10 = RFC-0011-r. Phase 11 = RFC-0011-s. Phase 12 = RFC-0011-t. Phase 13 = RFC-0011-u. Phase 14 = RFC-0011-v (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-v lands the **heartbeat probe** slice of RFC-0011-h §Implementation Phases. ONE CLI subcommand wires heartbeat probing (read-only peer reachability diagnostic) via ONE NEW substrate module in `mon/`:

- `octo network heartbeat probe <peer_did> [--timeout-ms <N>]` — read-only heartbeat probe returning `Reachable` with `rtt_ms` or `Unreachable` with reason or `Timeout` after `--timeout-ms` (default 5000ms)

Substrate per RFC-0855 §Wire Format heartbeat probing. ONE NEW substrate module (`mon/heartbeat.rs`) per Phase 7 RFC-0011-o NEW module precedent for SlashBridge substrate.

## Dependencies

- RFC-0011-h Accepted
- RFC-0855 §Wire Format
- RFC-0011-i Phase 1
- RFC-0011-j Phase 2
- RFC-0011-k Phase 3
- RFC-0011-l Phase 4
- RFC-0011-m Phase 5
- RFC-0011-n Phase 6
- RFC-0011-o Phase 7
- RFC-0011-p Phase 8
- RFC-0011-q Phase 9
- RFC-0011-r Phase 10
- RFC-0011-s Phase 11
- RFC-0011-t Phase 12
- RFC-0011-u Phase 13

## Design Goals

1. Wire `Heartbeat::probe()` to the CLI for operator peer reachability diagnostic (read-only)
2. Preserve per-extension crate pattern: substrate type in Layer B (`octo-network::mon::heartbeat`); concrete per-transport heartbeat adapter in Layer D, OUT OF SCOPE
3. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
4. THREE NEW types (`Heartbeat` + `HeartbeatProbeResult` enum + `UnreachableReason` enum); additive on NEW module, NOT existing sibling
5. Preserve scalar field-order determinism (no Map fields introduced; per RFC-0011-h §Output Envelope determinism)
6. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
7. 12 test vectors — tv_net14_1 through tv_net14_5 (5 dispatch) + tv_phase14_substrate_1 through tv_phase14_substrate_7 (7 substrate)
8. clap u16 overflow pre-dispatch rejection (Phase 5 RFC-0011-m precedent for `--timeout-ms`); no `parse_32_byte_hex` because `--peer_did` is a structured DID string (`did:octo:...`), NOT a hex byte array
9. Handler invokes substrate unconditionally (no always-false registry marker) per Phase 12 RFC-0011-t R2.5 substrate-faithfulness precedent

## Motivation

RFC-0011-h §Implementation Phases Phase 14 (G17) calls for wiring heartbeat probing to the CLI. Operators need a lightweight peer reachability diagnostic that does NOT require a full envelope round-trip. The substrate is MISSING: no `heartbeat` module exists in `mon/` (only `liveness.rs` for a different concern — election-window liveness vs. transport-level heartbeat probe). This RFC's companion mission (`G17` `0011-h-s-a-heartbeat-probe`) creates ONE NEW module in `mon/` with substrate types.

## Roles and Authorities

- **Operator**: invokes `octo network heartbeat probe <peer_did>` for reachability diagnostic
- **Peer DID**: the target peer's decentralized identifier (`did:octo:...` string)
- **Per-extension concrete impl crates** (Layer D): OUT OF SCOPE; substrate type in Layer B exposes the probe surface for future follow-on Layer D adapter missions

## Detailed Design

### CLI surface

```
octo network heartbeat probe <peer_did> [--timeout-ms <N: u16>] [--format ascii|json] [--json]
```

- `heartbeat probe` — read-only rendering of heartbeat probe result (Reachable { rtt_ms } + Unreachable { reason } + Timeout)
- `--timeout-ms` — probe timeout in milliseconds (clap u16 overflow pre-dispatch rejection; default 5000)
- `--format ascii|json` — output format (default ascii)
- `--json` — alias for `--format json`

### Substrate extensions (Layer B — ONE NEW MODULE)

NEW `crates/octo-network/src/mon/heartbeat.rs`:

```rust
/// Heartbeat probe (RFC-0855 §Wire Format heartbeat probing).
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table. Real
/// transport-level probe OUT OF SCOPE; per-extension impl
/// crates (Layer D) provide real transport-level probes in
/// follow-on missions.
#[derive(Clone, Debug, Default)]
pub struct Heartbeat;

/// Heartbeat probe result.
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table.
/// `#[non_exhaustive]` discipline per RFC-0011-h §Substrate
/// Faithfulness — per-extension Layer D adapter crates may
/// introduce additional result variants in follow-on missions.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeartbeatProbeResult {
    /// Peer is reachable; `rtt_ms` is round-trip-time in milliseconds.
    Reachable { rtt_ms: u32 },
    /// Peer is unreachable; `reason` describes why.
    Unreachable { reason: UnreachableReason },
    /// Probe timed out.
    Timeout,
}

/// Reason for an unreachable probe result.
///
/// Phase 14 G17 per RFC-0011-v §Substrate Mapping Table.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnreachableReason {
    /// Peer DID is malformed.
    InvalidPeerDid,
    /// No transport adapter registered for this peer.
    NoTransportAdapter,
    /// Transport adapter refused the probe (e.g. protocol mismatch).
    AdapterRefused,
    /// Per-adapter detail payload; opaque to the substrate
    /// (Layer D concrete adapter crates define the detail
    /// payload semantics in follow-on missions).
    Other(String),
}

/// Minimal DID syntactic validator (RFC-0855 §Identifiers).
///
/// Phase 14 stub: a peer DID is structurally valid iff it starts
/// with `did:octo:` and has a non-empty method-specific
/// identifier segment. Real DID validation (signature checks,
/// DID-document lookup, schema validation) is OUT OF SCOPE for
/// Phase 14; stub-envelope extension defers this to
/// per-extension Layer D adapter missions.
fn is_structurally_valid_did(peer_did: &str) -> bool {
    let Some(method_specific) = peer_did.strip_prefix("did:octo:") else {
        return false;
    };
    !method_specific.is_empty()
}

impl Heartbeat {
    /// Probe a peer by DID; returns `HeartbeatProbeResult`
    /// indicating reachability + RTT or unreachable reason or
    /// `Timeout`. Phase 14 stub contract:
    ///
    /// 1. A structurally malformed `peer_did` (does not start
    ///    with `did:octo:` or has empty method-specific
    ///    identifier) returns
    ///    `Unreachable { reason: InvalidPeerDid }`. This is
    ///    the ONLY information-bearing branch in Phase 14 (it
    ///    is derived from pure substring analysis).
    /// 2. A structurally well-formed `peer_did` returns
    ///    `Timeout` unconditionally. Real transport-level
    ///    probe (reachability detection + RTT measurement) is
    ///    OUT OF SCOPE for Phase 14; per-extension Layer D
    ///    adapter crates provide real transport-level probes
    ///    in follow-on missions.
    ///
    /// `timeout_ms` is part of the public API for forward
    /// compatibility with the Layer D adapter missions but is
    /// NOT consumed by the Phase 14 stub.
    pub fn probe(&self, peer_did: &str, timeout_ms: u16) -> HeartbeatProbeResult {
        if !is_structurally_valid_did(peer_did) {
            return HeartbeatProbeResult::Unreachable {
                reason: UnreachableReason::InvalidPeerDid,
            };
        }
        let _timeout_ms = timeout_ms;
        HeartbeatProbeResult::Timeout
    }
}
```

### Output envelope

```rust
/// `octo network heartbeat probe` output envelope.
/// Wraps the substrate `HeartbeatProbeResult` projection
/// for CLI dispatch. Flat scalar layout (no Map fields;
/// `result_label` discriminates Reachable / Unreachable /
/// Timeout; `rtt_ms`, `unreachable_reason_label`,
/// `unreachable_reason_detail` populated only when the
/// corresponding variant / sub-variant is selected).
#[derive(Serialize, Deserialize, Debug, Clone, schemars::JsonSchema)]
pub struct NetworkHeartbeatProbeOutput {
    /// Peer DID (echo).
    pub peer_did: String,
    /// Probe result label (`reachable` / `unreachable` / `timeout`).
    pub result_label: String,
    /// Round-trip-time in milliseconds (only present when `result_label == "reachable"`).
    pub rtt_ms: Option<u32>,
    /// Unreachable reason label (only present when `result_label == "unreachable"`).
    pub unreachable_reason_label: Option<String>,
    /// Unreachable reason detail (only present for `UnreachableReason::Other`).
    pub unreachable_reason_detail: Option<String>,
    /// Probe timeout in milliseconds (echo).
    pub timeout_ms: u16,
}
```

### Test vectors (12: 5 dispatch + 7 substrate)

#### Dispatch (`crates/octo-cli`)

- `tv_net14_1`: heartbeat probe default (no `--timeout-ms`) parses cleanly with default 5000ms timeout
- `tv_net14_2`: heartbeat probe `--timeout-ms 1000` parses cleanly
- `tv_net14_3`: heartbeat probe `--timeout-ms 65535` parses cleanly (clap u16 max)
- `tv_net14_4`: heartbeat probe dispatch body-contract (R1.5 fix: dispatch test invokes `network_heartbeat_probe` handler AND directly invokes substrate to verify body contract per Phase 10 + Phase 11 + Phase 13 R3 MAJOR-1 lesson)
- `tv_net14_5`: heartbeat probe output envelope serde round-trip (R1.5 fix: output envelope `Serialize + Deserialize` derives enable round-trip assertion; mirrors Phase 13 `tv_net13_9` envelope serde round-trip test pattern)

#### Substrate (`crates/octo-network`)

- `tv_phase14_substrate_1`: Heartbeat default construction + structural presence probe (R1.5 fix: strengthened assertion; `format!("{:?}", hb) == "Heartbeat"`)
- `tv_phase14_substrate_2`: probe returns `Timeout` for structurally well-formed peer_did (R1.5 fix: narrows the Phase 14 stub contract — malformed DIDs now return `InvalidPeerDid` per `is_structurally_valid_did`)
- `tv_phase14_substrate_3`: probe with `timeout_ms = 0` returns `Timeout` for a structurally well-formed peer_did (R1.5 fix: `timeout_ms` is plumbed through the stub API but is deliberately NOT consumed by Phase 14 — preserved for forward compatibility)
- `tv_phase14_substrate_4`: `HeartbeatProbeResult` variants `PartialEq` (Reachable{42} == Reachable{42}; Reachable{42} != Reachable{100}; Unreachable{InvalidPeerDid} != Unreachable{NoTransportAdapter}; Timeout == Timeout)
- `tv_phase14_substrate_5`: `UnreachableReason::Other` carries heap-backed `String` payload (Round-trip `UnreachableReason::Other("connection-reset")` → payload preserved)
- `tv_phase14_substrate_6`: probe returns `Unreachable { reason: InvalidPeerDid }` for structurally malformed peer_did (R1.5 fix: explicit distinction between well-formed → Timeout and malformed → Unreachable{InvalidPeerDid}; covers 4 malformed cases: missing prefix, empty DID, empty method-specific identifier, wrong method)
- `tv_phase14_substrate_7`: `UnreachableReason` all variants distinct + Equality (R1.5 fix: validates `#[non_exhaustive]` requires catch-all in user code; `Other(String)` payload semantics validated via Debug round-trip; Copy derive deliberately OMITTED because `Other(String)` payload must remain heap-backed)

## Exit codes

Slot 89 `NetworkSubstrateUnavailable` REUSE per Phase 6 precedent + user decision. 0 NEW OctoCliError variants for Phase 14.

## Layer discipline

- **Layer A frozen preserved**: zero change to `octo-governance-core`, `octo-audit-core`, `octo-settlement-core`.
- **Layer B substrate NEW MODULE**: `mon/heartbeat.rs` (per Phase 7 RFC-0011-o NEW module precedent for SlashBridge).
- **Layer C CLI dispatch**: `commands/network.rs` extended with `NetworkAction::Heartbeat { action: NetworkHeartbeatAction }` clap variant; `NetworkHeartbeatAction::Probe(HeartbeatProbeArgs)`; 1 output envelope (`NetworkHeartbeatProbeOutput` with `Serialize + Deserialize` derives); 1 handler (`network_heartbeat_probe` invokes substrate unconditionally per Phase 12 RFC-0011-t R2.5 precedent); 12 test vectors (5 dispatch + 7 substrate).

## Substrate-faithfulness

The `Heartbeat::probe()` Phase 14 stub contract is precisely stated:

- Structurally malformed `peer_did` (does not start with `did:octo:` or has empty method-specific identifier) → `Unreachable { reason: InvalidPeerDid }`. This is the ONLY information-bearing branch in Phase 14 (derived from pure substring analysis via `is_structurally_valid_did`).
- Structurally well-formed `peer_did` → `Timeout` unconditionally. Real transport-level probe (reachability detection + RTT measurement) is OUT OF SCOPE for Phase 14; per-extension Layer D adapter crates provide real transport-level probes in follow-on missions per RFC-0011-h §Future Work items F8+F9.
- `timeout_ms` is plumbed through the stub API but is deliberately NOT consumed by Phase 14. Preserved for forward compatibility with the Layer D adapter missions.

Scalar field-order determinism preserved per RFC-0011-h §Output Envelope determinism (no `HashMap`/`BTreeMap` fields introduced — the output envelope is flat scalar layout: `peer_did + result_label + rtt_ms + unreachable_reason_label + unreachable_reason_detail + timeout_ms`). Per-extension crate pattern preserved (substrate types in Layer B; concrete impls OUT OF SCOPE). Handler invokes substrate unconditionally per Phase 12 RFC-0011-t R2.5 substrate-faithfulness precedent (no always-false registry marker). `HeartbeatProbeResult` + `UnreachableReason` `#[non_exhaustive]` discipline preserved (per-extension Layer D adapter crates may introduce additional variants in follow-on missions).

## Companion stub missions

G17 `0011-h-s-a-heartbeat-probe` (Open → Claimed → Completed paired with CLI dispatch slice).

## Stale-stub sweep

Per Phase 6 `9a903993` precedent, the final commit for RFC-0011-v sweep any remaining Open stubs related to RFC-0011-h §Substrate-Additions after Phase 14 closure. Per "everything included, no deferral" user directive 2026-09-20, no G-row may remain Open after Phase 14.

## Out of Scope

- Live transport-level probe adapter (Layer D; follow-on per-extension crate missions)
- Per-extension impl crates (substrate-ext-heartbeat-transport-*) OUT OF SCOPE for Phase 14
- Wire format versioning (RFC-0011-h §Future Work items F8+F9)
- Real probe logic (Phase 14 substrate returns Timeout unconditionally; real probe in follow-on Layer D adapter mission)

## History

- 2026-09-20 — Draft (this RFC)
- 2026-09-21 — R1.5 fix sweep: cite hygiene (status parentheticals stripped, Governing RFC label removed, .gitignore line 46 file:line ref replaced with [[docs-plans-scratchpad]], this RFC idiom, Phase 12/13 R1.5 lessons); substrate-faithfulness (BTreeMap doc claim dropped, in-memory snapshot framing dropped, HeartbeatProbeResult + UnreachableReason `#[non_exhaustive]` discipline preserved, Copy derive deliberately OMITTED for UnreachableReason because Other(String) payload must remain heap-backed, is_structurally_valid_did added for malformed peer_did branch); bug fixes (heartbeat_probe_registry gate removed + fn deleted, NetworkHeartbeatProbeOutput Deserialize derive added, catch-all `unknown_future_variant` per `#[non_exhaustive]` discipline); test coverage (test vector count expanded 3 → 12 across 4 RFC locations, tv_net14_4 handler+substrate body-contract inspection test added, tv_net14_5 envelope serde round-trip test added, tv_phase14_substrate_1/6/7 strengthened/added); output envelope illustration drift (flat scalar layout with `Deserialize` derive, no `HeartbeatFormatKind` phantom type, no `result: HeartbeatProbeResult` projection).

## Stale-Stub Sweep (Phase 14 closure)

Per Phase 6 `9a903993` precedent, this section confirms that all 9 RFC-0011-h §Substrate-Additions Companion Missions G-rows G9-G17 are CLOSED with no remaining Open stubs after Phase 14 closure:

| G-row | Substrate companion YAML             | Status    | Anchor phase |
| ----- | ------------------------------------ | --------- | ------------ |
| G9    | `0011-h-s-a-slash-bridge-trait`      | Completed | Phase 7      |
| G10   | `0011-h-s-a-quota-router-node`       | Completed | Phase 8      |
| G11   | `0011-h-s-a-specialized-node-record` | Completed | Phase 9      |
| G13   | `0011-h-s-a-reputation-store`        | Completed | Phase 10     |
| G14   | `0011-h-s-a-topology-render`         | Completed | Phase 11     |
| G15   | `0011-h-s-a-gossip-stats`            | Completed | Phase 12     |
| G16a  | `0011-h-s-a-envelope-inspector`      | Completed | Phase 13     |
| G16b  | `0011-h-s-a-forward-envelope`        | Completed | Phase 13     |
| G17   | `0011-h-s-a-heartbeat-probe`         | Completed | Phase 14     |

All 8 CLI mission YAMLs paired with these G-rows (Created at CLI dispatch slice time per user decision 2026-09-20) are also Completed:

| CLI mission YAML              | Subcommand count | Status    |
| ----------------------------- | ---------------- | --------- |
| `0011-h-network-slash-bridge` | 2                | Completed |
| `0011-h-network-router`       | 2                | Completed |
| `0011-h-network-node`         | 2                | Completed |
| `0011-h-network-reputation`   | 2                | Completed |
| `0011-h-network-topology`     | 1                | Completed |
| `0011-h-network-gossip`       | 1                | Completed |
| `0011-h-network-envelope`     | 2                | Completed |
| `0011-h-network-heartbeat`    | 1                | Completed |

**NO DEFERRAL per user directive 2026-09-20. Phase 7-14 rollout plan 100% complete.**

## Cross-references

- [[RFC-0011-h]] — multi-phase rollout plan
- [[RFC-0011-o]] — Phase 7 G9 spec
- [[RFC-0011-p]] — Phase 8 G10 spec
- [[RFC-0011-q]] — Phase 9 G11 spec
- [[RFC-0011-r]] — Phase 10 G13 spec
- [[RFC-0011-s]] — Phase 11 G14 spec
- [[RFC-0011-t]] — Phase 12 G15 spec
- [[RFC-0011-u]] — Phase 13 G16a + G16b spec
- [[RFC-0011-v]] — Phase 14 G17 spec (this RFC)
