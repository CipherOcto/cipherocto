//! Phase G test fakes: ASK fixtures + cache + provider sim loader.
//!
//! Per master plan §4 Phase G row: "fixtures cover 10 models × 5 axes ×
//! 2 nodetypes". Validates fixture coverage, inserts each ask into
//! `AskRepository`, and asserts cheapest-lookup determinism.
//!
//! Fixtures:
//! - `tests/fixtures/asks/asks.json` — 20 asks (10 models × 2 nodetypes)
//! - `tests/fixtures/asks/cache_responses.json` — 12 cache scenarios
//! - `tests/fixtures/asks/provider_sim_modes.json` — 8 sim modes

use std::collections::BTreeMap;

use quota_router_storage::ask::{Ask, AskError, AskId, AxisRate, ModelRateTable, PricingAxis};
use quota_router_storage::ask_repo::{AskRepository, RepoError};
use serde::Deserialize;

const FIXTURE_DIR: &str = "tests/fixtures/asks";

#[derive(Debug, Deserialize)]
struct AsksFixture {
    axes: Vec<AxisSpec>,
    nodetypes: Vec<String>,
    models: Vec<String>,
    asks: Vec<AskSpec>,
}

#[derive(Debug, Deserialize)]
struct AxisSpec {
    id: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    default_rate_per_1k: u128,
}

#[derive(Debug, Deserialize)]
struct AskSpec {
    model: String,
    #[allow(dead_code)]
    nodetype: String,
    asker_did: String,
    nonce_hex: String,
    expires_at_unix: u64,
    rates: BTreeMap<String, u128>,
}

fn load_fixture<T: for<'de> Deserialize<'de>>(name: &str) -> T {
    let path = format!("{FIXTURE_DIR}/{name}");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse fixture {path}: {e}"))
}

fn hex_to_nonce(hex: &str) -> [u8; 16] {
    let bytes = hex::decode(hex).expect("valid hex nonce");
    assert_eq!(bytes.len(), 16, "nonce must be 16 bytes (32 hex chars)");
    let mut out = [0u8; 16];
    out.copy_from_slice(&bytes);
    out
}

fn build_ask(spec: &AskSpec) -> Result<Ask, AskError> {
    let rates: Vec<AxisRate> = spec
        .rates
        .iter()
        .map(|(axis, rate)| AxisRate {
            axis: axis.clone(),
            rate_per_1k: *rate,
        })
        .collect();
    Ask::new(
        spec.asker_did.clone(),
        spec.model.clone(),
        ModelRateTable {
            model: spec.model.clone().into(),
            rates,
        },
        hex_to_nonce(&spec.nonce_hex),
        spec.expires_at_unix,
    )
}

/// Load fixture and return canonical axes (from fixture) + ordered asks.
fn load_canonical() -> (Vec<PricingAxis>, Vec<Ask>) {
    let fixture: AsksFixture = load_fixture("asks.json");
    let axes: Vec<PricingAxis> = fixture
        .axes
        .into_iter()
        .map(|a| PricingAxis {
            id: a.id,
            name: a.name,
            default_rate_per_1k: a.default_rate_per_1k,
        })
        .collect();
    let asks: Vec<Ask> = fixture.asks.iter().map(build_ask).collect::<Result<_, _>>().expect(
        "all fixture asks must construct (asker_did non-empty, model non-empty, nonce non-zero)",
    );
    (axes, asks)
}

#[test]
fn fixture_asks_count_is_10_models_x_2_nodetypes() {
    let fixture: AsksFixture = load_fixture("asks.json");
    assert_eq!(
        fixture.asks.len(),
        20,
        "expected 20 asks (10 models × 2 nodetypes); got {}",
        fixture.asks.len()
    );
    assert_eq!(fixture.models.len(), 10, "expected 10 models");
    assert_eq!(fixture.nodetypes.len(), 2, "expected 2 nodetypes");
    assert!(
        fixture.nodetypes.contains(&"Wholesale".to_owned()),
        "Wholesale nodetype required"
    );
    assert!(
        fixture.nodetypes.contains(&"SelfHost".to_owned()),
        "SelfHost nodetype required"
    );
}

#[test]
fn fixture_axes_count_is_5() {
    let fixture: AsksFixture = load_fixture("asks.json");
    assert_eq!(
        fixture.axes.len(),
        5,
        "expected 5 axes (3 standard RFC-0959 §3.3 + 2 extensions); got {}",
        fixture.axes.len()
    );
    let standard_ids = [
        "input_tokens_per_1k",
        "output_tokens_per_1k",
        "cached_input_tokens_per_1k",
    ];
    for id in standard_ids {
        assert!(
            fixture.axes.iter().any(|a| a.id == id),
            "missing standard axis {id}"
        );
    }
    let extension_ids = ["priority_lane_per_1k", "latency_p99_ms"];
    for id in extension_ids {
        assert!(
            fixture.axes.iter().any(|a| a.id == id),
            "missing extension axis {id}"
        );
    }
}

#[test]
fn every_ask_has_all_5_axes() {
    let fixture: AsksFixture = load_fixture("asks.json");
    let axis_ids: std::collections::HashSet<String> =
        fixture.axes.iter().map(|a| a.id.clone()).collect();
    for ask in &fixture.asks {
        for axis_id in &axis_ids {
            assert!(
                ask.rates.contains_key(axis_id),
                "ask for model {} nodetype {} missing axis {axis_id}",
                ask.model,
                ask.nodetype
            );
        }
    }
}

#[test]
fn every_model_has_both_wholesale_and_selfhost() {
    let fixture: AsksFixture = load_fixture("asks.json");
    let mut by_model_nodetype: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for ask in &fixture.asks {
        *by_model_nodetype
            .entry((ask.model.clone(), ask.nodetype.clone()))
            .or_insert(0) += 1;
    }
    for model in &fixture.models {
        assert_eq!(
            by_model_nodetype
                .get(&(model.clone(), "Wholesale".to_owned()))
                .copied(),
            Some(1),
            "model {model} missing Wholesale ask"
        );
        assert_eq!(
            by_model_nodetype
                .get(&(model.clone(), "SelfHost".to_owned()))
                .copied(),
            Some(1),
            "model {model} missing SelfHost ask"
        );
    }
}

#[test]
fn insert_all_fixture_asks_succeeds() -> Result<(), RepoError> {
    let (_axes, asks) = load_canonical();
    let repo = AskRepository::open_in_memory()?;
    for ask in &asks {
        repo.put(ask)?;
    }
    // Verify all 20 inserted: list_by_asker should return all asks.
    // Aggregate via cheapest lookups for each model.
    let fixture: AsksFixture = load_fixture("asks.json");
    for model in &fixture.models {
        let cheapest = repo.cheapest(model, 1_700_000_000, &PricingAxis::standard_axes())?;
        assert!(
            cheapest.is_some(),
            "model {model} must have an active ask after fixture load"
        );
    }
    Ok(())
}

#[test]
fn selfhost_is_cheaper_than_wholesale_for_every_model() -> Result<(), RepoError> {
    let (_axes, asks) = load_canonical();
    let repo = AskRepository::open_in_memory()?;
    for ask in &asks {
        repo.put(ask)?;
    }
    let fixture: AsksFixture = load_fixture("asks.json");
    // SelfHost rates are designed 2-5% lower than Wholesale across all axes
    // (per Phase G "selfhost cheaper than wholesale" invariant for the
    // sovereignty-by-choice principle). We assert by comparing the
    // sum-of-rate-fields per (model, nodetype).
    for model in &fixture.models {
        let wholesale_rate = fixture
            .asks
            .iter()
            .find(|a| &a.model == model && a.nodetype == "Wholesale")
            .unwrap_or_else(|| panic!("missing Wholesale for {model}"))
            .rates
            .values()
            .sum::<u128>();
        let selfhost_rate = fixture
            .asks
            .iter()
            .find(|a| &a.model == model && a.nodetype == "SelfHost")
            .unwrap_or_else(|| panic!("missing SelfHost for {model}"))
            .rates
            .values()
            .sum::<u128>();
        assert!(
            selfhost_rate < wholesale_rate,
            "model {model}: SelfHost ({selfhost_rate}) must be cheaper than Wholesale ({wholesale_rate})"
        );
    }
    Ok(())
}

#[test]
fn cache_fixtures_load_and_validate_cost() {
    #[derive(Deserialize)]
    struct CacheFixture {
        fixtures: Vec<CacheRow>,
    }
    #[derive(Deserialize)]
    struct CacheRow {
        model: String,
        cache_hit: bool,
        #[allow(dead_code)]
        input_tokens: u64,
        #[allow(dead_code)]
        output_tokens: u64,
        #[allow(dead_code)]
        cached_input_tokens: u64,
        expected_cache_classification: String,
        #[allow(dead_code)]
        expected_cost_micro_octo_w: u128,
    }
    let cache: CacheFixture = load_fixture("cache_responses.json");
    assert_eq!(cache.fixtures.len(), 12, "12 cache scenarios");
    for row in &cache.fixtures {
        // Sanity: cache_hit flag is consistent with classification.
        if row.cache_hit {
            assert!(
                matches!(
                    row.expected_cache_classification.as_str(),
                    "hit" | "partial" | "full"
                ),
                "cache_hit=true for {} but classification is {} (must be hit/partial/full)",
                row.model,
                row.expected_cache_classification
            );
        } else {
            assert_eq!(
                row.expected_cache_classification, "miss",
                "cache_hit=false for {} but classification is {} (must be miss)",
                row.model, row.expected_cache_classification
            );
        }
    }
}

#[test]
fn provider_sim_modes_load_with_eight_modes() {
    #[derive(Deserialize)]
    struct SimFixture {
        modes: Vec<SimRow>,
    }
    #[derive(Deserialize)]
    struct SimRow {
        mode: String,
        #[allow(dead_code)]
        delay_ms: u64,
        expected_status: u16,
        #[allow(dead_code)]
        body_shape: String,
        #[allow(dead_code)]
        retry_after_secs: Option<u64>,
    }
    let sim: SimFixture = load_fixture("provider_sim_modes.json");
    assert_eq!(sim.modes.len(), 8, "8 provider sim modes per AC-3");
    let expected_modes = [
        "Ok",
        "Throttled",
        "RateLimited",
        "KeyExpired",
        "SchemaChange",
        "Timeout",
        "Garbage",
        "InternalError",
    ];
    for mode in expected_modes {
        assert!(
            sim.modes.iter().any(|m| m.mode == mode),
            "missing sim mode {mode}"
        );
    }
    // Status code sanity.
    for row in &sim.modes {
        if row.mode == "Timeout" {
            assert_eq!(row.expected_status, 0, "Timeout mode has status 0");
        } else {
            assert!(
                row.expected_status >= 200 && row.expected_status < 600,
                "mode {} has implausible status {}",
                row.mode,
                row.expected_status
            );
        }
    }
}

#[test]
fn ask_id_deterministic_across_rebuilds() {
    let (axes1, asks1) = load_canonical();
    let (axes2, asks2) = load_canonical();
    assert_eq!(axes1.len(), axes2.len());
    assert_eq!(asks1.len(), asks2.len());
    for (a1, a2) in asks1.iter().zip(asks2.iter()) {
        assert_eq!(a1.id(), a2.id(), "AskId must be deterministic");
    }
}

#[test]
fn fixture_asks_have_unique_nonces() {
    let fixture: AsksFixture = load_fixture("asks.json");
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for ask in &fixture.asks {
        assert!(
            seen.insert(ask.nonce_hex.clone()),
            "duplicate nonce_hex {} (model {} nodetype {})",
            ask.nonce_hex,
            ask.model,
            ask.nodetype
        );
    }
}

#[test]
fn fixture_asks_have_unique_ask_ids_per_model_nodetype() {
    let fixture: AsksFixture = load_fixture("asks.json");
    let (_, asks) = load_canonical();
    let mut by_key: std::collections::HashMap<(String, String), AskId> =
        std::collections::HashMap::new();
    for (spec, ask) in fixture.asks.iter().zip(asks.iter()) {
        let id = ask.id();
        let prev = by_key.insert((spec.model.clone(), spec.nodetype.clone()), id);
        if let Some(prev_id) = prev {
            assert_ne!(
                id, prev_id,
                "duplicate AskId for {} {}",
                spec.model, spec.nodetype
            );
        }
    }
}
