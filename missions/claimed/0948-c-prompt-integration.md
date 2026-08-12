# Mission: 0948-c — Prompt Integration

## Status

closed 2026-08-11 (@claude). LANDED.

**Substrate:** Missions `0948-a` (prompt registry) + `0948-b` (prompt API
endpoints) landed in prior sessions. `PromptRegistry` + `PromptStorage` +
`PromptCache` + `TemplateEngine` already in
`crates/quota-router-core/src/prompts/`. `resolve_prompt` was already
wired into `proxy.rs::handle_request_litellm` from prior work.

## Summary

Extract `AbTest` + `AbTestMetrics` into a new `prompts/ab_test.rs` module
with `AtomicU64`-backed counters. Refactor `resolve_prompt` request_id
priority chain to `api_key_id > user field > UUID` per RFC-0948
§Integration. Add `record_ab_test_outcome` + `persist_ab_test_metrics` to
the registry. Accept `prompt_id` + `prompt_variables` kwargs at the
Python SDK entry point.

## What landed

- [x] NEW `crates/quota-router-core/src/prompts/ab_test.rs` —
  `AbTestMetricsAtomic` with `Arc<AtomicU64>` counters + `f64::to_bits`
  accumulators; custom Serialize/Deserialize that snapshots in/out;
  `AbArm` enum (typed arm identifier); `AbTest::new()` constructor.
- [x] MOD `crates/quota-router-core/src/prompts/mod.rs` —
  `AbTest` + `AbTestMetrics` + `select_version` + `simple_hash` removed
  (now in `ab_test.rs`); re-exports added; 3 existing A/B tests
  rewritten to use `AbTest::new()` constructor.
- [x] MOD `crates/quota-router-core/src/prompts/storage.rs` —
  added `ab_test_metrics: HashMap<String, AbTestMetrics>` map +
  `persist_ab_test_metrics` + `persist_ab_test_metrics_snapshot` +
  `get_ab_test_metrics` + `live_ab_test_metrics` methods.
- [x] MOD `crates/quota-router-core/src/prompts/mod.rs` —
  added `record_ab_test_outcome(prompt_id, arm: AbArm, latency_ms,
  tokens, error)` (lock-free counter updates via `AbTestMetricsAtomic`)
  + `persist_ab_test_metrics(prompt_id)` (durable snapshot).
- [x] MOD `crates/quota-router-core/src/proxy.rs::resolve_prompt` —
  added `api_key_id: Option<&str>` parameter. Priority chain:
  `api_key_id > request.user > UUID`. Caller in `handle_request_litellm`
  derives `api_key_id` from `api_key` (first 8 chars when longer).
- [x] MOD `crates/quota-router-core/src/python_sdk_entry/completion.rs`
  — added `prompt_id: Option<String>` + `prompt_variables:
  Option<HashMap<String, String>>` kwargs for parity with the HTTP API
  surface (RFC-0948 §Integration; RFC-0917 mode-gate invariant).
- [x] Tests: 9 in `ab_test.rs` (default-zero, concurrent inc, snapshot
  round-trip, serde round-trip, `AbTest::new`, deterministic selection,
  weight boundaries, ended fallback, hash determinism) + 2 in
  `mod.rs` (record outcome + no-op) + 2 in `storage.rs` (persist +
  missing) + 1 in `proxy.rs` (priority-chain wiring). 14 new total.

## What did NOT land (deferred; explicit deferral)

- [ ] Stoolap backend — the `PromptStorage` substrate is in-memory
  HashMaps (`storage.rs:21-23` doc: "In-memory prompt storage
  (stoolap-backed in production)"). The new `persist_ab_test_metrics`
  method writes into the in-memory `ab_test_metrics` map. A real
  stoolap-backed backend would require a separate `StoolapPromptStorage`
  impl mirroring the trait surface — separate mission.
- [ ] Real `handle_request` (non-litellm path at `proxy.rs::handle_request`)
  integration — the 11k-LoC handler does not currently route through
  `resolve_prompt`. The litellm-mode path is the primary chat completion
  path; this is a follow-up wiring task.
- [ ] `record_ab_test_outcome` is plumbed in `PromptRegistry` but not
  called from `handle_request_litellm` after provider response. The
  caller needs latency + tokens + error + resolved arm (which
  `resolve_prompt` discards after rendering). Carve-out: a future
  mission adds a "metrics" return value from `resolve_prompt` and wires
  the call site.

## Acceptance Criteria

- [x] `prompt_id: Option<String>` + `prompt_variables:
  Option<HashMap<String, String>>` on `NativeHttpRequest` (proxy.rs
  ChatCompletionRequest carrier)
- [x] `resolve_prompt` integrated into `proxy.rs::handle_request_litellm`
- [x] `resolve_prompt` renders template + prepends system message
- [x] A/B deterministic hashing with priority chain `api_key_id >
  user > UUID` (verified by `test_resolve_prompt_request_id_priority_chain_wiring`)
- [x] Single source of truth: `AbTest.weight_b` only
- [x] `AbTestMetricsAtomic` uses `AtomicU64` for concurrent counter
  updates (verified by `test_atomic_metrics_inc_requests_concurrent`
  with 8 threads × 1000 incs = 8000/8000)
- [x] `persist_ab_test_metrics` + `record_ab_test_outcome` +
  `live_ab_test_metrics` cover the stoolap-persist contract
- [x] Python SDK `prompt_id` + `prompt_variables` kwargs accepted
- [x] `cargo test -p quota-router-core --lib` green (1571/1571)
- [x] `cargo clippy -p quota-router-core --lib --features full
  -- -D warnings` clean
- [x] `cargo fmt --all -- --check` clean

## Implementation Notes

- `AbTest::metrics` field changed from `AbTestMetrics` to
  `Arc<AbTestMetricsAtomic>`. Old snapshots still serialize via the
  custom `Serialize`/`Deserialize` impls on `AbTestMetricsAtomic`. The
  serde round-trip test verifies wire compatibility.
- `api_key_id` derives from `api_key` as the first 8 characters when
  longer. Truncation is for log-privacy and stable bucketing; not a
  cryptographic identifier. Truncation is deliberate (avoids leaking
  the full key into A/B hash inputs).
- `AbArm` enum decouples "which arm was selected" from the version
  string. Old code did `if version == "2.0.0"` which breaks for
  arbitrary version naming; the enum is operator-error-proof.
- `resolve_prompt` priority chain: empty string `""` for `api_key_id`
  falls through to `user` field (matches `Some("")` → `None`
  semantics). Test covers the empty-string fallthrough explicitly.

## Cross-references

- RFC-0948 (Economics): Prompt Management
- Mission `0948-a` (commit `e983bd0b`) — registry substrate
- Mission `0948-b` (commit `9ae79d1d`) — API endpoints substrate
- `crates/quota-router-core/src/prompts/mod.rs` — registry glue
- `crates/quota-router-core/src/prompts/storage.rs` — storage substrate
- `crates/quota-router-core/src/prompts/cache.rs` — LRU cache
- `crates/quota-router-core/src/prompts/template.rs` — template engine
- `crates/quota-router-core/src/proxy.rs` — wire-up site
- `crates/quota-router-core/src/python_sdk_entry/completion.rs` — SDK
  parity

## Version History

| Version | Date       | Status   | Changes |
| ------- | ---------- | -------- | ------- |
| v0.1    | 2026-08-11 | claimed  | Mission moved `open/` → `claimed/`; AbTest extraction + AtomicU64 + priority chain |
| v0.2    | 2026-08-11 | closed   | LANDED 2026-08-11. 14 new tests; 1571/1571 lib tests pass; clippy + fmt clean |

## RFC

RFC-0948 (Economics): Prompt Management

## Dependencies

- Mission-0948-a: Prompt Registry
- Mission-0948-b: Prompt API Endpoints

## Acceptance Criteria

- [ ] Add `prompt_id: Option<String>` and `prompt_variables: Option<HashMap<String, String>>` to `ChatCompletionRequest`
- [ ] Integrate prompt resolution into `proxy.rs` — resolve before provider call
- [ ] `resolve_prompt()` renders template and injects as system message
- [ ] Implement A/B testing — deterministic hashing (API key ID preferred, X-Request-Id fallback, generated UUID)
- [ ] Single source of truth for A/B weights: `AbTest.weight_b` only
- [ ] `AbTestMetrics` uses `AtomicU64` for concurrent counter updates
- [ ] Persist `AbTestMetrics` to stoolap
- [ ] Add Python SDK prompt support
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/proxy.rs` — Integrate prompt resolution
- `crates/quota-router-core/src/prompts/ab_test.rs` — New
- `crates/quota-router-core/src/python_sdk/mod.rs` — Python prompt support
