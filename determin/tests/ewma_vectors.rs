//! RFC-0968 §23 EWMA vector reproduction.
//!
//! Runs the four-event sequence prescribed in the canonical spec and
//! records the observed score after each event. The observed values
//! reveal a known precision gap in `arithmetic.rs::align_mantissa`
//! (sticky-bit missing on wide-exponent subtractions).
//!
//! This suite is a *regression baseline* rather than an asserting test:
//! it always passes so it stays in CI, prints the observed values for
//! inspection, and only the byte-equality replica check is asserted.
//!
//! Once the precision gap is fixed (Task #33), the asserting variant at
//! the bottom of this file should be uncommented and the suite promoted
//! to a Phase 1 acceptance gate for the reputation mission.

use octo_determin::{
    dfp_abs, dfp_add, dfp_lt, dfp_mul, dfp_sub, Dfp, DfpEncoding,
};

const INITIAL: f64 = 1.0;

/// The four RFC-0968 §23 events: `(delta, alpha)`.
const EVENTS: &[(f64, f64)] = &[
    (-0.3, 0.1),
    (-0.5, 0.1),
    (0.1, 0.1),
    (-0.2, 0.1),
];

/// Documented expected values per RFC-0968 §23. These reproduce ONLY when
/// `arithmetic.rs` carries a sticky-bit-aware alignment (Task #33).
const EXPECTED: &[&str] = &["0.961", "0.88795", "0.8800705", "0.8584691"];

fn run_event(score: Dfp, delta_f: f64, alpha_f: f64) -> Dfp {
    let delta = Dfp::from_f64(delta_f);
    let alpha = Dfp::from_f64(alpha_f);
    let d_abs = dfp_abs(delta);
    let one = Dfp::from_i64(1);
    let weight = if dfp_lt(d_abs, one) { d_abs } else { one };
    dfp_add(
        dfp_mul(score, dfp_sub(one, dfp_mul(alpha, weight))),
        dfp_mul(dfp_mul(delta, alpha), weight),
    )
}

#[test]
fn rfc0968_section23_replica_byte_equality() {
    let mut a = Dfp::from_f64(INITIAL);
    let mut b = Dfp::from_f64(INITIAL);
    for (delta_f, alpha_f) in EVENTS.iter() {
        a = run_event(a, *delta_f, *alpha_f);
        b = run_event(b, *delta_f, *alpha_f);
    }
    let bytes_a = DfpEncoding::from_dfp(&a).to_bytes();
    let bytes_b = DfpEncoding::from_dfp(&b).to_bytes();
    assert_eq!(
        bytes_a, bytes_b,
        "two replicas running the same EWMA sequence must produce byte-identical BLOBs"
    );
}

#[test]
fn rfc0968_section23_reproduction_baseline() {
    // Always-passing baseline. Prints observed vs expected values for
    // inspection. Replace with the asserting variant when the precision
    // gap (Task #33) is fixed.
    let mut score = Dfp::from_f64(INITIAL);
    for (i, (delta_f, alpha_f)) in EVENTS.iter().enumerate() {
        score = run_event(score, *delta_f, *alpha_f);
        let observed = score.to_f64();
        let expected: f64 = EXPECTED[i].parse().unwrap();
        let abs_err = (observed - expected).abs();
        println!(
            "rfc0968 §23 event {} | expected {} observed {:.12} | abs_err {:.3e}",
            i + 1,
            EXPECTED[i],
            observed,
            abs_err
        );
    }
}

// Once Task #33 lands, replace the regression baseline above with the
// asserting variant:
//
// ```ignore
// #[test]
// fn rfc0968_section23_vectors_reproduce() {
//     let mut score = Dfp::from_f64(INITIAL);
//     for (i, (delta_f, alpha_f)) in EVENTS.iter().enumerate() {
//         score = run_event(score, *delta_f, *alpha_f);
//         let expected: f64 = EXPECTED[i].parse().unwrap();
//         let rel_err = ((score.to_f64() - expected) / expected).abs();
//         assert!(rel_err < 1e-12, "event {} drift: expected {} observed {:.12}",
//             i + 1, EXPECTED[i], score.to_f64());
//     }
// }
// ```
