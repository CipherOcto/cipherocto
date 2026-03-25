# RFC-0127 (Numeric): DCS Blob Amendment -- Deterministic Canonical Serialization

## Status

Draft (v6, adversarial review round 10 -- Grok external review, all rounds consolidated into v6.0)

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
serialize_blob(data: &[u8]) -> Result<Vec<u8>, Err> {
    if data.len() > 0xFFFFFFFF {
        return Err(DCS_BLOB_LENGTH_OVERFLOW)
    }
    return Ok(serialize_bytes(data))  // u32_be(data.len()) || data
}
```

**Result return type note:** `serialize_blob` is the first DCS serialization function to return `Result<Vec<u8>, Err>`. Other serialization functions in RFC-0126 (`serialize_i128`, `serialize_dqa`, etc.) return `Vec<u8>` directly because their validity constraints are enforced at the type-system level (e.g., DQA canonical form guarantees a valid representation). The 4GB limit for Blob cannot be expressed as a type constraint -- it is a protocol enforcement -- so `serialize_blob` performs the check internally and returns an error rather than relying on the caller to pre-validate. This is consistent with the TRAP-before-serialize principle: the function itself is the last line of defense.

- **Length prefix**: Big-endian u32 byte count (not character count)
- **Maximum length**: 4GB (2^32 - 1 bytes) -- given by u32 length prefix
- **Error**: If length > 4GB, return Err(DCS_BLOB_LENGTH_OVERFLOW)
- **Byte-identical to Bytes (Raw)**: The serialization format is identical; the rename reflects first-class type status
- **Public API boundary**: `serialize_blob` is the public, type-tagged entry point for the DCS Blob type. `serialize_bytes` is retained as a low-level primitive available for internal DCS use (DVEC, DMAT, Option) and for other RFCs requiring raw length-prefixed serialization. It MUST NOT be used as the serialization entry point for the Blob type in typed contexts. See RFC-0201 BYTEA(32) Suitability in the Motivation section for the primary use case.
- **Typed-context requirement**: Blob deserialization MUST only be invoked in a typed context (schema-driven dispatch). Bare Blob/String concatenation without type context is forbidden. A Blob field deserialized where a String is expected (or vice versa) produces an error. This prevents the semantic ambiguity described in NEW-KI-2. See Change 13 for a concrete dispatcher example. **Implementation note:** Schema validation is recommended at compile time where possible (e.g., struct field type annotations in the schema definition). Dynamic schema environments SHOULD perform schema validation before deserialization begins to prevent misconfiguration from reaching the dispatcher.

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
| 17 | Blob | Length prefix + data | `b"hello"` | `0x00000005 0x68656c6c6f` |

> **Note:** Entry 4 (DMAT column-major) was removed because serialization output is indistinguishable for valid row-major input. DMAT input validation ensures data is stored row-major per RFC-0113.
>
> **Correction:** RFC-0126 v2.5.1 Entry 10 in the probe table incorrectly states "per RFC-0112." The authoritative source is RFC-0111, which is correctly cited in the RFC-0126 dependencies table and Entry 10 detail section. This amendment corrects the probe table reference to RFC-0111.
>
> **Wire format note:** Only Entry 16 (Struct) includes field_id in the wire format (`field_id || encoded_value`). Entries 0-15 and 17 are top-level type serializations without field_id prefixes.

#### Change 4: Probe Entry 17 Details (Section Part 3, after Entry 16)

**Entry 17: Blob Serialization**

- Input: `b"hello"` (5 bytes)
- Serialize: `length=5 (4 bytes BE) || data="hello"`
- Expected bytes: `0x00000005 0x68656c6c6f` (9 bytes total)
- Leaf hash: `SHA256(0x00 || 0x00000005 0x68656c6c6f)` = `01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6`
- Verified via `scripts/compute_dcs_probe_root.py` (see Change 9)

```
serialize_blob(b"hello") =
    u32_be(5) || [0x68, 0x65, 0x6c, 0x6c, 0x6f]
  = [0x00, 0x00, 0x00, 0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f]
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
- [ ] Deserialize Blob with buffer validation and error conditions
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
| DCS_STRING_LENGTH_OVERFLOW | String length exceeds 1MB (2^20 bytes) |
| DCS_INVALID_STRING | Input buffer too short for length prefix (fewer than 4 bytes), or declared length exceeds remaining buffer bytes |
| DCS_INVALID_BLOB | Input buffer too short for length prefix (fewer than 4 bytes), or declared length exceeds remaining buffer bytes |
| DCS_BLOB_LENGTH_OVERFLOW | Blob length exceeds 2^32 - 1 bytes (4GB) |
| DCS_INVALID_STRUCT | Zero-progress deserialization: a field consumed no bytes (new_remaining == remaining), indicating malformed input or dispatcher bug |

> **Note:** The prior combined `DCS_LENGTH_OVERFLOW` ("String/Bytes length exceeds 2^32 - 1") is replaced by two separate errors with distinct limits. String is capped at 1MB per RFC-0126 SectionString. Blob is capped at 4GB by the u32 length prefix. `DCS_INVALID_BLOB` covers buffer-underrun and length-mismatch conditions during deserialization. This change also resolves the pre-existing inconsistency between the error table (2^32-1) and the String section prose (1MB) in RFC-0126 v2.5.1.

#### Change 8: Deserialization (Section Part 3, after Existing Deserialization Rules)

Add Blob deserialization:

**Blob Deserialization**

**Notation:** All multi-byte integers use big-endian (network byte order). `input.len()` denotes the byte length of the input buffer. `input[a..b]` denotes the byte slice from index a (inclusive) to index b (exclusive). `as T` denotes unsigned integer type conversion. Implementations use language-native equivalents.

```
 deserialize_blob(input: &[u8]) -> Result<(&[u8], &[u8]), Err> {
     if input.len() < 4 {
         return Err(DCS_INVALID_BLOB)  // need at least 4 bytes for length prefix
     }
     let length = (input[0] << 24) | (input[1] << 16) | (input[2] << 8) | input[3];
     if input.len() < 4 + (length as usize) {
         return Err(DCS_INVALID_BLOB)  // truncated: declared length exceeds remaining bytes
     }
     let data = input[4..4+(length as usize)];
     let remaining = input[4+(length as usize)..];
     return Ok((data, remaining))  // returns (blob_data, remaining_input_for_next_field)
 }
```

- **Minimum input**: 4 bytes (for the length prefix). If fewer than 4 bytes remain, return Err(DCS_INVALID_BLOB).
- **Length validation**: Declared length MUST NOT exceed remaining buffer bytes. If exceeded, return Err(DCS_INVALID_BLOB).
- **Empty Blob**: `length=0` is valid and returns an empty byte slice. This is NOT an error.
- **Return type**: `Result<(&[u8], &[u8]), Err>` -- returns `(blob_data, remaining_bytes)` on success. This supports schema-driven concatenated deserialization where the caller uses `remaining_bytes` for the next field.
- **Typed-context enforcement**: Blob deserialization is only valid when the schema explicitly specifies a Blob field. Mixing Blob and String bytes without schema context produces indeterminate results and MUST be treated as an error condition by the caller.
- **UTF-8 acceptance**: Blob accepts any byte sequence, including valid UTF-8. This is not an error condition. Applications using Blob for binary data (e.g., cryptographic hashes) do not require UTF-8 validation. See NEW-KI-2 for the implications of byte-level Blob/String equivalence.

**String Deserialization**

**Notation:** All multi-byte integers use big-endian (network byte order). `input.len()` denotes the byte length of the input buffer. `input[a..b]` denotes the byte slice from index a (inclusive) to index b (exclusive). `as T` denotes unsigned integer type conversion. Implementations use language-native equivalents.

```
 deserialize_string(input: &[u8]) -> Result<(&str, &[u8]), Err> {
     if input.len() < 4 {
         return Err(DCS_INVALID_STRING)  // need at least 4 bytes for length prefix
     }
     let length = (input[0] << 24) | (input[1] << 16) | (input[2] << 8) | input[3];

     if length > 1_048_576 {  // 1MB = 2^20
         return Err(DCS_STRING_LENGTH_OVERFLOW)
     }
     if input.len() < 4 + (length as usize) {
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
             if i + 1 >= bytes.len() { return Err(DCS_INVALID_UTF8) }
             let b2 = bytes[i+1];
             if (b2 & 0xC0) != 0x80 { return Err(DCS_INVALID_UTF8) }
             let cp = ((b1 & 0x1F) as u32) << 6 | ((b2 & 0x3F) as u32);
             // Minimum check: overlong encoding rejected by requiring cp >= 0x80
             if cp < 0x80 { return Err(DCS_INVALID_UTF8) }
             i += 2;
         } else if (b1 & 0xF0) == 0xE0 {
             // 3-byte sequence: U+0800 to U+FFFF (except surrogates)
             if i + 2 >= bytes.len() { return Err(DCS_INVALID_UTF8) }
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
             if i + 3 >= bytes.len() { return Err(DCS_INVALID_UTF8) }
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
- **UTF-8 validation**: The byte sequence is validated as UTF-8 per RFC 3629 at deserialization time. This includes: valid byte structure for each sequence length, rejection of overlong encodings (minimum codepoint per length), rejection of surrogate codepoints (U+D800–U+DFFF), and rejection of codepoints above U+10FFFF. If invalid, return Err(DCS_INVALID_UTF8).
- **Return type**: `Result<(&str, &[u8]), Err>` -- returns `(string_slice, remaining_bytes)` on success.

#### Change 9: Published Merkle Root

The existing 17-entry Merkle Root (`2ed91a62f96f11151cd9211cf90aff36efc16c69d3ef910f4201592095abdaca`) was computed over entries 0-16. Adding Entry 17 changes the tree structure from odd (17 entries, last leaf duplicated) to even (18 entries, no duplication required).

**All 18 entry data and leaf hashes (for independent verification):**

| Index | Entry Data (hex) | Leaf Hash (SHA256 of 0x00 || entry_data) |
|-------|------------------|------------------------------------------|
| 0 | `00000000000000010000000000000000` | `5590b4a4eb4b7a9dba75b0176d06fbdabd8798d4b444741bb8efff24ad5b63f1` |
| 1 | `fffffffffffffffffb0100000000000000` | `ad199dd0c6dc5752316d5e8318f37e777d4057d75a4a0f05cb8a491c7ee91b83` |
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
| 12 | `0000000000000000000000000000002a` | `170e5f45c1585c19f017f3c0df39c010e0904b0980fc8251ff4dd8eeef0376c` |
| 13 | `ffffffffffffffffffffffffffffffd6` | `340bdc8e30453799595c901721334ae5ff819a3e19f4ec6db4e6e9665454eb30` |
| 14 | `01000000010000002a00000000000000` | `ba9bc680540d876003d8a04ed12363e87af3567283f73c5b0127f5ad40314063` |
| 15 | `0000000000000000000000000000002a0000000000000000` | `7b0dc69a6bd9f3985e909871a6465971aef51b7c4b05051daefa0aa6d1b1fbc3` |
| 16 | `000000010000002a0000000200000005616c6963650000000300000000000000010000000000000000` | `8ce4a58171d93997bec1861d361b1bfae9a376027dd65f5cb5b045b27a1de890` |
| 17 | `0000000568656c6c6f` | `01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6` |

**Entry 17 leaf hash:** `01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6`

**Verified computation:**

```
Entry 17 input bytes:  0x00 0x00 0x00 0x05 0x68 0x65 0x6c 0x6c 0x6f
Domain-separated leaf:  SHA256(0x00 || 0x0000000568656c6c6f)
                     = 01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6

Verification:         python3 scripts/compute_dcs_probe_root.py @ 7b22f8a
Script encodings:     RFC-0110 (Entry 14), RFC-0104 (Entry 15), RFC-0111 (Entry 10)
Cross-verified:       Yes -- implementers MUST use the script at commit 7b22f8a to reproduce the root
```

**18-entry Merkle Root:** `78154bb3879a85406ea09064603ecdcaae2bad5b0ff16066d578d9c17c38565c`

> **Tree structure transition:** The Merkle tree over 17 entries has an odd leaf count. Per RFC-0126 SectionMerkle Root Computation, the last leaf (leaf_16) is duplicated for the final pair: `SHA256(0x01 || leaf_16 || leaf_16)`. Adding Entry 17 (leaf_17) brings the count to 18, which is even -- no duplication needed. This changes the internal node structure of the entire tree. The new root is not an incremental append; all prior entries' contributions to the root are affected by the changed pairing structure.

#### Change 10: Known Issues

Update the Known Issues table:

| ID | Description |
|----|-------------|
| MED-10 | Entries 5 (Option::None) and 9 (Bool false) produce identical leaf hashes (`96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7`). Domain-separated leaf hashing prevents Merkle root collision. Note: RFC-0126 v2.5.1 published `6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d` (SHA256 of raw `0x00` without domain separation), which was incorrect. The correct domain-separated hash is `96a296d2...`. |
| NEW-KI-1 | Blob entry (Entry 17) did not appear in RFC-0126 v2.5.1 Primitive Type Encodings table or Probe table. This amendment adds it. |
| NEW-KI-2 | Entry 17 (Blob `"hello"`) and Entry 4 (String `"hello"`) produce identical leaf hashes (`01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6`) because they encode identically. Domain-separated leaf hashing prevents Merkle root collision -- this is safe by design, consistent with the existing Entry 5/Entry 9 collision. Typed-context deserialization (Change 8) prevents semantic ambiguity. The probe tests serialization equivalence only; negative deserialization (rejecting Blob bytes when String is expected, or vice versa) is verified by the typed-context requirement and the schema-driven dispatcher (Change 13), not by the probe. |
| NEW-KI-3 | Adding Entry 17 changes the Merkle tree from odd (17, last leaf duplicated) to even (18, no duplication) leaf count. This structural change affects the root. See Change 9. |

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

#### Change 12: RFC-0126 Version Update

Upon merge of this amendment, RFC-0126 version MUST be incremented to **v2.6.0** and the following entry added to RFC-0126's version history:

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 2.6.0 | 2026-03-25 | CipherOcto | Added Blob as first-class DCS type (Entry 17), renamed Bytes (Raw) to Blob, split DCS_LENGTH_OVERFLOW into String/Blob-specific errors, added Blob deserialization, incremented NUMERIC_SPEC_VERSION to 2, corrected Entry 10 probe table reference from RFC-0112 to RFC-0111, corrected Known Issues leaf hash to domain-separated value |

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
        // ... other types
    }
}

fn deserialize_struct(input: &[u8], schema: &StructSchema) -> Result<Value, Err> {
    let mut remaining = input;
    let fields = empty list;
    for field in schema.fields {  // fields in declaration order
        let field_result = deserialize_field(remaining, field.type_);
        match field_result {
            Err(e) => return Err(e),  // propagate error
            Ok((new_remaining, value)) => {
                // Validate: new_remaining must equal the bytes consumed for this field
                // If new_remaining == remaining (no progress), return error
                if new_remaining == remaining {
                    return Err(DCS_INVALID_STRUCT);  // field produced no bytes
                }
                remaining = new_remaining;
                append (field.id, value) to fields;
            }
        }
    }
    return Ok(Value::Struct(fields));
}
```

**Key properties:**

1. **Type tracking**: The dispatcher knows the expected type for each field from the schema. It never guesses based on bytes alone.
2. **Progress requirement**: Each field MUST consume at least 1 byte. Zero-progress deserialization produces an error.
3. **Empty Blob note**: The progress check (`new_remaining != remaining`) validates that the length prefix was consumed (4 bytes). It does NOT validate payload presence. An empty Blob (`length=0`) consumes exactly 4 bytes (the length prefix) and passes this check -- this is correct behavior. The check guarantees forward progress, not data presence.
4. **No cross-type byte passing**: The bytes returned from one field's deserialization are passed to the NEXT field's deserializer, never back to a different-type deserializer. Mixing Blob/String bytes without schema context is impossible by construction.
5. **Error propagation**: Any deserialization error propagates immediately; partial results are discarded.

**Type definitions in pseudocode:** The types `Type`, `Value`, `StructSchema`, `Field`, `FieldId` shown above are schema concepts defined by the application. The dispatcher pattern is language-agnostic; implementations use their local type system.

This dispatcher pattern is how DCS deserialization is intended to be used. The alternative -- calling `deserialize_blob` or `deserialize_string` directly on raw bytes with no schema context -- is not conformant DCS usage.

**Conformance requirement:** Conformance to RFC-0126 with Blob support REQUIRES using a schema-driven dispatcher for Blob fields. Direct calls to `deserialize_blob` on raw bytes without type context are not conformant. Other DCS types MAY use direct deserialization calls; the dispatcher requirement applies to Blob deserialization only. The dispatcher enforces the typed-context requirement; the requirement cannot be satisfied by prose alone.

**Error notation:** Pseudocode uses `return Err(ERROR_CODE)` for error returns. All error codes (`DCS_INVALID_STRING`, `DCS_STRING_LENGTH_OVERFLOW`, `DCS_INVALID_UTF8`, `DCS_INVALID_BLOB`, `DCS_BLOB_LENGTH_OVERFLOW`, `DCS_INVALID_STRUCT`) denote deterministic error states that abort deserialization. Implementations MUST treat all error conditions as fatal.

**Zero-byte type constraint:** The progress check constrains future DCS type design: all DCS types used with this dispatcher MUST consume at least 1 byte during deserialization. Zero-byte types are incompatible with this dispatcher pattern.

#### Change 14: Probe Extension Protocol (Normative)

Amendments adding new DCS types to the verification probe MUST follow this protocol:

1. **Append only**: New entries are added at the next sequential index (N+1, N+2, ...). Existing entries are never modified or reordered.
2. **Root recomputation**: The new entry changes the leaf count from odd to even or vice versa, which changes the pairing structure and thus the Merkle root. The new root MUST be computed and published in the amendment.
3. **Version increment**: Adding a new type constitutes a change to canonical encoding formats. `NUMERIC_SPEC_VERSION` MUST be incremented per RFC-0110 SectionVersion Increment Policy, including activation governance.
4. **Announcement**: The amendment MUST list all prior leaf hashes alongside the new entry so that the full tree can be independently verified without requiring the implementer to run prior versions of the script.

This ensures the probe is monotonically verifiable across amendments.

**Trade-off: Announcement size vs. verifiability.** Item 4 (Announcement) requires publishing all prior leaf hashes alongside each new entry. This grows linearly: entry N requires N-1 prior hashes. The reviewer correctly identified this as a scalability concern. The alternative — publishing only the new entry and prior Merkle root — would require implementers to trust that the prior root was correct, undermining independent verifiability. A structural solution (e.g., a vector commitment scheme enabling logarithmic or constant-size inclusion proofs) would eliminate linear growth but requires breaking changes to the Merkle tree structure defined in RFC-0126 — outside this amendment's scope. The linear growth is an explicit, acceptable trade-off: the probe advances slowly (only through RFC amendment), publication frequency is low, and the cost is borne by the amendment author once, not by implementers verifying the root. Implementers need only verify the root, not maintain the full publication history.

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-03-25 | CipherOcto | Initial amendment draft -- adds Blob (Entry 17) to DCS type system |
| 2.0 | 2026-03-25 | CipherOcto | Adversarial review fixes: CRIT-1 compute 18-entry Merkle root, CRIT-2 split DCS_LENGTH_OVERFLOW into String/Blob-specific errors with distinct limits, HIGH-1 add typed-context deserialization requirement, HIGH-2 retain serialize_bytes as low-level primitive, HIGH-3 verify leaf hash via compute_dcs_probe_root.py, HIGH-4 add deserialize_blob algorithm, MED-2 document odd-to-even tree structure change, MED-4 add length prefix endianness prose, MED-5 fix relationship table direction, MED-3 confirm BYTEA(32) suitability, LOW-1 specify RFC-0126 v2.6.0 target, LOW-2 address NUMERIC_SPEC_VERSION increment, LOW-3 fix table formatting and preserve historical note |
| 3.0 | 2026-03-25 | CipherOcto | Round 2 fixes: HIGH-1 fix NUMERIC_SPEC_VERSION to u32 value 2 (not 2.0), MED-2 fix deserialize_blob return type and add DCS_INVALID_BLOB to error table, MED-3 add H_upgrade governance note, MED-1 publish all 18 leaf hashes and fix RFC-0111/RFC-0112 discrepancy, MED-4 add 4GB security consideration, LOW-1 document domain-separated hash correction for MED-10, LOW-3 change RFC-0201 label to (Storage), LOW-4 replace unwrap() with explicit bytes |
| 4.0 | 2026-03-25 | CipherOcto | Round 3: CRIT-1 rebuttal (Entry 17 bhello identical to String is intentional, tests wire-format collision), CRIT-2 rebuttal (error split is bug fix not breaking change), CRIT-3 rebuttal (dispatcher is DCS layer boundary), HIGH-3 rebuttal (DCS_INVALID_BLOB unified error is better for debugging), MED-1 fix Entry 16 table description to match Person struct, MED-3 add Change 13 with concrete schema-driven dispatcher pseudocode example, MED-3 add Change 14 with probe extension protocol, HIGH-1 clarify serialize_blob vs serialize_bytes public API boundary, HIGH-2 add activation checklist to NUMERIC_SPEC_VERSION governance note |
| 5.0 | 2026-03-25 | CipherOcto | Round 4: NEW-CRIT-1 make Change 13 normative (schema-driven dispatcher conformance required), NEW-CRIT-2 document empty Blob + progress-check interaction, NEW-CRIT-3 add field_id wire-format note to Entry 16 table header, NEW-HIGH-1 clarify serialize_bytes visibility for other RFCs, NEW-HIGH-2 rebuttal (String 1MB enforcement pre-existing RFC-0126 gap, scope), NEW-HIGH-3 rebuttal (block versioning governed by RFC-0110, scope), NEW-MED-1 make Change 14 normative (probe extension protocol), NEW-MED-2 add negative-deserialization limitation note to NEW-KI-2, NEW-MED-3 cross-reference to existing Motivation section, NEW-MED-4 remove duplicate v3.0 version history row, NEW-MED-5 document UTF-8 acceptance as intentional in Change 8, NEW-LOW-1 add type definition note to dispatcher pseudocode, NEW-LOW-2 add script version note, NEW-LOW-3 note deferred to editorial pass |
| 6.0 | 2026-03-25 | CipherOcto | Round 5 fixes: NEW-CRIT-4 add DCS_INVALID_STRUCT to error table, NEW-CRIT-5 add deserialize_string pseudocode (with DCS_INVALID_STRING, 1MB check, RFC 3629 UTF-8 validation), NEW-HIGH-4 clarify dispatcher requirement applies to Blob fields only, NEW-HIGH-5 rebuttal (negative deserialization tests scope + would break Merkle root), NEW-MED-6 document zero-byte-type constraint, NEW-MED-7 add explicit length check to serialize_blob pseudocode, NEW-MED-8 pin script to commit 7b22f8a, NEW-MED-9 clarify RFC-0201 relationship, NEW-LOW-4 address linear growth trade-off explicitly in Change 14, NEW-LOW-5 clarify error return semantics, NEW-LOW-6 add field_id-only-on-Entry-16 note; Round 6 fixes: NEW-CRIT-6 fix table header wire format description, NEW-HIGH-6 standardize all pseudocode to return Err(), NEW-HIGH-7 rewrite deserialize_string in language-agnostic pseudocode, remove all TRAP notation; Round 7 fixes: NEW-MED-10 replace Vec::new()/push() with language-agnostic list notation, NEW-MED-11 add codepoint upper-bound and surrogate checks to UTF-8 validation (RFC 3629 compliant), NEW-MED-12 add Result return type note for serialize_blob, NEW-LOW-7 cp variable now used for full codepoint validation (not just minimum), NEW-LOW-8 add field_ids to Entry 16 description, NEW-LOW-9 consolidate patch notes into single v6.0 entry; Round 8 fixes: NEW-LOW-10 replace u32::from_be_bytes with language-agnostic shift/or notation, NEW-LOW-11 update header to note rounds consolidated, NEW-LOW-12 fix Entry 16 probe table field_id to use u32_be(1) not 0x01; Round 9 fixes: NEW-LOW-13 add notation note for slice/input.len/cast syntax, NEW-LOW-14 same notation note covers as cast notation, NEW-LOW-15 same notation note covers input.len method; Round 10 (Grok external review): MED-2 add static schema validation recommendation to typed-context requirement, LOW-1 add big-endian network byte order to notation notes |

## Related RFCs

- RFC-0126 (Numeric): Deterministic Canonical Serialization -- amended by this RFC
- RFC-0201 (Storage): Binary BLOB Type for Hash Storage -- blocked on this amendment for Accepted status
- RFC-0903 (Economics): Virtual API Key System -- requires Blob DCS entry
- RFC-0909 (Economics): Deterministic Quota Accounting -- requires Blob DCS entry

---

**Version:** 6.0
**Submission Date:** 2026-03-25
**Last Updated:** 2026-03-25
