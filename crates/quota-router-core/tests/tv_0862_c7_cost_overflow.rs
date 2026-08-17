//! TV-0862-C7 — Adjacent module u64→i64 wrap mitigation (mission 0862-c7).
//!
//! Pins the S4 Round 2 -class signed-underflow surface at the
//! `SpendEvent` boundary + `cost_u64_to_i64` free function. Without
//! this guard, `cost_amount > i64::MAX` (~9.2e18) silently wraps to
//! negative via `as i64`, letting `current + cost_i64 > budget` pass
//! when it should fail (defeats the budget gate).
//!
//! Pin points per mission 0862-c7 AC-3:
//! - `cost_u64_to_i64(i64::MAX as u64 + 1)` → `SpendEventError::CostOverflow`
//! - `cost_u64_to_i64(i64::MAX as u64)` → `Ok(i64::MAX)`
//! - `cost_u64_to_i64(0)` → `Ok(0)`
//! - `SpendEvent::cost_amount_i64()` mirrors the free function
//! - `KeyError::SpendEvent(SpendEventError)` propagates via `From`

use quota_router_core::keys::cost_u64_to_i64;
use quota_router_core::keys::errors::KeyError;
use quota_router_core::keys::models::SpendEventError;
use quota_router_core::keys::SpendEvent;

#[test]
fn tv_0862_c7_cost_overflow_at_boundary() {
    // Pin exact edge: cost_amount = i64::MAX + 1 → CostOverflow
    let cost_overflow = i64::MAX as u64 + 1;
    let err = cost_u64_to_i64(cost_overflow).expect_err("must fail closed");
    assert_eq!(
        err,
        SpendEventError::CostOverflow {
            cost: cost_overflow,
            max: i64::MAX,
        },
        "exact CostOverflow variant + payload pin"
    );

    // Verify KeyError round-trip via From<SpendEventError>
    let key_err: KeyError = err.into();
    match key_err {
        KeyError::SpendEvent(SpendEventError::CostOverflow { cost, max }) => {
            assert_eq!(cost, cost_overflow);
            assert_eq!(max, i64::MAX);
        }
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn tv_0862_c7_cost_at_max_passes() {
    // Pin boundary: cost_amount = i64::MAX exactly → Ok(i64::MAX)
    let cost_at_max = i64::MAX as u64;
    assert_eq!(
        cost_u64_to_i64(cost_at_max).expect("must pass at exact boundary"),
        i64::MAX
    );
}

#[test]
fn tv_0862_c7_cost_zero_passes() {
    // Pin lower boundary: cost_amount = 0 → Ok(0)
    assert_eq!(cost_u64_to_i64(0).expect("must pass at zero"), 0);
}

#[test]
fn tv_0862_c7_spend_event_method_mirrors_free_fn() {
    // Pin: SpendEvent::cost_amount_i64() delegates to cost_u64_to_i64
    let mut event = SpendEvent {
        event_id: "test".to_string(),
        request_id: "req".to_string(),
        key_id: uuid::Uuid::new_v4(),
        team_id: None,
        provider: "openai".to_string(),
        model: "gpt-4".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost_amount: 0,
        pricing_hash: [0u8; 32],
        token_source: Default::default(),
        tokenizer_version: None,
        provider_usage_json: None,
        timestamp: 0,
    };

    // cost_amount = 0 → Ok(0)
    assert_eq!(event.cost_amount_i64().expect("zero ok"), 0);

    // cost_amount = i64::MAX → Ok(i64::MAX)
    event.cost_amount = i64::MAX as u64;
    assert_eq!(event.cost_amount_i64().expect("max ok"), i64::MAX);

    // cost_amount = i64::MAX + 1 → CostOverflow
    event.cost_amount = i64::MAX as u64 + 1;
    assert_eq!(
        event.cost_amount_i64().expect_err("must overflow"),
        SpendEventError::CostOverflow {
            cost: i64::MAX as u64 + 1,
            max: i64::MAX,
        }
    );
}
