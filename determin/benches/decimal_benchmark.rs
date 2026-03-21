//! DECIMAL Arithmetic Benchmark Suite
//!
//! Compares num-bigint vs internal bigint implementations.
//!
//! Run with:
//!   cargo bench --bench decimal_benchmark -- --measurement-time=5
//!   cargo bench --bench decimal_benchmark --features use-internal-bigint -- --measurement-time=5

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use octo_determin::decimal::Decimal;
use octo_determin::decimal::RoundingMode;

// Import functions based on feature
#[cfg(not(feature = "use-internal-bigint"))]
use octo_determin::decimal::{
    decimal_add, decimal_cmp, decimal_div, decimal_mul, decimal_round, decimal_sqrt, decimal_sub,
};

#[cfg(feature = "use-internal-bigint")]
use octo_determin::decimal_internal::{
    decimal_add_internal as decimal_add, decimal_cmp_internal as decimal_cmp,
    decimal_div_internal as decimal_div, decimal_mul_internal as decimal_mul,
    decimal_round_internal as decimal_round, decimal_sqrt_internal as decimal_sqrt,
    decimal_sub_internal as decimal_sub,
};

// =============================================================================
// Test Fixtures
// =============================================================================

fn create_decimal(mantissa: i128, scale: u8) -> Decimal {
    Decimal::new(mantissa, scale).unwrap()
}

// =============================================================================
// ADD Benchmarks
// =============================================================================

fn add_same_scale(c: &mut Criterion) {
    let dec_a = create_decimal(123456789012345_i128, 6);
    let dec_b = create_decimal(987654321098765_i128, 6);
    c.bench_function("add_same_scale", |bencher| {
        bencher.iter(|| black_box(decimal_add(&dec_a, &dec_b).unwrap()))
    });
}

fn add_different_scale(c: &mut Criterion) {
    let dec_a = create_decimal(123456789012345_i128, 6);
    let dec_b = create_decimal(987654321098765_i128, 12);
    c.bench_function("add_different_scale", |bencher| {
        bencher.iter(|| black_box(decimal_add(&dec_a, &dec_b).unwrap()))
    });
}

fn add_large_mantissa(c: &mut Criterion) {
    let max_mantissa = 10i128.pow(36) - 1;
    let dec_a = create_decimal(max_mantissa / 2, 36);
    let dec_b = create_decimal(max_mantissa / 3, 36);
    c.bench_function("add_large_mantissa", |bencher| {
        bencher.iter(|| black_box(decimal_add(&dec_a, &dec_b).unwrap()))
    });
}

// =============================================================================
// SUB Benchmarks
// =============================================================================

fn sub_same_scale(c: &mut Criterion) {
    let dec_a = create_decimal(987654321098765_i128, 6);
    let dec_b = create_decimal(123456789012345_i128, 6);
    c.bench_function("sub_same_scale", |bencher| {
        bencher.iter(|| black_box(decimal_sub(&dec_a, &dec_b).unwrap()))
    });
}

fn sub_cancellation(c: &mut Criterion) {
    let dec_a = create_decimal(1000000000000_i128, 12);
    let dec_b = create_decimal(999999999999_i128, 12);
    c.bench_function("sub_cancellation", |bencher| {
        bencher.iter(|| black_box(decimal_sub(&dec_a, &dec_b).unwrap()))
    });
}

// =============================================================================
// MUL Benchmarks
// =============================================================================

fn mul_basic(c: &mut Criterion) {
    let dec_a = create_decimal(12345678_i128, 6);
    let dec_b = create_decimal(87654321_i128, 6);
    c.bench_function("mul_basic", |bencher| {
        bencher.iter(|| black_box(decimal_mul(&dec_a, &dec_b).unwrap()))
    });
}

fn mul_high_scale(c: &mut Criterion) {
    let dec_a = create_decimal(123456789012345_i128, 20);
    let dec_b = create_decimal(987654321098765_i128, 20);
    c.bench_function("mul_high_scale", |bencher| {
        bencher.iter(|| black_box(decimal_mul(&dec_a, &dec_b).unwrap()))
    });
}

fn mul_max_mantissa(c: &mut Criterion) {
    // Use smaller mantissa to avoid overflow in multiplication
    let large_mantissa = 10i128.pow(18) - 1; // Safe for multiplication
    let dec_a = create_decimal(large_mantissa, 18);
    let dec_b = create_decimal(large_mantissa, 18);
    c.bench_function("mul_max_mantissa", |bencher| {
        bencher.iter(|| black_box(decimal_mul(&dec_a, &dec_b).unwrap()))
    });
}

// =============================================================================
// DIV Benchmarks
// =============================================================================

fn div_basic(c: &mut Criterion) {
    let dec_a = create_decimal(123456789012345_i128, 6);
    let dec_b = create_decimal(87654321_i128, 6);
    c.bench_function("div_basic", |bencher| {
        bencher.iter(|| black_box(decimal_div(&dec_a, &dec_b, 18).unwrap()))
    });
}

fn div_small_divisor(c: &mut Criterion) {
    let dec_a = create_decimal(123456789012345_i128, 6);
    let dec_b = create_decimal(3_i128, 0);
    c.bench_function("div_small_divisor", |bencher| {
        bencher.iter(|| black_box(decimal_div(&dec_a, &dec_b, 18).unwrap()))
    });
}

fn div_large_scale_diff(c: &mut Criterion) {
    let dec_a = create_decimal(123456789012345_i128, 30);
    let dec_b = create_decimal(987654321098765_i128, 6);
    c.bench_function("div_large_scale_diff", |bencher| {
        bencher.iter(|| black_box(decimal_div(&dec_a, &dec_b, 18).unwrap()))
    });
}

// =============================================================================
// SQRT Benchmarks
// =============================================================================

fn sqrt_basic(c: &mut Criterion) {
    let dec_a = create_decimal(123456789012345_i128, 12);
    c.bench_function("sqrt_basic", |bencher| {
        bencher.iter(|| black_box(decimal_sqrt(&dec_a).unwrap()))
    });
}

fn sqrt_perfect_square(c: &mut Criterion) {
    let dec_a = create_decimal(144_i128, 0);
    c.bench_function("sqrt_perfect_square", |bencher| {
        bencher.iter(|| black_box(decimal_sqrt(&dec_a).unwrap()))
    });
}

fn sqrt_irrational(c: &mut Criterion) {
    let dec_a = create_decimal(2000000000000000000_i128, 18);
    c.bench_function("sqrt_irrational", |bencher| {
        bencher.iter(|| black_box(decimal_sqrt(&dec_a).unwrap()))
    });
}

fn sqrt_max_scale(c: &mut Criterion) {
    let dec_a = create_decimal(10_i128.pow(35), 36);
    c.bench_function("sqrt_max_scale", |bencher| {
        bencher.iter(|| black_box(decimal_sqrt(&dec_a).unwrap()))
    });
}

// =============================================================================
// ROUND Benchmarks
// =============================================================================

fn round_half_even(c: &mut Criterion) {
    let dec_a = create_decimal(12345678901234567890_i128, 18);
    c.bench_function("round_half_even", |bencher| {
        bencher.iter(|| black_box(decimal_round(&dec_a, 10, RoundingMode::RoundHalfEven).unwrap()))
    });
}

fn round_down(c: &mut Criterion) {
    let dec_a = create_decimal(12345678901234567890_i128, 18);
    c.bench_function("round_down", |bencher| {
        bencher.iter(|| black_box(decimal_round(&dec_a, 10, RoundingMode::RoundDown).unwrap()))
    });
}

fn round_up(c: &mut Criterion) {
    let dec_a = create_decimal(12345678901234567890_i128, 18);
    c.bench_function("round_up", |bencher| {
        bencher.iter(|| black_box(decimal_round(&dec_a, 10, RoundingMode::RoundUp).unwrap()))
    });
}

// =============================================================================
// CMP Benchmarks
// =============================================================================

fn cmp_same_scale(c: &mut Criterion) {
    let dec_a = create_decimal(987654321098765_i128, 6);
    let dec_b = create_decimal(123456789012345_i128, 6);
    c.bench_function("cmp_same_scale", |bencher| {
        bencher.iter(|| black_box(decimal_cmp(&dec_a, &dec_b)))
    });
}

fn cmp_different_scale(c: &mut Criterion) {
    let dec_a = create_decimal(1000000_i128, 6);
    let dec_b = create_decimal(1_i128, 0);
    c.bench_function("cmp_different_scale", |bencher| {
        bencher.iter(|| black_box(decimal_cmp(&dec_a, &dec_b)))
    });
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(std::time::Duration::from_secs(5));
    targets =
        add_same_scale,
        add_different_scale,
        add_large_mantissa,
        sub_same_scale,
        sub_cancellation,
        mul_basic,
        mul_high_scale,
        mul_max_mantissa,
        div_basic,
        div_small_divisor,
        div_large_scale_diff,
        sqrt_basic,
        sqrt_perfect_square,
        sqrt_irrational,
        sqrt_max_scale,
        round_half_even,
        round_down,
        round_up,
        cmp_same_scale,
        cmp_different_scale,
);
criterion_main!(benches);
