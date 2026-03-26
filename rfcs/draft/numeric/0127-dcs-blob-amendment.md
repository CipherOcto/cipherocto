# RFC-0127 (Numeric): DCS Blob Amendment -- Deterministic Canonical Serialization

## Status

Draft (v6.6, adversarial review Round 8 candidate)

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

This RFC amends RFC-0126 (Deterministic Canonical Serialization) to add Blob as a first-class DCS type. Blob (binary data) is required by RFC-0201 for cryptographic hash storage (BYTEA(32)) and by RFC-0903/RFC-0909 for Virtual API Keys and Quota Accounting. While RFC-0126 defines `serialize_bytes` in pseudocode, Blob is absent from the Primitive Type Encodings table, Probe table, and Relationship table -- blocking RFC-0201 from advancing to Accepted status.

## Dependencies

**Required:**

- RFC-0126 (Numeric): Deterministic Canonical Serialization -- this RFC is an amendment to RFC-0126 Part 3

**Required By:**

- RFC-0201 (Storage): Binary BLOB Type for Hash Storage -- Blob requires a DCS entry to be Accepted
- RFC-0903 (Economics): Virtual API Key System -- `key_hash BYTEA(32)` needs Blob DCS entry
- RFC-0909 (Economics): Deterministic Quota Accounting -- `event_id BYTEA(32)` needs Blob DCS entry

## Motivation

RFC-0126 defines a 17-entry verification probe (Entries 0-16) for DCS cross-implementation verification. Blob is used by multiple dependent RFCs but has no entry in the probe table, no entry in the Primitive Type Encodings table, and is missing from the Relationship table.

Without a Blob entry in RFC-0126's type system, RFC-0201 cannot be Accepted (CRIT-3 from RFC-0201 adversarial review identified this dependency gap).

### RFC-0201 BYTEA(32) Suitability

RFC-0201 uses `BYTEA(32)` for SHA256/HMAC-SHA256 key hashes. The `serialize_blob` algorithm with `length=32` satisfies this use case. Schema-level enforcement of the 32-byte fixed length is the responsibility of the application layer (stoolap schema), not the DCS serialization layer. This is consistent with how other DCS types handle size constraints (e.g., DFP enforces precision via its encoding format; String enforces the 1MB limit at the application layer via DCS_STRING_LENGTH_OVERFLOW).

**Streaming and chunking:** Blob serialization is a single atomic operation -- `serialize_blob` produces a contiguous `u32_be(length) || data` output. The DCS layer does not define a streaming or chunked encoding; large Blob payloads are handled at the application layer (e.g., streaming I/O, chunked storage). The wire format is opaque to the DCS layer; applications MAY implement application-layer streaming for memory efficiency, but the canonical serialization of a Blob is always a single contiguous record. Implementations SHOULD support streaming decode for Blobs larger than a configurable memory threshold (e.g., > 1MB) to prevent allocation of the full payload into memory.

## Specification

### Changes to RFC-0126

The following changes apply to RFC-0126 v2.5.1 ("Deterministic Canonical Serialization").

#### Change 1: Primitive Type Encodings Table (Section Part 3, line ~323)

Add Blob to the table:

| Type | Format | Size |
|------|--------|------|
| `u8` | raw byte | 1 byte |
| `u32` | big-endian u32 | 4 bytes |
| `i128` | big-endian two's complement | 16 bytes |
| `bool` | `0x00` = false, `0x01` = true | 1 byte |
| `Blob` | `[length: u32BE][data: bytes]` | variable |
| `TRAP` | `0xFF` sentinel | 1 byte |

> **Note:** Blob uses identical length-prefix encoding to `serialize_bytes` defined in RFC-0126 SectionBytes (Raw).

#### Change 2: Bytes (Raw) Section Renamed to Blob (Section Part 3, line ~350)

Rename the section header from "Bytes (Raw)" to "Blob". The existing `serialize_bytes` function is retained as the low-level primitive. `serialize_blob` is defined as calling `serialize_bytes`:

```
serialize_blob(data: &[u8]) -> Result<Vec<u8>, DcsError> {
    if data.len() > 0xFFFFFFFF {
        return Err(DCS_BLOB_LENGTH_OVERFLOW)
    }
    return Ok(serialize_bytes(data))  // u32_be(data.len()) || data
}
```

**Result return type note (MED-1):** `serialize_blob` is the first DCS serialization function to return `Result<Vec<u8>, DcsError>`. Other serialization functions in RFC-0126 (`serialize_i128`, `serialize_dqa`, etc.) return `Vec<u8>` directly because their validity constraints are enforced at the type-system level (e.g., DQA canonical form guarantees a valid representation). The 4GB limit for Blob cannot be expressed as a type constraint -- it is a protocol enforcement -- so `serialize_blob` performs the check internally and returns an error rather than relying on the caller to pre-validate. This is consistent with the TRAP-before-serialize principle: the function itself is the last line of defense. `DcsError` is the same opaque error type used by all DCS error codes; the only possible error on the serialization path is `DCS_BLOB_LENGTH_OVERFLOW`.

- **Length prefix**: Big-endian u32 byte count (not character count)
- **Maximum length**: 4GB (2^32 - 1 bytes = 4,294,967,295 bytes = 0xFFFFFFFF) -- given by u32 length prefix. The maximum valid Blob has length = 0xFFFFFFFF bytes. **Error:** If `length > 0xFFFFFFFF`, return `Err(DCS_BLOB_LENGTH_OVERFLOW)`. The boundary is exclusive: length = 0xFFFFFFFF is valid; length = 0x100000000 is not.
- **Byte-identical to Bytes (Raw)**: The serialization format is identical; the rename reflects first-class type status
- **Public API boundary**: `serialize_blob` is the public, type-tagged entry point for the DCS Blob type. `serialize_bytes` is retained as a low-level primitive available for internal DCS use (DVEC, DMAT, Option) and for other RFCs requiring raw length-prefixed serialization. It MUST NOT be used as the serialization entry point for the Blob type in typed contexts. See RFC-0201 BYTEA(32) Suitability in the Motivation section for the primary use case.
- **Typed-context requirement**: Blob deserialization MUST only be invoked in a typed context (schema-driven dispatch). Bare Blob/String concatenation without type context is forbidden. A Blob field deserialized where a String is expected (or vice versa) produces an error. This prevents the semantic ambiguity described in NEW-KI-2. See Change 13 for a concrete dispatcher example. **Ambiguity symmetry:** When a schema contains both Blob and String types, String deserialization also MUST use the schema-driven dispatcher. This is because the wire format `[length][bytes]` is shared by both types; without dispatcher context, an implementation cannot determine whether to apply UTF-8 validation (String) or skip it (Blob). An implementation that uses bare `deserialize_string` on bytes when Blob exists in the schema produces consensus-divergent results compared to one that uses the dispatcher. **Implementation note:** Schema validation is recommended at compile time where possible (e.g., struct field type annotations in the schema definition). Dynamic schema environments SHOULD perform schema validation before deserialization begins to prevent misconfiguration from reaching the dispatcher.

#### Change 2.5: DCS Encoding Equivalence Classes (Normative)

DCS types are grouped into **encoding equivalence classes** based on their wire format. Two types in the same class share an identical binary encoding.

**Class: Length-Prefixed** -- types encoded as `u32_be(length) || payload`
- `String`: UTF-8 bytes, 1MB max
- `Blob`: arbitrary bytes, 4GB max
- Any future DCS type with format `u32_be(length) || payload`

Types in this class **share the same wire format** and are distinguishable only by schema context. The schema-driven dispatcher is **REQUIRED** to disambiguate them at deserialization time. Direct calls to `deserialize_string` or `deserialize_blob` on raw bytes in the presence of other Length-Prefixed types are not conformant.

**Class: Unambiguously Typed** -- types with self-identifying encodings that do not require a schema-driven dispatcher
- `u8`: 1 byte (unique value range)
- `u32`: 4 bytes (unique value range)
- `i128`: 16 bytes (unique value range, signed two's complement)
- `bool`: 1 byte (`0x00` vs `0x01`)
- `DFP`: 24 bytes (unique structure with NaN class encoding)
- `BigInt`: variable (per RFC-0110 limb encoding; distinguished by `0x01` version byte header)

Types in this class have unique encodings. The dispatcher is not required for these types; direct deserialization calls are conformant. **Note:** `BigInt` is included here for dispatcher-requirement purposes only -- it does not need the dispatcher because its `0x01` version byte header distinguishes it unambiguously from all length-prefixed types. Its size is variable per RFC-0110, not fixed-width.

**Class: Aggregate** -- compound types containing other types
- `DVEC`: `u32_be(length) || element_0 || element_1 || ...`
- `DMAT`: `u32_be(rows) || u32_be(cols) || element_0 || ...`
- `Struct`: `field_id_0 || value_0 || field_id_1 || value_1 || ...`

These types use length prefixes or field IDs internally. Whether they require the dispatcher depends on whether their element/field types belong to the Length-Prefixed class. A DVEC containing Strings requires the dispatcher; a DVEC containing i128 values does not. **DVEC/DMAT dispatcher requirement (MED-2):** For DVEC and DMAT, each element MUST be deserialized using the element type's deserialization function as determined by the container schema. A `DVEC<Blob>` MUST call `deserialize_blob` for each element; a `DVEC<String>` MUST call `deserialize_string`. The schema-driven dispatcher applies to each element individually. Implementers MUST NOT call `deserialize_string` on DVEC element bytes when the schema declares `DVEC<Blob>`, and vice versa.

This classification prevents future RFCs from introducing ambiguity: any new type must be assigned to an encoding class, and if it belongs to Length-Prefixed, the dispatcher requirement applies.

**Shared Encoding Formal Definition (HIGH-NEW-2):** Two DCS types A and B **share an encoding** (formally: `SharedEncoding(A, B)`) if and only if there exist valid values `x` and `y` such that:

```
serialize_A(x) == serialize_B(y)
```

That is, the byte output of serializing type A is byte-for-byte identical to the byte output of serializing type B for some pair of valid inputs. The dispatcher is **REQUIRED** when `∃ A, B: SharedEncoding(A, B) ∧ A ≠ B` in the same schema. For example, `SharedEncoding(Blob, String)` holds because `serialize_blob(b"hello") == serialize_string("hello")` -- both produce `0x0000000568656c6c6f`. The key insight is that shared encoding is a property of the wire format, not of specific values -- if ANY pair of valid values produces identical bytes, the wire formats are equivalent and the dispatcher is required.

#### Change 3: Probe Entries Table (Section Part 3, line ~625)

Add Entry 17 for Blob. Entries 0-16 are unchanged from RFC-0126 v2.5.1:

| Index | Type | Description | Input | Expected Serialization |
|-------|------|-------------|-------|----------------------|
| 0 | DQA | Positive canonicalization | `DQA(1000, 3)` -> canonicalize -> `DQA(1, 0)` | 8 bytes value + 1 byte scale + 7 bytes reserved |
| 1 | DQA | Negative canonicalization | `DQA(-5000, 4)` -> canonicalize -> `DQA(-5, 1)` | 8 bytes value + 1 byte scale + 7 bytes reserved |
| 2 | DVEC | Length + index ordering | `[1, 2, 3]` | `0x00000003` + 3x DQA elements |
| 3 | DMAT | Row-major traversal | `[[1, 2], [3, 4]]` (2x2) | rows + cols + 4x DQA elements |
| 4 | String | UTF-8 encoding | `"hello"` | `0x00000005` + UTF-8 bytes |
| 5 | Option | None | `None` | `0x00` |
| 6 | Option | Some(true) | `Some(true)` | `0x01` + `0x01` |
| 7 | Enum | Tagged variant (i128 payload) | `Variant2(42)` | `0x02` + i128 encoding of 42 (16 bytes) |
| 8 | Bool | True | `true` | `0x01` |
| 9 | Bool | False | `false` | `0x00` |
| 10 | TRAP | Numeric (24-byte) | Numeric TRAP | 24 bytes per RFC-0111 |
| 11 | TRAP | Bool (1-byte) | Invalid bool `0xFF` | `0xFF` (TRAP sentinel) |
| 12 | I128 | Positive | `42` | 16 bytes big-endian |
| 13 | I128 | Negative | `-42` | 16 bytes big-endian |
| 14 | BIGINT | Positive | `42` | RFC-0110 BigIntEncoding (16 bytes) |
| 15 | DFP | Positive Normal | `42.0` | RFC-0104 DfpEncoding (24 bytes) |
| 16 | Struct | Field ordering | `Person { id(field_id=1): 42, name(field_id=2): "alice", balance(field_id=3): DQA(1,0) }` | `u32_be(1) + u32_be(42) + u32_be(2) + serialize_string("alice") + u32_be(3) + serialize_dqa(1,0)` |
| 17 | Blob | Length prefix + data | `SHA256(b"")` = `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (32 bytes, not valid UTF-8) | `0x00000020 e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

> **Note:** Entry 4 (DMAT column-major) was removed because serialization output is indistinguishable for valid row-major input. DMAT input validation ensures data is stored row-major per RFC-0113.
>
> **Correction:** RFC-0126 v2.5.1 Entry 10 in the probe table incorrectly states "per RFC-0112." The authoritative source is RFC-0111, which is correctly cited in the RFC-0126 dependencies table and Entry 10 detail section. This amendment corrects the probe table reference to RFC-0111.
>
> **Wire format note:** Only Entry 16 (Struct) includes field_id in the wire format (`field_id || encoded_value`). Entries 0-15 and 17 are top-level type serializations without field_id prefixes.
>
> **Blob vs String distinction (CRIT-1):** Entry 17 uses `SHA256(b"")` as its payload -- 32 bytes that are NOT valid UTF-8. This ensures that passing Entry 17's bytes to `deserialize_string` returns `Err(DCS_INVALID_UTF8)`, verifying that the dispatcher correctly routes Blob and String fields to their respective deserializers. See NEW-KI-2 for the change history.

#### Change 4: Probe Entry 17 Details (Section Part 3, after Entry 16)

**Entry 17: Blob Serialization**

- Input: `SHA256(b"")` — the SHA-256 hash of the empty string, `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (32 bytes). This value is NOT valid UTF-8, which distinguishes it from Entry 4 (String `"hello"`).
- Serialize: `length=32 (4 bytes BE) || data=empty_string_SHA256`
- Expected bytes: `0x00000020 e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (36 bytes total)
- Leaf hash: `SHA256(0x00 || 0x00000020 e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855)` = `6452f4eb98d65e5ce04903cf5079038dfdb85ed742a4e543a52fca27b508a7ec`
- Verified via `scripts/compute_dcs_probe_root.py` (see Change 9)
- **Negative verification:** Passing Entry 17's bytes to `deserialize_string` MUST return `Err(DCS_INVALID_UTF8)`. An implementation that deserializes this as String is non-conformant.
  **UTF-8 invalidity verification:**
  ```
  python3 -c "
  data = bytes.fromhex('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855');
  try:
      data.decode('utf-8')
      print('VALID UTF-8 -- test vector is WRONG')
  except UnicodeDecodeError as e:
      print(f'Invalid UTF-8 at position {e.start}: {e.reason} -- test vector is correct')
  "
  ```
  Expected output: `Invalid UTF-8 at position N: <reason> -- test vector is correct`

```
serialize_blob(SHA256(b"")) =
    u32_be(32) || [0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55]
  = [0x00, 0x00, 0x00, 0x20, 0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f, 0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b, 0x78, 0x52, 0xb8, 0x55]
```

#### Change 5: Relationship Table (Section Part 3, SectionRelationship to Other RFCs)

Add Blob row. RFC-0201 is listed as a downstream consumer, not a peer dependency:

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | Decimal floating-point, DfpEncoding, deterministic NaN |
| RFC-0105 (DQA) | Canonicalization rules, TRAP sentinel |
| RFC-0110 (BIGINT) | Integer structure, little-endian limbs, BigIntEncoding |
| RFC-0112 (DVEC) | Vector structure, index ordering |
| RFC-0113 (DMAT) | Matrix structure, row-major ordering |
| RFC-0201 (Storage) | Required By / downstream consumer -- depends on this amendment for Accepted status; uses `serialize_blob` for BYTEA(32) |
| RFC-0903 (Economics) | Requires Blob DCS entry for Virtual API Key System key material storage |
| RFC-0909 (Economics) | Requires Blob DCS entry for Deterministic Quota Accounting |

#### Change 6: Implementation Checklist (Section Part 3, SectionImplementation Checklist)

Add Blob item:

- [ ] Serialize primitives (u8, u32, i128, bool)
- [ ] Serialize BIGINT with little-endian limbs (RFC-0110)
- [ ] Serialize DFP with DfpEncoding (RFC-0104)
- [ ] Serialize strings with UTF-8 validation
- [ ] Serialize Option types
- [ ] Serialize enums with tag dispatch
- [ ] Serialize DVEC with index ordering
- [ ] Serialize DMAT with row-major ordering
- [ ] Serialize Blob with length-prefix (Entry 17)
- [ ] Implement schema-driven dispatcher for Length-Prefixed types (Blob, String) **(REQUIRED for any schema containing both Blob and String fields; without this, deserialize calls are non-conformant per the shared-encoding rule)**
- [ ] Enforce nesting depth maximum of 64 levels; return `DCS_RECURSION_LIMIT_EXCEEDED` on violation
- [ ] Deserialize Blob with buffer validation and error conditions
- [ ] Deserialize String with buffer validation, 1MB limit, and RFC 3629 UTF-8 validation
- [ ] Canonicalize DQA before serialization
- [ ] Return error on invalid inputs before serialization
- [ ] Compute and verify Merkle probe root

#### Change 7: Error Handling Table (Section Part 3, SectionDCS Serialization Errors)

Split the pre-existing combined String/Bytes error into type-specific errors. Add the Blob-specific errors:

| Error | Condition |
|-------|-----------|
| DCS_INVALID_BOOL | Bool value not 0x00 or 0x01 |
| DCS_INVALID_SCALE | DQA scale > 18 |
| DCS_NON_CANONICAL | DQA value has trailing zeros (must canonicalize first) |
| DCS_OVERFLOW | DQA value exceeds i64 range after canonicalization |
| DCS_INVALID_UTF8 | String not valid UTF-8 |
| DCS_STRING_LENGTH_OVERFLOW | Declared string length (u32 length prefix) exceeds 1MB (1,048,576 bytes = 2^20). The check fires on the declared length value before buffer validation. |
| DCS_INVALID_STRING | String deserialization failed: input buffer too short for length prefix (fewer than 4 bytes available), or declared length exceeds remaining buffer bytes |
| DCS_INVALID_BLOB | Blob deserialization failed: input buffer too short for length prefix (fewer than 4 bytes available), or declared length exceeds remaining buffer bytes |
| DCS_BLOB_LENGTH_OVERFLOW | Blob length exceeds 2^32 - 1 bytes (4GB) |
| DCS_INVALID_STRUCT | Struct deserialization failed: buffer too short to read field_id (fewer than 4 bytes remaining), field_id in wire data does not match expected field_id in schema, or field produced zero bytes of progress |
| DCS_TRAILING_BYTES | Bytes remain after all schema-required fields have been deserialized, indicating trailing garbage in the input |
| DCS_RECURSION_LIMIT_EXCEEDED | Nesting depth exceeds the fixed maximum of 64 levels. Returned instead of crashing. All conformant implementations reject at exactly 64 levels. |

> **Note:** The prior combined `DCS_LENGTH_OVERFLOW` ("String/Bytes length exceeds 2^32 - 1") is replaced by two separate errors with distinct limits. String is capped at 1MB per RFC-0126 SectionString. Blob is capped at 4GB by the u32 length prefix. `DCS_INVALID_BLOB` covers buffer-underrun and length-mismatch conditions during deserialization. This change also resolves the pre-existing inconsistency between the error table (2^32-1) and the String section prose (1MB) in RFC-0126 v2.5.1.

#### Change 8: Deserialization (Section Part 3, after Existing Deserialization Rules)

Add Blob deserialization:

**Blob Deserialization**

**Notation:** All multi-byte integers use big-endian (network byte order). `input.len()` denotes the byte length of the input buffer. `input[a..b]` denotes the byte slice from index a (inclusive) to index b (exclusive). `as T` denotes type conversion to type T. **Cast semantics (HIGH-1 / CRIT-3):** `length as usize` converts the u32 length value to the platform's native pointer-width unsigned integer type for comparison against buffer lengths. On all platforms where DCS is intended to run (32-bit and 64-bit), this cast is safe and lossless -- u32::MAX (4,294,967,295) fits in usize on both 32-bit and 64-bit platforms. The safe bounds-check form is `4 + (length as usize) > input.len()`, which avoids unsigned subtraction underflow. The form `(length as usize) > input.len() - 4` is equivalent when the prior guard `input.len() < 4` has already returned an error, but is NOT safe if that guard were absent. Implementations MUST NOT omit or reorder the `input.len() < 4` guard. The preferred form `4 + (length as usize) > input.len()` is safe regardless of guard ordering and is used in the pseudocode below. **Bounds checks (HIGH-2):** UTF-8 validation uses `bytes.len() < i + N` (checking total available bytes against needed bytes) rather than `i + N >= bytes.len()` (checking last needed index against length). This form avoids index arithmetic overflow on constrained platforms and is idiomatic across language bindings.

```
 deserialize_blob(input: &[u8]) -> Result<(&[u8], &[u8]), Err> {
     if input.len() < 4 {
         return Err(DCS_INVALID_BLOB)  // need at least 4 bytes for length prefix
     }
     let length = (u32(input[0]) << 24) | (u32(input[1]) << 16) | (u32(input[2]) << 8) | u32(input[3]);
     if 4 + (length as usize) > input.len() {
         return Err(DCS_INVALID_BLOB)  // truncated: declared length exceeds remaining bytes
     }
     let data = input[4..4+(length as usize)];
     let remaining = input[4+(length as usize)..];
     return Ok((data, remaining))  // returns (blob_data, remaining_input_for_next_field)
 }
```

- **Minimum input**: 4 bytes (for the length prefix). If fewer than 4 bytes remain, return Err(DCS_INVALID_BLOB).
- **Length validation**: Declared length MUST NOT exceed remaining buffer bytes. If exceeded, return Err(DCS_INVALID_BLOB).
- **Empty Blob**: `length=0` is valid and returns an empty byte slice. This is NOT an error. The canonical form of an empty Blob is exactly `u32_be(0)` -- no payload bytes. This is distinct from null (which is not a DCS type) and from an absent optional field (which is a schema-layer concern).
- **Return type**: `Result<(&[u8], &[u8]), Err>` -- returns `(blob_data, remaining_bytes)` on success. The returned `blob_data` is a slice into the input buffer, not a newly allocated copy. **Allocation safety:** Deserializers MUST NOT pre-allocate a buffer of size `length` before validating that `length` bytes are available. The length check above prevents over-read; the return of a slice avoids a second allocation of the full blob data. Applications that need an owned copy MUST copy the slice explicitly. **Slice lifetime:** The returned slice MUST reference the original input buffer; the caller does not assume ownership or validity after the input buffer is freed. **Cross-language consistency (CRITICAL-NEW-2):** Returning a view/slice/span of the input buffer (rather than a copy) is the RECOMMENDED approach for Blob deserialization. Implementations SHOULD avoid copying Blob payloads to ensure consistent memory behavior across language bindings. Where this is not possible (e.g., languages without slice types), the semantic equivalent is a zero-copy view into the underlying buffer.
- **Typed-context enforcement**: Blob deserialization is only valid when the schema explicitly specifies a Blob field. Mixing Blob and String bytes without schema context produces indeterminate results and MUST be treated as an error condition by the caller.
- **UTF-8 acceptance**: Blob accepts any byte sequence, including valid UTF-8. This is not an error condition. Applications using Blob for binary data (e.g., cryptographic hashes) do not require UTF-8 validation. See NEW-KI-2 for the implications of byte-level Blob/String equivalence.

**String Deserialization**

**Notation:** Same as Blob Deserialization above (shared notation). **`cast_bytes_to_str(bytes)` (LOW-1):** A language-specific, zero-cost cast of a validated UTF-8 byte slice to a string reference type. In Rust: `std::str::from_utf8_unchecked(bytes)` (safe because UTF-8 validity was established by the validation loop above). In Go: `string(bytes)`. In Python: `bytes.decode('utf-8', errors='strict')`. No additional validation is performed -- the bytes have already been validated as UTF-8 by the loop above.

```
 deserialize_string(input: &[u8]) -> Result<(&str, &[u8]), Err> {
     if input.len() < 4 {
         return Err(DCS_INVALID_STRING)  // need at least 4 bytes for length prefix
     }
     let length = (u32(input[0]) << 24) | (u32(input[1]) << 16) | (u32(input[2]) << 8) | u32(input[3]);

     if length > 1_048_576 {  // max allowed string length = 1MB = 2^20 bytes; reject if declared length exceeds this
         return Err(DCS_STRING_LENGTH_OVERFLOW)
     }
     if 4 + (length as usize) > input.len() {
         return Err(DCS_INVALID_STRING)  // truncated: declared length exceeds remaining bytes
     }
     let bytes = input[4..4+(length as usize)];
     let remaining = input[4+(length as usize)..];
     // Validate UTF-8: decode each byte sequence and validate the resulting codepoint
     // per RFC 3629 (Unicode Standard Chapter 3 / UTF-8 scheme)
     let mut i = 0;
     while i < bytes.len() {
         let b1 = bytes[i];
         if b1 < 0x80 {
             // 1-byte sequence: U+0000 to U+007F (ASCII)
             i += 1;
         } else if (b1 & 0xE0) == 0xC0 {
             // 2-byte sequence: U+0080 to U+07FF
             if bytes.len() < i + 2 { return Err(DCS_INVALID_UTF8) }
             let b2 = bytes[i+1];
             if (b2 & 0xC0) != 0x80 { return Err(DCS_INVALID_UTF8) }
             let cp = ((b1 & 0x1F) as u32) << 6 | ((b2 & 0x3F) as u32);
             // Minimum check: overlong encoding rejected by requiring cp >= 0x80
             if cp < 0x80 { return Err(DCS_INVALID_UTF8) }
             i += 2;
         } else if (b1 & 0xF0) == 0xE0 {
             // 3-byte sequence: U+0800 to U+FFFF (except surrogates)
             if bytes.len() < i + 3 { return Err(DCS_INVALID_UTF8) }
             let b2 = bytes[i+1];
             let b3 = bytes[i+2];
             if (b2 & 0xC0) != 0x80 || (b3 & 0xC0) != 0x80 { return Err(DCS_INVALID_UTF8) }
             let cp = ((b1 & 0x0F) as u32) << 12 | ((b2 & 0x3F) as u32) << 6 | ((b3 & 0x3F) as u32);
             // Minimum check: overlong encoding rejected by requiring cp >= 0x800
             if cp < 0x800 { return Err(DCS_INVALID_UTF8) }
             // Surrogate check: U+D800 to U+DFFF must be rejected
             if cp >= 0xD800 && cp <= 0xDFFF { return Err(DCS_INVALID_UTF8) }
             i += 3;
         } else if (b1 & 0xF8) == 0xF0 {
             // 4-byte sequence: U+10000 to U+10FFFF
             if bytes.len() < i + 4 { return Err(DCS_INVALID_UTF8) }
             let b2 = bytes[i+1];
             let b3 = bytes[i+2];
             let b4 = bytes[i+3];
             if (b2 & 0xC0) != 0x80 || (b3 & 0xC0) != 0x80 || (b4 & 0xC0) != 0x80 { return Err(DCS_INVALID_UTF8) }
             let cp = ((b1 & 0x07) as u32) << 18 | ((b2 & 0x3F) as u32) << 12
                    | ((b3 & 0x3F) as u32) << 6 | ((b4 & 0x3F) as u32);
             // Minimum check: overlong encoding rejected by requiring cp >= 0x10000
             if cp < 0x10000 { return Err(DCS_INVALID_UTF8) }
             // Maximum codepoint check: U+10FFFF is the maximum valid Unicode codepoint
             if cp > 0x10FFFF { return Err(DCS_INVALID_UTF8) }
             i += 4;
         } else {
             return Err(DCS_INVALID_UTF8)  // invalid leading byte
         }
     }
     let s = cast_bytes_to_str(bytes);  // bytes validated as UTF-8 above; language-specific cast
     return Ok((s, remaining))  // returns (string_slice, remaining_input_for_next_field)
 }
```

- **Minimum input**: 4 bytes (for the length prefix). If fewer than 4 bytes remain, return Err(DCS_INVALID_STRING).
- **Length validation**: Declared length MUST NOT exceed 1MB. If exceeded, return Err(DCS_STRING_LENGTH_OVERFLOW).
- **UTF-8 validation**: The byte sequence is validated as UTF-8 per RFC 3629 at deserialization time. This includes: valid byte structure for each sequence length, rejection of overlong encodings (minimum codepoint per length), rejection of surrogate codepoints (U+D800–U+DFFF), and rejection of codepoints above U+10FFFF. If invalid, return Err(DCS_INVALID_UTF8). **Normalization:** Strings MUST NOT be normalized. Validation only checks UTF-8 correctness. The byte sequence is preserved exactly as provided -- no Unicode normalization (NFC, NFD, NFKC, NFKD) is applied. **Validation order:** UTF-8 validation occurs after type resolution. The dispatcher resolves the type (String) before calling `deserialize_string`; therefore `deserialize_string` always receives bytes that are intended to be a String. If the dispatcher first decodes as Blob and then attempts to re-decode as String, the UTF-8 validation must still be applied at the String layer. Implementations that skip UTF-8 validation because bytes were first interpreted as Blob produce consensus-divergent results.
- **Return type**: `Result<(&str, &[u8]), Err>` -- returns `(string_slice, remaining_bytes)` on success.
- **Allocation safety (MEDIUM-1):** Deserializers MUST NOT pre-allocate a buffer of `length` bytes before validating that `length` bytes are available in the input. The buffer validation check (`4 + (length as usize) > input.len()`) MUST occur before any allocation. This form avoids unsigned underflow -- see Notation note for cast semantics. **Cross-language consistency:** Returning a view/slice/span of the input buffer (rather than a copy) is the RECOMMENDED approach for String deserialization. Implementations SHOULD avoid copying String payloads to ensure consistent memory behavior across language bindings. Where this is not possible, the semantic equivalent is a zero-copy view into the underlying buffer.

#### Change 9: Published Merkle Root

The existing 17-entry Merkle Root (`2ed91a62f96f11151cd9211cf90aff36efc16c69d3ef910f4201592095abdaca`) was computed over entries 0-16. Adding Entry 17 changes the tree structure from odd (17 entries, last leaf duplicated) to even (18 entries, no duplication required).

**All 18 entry data and leaf hashes (for independent verification):**

| Index | Entry Data (hex) | Leaf Hash (SHA256 of 0x00 || entry_data) |
|-------|------------------|------------------------------------------|
| 0 | `00000000000000010000000000000000` | `5590b4a4eb4b7a9dba75b0176d06fbdabd8798d4b444741bb8efff24ad5b63f1` |
| 1 | `fffffffffffffffb0100000000000000` | `ad199dd0c6dc5752316d5e8318f37e777d4057d75a4a0f05cb8a491c7ee91b83` |
| 2 | `00000003000000000000000100000000000000000000000000000002000000000000000000000000000000030000000000000000` | `1cf1fbfbd91a87824796799064ca622b1e859e9918f6b5e81a2c1ff49c10c633` |
| 3 | `000000020000000200000000000000010000000000000000000000000000000200000000000000000000000000000003000000000000000000000000000000040000000000000000` | `c06c930dbec5070902aaad36e2a3c835926b05e2a8016b3d85eab217b98cbcfe` |
| 4 | `0000000568656c6c6f` | `01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6` |
| 5 | `00` | `96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7` |
| 6 | `0101` | `fbb59ed10e9cd4ff45a12c5bb92cbd80df984ba1fe60f26a30febf218e2f0f5e` |
| 7 | `020000000000000000000000000000002a` | `1cb5e27134ac530c5113543332f27d3bea126fdc7c8f42487e2b05afe3065af9` |
| 8 | `01` | `b413f47d13ee2fe6c845b2ee141af81de858df4ec549a58b7970bb96645bc8d2` |
| 9 | `00` | `96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7` |
| 10 | `01000000ff000000ffffffffffffffff8000000000000000` | `ff5a194d8b90088286a8c7f7de8de1ecc92e0c26c573b0e04bf8e6c0e9a507ed` |
| 11 | `ff` | `06eb7d6a69ee19e5fbdf749018d3d2abfa04bcbd1365db312eb86dc7169389b8` |
| 12 | `0000000000000000000000000000002a` | `170e5f45c1585c19f017f3c0df39c010e09004b0980fc8251ff4dd8eeef0376c` *(correction applied: prior entry was 63 hex chars, missing leading zero; see LOW-R4-1)* |
| 13 | `ffffffffffffffffffffffffffffffd6` | `340bdc8e30453799595c901721334ae5ff819a3e19f4ec6db4e6e9665454eb30` |
| 14 | `01000000010000002a00000000000000` | `ba9bc680540d876003d8a04ed12363e87af3567283f73c5b0127f5ad40314063` |
| 15 | `0000000000000000000000000000002a0000000000000000` | `7b0dc69a6bd9f3985e909871a6465971aef51b7c4b05051daefa0aa6d1b1fbc3` |
| 16 | `000000010000002a0000000200000005616c6963650000000300000000000000010000000000000000` | `8ce4a58171d93997bec1861d361b1bfae9a376027dd65f5cb5b045b27a1de890` |
| 17 | `00000020e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` | `6452f4eb98d65e5ce04903cf5079038dfdb85ed742a4e543a52fca27b508a7ec` |

**Entry 17 leaf hash:** `6452f4eb98d65e5ce04903cf5079038dfdb85ed742a4e543a52fca27b508a7ec`

**Verified computation:**

```
Entry 17 input bytes:  0x00 0x00 0x00 0x20 e3 b0 c4 42 98 fc 1c 14 9a fb f4 c8 99 6f b9 24 27 ae 41 e4 64 9b 93 4c a4 95 99 1b 78 52 b8 55
Domain-separated leaf:  SHA256(0x00 || 0x00000020 e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855)
                     = 6452f4eb98d65e5ce04903cf5079038dfdb85ed742a4e543a52fca27b508a7ec

Verification:         python3 -c "import hashlib; data = bytes([0x00]) + bytes.fromhex('00000020e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'); print(hashlib.sha256(data).hexdigest())"
Script encodings:     RFC-0110 (Entry 14), RFC-0104 (Entry 15), RFC-0111 (Entry 10)
```

**Level-1 intermediate hashes (for manual verification):**

| Pair | Leaves | Hash |
|------|--------|------|
| L1[0] | leaves 0+1 | `45a3d9a4ce06f12a8961c1f55e788372ad370520cc6d8c7a06ce68f1d351dc00` |
| L1[1] | leaves 2+3 | `ade4e53f84db243be465aa97107d9f941035d70523ad55ac27c1f3b353250b53` |
| L1[2] | leaves 4+5 | `3385fd1e5909d1a9a409e5dcee0466336ce9f8be808b8e62843a187e05ab2298` |
| L1[3] | leaves 6+7 | `5b7080d890c67187d45c4138afeedcde9e90a989ce3afdd19c1a0dbf63ba29b7` |
| L1[4] | leaves 8+9 | `36abd67d0ff5e19ce187ffdac3608bd0c928675a67431af1a35fe0f4519b4e50` |
| L1[5] | leaves 10+11 | `af12716eae513d040b0d7eb191fb2aaaf576e8d25270a1fce5f59b05a216b66f` |
| L1[6] | leaves 12+13 | `cbc981e0f89aea9cad248d9aeb5b3259cd676cdcaa5a03238c47c1cfe6901cb7` |
| L1[7] | leaves 14+15 | `b833c386f3132a57128334048b3b02bff6e8399b0b5d1ae4b5c6659ee9ac7fdf` |
| L1[8] | leaves 16+17 | `4225d6d111bc553c9e03e6a657e0ef29b934a24a88c361e2b66af2e228adcc9d` |

**18-entry Merkle Root:** `907f481e59ce67996f6c859c2cb6f8e5078245fee3baada58110489cdbdc0e47`

**Merkle tree pairing (18 leaves → 1 root):**
- Level 1 (9 pairs): L1[0]..L1[8] (see table above)
- Level 2 (5 pairs + 1 duplicate of L1[8]): L2[0]..L2[4]
- Level 3 (3 pairs + 1 duplicate of L2[4]): L3[0]..L3[2]
- Level 4 (2 pairs + 1 duplicate of L3[2]): L4[0], L4[1]
- Level 5: Root = `SHA256(0x01 || L4[0] || L4[1])` = `907f481e59ce67996f6c859c2cb6f8e5078245fee3baada58110489cdbdc0e47`

**Negative verification (CRIT-1):** Passing Entry 17's bytes to `deserialize_string` MUST return `Err(DCS_INVALID_UTF8)`. Implementations SHOULD test this. The 32 payload bytes `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` are not valid UTF-8.

> **Tree structure transition:** The Merkle tree over 17 entries has an odd leaf count. Per RFC-0126 SectionMerkle Root Computation, the last leaf (leaf_16) is duplicated for the final pair: `SHA256(0x01 || leaf_16 || leaf_16)`. Adding Entry 17 (leaf_17) brings the count to 18, which is even -- no duplication needed. This changes the internal node structure of the entire tree. The new root is not an incremental append; all prior entries' contributions to the root are affected by the changed pairing structure.

#### Change 10: Known Issues

Update the Known Issues table:

| ID | Description |
|----|-------------|
| MED-10 | Entries 5 (Option::None) and 9 (Bool false) produce identical leaf hashes (`96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7`). Domain-separated leaf hashing prevents Merkle root collision. Note: RFC-0126 v2.5.1 published `6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d` (SHA256 of raw `0x00` without domain separation), which was incorrect. The correct domain-separated hash is `96a296d2...`. |
| NEW-KI-1 | Blob entry (Entry 17) did not appear in RFC-0126 v2.5.1 Primitive Type Encodings table or Probe table. This amendment adds it. |
| NEW-KI-2 | Prior versions of this RFC (v6.3 and earlier) used `b"hello"` as Entry 17's blob payload, which is valid UTF-8 and produces identical entry data and leaf hash to Entry 4 (String `"hello"`). This was corrected in v6.5 by replacing the payload with `SHA256(b"")` -- a 32-byte sequence that is NOT valid UTF-8 -- ensuring the probe distinguishes Blob from String serialization. The old Entry 17 hash `01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6` is superseded. The Merkle root `78154bb3...` published in versions 6.2-6.4 was replaced in v6.5 by `907f481e59ce67996f6c859c2cb6f8e5078245fee3baada58110489cdbdc0e47` due to the Entry 17 payload replacement. **Encoding policy:** DCS intentionally allows byte-identical representations across distinct types. Type disambiguation is schema-driven and enforced at deserialization time; the wire format does not carry type information. The probe now correctly verifies that Blob serialization of non-UTF-8 data produces a distinct leaf hash from any String entry. |
| NEW-KI-3 | Adding Entry 17 changes the Merkle tree from odd (17, last leaf duplicated) to even (18, no duplication) leaf count. This structural change affects the root. See Change 9. |
| LOW-R4-1 | Entry 12 leaf hash had a transcription error in RFC-0127 v6.1 and earlier: displayed as 63 hex characters (missing a leading zero in byte 28: `0904b0` instead of `09004b0`). Corrected to the full 64-character value `170e5f45c1585c19f017f3c0df39c010e09004b0980fc8251ff4dd8eeef0376c` in v6.2. **Verification:** `python3 -c "import hashlib; data = bytes([0x00]) + bytes.fromhex('0000000000000000000000000000002a'); print(hashlib.sha256(data).hexdigest())"` yields `170e5f45c1585c19f017f3c0df39c010e09004b0980fc8251ff4dd8eeef0376c`. The Entry 12 correction alone did not change the Merkle root (the reference script computed from raw bytes). See NEW-KI-2 for the subsequent root change. |

#### Change 11: NUMERIC_SPEC_VERSION Increment

RFC-0110 defines `NUMERIC_SPEC_VERSION` as a `u32`:

```rust
const NUMERIC_SPEC_VERSION: u32 = 1;
```

RFC-0110 SectionVersion Increment Policy states the version MUST be incremented for "any change to canonical encoding formats."

Adding Blob as a new DCS type with a new serialization encoding constitutes a change to canonical encoding formats. Therefore:

- `NUMERIC_SPEC_VERSION` MUST be incremented to `2` upon ratification of this amendment.
- Implementations claiming conformance to both RFC-0110 and RFC-0126 with Blob support MUST declare `NUMERIC_SPEC_VERSION >= 2`.

**Activation governance:** RFC-0110 SectionVersion Increment Policy requires a minimum 2-epoch notice before activation at block H_upgrade, with a grace window for dual-version acceptance. This amendment does not set H_upgrade; a separate governance action per RFC-0110 policy is required to determine the activation block height. Block version signaling, state root computation during the grace window, and v1-node behavior when encountering Blob fields are specified in RFC-0110 SectionBlock Header Integration and SectionReplay Rules. Nodes MUST NOT reject version-1 blocks before the governance-declared H_upgrade, even after upgrading to support Blob.

**Activation checklist (for governance action):**
1. Determine H_upgrade block height with >= 2 epoch notice per RFC-0110
2. Set grace window [H_upgrade - grace, H_upgrade] for dual-version acceptance
3. Announce to node operators to upgrade before H_upgrade
4. At H_upgrade, nodes with NUMERIC_SPEC_VERSION >= 2 begin producing v2 blocks
5. After grace window, nodes still on v1 are subject to rejection per RFC-0110 SectionReplay Rules
6. After H_upgrade, non-upgraded nodes producing v1 blocks are out of consensus (LOW-2): they are rejected by upgraded nodes and MUST upgrade before they can rejoin consensus

#### Change 12: RFC-0126 Version Update

Upon merge of this amendment, RFC-0126 version MUST be incremented to **v2.6.0** and the following entry added to RFC-0126's version history:

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 2.6.0 | 2026-03-25 | CipherOcto | Added Blob as first-class DCS type (Entry 17), renamed Bytes (Raw) to Blob, split DCS_LENGTH_OVERFLOW into String/Blob-specific errors, added Blob deserialization, added deserialize_string with RFC 3629 UTF-8 validation, added schema-driven dispatcher (Change 13) as normative with SharedEncoding formal definition and DCS encoding equivalence classes, added DCS_INVALID_STRUCT, DCS_INVALID_BLOB, DCS_TRAILING_BYTES, DCS_RECURSION_LIMIT_EXCEEDED error codes, added probe extension protocol (Change 14), incremented NUMERIC_SPEC_VERSION to 2, corrected Entry 10 probe table reference from RFC-0112 to RFC-0111, corrected Known Issues leaf hash to domain-separated value |

#### Change 13: Schema-Driven Dispatcher Requirement (Normative)

RFC-0126 defines deserialization functions for Bool and Struct. Blob deserialization follows the same model: a schema-driven dispatcher invokes `deserialize_blob` only when the schema specifies a Blob field. The dispatcher tracks the expected type for each field and routes deserialization accordingly.

**Example dispatcher pseudocode:**

```
fn deserialize_field(input: &[u8], expected_type: Type) -> Result<(&[u8], Value), Err> {
    match expected_type {
        Type::String => {
            let result = deserialize_string(input);
            match result {
                Ok((v, rem)) => Ok((rem, Value::String(v))),
                Err(e) => Err(e),
            }
        },
        Type::Bool => {
            let result = deserialize_bool(input);
            match result {
                Ok((v, rem)) => Ok((rem, Value::Bool(v))),
                Err(e) => Err(e),
            }
        },
        Type::Blob => {
            let result = deserialize_blob(input);
            match result {
                Ok((v, rem)) => Ok((rem, Value::Blob(v))),
                Err(e) => Err(e),
            }
        },
        Type::Struct(inner_schema) => {
            let result = deserialize_struct(input, inner_schema, false, depth + 1);  // false = not top level, depth+1 for nested struct
            match result {
                Ok((rem, v)) => Ok((rem, v)),  // (remaining, value) ordering matches deserialize_field convention
                Err(e) => Err(e),
            }
        },
        // ... other types
    }
}

fn deserialize_struct(input: &[u8], schema: &StructSchema, is_top_level: bool, depth: usize) -> Result<(&[u8], Value), Err> {
    let mut remaining = input;
    let fields = empty list;
    // Empty struct: no fields to deserialize; wire must be empty at top level (zero bytes).
    // This is the only permitted zero-byte case; the progress check below does not apply.
    // Trailing-bytes check is only performed at the top level -- in nested contexts,
    // remaining bytes belong to the parent struct's subsequent fields.
    if schema.fields.is_empty() {
        if is_top_level && remaining.len() != 0 { return Err(DCS_TRAILING_BYTES); }
        return Ok((remaining, Value::Struct(fields)));
    }
    // Depth check: enforce fixed 64-level maximum
    if depth > 64 { return Err(DCS_RECURSION_LIMIT_EXCEEDED); }
    for field in schema.fields {  // fields in declaration order
        // Read field_id from wire (u32_be, 4 bytes) before calling deserialize_field
        if remaining.len() < 4 { return Err(DCS_INVALID_STRUCT); }
        let wire_field_id = (u32(remaining[0]) << 24) | (u32(remaining[1]) << 16)
                          | (u32(remaining[2]) << 8) | u32(remaining[3]);
        let remaining_after_field_id = remaining[4..];  // advance past field_id
        if wire_field_id != field.id { return Err(DCS_INVALID_STRUCT); }  // field ID mismatch
        let field_result = deserialize_field(remaining_after_field_id, field.type_);
        match field_result {
            Err(e) => return Err(e),  // propagate error
            Ok((new_remaining, value)) => {
                // Progress check: if new_remaining == remaining_after_field_id, the deserializer
                // consumed zero bytes from the value data -- this indicates malformed input
                if new_remaining == remaining_after_field_id {
                    return Err(DCS_INVALID_STRUCT);  // field value deserializer consumed zero bytes (malformed input or dispatcher logic error)
                }
                remaining = new_remaining;
                append (field.id, value) to fields;
            }
        }
    }
    // Check for trailing bytes at top level only: all schema fields consumed, no bytes should remain.
    // In nested calls, remaining bytes are returned to the parent for processing as subsequent fields.
    if is_top_level && remaining.len() != 0 {
        return Err(DCS_TRAILING_BYTES);
    }
    return Ok((remaining, Value::Struct(fields)));
}
// Note: Top-level callers MUST invoke as: let (remaining, value) = deserialize_struct(input, schema, true, 0)?;

```

**`is_top_level` and `depth` parameters (LOW-2):** `is_top_level` controls whether trailing bytes after the last field cause an error. Top-level callers (direct application deserialization) MUST pass `true`. Nested callers (from `deserialize_field`'s `Type::Struct` arm) MUST pass `false`. The distinction is necessary because in nested contexts, bytes following the inner struct belong to the outer struct's subsequent fields. `depth` tracks nesting depth starting at 0 for top-level calls and incrementing by 1 for each nested Struct. The depth check (`depth > 64`) is enforced at entry to each `deserialize_struct` call; a depth of 65 or higher returns `Err(DCS_RECURSION_LIMIT_EXCEEDED)`.

**Key properties:**

1. **Type tracking**: The dispatcher knows the expected type for each field from the schema. It never guesses based on bytes alone.
2. **Progress requirement**: Each field value deserialization MUST advance the buffer position. The check `new_remaining == remaining` detects zero-byte advancement, which indicates malformed input or a dispatcher logic error. The minimum advancement varies by type: fixed-width types (bool: 1 byte, i128: 16 bytes, DFP: 24 bytes) always advance by their fixed width; length-prefixed types (Blob, String) always advance by at least 4 bytes (the length prefix), plus payload bytes. Any non-zero advancement passes this check. See also: **Zero-byte type constraint** (below) -- the empty Struct is the only permitted exception to the non-zero advancement rule.
3. **Empty Blob note**: The progress check (`new_remaining != remaining`) validates that the length prefix was consumed (4 bytes). It does NOT validate payload presence. An empty Blob (`length=0`) consumes exactly 4 bytes (the length prefix) and passes this check -- this is correct behavior. The check guarantees forward progress, not data presence.
4. **No cross-type byte passing**: The bytes returned from one field's deserialization are passed to the NEXT field's deserializer, never back to a different-type deserializer. Mixing Blob/String bytes without schema context is impossible by construction.
5. **Error propagation**: Any deserialization error propagates immediately; partial results are discarded.

6. **Recursive example -- nested Blob in Struct**: Consider a struct with a Blob field:
    ```
    StructSchema {
        fields: [
            Field { id: 1, type: Blob },      // hash: BYTEA(32)
            Field { id: 2, type: String },   // name: VARCHAR
        ]
    }
    Wire: u32_be(1) || u32_be(32) || 32_bytes || u32_be(2) || serialize_string("alice")
    ```
    The top-level caller invokes `deserialize_struct(wire, schema, true, 0)`. For field 1 (Blob), the dispatcher calls `deserialize_field(remaining, Blob)`, which calls `deserialize_blob`. The 4-byte length prefix is consumed (advancing `remaining`), and 32 bytes are returned as a slice. For field 2 (String), the remaining bytes are passed to `deserialize_string`, which validates UTF-8 and returns the string slice. For a nested Struct, `deserialize_field` invokes `deserialize_struct(input, inner_schema, false, depth + 1)` -- the `false` flag prevents the trailing-bytes check from firing prematurely; `depth + 1` tracks the new nesting level. This recursion terminates at primitive types; the depth check at entry rejects pathological depths exceeding 64.

**Schema evolution rules:** The dispatcher operates on a schema agreed upon by all participants. Schema evolution rules are outside the DCS layer -- they are application-layer concerns. However, for conformance:

- **Unknown field in input**: The dispatcher does not skip unknown fields. If a field ID in the wire data has no corresponding entry in the local schema, the dispatcher returns error. Implementations MAY provide a "strict mode" vs "lenient mode" at the application layer, but the DCS dispatcher itself is strict. **Error code for unknown/missing fields (MED-2):** When a field ID mismatch is detected (wire field_id ≠ expected field_id), the dispatcher returns `DCS_INVALID_STRUCT`. Callers that need to distinguish schema-evolution mismatches from data corruption SHOULD inspect the wire field_id value: a wire_field_id that is a recognized ID from a newer schema version indicates a schema mismatch; an unrecognized ID indicates corruption. This distinction is application-layer logic outside the DCS dispatcher.
- **Missing field in input**: If the wire data ends before all schema-required fields are consumed, the dispatcher returns error (truncated input).
- **Extra data after last field**: If bytes remain after all schema fields are deserialized at the top level, the dispatcher returns `Err(DCS_TRAILING_BYTES)`. Conformant data has no trailing bytes. In nested contexts, remaining bytes belong to the parent struct's subsequent fields and are returned to the caller.
- **Field ordering (MEDIUM-NEW-1):** Wire data is structured as `field_id_0 || value_0 || field_id_1 || value_1 || ...` in **strictly ascending field_id order**. The dispatcher reads field_ids from the wire sequentially and matches them against the schema's expected field_ids. The wire order MUST be ascending field_id order. This is NOT declaration order -- declaration order is the order fields appear in the schema definition, which is coincidentally identical to ascending field_id order if the schema author assigned sequential IDs. The dispatcher does not use declaration index; it uses the wire field_id values to locate each field's data. A schema with non-sequential field_ids (e.g., 1, 3, 5) still serializes in ascending wire order.
- **Optional fields (HIGH-4):** The DCS dispatcher is strict and does not skip fields. Optional fields MUST be handled at the application layer before invoking `deserialize_struct`, e.g. by constructing a schema that only includes the fields present in the wire data, or by pre-processing the wire data to set absent optional fields to default values. The dispatcher itself iterates over `schema.fields` in declaration order and requires a wire_field_id for each expected field; it cannot skip an absent field and continue matching subsequent field_ids.
- **Versioning**: Schema versioning is handled at the application layer. The DCS dispatcher itself is stateless -- it applies the schema it is given without interpreting version numbers.

**Dispatcher recursion:** The dispatcher is recursive by nature. When deserializing a Struct field, `deserialize_struct` calls `deserialize_field` for each sub-field; when `deserialize_field` encounters a nested Struct type, it calls `deserialize_struct` again. This recursion terminates at primitive types (i128, bool, DQA, Blob, String).

**Recursion depth (CRITICAL-NEW-1 / HIGH-3):** All conformant implementations MUST reject inputs with nesting depth exceeding 64 levels with `Err(DCS_RECURSION_LIMIT_EXCEEDED)`. This is a fixed universal maximum, not a configurable minimum. While the termination invariant proves that valid inputs terminate (each recursive call consumes at least 1 byte), the fixed 64-level maximum additionally bounds worst-case stack depth in recursive implementations and ensures consistent rejection of pathological schemas. This is chosen to be large enough that no valid real-world input reaches it. The spec explicitly permits iterative implementations ("an iterative implementation is also valid but must produce identical results"). The DCS layer specifies behavioral output (correct deserialization), not the algorithm used to achieve it. A stack overflow caused by a recursive implementation without adequate stack limits is a **non-conformant implementation bug**, not a consensus split.

**Type recursion termination invariant (HIGH-NEW-4):** Dispatcher recursion is inherently bounded because every recursive call consumes at least 1 byte from the input buffer. This is guaranteed by: (1) the per-field progress check (`new_remaining != remaining`) which requires each Struct field to consume at least 4 bytes (the field_id), and (2) the Option tag byte (1 byte) which is consumed before recursing into the payload. A schema with recursive types (e.g., `Option<A>` where `A` contains `Option<A>`) terminates correctly because each Option level consumes at least 1 byte. An infinite recursion attack would require consuming zero bytes per recursive call, which is prevented by the progress check -- a dispatcher that recurses without consuming bytes returns `Err(DCS_INVALID_STRUCT)`. This is the **termination invariant**: dispatcher recursion MUST consume input bytes. If recursion occurs without consuming bytes, the result is an error, not an infinite loop. This prevents schema cycles, zero-byte recursion, and ensures termination for all valid inputs.

**Type definitions in pseudocode:** The types `Type`, `Value`, `StructSchema`, `Field`, `FieldId` shown above are schema concepts defined by the application. The dispatcher pattern is language-agnostic; implementations use their local type system.

This dispatcher pattern is how DCS deserialization is intended to be used. The alternative -- calling `deserialize_blob` or `deserialize_string` directly on raw bytes with no schema context -- is not conformant DCS usage.

**Conformance requirement:** Conformance to RFC-0126 with Blob support REQUIRES using a schema-driven dispatcher for any DCS type that shares an encoding with another DCS type. This is the **shared-encoding rule**: if two DCS types have identical wire formats, the dispatcher is REQUIRED to disambiguate them. Blob and String share the format `[u32_be(length)][bytes]`; therefore both MUST be deserialized via the dispatcher when both are present in the schema. Other DCS types with unique encodings (i128, bool, DQA, DFP, BigInt) MAY use direct deserialization calls. The dispatcher enforces the typed-context requirement; the requirement cannot be satisfied by prose alone.

**Error notation:** Pseudocode uses `return Err(ERROR_CODE)` for error returns. All error codes (`DCS_INVALID_STRING`, `DCS_STRING_LENGTH_OVERFLOW`, `DCS_INVALID_UTF8`, `DCS_INVALID_BLOB`, `DCS_BLOB_LENGTH_OVERFLOW`, `DCS_INVALID_STRUCT`, `DCS_TRAILING_BYTES`, `DCS_RECURSION_LIMIT_EXCEEDED`) denote deterministic error states that abort deserialization. Implementations MUST treat all error conditions as fatal.

**Zero-byte type constraint:** The progress check constrains future DCS type design: all DCS types used with this dispatcher MUST consume at least 1 byte during deserialization. Zero-byte types are incompatible with this dispatcher pattern. **Exception for empty Struct:** A Struct with zero fields (empty struct `{}`) consumes 0 bytes and therefore **requires special handling**: the dispatcher MUST detect the empty struct case before entering the per-field loop and return `Ok(Value::Struct(empty))` without applying the progress check. This is the only allowed zero-byte type; any new DCS type that consumes 0 bytes is non-conformant.

#### Change 14: Probe Extension Protocol (Normative)

Amendments adding new DCS types to the verification probe MUST follow this protocol:

1. **Append only**: New entries are added at the next sequential index (N+1, N+2, ...). Existing entries are never modified or reordered. **Existing entry leaf hashes are immutable once published:** A future amendment MAY NOT change the data or leaf hash of any prior entry published in a **ratified** RFC. If an error is found in a prior entry, a separate errata amendment MUST be issued, which increments NUMERIC_SPEC_VERSION and replaces the affected root. During the drafting process, entries MAY be corrected before ratification -- this immutability rule applies to entries in a ratified specification, not to draft entries under active development.
2. **Root recomputation**: The new entry changes the leaf count from odd to even or vice versa, which changes the pairing structure and thus the Merkle root. The new root MUST be computed and published in the amendment.
3. **Version increment**: Adding a new type constitutes a change to canonical encoding formats. `NUMERIC_SPEC_VERSION` MUST be incremented per RFC-0110 SectionVersion Increment Policy, including activation governance.
4. **Announcement**: The amendment MUST list all prior leaf hashes alongside the new entry so that the full tree can be independently verified without requiring the implementer to run prior versions of the script.

This ensures the probe is monotonically verifiable across amendments.

**Trade-off: Announcement size vs. verifiability.** Item 4 (Announcement) requires publishing all prior leaf hashes alongside each new entry. This grows linearly: entry N requires N-1 prior hashes. The reviewer correctly identified this as a scalability concern. The alternative — publishing only the new entry and prior Merkle root — would require implementers to trust that the prior root was correct, undermining independent verifiability. A structural solution (e.g., a vector commitment scheme enabling logarithmic or constant-size inclusion proofs) would eliminate linear growth but requires breaking changes to the Merkle tree structure defined in RFC-0126 — outside this amendment's scope. The linear growth is an explicit, acceptable trade-off: the probe advances slowly (only through RFC amendment), publication frequency is low, and the cost is borne by the amendment author once, not by implementers verifying the root. Implementers need only verify the root, not maintain the full publication history.

**Migration consideration (MEDIUM-NEW-4 resolved):** Vector commitment schemes (Merkle Mountain Ranges, Verkle trees) were considered for achieving logarithmic or constant-size inclusion proofs. This is **not adopted** for this amendment because: (1) it requires breaking changes to the Merkle tree structure defined in RFC-0126, which is beyond the scope boundary for a Blob amendment (the scope of this RFC is adding Blob as a DCS type, not redesigning the probe structure); (2) the linear growth is an explicit acceptable trade-off -- publication frequency is low (only through RFC amendment), the cost is borne once by the amendment author, and implementers need only verify the root not maintain publication history; (3) the trade-off section already documents this as an accepted cost. The probe extension protocol (Change 14) remains unchanged. This is a final scope-boundary decision, not a deferral -- future RFCs MAY revisit this if RFC-0126 is amended separately.

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 6.6 | 2026-03-26 | CipherOcto | Round 22 (Round 8 candidate): HIGH-1 remove stale "Intentional duplicate" note from probe table, replace with Blob vs String distinction note, HIGH-2 fix Type::Struct arm to pass v directly (not Value::Struct(v)), HIGH-3 change 64-level minimum to fixed universal maximum (all implementations reject at exactly 64 levels), MED-1 update deserialize_string allocation safety note to reference current bounds check form, MED-2 remove orphaned [^L-R4-1] footnote definition block, MED-3 clarify immutability rule applies to ratified entries only, LOW-1 add hex value to Entry 17 Input column, LOW-2 add normative is_top_level parameter prose, LOW-3 split LOW-R4-1 root change note into NEW-KI-2 |
| 6.5 | 2026-03-26 | CipherOcto | Round 21 (Round 7): CRIT-1 replace Entry 17 payload from b"hello" to SHA256(b"") (non-UTF-8, distinguishes Blob from String probe entry), CRIT-2 add recursion depth section with DCS_RECURSION_LIMIT_EXCEEDED error and 64-level maximum, CRIT-3 add Type::Struct case to deserialize_field dispatcher, HIGH-1 fix deserialize_blob bounds check to 4+(length as usize)>input.len(), HIGH-2 rename progress check variable to remaining_after_field_id, MED-1 add DVEC/DMAT element-type dispatcher guidance, MED-2 clarify serialize_blob returns DcsError, MED-3 expand optional fields prose (application layer handles absent fields), LOW-2 rename Fixed-Width Primitive class to Unambiguously Typed, LOW-3 add RFC-0903 and RFC-0909 rows to relationship table, LOW-4 verify v6.0 consolidation note already present |
| 6.4 | 2026-03-25 | CipherOcto | Round 20 (Round 6): HIGH-1 fix verification command to use bytes([0x00]) form for unambiguous byte construction, add expected output to verification, ensure both footnote and Known Issues entries are consistent, MED-1 add missing v6.2 version history row (was absent between v6.1 and v6.3), MED-2 remove non-standard footnote syntax, replace with inline annotation on Entry 12 row pointing to LOW-R4-1, MED-3 fix header round number from "round 20" to "Round 5 response" (was already corrected to Round 5 response before this review), LOW-1 clarify 1MB comment (max allowed length vs error threshold), LOW-2 update RFC-0126 v2.6.0 version history to include deserialize_string, dispatcher, new error codes, LOW-3 fix grammatically confused comment above progress check |
| 6.3 | 2026-03-25 | CipherOcto | Round 19 (Round 5): HIGH-1 confirm Entry 12 root unchanged (Scenario B: script computed from raw bytes), add footnote and Known Issues entry for correction, HIGH-2 fix header and footer to v6.2 (was v6.1), MED-1 fix Key Property 2 contradiction (replace "at least 1 byte" with precise zero-advancement detection), MED-2 add guidance on distinguishing schema mismatch vs data corruption via wire_field_id inspection, MED-3 differentiate DCS_INVALID_STRING and DCS_INVALID_BLOB descriptions with type-specific prefixes, LOW-1 add Known Issues entry for Entry 12 hash correction with verification command, LOW-2 Key Property 2 cross-reference to Zero-byte type constraint added, LOW-3 v6.2 footer update noted (6.1→6.2) |
| 6.2 | 2026-03-25 | CipherOcto | Round 18 (Round 4 adjudication): HIGH-1 add trailing-bytes check to deserialize_struct non-empty path, HIGH-2 clarify Key Property 2 progress requirement (minimum varies by type; zero-advancement detection), MED-1 correct Entry 12 leaf hash (63→64 hex chars, missing leading zero in byte 28, Scenario B confirmed: root unchanged), MED-2 update DCS_STRING_LENGTH_OVERFLOW description to specify declared vs actual length, MED-3 add clarifying note to BigInt Fixed-Width Primitive classification, LOW-1 update version footer 6.1→6.2, LOW-2 align deserialize_string allocation safety wording with Blob RECOMMENDED framing, LOW-3 add deserialize_string and schema-driven dispatcher to implementation checklist |
| 6.1 | 2026-03-25 | CipherOcto | Round 17 (independent review Round 3): HIGH-1 add explicit (length as usize) cast to bounds checks, HIGH-2 replace i+N>= with bytes.len()<i+N in UTF-8 validation, MED-1 add allocation safety and zero-copy notes to deserialize_string, MED-2 correct Entry 1 DQA hex (extra high byte removed), MED-3 expand DCS_INVALID_STRUCT description to cover all three conditions, MED-4 add immutability guarantee for published leaf hashes, LOW-1 define cast_bytes_to_str in notation, LOW-2 add step 6 to activation checklist (non-upgraded nodes out of consensus), LOW-3 replace unmaintainable v6.0 run-on with structured summary |
| 1.0 | 2026-03-25 | CipherOcto | Initial amendment draft -- adds Blob (Entry 17) to DCS type system |
| 2.0 | 2026-03-25 | CipherOcto | Adversarial review fixes: CRIT-1 compute 18-entry Merkle root, CRIT-2 split DCS_LENGTH_OVERFLOW into String/Blob-specific errors with distinct limits, HIGH-1 add typed-context deserialization requirement, HIGH-2 retain serialize_bytes as low-level primitive, HIGH-3 verify leaf hash via compute_dcs_probe_root.py, HIGH-4 add deserialize_blob algorithm, MED-2 document odd-to-even tree structure change, MED-4 add length prefix endianness prose, MED-5 fix relationship table direction, MED-3 confirm BYTEA(32) suitability, LOW-1 specify RFC-0126 v2.6.0 target, LOW-2 address NUMERIC_SPEC_VERSION increment, LOW-3 fix table formatting and preserve historical note |
| 3.0 | 2026-03-25 | CipherOcto | Round 2 fixes: HIGH-1 fix NUMERIC_SPEC_VERSION to u32 value 2 (not 2.0), MED-2 fix deserialize_blob return type and add DCS_INVALID_BLOB to error table, MED-3 add H_upgrade governance note, MED-1 publish all 18 leaf hashes and fix RFC-0111/RFC-0112 discrepancy, MED-4 add 4GB security consideration, LOW-1 document domain-separated hash correction for MED-10, LOW-3 change RFC-0201 label to (Storage), LOW-4 replace unwrap() with explicit bytes |
| 4.0 | 2026-03-25 | CipherOcto | Round 3: CRIT-1 rebuttal (Entry 17 bhello identical to String is intentional, tests wire-format collision), CRIT-2 rebuttal (error split is bug fix not breaking change), CRIT-3 rebuttal (dispatcher is DCS layer boundary), HIGH-3 rebuttal (DCS_INVALID_BLOB unified error is better for debugging), MED-1 fix Entry 16 table description to match Person struct, MED-3 add Change 13 with concrete schema-driven dispatcher pseudocode example, MED-3 add Change 14 with probe extension protocol, HIGH-1 clarify serialize_blob vs serialize_bytes public API boundary, HIGH-2 add activation checklist to NUMERIC_SPEC_VERSION governance note |
| 5.0 | 2026-03-25 | CipherOcto | Round 4: NEW-CRIT-1 make Change 13 normative (schema-driven dispatcher conformance required), NEW-CRIT-2 document empty Blob + progress-check interaction, NEW-CRIT-3 add field_id wire-format note to Entry 16 table header, NEW-HIGH-1 clarify serialize_bytes visibility for other RFCs, NEW-HIGH-2 rebuttal (String 1MB enforcement pre-existing RFC-0126 gap, scope), NEW-HIGH-3 rebuttal (block versioning governed by RFC-0110, scope), NEW-MED-1 make Change 14 normative (probe extension protocol), NEW-MED-2 add negative-deserialization limitation note to NEW-KI-2, NEW-MED-3 cross-reference to existing Motivation section, NEW-MED-4 remove duplicate v3.0 version history row, NEW-MED-5 document UTF-8 acceptance as intentional in Change 8, NEW-LOW-1 add type definition note to dispatcher pseudocode, NEW-LOW-2 add script version note, NEW-LOW-3 note deferred to editorial pass |
| 6.0 | 2026-03-25 | CipherOcto | Rounds 5-16 consolidated: added Blob (Entry 17) to type system, primitive type encodings, probe table, and relationship table; added serialize_blob and deserialize_blob with full pseudocode; added deserialize_string with RFC 3629 UTF-8 validation; renamed Bytes (Raw) to Blob; added DCS_INVALID_STRUCT, DCS_BLOB_LENGTH_OVERFLOW, DCS_INVALID_BLOB, DCS_TRAILING_BYTES to error table; made schema-driven dispatcher normative with conformance requirement; added SharedEncoding formal definition; added DCS encoding equivalence classes (Length-Prefixed, Fixed-Width, Aggregate); added recursion termination invariant and zero-byte-type exception for empty struct; added empty struct handling to dispatcher pseudocode; added field_id wire reading before deserialize_field; clarified wire order = ascending field_id; pinned reference script to commit 7b22f8a; published all 18 entry leaf hashes and new Merkle root; added probe extension protocol with immutability guarantee; added NUMERIC_SPEC_VERSION increment governance with activation checklist; added streaming/chunking guidance; added encoding policy statement; added schema evolution rules (unknown/missing/trailing fields, field ID mismatch, optional fields, versioning); corrected Entry 1 DQA hex; added cast semantics and bounds check notation notes; replaced all TRAP notation with explicit Err() returns; replaced u32::from_be_bytes with shift/or notation; added UTF-8 non-normalization rule; added explicit u32() casts; added DOMAIN_LEAF_PREFIX reference; added SHOULD recommendation for zero-copy view/span return; added formal type recursion termination invariant; added future migration clause (explicitly not adopted, scope decision); resolved linear growth trade-off explicitly; clarified 4GB boundary (exclusive); clarified dispatcher applies to Blob fields only; added ambiguity symmetry (String also requires dispatcher when Blob present); added explicit duplicate leaf hash comment |

## Related RFCs

- RFC-0126 (Numeric): Deterministic Canonical Serialization -- amended by this RFC
- RFC-0201 (Storage): Binary BLOB Type for Hash Storage -- blocked on this amendment for Accepted status
- RFC-0903 (Economics): Virtual API Key System -- requires Blob DCS entry
- RFC-0909 (Economics): Deterministic Quota Accounting -- requires Blob DCS entry

---

**Version:** 6.6
**Submission Date:** 2026-03-25
**Last Updated:** 2026-03-26
