//! `SinkSpecific` payload cap-at-scrubber verification
//! (RFC-0014-v3 §S5.3 + mission 0014-v3 Deliverable 3).
//!
//! Asserts the substrate-faithful posture: `SettlementError::SinkSpecific`
//! substrate Display remains VERBATIM (no cap at substrate per
//! RFC-0014-v3 §S5.3 + §FW3 DEFERRED); cap-at-scrubber fires
//! when the scrubber sees > 4 KiB input.
//!
//! Run with:
//!   cargo test -p octo-settlement --test sink_specific_payload_cap_at_scrubber

use octo_settlement::scrub_adapter_error_with;
use octo_settlement_core::SettlementError;

const ADAPTER_TYPES: &[&str] = &["StoolapStore", "StoolapReceiptSink"];

#[test]
fn substrate_display_preserves_short_payload_substrate_faithful() {
    let payload = "stoolap tx aborted at row 42".to_owned();
    let e = SettlementError::SinkSpecific(payload.clone());
    let s = format!("{}", e);
    // Substrate-faithful: short payload emits verbatim at substrate.
    assert!(s.contains(&payload), "{}", s);
}

#[test]
fn substrate_display_preserves_10k_payload_substrate_faithful() {
    // Substrate-faithful = NO cap at substrate (per RFC-0014-v3 §S5.3
    // + §FW3 DEFERRED). Full 10K chars emitted verbatim at substrate.
    let payload = "x".repeat(10_000);
    let e = SettlementError::SinkSpecific(payload.clone());
    let s = format!("{}", e);
    assert_eq!(s.len(), payload.len() + "sink-specific error: ".len());
    assert!(s.ends_with(&payload), "substrate Display must be verbatim");
}

#[test]
fn scrubber_caps_at_4kib_input_with_redacted_too_long_sentinel() {
    // Cap-at-scrubber posture: > 4 KiB input → sentinel fires. Use a
    // non-empty adapter registry so the scrubber precondition
    // (`scrub_adapter_error_with called with empty ADAPTER_TYPES
    // registry and non-empty input`) does not abort.
    let big = "x".repeat(5_000);
    let out = scrub_adapter_error_with(&big, ADAPTER_TYPES);
    assert!(
        out.contains("<redacted-too-long>"),
        "scrubber must emit <redacted-too-long> sentinel on > 4 KiB input: out len={}",
        out.len()
    );
}

#[test]
fn scrubber_passes_short_input_unchanged() {
    let short = "stoolap tx aborted";
    let out = scrub_adapter_error_with(short, ADAPTER_TYPES);
    // Pattern 5 (io error) doesn't match "stoolap tx aborted" — the
    // input passes through verbatim. (Pattern 6 is substring-replace
    // and only fires when ADAPTER_TYPES registry contains a name
    // substring present in the input.)
    assert_eq!(out, short);
}

#[test]
fn scrubber_passes_exactly_4kib_input() {
    // Exactly at the boundary: 4 KiB passes through unchanged (no
    // sentinel). > 4 KiB triggers the sentinel.
    let exact = "a".repeat(4 * 1024);
    let out = scrub_adapter_error_with(&exact, ADAPTER_TYPES);
    assert!(
        !out.contains("<redacted-too-long>"),
        "exactly 4 KiB input must pass through unchanged"
    );
}
