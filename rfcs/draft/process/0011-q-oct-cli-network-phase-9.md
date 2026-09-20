# RFC-0011-q: `octo network` Phase 9 — Specialized Node (Show + Bind)

## Status

Draft (2026-09-20) — RFC-0011-q lands RFC-0011-h §Implementation Phases Phase 9. Two subcommands wire specialized node show + bind to the CLI. Substrate absent: `SpecializedNodeRecord` struct + `NodeClass` enum + `load()` + `bind_to_did()` methods MISSING from `crates/octo-network/src/specialized/node_record.rs`; this amendment adds 1 companion substrate mission (G11 `0011-h-s-a-specialized-node-record` per RFC-0011-h row) + 0 NEW OctoCliError variants (REUSES slot 89 `NetworkSubstrateUnavailable` per RFC-0011-h §Error Handling row 89) + 2 output envelopes + 6 test vectors.

> **Amendment chain:** Ninth amendment in the `0011-h-multiphase-rollout-plan` (see `docs/plans/2026-09-20-0011-h-multiphase-rollout-plan.md`, gitignored scratchpad per `.gitignore` line 46). Phase 1 = RFC-0011-i (DRY CLOSED). Phase 2 = RFC-0011-j (DRY CLOSED). Phase 3 = RFC-0011-k (DRY CLOSED). Phase 4 = RFC-0011-l (DRY CLOSED). Phase 5 = RFC-0011-m (DRY CLOSED). Phase 6 = RFC-0011-n (DRY CLOSED). Phase 7 = RFC-0011-o (DRY CLOSED + Accepted). Phase 8 = RFC-0011-p (IMPLEMENTATION CLOSED). Phase 9 = RFC-0011-q (this RFC).

## Authors

- Author: @mmacedoeu

## Maintainers

- Maintainer: @mmacedoeu

## Summary

RFC-0011-q lands the **specialized node show + bind** slice of RFC-0011-h §Implementation Phases. Two CLI subcommands wire to substrate (companion mission for `SpecializedNodeRecord` struct + `NodeClass` enum + accessor + mutator methods):

- `octo network node show <node_id_hex>` — read-only projection of a specialized node record by node_id
- `octo network node bind <node_id_hex> --holder-did <did>` — mutating bind of a node to a holder DID (requires `--confirm-acknowledge`)

Substrate per RFC-0871 (Specialized Node Protocol Envelope). Per-extension transport impl crates (substrate-ext-specialized-node-*) OUT OF SCOPE.

## Dependencies

- RFC-0011-h Accepted
- RFC-0871 Specialized Node Protocol Envelope
- RFC-0011-i Phase 1 IMPLEMENTATION CLOSED
- RFC-0011-j Phase 2 IMPLEMENTATION CLOSED
- RFC-0011-k Phase 3 IMPLEMENTATION CLOSED
- RFC-0011-l Phase 4 IMPLEMENTATION CLOSED
- RFC-0011-m Phase 5 IMPLEMENTATION CLOSED
- RFC-0011-n Phase 6 IMPLEMENTATION CLOSED
- RFC-0011-o Phase 7 IMPLEMENTATION CLOSED + Accepted
- RFC-0011-p Phase 8 IMPLEMENTATION CLOSED

## Design Goals

1. Wire `SpecializedNodeRecord` substrate surface (RFC-0871) to the CLI for operator observability + controlled mutating bind
2. Preserve per-extension crate pattern: trait in Layer B (`octo-network::specialized::node_record::SpecializedNodeRecordAccess`); concrete per-transport impl crates in Layer D, OUT OF SCOPE
3. Preserve Layer A frozen contract (zero Layer A change per RFC-0011-h §Layer Discipline)
4. Preserve pastejacking defense via `parse_32_byte_hex` shared helper for `node_id_hex` arg (mixed-case rejected)
5. Preserve BTreeMap determinism where substrate carries metadata
6. Preserve slot 89 REUSE per Phase 6 precedent + user decision (0 NEW OctoCliError variants)
7. Preserve confirmation-flag pattern per RFC-0011-h §Confirmation Flag for the mutating `bind` subcommand (--dry-run + --confirm-acknowledge)
8. 6 test vectors (3 per subcommand) — tv_net9_1 through tv_net9_6

## Motivation

RFC-0011-h §Implementation Phases Phase 9 calls for wiring specialized node observability to the CLI. Operators need to inspect a node's class + holder DID + creation epoch, and to bind a node to a holder DID under explicit confirmation. The substrate (`SpecializedNodeRecord`) is MISSING from `crates/octo-network/src/specialized/node_record.rs` and must be added in this RFC's companion mission (G11 `0011-h-s-a-specialized-node-record`).

## Roles and Authorities

- **Operator**: invokes `octo network node show` + `octo network node bind` for diagnostics and controlled bind operations
- **Holder DID**: the DID that owns/controls the node post-bind (subject of the mutating bind operation)
- **Per-extension concrete impl crates** (Layer D): OUT OF SCOPE; trait in Layer B exposes the substrate surface for future follow-on Layer D adapter missions

## Specification

### Substrate additions

NEW module `crates/octo-network/src/specialized/node_record.rs` (NEW subdir `specialized/`):

```rust
use octo_did::Did;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecializedNodeRecord {
    pub node_id: [u8; 32],
    pub holder_did: Option<Did>,
    pub node_class: NodeClass,
    pub creation_epoch: u64,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeClass {
    Builder,
    Provider,
    Storage,
    Bandwidth,
    Orchestrator,
}

pub trait SpecializedNodeRecordAccess: Send + Sync {
    fn load(&self, node_id: &[u8; 32]) -> Option<SpecializedNodeRecord>;
    fn bind_to_did(&mut self, node_id: &[u8; 32], holder_did: &Did) -> Result<(), SpecializedNodeError>;
    fn node_id(&self) -> [u8; 32];
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SpecializedNodeError {
    NotFound,
    AlreadyBound,
    InvalidDid(String),
    Internal(String),
}
```

Layer B only. `BTreeMap` for `metadata` determinism. `#[non_exhaustive]` on error enum for forward-compatible variant growth.

### Subcommand Taxonomy (RFC-0011-h §Subcommand Taxonomy Phase 9)

| Subcommand                                                | Type     | Layer | Companion |
| --------------------------------------------------------- | -------- | ----- | --------- |
| `octo network node show <node_id_hex>`                    | read     | C     | G11       |
| `octo network node bind <node_id_hex> --holder-did <did>` | mutating | C     | G11       |

### Output Envelope (RFC-0011-h §Output Envelope Phase 9)

- `octo.network.node.show.v1` — wrapper envelope `NetworkNodeShowOutput` + projection `SpecializedNodeRecordProjection`
- `octo.network.node.bind.v1` — wrapper envelope `NetworkNodeBindOutput` + projection `BindReceiptProjection`

### Error Handling (RFC-0011-h §Error Handling Phase 9)

- Pre-companion G11 path: surfaces exit 89 `NetworkSubstrateUnavailable` (companion = "G11", detail = "")
- `bind_to_did` errors: per-variant mapping to exit 89 with distinct detail strings (NotFound, AlreadyBound, InvalidDid, Internal)

### Confirmation Flag (RFC-0011-h §Confirmation Flag Phase 9)

`bind` is mutating but reversible (re-bind allowed via `AlreadyBound` error path). Per RFC-0011-h §Confirmation Flag:

- `--dry-run` (default false; mutually exclusive with --apply via clap conflicts_with)
- `--apply` requires `--confirm-acknowledge` (parse-time rejection via clap requires)

### Pastejacking Defense (RFC-0011-h §Pastejacking Defense Phase 9)

`node_id_hex` arg uses `parse_32_byte_hex` shared helper via new `parse_specialized_node_id_hex` 1-arg wrapper. Accepts lowercase OR uppercase; rejects mixed-case.

### Performance Targets (RFC-0011-h §Performance Targets Phase 9)

- `node show`: 50 ms (in-memory lookup)
- `node bind`: 500 ms (in-memory mutation + epoch bump)

### Implicit Assumptions Audit (RFC-0011-h §Implicit Assumptions Audit Phase 9)

| Assumption                                           | Where Relied Upon                         | Blast Radius if False                                     | Mitigation                                               |
| ---------------------------------------------------- | ----------------------------------------- | --------------------------------------------------------- | -------------------------------------------------------- |
| `SpecializedNodeRecord` is per-node canonical record | substrate struct                          | bind fails or duplicates                                  | `node_id` collision check in `bind_to_did`               |
| `NodeClass` enum is closed set                       | RFC-0871 + this RFC                       | unknown class surfaces as `#[non_exhaustive]` fallthrough | `#[serde(rename_all = "lowercase")]` + catch-all variant |
| `Did` is parseable from string                       | clap value_parser for `--holder-did` flag | bind rejects malformed DID                                | pre-bind DID validation in `bind_to_did`                 |
| BTreeMap determinism                                 | metadata field                            | non-deterministic JSON output                             | BTreeMap over HashMap per RFC-0011-h §Output Envelope    |

### Security Considerations (RFC-0011-h §Security Considerations Phase 9)

- Bind requires `--confirm-acknowledge` (parse-time rejection prevents accidental bind)
- Bind is reversible via `AlreadyBound` error path (operator can rebind)
- Holder DID redacted on error paths per RFC-0011-h §Security Considerations redaction invariant

### Adversarial Review (RFC-0011-h §Adversarial Review Phase 9)

- Threat 1: Operator binds node to wrong DID — mitigated by `--confirm-acknowledge` parse-time requirement
- Threat 2: Pastejacking via mixed-case hex — mitigated by `parse_32_byte_hex` shared helper
- Threat 3: Race condition on concurrent bind — single-threaded trait impl; per-extension impl crates handle concurrency
- Threat 4: DID string injection via `--holder-did` — mitigated by `Did` parser rejecting malformed input
- Threat 5: Metadata injection via internal callers — BTreeMap<String, String> bounds check + size limit in follow-on Layer D impl

### Compatibility (RFC-0011-h §Compatibility Phase 9)

- Backward compatible: NEW subcommand + NEW envelope; no change to existing dispatch surface
- Forward compatible: `#[non_exhaustive]` on `SpecializedNodeError` allows per-extension impl crates to add new variants

### Test Vectors (RFC-0011-h §Test Vectors Phase 9)

| Vector    | Surface         | Coverage                                                                   |
| --------- | --------------- | -------------------------------------------------------------------------- |
| tv_net9_1 | CLI parse       | `node show` parses cleanly with no args                                    |
| tv_net9_2 | CLI parse       | `node show --json` flag parses cleanly                                     |
| tv_net9_3 | substrate trait | `node show` envelope projection is substrate-faithful (None = not found)   |
| tv_net9_4 | CLI parse       | `node bind --apply --confirm-acknowledge` parses cleanly                   |
| tv_net9_5 | CLI parse       | `node bind --apply` without `--confirm-acknowledge` rejected at parse-time |
| tv_net9_6 | pastejacking    | `node bind` accepts uppercase-only hex; rejects mixed-case                 |

Coverage split per Phase 5 RFC-0011-m precedent: CLI tests cover clap parsing + handler dispatch to the trait boundary; substrate tests cover trait behavior.

### Alternatives Considered

- **Single struct + JSON payload**: rejected — substrate-faithful typed projection preserves Layer B stability
- **Mutating bind without confirmation**: rejected — RFC-0011-h §Confirmation Flag mandates confirmation
- **Direct substrate call without trait**: rejected — per-extension crate pattern requires trait for registry dispatch

### Substrate-Additions Companion Missions (RFC-0011-h §Substrate-Additions Companion Missions row G11)

- `0011-h-s-a-specialized-node-record` — substrate-additions prerequisite (Open → Claimed → Completed paired with CLI dispatch)

### Implementation Phases

1. Stub fill-in: substrate companion YAML `0011-h-s-a-specialized-node-record` (Open → Claimed) with full type signatures + AC
2. Substrate slice: NEW `crates/octo-network/src/specialized/node_record.rs` + `pub mod specialized;` + `pub mod node_record;` insertions
3. Paired-YAML Claimed transition for substrate companion
4. CLI dispatch slice atop substrate: `NetworkAction::Node` + nested `NetworkNodeAction` + `NodeShowArgs` + `NodeBindArgs` + 2 envelopes + 2 handlers + `specialized_node_registry` helper + 6 test vectors
5. CLI mission YAML CREATED Completed + paired substrate companion YAML Completed paired
6. Closure artifacts: audit doc + memory card + MEMORY.md index entry

> Phase 9 follows the Phase 5 RFC-0011-m 5-commit pattern (stub fill-in → substrate slice → YAML Claimed → CLI dispatch → YAMLs Completed) verified at `next 8e7c5cec`, `24bfec96`, `fcb58331`, `346f10cc`, `97955c00`.

### Key Files to Modify

- `rfcs/draft/process/0011-q-oct-cli-network-phase-9.md` — NEW RFC Draft (this file)
- `missions/open/0011-h-s-a-specialized-node-record.md` — substrate companion YAML stub fill-in
- `crates/octo-network/src/specialized/node_record.rs` — NEW substrate module
- `crates/octo-network/src/specialized/mod.rs` — NEW mod.rs
- `crates/octo-network/src/lib.rs` — `pub mod specialized;` insertion
- `crates/octo-cli/src/commands/network.rs` — CLI dispatch slice (Args + envelopes + handlers + test vectors)
- `missions/open/0011-h-network-node.md` — NEW CLI mission YAML (Created Completed paired)

### Future Work

- Per-extension concrete impls (BLE/USB/TCP/QUIC/HID specialized node adapters) — follow-on Layer D missions, OUT OF SCOPE for this trait-only phase
- Persistence adapter (in-memory mutation only; persistence in follow-on Layer D adapter)
- DID validation library (uses `octo-did` crate; out of scope per RFC-0011-h)

## Rationale

RFC-0011-q closes the G11 deferred stub per "everything included, no deferral" directive. The substrate (`SpecializedNodeRecord`) lands FIRST per [[no-phantom-mission-pointers]] pairing invariant, then CLI dispatch atop it. The trait-based dispatch surface preserves the per-extension crate pattern (trait in Layer B; concrete impls in Layer D), enabling follow-on Layer D adapter missions without changing core CLI dispatch.

## Version History

| Version | Date       | Notes                                                |
| ------- | ---------- | ---------------------------------------------------- |
| v0.1.0  | 2026-09-20 | Initial draft; pending R1 of 5-len DRY CLOSURE cycle |

## Cross-references

- **RFC-0011-h** §Substrate-Additions Companion Missions row G11 — canonical substrate spec for `SpecializedNodeRecord`
- **RFC-0011-h** §Error Handling row 89 — slot 89 `NetworkSubstrateUnavailable` REUSE
- **RFC-0011-h** §Exit Codes slot 89 — REUSE per Phase 9
- **RFC-0011-h** §Confirmation Flag + Per-Axis Exit Code Matrix — `--apply` + `--confirm-acknowledge` precedent
- **RFC-0011-h** §Performance Targets Phase 9 rows — 50 ms show + 500 ms bind
- **RFC-0011-h** §Subcommand Taxonomy Phase 9 rows — `node show` + `node bind`
- **RFC-0011-h** §Output Envelope Phase 9 rows — `NetworkNodeShowOutput` + `NetworkNodeBindOutput`
- **RFC-0011-h** §Implicit Assumptions Audit Phase 9 rows
- **RFC-0011-h** §Security Considerations Phase 9 rows — pastejacking + bind redaction
- **RFC-0011-h** §Adversarial Review Phase 9 rows — 5 threats
- **RFC-0871** Specialized Node Protocol Envelope — governing RFC for substrate spec
- **RFC-0011-o** Phase 7 — prior phase (slash-bridge)
- **RFC-0011-p** Phase 8 — prior phase (router)
