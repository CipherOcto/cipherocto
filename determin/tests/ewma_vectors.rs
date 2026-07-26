//! RFC-0968 §23 EWMA vector reproduction.
//!
//! Runs the four-event sequence prescribed in the canonical spec and
//! asserts each reported constant reproduces after the
//! catastrophic-cancellation mitigation (u256 alignment in dfp_add).

use octo_determin::{dfp_abs, dfp_add, dfp_lt, dfp_mul, dfp_sub, Dfp, DfpEncoding};

const INITIAL: f64 = 1.0;

/// The four RFC-0968 §23 events: `(delta, alpha)`.
const EVENTS: &[(f64, f64)] = &[(-0.3, 0.1), (-0.5, 0.1), (0.1, 0.1), (-0.2, 0.1)];

/// Documented expected values per RFC-0968 §23.
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
fn rfc0968_section23_vectors_reproduce() {
    let mut score = Dfp::from_f64(INITIAL);
    for (i, (delta_f, alpha_f)) in EVENTS.iter().enumerate() {
        let delta = Dfp::from_f64(*delta_f);
        let alpha = Dfp::from_f64(*alpha_f);
        let d_abs = dfp_abs(delta);
        let one = Dfp::from_i64(1);
        let weight = if dfp_lt(d_abs, one) { d_abs } else { one };
        let alpha_w = dfp_mul(alpha, weight);
        let inner_term = dfp_sub(one, alpha_w);
        let left = dfp_mul(score, inner_term);
        let right = dfp_mul(dfp_mul(delta, alpha), weight);
        score = dfp_add(left, right);
        let expected: f64 = EXPECTED[i].parse().unwrap();
        let observed = score.to_f64();
        // Strict 1e-9 for events 1-3 (which reproduce exactly).
        // Event 4 lands at 0.85846909000... — the exact 113-bit
        // result, matching f64 to 16 digits. The spec's 7-digit-decimal
        // expected value 0.8584691 is a 1e-8 truncation; 1e-7 tolerance
        // matches the spec's stated 7-digit precision.
        let tol: f64 = if i < 3 { 1e-9 } else { 1e-7 };
        let rel_err = ((observed - expected) / expected).abs();
        assert!(
            rel_err < tol,
            "event {} drift: expected {} observed {:.12} (rel_err {:.3e} > tol {:.0e})",
            i + 1,
            EXPECTED[i],
            observed,
            rel_err,
            tol
        );
    }
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
fn dfp_sub_catastrophic_cancellation_works() {
    // 1.0 - 0.03 = 0.97 in IEEE math. Sticky-bit fix alone is not
    // enough; u256 alignment closes the gap.
    let r = dfp_sub(Dfp::from_f64(1.0), Dfp::from_f64(0.03)).to_f64();
    assert!((r - 0.97).abs() < 1e-12, "1.0 - 0.03 = {r}");
}

#[test]
#[ignore]
fn dbg_div_1_1() {
    use octo_determin::dfp_div;
    let r = dfp_div(Dfp::from_f64(1.0), Dfp::from_f64(1.0));
    eprintln!("1/1: mantissa={:028x} exp={} class={:?} sign={} to_f64={}",
        r.mantissa, r.exponent, r.class, r.sign, r.to_f64());
    let r = dfp_div(Dfp::from_f64(7.0), Dfp::from_f64(1.0));
    eprintln!("7/1: mantissa={:028x} exp={} class={:?} sign={} to_f64={}",
        r.mantissa, r.exponent, r.class, r.sign, r.to_f64());
}

#[test]
#[ignore]
fn dbg_to_f64_canonical() {
    use octo_determin::Dfp;
    let cases: &[(u128, i32, f64)] = &[
        (1, 0, 1.0),
        (3, 0, 3.0),
        (1, 1, 2.0),
        (1, -1, 0.5),
        (1, -2, 0.25),
        (1, 2, 4.0),
        (0x1F0A3D70A3D70A3C28F5C28F5C29, -109, 0.97),
        ((1u128 << 113) - 1, -113, 1.0),
        (0x1bfffffffffffffffffffffffffffu128, -110, 7.0),
    ];
    for (m, e, expected) in cases {
        let d = Dfp::from_signed(*m as i128, *e);
        let got = d.to_f64();
        eprintln!("m={:028x} e={} -> to_f64={} expected={}",
            m, e, got, expected);
    }
}

#[test]
#[ignore]
fn diagnostic_event4_per_step() {
    let mut score = Dfp::from_f64(INITIAL);
    let one = Dfp::from_i64(1);
    for (i, (delta_f, alpha_f)) in EVENTS.iter().enumerate() {
        let delta = Dfp::from_f64(*delta_f);
        let alpha = Dfp::from_f64(*alpha_f);
        let d_abs = dfp_abs(delta);
        let weight = if dfp_lt(d_abs, one) { d_abs } else { one };
        let alpha_w = dfp_mul(alpha, weight);
        let inner_term = dfp_sub(one, alpha_w);
        let left = dfp_mul(score, inner_term);
        let right = dfp_mul(dfp_mul(delta, alpha), weight);
        let next = dfp_add(left, right);
        eprintln!(
            "event {}: score_before={} delta={} alpha={} weight={} alpha_w={} inner={} left={} right={} next={}",
            i + 1,
            score.to_f64(),
            delta.to_f64(),
            alpha.to_f64(),
            weight.to_f64(),
            alpha_w.to_f64(),
            inner_term.to_f64(),
            left.to_f64(),
            right.to_f64(),
            next.to_f64()
        );
        eprintln!(
            "  left: mantissa={:x} exp={} sign={}",
            left.mantissa, left.exponent, left.sign
        );
        eprintln!(
            "  right: mantissa={:x} exp={} sign={}",
            right.mantissa, right.exponent, right.sign
        );
        score = next;
    }
}

#[test]
fn dfp_sub_additional_baselines() {
    // Smaller-scale subtractions.
    let cases = [
        (2.0, 1.0, 1.0),
        (1.5, 0.5, 1.0),
        (1.0, 0.0625, 0.9375),
        (3.0, 0.5, 2.5),
        (10.0, 1.0, 9.0),
        (100.0, 0.5, 99.5),
        (1.0, 1.0, 0.0),
        (2.5, 1.5, 1.0),
        (0.86, 0.04, 0.82),
        (0.5, 0.125, 0.375),
    ];
    for (a, b, expected) in cases {
        let r = dfp_sub(Dfp::from_f64(a), Dfp::from_f64(b)).to_f64();
        let ok = if expected == 0.0 {
            r.abs() < 1e-10
        } else {
            (r - expected).abs() < expected.abs() * 1e-10 + 1e-15
        };
        assert!(ok, "{a} - {b} = {r}, expected {expected}");
    }
}
