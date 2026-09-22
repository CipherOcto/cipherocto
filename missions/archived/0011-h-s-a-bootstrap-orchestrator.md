# 0011-h-s-a-bootstrap-orchestrator — Substrate additions for BootstrapConfig parser + writer

## Status

Completed (2026-09-20) — Substrate additions LANDED at `next c9121aa5`. Substrate-faithful `BootstrapConfig::from_toml` + `::save_toml` methods land in `crates/octo-network/src/mon/bootstrap.rs`. Substrate-additions prerequisite per RFC-0011-h §Substrate-Additions Companion Missions row G1.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G1

## Summary

Adds `BootstrapConfig::from_toml(path)` reader + `BootstrapConfig::save_toml(path)` writer per RFC-0851p-a §1 BootstrapNode Registry. Pre-requisite for `octo network mode show/set`.

### Substrate additions target

```rust
// crates/octo-network/src/mon/bootstrap.rs
impl BootstrapConfig {
    pub fn from_toml(path: &Path) -> Result<Self, BootstrapConfigError>;
    pub fn save_toml(&self, path: &Path) -> Result<(), BootstrapConfigError>;
}
```

Substrate additions land 2026-09-20 at `next c9121aa5`:

- `BootstrapConfig::from_toml(path)` reader (parses `<octo_home>/network/bootstrap.toml`)
- `BootstrapConfig::save_toml(path)` writer (serializes via TOML)
- `BootstrapConfigError` enum with `Io` + `Parse` variants
- 4 unit tests: `from_toml_accepts_valid_config`, `from_toml_rejects_missing_file`, `save_toml_roundtrips`, `from_toml_rejects_malformed_toml`

## Acceptance Criteria

- [x] Substrate additions land in `crates/octo-network/src/mon/bootstrap.rs` per RFC-0011-h §Substrate-Additions row G1
- [x] `cargo clippy -p octo-network --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-network --lib` green (4/4 from_toml/save_toml tests pass)
- [x] Layer discipline preserved (Layer B only; zero Layer A change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥1 integration test (4 unit tests added)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- CLI dispatch (paired CLI mission `0011-h-network-mode` covers that surface — completed at `next 8649ca4d`)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- `BootstrapOrchestrator` lifecycle struct (Phase 6 mission `0011-h-s-a-bootstrap-orchestrator-v2` / G26 — distinct from this Phase 2 G1)

## Notes

Phase 2 G1 (parser/saver only) landed at `next c9121aa5`. Paired CLI mission `0011-h-network-mode` closed at `next 8649ca4d`. Phase 6 G26 (`BootstrapOrchestrator` lifecycle struct + `start_bootstrap` + `status`) is a separate companion mission — distinct from this parser/saver slice per RFC-0011-n R1 substrate-faithfulness finding.
