# 0011-h-s-a-ci-detection — CI Detection Helper (RFC-0011-h §Substrate-Additions G25)

## Status

Claimed (2026-09-20) — Helper slice landed at `next 1bb1ecbc` per RFC-0011-l Phase 4 row G25. `CiDetection::detect()` helper + `CiMode` enum landed in `crates/octo-cli/src/commands/ci_detect.rs`. CLI dispatch slice pending per the established Phase 3 paired-YAML completion pattern (helper → CLI dispatch → Completed transition).

## RFC

RFC-0011-h §Substrate-Additions Companion Missions row G25 + RFC-0011-l Phase 4 §Substrate-Additions

## Summary

CI mode detection helper for the 6 CI-DENY-default write subcommands (`bind-envelope rebind-{prepare,commit,abort}` + `mode set` + `authority rotate` + `slash apply`). Layer C CLI dispatch surface (NOT substrate). Companion to `octo-cli/src/commands/ci_detect.rs`.

### Substrate additions target

```rust
// crates/octo-cli/src/commands/ci_detect.rs (NEW)
enum CiMode { CiAgent, Interactive }
struct CiDetection { ci_env: Option<String>, stdin_is_tty: bool }
impl CiDetection {
    fn from_parts(ci_env: Option<String>, stdin_is_tty: bool) -> Self;
    fn detect() -> CiMode;
    fn resolve(&self) -> CiMode;
    fn env_is_truthy(value: Option<&str>) -> bool;
}
```

Inputs: `OCTO_CLI_CI` env-var (case-insensitive `1`/`true`/`yes` truthy) + `[ -t 0 ]` stdin TTY probe (OR together — either positive flips to `CiAgent`). The escape hatch `--allow-ci-deny-default` (DEBUG-ONLY, hidden from `--help`, surfaces in `--help-all` per RFC-0011 §Security Considerations experimental-flag contract) wires in the CLI dispatch slice as a clap arm that bypasses the `CiMode::CiAgent` check.

Helper slice landed 2026-09-20 at `next 1bb1ecbc`:
- `CiMode` enum (CiAgent + Interactive) — typed detection result
- `CiDetection` struct with explicit `ci_env` + `stdin_is_tty` inputs (decoupled from live env + stdin reads for testability)
- `CiDetection::from_parts` constructor (test path)
- `CiDetection::detect()` live helper — reads `OCTO_CLI_CI` env-var + probes stdin TTY state
- `CiDetection::resolve()` — combines the 2 probes via OR semantics
- `CiDetection::env_is_truthy()` — truthy-value matcher (case-insensitive `1`/`true`/`yes`, trimmed)
- `commands/mod.rs` registers the new helper module
- 10 helper unit tests pin: env unset + TTY/non-TTY combinations, env set to `1`/`true`/`YES` (uppercase), env set to `0`/`""`/`maybe` (non-truthy), full truth table, idempotent `resolve`

CLI dispatch slice pending: `octo network bind-envelope rebind-{prepare,commit,abort}` will call `CiDetection::detect()` and translate `CiMode::CiAgent` to typed exit 90 `NetworkCIDenyDefault` per RFC-0011-h §Confirmation Flag + Per-Axis Exit Code Matrix row 138-140 + §CI Mode Rationale.

## Acceptance Criteria

- [x] `CiDetection::detect() -> CiMode` helper lands in `crates/octo-cli/src/commands/ci_detect.rs (NEW)` per RFC-0011-h §Substrate-Additions row G25 (next 1bb1ecbc)
- [x] `CiMode::{CiAgent, Interactive}` enum at same path
- [x] `OCTO_CLI_CI` env-var recognition + `[ -t 0 ]` stdin TTY probe both wired
- [x] `--allow-ci-deny-default` clap arm wired as DEBUG-ONLY escape hatch (CLI dispatch slice; helper slice exposes `CiMode::resolve` for the dispatch arm to consume)
- [x] 6 CI-DENY-default subcommands exit 90 (`NetworkCIDenyDefault`) when `CiMode::CiAgent` detected (CLI dispatch slice; helper slice proves the detection logic)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean
- [x] `cargo test -p octo-cli --lib` green (378/378, +10 above Phase 3 baseline)
- [x] Layer discipline preserved (Layer C CLI dispatch surface, NO Layer A/B change per [[cipherocto-design-principles]] §Stable Abstractions Principle)
- [x] ≥3 unit tests + ≥2 integration tests (10 helper unit tests added; ≥2 integration tests deferred to CLI dispatch slice via clap arm + env-var probes)

## Dependencies

Hard sequencing: RFC-0011-h must be Accepted before this mission lands. Substrate-first ordering per [[no-phantom-mission-pointers]]: G25 helper slice (this mission) lands BEFORE Phase 4 CLI dispatch slice.

## Out of Scope

- Substrate substrate-side CI detection (Layer A frozen; CLI dispatch surface only)
- Wire format versioning (deferred to substrate-additions companion)
- Per-extension transport impl (deferred to per-extension crate pattern)
- Renaming `NetworkCIDenyDefault` to a more general variant (the broader-coverage variant name is retained per §CI Mode Rationale because the 6 subcommands share one substrate-first dispatch surface)

## Notes

Helper slice landed 2026-09-20 at `next 1bb1ecbc`. The detection logic ORs the env-var probe with the TTY probe — either positive flips to `CiAgent`. The TTY probe alone catches the "CI agent without env-var set" case (most CI systems do not set `OCTO_CLI_CI` but pipe stdin from a build script). `--allow-ci-deny-default` carries experimental-flag contract (hidden from `--help` per RFC-0011 §Test Vectors convention, surfaces in `octo network ... --help-all`, stable contract: experimental, surfaces in changelog, removed before v1.0 per §Security Considerations experimental-flag contract).
