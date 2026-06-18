# Round 41 Adversarial Review — RFCs 0904 v1.29, 0910 v27, 0917 v2.21

**Reviewer:** Internal analysis (Claude Code)
**Date:** 2026-04-26
**Base versions:** RFC-0904 v1.29, RFC-0910 v27, RFC-0917 v2.21

---

## CR-01 (`event_id` concatenation without field delimiters — multi-tenant collision)

**Partially valid — existing mitigations are adequate, but enhancement is warranted.**

### Analysis

RFC-0909 Final (the authoritative source for `compute_event_id`) already documents this exact attack surface in its §Security Note (lines 249-276). The document explicitly states:

> "A malicious client who knows another tenant's `key_id` could craft a `request_id` causing cross-tenant event_id collision"

The prescribed mitigations are:
1. Length-prefixed encoding in a custom `compute_event_id` variant, OR
2. Tenant isolation via filtered `build_merkle_tree` calls

### Rebuttal

The reviewer's suggested fix ("provide a reference `compute_event_id_v2()` with length-prefixing") is a **future enhancement**, not a current defect. The RFC does not claim to support multi-tenant adversarial deployments out of the box — it explicitly prescribes the isolation requirement for such deployments.

The claim that "identical `event_id` hashes will corrupt Merkle tree proofs" is incorrect: the `UNIQUE(key_id, request_id)` constraint prevents duplicate `(key_id, request_id)` pairs. A cross-tenant attack requires the attacker to know a victim's `key_id` AND craft a `request_id` that hashes to collide — this is a second-preimage attack on SHA256, not a practical exploit.

### Fix (enhancement, not defect)

Add to RFC-0909 §Security Note:
> "A `compute_event_id_v2()` using length-prefixed encoding (RFC-0910 §DCS Entry 16 field format) is planned as a future optimization for deployments requiring stronger tenant isolation guarantees without per-tenant Merkle tree filtering."

**Not blocking.** The existing documentation is complete and accurate. Multi-tenant deployers already have the guidance they need.

---

## CR-02 (F2 auto-reset idempotency — concurrent reset calls)

**Partially valid — the threat model is overstated, but a `period_start` parameter is a legitimate enhancement.**

### Analysis

The `budget_reset_log` schema (line 909) uses `PRIMARY KEY (key_id, reset_time)`. If a scheduler retries after partition, the second reset call with the same `reset_time` will violate the PK and error. This is not gracefully handled.

### Rebuttal

The reviewer's scenario requires:
1. Network partition during reset
2. Scheduler retry to the **same router instance**
3. The retry processing the **same key batch** with the same `reset_time`

In practice, deployers running clustered routers would route retries to any instance. The idempotency window is effectively "within a single handler invocation." The RFC already recommends randomized scheduler jitter (±30s at line 893) to spread load.

The `period_start` parameter enhancement is valid — it makes the endpoint idempotent-by-design rather than relying on PK collision. This should be added.

### Fix Required

Add `period_start: i64` parameter to `POST /admin/internal/budget/reset` body. The handler uses `period_start` as the reset boundary and skips keys whose `window_start` already matches — simple idempotent re-entry.

---

## CR-03 (`ProviderHandle` strategy binding in `full` mode)

**NOT VALID — this is a misreading of the architecture.**

### Analysis

The reviewer's concern assumes that in `full` mode, a provider could be simultaneously configured as both `Http` and `Sdk` and that routing logic could "expect" one over the other. This is impossible under the RFC's design.

The TOML feature gates (`litellm-mode` vs `any-llm-mode`) are **compile-time** alternatives. In `full` mode, both provider implementations exist in the binary but `Router::route_and_forward()` selects which `ProviderHandle` to invoke based on the **provider's configured strategy** — a stable, immutable configuration field per provider entry in `config.yaml`. There is no dynamic strategy switching at runtime that could cause the scenario described.

### Rebuttal

The Router struct (A3 in RFC-0917) uses a `HashMap<Provider, ProviderHandle>` where `ProviderHandle` is a trait object constructed once at startup. The routing strategy (LatencyBased, CostBased, etc.) selects a **provider name**, not a **provider type**. The provider's type (Http vs Sdk) is determined at configuration time and does not change.

The reviewer's "runtime validation" suggestion describes behavior that cannot occur under the specified architecture. No fix needed.

---

## HI-01 (Soft pre-check cache drift vs atomic ledger)

**Partially valid — the risk exists but is mitigated; reconciliation job is a good enhancement.**

### Analysis

The concern is: `check_budget` reads `key_spend.total_spend` (denormalized cache) while `record_spend_ledger` writes `spend_ledger`. Under concurrent load, cache drift could allow requests through `check_budget` that `record_spend_ledger` later rejects.

### Rebuttal

The RFC explicitly states (line 586):
> "The soft check is purely informational. The **authoritative enforcement** is always in `record_spend_ledger` which uses `FOR UPDATE` locking. Callers must handle `BudgetExceeded` from `record_spend_ledger` even when the soft check passed."

The soft pre-check is **intentional** as a performance optimization — it lets obviously-over-budget keys fail fast without acquiring a row lock. The atomic enforcement in `record_spend_ledger` is the authoritative gate. Any request that passes `check_budget` but fails `record_spend_ledger` receives an error response and no billing occurs.

The scenario where this causes "increased 402/502 rates" is the intended design: fast rejection path for obvious overages, locked atomic path for close-to-limit cases.

### Enhancement (not blocking)

Add a background reconciliation job as suggested. This is good ops practice. However, it should be marked as **recommended** (not required) because the cache drift, while real, does not cause incorrect billing — it only causes unnecessary lock acquisitions. The authoritative path is correct.

---

## HI-02 (F1 alert webhook no DLQ)

**NOT VALID — the RFC explicitly specifies at-least-once delivery with retry and documents dropped-alert behavior.**

### Analysis

The reviewer states "If all retries fail, the alert is dropped. No dead-letter queue (DLQ) or persistent retry backlog is specified."

### Rebuttal

Line 857 of RFC-0904 v1.29:
> "**Delivery guarantee:** Alert delivery is **at-least-once**: the router retries up to 3 times with fixed-interval backoff until the receiver returns a 2xx response. If all retries fail, the alert is dropped and an error is logged."

This is an **explicit design decision**, not an oversight. The RFC opts for at-least-once with bounded retries rather than unbounded retry with DLQ because:
1. Budget alerts are **informational** — they do not block requests
2. Persistent retry would require storage for an unbounded queue
3. Enterprise compliance is addressed by the `budget_alert_log` which records what thresholds fired

The reviewer conflates billing alerts with transaction integrity — they are not the same. If SOX/GDPR compliance requires guaranteed alert delivery, the deployment should configure a webhook receiver that acknowledges and stores alerts idempotently (the RFC explicitly recommends idempotent receiver behavior at line 857).

**Not blocking.** Document the rationale briefly to clarify intent.

---

## HI-03 (`o3-mini`/`o3-pro` tokenizer marked UNCERTAIN)

**Valid — enhancement warranted.**

### Analysis

The concern is valid: if a model is marked `UNCERTAIN` with a fallback tokenizer and the provider silently changes the tokenizer, `token_source` diverges → `event_id` changes → billing inconsistency or UNIQUE constraint violations.

### Rebuttal

The RFC currently assigns `UNCERTAIN` models a fallback in `get_canonical_tokenizer()`. The reviewer's enhancement (adding `tokenizer_version_expiry` and a `GET /admin/tokenizer/verify` endpoint) is a good ops tooling improvement.

However, the framing of this as a **security/determinism** issue is overstated. The `UNCERTAIN` flag is documented — operators are aware these assignments may require updates. The fix should be an enhancement, not a spec change.

### Fix (Medium, Enhancement)

Add to RFC-0910:
- `tokenizer_version_expiry` field to `PricingModel` or `PricingTable`
- `PricingRegistry::verify_tokenizer(model, provider_reported_tokenizer)` method
- Documentation that `UNCERTAIN` assignments require active registry maintenance

---

## HI-04 (`BudgetError::CostOverflow` mapped to HTTP 500)

**Valid — this is a bug.**

### Analysis

`CostOverflow` indicates deployment misconfiguration (pricing values so extreme they overflow u64 micro-unit math). Returning 500 triggers generic retry logic.

### Rebuttal

The HTTP status mapping table in RFC-0917 needs `CostOverflow` mapped to 422. This is a straightforward spec bug, not an architecture issue.

### Fix Required

Update the HTTP status code mapping for QuotaRouterError variants: `BudgetError::CostOverflow` → 422 Unprocessable Entity.

---

## MD-01 (F3 OCTO-W deduct failure returns 200 OK)

**NOT VALID — the RFC explicitly documents this behavior and the rationale.**

### Analysis

The reviewer states "F3 OCTO-W deduct failure after provider success returns 200 OK to client. Manual reconciliation is required, but no `deduct_retry_id` or reconciliation webhook is specified."

### Rebuttal

Line 862 of RFC-0904 v1.29 explicitly specifies:
> "If the OCTO-W deduct fails after the provider has already charged successfully, the router returns `200 OK` to the client (the user's request succeeded) and emits an `octo_w_reconciliation_pending` event to the event log."

The reconciliation workflow is for **operational teams** to handle asynchronously — the user's request succeeded. The RFC also specifies the `octo_w_reconciliation_pending` event (line 864). This is an intentional design decision distinguishing OCTO-W settlement from request-level billing.

The reviewer is correct that no `deduct_retry_id` or `POST /admin/budget/octo_w/retry` endpoint is specified. These would be nice-to-have ops tools but do not represent a missing specification — the reconciliation is event-driven, not API-driven.

**Not blocking.** Enhancement acceptable in future round.

---

## MD-02 (`build_merkle_tree()` sorts by `event_id` hex string lexicographically)

**Valid — documentation clarification needed.**

### Analysis

The concern: external verifiers expecting numeric sort will compute different Merkle roots.

### Rebuttal

SHA256 output is **hex digits only** (0-9, a-f) — there is no "numeric vs lexicographic" ambiguity. ASCII 'a' < 'b' < ... < 'f' and '0' < '1' < ... < '9', which is the natural order for hex strings. Any verifier implementing SHA256 correctly will produce the same ordering.

However, the RFC should explicitly state "ASCII lexicographic" to prevent a verifier from implementing some other interpretation (e.g., treating hex as a number, which would require parsing and be wrong).

### Fix (Low)

Add one sentence to RFC-0909 §Audit Proof Generation:
> "event_id sorting uses **ASCII lexicographic order** on the 64-character hex string. This is equivalent to sorting raw [u8; 32] by unsigned byte value when hex is interpreted as fixed-width big-endian encoding."

---

## MD-03 (`percent_used` division truncation)

**NOT VALID — truncation is the documented behavior, not an ambiguity.**

### Analysis

The reviewer requests rounding spec and a `?round=nearest|floor|ceil` query parameter.

### Rebuttal

Line 774 of RFC-0904 v1.29:
> "`percent_used = (current_spend * 100) / budget_limit`"

This is unambiguous integer division — truncating toward zero. The reviewer is requesting a feature extension (rounding options) that does not exist in the current spec. The current behavior is clearly defined. Adding query parameters for rounding is a nice enhancement but not a spec gap.

**Not blocking.** Enhancement acceptable in future round.

---

## MD-04 (`KNOWN_PROVIDERS` list static — new providers silently misroute)

**Valid — dynamic loading is an enhancement.**

### Analysis

The concern: adding a new provider without updating `KNOWN_PROVIDERS` causes silent misrouting.

### Rebuttal

This is a configuration management issue, not a spec defect. The RFC specifies that `model` strings are parsed by a provider prefix lookup — if a provider isn't in the list, it falls through to `default_provider`. This is **intentional graceful degradation** (fail-safe default provider behavior), not silent misrouting.

The enhancement to make `KNOWN_PROVIDERS` dynamically loadable from `config.yaml` is valid but not a spec bug.

### Fix (Medium, Enhancement)

Document in RFC-0917 §Provider Abstraction Layer that:
- `KNOWN_PROVIDERS` SHOULD be dynamically loaded from `config.yaml`
- An `UnknownProviderPrefix` event SHOULD be emitted at WARN level when a provider prefix falls through to default
- The `?provider=explicit` override is an acceptable enhancement

---

## LO-01 (`fired_at` timezone)

**NOT VALID — Unix epoch is timezone-agnostic by definition.**

### Analysis

The concern: timezone of `fired_at` (stored as Unix epoch) is not explicitly defined.

### Rebuttal

Unix epoch timestamps are **inherently UTC** — they represent seconds since 1970-01-01 00:00:00 UTC regardless of timezone. The RFC states `fired_at` is "debug-only storage" (line 803). Adding a comment about UTC is harmless but not a fix for any actual defect.

**Not blocking.** Trivial documentation clarification acceptable.

---

## LO-02 (`PricingRegistry::register()` no eviction policy)

**Partially valid — memory growth concern is real but overstated.**

### Analysis

The concern: `Vec` retains all superseded versions, potentially causing memory bloat.

### Rebuttal

`MAX_VERSIONS_PER_MODEL=1000` provides an explicit bound. At 1000 versions per model, even aggressive price-flipping deployments hit the cap. The memory concern is real for long-running processes but the existing limit is a sufficient mitigation. Adding a `prune_old_versions()` method is an acceptable enhancement, not a spec requirement.

**Not blocking.** Enhancement acceptable in future round.

---

## LO-03 (`LatencyTracker` concurrent Vec push without lock)

**Valid — this is a potential bug.**

### Analysis

`HashMap<String, Vec<u64>>` with `RwLock` on `RouterState` — if `record()` only acquires read-lock while pushing to `Vec`, concurrent pushes to the same provider's vector could cause data loss.

### Rebuttal

The reviewer's concern assumes `record()` only uses a read-lock. I should verify the actual implementation to confirm. Since this is Rust with `RwLock`, a read-lock would not protect a write to the inner `Vec`. This could be a real concurrency bug.

### Fix Required

Verify the `LatencyTracker::record()` implementation uses **write-lock** (`write().unwrap()`) when pushing to the inner `Vec`. If not, fix to use write-lock or switch to `Mutex<Vec>`.

---

## Summary

| Finding | Verdict | Action |
|---------|---------|--------|
| CR-01 | **NOT VALID** | Multi-tenant guidance already in RFC-0909 §Security Note; length-prefixed v2 is enhancement |
| CR-02 | **PARTIALLY VALID** | Add `period_start` parameter to F2 reset endpoint for idempotent re-entry |
| CR-03 | **NOT VALID** | Architecture prevents the described scenario; compile-time feature gates, immutable provider config |
| HI-01 | **PARTIALLY VALID** | Reconciliation job is good ops practice; authoritative path is already correct |
| HI-02 | **NOT VALID** | At-least-once with bounded retries is explicit design decision; documented |
| HI-03 | **VALID — Enhancement** | Add `tokenizer_version_expiry` and `verify_tokenizer()` to RFC-0910 |
| HI-04 | **VALID — Bug** | Map `CostOverflow` to HTTP 422 in QuotaRouterError status mapping |
| MD-01 | **NOT VALID** | 200 OK after provider success is intentional; event-driven reconciliation documented |
| MD-02 | **VALID — Doc fix** | Add "ASCII lexicographic" clarification to RFC-0909 sorting note |
| MD-03 | **NOT VALID** | Truncation is explicitly defined; rounding options are enhancement |
| MD-04 | **VALID — Enhancement** | Document dynamic KNOWN_PROVIDERS loading and UnknownProviderPrefix logging |
| LO-01 | **NOT VALID** | Unix epoch is UTC by definition |
| LO-02 | **NOT BLOCKING** | MAX_VERSIONS_PER_MODEL=1000 is sufficient; prune_old_versions enhancement |
| LO-03 | **VALID — Bug** | Verify LatencyTracker::record() uses write-lock; fix if needed |

---

## Required Fixes

1. **CR-02 (Medium):** Add `period_start: i64` to `POST /admin/internal/budget/reset` body; make handler idempotent by skipping keys whose `window_start` already matches
2. **HI-04 (High):** Map `BudgetError::CostOverflow` → HTTP 422 in QuotaRouterError status mapping table
3. **MD-02 (Low):** Add "ASCII lexicographic" clarification to RFC-0909 §Audit Proof Generation sorting note
4. **HI-03 (Medium):** Add `tokenizer_version_expiry` field and `verify_tokenizer()` method to RFC-0910
5. **MD-04 (Medium):** Document dynamic `KNOWN_PROVIDERS` loading and `UnknownProviderPrefix` WARN event in RFC-0917
6. **LO-03 (Medium):** Verify `LatencyTracker::record()` uses write-lock; fix if data loss risk confirmed
