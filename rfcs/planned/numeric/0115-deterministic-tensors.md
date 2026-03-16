# RFC-0115 (Numeric/Math): Deterministic Tensors (DTENSOR)

## Status

**Version:** 0.1 (2026-03-14)
**Status:** Planned

> **Note:** This RFC is extracted from RFC-0106 (Deterministic Numeric Tower) as part of the Track B dismantling effort.

## Summary

This RFC defines Deterministic Tensor (DTENSOR) operations for consensus-critical batch AI inference and high-dimensional computations.

## Status: Planned

DTENSOR is deferred to Phase 4 (Q4 2026+) until:
- Real AI inference requirements are better understood
- Broadcasting and layout specifications are finalized
- Convolution specifications are defined

## Conceptual Scope

### Type System (Draft)

```rust
/// Deterministic N-dimensional Tensor
pub struct DTensor<T: Numeric> {
    /// Shape: N-dimensional sizes
    pub shape: Vec<usize>,
    /// Data: flattened in row-major order
    pub data: Vec<T>,
}
```

### Potential Operations

| Operation | Description | Priority |
|-----------|-------------|----------|
| TENSOR_ADD | Element-wise addition | High |
| TENSOR_MUL | Element-wise multiplication | High |
| TENSOR_MAT_MUL | Matrix multiplication on last two dims | High |
| BROADCAST | Expand dimensions | Medium |
| TRANSPOSE | Dimension permutation | Medium |
| RESHAPE | Change shape | Medium |
| CONV2D | 2D convolution | Low |
| BATCH_MATMUL | Batched matrix multiply | Low |

### Open Questions

1. **Memory Layout**: NCHW vs NHWC?
2. **Broadcasting Rules**: How to align mismatched dimensions?
3. **Convolution**: Stride, padding, dilation parameters?
4. **Batch Dimension**: How to handle variable batch sizes?

## Dependencies

| RFC | Relationship |
|-----|--------------|
| RFC-0113 (DMAT) | DTENSOR extends 2D matrices to N-dimensions |

## Timeline

- **Phase 4 (Q4 2026+)**: Initial specification
- **Phase 5 (Q1 2027+)**: Implementation

## References

- RFC-0104: Deterministic Floating-Point
- RFC-0105: Deterministic Quant Arithmetic
- RFC-0112: Deterministic Vectors
- RFC-0113: Deterministic Matrices
- RFC-0114: Deterministic Activation Functions
- RFC-0106: Deterministic Numeric Tower (archived)
