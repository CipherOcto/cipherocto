# 0011-h-s-a-ci-detection — CI Detection Helper (RFC-0011-h §Substrate-Additions G25)

## Status

Open (2026-09-18) — Stub filed per [[no-phantom-mission-pointers]] pairing invariant. Full AC + scope land when work enters Phase X.

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G25

## Summary

CI mode detection helper for the 5 CI-DENY-default write subcommands (`bind-envelope rebind-{prepare,commit,abort}` + `mode set` + `authority rotate`). Layer C CLI dispatch surface (NOT substrate). Companion to `octo-cli/src/commands/ci_detect.rs`.

### Substrate additions target

```rust
// crates/octo-cli/src/commands/ci_detect.rs (NEW)
enum CiMode { CiAgent, Interactive }
fn CiDetection::detect() -> CiMode
```

Inputs: `OCTO_CLI_CI` env-var + `[ -t 0 ]` stdin TTY probe + `--allow-ci-rebind` clap arm. Until this mission lands, the 5 CI-DENY-default subcommands fall through to the substrate BLOCKED check (exit 89, `NetworkSubstrateUnavailable`); `--allow-ci-rebind` is NOT exposed in `octo network ... --help-all`.

## Acceptance Criteria

- [ ] `CiDetection::detect() -> CiMode` helper lands in `crates/octo-cli/src/commands/ci_detect.rs (NEW)` per RFC-0011-h §Substrate-Additions row G25
- [ ] `CiMode::{CiAgent, Interactive}` enum at same path
- [ ] `OCTO_CLI_CI` env-var recognition + `[ -t 0 ]` stdin TTY probe both wired
- [ ] `--allow-ci-rebind` clap arm wired as DEBUG-ONLY escape hatch
- [ ] 5 CI-DENY-default subcommands exit 90 (`NetworkCIRebindDenied`) when `CiMode::CiAgent` detected
- [ ] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [ ] `cargo test -p octo-cli --lib` green
- [ ] Layer discipline preserved (Layer C CLI dispatch surface, NO Layer A/B change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [ ] ≥3 unit tests + ≥2 integration tests (one per env-var probe + TTY probe)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands.

## Out of Scope

- Substrate substrate-side CI detection (Layer A frozen; CLI dispatch surface only)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Renaming `NetworkCIRebindDenied` to a more general variant (the broader-coverage variant name is retained per §CI Mode Rationale because the 5 subcommands share one substrate-first dispatch surface)

## Notes

Stub filed 2026-09-18 per [[no-phantom-mission-pointers]]. Full AC + scope land when work enters Phase X. `--allow-ci-rebind` carries experimental-flag contract (hidden from `--help` per RFC-0011 §Test Vectors convention; surfaces in `octo network ... --help-all`; stable contract: experimental, surfaces in changelog, removed before v1.0 per §Security Considerations experimental-flag contract).
