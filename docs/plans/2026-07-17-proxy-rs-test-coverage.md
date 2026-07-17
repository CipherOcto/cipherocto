# Router Part Coverage — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Lift router-part coverage on `quota-router-core` from proxy.rs 87.07% / router.rs 73.66% by adding 10 targeted tests covering ~110 uncovered lines in the biggest ROI branches.

**Architecture:** Coverage-gap tests, not feature work. All target functions exist and work; tests verify behavior is locked in. TDD adapted: per superpowers:test-driven-development, "watch it fail" is awkward for existing-behavior regression tests — we verify each new test FAILS when the relevant line is briefly stubbed out (sentinel assertion), then restore. For env-touching tests we use std::env::set_var + remove (matches existing pattern in proxy.rs:2987-3014).

**Tech Stack:** Rust 1.x, cargo, inline `#[cfg(test)] mod tests` for router.rs + proxy.rs.

**Build/test command** (use `--features litellm-mode` to enable proxy.rs `mod tests` block):

```bash
cd crates/quota-router-core
cargo test --lib --features litellm-mode <test_name> -- --nocapture
cargo fmt && git add -A && git commit -m "<msg>"
```

---

## Task 1: `test_routing_strategy_display` (router.rs)

**Files:**
- Modify: `crates/quota-router-core/src/router.rs` — append to `#[cfg(test)] mod tests` block (currently ends ~line 1700).
- Test: same file, inline.

**Coverage target:** lines 49-58 (Display impl, 10 lines).

**Step 1: Add the test** — append at end of `mod tests`:

```rust
    #[test]
    fn test_routing_strategy_display() {
        use std::str::FromStr;
        assert_eq!(RoutingStrategy::SimpleShuffle.to_string(), "simple-shuffle");
        assert_eq!(RoutingStrategy::RoundRobin.to_string(), "round-robin");
        assert_eq!(RoutingStrategy::LeastBusy.to_string(), "least-busy");
        assert_eq!(RoutingStrategy::LatencyBased.to_string(), "latency-based");
        assert_eq!(RoutingStrategy::CostBased.to_string(), "cost-based");
        assert_eq!(RoutingStrategy::UsageBased.to_string(), "usage-based");
        assert_eq!(RoutingStrategy::UsageBasedV2.to_string(), "usage-based-v2");
        assert_eq!(RoutingStrategy::Weighted.to_string(), "weighted");
        // Round-trip Display -> FromStr
        for s in [
            RoutingStrategy::SimpleShuffle,
            RoutingStrategy::RoundRobin,
            RoutingStrategy::LeastBusy,
            RoutingStrategy::LatencyBased,
            RoutingStrategy::CostBased,
            RoutingStrategy::UsageBased,
            RoutingStrategy::UsageBasedV2,
            RoutingStrategy::Weighted,
        ] {
            assert_eq!(RoutingStrategy::from_str(&s.to_string()).unwrap(), s);
        }
    }
```

**Step 2: Run.**

```bash
cargo test --lib test_routing_strategy_display -- --nocapture
```

Expected: PASS (existing Display impl works).

**Step 3: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): RoutingStrategy Display impl + FromStr round-trip"
```

---

## Task 2: `test_from_str_unknown_strategy_error` (router.rs)

**Files:**
- Modify: `crates/quota-router-core/src/router.rs` — same `mod tests`.

**Coverage target:** lines 77, 86 (FromStr Err arm + the closure-execution path that constructs `Err(format!(...))`).

**Step 1: Add the test:**

```rust
    #[test]
    fn test_from_str_unknown_strategy_error() {
        use std::str::FromStr;
        let err = RoutingStrategy::from_str("nonsense-strategy").unwrap_err();
        assert!(err.contains("Unknown routing strategy"));
        assert!(err.contains("nonsense-strategy"));
        // empty string
        assert!(RoutingStrategy::from_str("").is_err());
    }
```

**Step 2: Run.**

```bash
cargo test --lib test_from_str_unknown_strategy_error -- --nocapture
```

Expected: PASS.

**Step 3: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): RoutingStrategy::from_str unknown-input error path"
```

---

## Task 3: `test_best_provider_with_penalties` (router.rs)

**Files:**
- Modify: `crates/quota-router-core/src/router.rs` — same `mod tests`.

**Coverage target:** lines 574-634 — `best_provider_with_penalties` body (27 lines).

**Step 1: Inspect the LatencyTracker test surface** (read router.rs:418-500 for `LatencyTracker::record` and `best_provider_among` shape). Construct a `LatencyTracker` directly, record samples + penalties, then call the method.

**Step 2: Add the test:**

```rust
    #[test]
    fn test_best_provider_with_penalties_prefers_low_penalty() {
        let mut tracker = LatencyTracker::default();

        // Baseline samples — azure is faster than openai
        for _ in 0..3 {
            tracker.record("azure", 100_000, None);
            tracker.record("openai", 500_000, None);
        }

        // No penalties — azure wins on raw latency
        let avail: std::collections::HashSet<&str> =
            ["azure", "openai"].into_iter().collect();
        let empty_penalties = std::collections::HashMap::new();
        let (name, _score) = tracker
            .best_provider_with_penalties(&empty_penalties, &avail, false)
            .expect("should select a provider");
        assert_eq!(name, "azure");

        // Heavy penalty on azure — should still pick azure (penalty only adjusts score, not availability)
        // unless penalty-adjusted score exceeds openai's baseline
        let mut penalties: std::collections::HashMap<String, Vec<u64>> =
            std::collections::HashMap::new();
        penalties.insert("azure".to_string(), vec![10_000_000]); // 10s penalty
        let (name2, _) = tracker
            .best_provider_with_penalties(&penalties, &avail, false)
            .expect("should still select something");
        // With azure baseline 100ms + 10s penalty, openai (500ms no penalty) wins
        assert_eq!(name2, "openai");

        // No available providers matching recorded samples
        let only_garbage: std::collections::HashSet<&str> = ["nonexistent"].into_iter().collect();
        assert!(tracker
            .best_provider_with_penalties(&empty_penalties, &only_garbage, false)
            .is_none());
    }
```

**Step 3: Run.**

```bash
cargo test --lib test_best_provider_with_penalties_prefers_low_penalty -- --nocapture
```

Expected: PASS (verifies penalty-adjusted selection logic + None return for unavailable).

**Step 4: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): LatencyTracker::best_provider_with_penalties scoring"
```

---

## Task 4: `test_latency_based_routing_no_available_providers` (router.rs)

**Files:**
- Modify: `crates/quota-router-core/src/router.rs` — same `mod tests`.

**Coverage target:** lines 1112-1115 + 1140-1142 + 1150-1155 (cooldown exit + no-available + penalty branch in `latency_based_with_cooldown_impl`).

**Step 1: Add the test:**

```rust
    #[test]
    fn test_latency_based_routing_no_available_providers_returns_none() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::LatencyBased,
            ..Default::default()
        };
        let mut router = Router::new(config, providers);

        // Put BOTH providers into cooldown so none are available
        for p in router.providers.get_mut("gpt-3.5-turbo").unwrap().iter_mut() {
            p.cooldown_tracker.enter_cooldown(3600);
        }

        // Should return None — no provider is available
        assert!(router.route("gpt-3.5-turbo", false).is_none());
    }
```

**Step 2: Run.**

```bash
cargo test --lib test_latency_based_routing_no_available_providers_returns_none -- --nocapture
```

Expected: PASS.

**Step 3: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): LatencyBased routing returns None when all in cooldown"
```

---

## Task 5: `test_usage_based_v2_routing_prefers_low_rpm_high_success` (router.rs)

**Files:**
- Modify: `crates/quota-router-core/src/router.rs` — same `mod tests`.

**Coverage target:** lines 1189-1205 — `usage_based_v2_impl` body (7 lines).

**Step 1: Add the test:**

```rust
    #[test]
    fn test_usage_based_v2_routing_prefers_low_rpm_high_success() {
        let providers = test_providers();
        let config = RouterConfig {
            routing_strategy: RoutingStrategy::UsageBasedV2,
            ..Default::default()
        };
        let mut router = Router::new(config, providers);

        // azure: low RPM + high success rate → should be preferred
        // openai: high RPM + low success rate → should be avoided
        if let Some(list) = router.providers.get_mut("gpt-3.5-turbo") {
            for p in list.iter_mut() {
                if p.provider.name == "azure" {
                    p.current_rpm = 10;
                    p.success_count = 95;
                    p.total_count = 100; // 95% success
                } else {
                    p.current_rpm = 500;
                    p.success_count = 50;
                    p.total_count = 100; // 50% success
                }
            }
        }

        let idx = router.route("gpt-3.5-turbo", false).unwrap();
        let name = &router.get_provider("gpt-3.5-turbo", idx).unwrap().provider.name;
        assert_eq!(name, "azure");
    }
```

**Step 2: Run.**

```bash
cargo test --lib test_usage_based_v2_routing_prefers_low_rpm_high_success -- --nocapture
```

Expected: PASS.

**Step 3: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): UsageBasedV2 routing score = RPM * (100 - success_rate) / 100"
```

---

## Task 6: `test_evict_old_buckets_for_removes_expired` (router.rs)

**Files:**
- Modify: `crates/quota-router-core/src/router.rs` — same `mod tests`.

**Coverage target:** lines 778-790 — `evict_old_buckets_for` body (11 lines).

**Step 1: Find the `record` method on the bucket-tracker struct** (around router.rs:770, the caller of `evict_old_buckets_for`). The struct holds `buckets: HashMap<String, HashMap<String, u64>>` + `bucket_timestamps`. Use the existing `record(deployment_id, tokens)` method to seed buckets, then call `evict_old_buckets_for` with a short TTL.

**Step 2: Add the test** (verify exact field/method names by reading router.rs:760-800 first):

```rust
    #[test]
    fn test_evict_old_buckets_for_removes_expired() {
        // Find the bucket tracker (the struct holding buckets + bucket_timestamps).
        // Construct one, record a token sample for a deployment, wait briefly,
        // then evict with a 0-second TTL — bucket should disappear.
        //
        // Replace `BucketTracker` with the actual struct name from router.rs (search
        // for `bucket_timestamps` to find the type).
        use std::time::Duration;

        let mut tracker = /* BucketTracker::default() — see router.rs:760-795 for type */;
        tracker.ttl_seconds = 0;
        tracker.record("dep-1", 100);
        // Sanity: bucket exists
        assert!(tracker.buckets.contains_key("dep-1"));

        // Evict with ttl=0 — Instant::now() - created > 0s for any non-instant bucket
        tracker.evict_old_buckets_for("dep-1");
        assert!(!tracker.buckets.contains_key("dep-1"));
        assert!(!tracker.bucket_timestamps.contains_key("dep-1"));
    }
```

**NOTE**: If `BucketTracker` and its fields are not public, find the existing public constructor and the `record` method signature first. The exact struct/method names must be confirmed against router.rs:760-800 before writing this test. If fields are private, add a `#[cfg(test)] pub` shim or test through the public `record()` + a sibling helper that already exists.

**Step 3: Run.**

```bash
cargo test --lib test_evict_old_buckets_for_removes_expired -- --nocapture
```

Expected: PASS (after reconciling actual API).

**Step 4: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): evict_old_buckets_for removes stale per-deployment buckets"
```

---

## Task 7: `test_rolling_avg_tpm` (router.rs)

**Files:**
- Modify: `crates/quota-router-core/src/router.rs` — same `mod tests`.

**Coverage target:** lines 891-917 — `rolling_avg_tpm` body (15 lines: `cutoff` filter, `recent` collection, average).

**Step 1: Reuse the BucketTracker from Task 6. Add the test:**

```rust
    #[test]
    fn test_rolling_avg_tpm_averages_recent_minutes() {
        let mut tracker = /* same type as Task 6 */;
        tracker.ttl_seconds = 3600;

        // Record a sample
        tracker.record("dep-1", 100);
        // rolling_avg_tpm should return Some(non-zero)
        let avg = tracker.rolling_avg_tpm("dep-1", 5).expect("should compute avg");
        assert!(avg > 0.0);

        // Unknown deployment → None
        assert!(tracker.rolling_avg_tpm("nonexistent", 5).is_none());
    }
```

**Step 2: Run.**

```bash
cargo test --lib test_rolling_avg_tpm_averages_recent_minutes -- --nocapture
```

Expected: PASS.

**Step 3: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): rolling_avg_tpm averages recent minutes, None for unknown"
```

---

## Task 8: `test_reset_all_usage_and_reset_usage` (router.rs)

**Files:**
- Modify: `crates/quota-router-core/src/router.rs` — same `mod tests`.

**Coverage target:** lines 207-209 (`reset_usage`) + 1288-1291 (`reset_all_usage`).

**Step 1: Locate the two functions** (search for `pub fn reset_usage` + `pub fn reset_all_usage`). Confirm they live on the same `ProviderWithState` (or whichever struct holds `current_rpm`/`success_count`).

**Step 2: Add the test:**

```rust
    #[test]
    fn test_reset_usage_and_reset_all_usage_zero_counters() {
        let providers = test_providers();
        let mut router = Router::new(RouterConfig::default(), providers);

        // Seed non-zero counters on every provider of gpt-3.5-turbo
        if let Some(list) = router.providers.get_mut("gpt-3.5-turbo") {
            for p in list.iter_mut() {
                p.current_rpm = 999;
                p.success_count = 50;
                p.total_count = 100;
            }
        }

        // reset_usage on first provider — only that one zeroes
        if let Some(p) = router.get_provider("gpt-3.5-turbo", 0) {
            p.reset_usage();
        }
        let first = router.get_provider("gpt-3.5-turbo", 0).unwrap();
        assert_eq!(first.current_rpm, 0);
        assert_eq!(first.success_count, 0);
        assert_eq!(first.total_count, 0);

        // reset_all_usage — every provider zeroes
        if let Some(list) = router.providers.get_mut("gpt-3.5-turbo") {
            for p in list.iter_mut() {
                p.current_rpm = 999;
                p.success_count = 50;
                p.total_count = 100;
            }
        }
        // reset_all_usage lives on Router (find the actual call site — may be Router::reset_all_usage,
        // or a free function, or on the Router itself). Adjust invocation per actual signature.
        /* router.reset_all_usage_or_equivalent(); */

        for p in router.providers.get("gpt-3.5-turbo").unwrap().iter() {
            assert_eq!(p.current_rpm, 0);
            assert_eq!(p.success_count, 0);
            assert_eq!(p.total_count, 0);
        }
    }
```

**NOTE**: `reset_all_usage` may live on the Router struct, not `ProviderWithState`. Read router.rs:1285-1295 to confirm signature before writing the call. Adjust as needed.

**Step 3: Run + commit.**

```bash
cargo test --lib test_reset_usage_and_reset_all_usage_zero_counters -- --nocapture
cargo fmt && git add -A && git commit -m "test(quota-router-core): reset_usage + reset_all_usage zero RPM/success/total counters"
```

---

## Task 9: `test_resolve_api_key_uses_any_llm_key` (proxy.rs)

**Files:**
- Modify: `crates/quota-router-core/src/proxy.rs` — append to the `#[cfg(any(feature = "litellm-mode", feature = "full"))] #[cfg(test)] mod tests` block at line ~2960.

**Coverage target:** `resolve_api_key` priority-2 branch (line 541-548, `ANY_LLM_KEY` env var lookup + warn log).

**Step 1: Add the test:**

```rust
    #[test]
    fn test_resolve_api_key_uses_any_llm_key_fallback() {
        // Provider-specific env var absent, ANY_LLM_KEY set → ANY_LLM_KEY wins (priority 2)
        std::env::remove_var("ANYLLMPROV4_API_KEY");
        std::env::set_var("ANY_LLM_KEY", "universal-key");
        let provider = Provider::new("anyllmprov4", "https://example.com");

        let resolved = resolve_api_key(&provider, None);
        assert_eq!(resolved, Some("universal-key".to_string()));

        std::env::remove_var("ANY_LLM_KEY");
    }
```

**Step 2: Run** (must include feature flag to compile the inline tests):

```bash
cargo test --lib --features litellm-mode test_resolve_api_key_uses_any_llm_key_fallback -- --nocapture
```

Expected: PASS.

**Step 3: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): resolve_api_key priority-2 ANY_LLM_KEY fallback"
```

---

## Task 10: `test_classify_http_error_mapping` (proxy.rs)

**Files:**
- Modify: `crates/quota-router-core/src/proxy.rs` — same `mod tests` block.

**Coverage target:** `classify_http_error` body at lines 2877-2885 (9 lines).

**Step 1: Add the test:**

```rust
    #[test]
    fn test_classify_http_error_mapping() {
        use http::StatusCode;
        use crate::fallback::RouterError;
        assert!(matches!(classify_http_error(StatusCode::TOO_MANY_REQUESTS), RouterError::RateLimit));
        assert!(matches!(classify_http_error(StatusCode::SERVICE_UNAVAILABLE), RouterError::ProviderUnavailable));
        assert!(matches!(classify_http_error(StatusCode::UNAUTHORIZED), RouterError::AuthError));
        assert!(matches!(classify_http_error(StatusCode::FORBIDDEN), RouterError::AuthError));
        assert!(matches!(classify_http_error(StatusCode::REQUEST_TIMEOUT), RouterError::Timeout));
        assert!(matches!(classify_http_error(StatusCode::GATEWAY_TIMEOUT), RouterError::Timeout));
        assert!(matches!(classify_http_error(StatusCode::INTERNAL_SERVER_ERROR), RouterError::Unknown));
        assert!(matches!(classify_http_error(StatusCode::BAD_REQUEST), RouterError::Unknown));
    }
```

If `http` is not a direct dep, use `hyper::StatusCode` instead — confirm by reading proxy.rs imports near line 1.

**Step 2: Run.**

```bash
cargo test --lib --features litellm-mode test_classify_http_error_mapping -- --nocapture
```

Expected: PASS.

**Step 3: Commit.**

```bash
cargo fmt && git add -A && git commit -m "test(quota-router-core): classify_http_error status-code → RouterError mapping"
```

---

## Final verification

```bash
cd crates/quota-router-core
cargo test --lib --features litellm-mode 2>&1 | tail -5
cargo clippy --all-targets --features litellm-mode -- -D warnings 2>&1 | tail -5
```

Expected: all tests pass, clippy clean. Re-run coverage with `cargo llvm-cov` and verify proxy.rs line-rate + router.rs line-rate each move up at least ~3-5 points.