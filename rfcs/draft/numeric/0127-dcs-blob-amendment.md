# RFC-0127 (Numeric): DCS Blob Amendment -- Deterministic Canonical Serialization

## Status

Draft (v3, adversarial review round 2)

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
 serialize_blob(data: &[u8]) -> Vec<u8> {
     serialize_bytes(data)  // u32_be(data.len()) || data
 }
```

- **Length prefix**: Big-endian u32 byte count (not character count)
- **Maximum length**: 4GB (2^32 - 1 bytes) -- given by u32 length prefix
- **TRAP**: If length > 4GB, TRAP(DCS_BLOB_LENGTH_OVERFLOW)
- **Byte-identical to Bytes (Raw)**: The serialization format is identical; the rename reflects first-class type status
- **Typed-context requirement**: Blob deserialization MUST only be invoked in a typed context (schema-driven dispatch). Bare Blob/String concatenation without type context is forbidden. A Blob field deserialized where a String is expected (or vice versa) produces a TRAP. This prevents the semantic ambiguity described in NEW-KI-2.

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
| 16 | Struct | Field ordering | `Struct { a: 1, b: true }` | `0x00` + field_0 + `0x01` + field_1 |
| 17 | Blob | Length prefix + data | `b"hello"` | `0x00000005 0x68656c6c6f` |

> **Note:** Entry 4 (DMAT column-major) was removed because serialization output is indistinguishable for valid row-major input. DMAT input validation ensures data is stored row-major per RFC-0113.
>
> **Correction:** RFC-0126 v2.5.1 Entry 10 in the probe table incorrectly states "per RFC-0112." The authoritative source is RFC-0111, which is correctly cited in the RFC-0126 dependencies table and Entry 10 detail section. This amendment corrects the probe table reference to RFC-0111.

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
| RFC-0201 (Storage) | Binary BLOB type, length-prefixed serialization (downstream consumer) |

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
- [ ] Deserialize Blob with buffer validation and TRAP conditions
- [ ] Canonicalize DQA before serialization
- [ ] TRAP on invalid inputs before serialization
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
| DCS_INVALID_BLOB | Input buffer too short for length prefix (fewer than 4 bytes), or declared length exceeds remaining buffer bytes |
| DCS_BLOB_LENGTH_OVERFLOW | Blob length exceeds 2^32 - 1 bytes (4GB) |

> **Note:** The prior combined `DCS_LENGTH_OVERFLOW` ("String/Bytes length exceeds 2^32 - 1") is replaced by two separate errors with distinct limits. String is capped at 1MB per RFC-0126 SectionString. Blob is capped at 4GB by the u32 length prefix. `DCS_INVALID_BLOB` covers buffer-underrun and length-mismatch conditions during deserialization. This change also resolves the pre-existing inconsistency between the error table (2^32-1) and the String section prose (1MB) in RFC-0126 v2.5.1.

#### Change 8: Deserialization (Section Part 3, after Existing Deserialization Rules)

Add Blob deserialization:

**Blob Deserialization**

```
 deserialize_blob(input: &[u8]) -> Result<(&[u8], &[u8]), TRAP> {
     if input.len() < 4 {
         TRAP(DCS_INVALID_BLOB)  // need at least 4 bytes for length prefix
     }
     let (length_bytes, rest) = input.split_at(4);
     // safe: length_bytes is exactly 4 bytes, no unwrap needed
     let length = u32::from_be_bytes([
         length_bytes[0], length_bytes[1], length_bytes[2], length_bytes[3]
     ]);

     if rest.len() < length as usize {
         TRAP(DCS_INVALID_BLOB)  // truncated: declared length exceeds remaining bytes
     }
     let (data, leftover) = rest.split_at(length as usize);

     // Empty blob (length=0) is valid and returns empty slice
     // No minimum length requirement

     Ok((data, leftover))  // returns (blob_data, remaining_input_for_next_field)
 }
```

- **Minimum input**: 4 bytes (for the length prefix). If fewer than 4 bytes remain, TRAP.
- **Length validation**: Declared length MUST NOT exceed remaining buffer bytes. If exceeded, TRAP.
- **Empty Blob**: `length=0` is valid and returns an empty byte slice. This is NOT a TRAP.
- **Return type**: `Result<(&[u8], &[u8]), TRAP>` -- returns `(blob_data, remaining_bytes)` on success. This supports schema-driven concatenated deserialization where the caller uses `remaining_bytes` for the next field.
- **Typed-context enforcement**: Blob deserialization is only valid when the schema explicitly specifies a Blob field. Mixing Blob and String bytes without schema context produces indeterminate results and MUST be treated as a TRAP condition by the caller.

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

Verification:         python3 scripts/compute_dcs_probe_root.py (extended with Entry 17)
Script encodings:     RFC-0110 (Entry 14), RFC-0104 (Entry 15), RFC-0111 (Entry 10)
Cross-verified:       Yes
```

**18-entry Merkle Root:** `78154bb3879a85406ea09064603ecdcaae2bad5b0ff16066d578d9c17c38565c`

> **Tree structure transition:** The Merkle tree over 17 entries has an odd leaf count. Per RFC-0126 SectionMerkle Root Computation, the last leaf (leaf_16) is duplicated for the final pair: `SHA256(0x01 || leaf_16 || leaf_16)`. Adding Entry 17 (leaf_17) brings the count to 18, which is even -- no duplication needed. This changes the internal node structure of the entire tree. The new root is not an incremental append; all prior entries' contributions to the root are affected by the changed pairing structure.

#### Change 10: Known Issues

Update the Known Issues table:

| ID | Description |
|----|-------------|
| MED-10 | Entries 5 (Option::None) and 9 (Bool false) produce identical leaf hashes (`96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7`). Domain-separated leaf hashing prevents Merkle root collision. Note: RFC-0126 v2.5.1 published `6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d` (SHA256 of raw `0x00` without domain separation), which was incorrect. The correct domain-separated hash is `96a296d2...`. |
| NEW-KI-1 | Blob entry (Entry 17) did not appear in RFC-0126 v2.5.1 Primitive Type Encodings table or Probe table. This amendment adds it. |
| NEW-KI-2 | Entry 17 (Blob `"hello"`) and Entry 4 (String `"hello"`) produce identical leaf hashes (`01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6`) because they encode identically. Domain-separated leaf hashing prevents Merkle root collision -- this is safe by design, consistent with the existing Entry 5/Entry 9 collision. Typed-context deserialization (Change 8) prevents semantic ambiguity. |
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

**Activation governance:** RFC-0110 SectionVersion Increment Policy requires a minimum 2-epoch notice before activation at block H_upgrade, with a grace window for dual-version acceptance. This amendment does not set H_upgrade; a separate governance action per RFC-0110 policy is required to determine the activation block height. Nodes MUST NOT reject version-1 blocks before the governance-declared H_upgrade, even after upgrading to support Blob.

#### Change 12: RFC-0126 Version Update

Upon merge of this amendment, RFC-0126 version MUST be incremented to **v2.6.0** and the following entry added to RFC-0126's version history:

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 2.6.0 | 2026-03-25 | CipherOcto | Added Blob as first-class DCS type (Entry 17), renamed Bytes (Raw) to Blob, split DCS_LENGTH_OVERFLOW into String/Blob-specific errors, added Blob deserialization, incremented NUMERIC_SPEC_VERSION to 2, corrected Entry 10 probe table reference from RFC-0112 to RFC-0111, corrected Known Issues leaf hash to domain-separated value |

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-03-25 | CipherOcto | Initial amendment draft -- adds Blob (Entry 17) to DCS type system |
| 2.0 | 2026-03-25 | CipherOcto | Adversarial review fixes: CRIT-1 compute 18-entry Merkle root, CRIT-2 split DCS_LENGTH_OVERFLOW into String/Blob-specific errors with distinct limits, HIGH-1 add typed-context deserialization requirement, HIGH-2 retain serialize_bytes as low-level primitive, HIGH-3 verify leaf hash via compute_dcs_probe_root.py, HIGH-4 add deserialize_blob algorithm, MED-2 document odd-to-even tree structure change, MED-4 add length prefix endianness prose, MED-5 fix relationship table direction, MED-3 confirm BYTEA(32) suitability, LOW-1 specify RFC-0126 v2.6.0 target, LOW-2 address NUMERIC_SPEC_VERSION increment, LOW-3 fix table formatting and preserve historical note |
| 3.0 | 2026-03-25 | CipherOcto | Round 2 fixes: HIGH-1 fix NUMERIC_SPEC_VERSION to u32 value 2 (not 2.0), MED-2 fix deserialize_blob return type and add DCS_INVALID_BLOB to error table, MED-3 add H_upgrade governance note, MED-1 publish all 18 leaf hashes and fix RFC-0111/RFC-0112 discrepancy, MED-4 add 4GB security consideration, LOW-1 document domain-separated hash correction for MED-10, LOW-3 change RFC-0201 label to (Storage), LOW-4 replace unwrap() with explicit bytes |

## Related RFCs

- RFC-0126 (Numeric): Deterministic Canonical Serialization -- amended by this RFC
- RFC-0201 (Storage): Binary BLOB Type for Hash Storage -- blocked on this amendment for Accepted status
- RFC-0903 (Economics): Virtual API Key System -- requires Blob DCS entry
- RFC-0909 (Economics): Deterministic Quota Accounting -- requires Blob DCS entry

---

**Version:** 3.0
**Submission Date:** 2026-03-25
**Last Updated:** 2026-03-25
