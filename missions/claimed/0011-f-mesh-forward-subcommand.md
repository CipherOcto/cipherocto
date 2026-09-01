---
name: 0011-f-mesh-forward-subcommand
description: Land `octo mesh forward` subcommand (ops escape hatch for envelope replay) per RFC-0011-f §Subcommand Taxonomy
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
status: Claimed
claimed_by: mmacedoeu
claimed_at: 2026-09-01
---

# 0011-f-mesh-forward-subcommand — `octo mesh forward` subcommand (ops envelope replay)

**Status:** Open — substrate prereqs landed (RFC-0855 + RFC-0855p-b + RFC-0855p-c + RFC-0871 + RFC-0957 Accepted). No formal release gate; implementation may proceed when cross-mission ordering permits per [[feedback_initiation_user_only]] + [[git-workflow]]. The mission depends on `0011-f-mesh-peer-subcommands` for `peer_did` resolution via the local peer table.
**Substrate:** RFC-0011-f §Subcommand Taxonomy (forward entry), RFC-0871 envelope shape, RFC-0855 + RFC-0855p-b + RFC-0855p-c peer lifecycle hooks
**Parent:** RFC-0011-f
**Depends on:**

- Mission `0011-core-output-envelope-redaction` — `OutputEnvelope<T>` + `OctoCliError` + clap root
- Mission `0011-identity-commands` — `active_signer()` for envelope signature
- Mission `0011-capability-commands` — capability caveat gating per RFC-0957
- Mission `0011-policy-commands` — `body` redaction pass extended for envelope payload bytes
- Mission `0011-f-mesh-peer-subcommands` — local peer table for `peer_did` → `peer_node_id` resolution
  **Blocks:** none (terminal mission in the mesh forward chain; no further cross-mission dependencies)

## Status

Open — substrate prereqs (RFC-0871 + RFC-0855 + RFC-0855p-b + RFC-0855p-c Accepted) verified per RFC-0011-f §Dependencies The lifecycle-hook RFCs (RFC-0855p-b coordinator lifecycle, RFC-0855p-c domain coordinator role) inform `TrustLevel::classify` semantics but are not consumed directly by this mission — the forward mission uses the `octo_mesh::forward` substrate call which delegates trust-level derivation to the peer mission.

## RFC

RFC-0011-f §Subcommand Taxonomy `octo mesh forward` entry (rfcs/draft/process/0011-f-mesh-operations.md)

## Dependencies

See YAML frontmatter `depends_on` block above. RFC-0855p-b / RFC-0855p-c are Accepted and the lifecycle hooks are substrate-visible via the peer mission's `TrustLevel::classify` integration; this mission inherits the lifecycle hook semantics transitively. No formal release gate on RFC-0855p-b / RFC-0855p-c — those are already Accepted.

## Acceptance Criteria

- [ ] `octo mesh forward` implemented + unit-tested (TV-FWD-1..6 pass)
- [ ] `octo_mesh::forward` substrate function implemented + unit-tested (`[ADD]` #4 per RFC-0011-f §Subcommand Taxonomy)
- [ ] RFC-0871 `NodeEnvelope` shape validation at CLI dispatch (envelope JSON parsed via substrate `NodeEnvelope` definition; exit 7 on shape violation)
- [ ] RFC-0010 canonical DID validation on `--target-did` (exit 4 on shape violation)
- [ ] `--ttl-hops` bounded 1..=8 per RFC-0871 ceiling (substrate clamps to per-node-type TTL from `RouterAnnouncePayload`; CLI exit 17 on out-of-range input)
- [ ] RFC-0957 capability caveat verification at dispatch (CLI exit 18 on capability missing/insufficient; audience mismatch → exit 19)
- [ ] Two-step `--confirm` + `--confirm-acknowledge` gate wired per RFC-0011-f §Security Considerations + RFC-0011 §Security Considerations 1a (pastejacking defense)
- [ ] `--dry-run` envelope header preview (correlation_id, origin_did, target_did, ttl_hops, payload_hash, capability_ref) WITHOUT payload body bytes per RFC-0011-f §Subcommand Taxonomy "Dry-run" row
- [ ] Forward receipt persistence to `$OCTO_HOME/mesh/forward-receipts.log` for audit per RFC-0011-f §Subcommand Taxonomy "Side effects" row
- [ ] `OctoCliError` variants: `InvalidTtlHops`, `MeshCapabilityInsufficient`, `EnvelopeAuthorizationFailed` implemented + unit-tested per RFC-0011-f §Error Handling
- [ ] Redaction: envelope `payload` bytes NEVER echoed in logs (BLAKE3-256 digest only); test vector `forward-payload-redacted` asserts no payload bytes in stdout/stderr/logs
- [ ] Output envelope `schema_version: 3` per RFC-0011-f §Output Envelope
- [ ] Cross-mission AC: forward command integrates with peer mission's local peer table for `peer_node_id` resolution per RFC-0011-f §Forward Envelope Shape "target_did" mapping
- [ ] Layer direction verified (no reverse deps per [[cipherocto-design-principles]])
- [ ] Cargo clippy --workspace --all-targets --features full -- -D warnings clean
- [ ] Cargo test -p octo-cli --lib --tests green
- [ ] No new INVALID cites introduced (Guard 2 cite validator green)

### Type Coverage

| RFC-0011-f type                             | Sub-step                            | Notes                                                                                                                                                                         |
| ------------------------------------------- | ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `forward`                                   | Sub-step 1 (substrate `[ADD]` #4)   | Layer C; `octo_mesh::forward(envelope: &NodeEnvelope, target_did: &Did, ttl_hops: u8) -> Result<ForwardReceipt, MeshError>`; validates shape + auth + clamps TTL + dispatches |
| `ForwardOutput`                             | Sub-step 2 (output types)           | Layer C/D; `correlation_id: Hex32` + `target_did: Did` + `ttl_hops: u8` + `expires_at_unix_ms: u64` + `dispatch_started_at_unix_ms: u64` per RFC-0011-f §Output Envelope      |
| `ForwardReceipt`                            | Sub-step 3 (audit log entry)        | Layer C; substrate-side receipt persisted to `$OCTO_HOME/mesh/forward-receipts.log`; CLI does NOT serialize this — only `ForwardOutput` is operator-visible                   |
| `OctoCliError::InvalidTtlHops`              | Sub-step 4 (error variants)         | Layer C/D; `hops: u8` field; exit 17 per RFC-0011-f §Error Handling                                                                                                           |
| `OctoCliError::MeshCapabilityInsufficient`  | Sub-step 5 (error variants)         | Layer C/D; `detail: String` field; exit 18 per RFC-0011-f §Error Handling                                                                                                     |
| `OctoCliError::EnvelopeAuthorizationFailed` | Sub-step 6 (error variants)         | Layer C/D; `detail: String` field; exit 19 per RFC-0011-f §Error Handling                                                                                                     |
| `Hex32`                                     | Sub-step 7 (correlation_id newtype) | Layer C/D; reused from capability mission; display form per RFC-0011 §Hex32 newtype convention                                                                                |

### Implementation Guide

See `docs/07-developers/octo-cli-implementation-guide.md` §Phase 7 mesh extension (forward chapter) for Rust snippets + clap wiring patterns. Per RFC-0011-f §Key Files to Modify "SUBSTRATE" section, this mission creates `crates/octo-mesh/src/forward.rs` and extends `crates/octo-mesh/src/lib.rs`. The CLI side extends `crates/octo-cli/src/commands/mesh.rs`, `crates/octo-cli/src/output.rs`, `crates/octo-cli/src/error.rs`, `crates/octo-cli/src/redact.rs`.

## Pull Request

# (PR opened by user per [[feedback_initiation_user_only]] + [[git-workflow]])

## Notes

- **Envelope file input, NOT flag construction** — `octo mesh forward` accepts `--envelope <PATH>` (JSON file) rather than constructing the envelope from CLI flags. Per RFC-0011-f §Rationale "Why envelope file input (not flag-based construction)", this defends against pastejacking attacks (clipboard hijacker swaps CLI flags to inject arbitrary envelopes).
- **Substrate-truth TTL clamping** — the per-node-type TTL ceiling from `RouterAnnouncePayload` is enforced substrate-side; the CLI bounds `--ttl-hops` to 1..=8 but the substrate may further clamp to a lower value (substrate owns the canonical ceiling per RFC-0871 §Adversary Analysis A4).
- **Capability audience caveat** — per RFC-0957 §Attenuation Invariant, every forwarded envelope carries a `Caveat::Audience(OverlayIdentity)` binding; substrate verifies audience matches the resolved `peer_did` on every dispatch. CLI exit 19 surfaces audience mismatch.
- **Dry-run safety** — dry-run output shows the parsed envelope header WITHOUT payload body bytes; the operator inspects `payload_hash` (BLAKE3-256 digest) instead of raw payload, eliminating clipboard-leak attack surface even in dry-run.

## Scope

Land 1 forward subcommand per RFC-0011-f §Subcommand Taxonomy `octo mesh forward` entry. The substrate `[ADD]` surface for this mission is function 4 of the 5 listed in RFC-0011-f §Subcommand Taxonomy; the remaining 1 (`rpc`) lands in the cross-mission follow-on `0011-f-mesh-rpc-subcommand`.

## Sub-steps

Per RFC-0011-f §Implementation Phases "Phase 7" + §Key Files to Modify:

1. **Substrate `crates/octo-mesh/src/forward.rs` (NEW)** — `forward()` wrapper around `octo-protocol::NodeEnvelope` + `NodeTransport::send_best` per RFC-0871 §Algorithms step 5 per RFC-0011-f §Key Files to Modify.
2. **Substrate `crates/octo-mesh/src/lib.rs` (EXTEND)** — export `forward` per RFC-0011-f §Subcommand Taxonomy entry 4 (`[ADD]` surface).
3. **CLI `crates/octo-cli/src/commands/mesh.rs` (EXTEND)** — add `MeshAction::Forward` impl per RFC-0011-f §Binary Surface (or seed file from peer mission + extend here).
4. **CLI `crates/octo-cli/src/output.rs` (EXTEND)** — add `ForwardOutput` per RFC-0011-f §Output Envelope (`schema_version: 3`).
5. **CLI `crates/octo-cli/src/error.rs` (EXTEND)** — add `OctoCliError::InvalidTtlHops` + `OctoCliError::MeshCapabilityInsufficient` + `OctoCliError::EnvelopeAuthorizationFailed` per RFC-0011-f §Error Handling.
6. **CLI `crates/octo-cli/src/redact.rs` (EXTEND)** — extend pattern set with envelope payload bytes (BLAKE3-256 digest shape only in logs; ensure envelope `payload` `Vec<u8>` is NEVER serialized into log fields per RFC-0011-f §Key Files to Modify).
7. **Test files `crates/octo-cli/tests/mesh_forward_*.rs` (NEW)** — 6 test vectors per RFC-0011-f §Test Vectors forward + redaction groups.
8. **Doc `docs/07-developers/octo-cli-implementation-guide.md` (EXTEND)** — Phase 7 mesh extension chapter (forward section).

### Cargo deps

Per RFC-0011-f §Key Files to Modify "SUBSTRATE" + "CLI" sections:

```toml
# crates/octo-cli/Cargo.toml additions (CLI binding layer)
octo-protocol = { path = "../octo-protocol" }  # Layer 1 stable (RFC-0871 NodeEnvelope)
octo-cap-macaroon = { path = "../octo-cap-macaroon" }  # Layer B (RFC-0957 capability verification)
octo-mesh = { path = "../octo-mesh" }                  # Layer C substrate (this mission)
octo-ident = { path = "../octo-ident" }                # Layer 1 stable (RFC-0010 canonical codec)
serde_json = "1"                                       # JSON parsing for envelope file input
```

## Test Vectors (per RFC-0011-f §Test Vectors — forward + redaction groups)

6 TV (TV-FWD-1..6) — drawn from RFC-0011-f §Test Vectors sketches TV-3 (forward-success-1-hop), TV-4 (forward-invalid-ttl-hops-out-of-range), TV-5 (forward-envelope-shape-violation), TV-6 (forward-capability-insufficient), TV-11 (forward-payload-redacted), plus the forward correlation determinism vector from the envelope-shape group.

| TV       | Group          | Sketch                                                                                                                                                                                         |
| -------- | -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TV-FWD-1 | forward        | `forward-success-1-hop` — peer_a (Verified) in table; valid RFC-0871 envelope; `--ttl-hops 1`; exit 0; `correlation_id` + `expires_at_unix_ms` set; receipt persisted                          |
| TV-FWD-2 | forward        | `forward-invalid-ttl-hops-out-of-range` — `--ttl-hops 9` → exit 17 `InvalidTtlHops { hops: 9 }`; no dispatch                                                                                   |
| TV-FWD-3 | forward        | `forward-envelope-shape-violation` — malformed envelope JSON → exit 7 `CaveatParse` (or new `MeshParse` variant); no dispatch                                                                  |
| TV-FWD-4 | forward        | `forward-capability-insufficient` — envelope has `Authorization::Signature` only (no capability); forward requires capability per G7 → exit 18 `MeshCapabilityInsufficient`; no dispatch       |
| TV-FWD-5 | redaction      | `forward-payload-redacted` — envelope payload `b"secret-marker-abc123"`; dry-run output shows `payload_hash` (BLAKE3-256) but NEVER the raw string in any stdout/stderr/log line               |
| TV-FWD-6 | envelope shape | `forward-receipt-correlation-id-deterministic` — same input envelope → same `correlation_id` (BLAKE3-256 of canonical_ser(envelope_without_id)) across runs (Class A determinism per RFC-0008) |

## Layer direction (per RFC-0011-f §Rationale "Why Layer C/D placement" + [[cipherocto-design-principles]])

- `octo-cli` (Layer C/D) — new `MeshAction::Forward` dispatch + `ForwardOutput` + 3 new `OctoCliError` variants + redaction pattern extension
- `octo-mesh` (Layer C) — extended with `[ADD]` `forward` function + `ForwardReceipt` type
- `octo-protocol` (Layer 1) — REUSED via `NodeEnvelope` + `NodeTransport::send_best`; no new Layer-1 types
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

- Additive only: `MeshAction` enum gains one new `Forward` variant per RFC-0011-f §Compatibility "Additive compatibility"; no existing variant is modified
- `OutputEnvelope<T>` struct is unchanged; `data: T` parameter gains 1 new payload type (`ForwardOutput`) per RFC-0011-f §Compatibility
- `OctoCliError` enum gains 3 new variants (`InvalidTtlHops` + `MeshCapabilityInsufficient` + `EnvelopeAuthorizationFailed`); existing variants unchanged per RFC-0011-f §Error Handling
- Exit-code table reserves codes 17-30 for mesh errors; 17, 18, 19 are used per RFC-0011-f §Exit Codes
- Redaction layer extended for envelope payload bytes (BLAKE3-256 digest shape only); the existing field-name redactor (11 names) and value-pattern redactor (8 patterns) are unchanged

## Cross-references

- RFC-0011-f §Subcommand Taxonomy `octo mesh forward` entry — primary substrate
- RFC-0011-f §Binary Surface — clap `MeshAction::Forward` enum variant + flags
- RFC-0011-f §Output Envelope — `ForwardOutput` payload + `schema_version: 3`
- RFC-0011-f §Error Handling — 3 new `OctoCliError` variants
- RFC-0011-f §Exit Codes — codes 17, 18, 19 assigned
- RFC-0011-f §Roles and Authorities — `--confirm` + `--confirm-acknowledge` two-step gate + capability gating per G7
- RFC-0011-f §Implicit Assumptions Audit — Peer DID canonical form + Envelope TTL bounded + RFC-0871 envelope version + Payload hash integrity + Capability present + Local peer table permissions
- RFC-0011-f §Security Considerations — peer downgrade + TTL exhaustion + payload injection + capability replay + envelope body leak via logs
- RFC-0011-f §Adversary Analysis A1-A3 — 5-Question Adversary Test (peer downgrade via DNS rebinding, capability replay across forwards, TTL exhaustion)
- RFC-0011-f §RFC-0008 Execution Class Mapping — `forward` is Class B (consensus-impacting routing)
- RFC-0011-f §Performance Targets — forward dry-run <100ms p95, real dispatch <100ms p95 (1-hop local) per RFC-0871 §Performance Targets
- RFC-0011-f §Compatibility — additive compat, schema_version discipline
- RFC-0011-f §Rationale "Why envelope file input (not flag-based construction)" — pastejacking defense rationale
- RFC-0011-f §Forward Envelope Shape — logical view ↔ substrate `NodeEnvelope` mapping table
- RFC-0011-f §RFC-0871 Envelope Mapping (CLI View ↔ Substrate) — `correlation_id` / `target_did` / `ttl_hops` / `payload_hash` / `capability_ref` mappings
- RFC-0011 §Subcommand Taxonomy — parent substrate
- RFC-0011 §Security Considerations 1a — pastejacking defense (`--confirm-acknowledge` pattern)
- RFC-0871 §Data Structures — `NodeEnvelope` shape (envelope_id, from_did, to_node_id, payload_kind, payload, authorization, nonce, expires_at_unix_ms)
- RFC-0871 §Algorithms "Envelope receive (node-side)" + step 5 — verification + dispatch path
- RFC-0871 §Adversary Analysis A4 — per-node-type TTL ceiling from `RouterAnnouncePayload`
- RFC-0855 — Mission Overlay Networks peer model
- RFC-0855p-b — Coordinator Lifecycle (peer lifecycle hooks; informs `TrustLevel::classify` via peer mission)
- RFC-0855p-c — Domain Coordinator Role (peer-as-coordinator case)
- RFC-0957 §Attenuation Invariant — capability `Audience` caveat binding to single `OverlayIdentity`
- RFC-0957 — Macaroon substrate (capability caveat gating for `forward` per G7)
- RFC-0010 §2 ledger_chain_registry Table Codec — `did:octo:z<base58btc>` wire form validation
- RFC-0008 — Execution Class mapping (`forward` = Class B per RFC-0011-f §RFC-0008 Execution Class Mapping)
- RFC-0011-d — Role Provisioning (NOT consumed by this mission; this mission is not role-gated per RFC-0011-f §Roles and Authorities)
- [[cipherocto-design-principles]] — Layer A/B stability contract

## Why 1 release cycle gate (forward mission)

No formal release gate. RFC-0855 + RFC-0855p-b + RFC-0855p-c lifecycle hooks are substrate-visible and Accepted; this mission inherits `TrustLevel::classify` semantics transitively via the peer mission's substrate. RFC-0871 envelope shape is Layer 1 stable and Accepted. The only sequencing constraint is the cross-mission substrate ordering (peer mission lands before forward mission, per `0011-f-mesh-peer-subcommands` §Depends on "Blocks" note). Per RFC-0011-f §Implementation Phases "Phase 7 dependencies", RFC-0855p-b lifecycle hooks are required for full `TrustLevel::classify` wiring in the peer mission; if acceptance were delayed, forward ships with `trust_level: Untrusted` as the only valid value for all peers (per RFC-0011-f §Implementation Phases "Phase 7 dependencies" row 1). Since RFC-0855p-b is Accepted, this fallback does not apply.

## Claimant

@unassigned
