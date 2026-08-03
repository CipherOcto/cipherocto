# Research: Pricing Axes for Per-Node Ask Market

## Executive Summary

This research investigates the MVP axis set for the per-node Ask primitive in CipherOcto's quota marketplace (Session 03, RFC-0959), the rationale for axis separation (vs. flat per-request pricing), the extension model for future axes, and the cache-classification approach.

Recommendation: ship **3 MVP axes** with snake_case registry IDs (`input_tokens_per_1k`, `output_tokens_per_1k`, `cached_input_tokens_per_1k`), integer-only `u128` math via type-distinct newtypes `OCTO_WAmount(pub u128)` (display) + `MicroOCTO_W(pub u128)` (on-wire, 1 OCTO-W = 1_000_000 MicroOCTO_W), axis registry in `crates/octo-core/config/pricing-axes.toml`, BLAKE3(prompt_tokens) for cache classification, and multi-layer anti-fraud mitigation (provider cooperation + circuit-breaker + receipt `cache_key_hash` binding).

**No production CipherOcto deployment yet** — all numeric thresholds (`MIN_PROMPT_DIVERSITY = 50`, MVP axis set) are heuristic / observational surrogates from external provider pricing models. Empirical validation deferred until Phase F exercise (master plan §6).

## Problem Statement

CipherOcto needs a deterministic, multi-axis pricing model that:

1. Supports per-axis cost attribution (input tokens vs output tokens vs cached vs streaming) without float arithmetic.
2. Settles identically across two independent nodes replaying the same event sequence.
3. Extends to future axes (image, audio, fine-tuning, streaming delay) without breaking the existing settlement hash. The **current** target hash surface is RFC-0909 v69 (`Accepted`); the v70 bump is gated on RFC-0959 acceptance (per master plan §4 Phase D + RFC-0959 §Implementation Phases Phase 1).
4. Detects cache hits deterministically so cache-classified axes consume distinct ledger amounts from non-cache axes.
5. Prevents cache-hit-rate gaming via multi-layer mitigation (RFC-0959 §Adversary A5, reclassified HIGH severity per R1 reviewer feedback).

## Research Scope

**Included:**

- MVP axis selection rationale.
- Currency unit choice (MicroOCTO_W with `u128` integer math, no float; type-distinct wrappers to prevent silent unit-conversion bugs).
- Model-ref schema (`{namespace, family, version?}` strings, no enum).
- Ask identity derivation (BLAKE3 over canonical_ser of `AskUnsignedPayload` per RFC-0959 §Algorithms; signature+ask_id derived independently from unsigned payload to avoid circular derivation).
- PricingAxis registry file format + extension model.
- Cache classification approach (provider-side `cache_control == HIT` + BLAKE3(prompt_tokens) binding).
- Anti-fraud circuit-breaker heuristic + multi-layer mitigation.

**Excluded:**

- On-chain settlement integration (no canonical RFC yet; tracked as a future RFC).
- ZK-class axes (RFC-0958 — Planned, not yet authored).
- Wholesale spread economics (`spread_bps` is basis points — a percentage rate, not USD; recorded in `node_revenue` table only, NOT in settlement hash per RFC-0959 §Adversary A4).
- Provider revenue split logic (settlement engine emits `cost`; downstream ledgering handles splits).

## Findings

### Currency unit

| Approach | Pros | Cons |
|----------|------|------|
| Float (f64) per-axis rate | Familiar | Breaks determinism across implementations (FMA, kernel variations) |
| Decimal (DEC64-style) | Human-readable | Overhead + canonicalization complexity |
| **`u128` newtype-wrapped MicroOCTO_W** (1 OCTO-W = 1e6 micro) | Integer-only; u128 cap = ~3.4e38 (no realistic exhaustion); type-distinct `OCTO_WAmount` vs `MicroOCTO_W` prevents silent unit-conversion; RFC-0909 §Determinism Requirements alignment | Lacks human readability; CLI must format + convert at ingress |

**Recommendation:** Use distinct `OCTO_WAmount(pub u128)` (display unit) and `MicroOCTO_W(pub u128)` (on-wire unit, 1 OCTO-W = 1_000_000 MicroOCTO_W). Conversion via `OCTO_WAmount::to_micro()` at ingress. Aligns with RFC-0909 §Determinism Requirements (no float) + RFC-0903 (virtual API keys) cost-unit treatment. CLI surfaces format OCTO-W amounts with `0.500` display via integer division. Note: RFC-0909 has no numbered §G5 design goal — cite is to §Determinism Requirements; the cited property (`CostUnit = u64` w/ `TOKEN_SCALE = 1000`) is per-axis-micro, differing in scale (1e3 vs 1e6) and type (`u64` vs `u128`) — RFC-0959 chooses `u128` + 1e6 to accommodate router-tier settlement scale (aggregated across many axes) without overflow at production volumes.

### MVP axis selection

| Approach | Pros | Cons |
|----------|------|------|
| Single axis (`PerRequest`) | Simplest | Cannot differentiate cost; providers cannot price cache vs non-cache distinctly |
| Two axes (input vs output) | Matches most providers (OpenAI, Anthropic) | Misses cache pricing incentive |
| **Three axes (input / output / cached-input)** | OpenAI + Anthropic + cache-aware; preserves cache economics | More surface; registry grows |
| All-axes-from-day-1 | Future-proof | Spec bloat; axes invented without data |

**Recommendation:** Three MVP axes with snake_case registry IDs `input_tokens_per_1k`, `output_tokens_per_1k`, `cached_input_tokens_per_1k`. Wait for production data before adding streaming delay, image, audio, fine-tuning axes (registry leaves room; future RFC revision for axis-class additions per RFC-0959 §Future Work F1; image/audio/fine-tuning as registry-only TOML entries per F2 — no RFC revision needed for these specific axes). Cross-RFC consistency: registry IDs match `crates/octo-core/config/pricing-axes.toml` and S03 mission acceptance criteria (snake_case throughout).

### Model-ref schema

| Approach | Pros | Cons |
|----------|------|------|
| Enum (`enum ModelRef { OpenAIGPT4, AnthropicClaude3, ... }`) | Type-safe | Requires RFC revision for each new model; not extensible |
| Strings `{namespace, family, version?}` | Extensible; no RFC revision for new models | No type safety at compile time; runtime validation required |

**Recommendation:** Strings. New models appear daily; enum-style churn is inviable. Runtime validation in `octo-core/src/axis_registry.rs` rejects unknown namespaces (except `cipherocto` namespace which gates on capability bit per RFC-0009 §NodeType — Note: this is the `ModelRef.namespace == "cipherocto"` capability gate, distinct from the Ask `node_type` capability bit; both are documented in RFC-0959 §Implicit Assumptions Audit IA-7).

### Ask identity derivation

| Approach | Pros | Cons |
|----------|------|------|
| UUID v4 | Familiar | Random per mint; no content binding |
| SHA-256(canonical_ser) | Standard | BLAKE3 is faster + native to CipherOcto (RFC-0853) |
| **BLAKE3(canonical_ser(AskUnsignedPayload))** | 256-bit; BLAKE3 native; content-addressable; canonical_ser deterministic per RFC-0126; non-circular (unsigned payload excludes ask_id + signature) | Tied to RFC-0126 version byte |

**Recommendation:** BLAKE3 keyed-hash (or vanilla 256-bit BLAKE3) over canonical_ser of `AskUnsignedPayload` (which excludes both `ask_id` and `signature`). This avoids the circular derivation class of bugs (RFC-0959 R1 reviewer fix). Two implementations signing the same `AskUnsignedPayload` produce identical ask_id. RFC-0853 path is `rfcs/draft/networking/` per repo convention; the primitive is BLAKE3 (CipherOcto brand).

### PricingAxis registry extension model

| Approach | Pros | Cons |
|----------|------|------|
| Hardcoded enum | Compile-time type safety | Every new axis requires code change + RFC revision |
| TOML registry file | Extensible at runtime; no recompile | Runtime validation required |
| **TOML registry + RFC revision for axis-class changes only** | Best of both: trivial addition for known axes; spec control for new axis classes | Boundary between "axis instance" and "axis class" must be defined |

**Recommendation:** TOML registry at `crates/octo-core/config/pricing-axes.toml`. Adding a new axis ID for an existing class (e.g., `input_tokens_per_2k` — instance variation) is a TOML commit + parser version bump. Adding a new axis class (e.g., `streaming_delay_seconds`) requires RFC-0959 v2 (axis-class). Image/audio/fine-tuning axes are registry-only instances — no RFC revision needed.

### Cache classification approach

| Approach | Pros | Cons |
|----------|------|------|
| Provider-side `cache_control == HIT` flag | Accurate (provider knows its cache) | Provider-owned; router trusts provider |
| BLAKE3(prompt_tokens) hash + local cache | Router-side; determinable without provider cooperation | False positives on hash collisions; cache eviction races |
| **Both: provider flag OR local cache hit, plus receipt `cache_key_hash` binding** | Defense in depth; combines accuracy + router verification + per-receipt binding prevents forger cheat without provider cooperation | Two sources of truth; reconciliation needed (provider flag wins) |

**Recommendation:** Both, with provider flag winning + receipt binding `cache_key_hash` per RFC-0959 §Adversary A5 mitigation. Local cache hit treated as hint only. Router verifies prompt tokens deterministically for receipt binding. Prevents gaming class where Asker lies about cache hit without provider cooperation (BLAKE3 binding + signed receipt makes forgery cryptographically expensive + trivially auditable).

### Anti-fraud circuit-breaker

| Approach | Pros | Cons |
|----------|------|------|
| No anti-fraud | Simplest | Cache gaming unmitigated |
| Fixed cache-hit-rate threshold (e.g., >90% triggers) | Catches gaming | Legitimate batch jobs with similar prompts false-positive |
| **Rate + prompt diversity threshold** (`MIN_PROMPT_DIVERSITY = 50` unique BLAKE3 keys) + receipt binding + provider cooperation (3-layer defense per RFC-0959 §Adversary A5) | Catches gaming AND allows legitimate batch | Diversity heuristic imperfect; HIGH residual risk; multi-layer mitigation reduces residual but cannot eliminate |

**Recommendation:** Combined signal + multi-layer mitigation. Documented as RFC-0959 §Adversary A5 (HIGH severity — reclassified per R1 reviewer finding; trivial adversary cost + revenue-bypass gain requires multi-layer defense: provider cooperation + circuit-breaker + receipt binding). Anti-Fraud Monitor is **advisory only** per RFC-0959 §Lifecycle Requirements — state transitions (Active/Tripped/Recovering) gate FUTURE classification, do NOT retroactively mutate settlement-consumed axes (preserves Class A settlement determinism).

The `MIN_PROMPT_DIVERSITY = 50` threshold is **heuristic** with no empirical derivation; served as a strawman value during R0 design. Production tuning deferred until Phase F (master plan §6 11-step exercise) collects real adversary data. RFC-0959 v2 may revise the value based on exercise-path telemetry; S03 mission acceptance criteria treats the value as a default with documentation that production tuning is required.

## Recommendations

1. **Adopt 3 MVP axes** with snake_case IDs `input_tokens_per_1k`, `output_tokens_per_1k`, `cached_input_tokens_per_1k` per RFC-0959 §Specification → Algorithms → PricingAxis registry (NOT §Design Goals — §Design Goals lists determinism + latency + integer-math + extension + back-compat properties; MVP axis set is in §Summary + §Data Structures).
2. **Currency = type-distinct newtypes**: `OCTO_WAmount(pub u128)` (display) + `MicroOCTO_W(pub u128)` (on-wire) per RFC-0959 §Data Structures. `MicroOCTO_W = OCTO_WAmount * 1_000_000`. No float anywhere; u128 cap is sufficient.
3. **Model ref = strings** `{namespace, family, version?}`. `cipherocto` namespace gates on capability bit per RFC-0959 §Implicit Assumptions IA-7 (distinct from `Ask.node_type` capability gate).
4. **Ask identity = BLAKE3(canonical_ser(AskUnsignedPayload))**. Non-circular: unsigned payload excludes ask_id and signature. 256-bit; 2^-128 collision resistance.
5. **PricingAxis registry = TOML file** at `crates/octo-core/config/pricing-axes.toml`. snake_case IDs. Trivial extension for known axes; RFC-0959 v2 revision for axis-class additions (streaming delay).
6. **Cache classification = provider flag wins, local cache as hint**. BLAKE3(prompt_tokens) bound in receipt `SettlementEvent.axes_consumed.cache_key_hash`. Provider cooperation required for `CachedInputTokensPer1k` axis classification.
7. **Anti-fraud = circuit-breaker** at `MIN_PROMPT_DIVERSITY = 50` unique BLAKE3 keys (advisory only — does NOT mutate canonical axes_consumed). HIGH residual risk per RFC-0959 §Adversary A5.

## Evaluation Criteria (BLUEPRINT.md Research Review Gate)

Per `docs/BLUEPRINT.md` "Research Review Gate" line 232-247 — research → Use Case promotion requires Evaluation Criteria approval:

| Criterion | Assessment | Notes |
|-----------|------------|-------|
| **Technical feasibility** | ✓ PASS | BLAKE3 + u128 + canonical_ser stack exists in adjacent RFCs (RFC-0853, RFC-0126); TOML parser routine; provider cooperation is the only external dependency |
| **Protocol relevance** | ✓ PASS | Per-node Ask pricing is the explicit purpose of RFC-0959 + master plan §3 Invariant 5 (OCTO-W native) |
| **Economic viability** | ⚠ DEFERRED | No production traffic; `MIN_PROMPT_DIVERSITY = 50` heuristic; empirical validation pending Phase F exercise data |
| **Security implications** | ✓ PASS w/ residual HIGH (A5) | Multi-layer mitigation documented; provider cooperation mandatory; receipt binding prevents forgery; A5 HIGH risk acknowledged + monitored |

## Next Steps

- Author RFC-0959 at `rfcs/draft/economics/0959-rfc-0909-amendment-ask-settlement.md` referencing this research — **DONE 2026-07-19** (Session 03, this research document).
- Cross-link from S03 session plan §2 (Decisions Locked) — **DONE**.
- Plan settlement engine + PricingAxis registry + cache classification in mission `missions/open/0959-a-ask-pricing-stoolap.md` — **DONE 2026-07-19** (S03).
- **Submit Research to Review Board:** per BLUEPRINT.md Research Review Gate ("Review by maintainers (min. 2 reviewers)"), circulate this research to maintainers for ≥ 2 reviewer approvals before considering it for Use Case promotion status. Reviewer signatures tracked in §Reviewer Signatures below.
- **Create Use Case? (Yes/No):** **YES** — `docs/use-cases/ai-quota-marketplace.md` already exists and this research is a deeper dive into one of its technical axes (per-node Ask pricing); promotion from research → use case is implicit (the use case pre-dates this research). NO new use case artifact needed; this research supports the existing use case with finer-grained rationale.

## Reviewer Signatures

Per BLUEPRINT.md Research Review Gate: "Review by maintainers (min. 2 reviewers)" required before research → use case promotion. Signatures pending review board circulation.

| Reviewer | Date | Decision | Notes |
|----------|------|----------|-------|
| (maintainer 1) | — | — | Pending |
| (maintainer 2) | — | — | Pending |

(Author note: the 2-reviewer signature requirement cannot be completed in-session without maintainer availability; research is ready for circulation. This documentation gap is acknowledged; the Sigil remains until external reviewer signature is collected.)

## Related Research

- `docs/research/ai-quota-marketplace-research.md` — feasibility for per-node Ask market (existing); does not cite RFC-0900 explicitly by line reference (R2 reviewer flagged cross-ref claim against line 153, which is a Token Economics table row, not an RFC-0900 cite; corrected.)
- `docs/research/litellm-analysis-and-quota-router-comparison.md` — LiteLLM pricing-axis patterns (100+ providers).
- `docs/research/bifrost-litellm-caching.md` — Bifrost cache-classification patterns.
- RFC-0909: Deterministic Quota Accounting — base settlement hash surface; v70 bump gated on RFC-0959 Accepted.
- RFC-0903: Virtual API Keys — cost-unit treatment reference (per RFC-0959 §Data Structures currency discussion). **Canonical path:** `rfcs/final/economics/0903-virtual-api-key-system.md` (Final-stage folder per BLUEPRINT convention; sibling drafts at `rfcs/accepted/economics/0903-B1-schema-amendments.md`, `rfcs/accepted/economics/0903-C1-extended-schema-amendments.md`, `rfcs/planned/economics/0903-C2-existing-deployment-migration.md`).
- RFC-0910: Pricing Table Registry — pricing-table consumer; canonical path `rfcs/accepted/economics/0910-pricing-table-registry.md`.
- RFC-0853: Overlay Cryptography — BLAKE3 primitive source. Canonical path: `rfcs/draft/networking/0853-overlay-cryptography.md` (Networking category per BLUEPRINT.md numbering; not Numeric).
- RFC-0009: Identity Management — NodeType taxonomy for `Ask.node_type` + capability bit for `ModelRef.namespace == "cipherocto"` gate. Canonical path: `rfcs/draft/process/0009-identity-management.md`.
- RFC-0957: Capability Token Format — AskBinding caveat host. Canonical path: `rfcs/draft/economics/0957-capability-token-format.md`.

## Research Quality Notes

- No empirical pricing-axis market data used (no production CipherOcto deployment yet). MVP axis selection mirrors OpenAI, Anthropic, LiteLLM public pricing models.
- Anti-fraud threshold (`MIN_PROMPT_DIVERSITY = 50`) is heuristic with no empirical derivation; production tuning required.
- Extension model boundary ("axis instance" vs "axis class") is editorial; clarified in RFC-0959 §Future Work F1 (streaming delay = axis-class, needs RFC revision) vs F2 (image/audio/fine-tuning = axis-instances, registry-only).
- BLAKE3 casing standardized: primitive reference = `BLAKE3` (uppercase, brand); crate import = `blake3` (lowercase, Rust crate); type alias `BLAKE3Hash = [u8; 32]` for the 256-bit output.
- Currency types: `OCTO_WAmount` (display, e.g., 1 for "1 OCTO-W") + `MicroOCTO_W` (on-wire, e.g., 1_000_000 for "1 OCTO-W"). Conversion at CLI ingress; compiler-enforced by type distinction.

**Research Status:** Complete. **Recommended Action:** Proceed to Use Case (already exists at `docs/use-cases/ai-quota-marketplace.md`); circulate to maintainer review board for 2-reviewer Research Review Gate signature collection (signature table above).
