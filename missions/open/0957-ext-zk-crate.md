# Mission: 0957-ext-zk — Extract ZK Capability to Per-Extension Crate (RFC-0957 v2.0)

## Status

Open (2026-08-08). RFC-0957 v2.0 + RFC-0965 v1.4 amendments mandate per-extension crate layout.

## RFC

RFC-0957 (Economics): Capability Token Format — Accepted v2.0 (2026-08-08 amendment)
RFC-0958 (Proof Systems): ZK Capability Subclass — Accepted
RFC-0965 (Economics): Capability Extension Format — Accepted v1.4 (2026-08-08 amendment)

**BLUEPRINT gate note:** All three RFCs are Accepted. Mission 0957-ext-zk implements the ZK capability extraction mandate.

This mission extracts `crates/octo-wallet/src/capability/zk_mint.rs` (781 lines) into a dedicated `crates/octo-cap-zk/` crate. The ZK capability type (per RFC-0958) becomes a plugin alongside the macaroon v1 capability (per RFC-0957).

## Summary

Extract the ZK-verified capability type into a dedicated `crates/octo-cap-zk/` crate. The extraction preserves the macaroon substrate in `octo-cap-macaroon` and adds ZK as a sibling capability extension. Wallet core becomes thin substrate; ZK substrate becomes a plugin.

## Acceptance Criteria

### Top-level: ZK extraction

- [ ] NEW: `crates/octo-cap-zk/` crate created with `Cargo.toml` + `src/lib.rs`
- [ ] Migrated from `crates/octo-wallet/src/capability/zk_mint.rs` (781 lines): `mint_with_zk`, `verify_zk_capability`, `ProofBundle`, `ZkMintError`, `ExecutionTrace`, `TraceStep`
- [ ] Migrated from `crates/octo-wallet/src/capability/zk_mint.rs` test module: all unit tests pass with byte-identical signatures
- [ ] Dependency: `octo-cap-zk` depends on `octo-cap-macaroon` (for CapabilityToken substrate) + `zk-verifier` + `zk-circuit` (per RFC-0958)
- [ ] `CapabilitySpec` impl registered via plugin at startup
- [ ] `octo-wallet` `Cargo.toml` adds `octo-cap-zk = { path = "../octo-cap-zk" }` dep
- [ ] Workspace `Cargo.toml` adds `crates/octo-cap-zk` to `members`
- [ ] All existing ZK tests pass: `cargo test -p octo-cap-zk --lib`
- [ ] Integration tests: `cargo test -p octo-wallet --tests capability/zk*` (eleven_step_zk, zk_vectors, capability_zk_acceptance, wire_v2_roundtrip)
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` clean
- [ ] `cargo fmt --check` clean

### Cross-crate compat

- [ ] `cargo build --workspace --features full` green
- [ ] `cargo test --workspace --lib` green
- [ ] `cargo clippy --workspace --all-targets --features full -- -D warnings` green

### RFC-0958 compatibility

- [ ] `HolderKind::ZKBearing` (RFC-0957-A1) variant preserved; macaroon `Kind` discriminates the ZK capability variant
- [ ] `ProofBundle.witness_format` field (per RFC-0958-A1 S05) preserved across crate boundary
- [ ] `zk_vendor::prover_input::WitnessFormat` enum shared across proofer + verifier + wallet (per RFC-0958-A1 S05 mission `0958-c`)

## Dependencies

**Requires:**

- RFC-0957 (Accepted v2.0) — per-extension crate layout mandate
- RFC-0958 (Accepted) — ZK capability subclass specification
- RFC-0965 (Accepted v1.4) — per-extension crate layout mandate
- `crates/octo-wallet/src/capability/zk_mint.rs` — source code to extract

**Mission gates:**

- RFC-0957 v2.0 + RFC-0965 v1.4 amendments (committed 2026-08-08)
- Mission `0957-ext-macaroon-crate.md` (macaroon substrate extraction precedes ZK extraction)
- Workspace `Cargo.toml` member registration

**Not Requires:**

- RFC-0871 acceptance (per-extension crate layout is independent of NodeEnvelope work)

## Implementation Guide

- NEW crate: `crates/octo-cap-zk/` with standard Rust crate layout
- Migrate source files:
  - `crates/octo-wallet/src/capability/zk_mint.rs` → `crates/octo-cap-zk/src/zk_mint.rs`
  - `crates/octo-wallet/src/capability/zk_mint.rs` test module → `crates/octo-cap-zk/tests/`
- Update `use` statements in all consumers
- Add `pub use octo_cap_zk::{ProofBundle, ExecutionTrace, mint_with_zk, verify_zk_capability}` re-exports in `octo-wallet::capability`
- CapabilitySpec impl: `crates/octo-cap-zk/src/spec.rs`

## Decomposition Rationale

Per-extension crate extraction is multi-file (1 NEW crate + 1 source migration + tests + re-exports). Below the BLUEPRINT §Multi-Mission Decomposition threshold (>10 types, >4 phases, different prerequisite chains). Single mission.

## Claimant

@unassigned (per `[[feedback_initiation_user_only]]` — user initiates the claim)

## Pull Request

(unset)

## Notes

- This mission is the SECOND per-extension crate extraction (after macaroon). Future extractions (`0957-ext-federation`, `0957-ext-time-lock`, `0957-ext-threshold-mpc`) follow the same pattern.
- Mission depends on `0957-ext-macaroon-crate.md` (macaroon substrate must exist as separate crate before ZK can depend on it).
- Mission `0958-c-real-cairo-crypto-followup.md` (5/30 ACs) tracks the broader RFC-0958 implementation work; this mission is the per-extension crate extraction subset.
- Per `[[stoolap-general-purpose-db]]` red line: this is workspace-side extraction (NOT a fork PR); ZK crate lives in cipherocto workspace.
- Per `[[cargo-fmt-workflow]]` + `[[feedback_clippy_zero_warnings]]`: `cargo fmt` + `cargo clippy -D warnings` green before commit.

**Version History:**

| Version | Date | Change |
| --- | --- | --- |
| v0.1 | 2026-08-08 | Mission filed. RFC-0957 v2.0 + RFC-0965 v1.4 amendments mandate per-extension crate layout. Mission captures ZK extraction scope. Cross-references RFC-0958 ZK capability subclass. |

Last Updated: 2026-08-08
Version: 0.1