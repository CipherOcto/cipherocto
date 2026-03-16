# RFC-0126 (Serialization): Deterministic Serialization

## Status

**Version:** 1.2 (2026-03-16)
**Status:** Draft

> **Note:** This RFC defines canonical serialization formats for all protocol data structures to ensure bit-identical encoding across implementations. It covers both **binary serialization** (for numeric types) and **JSON serialization** (for structured data).

## Authors

- Primary Author: CipherOcto Team
- Contributing Reviewers: TBD

## Maintainers

- Lead Maintainer: TBD
- Technical Contact: TBD
- Repository: `rfcs/draft/numeric/0126-deterministic-serialization.md`

## Dependencies

### Required RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0104 (DFP) | Required | Defines DfpEncoding format |
| RFC-0105 (DQA) | Required | Defines DqaEncoding format |
| RFC-0110 (BIGINT) | Required | Defines BigIntEncoding format |

### Optional RFCs

| RFC | Relationship | Reason |
|-----|--------------|--------|
| RFC-0111 (DECIMAL) | Optional | Future decimal encoding extension |

## Design Goals

1. **Determinism**: All types must serialize to identical bytes across implementations
2. **No Ambiguity**: Each numeric type uses distinct encoding to prevent Merkle hash collisions
3. **Efficiency**: Fixed-size encodings where possible for fast parsing
4. **Extensibility**: Version byte allows future format changes without breaking compatibility
5. **Validation**: All deserialized data validated for canonical form

## Motivation

### Why Serialization Matters

Currently serialization is implicitly assumed. Without a standard:

- **Hash mismatches** between implementations (different byte orderings)
- **Proof verification failures** (inconsistent encoding)
- **Cross-language compatibility bugs** (endianness, padding, struct layout)
- **JSON variability** (key ordering, whitespace, number formatting)

### The Merkle Hash Ambiguity Problem

If multiple numeric types use the same encoding format, a Merkle tree cannot distinguish between them:

```
Example: DQA(1.0) vs BIGINT(1)
If both encode to identical bytes, their Merkle hashes are identical.
This breaks consensus state verification.
```

**Solution**: Each type uses a distinct encoding format.

## Summary

This RFC defines two complementary serialization systems:

### Part 1: Binary Serialization (Numeric Types)

For consensus-critical numeric types:

| Encoding | Authoritative RFC | Type | Size |
|----------|-------------------|------|------|
| I128Encoding | RFC-0110 | Integer | 16 bytes |
| BigIntEncoding | RFC-0110 | Arbitrary Integer | 8-520 bytes |
| DqaEncoding | RFC-0105 | Decimal | 16 bytes |
| DfpEncoding | RFC-0104 | Floating-Point | 24 bytes |

### Part 2: JSON Serialization (Structured Data)

For non-consensus data that requires deterministic representation:

- Canonical JSON field ordering
- Consistent number formatting
- Whitespace normalization
- Escape sequence handling

## Part 1: Binary Serialization (Numeric Types)

### Overview

```
┌─────────────────────────────────────────────┐
│           Numeric Encoding Types              │
├─────────────────────────────────────────────┤
│ I128Encoding   → i128 (16 bytes, BE)      │
│ BigIntEncoding → Arbitrary Integer         │
│                 (variable, 8-520 bytes)    │
│ DqaEncoding    → Decimal (16 bytes, BE)   │
│ DfpEncoding    → Floating-Point (24 bytes)│
└─────────────────────────────────────────────┘
```

### Encoding Reference

#### I128Encoding

- **Authoritative RFC**: RFC-0110 (§I128Encoding)
- **Size**: 16 bytes
- **Format**: i128 two's complement, big-endian
- **Version**: Embedded in first byte (for BIGINT), implicit for raw i128

#### BigIntEncoding

- **Authoritative RFC**: RFC-0110 (§Canonical Byte Format)
- **Size**: 8-520 bytes (variable)
- **Format**: `[version:1][sign:1][reserved:2][num_limbs:1][reserved:3][limbs:n*8]`
- **Version**: Byte 0 (0x01 = current)

#### DqaEncoding

- **Authoritative RFC**: RFC-0105 (§DqaEncoding)
- **Size**: 16 bytes
- **Format**: `[value:8][scale:1][reserved:7]`
- **Version**: Implicit v1

#### DfpEncoding

- **Authoritative RFC**: RFC-0104 (§DfpEncoding)
- **Size**: 24 bytes
- **Format**: `[mantissa:16][exponent:4][class_sign:4]`
- **Version**: Implicit v1

### Cross-Type Ambiguity Prevention

Each type's encoding is structurally distinct, preventing Merkle hash collisions:

| Type A | Type B | Encoding Difference |
|--------|--------|---------------------|
| DQA(1.0) | BIGINT(1) | DQA: 16 bytes with scale; BIGINT: variable header + limbs |
| DFP(1.0) | BIGINT(1) | DFP: 24 bytes with class_sign; BIGINT: variable header + limbs |
| DQA(1) | I128(1) | DQA: 16 bytes with scale field; I128: 16 bytes raw |

## Part 2: JSON Serialization (Structured Data)

### Overview

For structured data (non-consensus) that requires deterministic representation (e.g., API metadata, configuration), this RFC defines canonical JSON serialization rules.

### Canonical JSON Rules

#### 1. Field Ordering

- **Object keys** MUST be sorted lexicographically (ASCII order)
- **Array order** MUST be preserved as-is (no sorting)
- **Use BTreeMap** in implementations for automatic ordering

```
✓ {"a": 1, "b": 2}     // Correct: sorted keys
✗ {"b": 2, "a": 1}     // Incorrect: unsorted
```

#### 2. Number Formatting

- **Integers** MUST NOT have decimal points or exponents
- **Floating-point** MUST use lowercase `e` for exponents (if present)
- **No leading zeros** except for zero (0)
- **No unnecessary leading plus signs**

```
✓ 12345
✗ 012345
✗ +12345
✗ 1.2345e4
```

#### 3. Whitespace

- **No whitespace** inside JSON structures
- **No trailing whitespace**
- **Single space** after colon and comma

```
✓ {"key":"value"}
✗ { "key" : "value" }
✗ {"key": "value" }
```

#### 4. String Encoding

- **UTF-8** only
- **Escape sequences** MUST use lowercase (e.g., `\n` not `\N`)
- **Control characters** MUST be escaped
- **No unnecessary escapes** (e.g., `\/` is allowed but `/` is preferred)

```
✓ "hello\nworld"
✗ "hello\nworld" (uppercase N)
```

#### 5. Null vs Empty

- **Empty array**: `[]`
- **Empty object**: `{}`
- **Null** MUST be the literal string `null` (not `undefined` or omitted)

```
✓ {"field": null}
✗ {"field": ""}
```

### Example: Canonical JSON Serialization

#### Input (unspecified order)

```json
{
  "metadata": {
    "created_at": 1234567890,
    "name": "test"
  },
  "enabled": true,
  "tags": ["a", "c", "b"]
}
```

#### Canonical Output

```json
{"enabled":true,"metadata":{"created_at":1234567890,"name":"test"},"tags":["a","c","b"]}
```

### Implementation Guidelines

```rust
use std::collections::BTreeMap;

fn canonical_json_encode<T: Serialize>(value: &T) -> String {
    // Use BTreeMap for automatic key ordering
    // Serialize with compact formatting (no whitespace)
    // Use lowercase escape sequences
}

fn verify_canonical(json: &str) -> bool {
    // Parse and re-serialize, compare results
    // Check key ordering, whitespace, number format
}
```

## Error Handling

### Binary Serialization Errors

All serialization errors are fatal (TRAP). See authoritative RFCs for specific error codes:

| Error Domain | Authoritative RFC |
|--------------|-------------------|
| BIGINT errors | RFC-0110 |
| DQA errors | RFC-0105 |
| DFP errors | RFC-0104 |

### JSON Serialization Errors

| Error | Condition |
|-------|-----------|
| JSON_INVALID_UTF8 | Input not valid UTF-8 |
| JSON_INVALID_NUMBER | Number formatting violation |
| JSON_KEY_ORDER_VIOLATION | Keys not lexicographically sorted |
| JSON_WHITESPACE_VIOLATION | Extra whitespace detected |

## Test Vectors

### Binary Serialization

Test vectors are defined in each authoritative RFC:

| Encoding | Authoritative RFC |
|----------|-------------------|
| I128Encoding | RFC-0110 (§Test Vectors) |
| BigIntEncoding | RFC-0110 (§Test Vectors) |
| DqaEncoding | RFC-0105 (§Test Vectors) |
| DfpEncoding | RFC-0104 (§Test Vectors) |

### JSON Serialization

| Input | Canonical Output |
|-------|-----------------|
| `{"b":2,"a":1}` | `{"a":1,"b":2}` |
| `{"x":1e2}` | `{"x":100}` |
| `{"x":+1}` | `{"x":1}` |
| `{"x":01}` | ERROR |
| `{"a":["b","a"]}` | `{"a":["b","a"]}` (array order preserved) |

## Security Considerations

### Binary Serialization

1. **Buffer Overflow**: Prevented by explicit length validation
2. **Canonical Form Violation**: TRAP on non-canonical input
3. **Version Rollback**: Unknown versions TRAP
4. **Endianness Confusion**: Explicit big-endian everywhere

### JSON Serialization

1. **Hash Manipulation**: Canonical form prevents different representations producing same hash
2. **Unicode Attacks**: Enforce UTF-8, normalize unicode escape sequences
3. **Number Precision**: Specify precision limits for floating-point in JSON
4. **Whitespace Injection**: Reject inputs with inconsistent whitespace

## Adversarial Review

### Review History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-16 | Initial draft with duplicated definitions |
| 1.1 | 2026-03-16 | Removed duplicated definitions, now references authoritative RFCs |
| 1.2 | 2026-03-16 | Added Part 2: JSON Serialization for RFC-0903 compatibility |

### Known Issues

None.

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-03-16 | CipherOcto | Initial draft with duplicated definitions |
| 1.1 | 2026-03-16 | CipherOcto | Removed duplication - references authoritative RFCs |
| 1.2 | 2026-03-16 | CipherOcto | Added JSON Serialization (Part 2) for RFC-0903 |

## Compatibility

### Binary Serialization

- Version byte (where present) allows format evolution
- Old versions TRAP on deserialization
- Forward compatibility not guaranteed

### JSON Serialization

- Implementations MUST produce canonical output
- Consumers MAY accept non-canonical but SHOULD reject
- Version negotiation not supported

## Future Work

1. **DecimalEncoding**: RFC-0111 DECIMAL type serialization
2. **Enum/Union Tags**: Type-safe wrapper for multi-type numeric values
3. **Binary Object Signing (BOS)**: Canonical binary serialization wrapper
4. **Schema Validation**: JSON Schema for canonical JSON validation

## Related Use Cases

- UC-0903: Virtual API Key System (uses JSON serialization)
- UC-XXX: Cross-chain state verification (future)
- UC-XXX: Deterministic proof verification (future)

## References

- [RFC-0104: Deterministic Floating-Point](../draft/numeric/0104-deterministic-floating-point.md)
- [RFC-0105: Deterministic Quant Arithmetic](../draft/numeric/0105-deterministic-quant-arithmetic.md)
- [RFC-0110: Deterministic BIGINT](../accepted/numeric/0110-deterministic-bigint.md)
- [RFC-0111: Deterministic DECIMAL](../draft/numeric/0111-deterministic-decimal.md) (planned)
- [RFC-0903: Virtual API Key System](../final/economics/0903-virtual-api-key-system.md)
