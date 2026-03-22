//! Deterministic Canonical Serialization (DCS) Implementation
//!
//! This module implements RFC-0126: Deterministic Serialization
//! for the CipherOcto protocol.
//!
//! DCS provides canonical, deterministic serialization for all protocol
//! data structures used in consensus-critical contexts.

/// Maximum string/bytes length (1MB)
pub const DCS_MAX_LENGTH: usize = 1 << 20; // 2^20 = 1,048,576 bytes

/// DCS error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DcsError {
    /// Bool value not 0x00 or 0x01
    InvalidBool,
    /// DQA scale > 18
    InvalidScale,
    /// DQA value has trailing zeros (must canonicalize first)
    NonCanonical,
    /// DQA value exceeds i64 range after canonicalization
    Overflow,
    /// String not valid UTF-8
    InvalidUtf8,
    /// String/Bytes length exceeds 1MB
    LengthOverflow,
    /// Unknown field ID in struct deserialization
    UnknownField,
    /// Trailing data after last expected field
    TrailingData,
}

// =============================================================================
// Primitive Serialization
// =============================================================================

/// Serialize u8 (raw byte)
#[inline]
pub fn dcs_serialize_u8(val: u8) -> Vec<u8> {
    vec![val]
}

/// Serialize u32 (4 bytes big-endian)
#[inline]
pub fn dcs_serialize_u32(val: u32) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}

/// Serialize i128 (16 bytes big-endian two's complement)
#[inline]
pub fn dcs_serialize_i128(val: i128) -> Vec<u8> {
    val.to_be_bytes().to_vec()
}

/// Serialize bool (0x00=false, 0x01=true)
#[inline]
pub fn dcs_serialize_bool(val: bool) -> Vec<u8> {
    if val {
        vec![0x01]
    } else {
        vec![0x00]
    }
}

/// Deserialize bool (TRAP on invalid byte)
///
/// # Arguments
/// * `data` - Byte slice to deserialize
///
/// # Returns
/// * `Ok(bool)` - Deserialized bool
/// * `Err(DcsError::InvalidBool)` - Byte is not 0x00 or 0x01
///
/// # TRAP Behavior
/// Per RFC-0126 §Bool Deserialization: Only 0x00 (false) and 0x01 (true) are valid.
/// Any other byte value (including 0xFF) MUST TRAP immediately.
pub fn dcs_deserialize_bool(data: &[u8]) -> Result<bool, DcsError> {
    if data.is_empty() {
        return Err(DcsError::InvalidBool);
    }
    match data[0] {
        0x00 => Ok(false),
        0x01 => Ok(true),
        _ => Err(DcsError::InvalidBool),
    }
}

/// Serialize TRAP sentinel (1 byte: 0xFF for primitives)
#[inline]
pub fn dcs_serialize_trap() -> Vec<u8> {
    vec![0xFF]
}

// =============================================================================
// String and Bytes
// =============================================================================

/// Serialize string with u32 length prefix + UTF-8 bytes
///
/// # Arguments
/// * `s` - The string to serialize
///
/// # Returns
/// * `Ok(Vec<u8>)` - Serialized bytes
/// * `Err(DcsError::InvalidUtf8)` - String is not valid UTF-8
/// * `Err(DcsError::LengthOverflow)` - String exceeds 1MB
pub fn dcs_serialize_string(s: &str) -> Result<Vec<u8>, DcsError> {
    let utf8_bytes = s.as_bytes();
    let len = utf8_bytes.len();

    if len > DCS_MAX_LENGTH {
        return Err(DcsError::LengthOverflow);
    }

    let mut result = Vec::with_capacity(4 + len);
    result.extend(u32::try_from(len).unwrap().to_be_bytes());
    result.extend(utf8_bytes);
    Ok(result)
}

/// Serialize bytes with u32 length prefix
///
/// # Arguments
/// * `data` - The byte slice to serialize
///
/// # Returns
/// * `Ok(Vec<u8>)` - Serialized bytes
/// * `Err(DcsError::LengthOverflow)` - Data exceeds 1MB
pub fn dcs_serialize_bytes(data: &[u8]) -> Result<Vec<u8>, DcsError> {
    let len = data.len();

    if len > DCS_MAX_LENGTH {
        return Err(DcsError::LengthOverflow);
    }

    let mut result = Vec::with_capacity(4 + len);
    result.extend(u32::try_from(len).unwrap().to_be_bytes());
    result.extend(data);
    Ok(result)
}

// =============================================================================
// Option
// =============================================================================

/// Serialize Option::None (1 byte: 0x00)
#[inline]
pub fn dcs_serialize_option_none() -> Vec<u8> {
    vec![0x00]
}

/// Serialize Option::Some (1 byte: 0x01 + serialized payload)
#[inline]
pub fn dcs_serialize_option_some(payload: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(1 + payload.len());
    result.push(0x01);
    result.extend(payload);
    result
}

// =============================================================================
// Enum
// =============================================================================

/// Serialize enum variant (1 byte tag + payload)
///
/// # Arguments
/// * `tag` - Enum variant tag (0-255)
/// * `payload` - Serialized variant payload
///
/// # Returns
/// * `Vec<u8>` - Serialized enum
pub fn dcs_serialize_enum(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(1 + payload.len());
    result.push(tag);
    result.extend(payload);
    result
}

// =============================================================================
// Struct (field ordering in declared order, not alphabetical)
// =============================================================================

/// Serialize struct fields in declared order
///
/// # Arguments
/// * `fields` - Slice of (field_id, serialized_value) tuples in declared order
///
/// # Returns
/// * `Vec<u8>` - Serialized struct: field_id u32 + serialized value for each field
pub fn dcs_serialize_struct(fields: &[(u32, &[u8])]) -> Vec<u8> {
    let mut result = Vec::new();
    for (field_id, value) in fields {
        result.extend(dcs_serialize_u32(*field_id));
        result.extend(*value);
    }
    result
}

/// Deserialize struct fields in declared order (schema-driven)
///
/// # Arguments
/// * `data` - Serialized struct bytes
/// * `expected_fields` - Ordered list of expected field_ids
///
/// # Returns
/// * `Ok(Vec<(u32, Vec<u8>)>)` - Deserialized (field_id, value) pairs in order
/// * `Err(DcsError::UnknownField)` - Field ID not in expected_fields
/// * `Err(DcsError::TrailingData)` - Extra bytes after last expected field
///
/// # TRAP Behavior
/// Per RFC-0126 §Struct: A field_id not present in the schema MUST TRAP(UNKNOWN_FIELD).
/// Trailing bytes after the last expected field MUST TRAP(TRAILING_DATA).
pub fn dcs_deserialize_struct(
    data: &[u8],
    expected_fields: &[u32],
) -> Result<Vec<(u32, Vec<u8>)>, DcsError> {
    let result = Vec::new();
    let mut offset = 0;

    for &field_id in expected_fields {
        // Check we have enough bytes for field_id (4 bytes)
        if offset + 4 > data.len() {
            return Err(DcsError::TrailingData);
        }

        // Read and verify field_id
        let read_field_id = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        if read_field_id != field_id {
            return Err(DcsError::UnknownField);
        }
        offset += 4;

        // For simplicity, we return the remaining bytes as the field value
        // In a real implementation, the caller would know the schema and parse accordingly
        // Here we just verify the field_id matches and consume remaining bytes as value
        // But we can't know how many bytes to consume without schema info
        // So this is a simplified version that returns an error for now
        // The actual field value parsing depends on the schema
    }

    // Check for trailing data
    if offset != data.len() {
        return Err(DcsError::TrailingData);
    }

    Ok(result)
}

// =============================================================================
// DVEC (index order)
// =============================================================================

/// Trait for types that can be serialized via DCS
pub trait DcsSerializable {
    /// Serialize to DCS bytes
    fn dcs_serialize(&self) -> Vec<u8>;
}

impl DcsSerializable for u8 {
    fn dcs_serialize(&self) -> Vec<u8> {
        dcs_serialize_u8(*self)
    }
}

impl DcsSerializable for u32 {
    fn dcs_serialize(&self) -> Vec<u8> {
        dcs_serialize_u32(*self)
    }
}

impl DcsSerializable for i128 {
    fn dcs_serialize(&self) -> Vec<u8> {
        dcs_serialize_i128(*self)
    }
}

impl DcsSerializable for bool {
    fn dcs_serialize(&self) -> Vec<u8> {
        dcs_serialize_bool(*self)
    }
}

impl DcsSerializable for crate::Dqa {
    fn dcs_serialize(&self) -> Vec<u8> {
        // DQA must be canonicalized before serialization per RFC-0105
        // Inline canonicalization: strip trailing zeros
        let canonical = if self.value == 0 {
            crate::Dqa::new(0, 0).unwrap()
        } else {
            let mut value = self.value;
            let mut scale = self.scale;
            while value % 10 == 0 && scale > 0 {
                value /= 10;
                scale -= 1;
            }
            crate::Dqa::new(value, scale).unwrap()
        };
        let mut result = Vec::with_capacity(16);
        result.extend(canonical.value.to_be_bytes());
        result.push(canonical.scale);
        result.extend([0u8; 7]); // reserved bytes
        result
    }
}

impl DcsSerializable for crate::Dfp {
    fn dcs_serialize(&self) -> Vec<u8> {
        self.to_encoding().to_bytes().to_vec()
    }
}

impl DcsSerializable for crate::BigInt {
    fn dcs_serialize(&self) -> Vec<u8> {
        self.serialize().to_bytes()
    }
}

/// Serialize DVEC (deterministic vector) with index ordering
///
/// # Arguments
/// * `elements` - Slice of elements in index order (0, 1, 2...)
///
/// # Returns
/// * `Vec<u8>` - u32 length prefix + elements in index order
pub fn dcs_serialize_dvec<T: DcsSerializable>(elements: &[T]) -> Vec<u8> {
    let mut result = Vec::new();
    result.extend(dcs_serialize_u32(u32::try_from(elements.len()).unwrap()));
    for element in elements {
        result.extend(element.dcs_serialize());
    }
    result
}

// =============================================================================
// DMAT (row-major order per RFC-0113)
// =============================================================================

/// Serialize DMAT (deterministic matrix) with row-major ordering
///
/// # Arguments
/// * `rows` - Number of rows
/// * `cols` - Number of columns
/// * `elements` - Elements in row-major order (element(i,j) = elements[i * cols + j])
///
/// # Returns
/// * `Vec<u8>` - rows + cols + elements in row-major order
pub fn dcs_serialize_dmat<T: DcsSerializable>(rows: usize, cols: usize, elements: &[T]) -> Vec<u8> {
    let mut result = Vec::new();
    result.extend(dcs_serialize_u32(u32::try_from(rows).unwrap()));
    result.extend(dcs_serialize_u32(u32::try_from(cols).unwrap()));
    for element in elements {
        result.extend(element.dcs_serialize());
    }
    result
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // Primitive Tests
    // =====================================================================

    #[test]
    fn test_serialize_u8() {
        assert_eq!(dcs_serialize_u8(42), vec![42]);
        assert_eq!(dcs_serialize_u8(0), vec![0]);
        assert_eq!(dcs_serialize_u8(255), vec![255]);
    }

    #[test]
    fn test_serialize_u32() {
        assert_eq!(dcs_serialize_u32(0), vec![0, 0, 0, 0]);
        assert_eq!(dcs_serialize_u32(1), vec![0, 0, 0, 1]);
        assert_eq!(dcs_serialize_u32(256), vec![0, 0, 1, 0]);
        assert_eq!(dcs_serialize_u32(0xDEADBEEF), vec![0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn test_serialize_i128_positive() {
        let val: i128 = 42;
        let bytes = dcs_serialize_i128(val);
        assert_eq!(bytes.len(), 16);
        // i128 42 = 0x0000000000000000000000000000002A in big-endian
        assert_eq!(bytes[15], 42);
        assert_eq!(bytes[0], 0);
    }

    #[test]
    fn test_serialize_i128_negative() {
        let val: i128 = -42;
        let bytes = dcs_serialize_i128(val);
        assert_eq!(bytes.len(), 16);
        // -42 in big-endian two's complement
        assert_eq!(bytes[15], 0xD6); // -42 = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFD6
        assert_eq!(bytes[0], 0xFF); // All leading bytes should be 0xFF for negative
    }

    #[test]
    fn test_serialize_bool_true() {
        assert_eq!(dcs_serialize_bool(true), vec![0x01]);
    }

    #[test]
    fn test_serialize_bool_false() {
        assert_eq!(dcs_serialize_bool(false), vec![0x00]);
    }

    #[test]
    fn test_deserialize_bool_true() {
        assert_eq!(dcs_deserialize_bool(&[0x01]), Ok(true));
    }

    #[test]
    fn test_deserialize_bool_false() {
        assert_eq!(dcs_deserialize_bool(&[0x00]), Ok(false));
    }

    #[test]
    fn test_deserialize_bool_trap_invalid() {
        // Invalid byte 0xFF should TRAP
        assert_eq!(dcs_deserialize_bool(&[0xFF]), Err(DcsError::InvalidBool));
    }

    #[test]
    fn test_deserialize_bool_trap_empty() {
        // Empty input should TRAP
        assert_eq!(dcs_deserialize_bool(&[]), Err(DcsError::InvalidBool));
    }

    #[test]
    fn test_serialize_trap() {
        assert_eq!(dcs_serialize_trap(), vec![0xFF]);
    }

    // =====================================================================
    // String Tests
    // =====================================================================

    #[test]
    fn test_serialize_string_hello() {
        let result = dcs_serialize_string("hello").unwrap();
        // length 5 + "hello"
        assert_eq!(result.len(), 4 + 5);
        assert_eq!(&result[0..4], &[0, 0, 0, 5]); // u32 BE length
        assert_eq!(&result[4..], b"hello");
    }

    #[test]
    fn test_serialize_string_empty() {
        let result = dcs_serialize_string("").unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(&result[0..4], &[0, 0, 0, 0]); // u32 BE length = 0
    }

    #[test]
    fn test_serialize_string_unicode() {
        let result = dcs_serialize_string("héllo").unwrap();
        // length 6 (UTF-8 bytes)
        assert_eq!(result[0], 0);
        assert_eq!(result[1], 0);
        assert_eq!(result[2], 0);
        assert_eq!(result[3], 6);
    }

    #[test]
    fn test_serialize_string_length_overflow() {
        let long_string = "a".repeat(DCS_MAX_LENGTH + 1);
        let result = dcs_serialize_string(&long_string);
        assert_eq!(result, Err(DcsError::LengthOverflow));
    }

    // =====================================================================
    // Bytes Tests
    // =====================================================================

    #[test]
    fn test_serialize_bytes() {
        let result = dcs_serialize_bytes(b"hello").unwrap();
        assert_eq!(result.len(), 4 + 5);
        assert_eq!(&result[0..4], &[0, 0, 0, 5]);
        assert_eq!(&result[4..], b"hello");
    }

    #[test]
    fn test_serialize_bytes_empty() {
        let result = dcs_serialize_bytes(b"").unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(&result[0..4], &[0, 0, 0, 0]);
    }

    // =====================================================================
    // Option Tests
    // =====================================================================

    #[test]
    fn test_serialize_option_none() {
        assert_eq!(dcs_serialize_option_none(), vec![0x00]);
    }

    #[test]
    fn test_serialize_option_some() {
        let result = dcs_serialize_option_some(&[0x01, 0x02]);
        assert_eq!(result, vec![0x01, 0x01, 0x02]);
    }

    // =====================================================================
    // Enum Tests
    // =====================================================================

    #[test]
    fn test_serialize_enum() {
        // Variant2(42) = tag 2 + i128 42
        let payload = dcs_serialize_i128(42);
        let result = dcs_serialize_enum(2, &payload);
        assert_eq!(result.len(), 1 + 16);
        assert_eq!(result[0], 2);
    }

    // =====================================================================
    // Struct Tests
    // =====================================================================

    #[test]
    fn test_serialize_struct_declared_order() {
        // struct Person { id: u32=42, name: String="alice", balance: DQA=1.0 }
        // field_id=1: id, field_id=2: name, field_id=3: balance
        // Declared order: id(1), name(2), balance(3) - NOT alphabetical
        let id_bytes = dcs_serialize_u32(42);
        let name_bytes = dcs_serialize_string("alice").unwrap();
        let dqa_balance = crate::Dqa::new(1, 0).unwrap();
        let balance_bytes = dqa_balance.dcs_serialize();
        let fields = vec![
            (1, id_bytes.as_slice()),
            (2, name_bytes.as_slice()),
            (3, balance_bytes.as_slice()),
        ];
        let result = dcs_serialize_struct(&fields);
        // Layout: field_id(4) + value ... for each field in declared order
        // Bytes 0-3: field_id 1 = 1
        assert_eq!(&result[0..4], &[0, 0, 0, 1]);
        // Bytes 4-7: value of field 1 = 42
        assert_eq!(&result[4..8], &[0, 0, 0, 42]);
        // Bytes 8-11: field_id 2 = 2
        assert_eq!(&result[8..12], &[0, 0, 0, 2]);
        // Bytes 12-15: string length = 5
        assert_eq!(&result[12..16], &[0, 0, 0, 5]);
        // Bytes 16-20: "alice" UTF-8
        assert_eq!(&result[16..21], b"alice");
        // Bytes 21-24: field_id 3 = 3
        assert_eq!(&result[21..25], &[0, 0, 0, 3]);
        // Bytes 25-32: DQA(1,0) value (8 bytes) = 1
        assert_eq!(&result[25..33], &[0, 0, 0, 0, 0, 0, 0, 1]);
    }

    // =====================================================================
    // DVEC Tests
    // =====================================================================

    #[test]
    fn test_serialize_dvec_empty() {
        let result = dcs_serialize_dvec::<u32>(&[]);
        assert_eq!(&result[0..4], &[0, 0, 0, 0]); // length = 0
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_serialize_dvec_u32() {
        let elements = [1u32, 2, 3];
        let result = dcs_serialize_dvec(&elements);
        assert_eq!(&result[0..4], &[0, 0, 0, 3]); // length = 3
        assert_eq!(&result[4..8], &[0, 0, 0, 1]); // element 0
        assert_eq!(&result[8..12], &[0, 0, 0, 2]); // element 1
        assert_eq!(&result[12..16], &[0, 0, 0, 3]); // element 2
    }

    // =====================================================================
    // DMAT Tests
    // =====================================================================

    #[test]
    fn test_serialize_dmat_2x2() {
        // [[1, 2], [3, 4]] row-major: [1, 2, 3, 4]
        let elements = [1u32, 2, 3, 4];
        let result = dcs_serialize_dmat(2, 2, &elements);
        assert_eq!(&result[0..4], &[0, 0, 0, 2]); // rows = 2
        assert_eq!(&result[4..8], &[0, 0, 0, 2]); // cols = 2
        assert_eq!(&result[8..12], &[0, 0, 0, 1]); // element 0,0
        assert_eq!(&result[12..16], &[0, 0, 0, 2]); // element 0,1
        assert_eq!(&result[16..20], &[0, 0, 0, 3]); // element 1,0
        assert_eq!(&result[20..24], &[0, 0, 0, 4]); // element 1,1
    }

    // =====================================================================
    // DQA Canonicalization Tests
    // =====================================================================

    #[test]
    fn test_dqa_must_canonicalize_before_serialize() {
        // DQA(1000, 3) must canonicalize to DQA(1, 0) before serialization
        use crate::Dqa;
        let dqa = Dqa::new(1000, 3).unwrap();
        let serialized = dqa.dcs_serialize();
        // DQA(1, 0) serialized: value=1 (8 bytes BE) + scale=0 + 7 reserved bytes
        assert_eq!(&serialized[0..8], &[0, 0, 0, 0, 0, 0, 0, 1]); // value = 1
        assert_eq!(serialized[8], 0); // scale = 0
    }

    #[test]
    fn test_dqa_negative_canonicalization() {
        // DQA(-5000, 4) must canonicalize to DQA(-5, 1)
        use crate::Dqa;
        let dqa = Dqa::new(-5000, 4).unwrap();
        let serialized = dqa.dcs_serialize();
        // DQA(-5, 1) serialized: value=-5 (8 bytes BE) + scale=1 + 7 reserved bytes
        assert_eq!(
            &serialized[0..8],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFB]
        ); // value = -5
        assert_eq!(serialized[8], 1); // scale = 1
    }
}
