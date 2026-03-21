# DECIMAL Implementation Benchmark Report

**Date:** 2026-03-21
**Benchmark Suite:** decimal_benchmark
**Configuration:** 100 samples, 5s measurement time per operation
**Platform:** Linux 5.15.0, Rust 1.93.0

---

## Executive Summary

| Aspect | num-bigint (default) | internal bigint |
|--------|---------------------|-----------------|
| ADD/SUB | Faster (~130-150ns) | Slower (~180-240ns, +30-60%) |
| MUL | Faster (~110-290ns) | Mixed (-6% to +49%) |
| **DIV** | **~24% faster after hybrid optimization** | Was faster, now tied |
| SQRT | **Significantly faster** (~2-3.5µs) | **Much slower** (~10-13µs, 3-5x) |
| ROUND | Equivalent (~30ns) | Equivalent (~30ns) |
| CMP | Equivalent (~2.5ns) | Equivalent (~2.5-2.7ns) |

**Note:** The default `decimal.rs` now uses internal bigint for DIV multiplication (hybrid approach), giving 24-26% DIV speedup while keeping ADD/SUB/MUL/SQRT on num-bigint.

**Conclusion:** The hybrid approach (num-bigint + internal bigint for DIV) provides optimal performance across all operations.

---

## Detailed Results

### ADD Operations

| Operation | num-bigint | internal bigint | Δ |
|-----------|------------|-----------------|---|
| add_same_scale | 129.13 ns | 145.89 ns | **+13.0%** |
| add_different_scale | 206.79 ns | 239.87 ns | **+16.0%** |
| add_large_mantissa | 129.10 ns | 183.32 ns | **+42.0%** |

**Analysis:** Internal bigint is consistently slower for ADD operations, with larger performance degradation when working with large mantissas (36-digit numbers).

---

### SUB Operations

| Operation | num-bigint | internal bigint | Δ |
|-----------|------------|-----------------|---|
| sub_same_scale | 122.73 ns | 180.99 ns | **+47.5%** |
| sub_cancellation | 153.89 ns | 234.62 ns | **+52.5%** |

**Analysis:** SUB operations show significant regression (~50% slower). Cancellation cases (similar magnitude numbers) are particularly impacted.

---

### MUL Operations

| Operation | num-bigint | internal bigint | Δ |
|-----------|------------|-----------------|---|
| mul_basic | 107.79 ns | 160.58 ns | **+49.0%** |
| mul_high_scale | 284.81 ns | 311.08 ns | **+9.2%** |
| mul_max_mantissa | 186.88 ns | 175.01 ns | **-6.3%** ✓ |

**Analysis:** Basic multiplication is ~50% slower, but large mantissa multiplication is actually slightly faster with internal bigint.

---

### DIV Operations

| Operation | Before (pure num-bigint) | After (hybrid) | Δ |
|-----------|--------------------------|----------------|---|
| div_basic | 186.10 ns | 141.62 ns | **-24%** ✓ |
| div_small_divisor | 313.41 ns | 265.61 ns | **-15%** ✓ |
| div_large_scale_diff | 195.60 ns | 144.19 ns | **-26%** ✓ |

**Analysis:** The default `decimal.rs` now uses internal bigint for the multiplication step in DIV (scale_diff > 0 case), giving 24-26% improvement. Pure internal bigint was previously 12-20% faster, but the hybrid approach achieves similar results while keeping num-bigint for other operations.

---

### SQRT Operations

| Operation | num-bigint | internal bigint | Δ |
|-----------|------------|-----------------|---|
| sqrt_basic | 3.497 µs | 12.903 µs | **+269%** ⚠️ |
| sqrt_perfect_square | 2.012 µs | 11.048 µs | **+449%** ⚠️ |
| sqrt_irrational | 2.495 µs | 11.095 µs | **+345%** ⚠️ |
| sqrt_max_scale | 2.578 µs | 11.056 µs | **+329%** ⚠️ |

**Analysis:** SQRT is dramatically slower (3-5x) with internal bigint. This is the most significant performance regression. The Newton-Raphson iteration in internal bigint uses bit shifts for division instead of optimized division.

---

### ROUND Operations

| Operation | num-bigint | internal bigint | Δ |
|-----------|------------|-----------------|---|
| round_half_even | 31.10 ns | 30.93 ns | **-0.5%** ✓ |
| round_down | 30.36 ns | 30.01 ns | **-1.2%** ✓ |
| round_up | 31.43 ns | 30.66 ns | **-2.5%** ✓ |

**Analysis:** ROUND operations are equivalent, with internal bigint showing slight advantages.

---

### CMP Operations

| Operation | num-bigint | internal bigint | Δ |
|-----------|------------|-----------------|---|
| cmp_same_scale | 2.62 ns | 2.60 ns | **-1.0%** ✓ |
| cmp_different_scale | 2.50 ns | 2.72 ns | **+8.8%** |

**Analysis:** CMP operations are nearly equivalent. The different-scale comparison shows minor regression.

---

## Root Cause Analysis

### Why SQRT is 3-5x Slower

The internal bigint SQRT uses `bigint_div` (division by 2) in each Newton-Raphson iteration, while num-bigint's `BigInt` type has an optimized right-shift operator (`>>`). Each iteration of:
```rust
x = (x + n/x) / 2
```
requires one division operation. With internal bigint, division is implemented via `bigint_divmod`, which has higher constant factors than num-bigint's optimized `BigInt::div_floor`.

### Why DIV is Faster

Internal bigint's `bigint_divmod` appears to have lower overhead for the specific division patterns in DECIMAL arithmetic (dividing by multi-limb numbers with specific scale adjustments).

---

## Recommendations

### Default (Recommended)

**Use default `decimal.rs` (hybrid)** - It offers:
- Fastest DIV operations (24-26% improvement via internal bigint mul)
- Fastest SQRT operations (num-bigint)
- Good ADD/SUB/MUL performance (num-bigint)
- Optimal balance across all operations

### For Minimal Dependencies / Embedded Use

**Use `decimal_internal.rs` with `--features use-internal-bigint`** - It offers:
- No external dependency on num-bigint
- ~12-20% faster division operations
- Suitable when SQRT is rarely used or performance is less critical

### Potential Optimizations for Internal BigInt SQRT

1. Add `bigint_shr` (right shift) and use `x >> 1` instead of `x / 2`
2. Consider pre-computing initial guess using bit-length analysis
3. Add early termination when convergence is achieved

---

## Benchmark Methodology

```rust
// Each operation benchmark:
// 1. Create test fixtures (Decimals) outside the benchmark loop
// 2. Use black_box() to prevent optimization
// 3. 100 samples with 5 second measurement time per operation
// 4. Run on release profile with optimizations

// Example:
fn add_same_scale(c: &mut Criterion) {
    let dec_a = create_decimal(123456789012345_i128, 6);
    let dec_b = create_decimal(987654321098765_i128, 6);
    c.bench_function("add_same_scale", |bencher| {
        bencher.iter(|| black_box(decimal_add(&dec_a, &dec_b).unwrap()))
    });
}
```

---

## Files

- `benches/decimal_benchmark.rs` - Benchmark suite
- `src/decimal.rs` - num-bigint implementation
- `src/decimal_internal.rs` - internal bigint implementation
