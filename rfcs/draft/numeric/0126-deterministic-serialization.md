# RFC-0126 (Serialization): Deterministic Serialization

## Status

**Version:** 2.1 (2026-03-19)
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
| RFC-0105 (DQA) | Required | Defines DqaEncoding format, canonicalize rules, TRAP sentinel |
| RFC-0110 (BIGINT) | Required | Defines BigIntEncoding format |
| RFC-0112 (DVEC) | Required | Defines DVEC structure and ordering |
| RFC-0113 (DMAT) | Required | Defines DMAT structure, row-major ordering |

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

This RFC defines three complementary serialization systems:

### Part 1: Binary Serialization (Numeric Types)

For consensus-critical numeric types:

| Encoding | Authoritative RFC | Type | Size |
|----------|-------------------|------|------|
| I128Encoding | RFC-0110 | Integer | 16 bytes |
| BigIntEncoding | RFC-0110 | Arbitrary Integer | 8-520 bytes |
| DqaEncoding | RFC-0105 | Decimal | 16 bytes |
| DfpEncoding | RFC-0104 | Floating-Point | 24 bytes |

### Part 2: JSON Serialization (Structured Data)

For non-consensus data that requires deterministic representation (e.g., API metadata, configuration), this RFC defines canonical JSON serialization rules.

### Part 3: Deterministic Canonical Serialization (DCS)

For cross-language, consensus-critical serialization of primitive and composite types. DCS provides:
- Canonical encoding (exactly one valid byte representation)
- Deterministic ordering (struct fields declared order, arrays by index, matrices row-major)
- TRAP-before-serialize semantics (invalid inputs cannot be serialized)
- Merkle-committed verification probe for cross-implementation verification

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
- **class_sign bit layout**: `[class:8][sign:8][reserved:16]`
  - class (bits 24-31): 0=Normal, 1=Infinity, 2=NaN, 3=Zero
  - sign (bits 16-23): 0=positive, 1=negative
  - reserved (bits 0-15): MUST be 0x0000
- **Version**: Implicit v1

### Cross-Type Ambiguity Prevention

Each type's encoding is structurally distinct, preventing Merkle hash collisions:

| Type A | Type B | Encoding Difference |
|--------|--------|---------------------|
| DQA(1.0) | BIGINT(1) | DQA: 16 bytes with scale; BIGINT: variable header + limbs |
| DFP(1.0) | BIGINT(1) | DFP: 24 bytes with class_sign; BIGINT: variable header + limbs |
| DQA(1) | I128(1) | DQA: 16 bytes with scale field; I128: 16 bytes raw |

### Version Byte Handling

| Type | Version | Notes |
|------|---------|-------|
| I128 | None (implicit) | Fixed 16-byte format |
| BIGINT | Byte 0 = version | Currently 0x01; unknown versions TRAP |
| DQA | None (implicit) | Fixed 16-byte format |
| DFP | None (implicit) | Fixed 24-byte format |

**BIGINT Exception:** Variable-length limb array requires version byte for future extensibility. Fixed-size types (I128, DQA, DFP) do not need versioning.

### Endianness

**Wire Format:** All multi-byte integers use big-endian (network byte order).

**Host Memory:** This is independent of host CPU endianness. Implementations MUST:
- Use `to_be_bytes()` / `from_be_bytes()` for serialization
- NOT use `memcpy` to copy struct memory directly
- NOT assume host memory layout matches wire format

**Cross-Platform:** Serialization output must be identical on little-endian and big-endian hosts.

## Part 2: JSON Serialization (Structured Data)

### Overview

For structured data (non-consensus) that requires deterministic representation (e.g., API metadata, configuration), this RFC defines canonical JSON serialization rules.

### JSON Allowed Contexts

JSON serialization (Part 2) is allowed in:

| Context | Example | Notes |
|---------|---------|-------|
| API request/response | REST, GraphQL | Non-consensus |
| Configuration files | config.json | Non-consensus |
| Cross-chain messages | Bridge events | Verify per-chain format |
| Oracle data feeds | Price feeds | Must verify format |

JSON serialization is FORBIDDEN in:

| Context | Reason |
|---------|--------|
| Consensus state | Use Part 1 binary encoding |
| Merkle tree leaves | Use DCS (Part 3) |
| Cryptographic proofs | Use canonical binary |

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

## Part 3: Deterministic Canonical Serialization (DCS)

### Overview

DCS provides a canonical, deterministic serialization format for all protocol data structures used in consensus-critical contexts. Unlike Parts 1 and 2 which focus on numeric type encoding, DCS defines serialization rules for **all** primitive and composite types with cross-language determinism guarantees.

### Core Principles (NON-NEGOTIABLE)

1. **Canonical Encoding**: Every serializable value has exactly one valid byte representation. No alternative encodings allowed.

2. **Deterministic Ordering**:
   - Struct fields: **declared order** (not alphabetical, not sorted)
   - Arrays: **index order** (element 0, then 1, then 2...)
   - Matrices: **row-major order** (RFC-0113)

3. **Length Prefixing**: All variable-length types use `u32_be` for length prefix.

4. **Big-Endian Encoding**: All integers use big-endian byte order.

5. **No Floating-Point**: Floating-point types (f32, f64, DFP) are **FORBIDDEN** in DCS serialization. Use DQA for decimal representations.

6. **TRAP-Before-Serialize**: Invalid inputs (overflow, NaN, invalid states) **MUST NOT** be serialized. Instead, they TRAP before serialization is attempted.

### Primitive Type Encodings

| Type | Format | Size |
|------|--------|------|
| `u8` | raw byte | 1 byte |
| `u32` | big-endian u32 | 4 bytes |
| `i128` | big-endian two's complement | 16 bytes |
| `bool` | `0x00` = false, `0x01` = true | 1 byte |
| `TRAP` | `0xFF` sentinel | 1 byte |

> **Bool TRAP**: Only `0x00` and `0x01` are valid. Any other byte value TRAPs before serialization.

### Composite Serialization

#### String

```
 serialize_string(s: &str) -> Vec<u8> {
     let utf8_bytes = s.as_bytes();
     u32_be(utf8_bytes.len()) || utf8_bytes
 }
```

- UTF-8 encoding required
- Length prefix is byte count, not character count
- **Maximum length**: 1MB (2²⁰ bytes) for all string serialization
- **TRAP**: If length > 1MB, TRAP(LENGTH_OVERFLOW)
- For strings exceeding 1MB, use out-of-band chunking (not defined in this RFC)

#### Bytes (Raw)

```
 serialize_bytes(data: &[u8]) -> Vec<u8> {
     u32_be(data.len()) || data
 }
```

#### Option<T>

```
 serialize_option_none() -> Vec<u8> {
     [0x00]  // 1 byte: 0x00 indicates None
 }

 serialize_option_some(payload: &[u8]) -> Vec<u8> {
     [0x01] || payload  // 1 byte: 0x01 indicates Some, followed by serialized payload
 }
```

**Type Context Requirement:** Option serialization MUST be within a typed container.

`Option<DQA>::None` and `Option<STRING>::None` both encode as `0x00`. This is safe when:
1. The Option is a field within a struct with explicit type context
2. OR the Option is the top-level serialized value with type context implied by protocol

**Unsafe scenarios:**
- Concatenating `Option<DQA>::None || Option<STRING>::None` produces ambiguous bytes
- This is prohibited unless additional type tags are added

**Recommendation:** Always use Option within typed structs. Do not concatenate bare Option values.

#### Enum (Tagged Union)

```
 serialize_enum(tag: u8, payload: &[u8]) -> Vec<u8> {
     [tag] || payload  // 1 byte tag, followed by serialized payload
 }
```

- Tag values: 0-255
- Payload serialization depends on variant

#### DVEC (Deterministic Vector)

```
 serialize_dvec<T: Serialize>(elements: &[T]) -> Vec<u8> {
     let mut result = u32_be(elements.len());  // length prefix
     for element in elements {
         result.extend(serialize(element));  // index order
     }
     result
 }
```

- Length prefix: u32_be (number of elements)
- Elements serialized in index order (0, 1, 2...)
- Per RFC-0112: DVEC ordering is by index

**Length Prefix Contexts:**
- Production wire format: `u32_be(elements.len())` — supports up to 2³²-1 elements
- Verification probe (RFC-0112): 1-byte length — limited to 255 elements per probe constraint
- Production limit: N ≤ 64 (per RFC-0112 §Production Limitations)

#### DMAT (Deterministic Matrix)

**DMAT Ordering Invariant (per RFC-0113):**

The data array MUST be in row-major order. Serialization assumes:
- element(i, j) = data[i * cols + j]
- Row 0: elements 0 to cols-1
- Row 1: elements cols to 2*cols-1
- etc.

**Input Validation:** If input data is not in row-major order, the caller MUST reorder before serialization, or TRAP with ORDER_ERROR.

```
 serialize_dmat<T: Serialize>(rows: usize, cols: usize, elements: &[T]) -> Vec<u8> {
     let mut result = Vec::new();
     result.extend(u32_be(rows));     // rows count
     result.extend(u32_be(cols));     // columns count
     // Row-major traversal: elements[0..cols] is row 0, elements[cols..2*cols] is row 1, etc.
     for element in elements {
         result.extend(serialize(element));
     }
     result
 }
```

- Per RFC-0113: Row-major layout (elements stored row by row)
- Index formula: `element(i, j) = elements[i * cols + j]`

### DQA Serialization (per RFC-0105)

**CRITICAL: SQL Storage vs Consensus Serialization**

RFC-0105 defines two distinct contexts:

| Context | Canonicalization | Reference |
|---------|-----------------|-----------|
| Consensus/state hashing | MUST canonicalize | RFC-0126 §serialize_dqa |
| SQL column storage | Retain column scale | RFC-0105 §SQL Column Scale Semantics |
| VM intermediate registers | MAY defer | RFC-0105 §Lazy Canonicalization |

**SQL Storage Rule:** Values stored in SQL columns retain the column's declared scale, NOT the canonical form. A value like `1.200000` with column scale 6 is stored as `{value: 1200000, scale: 6}`, not canonicalized to `{value: 12, scale: 1}`.

**Consensus Serialization Rule:** Before serialization for state hashing or Merkle computation, DQA values MUST be canonicalized per RFC-0105 §Canonical Representation.

Implementations MUST NOT canonicalize SQL column values before storage.

```
 serialize_dqa(value: i128, scale: u8) -> Vec<u8> {
     // CRITICAL: Canonicalize BEFORE serialization
     let (canon_value, canon_scale) = canonicalize_dqa(value, scale);

     let mut result = Vec::new();
     // value: 16 bytes, big-endian two's complement
     result.extend(canon_value.to_be_bytes());
     // scale: 1 byte
     result.push(canon_scale);
     // reserved: 7 bytes (must be zero)
     result.extend([0u8; 7]);

     result
 }

 canonicalize_dqa(value: i128, scale: u8) -> (i128, u8) {
     // Per RFC-0105 §Canonical Representation
     if value == 0 {
         return (0, 0);
     }
     // Strip trailing zeros
     let mut v = value;
     let mut s = scale;
     while v % 10 == 0 && s > 0 {
         v /= 10;
         s -= 1;
     }
     (v, s)
 }
```

**TRAP Conditions:**
- `scale > 18`: TRAP(INVALID_SCALE)
- Trailing zeros in non-zero value: MUST canonicalize before serialization
- Value does not fit in i64 after canonicalization: TRAP(OVERFLOW)

### TRAP Sentinel Serialization

**TRAP Sentinel Definitions (Cross-RFC Reference)**

| Context | Type | TRAP Encoding | Reference |
|---------|------|---------------|-----------|
| Numeric operations (DQA, DVEC, DMAT) | Scalar | 24-byte: `[version:1=0x01][scale:1=0xFF][reserved:3=0x00][mantissa:16=i64::MIN]` | RFC-0112 §TRAP Sentinel |
| Non-numeric DCS (bool, enum tags) | Primitive | 1-byte: `0xFF` | RFC-0126 |
| BIGINT | Special | 48-byte: `[0xDEAD...]` pattern | RFC-0110 §TRAP |

> **Note**: TRAP values should not reach serialization. They TRAP at the point of detection. The TRAP sentinel is used only in probe encodings where an operation's result is an error state.

**Numeric TRAP Encoding (RFC-0112/RFC-0105):**
```
 serialize_trap_numeric() -> Vec<u8> {
     // 24-byte format per RFC-0112
     // version = 0x01, scale = 0xFF, mantissa = i64::MIN (0x8000000000000000)
     [0x01] || [0xFF] || [0x00, 0x00, 0x00] || i64::MIN.to_be_bytes()
 }
```

**Non-Numeric TRAP Encoding (RFC-0126):**
```
 serialize_trap_primitive() -> Vec<u8> {
     [0xFF]  // 1 byte: TRAP sentinel for bool, enum tags
 }
```

#### Bool Deserialization

**Valid values:** Only `0x00` (false) and `0x01` (true) are valid.

**Deserialization behavior for invalid values:**
- 收到 `0xFF` or any other byte ≠ `0x00` or `0x01`:
  - MUST TRAP immediately (deterministic failure)
  - No partial deserialization, no error return
- This applies to all bool fields in composite types

**TRAP vs Error:** Bool deserialization always TRAPs on invalid input. There is no error return distinction.

### Verification Probe

DCS includes a 15-entry Merkle-committed verification probe for cross-implementation verification.

#### Probe Entry Format

Each entry is serialized as a leaf in a Merkle tree:

```
leaf = SHA256(entry_data)
root = MerkleRoot(leaf_0, leaf_1, ..., leaf_14)
```

#### Probe Entries

| Index | Type | Description | Input | Expected Serialization |
|-------|------|-------------|-------|----------------------|
| 0 | DQA | Positive canonicalization | `DQA(1000, 3)` → canonicalize → `DQA(1, 0)` | 16 bytes value + 1 byte scale + 7 bytes reserved |
| 1 | DQA | Negative canonicalization | `DQA(-5000, 4)` → canonicalize → `DQA(-5, 1)` | 16 bytes value + 1 byte scale + 7 bytes reserved |
| 2 | DVEC | Length + index ordering | `[1, 2, 3]` | `0x00000003` + 3× DQA elements |
| 3 | DMAT | Row-major traversal | `[[1, 2], [3, 4]]` (2×2) | rows + cols + 4× DQA elements |
| 4 | String | UTF-8 encoding | `"hello"` | `0x00000005` + UTF-8 bytes |
| 5 | Option | None | `None` | `0x00` |
| 6 | Option | Some(true) | `Some(true)` | `0x01` + `0x01` |
| 7 | Enum | Tagged variant | `Variant2(42)` | `0x02` + `serialize(42)` |
| 8 | Bool | True | `true` | `0x01` |
| 9 | Bool | False | `false` | `0x00` |
| 10 | TRAP | Numeric (24-byte) | Numeric TRAP | 24 bytes per RFC-0112 |
| 11 | TRAP | Bool (1-byte) | Invalid bool `0xFF` | `0xFF` (TRAP sentinel) |
| 12 | I128 | Positive | `42` | 16 bytes big-endian |
| 13 | I128 | Negative | `-42` | 16 bytes big-endian |
| 14 | (reserved) | Future extension | - | - |

**Note:** Entry 4 (DMAT column-major) was removed because serialization output is indistinguishable for valid row-major input. DMAT input validation ensures data is stored row-major per RFC-0113.

#### Merkle Root Computation

```
fn merkle_root(leaves: Vec<[u8; 32]>) -> [u8; 32] {
    // Pairwise hashing until single root
    // If odd number, duplicate last leaf
    let mut current_level = leaves;
    while current_level.len() > 1 {
        let mut next_level = Vec::new();
        for pair in current_level.chunks(2) {
            if pair.len() == 2 {
                next_level.push(sha256(pair[0] || pair[1]));
            } else {
                // Duplicate last element
                next_level.push(sha256(pair[0] || pair[0]));
            }
        }
        current_level = next_level;
    }
    current_level[0]
}
```

> **Published Merkle Root:** `0bfbb7e404c9a1412f3339a0d7515fa496b262b514a340484e2445512003cf85`

#### Probe Entry Details

**Entry 0: DQA Canonicalization (Positive)**
- Input: `DQA(1000, 3)`
- Canonicalize: `1000 × 10^-3 = 1.0 → DQA(1, 0)` (strip trailing zeros)
- Serialize: `value=1 (16 bytes BE) || scale=0 || reserved=7 bytes zero`

**Entry 1: DQA Canonicalization (Negative)**
- Input: `DQA(-5000, 4)`
- Canonicalize: `-5000 × 10^-4 = -0.5 → DQA(-5, 1)` (strip trailing zeros: -5000→-500→-50→-5)
- Serialize: `value=-5 (16 bytes BE) || scale=1 || reserved=7 bytes zero`

**Entry 2: DVEC Serialization**
- Input: `DVEC [DQA(1,0), DQA(2,0), DQA(3,0)]`
- Serialize: `length=3 (4 bytes) || DQA(1,0) || DQA(2,0) || DQA(3,0)`

**Entry 3: DMAT Serialization (Row-Major)**
- Input: `DMAT 2×2 [[DQA(1,0), DQA(2,0)], [DQA(3,0), DQA(4,0)]]`
- Layout: `[1, 2, 3, 4]` (row 0: [1,2], row 1: [3,4])
- Serialize: `rows=2 (4 bytes) || cols=2 (4 bytes) || DQA(1,0) || DQA(2,0) || DQA(3,0) || DQA(4,0)`

**Entry 4: String Serialization**
- Input: `"hello"`
- UTF-8: `[0x68, 0x65, 0x6C, 0x6C, 0x6F]`
- Serialize: `length=5 (4 bytes) || UTF-8 bytes`

**Entry 5: Option::None**
- Serialize: `0x00`

**Entry 6: Option::Some(true)**
- Some: `0x01 || serialize(true)`
- Serialize: `0x01 || 0x01`

**Entry 7: Enum::Variant2(42)**
- Tag: `2`
- Payload: `serialize(42)` = `0x0000002A` (u32 big-endian)
- Serialize: `0x02 || 0x0000002A`

**Entry 8: TRAP Case (Invalid Bool)**
- Input: `0xFF` (not 0x00 or 0x01)
- TRAP at validation, serialize TRAP sentinel
- Serialize: `0xFF`

### Cross-Language Determinism Guarantees

To ensure identical serialization across implementations:

1. **No Ambiguous Types**: Every type has explicit wire format
2. **Fixed Size Primitives**: All primitive sizes are platform-independent
3. **Explicit Ordering**: Struct field order is declaration order, not hash/sorted order
4. **No Pointers**: Serialization produces flat byte sequences, not pointer chains
5. **Validation Before Serialization**: Invalid states TRAP, cannot be serialized

### Implementation Checklist

- [ ] Serialize primitives (u8, u32, i128, bool)
- [ ] Serialize strings with UTF-8 validation
- [ ] Serialize Option types
- [ ] Serialize enums with tag dispatch
- [ ] Serialize DVEC with index ordering
- [ ] Serialize DMAT with row-major ordering
- [ ] Canonicalize DQA before serialization
- [ ] TRAP on invalid inputs before serialization
- [ ] Compute and verify Merkle probe root

### Relationship to Other RFCs

| RFC | Relationship |
|-----|--------------|
| RFC-0105 (DQA) | Canonicalization rules, TRAP sentinel |
| RFC-0112 (DVEC) | Vector structure, index ordering |
| RFC-0113 (DMAT) | Matrix structure, row-major ordering |

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

### DCS Serialization Errors

| Error | Condition |
|-------|-----------|
| DCS_INVALID_BOOL | Bool value not 0x00 or 0x01 |
| DCS_INVALID_SCALE | DQA scale > 18 |
| DCS_NON_CANONICAL | DQA value has trailing zeros (must canonicalize first) |
| DCS_OVERFLOW | i128 value does not fit in i64 after canonicalization |
| DCS_INVALID_UTF8 | String not valid UTF-8 |
| DCS_LENGTH_OVERFLOW | String/Bytes length exceeds 2^32 - 1 |

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
| 2.0 | 2026-03-19 | Added Part 3: Deterministic Canonical Serialization (DCS), 9-entry Merkle probe |
| 2.1 | 2026-03-19 | Adversarial review fixes: TRAP sentinel unification, DFP bit layout, SQL storage exception, probe expansion to 15 entries |

### Known Issues

None.

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-03-16 | CipherOcto | Initial draft with duplicated definitions |
| 1.1 | 2026-03-16 | CipherOcto | Removed duplication - references authoritative RFCs |
| 1.2 | 2026-03-16 | CipherOcto | Added JSON Serialization (Part 2) for RFC-0903 |
| 2.0 | 2026-03-19 | CipherOcto | Added Part 3: Deterministic Canonical Serialization (DCS) |
| 2.1 | 2026-03-19 | CipherOcto | Adversarial review fixes: CRIT-1/2/3/4, HIGH-1/2/3/4/5, MED-1/2/3 |

## Compatibility

### Binary Serialization

- Version byte (where present) allows format evolution
- Old versions TRAP on deserialization
- Forward compatibility not guaranteed

### JSON Serialization

- Implementations MUST produce canonical output
- Consumers MAY accept non-canonical but SHOULD reject
- Version negotiation not supported

### DCS Serialization

- All types have canonical form - no alternative representations allowed
- Invalid inputs TRAP before serialization (cannot produce non-canonical output)
- Merkle probe root provides cross-implementation verification

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
- [RFC-0105: Deterministic Quant Arithmetic](../accepted/numeric/0105-deterministic-quant-arithmetic.md)
- [RFC-0110: Deterministic BIGINT](../accepted/numeric/0110-deterministic-bigint.md)
- [RFC-0111: Deterministic DECIMAL](../draft/numeric/0111-deterministic-decimal.md) (planned)
- [RFC-0112: Deterministic Vectors](../accepted/numeric/0112-deterministic-vectors.md)
- [RFC-0113: Deterministic Matrices](../accepted/numeric/0113-deterministic-matrices.md)
- [RFC-0903: Virtual API Key System](../final/economics/0903-virtual-api-key-system.md)
- [DCS Probe Script: compute_dcs_probe_root.py](../../scripts/compute_dcs_probe_root.py)
