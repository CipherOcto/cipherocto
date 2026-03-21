# DECIMAL Core Type Implementation Design

## Mission
`0111-decimal-core-type` — RFC-0111 Deterministic DECIMAL core type

## Context

RFC-0111 defines Deterministic DECIMAL — an i128-based scaled integer with 0-36 decimal scale. This design covers the core type only (data structure, canonicalization, validation, serialization). Arithmetic operations are separate missions.

## Design Decisions

### Module Structure
- New `decimal.rs` module in `determin/src/`
- Standalone functions throughout (matches BigInt/DFP pattern, matches RFC algorithm naming)
- Private fields with getters (enforces canonical invariant, per RFC lazy canonicalization model)

### Error Model
```rust
pub enum DecimalError {
    Overflow,       // |mantissa| > 10^36-1
    DivisionByZero, // DIV by zero
    InvalidScale,   // scale > 36
    NonCanonical,    // deserialize received non-canonical input
    ConversionLoss,  // DECIMAL→DQA scale > 18, or scale != 0
}
```
Five distinct variants — no additional context carried (determinism requirement).

### Overflow Handling
- `checked_mul`, `checked_add`, etc. throughout — returns `None → Err(Overflow)`
- i256 intermediates for scale alignment (matches RFC pseudo-code exactly)

### Constants
```rust
pub const MAX_DECIMAL_SCALE: u8 = 36;
pub const MAX_DECIMAL_MANTISSA: i128 = 10_i128.pow(36) - 1;
pub const MIN_DECIMAL_MANTISSA: i128 = -(10_i128.pow(36) - 1);
const POW10: [i128; 37] = [/* 10^0 to 10^36 */];
```
- POW10: hardcoded inline array (config hash requirement)
- MAX/MIN: computed from 10^36 (independent from POW10 per RFC)

### Canonicalization
```rust
fn canonicalize(&mut self) {
    if self.mantissa == 0 { self.scale = 0; return; }
    while self.scale > 0 && self.mantissa % 10 == 0 {
        self.mantissa /= 10;
        self.scale -= 1;
    }
}
```
- Zero forced to `{0, 0}`
- Trailing zeros stripped
- `validate()` separate from `canonicalize()` (range vs normalization)

### Serialization
- 24-byte canonical wire format (big-endian i128 mantissa + 7 bytes zero padding + u8 scale)
- Zero padding verified on deserialize — reject with `NonCanonical` if non-zero

### Constructors
- `Decimal::new(mantissa, scale)` — public, validates + canonicalizes
- `Decimal::from_parts_unchecked(mantissa, scale)` — internal, for arithmetic ops post-validation

## Implementation

```rust
// determin/src/decimal.rs
pub struct Decimal { mantissa: i128, scale: u8 }

impl Decimal {
    pub fn new(mantissa: i128, scale: u8) -> Result<Self, DecimalError>
    fn from_parts_unchecked(mantissa: i128, scale: u8) -> Self
    pub fn mantissa(&self) -> i128
    pub fn scale(&self) -> u8
    pub fn is_zero(&self) -> bool
    fn canonicalize(&mut self)
    fn validate(&self) -> Result<(), DecimalError>
    pub fn canonicalized(mut self) -> Self
}

pub fn decimal_to_bytes(d: &Decimal) -> [u8; 24]
pub fn decimal_from_bytes(bytes: [u8; 24]) -> Result<Decimal, DecimalError>

impl From<&Decimal> for [u8; 24]
impl TryFrom<[u8; 24]> for Decimal
```

## Tests

- Canonical zero: any zero → `{0, 0}`
- Negative mantissa canonicalizes
- Trailing zeros stripped
- MAX/MIN boundary accepted
- Invalid scale rejected
- Overflow rejected (positive and negative)
- Roundtrip serialize/deserialize
- Non-canonical padding rejected
- Non-canonical bytes rejected on deserialize

## Dependencies

None — this mission is self-contained.

## Reference

- RFC-0111 §Data Structure, §Canonical Form, §Constants, §POW10 Table, §Canonical Byte Format
