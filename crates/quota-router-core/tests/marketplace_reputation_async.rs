//! Dual-read parity tests for the RFC-0968 retirement gate
//! (mission `marketplace-facade-reputation-async-migration`).
//!
//! Exercises the legacy `record_outcome` / `provider_score` (sync, in
//! `scoring::ProviderReputationRegistry`) and the new
//! `record_outcome_async` / `read_reputation_async` (async, through
//! `octo_reputation::ReputationStore`) surfaces.
//!
//! The two paths are structurally different, and the difference is large:
//! legacy maintains a decayed `f64` EWMA over `success_rate`, while compat
//! exposes a plain success **ratio** (`successes / total`) via its `Dfp`
//! per-signal `score_ewma`. Measured divergence on identical mixed input is
//! therefore not a rounding margin — see
//! `dual_read_mixed_outcome_divergence_within_measured_bound`, which pins the
//! worst observed case at Δ ≈ 0.357.
//!
//! At the extremes (all-success, all-failure) both paths saturate to the same
//! value by construction, so Δ = 0 identically. Those two fixtures are kept —
//! saturation agreement is a real property — but they are not evidence of
//! parity. This header previously claimed a `0.999` parity tolerance and a
//! `Δ ≤ 0.10` bound that the code never enforced (it asserted `Δ ≤ 0.5`, on
//! fixtures where Δ is structurally zero). See mission
//! `dual-read-parity-asserts-a-divergence-bound-it-never-measures`.

use octo_ident::test_helpers::sample_did;
use quota_router_core::marketplace::Marketplace;

/// Synthetic 24h fixture: alternating success/failure pattern of 10
/// records. The controller_id is derived from a fixed governance
/// pubkey per RFC-0968-A1 amendment 44 — using a non-zero pubkey keeps
/// the compat `record_with_now` happy.
const GOV_PUBKEY: [u8; 32] = [0xAB; 32];

#[tokio::test]
async fn dual_read_parity_success_path_within_tolerance() {
    let m = Marketplace::open_in_memory().expect("open_in_memory");
    let did = sample_did(7);

    // 10 successful records via both paths, interleaved.
    for i in 0..10u64 {
        m.record_outcome(&did, true, 50);
        let now = 1_700_000_000 + i * 60;
        m.record_outcome_async(&did, true, 50, blake3_runtime(GOV_PUBKEY), now)
            .await
            .expect("async record");
    }

    // Legacy read.
    let legacy = m
        .provider_score(&did)
        .expect("legacy score present after 10 records");
    // Compat read.
    let compat = m.read_reputation_async(&did).await.expect("compat read");

    // All 10 records succeeded. Both paths saturate at the top: the
    // legacy EWMA pins to 1.0 and the compat ratio is 10/10 = 1.0.
    // Saturation agreement is a real property and is asserted exactly —
    // it is NOT evidence of parity, because the two formulas diverge on
    // every non-saturated input (see the mixed fixture below).
    let delta = (legacy.success_rate - compat.success_rate).abs();
    assert!(
        delta == 0.0,
        "saturation agreement broken: legacy={} compat={} delta={}",
        legacy.success_rate,
        compat.success_rate,
        delta
    );
    assert_eq!(legacy.success_rate, 1.0, "legacy saturates at top");
    assert_eq!(compat.success_rate, 1.0, "compat ratio saturates at top");
}

#[tokio::test]
async fn dual_read_parity_failure_path_within_tolerance() {
    let m = Marketplace::open_in_memory().expect("open_in_memory");
    let did = sample_did(11);

    // 10 failed records via both paths.
    for i in 0..10u64 {
        m.record_outcome(&did, false, 200);
        let now = 1_700_000_000 + i * 60;
        m.record_outcome_async(&did, false, 200, blake3_runtime(GOV_PUBKEY), now)
            .await
            .expect("async record");
    }

    let legacy = m.provider_score(&did).expect("legacy score");
    let compat = m.read_reputation_async(&did).await.expect("compat");

    // All 10 failed. Both paths saturate at the bottom: legacy EWMA
    // pins to 0.0 and the compat ratio is 0/10 = 0.0. Asserted exactly,
    // for the same reason as the success fixture above — saturation
    // agreement, not parity.
    let delta = (legacy.success_rate - compat.success_rate).abs();
    assert!(
        delta == 0.0,
        "saturation agreement broken: legacy={} compat={} delta={}",
        legacy.success_rate,
        compat.success_rate,
        delta
    );
    assert_eq!(legacy.success_rate, 0.0, "legacy saturates at bottom");
    assert_eq!(compat.success_rate, 0.0, "compat ratio saturates at bottom");
}

#[tokio::test]
async fn dual_read_parity_unknown_did_returns_perfect_reputation() {
    let m = Marketplace::open_in_memory().expect("open_in_memory");
    let did = sample_did(42);

    // No records at all. Both paths should return a "perfect" reading
    // (legacy: Some(score with success_rate=1.0); compat: success_rate
    // = 1.0 from the empty-aggregate fallback).
    let legacy = m.provider_score(&did);
    let compat = m.read_reputation_async(&did).await.expect("compat");

    // Legacy registry returns Some even for unknown DIDs (with the
    // perfect default); compat returns a fresh ProviderScore populated
    // from the empty-aggregate fallback.
    assert_eq!(compat.success_rate, 1.0, "compat unknown = perfect");
    assert_eq!(compat.samples, 0, "compat unknown = 0 samples");
    if let Some(legacy) = legacy {
        assert_eq!(legacy.success_rate, 1.0, "legacy unknown = perfect");
    }
}

#[tokio::test]
async fn all_zero_controller_id_rejected() {
    // Per RFC-0968-A1 amendment 40: an all-zero controller_id is
    // reserved (never produced by the governance-pubkey derivation).
    // The compat rejects it via the canonical
    // `ReputationError::ControllerIdMissing` (discriminant `0x34`,
    // mission `octo-reputation-controller-id-missing-variant`).
    use octo_reputation::error::ReputationError;
    let m = Marketplace::open_in_memory().expect("open_in_memory");
    let did = sample_did(99);
    let err = m
        .record_outcome_async(&did, true, 50, [0u8; 32], 1_700_000_000)
        .await
        .unwrap_err();
    // Canonical assertion: the compat now returns the dedicated variant.
    assert!(
        matches!(err, ReputationError::ControllerIdMissing),
        "expected ControllerIdMissing, got: {err:?}"
    );
    assert_eq!(err.discriminant(), 0x34, "discriminant must be 0x34");
}

/// The divergence that actually exists.
///
/// The two saturation fixtures above pin Δ = 0 because both paths land on the
/// same endpoint. This drives identical *mixed* records into both paths, where
/// the legacy decayed EWMA and the compat ratio genuinely separate, and pins
/// the relationship the production code actually exhibits:
///
/// - compat `success_rate` is the plain ratio `successes / total`
/// - legacy lags it (the EWMA has not converged after 10 samples)
/// - worst observed gap is Δ ≈ 0.357 at 7/10 successes
///
/// Measured table (successes out of 10):
///   8 → 0.490 / 0.800   Δ 0.310
///   7 → 0.343 / 0.700   Δ 0.357   <- worst case
///   5 → 0.168 / 0.500   Δ 0.332
///   3 → 0.082 / 0.300   Δ 0.218
///   1 → 0.040 / 0.100   Δ 0.060
///
/// `DIVERGENCE_CEILING` sits above the measured worst case with headroom and
/// well below the old `Δ ≤ 0.5` bound, so drift in either formula trips this
/// rather than sliding through.
#[tokio::test]
async fn dual_read_mixed_outcome_divergence_within_measured_bound() {
    /// Worst measured Δ is 0.357 (7/10 successes); 0.40 leaves headroom
    /// without admitting the old 0.5 slack.
    const DIVERGENCE_CEILING: f64 = 0.40;

    let controller = blake3_runtime(GOV_PUBKEY);
    let mut worst: f64 = 0.0;

    for successes in [8u8, 7, 5, 3, 1] {
        let m = Marketplace::open_in_memory().expect("open_in_memory");
        let did = sample_did(100u8.wrapping_add(successes));

        for i in 0..10u64 {
            let ok = (i as u8) < successes;
            m.record_outcome(&did, ok, 100);
            m.record_outcome_async(&did, ok, 100, controller, 1_700_000_000 + i * 60)
                .await
                .expect("async record");
        }

        let legacy = m.provider_score(&did).expect("legacy score");
        let compat = m.read_reputation_async(&did).await.expect("compat");

        assert_eq!(legacy.samples, 10, "legacy saw every record");
        assert_eq!(compat.samples, 10, "compat saw every record");

        // compat exposes the plain success ratio, not an EWMA.
        let ratio = f64::from(successes) / 10.0;
        assert!(
            (compat.success_rate - ratio).abs() < 1e-9,
            "compat is the success ratio: successes={} expected={} got={}",
            successes,
            ratio,
            compat.success_rate
        );

        // The legacy EWMA lags the ratio on every non-saturated input.
        assert!(
            legacy.success_rate < compat.success_rate,
            "legacy EWMA should lag compat ratio: successes={} legacy={} compat={}",
            successes,
            legacy.success_rate,
            compat.success_rate
        );

        let delta = (legacy.success_rate - compat.success_rate).abs();
        worst = worst.max(delta);
        assert!(
            delta <= DIVERGENCE_CEILING,
            "mixed divergence exceeds ceiling: successes={} legacy={} compat={} delta={} ceiling={}",
            successes,
            legacy.success_rate,
            compat.success_rate,
            delta,
            DIVERGENCE_CEILING
        );
    }

    // Guards the guard: if the two paths ever converge, this test stops
    // measuring anything and would pass vacuously.
    assert!(
        worst > 0.30,
        "expected real divergence across the mixed fixtures, saw worst={}",
        worst
    );
}

fn blake3_runtime(pubkey: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&pubkey);
    let out = hasher.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(out.as_bytes());
    id
}

/// Current unix seconds — used by the dual-read comparison tests to
/// seed ask `expires_at_unix` so the asks land in the in-memory book.
fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Build an `Ask` for integration-test fixtures. Mirrors the
/// `sample_ask` helper inside `mod.rs::tests` (not reachable from
/// here). Uses `nonce = [0x42; 16]` and a single axis at `rate_per_1k`
/// to keep settlement math identical across providers.
fn make_ask(
    asker: &str,
    model: &str,
    rate_per_1k: u128,
    expires: u64,
) -> quota_router_storage::ask::Ask {
    use quota_router_storage::ask::{Ask, AxisRate, ModelRateTable, ModelRef};
    Ask {
        asker_did: asker.to_owned(),
        model: ModelRef::from(model),
        rates: ModelRateTable {
            model: ModelRef::from(model),
            rates: vec![AxisRate {
                axis: "input_tokens_per_1k".to_owned(),
                rate_per_1k: octo_determin::Dqa::new(
                    i64::try_from(rate_per_1k).expect("rate fits in i64"),
                    0,
                )
                .expect("non-overflow"),
            }],
        },
        nonce: [0x42; 16],
        expires_at_unix: expires,
    }
}

#[tokio::test]
async fn dual_read_ranking_agrees_on_success_failure_mix() {
    // Mission `marketplace-cheapest-with-ranking-async` AC: ≥3 records,
    // success + failure mix, both `cheapest_with_ranking` and the new
    // `cheapest_with_ranking_async` return the same `MarketplaceEntry`
    // ordering. The compat path is seeded via `record_outcome_async`
    // (canonical surface); the legacy shadow is seeded via
    // `set_provider_score` to identical values. Both paths must
    // surface the same winner on `cheapest_with_ranking_default`.
    use quota_router_core::marketplace::scoring::LatencyRanking;
    use quota_router_core::marketplace::scoring::ProviderScore;

    let m = Marketplace::open_in_memory().expect("open_in_memory");
    let now = now_unix();

    // 3 asks at ascending prices; model gpt-4.
    let cheap = sample_did(201);
    let mid = sample_did(202);
    let expensive = sample_did(203);
    let expires = now + 3600;
    m.put(&make_ask(&cheap, "openai/gpt-4", 10_000, expires))
        .unwrap();
    m.put(&make_ask(&mid, "openai/gpt-4", 30_000, expires))
        .unwrap();
    m.put(&make_ask(&expensive, "openai/gpt-4", 50_000, expires))
        .unwrap();

    // Seed BOTH surfaces with the same scores. Cheap + mid: high
    // success. Expensive: low success.
    let controller = blake3_runtime(GOV_PUBKEY);
    // Cheap: 8 successes, 2 failures → high EWMA.
    for i in 0..10u64 {
        let success = i < 8;
        m.record_outcome(&cheap, success, 100);
        m.record_outcome_async(&cheap, success, 100, controller, now + i * 60)
            .await
            .expect("async record cheap");
    }
    m.set_provider_score(ProviderScore {
        asker_did: cheap.clone(),
        success_rate: 0.8,
        latency_ms: 100,
        samples: 10,
    });
    // Mid: 7 successes, 3 failures.
    for i in 0..10u64 {
        let success = i < 7;
        m.record_outcome(&mid, success, 150);
        m.record_outcome_async(&mid, success, 150, controller, now + i * 60)
            .await
            .expect("async record mid");
    }
    m.set_provider_score(ProviderScore {
        asker_did: mid.clone(),
        success_rate: 0.7,
        latency_ms: 150,
        samples: 10,
    });
    // Expensive: 3 successes, 7 failures → low EWMA.
    for i in 0..10u64 {
        let success = i < 3;
        m.record_outcome(&expensive, success, 300);
        m.record_outcome_async(&expensive, success, 300, controller, now + i * 60)
            .await
            .expect("async record expensive");
    }
    m.set_provider_score(ProviderScore {
        asker_did: expensive.clone(),
        success_rate: 0.3,
        latency_ms: 300,
        samples: 10,
    });

    // Price-only ranking: both paths must pick the cheapest.
    let sync = m
        .cheapest_with_ranking("openai/gpt-4", LatencyRanking::cheapest())
        .expect("sync non-empty");
    let async_path = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::cheapest())
        .await
        .expect("async non-empty")
        .expect("async Some");
    assert_eq!(
        sync.asker_did, async_path.asker_did,
        "sync vs async must agree on cheapest asker"
    );
    assert_eq!(
        sync.asker_did, cheap,
        "price-only path must pick the cheapest-priced asker"
    );
    assert_eq!(sync.ask_id, async_path.ask_id);
}

#[tokio::test]
async fn dual_read_ranking_agrees_under_prefer_latency() {
    // Under `prefer_latency`, the mid-priced fast provider must
    // outrank the cheap-but-slow provider on BOTH surfaces.
    use quota_router_core::marketplace::scoring::LatencyRanking;
    use quota_router_core::marketplace::scoring::ProviderScore;

    let m = Marketplace::open_in_memory().expect("open_in_memory");
    let now = now_unix();
    let cheap_slow = sample_did(210);
    let mid_fast = sample_did(211);
    let expensive_slower = sample_did(212);
    let expires = now + 3600;
    m.put(&make_ask(&cheap_slow, "openai/gpt-4", 10_000, expires))
        .unwrap();
    m.put(&make_ask(&mid_fast, "openai/gpt-4", 30_000, expires))
        .unwrap();
    m.put(&make_ask(
        &expensive_slower,
        "openai/gpt-4",
        50_000,
        expires,
    ))
    .unwrap();

    // Seed both paths with same latencies: cheap=5000ms, mid=100ms,
    // expensive=4000ms.
    let controller = blake3_runtime(GOV_PUBKEY);
    for (did, lat) in [
        (&cheap_slow, 5_000_u64),
        (&mid_fast, 100),
        (&expensive_slower, 4_000),
    ] {
        m.record_outcome(did, true, lat);
        m.record_outcome_async(did, true, lat, controller, now)
            .await
            .unwrap();
        m.set_provider_score(ProviderScore {
            asker_did: did.clone(),
            success_rate: 1.0,
            latency_ms: lat,
            samples: 1,
        });
    }

    let sync = m
        .cheapest_with_ranking("openai/gpt-4", LatencyRanking::prefer_latency())
        .expect("sync non-empty");
    let async_path = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::prefer_latency())
        .await
        .expect("async non-empty")
        .expect("async Some");
    assert_eq!(
        sync.asker_did, async_path.asker_did,
        "sync vs async must agree on prefer_latency winner"
    );
    assert_eq!(
        sync.asker_did, mid_fast,
        "mid-fast must beat cheap-slow under prefer_latency"
    );
    // Latency surfaces on both entries (compat path surfaces
    // `latency_ms` from the canonical aggregate).
    assert_eq!(sync.latency_ms, async_path.latency_ms);
    assert_eq!(sync.latency_ms, Some(100));
}

#[tokio::test]
async fn dual_read_both_paths_return_none_on_empty_match() {
    // Trivial API surface parity: with no asks matching the model,
    // both `cheapest_with_ranking` (sync) and
    // `cheapest_with_ranking_async` (async) return `Ok(None)` /
    // `None`. This is the no-candidate contract from the mission
    // AC "all existing tests pass + 3 new dual-read comparison tests".
    use quota_router_core::marketplace::scoring::LatencyRanking;

    let m = Marketplace::open_in_memory().expect("open_in_memory");
    // No puts → empty book.
    let sync = m.cheapest_with_ranking("openai/gpt-4", LatencyRanking::cheapest());
    let async_path = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::cheapest())
        .await
        .expect("async no error on empty");
    assert!(sync.is_none(), "sync empty book = None");
    assert!(async_path.is_none(), "async empty book = None");
}

#[tokio::test]
async fn open_in_memory_with_store_wires_canonical_reputation_store() {
    // Mission `marketplace-generic-store` AC: the new
    // `open_in_memory_with_store(store)` constructor lets production
    // wire a custom `ReputationStore` impl. This test verifies the
    // round-trip — open with an `InMemoryReputationStore`, record
    // outcomes via the async path, read back via both surfaces —
    // exercises the new constructor end-to-end.
    use octo_reputation::store::InMemoryReputationStore;
    let custom_store = InMemoryReputationStore::new();
    let m =
        Marketplace::open_in_memory_with_store(custom_store).expect("open_in_memory_with_store");
    let did = sample_did(150);
    let controller = blake3_runtime(GOV_PUBKEY);

    // Round-trip a single outcome via the async path.
    m.record_outcome_async(&did, true, 75, controller, 1_700_000_000)
        .await
        .expect("async record through custom store");
    let compat = m.read_reputation_async(&did).await.expect("compat read");
    assert_eq!(compat.samples, 1, "compat store saw the record");
    assert!(compat.success_rate > 0.0, "compat success_rate populated");

    // Legacy shadow stays empty for this DID (record_outcome_async
    // does not write to it). Mirror the round-trip via legacy to
    // satisfy the dual-read comparison AC.
    m.record_outcome(&did, true, 75);
    let legacy = m
        .provider_score(&did)
        .expect("legacy score after legacy record");
    assert_eq!(legacy.latency_ms, 75);
}

/// RFC-0900 Gap 7.1 circuit-breaker must apply on BOTH read paths.
///
/// `set_min_reputation` used to write only the legacy shadow while
/// `cheapest_with_ranking_async` consults the compat registry's own
/// threshold. The setter appeared to work and read back correctly, but
/// the breaker was inert on the canonical async surface — a failing
/// provider was still returned as `cheapest`.
#[tokio::test]
async fn set_min_reputation_applies_to_both_read_paths() {
    use quota_router_core::marketplace::scoring::LatencyRanking;
    use quota_router_core::marketplace::Marketplace;

    let m = Marketplace::open_in_memory().expect("open");
    let controller = blake3_runtime(GOV_PUBKEY);
    let bad = sample_did(41);
    let good = sample_did(42);

    // `bad` quotes cheaper, so it wins unless the breaker excludes it.
    m.put(&make_ask(&bad, "openai/gpt-4", 1_000, 1_900_000_000))
        .unwrap();
    m.put(&make_ask(&good, "openai/gpt-4", 90_000, 1_900_000_000))
        .unwrap();

    for i in 0..20u64 {
        m.record_outcome(&bad, false, 50);
        m.record_outcome_async(&bad, false, 50, controller, 1_700_000_000 + i * 60)
            .await
            .expect("async record");
    }

    // Breaker off: the cheap failing provider wins on both paths.
    m.set_min_reputation(0.0);
    let sync_off = m.cheapest("openai/gpt-4").expect("some");
    assert_eq!(sync_off.cost_per_1k, 1_000, "breaker off: cheapest wins");
    let async_off = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::cheapest())
        .await
        .expect("async cheapest")
        .expect("some");
    assert_eq!(async_off.cost_per_1k, 1_000, "breaker off, async agrees");

    // Breaker on: the failing provider is excluded on BOTH paths.
    m.set_min_reputation(0.5);
    assert_eq!(m.min_reputation(), 0.5, "threshold reads back");

    let sync_on = m.cheapest("openai/gpt-4").expect("some");
    assert_eq!(
        sync_on.cost_per_1k, 90_000,
        "sync path must exclude the failing provider"
    );

    let async_on = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::cheapest())
        .await
        .expect("async cheapest")
        .expect("some");
    assert_eq!(
        async_on.cost_per_1k, 90_000,
        "async path must exclude the failing provider too"
    );
}

/// The two write surfaces are **not** mutually dual-writing, so a caller
/// that picks one gets a half-populated registry on each side.
///
/// `record_outcome` writes only `scoring::ProviderReputationRegistry`;
/// `record_outcome_async` writes only the RFC-0968 compat store. Nothing
/// fans one out to the other. During the dual-read window a caller must
/// invoke **both**, and every pre-existing parity fixture here does —
/// which is exactly why the gap was invisible.
///
/// The consequence is not a cosmetic `None`. With the breaker threshold
/// raised, an async-only failing provider is still returned by the sync
/// selection path, because the legacy registry holds no record of the
/// failures and an unknown provider is treated as un-excluded.
///
/// Measured (20 async-only records):
///   - `provider_score` -> `None`; `read_reputation_async` ->
///     `samples=20, success_rate=0.0`
///   - breaker on at 0.5, provider failed 20/20:
///       sync  `cheapest` -> 1_000  (failing provider, breaker INERT)
///       async `cheapest` -> 90_000 (correctly excluded)
///
/// Both assertions below were mutation-verified: making
/// `record_outcome_async` also write the legacy shadow fails the
/// `is_none` check, and with that check neutralized fails the breaker
/// check (`left: 90000, right: 1000`).
///
/// This test pins the divergence in its measured direction. It is a
/// change-detector, not an endorsement: if it fails, the dual-write gap
/// recorded in the `record_outcome` / `record_outcome_async` doc comments
/// has been closed and deferred mission
/// `marketplace-record-outcome-surfaces-require-a-dual-write` should be
/// closed with corrected numbers.
#[tokio::test]
async fn async_only_recording_leaves_the_sync_breaker_inert() {
    use quota_router_core::marketplace::scoring::LatencyRanking;
    use quota_router_core::marketplace::Marketplace;

    let m = Marketplace::open_in_memory().expect("open");
    let controller = blake3_runtime(GOV_PUBKEY);
    let bad = sample_did(43);
    let good = sample_did(44);

    m.put(&make_ask(&bad, "openai/gpt-4", 1_000, 1_900_000_000))
        .expect("put bad");
    m.put(&make_ask(&good, "openai/gpt-4", 90_000, 1_900_000_000))
        .expect("put good");

    // 20 failures recorded through the async surface ONLY.
    for i in 0..20u64 {
        m.record_outcome_async(&bad, false, 50, controller, 1_700_000_000 + i * 60)
            .await
            .expect("async record");
    }

    // The compat store holds the truth...
    let compat = m.read_reputation_async(&bad).await.expect("compat read");
    assert_eq!(compat.samples, 20, "compat saw every record");
    assert_eq!(compat.success_rate, 0.0, "compat recorded all failures");

    // ...and the legacy shadow holds nothing.
    assert!(
        m.provider_score(&bad).is_none(),
        "legacy shadow is unwritten by the async path"
    );

    // Raise the breaker. A provider that has failed 20/20 must not be
    // selectable on a reputation-gated path.
    m.set_min_reputation(0.5);

    let sync_pick = m.cheapest("openai/gpt-4").expect("some ask");
    assert_eq!(
        sync_pick.cost_per_1k, 1_000,
        "async-only failures leave the sync breaker inert: this is the \
         measured hazard, and it fails once the dual-write gap is closed"
    );

    let async_pick = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::cheapest())
        .await
        .expect("async cheapest")
        .expect("some ask");
    assert_eq!(
        async_pick.cost_per_1k, 90_000,
        "the async path, which does hold the record, excludes the failing provider"
    );
}

/// Positive control for the test above: the identical fixture recorded
/// through **both** surfaces must activate the sync breaker.
///
/// Without this, `async_only_recording_leaves_the_sync_breaker_inert`
/// could be satisfied by some other sync-path defect — a threshold that
/// ignores the registry altogether, or an exclusion rule keyed on the
/// wrong field — rather than by the dual-write gap. `bad` quotes cheaper
/// than `good`, so returning 90_000 can only mean `bad` was excluded.
#[tokio::test]
async fn dual_write_activates_the_sync_breaker() {
    use quota_router_core::marketplace::Marketplace;

    let m = Marketplace::open_in_memory().expect("open");
    let controller = blake3_runtime(GOV_PUBKEY);
    let bad = sample_did(45);
    let good = sample_did(46);

    m.put(&make_ask(&bad, "openai/gpt-4", 1_000, 1_900_000_000))
        .expect("put bad");
    m.put(&make_ask(&good, "openai/gpt-4", 90_000, 1_900_000_000))
        .expect("put good");

    // Same fixture as the test above — the only difference is that each
    // record also goes through the legacy surface.
    for i in 0..20u64 {
        m.record_outcome(&bad, false, 50);
        m.record_outcome_async(&bad, false, 50, controller, 1_700_000_000 + i * 60)
            .await
            .expect("async record");
    }

    assert_eq!(
        m.provider_score(&bad).map(|s| s.samples),
        Some(20),
        "legacy shadow now holds the failures"
    );

    m.set_min_reputation(0.5);

    let sync_pick = m.cheapest("openai/gpt-4").expect("some ask");
    assert_eq!(
        sync_pick.cost_per_1k, 90_000,
        "with both surfaces written, the sync breaker excludes the failing provider"
    );
}

/// `set_provider_score` is the mirror image of the dual-write gap pinned
/// by `async_only_recording_leaves_the_sync_breaker_inert`, and unlike it
/// the gap **cannot** be closed by fanning the write out.
///
/// `set_provider_score` writes only the legacy
/// `scoring::ProviderReputationRegistry`. The async path reads only
/// `reputation_compat`, and `ProviderReputationRegistryCompat` exposes no
/// score-override setter at all — its sole write is `record_with_now`,
/// which appends events. An operator override therefore has no
/// representation on the canonical surface.
///
/// Measured: provider `bad` recorded healthy via the async path, then
/// overridden to `success_rate = 0.0` via `set_provider_score`, breaker
/// at 0.5, `bad` quoting 1_000 against `good` at 90_000.
///
/// ```text
/// sync  provider_score = Some(success_rate 0.0, latency 900, samples 10)
/// async compat        = samples=1, success_rate=1.0, latency 50
/// SYNC  cheapest      = 90_000   (override honoured, provider excluded)
/// ASYNC cheapest      = 1_000    (override invisible, provider selected)
/// ```
///
/// Taken together with the async-only case, the invariant is:
/// **whichever reputation surface a caller writes to exclusively, the
/// other surface's circuit breaker is inert.**
#[tokio::test]
async fn score_override_is_invisible_to_the_async_breaker() {
    use quota_router_core::marketplace::scoring::{LatencyRanking, ProviderScore};
    use quota_router_core::marketplace::Marketplace;

    let m = Marketplace::open_in_memory().expect("open");
    let controller = blake3_runtime(GOV_PUBKEY);
    let bad = sample_did(51);
    let good = sample_did(52);

    m.put(&make_ask(&bad, "openai/gpt-4", 1_000, 1_900_000_000))
        .expect("put bad");
    m.put(&make_ask(&good, "openai/gpt-4", 90_000, 1_900_000_000))
        .expect("put good");

    // Healthy on the async surface first, so the only reason to exclude
    // `bad` afterwards is the override.
    m.record_outcome_async(&bad, true, 50, controller, 1_700_000_000)
        .await
        .expect("async record");

    m.set_min_reputation(0.5);
    m.set_provider_score(ProviderScore {
        asker_did: bad.to_owned(),
        samples: 10,
        success_rate: 0.0,
        latency_ms: 900,
    });

    // The override lands on the legacy surface.
    assert_eq!(
        m.provider_score(&bad).map(|s| s.success_rate),
        Some(0.0),
        "legacy registry holds the override"
    );
    // ...and not on the canonical one.
    let compat = m.read_reputation_async(&bad).await.expect("compat read");
    assert_eq!(compat.samples, 1, "compat still holds one healthy record");
    assert_eq!(compat.success_rate, 1.0, "compat never saw the override");

    let sync_pick = m.cheapest("openai/gpt-4").expect("some ask");
    assert_eq!(
        sync_pick.cost_per_1k, 90_000,
        "sync path honours the override and excludes the provider"
    );

    let async_pick = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::cheapest())
        .await
        .expect("async cheapest")
        .expect("some ask");
    assert_eq!(
        async_pick.cost_per_1k, 1_000,
        "async path cannot see the override and selects the \
         reputation-gated provider: this fails once the override reaches \
         the canonical surface"
    );
}

/// Positive control: on the same fixture shape, feeding the async surface
/// real failure data — rather than an override it cannot express — does
/// exclude the provider. Without this, the assertion above could be
/// satisfied by a broken async exclusion gate rather than by the
/// unpropagated override.
#[tokio::test]
async fn async_breaker_excludes_on_its_own_recorded_failures() {
    use quota_router_core::marketplace::scoring::LatencyRanking;
    use quota_router_core::marketplace::Marketplace;

    let m = Marketplace::open_in_memory().expect("open");
    let controller = blake3_runtime(GOV_PUBKEY);
    let bad = sample_did(53);
    let good = sample_did(54);

    m.put(&make_ask(&bad, "openai/gpt-4", 1_000, 1_900_000_000))
        .expect("put bad");
    m.put(&make_ask(&good, "openai/gpt-4", 90_000, 1_900_000_000))
        .expect("put good");

    for i in 0..20u64 {
        m.record_outcome_async(&bad, false, 50, controller, 1_700_000_000 + i * 60)
            .await
            .expect("async record");
    }
    m.set_min_reputation(0.5);

    let async_pick = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::cheapest())
        .await
        .expect("async cheapest")
        .expect("some ask");
    assert_eq!(
        async_pick.cost_per_1k, 90_000,
        "the async breaker does exclude a provider it has failures for"
    );
}

/// A provider with **no** latency observation must not be ranked as if
/// it had measured a 0 ms completion.
///
/// `0` is the fastest value the scale admits, so resolving absent data to
/// zero gave an unmeasured provider a perfect latency component and let
/// it outrank every measured one. Measured before the fix, on both the
/// sync and async paths: with `unknown` quoting 90_000 and `measured`
/// quoting 1_000 at a proven 50 ms, `cheapest` returned the measured
/// provider while `prefer_latency` returned `unknown` on both paths —
/// a full inversion driven only by `unknown` having no history.
///
/// Absent observations now resolve to the worst latency observed among
/// the candidates, so missing data never wins. Cold-start providers stay
/// selectable on price, which is the property the midpoint reading would
/// have given for free and the exclusion reading would not.
#[tokio::test]
async fn unmeasured_provider_does_not_rank_as_zero_latency() {
    use quota_router_core::marketplace::scoring::LatencyRanking;
    use quota_router_core::marketplace::Marketplace;

    let m = Marketplace::open_in_memory().expect("open");
    let controller = blake3_runtime(GOV_PUBKEY);
    let unknown = sample_did(71);
    let measured = sample_did(72);

    m.put(&make_ask(&unknown, "openai/gpt-4", 90_000, 1_900_000_000))
        .expect("put unknown");
    m.put(&make_ask(&measured, "openai/gpt-4", 1_000, 1_900_000_000))
        .expect("put measured");

    for i in 0..10u64 {
        m.record_outcome(&measured, true, 50);
        m.record_outcome_async(&measured, true, 50, controller, 1_700_000_000 + i * 60)
            .await
            .expect("async record");
    }

    assert!(
        m.provider_score(&unknown).is_none(),
        "fixture precondition: unknown has no legacy score"
    );
    let compat_unknown = m.read_reputation_async(&unknown).await.expect("compat");
    assert_eq!(
        compat_unknown.samples, 0,
        "fixture precondition: unknown has no canonical record"
    );

    // Price-only ranking is unaffected and picks the cheap provider.
    assert_eq!(
        m.cheapest("openai/gpt-4").expect("some").cost_per_1k,
        1_000,
        "price-only ranking still prefers the cheaper provider"
    );

    // Latency-weighted ranking must agree that the proven provider wins,
    // rather than promoting the one with no history at all.
    let sync_pick = m
        .cheapest_with_ranking("openai/gpt-4", LatencyRanking::prefer_latency())
        .expect("some ask");
    assert_eq!(
        sync_pick.cost_per_1k, 1_000,
        "sync path: an unmeasured provider must not beat a measured one on latency"
    );

    let async_pick = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::prefer_latency())
        .await
        .expect("async cheapest")
        .expect("some ask");
    assert_eq!(
        async_pick.cost_per_1k, 1_000,
        "async path: same rule, and the two paths must not disagree"
    );
}

/// Positive control for the test above: an unmeasured provider that is
/// *also* the cheapest still wins under `prefer_latency`.
///
/// Without this, the fix could be passing simply by refusing to rank any
/// provider without a latency observation — which would lock every
/// cold-start provider out of the book. The fail-closed encoding must
/// keep them selectable; it must only stop absent data from outranking
/// measured data.
#[tokio::test]
async fn unmeasured_provider_still_wins_on_price_when_cheapest() {
    use quota_router_core::marketplace::scoring::LatencyRanking;
    use quota_router_core::marketplace::Marketplace;

    let m = Marketplace::open_in_memory().expect("open");
    let controller = blake3_runtime(GOV_PUBKEY);
    let cold = sample_did(73);
    let measured = sample_did(74);

    m.put(&make_ask(&cold, "openai/gpt-4", 1_000, 1_900_000_000))
        .expect("put cold");
    m.put(&make_ask(&measured, "openai/gpt-4", 90_000, 1_900_000_000))
        .expect("put measured");

    for i in 0..10u64 {
        m.record_outcome(&measured, true, 900);
        m.record_outcome_async(&measured, true, 900, controller, 1_700_000_000 + i * 60)
            .await
            .expect("async record");
    }

    let sync_pick = m
        .cheapest_with_ranking("openai/gpt-4", LatencyRanking::prefer_latency())
        .expect("some ask");
    assert_eq!(
        sync_pick.cost_per_1k, 1_000,
        "an unmeasured provider stays selectable when it wins on price"
    );

    let async_pick = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::prefer_latency())
        .await
        .expect("async cheapest")
        .expect("some ask");
    assert_eq!(
        async_pick.cost_per_1k, 1_000,
        "async path agrees: cold-start providers are not locked out"
    );
}

/// A listing whose `asker_did` cannot be parsed must not take down the
/// async selection path.
///
/// `Ask::new` rejects only an *empty* `asker_did`, so any non-empty
/// string could be listed. The async gate and latency read both parsed
/// every candidate DID, so a single such listing returned `Err` for the
/// entire model while the sync path carried on — listing content became
/// a denial-of-service on the canonical path.
///
/// Measured before the fix: with `asker_did = "not-a-canonical-did"`,
/// `cheapest` returned that listing, and with the breaker enabled
/// `cheapest_with_ranking_async` failed with `recorder did malformed`.
///
/// An unparseable DID now means "no recorded reputation", which is
/// exactly what it is — nothing to exclude and no latency observation.
/// `record_outcome_async` still rejects such a DID, so reputation can
/// never be *written* under one.
#[tokio::test]
async fn unparseable_listing_did_does_not_fail_the_async_selection_path() {
    use quota_router_core::marketplace::scoring::LatencyRanking;
    use quota_router_core::marketplace::Marketplace;

    let m = Marketplace::open_in_memory().expect("open");
    let good = sample_did(81);
    let malformed = "not-a-canonical-did";

    m.put(&make_ask(&good, "openai/gpt-4", 90_000, 1_900_000_000))
        .expect("put good");
    m.put(&make_ask(malformed, "openai/gpt-4", 1_000, 1_900_000_000))
        .expect("put malformed");

    // Breaker off: sync selects the cheaper listing.
    let sync_off = m.cheapest("openai/gpt-4").expect("some");
    assert_eq!(sync_off.asker_did, malformed, "sync picks the cheaper one");

    // Breaker on. This used to return Err for the whole model.
    m.set_min_reputation(0.5);
    let sync_on = m.cheapest("openai/gpt-4").expect("some");
    let async_on = m
        .cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::cheapest())
        .await
        .expect("async path must not fail on listing content")
        .expect("some ask");
    assert_eq!(
        async_on.asker_did, sync_on.asker_did,
        "async and sync must agree, not merely both survive"
    );

    // The latency-weighted path takes the same route and must survive too.
    m.cheapest_with_ranking_async("openai/gpt-4", LatencyRanking::prefer_latency())
        .await
        .expect("latency-weighted async path must not fail either")
        .expect("some ask");

    // Strictness is preserved where it belongs: no reputation can be
    // written under an unparseable DID.
    let err = m
        .record_outcome_async(
            malformed,
            true,
            50,
            blake3_runtime(GOV_PUBKEY),
            1_700_000_000,
        )
        .await
        .expect_err("record must still reject an unparseable DID");
    assert!(
        matches!(
            err,
            octo_reputation::error::ReputationError::RecorderDidMalformed { .. }
        ),
        "expected RecorderDidMalformed, got {err:?}"
    );
}
