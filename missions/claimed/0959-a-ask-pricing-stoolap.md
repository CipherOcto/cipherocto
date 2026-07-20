# Mission: Node Ask + Multi-Axis Pricing in OCTO-W (with Stoolap `asks` table)

## Status

Claimed (2026-07-20)

> **Availability:** Mission is now CLAIMABLE per BLUEPRT Mission Lifecycle (Requires RFC-0959 + RFC-0009 + RFC-0853 + RFC-0957 all reached Accepted 2026-07-20; RFC-0862 already Accepted 2026-06-20; RFC-0126 already Accepted). Implementation coverage (RFC-0959 §Specification) ships the spec; the implement transition is now unblocked as of 2026-07-20.

## RFC

- RFC-0959 (Economics): Independent Settlement Chain for Ask Pricing (Option A rewrite 2026-07-20) — ACCEPTED 2026-07-20 v1.0 (authored 2026-07-19 v0.1-v0.3 amendment → rewritten 2026-07-20 v1.0 Option A per S04 audit; file moved `rfcs/draft/economics/0959-ask-settlement-chain.md` → `rfcs/accepted/economics/0959-ask-settlement-chain.md`; 8 BLUEPRINT v1.3 mandatory sections present + §Economic Analysis + byte-exact test vectors TV-3 + TV-4)
- RFC-0909 (Economics): Deterministic Quota Accounting — Accepted (v69); coexistence only (independent chain per Option A; no v70 bump required). **Folder/header note:** file lives in `rfcs/final/economics/` but header reads `Accepted (v69)`; Final-stage convention per BLUEPRINT.md is "Implemented and stable" — RFC-0909 v69 is Accepted without known implementation. Reconciliation tracked as §0 prereq action in `docs/plans/2026-07-19-session-03-node-ask-pricing.md`.

**BLUEPRINT gate note (resolved 2026-07-20):** Per BLUEPRINT.md "Missions REQUIRE an approved RFC. No RFC = Create one first." — this mission is now CLAIMABLE; all Requires RFCs below reached Accepted status 2026-07-20 (RFC-0959 v1.0 + RFC-0853 + RFC-0009 + RFC-0957 all promoted same day; RFC-0862 + RFC-0126 + RFC-0909 already Accepted pre-2026-07-20). The Requires RFC table below is retained for reference:

| Requires RFC | Status | Blocking |
|--------------|--------|----------|
| RFC-0959 | ACCEPTED (2026-07-20, v1.0) | Yes |
| RFC-0909 | Accepted | No (already Accepted) |
| RFC-0126 (Deterministic Serialization; canonical_ser substrate) | Accepted | No |
| RFC-0853 (Overlay Cryptography; BLAKE3 primitive) | ACCEPTED (2026-07-20) | No (promoted) |
| RFC-0009 (Identity Management; IdentityKey + NodeType) | ACCEPTED (2026-07-20) | No (promoted) |
| RFC-0957 (Capability Token Format; cap_root_hash + AskBinding host) | ACCEPTED (2026-07-20) | No (promoted) |
| RFC-0862 (Stoolap Sync Layer; marketplace rebuild driver; `rfcs/accepted/networking/0862-stoolap-data-sync.md`) | **Accepted (2026-06-20; v1.2.0 updated 2026-06-25)** | No (already Accepted; **R3 fix**: line 144 Cross-RFC primitives table previously said "Draft"; corrected to Accepted across all surfaces) |

Claim completed 2026-07-20: RFC-0959 + RFC-0853 + RFC-0009 + RFC-0957 promotion to Accepted via 7-day review + 2 maintainer approvals each (per master plan §0 + BLUEPRINT.md RFC Acceptance Process). RFC-0862 was already Accepted 2026-06-20 pre-claim; RFC-0862 line above marked 'No (already Accepted)'. **RFC-0862 is no longer a hard-blocks gate** — `rfcs/accepted/networking/0862-stoolap-data-sync.md` is already Accepted per actual repo state (R3 fix).

## Summary

Implement the Ask primitive + multi-axis pricing in OCTO-W end-to-end. Delivers `crates/octo-core/src/ask.rs` + `settlement.rs` + `cache.rs` + `axis_registry.rs`, an MVP PricingAxis registry (3 axes, snake_case registry IDs), a stoolap `asks` table migration (cross-repo PR to `/home/mmacedoeu/_w/databases/stoolap` on `feat/blockchain-sql` branch), an in-memory marketplace index rebuilt on RFC-0862 sync events (`BTreeMap<(namespace, family, version), BTreeSet<AskId>>`), the settlement hash extension over RFC-0909 binding `cap_root_hash + ask_id + invocation_hash + canonical_axes_consumed`, integer-only `compute_cost` (u128 throughout, type-distinct `OCTO_WAmount` (display) + `MicroOCTO_W` (on-wire) per RFC-0959 fix), BLAKE3-based cache classification, multi-layer anti-fraud (provider cooperation + advisory circuit-breaker + receipt `cache_key_hash` binding — anti-fraud is ADVISORY only, does NOT mutate canonical axes_consumed), and CLI surfaces (`octo-wallet ask publish/list/show/revoke`, `quota-router-cli settle/settle-replay`). Final-state property test: settlement_hash equal across two independent nodes replaying the same event set; cross-impl replay via RFC-0959 §Test Vectors fixed bytes.

## Acceptance Criteria

### Ask primitive (`crates/octo-core/src/ask.rs`)

- [ ] `AskUnsignedPayload` struct with `asker_did`, `node_type`, `model`, `axes`, `ttl_unix`, `jurisdiction`, `published_at_unix` (RFC-0959 §Data Structures; signed content; `ask_id` + `signature` derived FROM it, never part of canonical signed payload — non-circular per R1 fix)
- [ ] `Ask` struct with `ask_id`, `payload`, `signature` (RFC-0959 §Data Structures; non-circular composition)
- [ ] `AskId = [u8; 32]`; `ask_id = BLAKE3(canonical_ser(AskUnsignedPayload))` — RFC-0959 §Algorithms (non-circular derivation)
- [ ] `PricingAxis { id, unit, per_octow_resolution, description }` — snake_case registry IDs
- [ ] `ModelRef { namespace: String, family: String, version: Option<String> }` — strings, no enum
- [ ] **Type-distinct** `OCTO_WAmount(pub u128)` (display) + `MicroOCTO_W(pub u128)` (on-wire, 1 OCTO-W = 1_000_000 MicroOCTO_W) per RFC-0959 §Data Structures — NOT type aliases to `u128` (R1 critical fix; type aliases permitted silently)
- [ ] `TokenCount = u32` (per-axis cap = 4.29B tokens); `Ed25519Signature = [u8; 64]` (RFC 8032 standard)
- [ ] `NodeType` re-exported from `octo-wallet::node` (RFC-0009 substrate)
- [ ] `Ask::sign(identity: &IdentityKey) -> (AskId, Ed25519Signature)` produces `(ask_id, sig)` from payload only
- [ ] `Ask::verify() -> Result<(), SettlementError>` — recompute `ask_id = BLAKE3(canonical_ser(payload))` (NOT raw payload); assert recomputation equals `ask.ask_id`; verify Ed25519 signature over `canonical_ser(payload)` against `payload.asker_did` per RFC-0959 §Algorithms `verify_ask` (R2 fix: BLAKE3 + canonical_ser derivation was missing from R1 acceptance criterion)
- [ ] Round-trip serialization test: sign → serialize → deserialize → `ask_id` stable + `verify()` succeeds

### Settlement engine (`crates/octo-core/src/settlement.rs`)

- [ ] `AxesConsumed { axes: BTreeMap<String, TokenCount>, cache_key_hash: Option<[u8; 32]> }`
- [ ] `compute_cost(ask, axes) -> Result<MicroOCTO_W, SettlementError>` — `Σ ceil(tokens/1000) * rate[axis]` (u128 throughout, integer-only, no float)
- [ ] `SettlementEvent { cap_root_hash: [u8; 32], ask_id: AskId, invocation_hash: [u8; 32], axes_consumed: AxesConsumed, cost: MicroOCTO_W, settled_at_unix: u64 }` (no `hash` field — `settlement_hash()` computed externally; R1 fix)
- [ ] `SettlementReceipt { event: SettlementEvent, router_signature: Ed25519Signature, nonce: [u8; 16] }` — nonce derived from `csprng.next_u64().to_le_bytes() ++ wall_clock_now.to_le_bytes()`; signed by router identity over `canonical_ser((event || nonce || settled_at_unix))` (RFC-0959 §Algorithms nonce defense — R1 fix)
- [ ] `settlement_hash(event: &SettlementEvent) -> Result<[u8; 32], SettlementError>` — `BLAKE3(b"cipherocto/settlement/v1\n" || cap_root_hash || ask_id || invocation_hash || canonical_ser(axes_consumed))`; **Result return** to propagate `CanonicalSerError` (R1 fix)
- [ ] `SettlementError` enum: `UnknownAxis(String)`, `AskExpired { ask_id, ttl_unix, now }`, `AskNotFound(AskId)`, `JurisdictionMismatch { declared, actual }`, `CacheStrategyRequired`, **`Overflow { axis_id: String, partial_sum: MicroOCTO_W }`**, `AskSignatureInvalid`, `CanonicalSerError(serde_json::Error)` (RFC-0959 §Data Structures; `Overflow` is a MUST-add variant — R1 critical fix; `AskSignatureInvalid` added per same fix pass; `NonceGenerationError` removed in R2 — unreachable because `csprng.next_u64()` returns `u64`, not `Result`)
- [ ] Property test: 10K random `(ask, cap_root_hash, invocation_hash, axes_consumed)` tuples replayed across 2 nodes → identical 32-byte `settlement_hash`
- [ ] Property test: `compute_cost` overflow → `SettlementError::Overflow { axis_id, partial_sum }`, never panics
- [ ] Test: `CachedInputTokensPer1k` axis without `cache_key_hash` → `SettlementError::CacheStrategyRequired`
- [ ] Test: 100 random unsigned / ask_id-tampered asks → `AskSignatureInvalid` returned
- [ ] Test: every `SettlementError` variant has a documented acceptance path (no dead error arms)

### Cache classification (`crates/octo-core/src/cache.rs`)

- [ ] `cache_key(prompt_tokens: &[u32]) -> [u8; 32]` — BLAKE3 keyed-hash keyed on `CACHE_KEY_DOMAIN: [u8; 32]` per RFC-0959 v1.0 §Data Structures (exactly 32-byte literal `b"cipherocto/cache-key/v1........."` — R7 fix: 23 chars + 9 dots; R6 used 10 dots = 33 bytes); **was originally `b"cipherocto/cache/v1\0"` (20 bytes) per S03 v0.3 spec; updated to RFC-0959 v1.0 32-byte requirement**; canonical byte encoding = `u32::to_le_bytes()` concatenated per-token, version-byte prefixed
- [ ] Test: identical prompts → identical `cache_key`; distinct → divergent
- [ ] Test: cache hit detected on deduplicated prompt; miss on distinct

### PricingAxis registry (`crates/octo-core/src/axis_registry.rs` + `crates/octo-core/config/pricing-axes.toml`)

- [ ] TOML parser at boot; rejects unknown axis IDs in capability caveat (fail-closed)
- [ ] MVP axes: `input_tokens_per_1k`, `output_tokens_per_1k`, `cached_input_tokens_per_1k` (snake_case registry IDs)
- [ ] `PricingAxis.per_octow_resolution: MicroOCTO_W` (u128 integer field; e.g., `500000` for `0.5 OCTO-W/1K`)
- [ ] Add new axis at runtime → capability mints against it
- [ ] Test: TOML parser rejects mixed-case axis IDs (snake_case required; kebab-case rejected)

### Stoolap `asks` table (cross-repo: `/home/mmacedoeu/_w/databases/stoolap` on `feat/blockchain-sql`)

- [ ] Migration: `CREATE TABLE asks (...)` Stoolap DDL per RFC-0959 §Implementation Phases + S03 plan §3 Step 3 (columns: `id` PRIMARY KEY `BLAKE3(canonical_ser(AskUnsignedPayload))`, `asker_did`, `node_type`, `model_namespace`, `model_family`, `model_version`, `axes_json`, `ttl_unix`, `jurisdiction_json`, `signature` BLOB 64 bytes, `published_at_unix`, `revoked_at_unix` NULL)
- [ ] Idempotent: `IF NOT EXISTS`; migrate over empty DB and populated DB both succeed
- [ ] Indexes: `idx_asker (asker_did)`, `idx_model (model_namespace, model_family, model_version)`. **No partial `idx_active` over `ttl_unix > now()`** — Stoolap `CreateIndexStatement` AST does not support volatile predicates (R1 fix); eviction handled by router-side in-memory BTreeMap + explicit revoke
- [ ] Fixture: 10 models × 5 axis combos × 2 NodeType = 100 Asks seeded (unit-test scale; 100K scale-bench at §4 Validation bench)
- [ ] Migration cross-repo PR sequencing: cipherocto PR (spec + Rust types) lands first; stoolap fork PR (asks table migration) lands second or co-lands per master plan §5 Session 05
- [ ] **Workspace validation includes stoolap fork:** `cd /home/mmacedoeu/_w/databases/stoolap && cargo test --lib --features cipherocto-asks` runs the migration against empty + populated DB

### Marketplace index (`crates/quota-router-core/src/marketplace.rs`)

- [ ] In-memory `BTreeMap<(String namespace, String family, Option<String> version), BTreeSet<AskId>>` rebuilt on RFC-0862 sync event (RFC-0959 §Roles fix — BTreeMap for ordered + deterministic scans, NOT HashMap)
- [ ] `select_ask(did, model, jurisdiction, budget_ceiling) -> Option<Ask>` — deterministic tie-break by `ask_id` ASC (lowest ask_id wins within budget)
- [ ] Cache invalidation: ask `ttl_unix < now` → evict; pruned in `published_at_unix` ASC order at cap 100K
- [ ] Benchmark harness: reproducible protocol — warm-up 1000 calls, sample 10K calls, p99 recorded per S03 plan §4 Validation
- [ ] Test: active-ask cap enforcement at 100K (pruning policy)
- [ ] Test: deterministic tie-break across multiple selection calls (same state → same ask_id returned)

### Anti-fraud monitor (`crates/quota-router-core/src/anti_fraud.rs`)

- [ ] Per-ask, per-customer cache-hit-rate dashboard hook
- [ ] Circuit-breaker: if `cache_hit_rate > 0.90` over last 1K calls AND prompt diversity > `MIN_PROMPT_DIVERSITY = 50` unique BLAKE3 keys → advisory state transition Active → Tripped (R3 fix: mission now requires all 5 RFC transitions — Active→Tripped, Tripped→Recovering, Recovering→Active, Active→Recovering, Recovering→Tripped per RFC-0959 §Anti-Fraud Monitor state machine; `Active → Recovering` requires Operator signature for administrative audit; advisory only — does NOT mutate canonical axes_consumed)
- [ ] **State machine per RFC-0959 §Lifecycle Requirements:** `Active → Tripped → Recovering → Active`; transitions do NOT mutate canonical `axes_consumed` on already-settled events; only gate FUTURE `CachedInputTokensPer1k` vs `InputTokensPer1k` axis classification (R1 critical fix — anti-fraud does NOT break Class A settlement equivalence)
- [ ] Reputation delta on confirmed fraud signal (`RFC-0909 §Failure Handling` contract; cross-link recorded in RFC-0959 §Security Considerations)
- [ ] Multi-layer mitigation per RFC-0959 §Adversary A5 HIGH severity: provider cooperation (provider-side `cache_control == HIT` cross-check required for `CachedInputTokensPer1k`) + receipt `cache_key_hash` binding
- [ ] Test: simulate hit-rate spike; circuit-breaker trips after threshold; verify settled events unchanged post-trip

### CLI (`crates/octo-wallet/src/bin/octo-wallet.rs` + `crates/quota-router-cli/src/main.rs` — binary `quota-router-cli`)

- [ ] `octo-wallet ask publish --model openai/gpt-4 --axes input:500000,output:1500000,cached:50000 --jurisdiction US,EU --ttl 30d --key <identity>` (rates as integers in `MicroOCTO_W`; CLI accepts aliases `input/output/cached` mapping to TOML IDs `input_tokens_per_1k/output_tokens_per_1k/cached_input_tokens_per_1k` per S03 §3 Step 9 alias normalization; `0.5 OCTO-W/1K = 500000 MicroOCTO_W`; missing `--jurisdiction` defaults to empty set; `--ttl` defaults to 30 days; `--key` resolves via `octo-wallet`'s IdentityKey slot)
- [ ] `octo-wallet ask list [--namespace openai] [--cheapest]` (`--cheapest` = lowest `compute_cost` for `(input_tokens_per_1k=1000, output_tokens_per_1k=1000)` synthetic consumption)
- [ ] `octo-wallet ask show <ask_id>` (prints `AskUnsignedPayload` + signature + asker_did)
- [ ] `octo-wallet ask revoke --ask-id <id>` (writes `revoked_at_unix` row update)
- [ ] `quota-router-cli settle --ask <ask_id> --cap-root-hash <hex32> --invocation-hash <hex32> --axes input:<n>,output:<m> [--cache-key-hash <hex32>]` (binary `quota-router-cli` per repo crate layout `crates/quota-router-cli/` with `[[bin]] name = "quota-router-cli"`; flags match `SettlementEvent` shape; missing `--cache-key-hash` with `cached_input_tokens_per_1k` axis → `SettlementError::CacheStrategyRequired`). **R2 fix:** R1 incorrectly renamed this binary to `quota-router`; the actual repo crate layout is `crates/quota-router-cli/` with binary `quota-router-cli` (`crates/quota-router-cli/Cargo.toml` confirms `name = "quota-router-cli"`). Reverted.
- [ ] `quota-router-cli settle-replay --log-path <path> --expected-hash-manifest <json>` — sequential event replay; reads manifest of `{event_id, expected_settlement_hash}` pairs; computes `settlement_hash(event)` per event; prints PASS/FAIL per event + aggregate count. (R2 fix: per-event hashes cannot match a single `--expected-hash`; manifest format added.)
- [ ] Tests via `assert_cmd`

### RFC-0959 follow-up amendments

- [ ] Add `## Version History` entry referencing this mission's PR
- [ ] RFC-0959 promotion Draft → Accepted completed 2026-07-20 (was gated pre-claim; claim now filed)
- [ ] Bump RFC-0909 to v70 once RFC-0959 reaches Accepted (header + Version History entry) — **DEPRECATED per Option A** (RFC-0959 v1.0 is independent chain; no v70 bump required)

### Cross-crate compat

- [ ] `cargo build --workspace` green
- [ ] `cargo test --workspace` green (existing octo-core/octo-wallet/octo-router tests still pass)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean (per CLAUDE.md repo lint rule)
- [ ] `cargo fmt --check` clean
- [ ] `cargo doc --workspace --no-deps` builds without broken-doc-link warnings
- [ ] Stoolap fork validation: `cd /home/mmacedoeu/_w/databases/stoolap && cargo test --lib --features cipherocto-asks` passes

## Dependencies

**Mission-level (RFC prerequisites):**

| Mission | RFC | Status | Hard-blocks claim? |
|---------|-----|--------|--------------------|
| `0102-a-wallet-foundation.md` | RFC-0102 — ACCEPTED (2026-07-20)) | Yes — IdentityKey + NodeType substrate |
| `0957-a-capability-token-macaroon.md` | RFC-0957 — ACCEPTED (2026-07-20)) | Yes — cap_root_hash + AskBinding host |
| **This mission** | RFC-0959 | Claimed (2026-07-20; RFC ACCEPTED v1.0; this RFC) | n/a (self) |

**Cross-RFC primitives (gating per RFC-0959 §Dependency Validation):**

| RFC | Status | Hard-blocks RFC-0959 acceptance? |
|-----|--------|-----------------------------------|
| RFC-0853 — ACCEPTED (2026-07-20) | YES (IA-1 ACCEPTED RISK) |
| RFC-0126 (Deterministic Serialization / canonical_ser; accepted; substrate for `AskUnsignedPayload.sign` field hash; **`canonical_ser` implementation gap**: this mission assumes a working `canonical_ser` library — verification of an existing crate is not in scope; if no library exists, this mission's `ask_id` derivation needs an alternate non-canonical_ser implementation OR a separate mission must implement canonical_ser first.) | Accepted | implementation gap: verify or ship prerequisite |
| RFC-0862 (Stoolap Sync Layer; `rfcs/accepted/networking/0862-stoolap-data-sync.md`) | **Accepted (2026-06-20; v1.2.0 updated 2026-06-25)** | **No** (already Accepted; corrected per R3 fix — supersedes earlier "Draft" reading) |
| RFC-0909 (Deterministic Quota Accounting; coexistence only per Option A — independent chain) | Accepted | No |

**Library-level:**

- `crates/octo-wallet` — IdentityKey + NodeType (from S01 mission)
- `crates/octo-core` — base types + canonical_ser (RFC-0126)
- `/home/mmacedoeu/_w/databases/stoolap` (forked repo, NOT `external/stoolap`) — `asks` table persistence (cross-repo PR to `feat/blockchain-sql` branch)

## Type Coverage

Per BLUEPRINT.md Mission template, the RFC-0959 specification defines the following types; this mission implements them as listed:

| RFC-0959 Type | Implemented By |
|---------------|----------------|
| `AskUnsignedPayload` struct | This mission (in `crates/octo-core/src/ask.rs`) |
| `Ask` struct | This mission (in `crates/octo-core/src/ask.rs`) |
| `AskId` type alias (`[u8; 32]`) | This mission (in `crates/octo-core/src/ask.rs`) |
| `OCTO_WAmount(pub u128)` newtype (display unit) | This mission (in `crates/octo-core/src/ask.rs`) |
| `MicroOCTO_W(pub u128)` newtype (on-wire unit, 1 OCTO-W = 1e6) | This mission (in `crates/octo-core/src/ask.rs`) |
| `TokenCount` type alias (`u32`) | This mission (in `crates/octo-core/src/ask.rs`) |
| `Ed25519Signature` type alias (`[u8; 64]`) | This mission (in `crates/octo-core/src/ask.rs`) |
| `PricingAxis` struct (in `axis_registry.rs`, NOT `ask.rs` per RFC-0959 §Data Structures cross-ref) | This mission |
| `ModelRef` struct | This mission (in `crates/octo-core/src/ask.rs`) |
| `NodeType` enum | Re-exported from `octo-wallet::node` (RFC-0009 substrate; S01 mission) |
| `AxesConsumed` struct | This mission (in `crates/octo-core/src/settlement.rs`) |
| `SettlementEvent` struct (no `hash` field — R1 fix; `settlement_hash()` is a free function) | This mission (in `crates/octo-core/src/settlement.rs`) |
| `SettlementReceipt` struct (`event` + `router_signature` + `nonce`; R1 fix — replay defense) | This mission |
| `SettlementError` enum (with `Overflow { axis_id, partial_sum }` + `AskSignatureInvalid` variants per R1 critical fix; **R3 fix**: removed stale `NonceGenerationError` mention — variant was removed in R2 as unreachable per `csprng.next_u64()` returning `u64` not `Result`) | This mission (in `crates/octo-core/src/settlement.rs`) |
| `compute_cost(ask, axes) -> Result<MicroOCTO_W, SettlementError>` fn | This mission |
| `settlement_hash(event) -> Result<[u8; 32], SettlementError>` fn (R1 Result return fix) | This mission |
| `cache_key(prompt_tokens: &[u32]) -> [u8; 32]` fn | This mission (in `crates/octo-core/src/cache.rs`) |
| `sign_ask(identity, payload) -> (AskId, Ed25519Signature)` fn | This mission |
| `verify_ask(ask) -> Result<(), SettlementError>` fn | This mission |
| Marketplace BTreeMap: `BTreeMap<(String, String, Option<String>), BTreeSet<AskId>>` (R1 BTreeMap-over-HashMap fix) | This mission (in `crates/quota-router-core/src/marketplace.rs`) |
| Full capability token (Caveat::AskBinding payload schema) | Reference only — RFC-0957 (S02 mission `0957-a-capability-token-macaroon.md`) hosts this; this mission supplies `AskId` as the binding key |
| ZK-class capability subclass | **NOT this mission** — RFC-0958 (S05 mission, Planned) |
| On-chain settlement discharge | **NOT this mission** — no canonical RFC yet for on-chain ASK settlement |
| Dual-stake economics (OCTO-W provider role) | Token role economics documented in `docs/04-tokenomics/token-design.md`; not implemented here |

## Location

- New files: `crates/octo-core/src/ask.rs`, `crates/octo-core/src/settlement.rs`, `crates/octo-core/src/cache.rs`, `crates/octo-core/src/axis_registry.rs`, `crates/octo-core/config/pricing-axes.toml`
- New files: `crates/quota-router-core/src/marketplace.rs`, `crates/quota-router-core/src/anti_fraud.rs`
- CLI additions: `crates/octo-wallet/src/bin/octo-wallet.rs` (existing, extend with `ask` subcommand group), `crates/quota-router-cli/src/main.rs` (existing crate, extend with `settle` + `settle-replay` subcommands)
- Cross-repo PR: `/home/mmacedoeu/_w/databases/stoolap` on `feat/blockchain-sql` branch (asks table migration)
- RFC edits this mission triggers:
  - `rfcs/accepted/economics/0959-ask-settlement-chain.md` (ACCEPTED v1.0 — Option A independent chain; file moved 2026-07-20 per S04 audit)
  - `rfcs/final/economics/0909-deterministic-quota-accounting.md` (no changes required per Option A — independent chain, no v70 bump)
  - `rfcs/draft/economics/0900-ai-quota-marketplace.md` (cross-link from §SettlementModel to RFC-0959 v1.0)
- Plan: `docs/plans/2026-07-19-session-03-node-ask-pricing.md`

## Complexity

Medium-High (3 new core modules, 1 marketplace router module, 1 anti-fraud module, 6 CLI subcommands, cross-repo PR, property tests, multi-feature cross-refs).

## Reference

- `docs/plans/2026-07-19-identity-master-plan.md` § 0 BLUEPRINT Workflow Gate
- `docs/plans/2026-07-19-session-03-node-ask-pricing.md` § 0 BLUEPRINT Workflow Gate + § 3 Steps 1-10
- `rfcs/accepted/economics/0959-ask-settlement-chain.md` — ACCEPTED v1.0 (this mission's primary spec authority; Option A independent chain)
- `rfcs/final/economics/0909-deterministic-quota-accounting.md` (v69) — coexistence only (independent chain per Option A; no v70 bump)
- `rfcs/accepted/economics/0957-capability-token-format.md` — capability token + `AskBinding` caveat host
- `rfcs/accepted/economics/0910-pricing-table-registry.md` (v31) — pricing table consumer
- `rfcs/accepted/networking/0853-overlay-cryptography.md` — BLAKE3 primitive
- `rfcs/accepted/process/0009-identity-management.md` — IdentityKey + NodeType
- `rfcs/accepted/numeric/0126-deterministic-serialization.md` — canonical_ser
- `docs/research/ai-quota-marketplace-research.md` — feasibility
- `docs/research/pricing-axes-research.md` — MVP axis selection + extension model
- `docs/use-cases/ai-quota-marketplace.md` — intent layer

## Security Review Status

- RFC-0959 §Adversary Analysis: 5 decisions (A1-A5), severity tagged. **CRITICAL = none; HIGH × 1 (A5 cache-hit-rate gaming — multi-layer mitigation documented per RFC-0959 §Adversary A5 + §Lifecycle Requirements Anti-Fraud Monitor); MEDIUM × 1 (A4 USD-fiat audit residual) per R2 reviewer reclassification consistency check. LOW × 3 (A1 BLAKE3/Ed25519 stack, A2 BLAKE3 collision residual, A3 axis-set extension).** Multi-round adversarial review: scheduled after all Requires RFCs reach Accepted status. (R2 fix: previously listed A5 as MEDIUM after R1 reclassification to HIGH — this contradicts RFC-0959 §Severity Classification line 482; corrected.)
- RFC-0959 §Implicit Assumptions Audit: 8 entries (IA-1 to IA-8). Three ACCEPTED RISK entries (IA-1, IA-2, IA-3) gated on Draft-RFC promotion prior to RFC-0959 acceptance.
- Multi-round adversarial review: scheduled after all Requires RFCs reach Accepted status.

## Claimant

CLAIMED 2026-07-20 (mission moved from missions/open/0959-a-ask-pricing-stoolap.md to missions/claimed/0959-a-ask-pricing-stoolap.md per BLUEPRT Mission Lifecycle; all 4 new Requires RFCs reached Accepted 2026-07-20 — RFC-0959 v1.0 + RFC-0853 + RFC-0009 + RFC-0957; RFC-0862 + RFC-0126 + RFC-0909 already Accepted pre-2026-07-20)

## Pull Request

(none yet — implementation pending per S03 plan §3 Steps 1-10 sequencing)

## Implementation Guide

No external implementation guide authored (out of scope for this mission). Inline implementation notes per acceptance criterion above; Rust type signatures match RFC-0959 §Data Structures.

For complex sub-systems, missions may link a companion implementation guide at `docs/07-developers/{topic}-implementation-guide.md` per BLUEPRINT.md "Tools" section. This mission does not require a separate guide (≤10 distinct modules).

## Notes

- **Mission decomposition:** RFC-0959 defines >10 RFC types listed in `## Type Coverage` (Ask, AskUnsignedPayload, AskId, OCTO_WAmount, MicroOCTO_W, TokenCount, Ed25519Signature, PricingAxis, ModelRef, NodeType, AxesConsumed, SettlementEvent, SettlementReceipt, SettlementError = 14 types). Per BLUEPRINT.md "Multi-Mission Decomposition" rule (>10 types → decompose into `{RFC-number}{letter}-{abbreviation}-{description}.md` sub-missions), this exceeds the threshold. Acceptable-for-now rationale: Ask primitive + PricingAxis + Settlement engine form a single content-addressable surface (ask_id derives from canonical_ser of payload fields across all three modules) that must ship atomically to avoid signature/identity divergence. If PR review flags atomicity as a blocker, decompose into `0959-b-pricing-registry.md` (just `PricingAxis` TOML) + `0959-c-settlement-engine.md` (just `SettlementEvent` + `Receipt` + `Error`) + keep this as `0959-a-ask-types.md`.
- **Cross-repo coordination note:** Migration lives at `/home/mmacedoeu/_w/databases/stoolap` (NOT `external/stoolap` which is the cipherocto repo convention but doesn't apply to the stoolap fork) on `feat/blockchain-sql` branch per master plan §5 Session 05 + RFC-0959 §Key Files to Modify. PR ordering: cipherocto PR (spec + Rust types) lands first; stoolap fork PR (asks table migration) lands second or co-lands.
- **RFC-0909 folder/header reconciliation:** Per BLUEPRINT.md Stage convention, Final-stage folder = Implemented and stable. RFC-0909 v69 is Accepted but not yet implemented in `crates/octo-core/src/settlement.rs`. This mission's implementation triggers the v70 bump per RFC-0959 §Implementation Phases Phase 1: header updated to `Accepted (v70)` + Version History entry referencing RFC-0959.
- **PricingAxis registry versioning:** Per RFC-0959 §Future Work — adding a new axis instance (different rate within an existing class, e.g. `input_tokens_per_2k`) is a TOML commit + parser version bump ONLY (no RFC revision). Adding a new axis CLASS (e.g. streaming delay) requires RFC-0959 v2. Image/audio/fine-tuning axes are REGISTRY-ONLY axis instances per F2 (TOML commit suffices; NO RFC revision).
- **Cache-hit-rate circuit-breaker:** Heuristic signal (`MIN_PROMPT_DIVERSITY = 50` unique BLAKE3 keys) per RFC-0959 §Adversary A5 (HIGH severity per R1 reclassification). Multi-layer mitigation: provider cooperation + circuit-breaker + receipt `cache_key_hash` binding. Anti-Fraud Monitor is advisory only — state transitions gate FUTURE axis classification, do NOT retroactively mutate canonical axes_consumed. False-positive risk: legitimate batch jobs with similar prompts (e.g., log analysis, repeated queries) could trip breaker; ≤ 1% false-positive target per RFC-0959 §Adversary A5; monitor + alert.
- **Settlement hash forward-compat:** Version tag `b"cipherocto/settlement/v1\n"` allows future revisions without breaking verifiers. New verifiers MAY opt in to `settlement.v69_compat = true` to accept v69 events by setting `ask_id = BLAKE3(zero_vec_32)` + `cap_root_hash = BLAKE3(api_key_id_v69)` placeholders. Default = strict rejection of v69 events. RFC-0909 v69 verifiers see v70 events as unrecognized preimage and reject (NOT a forward-compat — RFC-0959 strict-rejection default; opt-in for v69 acceptance). The claim "v0 receivers parse as RFC-0909 v69 baseline by treating unknown fields as zero" is **incorrect** per RFC-0959 §Dependency Validation — corrected to: v70 verifiers MAY opt in to v69-compat; default behavior = strict rejection. (R1 reviewer fix.)
- **canonical_ser implementation gap:** RFC-0126 is the substrate for `AskUnsignedPayload` canonicalization + `AxesConsumed` canonicalization. If no working `canonical_ser` library exists in `crates/octo-core/`, this mission needs an alternate implementation (e.g., manual BTreeMap deterministic string assembly) OR a separate mission must implement canonical_ser first. Acceptance criterion: `crates/octo-core/src/canonical.rs` exists with `canonical_ser<T: Serialize>(val: &T) -> Result<Vec<u8>, CanonicalSerError>` producing RFC-0126 conformance (BTreeMap key sorting, version-byte prefix, fixed-point integer encoding). If absent, mission gates on adjacent `0126-cs-conformance-implementation.md` mission.
- **CipherOcto brand casing on hash tags:** The settlement hash version tag uses lowercase `b"cipherocto/settlement/v1"` (project brand is `CipherOcto` Pascal case) per RFC-0959 §Settlement hash — the lowercase form is intentional (registry convention per RFC-0959 §Compatibility). Documented as deliberate to prevent reviewer re-flagging the casing.
- **Status block strict-reading:** Per BLUEPRINT.md "Missions REQUIRE an approved RFC" literal reading, mission Status = `Open` is nominal — no claim is permitted until RFC-0959 reaches Accepted, irrespective of RFC authorship. Documented at top of file.
- **Implementation acceptance cross-ref:** every S03 §3 step is captured in §"Acceptance Criteria" above; mission §Notes provides rationale for the boundary between in-scope (this mission) and out-of-scope (S02/S04/S05 missions).
