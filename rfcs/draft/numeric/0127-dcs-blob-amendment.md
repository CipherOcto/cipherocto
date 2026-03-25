# RFC-0127 (Numeric): DCS Blob Amendment -- Deterministic Canonical Serialization

## Status

Draft (v1)

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

Rename the section header from "Bytes (Raw)" to "Blob":

```
 serialize_blob(data: &[u8]) -> Vec<u8> {
     u32_be(data.len()) || data
 }
```

- **Maximum length**: 4GB (2³^32 bytes) -- given by u32 length prefix
- **TRAP**: If length > 4GB, TRAP(LENGTH_OVERFLOW)
- **Byte-identical to Bytes (Raw)**: The serialization format is identical; the rename reflects first-class type status

#### Change 3: Probe Entries Table (Section Part 3, line ~625)

Add Entry 17 for Blob:

| Index | Type | Description | Input | Expected Serialization |
|-------|------|-------------|-------|----------------------|
| 0 | DQA | Positive canonicalization | `DQA(1000, 3)` → canonicalize → `DQA(1, 0)` | 8 bytes value + 1 byte scale + 7 bytes reserved |
| 1 | DQA | Negative canonicalization | `DQA(-5000, 4)` → canonicalize → `DQA(-5, 1)` | 8 bytes value + 1 byte scale + 7 bytes reserved |
| 2 | DVEC | Length + index ordering | `[1, 2, 3]` | `0x00000003` + 3x DQA elements |
| 3 | DMAT | Row-major traversal | `[[1, 2], [3, 4]]` (2x2) | rows + cols + 4x DQA elements |
| 4 | String | UTF-8 encoding | `"hello"` | `0x00000005` + UTF-8 bytes |
| 5 | Option | None | `None` | `0x00` |
| 6 | Option | Some(true) | `Some(true)` | `0x01` + `0x01` |
| 7 | Enum | Tagged variant (i128 payload) | `Variant2(42)` | `0x02` + i128 encoding of 42 (16 bytes) |
| 8 | Bool | True | `true` | `0x01` |
| 9 | Bool | False | `false` | `0x00` |
| 10 | TRAP | Numeric (24-byte) | Numeric TRAP | 24 bytes per RFC-0112 |
| 11 | TRAP | Bool (1-byte) | Invalid bool `0xFF` | `0xFF` (TRAP sentinel) |
| 12 | I128 | Positive | `42` | 16 bytes big-endian |
| 13 | I128 | Negative | `-42` | 16 bytes big-endian |
| 14 | BIGINT | Positive | `42` | RFC-0110 BigIntEncoding (16 bytes) |
| 15 | DFP | Positive Normal | `42.0` | RFC-0104 DfpEncoding (24 bytes) |
| 16 | Struct | Field ordering | `Struct { a: 1, b: true }` | `0x00` + field_0 + `0x01` + field_1 |
| **17** | **Blob** | **Length prefix + data** | **`b"hello"`** | **`0x00000005 0x68656c6c6f`** |

#### Change 4: Probe Entry 17 Details (Section Part 3, after Entry 16)

Add detailed entry for Blob:

**Entry 17: Blob Serialization**

- Input: `b"hello"` (5 bytes)
- Serialize: `length=5 (4 bytes BE) || data="hello"`
- Expected bytes: `0x00000005 0x68656c6c6f` (9 bytes total)
- Leaf hash: `SHA256(0x00 || 0x00000005 0x68656c6c6f)` = `01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6`

```
serialize_blob(b"hello") =
    u32_be(5) || [0x68, 0x65, 0x6c, 0x6c, 0x6f]
  = [0x00, 0x00, 0x00, 0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f]
```

#### Change 5: Relationship Table (Section Part 3, SectionRelationship to Other RFCs)

Add Blob row:

| RFC | Relationship |
|-----|--------------|
| RFC-0104 (DFP) | Decimal floating-point, DfpEncoding, deterministic NaN |
| RFC-0105 (DQA) | Canonicalization rules, TRAP sentinel |
| RFC-0110 (BIGINT) | Integer structure, little-endian limbs, BigIntEncoding |
| RFC-0112 (DVEC) | Vector structure, index ordering |
| RFC-0113 (DMAT) | Matrix structure, row-major ordering |
| **RFC-0201 (Blob)** | **Binary BLOB type, length-prefixed serialization** |

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
- [ ] Canonicalize DQA before serialization
- [ ] TRAP on invalid inputs before serialization
- [ ] Compute and verify Merkle probe root

#### Change 7: Error Handling Table (Section Part 3, SectionDCS Serialization Errors)

Blob uses existing DCS_LENGTH_OVERFLOW error. No new error codes required.

#### Change 8: Known Issues

Add to Known Issues:

| ID | Description |
|----|-------------|
| MED-10 | Entries 5 (Option::None) and 9 (Bool false) produce identical leaf hashes (`6e340b9c...`). Domain-separated leaf hashing prevents Merkle root collision. |
| **NEW-KI-1** | **Blob entry (Entry 17) does not appear in RFC-0126 v2.5.1 Primitive Type Encodings table or Probe table. This amendment adds it.** |
| **NEW-KI-2** | **Entry 17 (Blob `"hello"`) and Entry 4 (String `"hello"`) produce identical leaf hashes (`01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6`) because they encode identically. Domain-separated leaf hashing prevents Merkle root collision -- this is safe by design, consistent with the existing Entry 5/Entry 9 collision documented in RFC-0126 Known Issues.** |

#### Change 9: Published Merkle Root

The existing 17-entry Merkle Root (`2ed91a62f96f11151cd9211cf90aff36efc16c69d3ef910f4201592095abdaca`) was computed over entries 0-16. Adding Entry 17 produces a new 18-entry Merkle Root.

**Entry 17 leaf hash:** `01cc2c521e69293f581e0df49c071c2e9d44b16586b36024872d77244b405be6`

**New 18-entry Merkle Root:** To be computed by implementations using exact RFC-0104 (DFP), RFC-0110 (BIGINT), and RFC-0112 (TRAP) byte encodings as specified in the authoritative RFCs. The 18-entry root MUST be verified independently by implementations before conformance claims.

> **Note:** The 18-entry Merkle Root cannot be computed from this amendment alone because entries 10 (Numeric TRAP, 24-byte format per RFC-0112), 14 (BIGINT, RFC-0110 BigIntEncoding), and 15 (DFP, RFC-0104 DfpEncoding) require exact byte-level definitions from their authoritative RFCs.

## Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-03-25 | CipherOcto | Initial amendment draft -- adds Blob (Entry 17) to DCS type system |

## Related RFCs

- RFC-0126 (Numeric): Deterministic Canonical Serialization -- amended by this RFC
- RFC-0201 (Storage): Binary BLOB Type for Hash Storage -- blocked on this amendment for Accepted status
- RFC-0903 (Economics): Virtual API Key System -- requires Blob DCS entry
- RFC-0909 (Economics): Deterministic Quota Accounting -- requires Blob DCS entry

---

**Version:** 1.0
**Submission Date:** 2026-03-25
**Last Updated:** 2026-03-25
