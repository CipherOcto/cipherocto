#!/usr/bin/env python3
"""
DCS (Deterministic Canonical Serialization) Verification Probe Script

Computes the Merkle root for the 16-entry DCS verification probe.

Run with: python3 scripts/compute_dcs_probe_root.py
"""

import hashlib
from typing import List, Tuple


def sha256(data: bytes) -> bytes:
    """Compute SHA256 hash."""
    return hashlib.sha256(data).digest()


def serialize_u8(v: int) -> bytes:
    """Serialize u8 as raw byte."""
    return bytes([v & 0xFF])


def serialize_u32(v: int) -> bytes:
    """Serialize u32 as big-endian 4 bytes."""
    return v.to_bytes(4, byteorder='big')


def serialize_i128(v: int) -> bytes:
    """Serialize i128 as big-endian two's complement, 16 bytes."""
    return v.to_bytes(16, byteorder='big', signed=True)


def serialize_bigint(value: int) -> bytes:
    """
    Serialize BIGINT per RFC-0110 BigIntEncoding.

    Format: version(1) || sign(1) || reserved(2) || num_limbs(1) || reserved(3) || limbs(8*n bytes)
    - version: 0x01
    - sign: 0x00 (positive), 0xFF (negative)
    - num_limbs: 1-64, number of u64 limbs
    - limbs: little-endian u64 array, least significant limb first
    - Canonical: no leading zero limbs, zero = [0] with sign=false
    """
    # Determine if negative
    sign = value < 0
    abs_value = abs(value)

    # Convert to limbs (little-endian u64 array)
    # We need to find how many limbs and their values
    if abs_value == 0:
        limbs = [0]
    else:
        limbs = []
        remaining = abs_value
        while remaining > 0:
            limbs.append(remaining & 0xFFFFFFFFFFFFFFFF)  # Get lowest 64 bits
            remaining >>= 64

    # Strip leading zero limbs (canonical form)
    while len(limbs) > 1 and limbs[-1] == 0:
        limbs.pop()

    # Build result
    result = bytes([0x01])  # version
    result += bytes([0xFF if sign else 0x00])  # sign
    result += bytes([0, 0])  # reserved
    result += bytes([len(limbs) & 0xFF])  # num_limbs
    result += bytes([0, 0, 0])  # reserved

    # Add limbs in little-endian order
    for limb in limbs:
        result += limb.to_bytes(8, byteorder='little')

    return result


def serialize_bigint_trap() -> bytes:
    """
    Serialize BIGINT TRAP sentinel per RFC-0110.

    Uses 0xDEAD... pattern for TRAP entries in probe context.
    12 bytes: 0xDEAD_DEAD_DEAD_DEAD_DEAD (little-endian u64 × 1.5)
    """
    return bytes([0xAD, 0xDE, 0xAD, 0xDE, 0xAD, 0xDE, 0xAD, 0xDE, 0xAD, 0xDE, 0xAD, 0xDE])


def serialize_dfp(mantissa: int, exponent: int, dfp_class: int, sign: bool) -> bytes:
    """
    Serialize DFP per RFC-0104 DfpEncoding.

    Format: [mantissa:16][exponent:4][class_sign:4] = 24 bytes
    - mantissa: u128, big-endian
    - exponent: i32, big-endian
    - class_sign: u32, big-endian = [class:8][sign:8][reserved:16]
      - class: 0=Normal, 1=Infinity, 2=NaN, 3=Zero
      - sign: 0=positive, 1=negative
    """
    # Pack class_sign: [class:8][sign:8][reserved:16]
    class_sign = (dfp_class << 24) | ((1 if sign else 0) << 16)

    # Build result
    result = mantissa.to_bytes(16, byteorder='big', signed=False)  # mantissa (unsigned)
    result += exponent.to_bytes(4, byteorder='big', signed=True)    # exponent (signed)
    result += class_sign.to_bytes(4, byteorder='big', signed=False)  # class_sign

    return result


def serialize_dfp_trap() -> bytes:
    """
    Serialize DFP TRAP sentinel per RFC-0104.

    Uses class=NaN with mantissa=0 for TRAP representation.
    24 bytes: all zeros except class_sign indicates NaN.
    """
    # DFP NaN: mantissa=0, exponent=0, class_sign=NaN(2)
    result = bytes(16)  # mantissa = 0
    result += bytes(4)  # exponent = 0
    result += (2 << 24).to_bytes(4, byteorder='big')  # class_sign = NaN
    return result


def serialize_bool(v: bool) -> bytes:
    """Serialize bool: 0x00=false, 0x01=true."""
    return bytes([0x01 if v else 0x00])


def canonicalize_dqa(value: int, scale: int) -> Tuple[int, int]:
    """
    Canonicalize DQA value per RFC-0105 §Canonical Representation.

    Strip trailing zeros from the value, adjusting scale accordingly.
    """
    if value == 0:
        return (0, 0)

    v = value
    s = scale
    while v % 10 == 0 and s > 0:
        v //= 10
        s -= 1
    return (v, s)


def serialize_dqa(value: int, scale: int) -> bytes:
    """
    Serialize DQA per RFC-0105.

    Format: value (16 bytes BE) || scale (1 byte) || reserved (7 bytes zero)

    CRITICAL: Canonicalize BEFORE serialization.
    """
    # Canonicalize per RFC-0105
    canon_value, canon_scale = canonicalize_dqa(value, scale)

    # TRAP: scale > 18 is invalid
    if canon_scale > 18:
        raise ValueError("DCS_INVALID_SCALE: scale > 18")

    # Serialize value: 16 bytes big-endian signed
    result = canon_value.to_bytes(16, byteorder='big', signed=True)
    # Append scale: 1 byte
    result += bytes([canon_scale])
    # Append reserved: 7 bytes zero
    result += bytes([0] * 7)

    return result


def serialize_trap_numeric() -> bytes:
    """
    Serialize numeric TRAP sentinel per RFC-0112.

    24-byte format: version(1) || scale(1) || reserved(3) || mantissa(16)
    TRAP = { mantissa: i128::MIN, scale: 0xFF }
    """
    # version = 0x01
    result = bytes([0x01])
    # scale = 0xFF (TRAP indicator)
    result += bytes([0xFF])
    # reserved = 3 bytes zero
    result += bytes([0, 0, 0])
    # mantissa = i128::MIN in big-endian (16 bytes)
    result += (0x80000000000000000000000000000000).to_bytes(16, byteorder='big', signed=False)
    return result


def serialize_string(s: str) -> bytes:
    """
    Serialize string with UTF-8 encoding.

    Format: u32_be length || UTF-8 bytes
    """
    utf8_bytes = s.encode('utf-8')
    return serialize_u32(len(utf8_bytes)) + utf8_bytes


def serialize_bytes(data: bytes) -> bytes:
    """
    Serialize raw bytes.

    Format: u32_be length || raw bytes
    """
    return serialize_u32(len(data)) + data


def serialize_option_none() -> bytes:
    """Serialize Option::None as 0x00."""
    return bytes([0x00])


def serialize_option_some(payload: bytes) -> bytes:
    """Serialize Option::Some as 0x01 || payload."""
    return bytes([0x01]) + payload


def serialize_enum(tag: int, payload: bytes) -> bytes:
    """Serialize enum as tag (u8) || payload."""
    return bytes([tag & 0xFF]) + payload


def serialize_dvec(elements: List[bytes]) -> bytes:
    """
    Serialize DVEC (Deterministic Vector).

    Format: u32_be length || element_0 || element_1 || ... || element_n
    Elements are in index order (0, 1, 2, ...).
    """
    result = serialize_u32(len(elements))
    for element in elements:
        result += element
    return result


def serialize_dmat(rows: int, cols: int, elements: List[bytes]) -> bytes:
    """
    Serialize DMAT (Deterministic Matrix).

    Format: u32_be rows || u32_be cols || element_0 || element_1 || ... (row-major)
    """
    result = serialize_u32(rows) + serialize_u32(cols)
    for element in elements:
        result += element
    return result


def merkle_root(leaves: List[bytes]) -> bytes:
    """
    Compute Merkle root from leaves.

    Uses pairwise hashing until single root remains.
    If odd number of leaves, duplicate last leaf.
    """
    current_level = [sha256(leaf) for leaf in leaves]

    while len(current_level) > 1:
        next_level = []
        for i in range(0, len(current_level), 2):
            if i + 1 < len(current_level):
                # Pair of two
                next_level.append(sha256(current_level[i] + current_level[i + 1]))
            else:
                # Duplicate last element (uneven case)
                next_level.append(sha256(current_level[i] + current_level[i]))

        current_level = next_level

    return current_level[0]


def build_probe() -> List[bytes]:
    """
    Build the 16-entry DCS verification probe.

    Returns list of 16 serialized entries.
    """
    entries = []

    # Entry 0: DQA canonicalization - DQA(1000, 3) -> canonicalize -> DQA(1, 0)
    # 1000 * 10^-3 = 1.0 -> canonical form is (1, 0)
    canon_val, canon_scale = canonicalize_dqa(1000, 3)
    assert canon_val == 1 and canon_scale == 0, f"Entry 0 canonicalization failed: ({canon_val}, {canon_scale})"
    entries.append(serialize_dqa(1000, 3))
    print(f"Entry 0: DQA(1000, 3) -> canonicalize -> DQA({canon_val}, {canon_scale})")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 1: DQA canonicalization - DQA(-5000, 4) -> canonicalize -> DQA(-5, 1)
    # -5000 * 10^-4 = -0.5 -> canonical form is (-5, 1)
    # -5000 -> -500 (scale=3) -> -50 (scale=2) -> -5 (scale=1)
    canon_val, canon_scale = canonicalize_dqa(-5000, 4)
    assert canon_val == -5 and canon_scale == 1, f"Entry 1 canonicalization failed: ({canon_val}, {canon_scale})"
    entries.append(serialize_dqa(-5000, 4))
    print(f"Entry 1: DQA(-5000, 4) -> canonicalize -> DQA({canon_val}, {canon_scale})")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 2: DVEC length + ordering - [1, 2, 3] -> serialize with length prefix
    dvec_elements = [
        serialize_dqa(1, 0),
        serialize_dqa(2, 0),
        serialize_dqa(3, 0)
    ]
    entries.append(serialize_dvec(dvec_elements))
    print(f"Entry 2: DVEC [1, 2, 3]")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 3: DMAT row-major traversal - 2x2 [[1,2],[3,4]]
    # Row-major: [1, 2, 3, 4] (row 0: [1,2], row 1: [3,4])
    dmat_elements = [
        serialize_dqa(1, 0),
        serialize_dqa(2, 0),
        serialize_dqa(3, 0),
        serialize_dqa(4, 0)
    ]
    entries.append(serialize_dmat(2, 2, dmat_elements))
    print(f"Entry 3: DMAT 2x2 [[1,2],[3,4]] (row-major: [1,2,3,4])")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 4: String UTF-8 encoding - "hello"
    entries.append(serialize_string("hello"))
    print(f'Entry 4: String "hello"')
    print(f'  Serialized: {entries[-1].hex()}')

    # Entry 5: Option::None
    entries.append(serialize_option_none())
    print(f"Entry 5: Option::None")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 6: Option::Some(true)
    entries.append(serialize_option_some(serialize_bool(True)))
    print(f"Entry 6: Option::Some(true)")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 7: Enum::Variant2(42)
    # Tag = 2, payload = serialize(42) = u32 big-endian
    entries.append(serialize_enum(2, serialize_u32(42)))
    print(f"Entry 7: Enum::Variant2(42)")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 8: Bool True
    entries.append(serialize_bool(True))
    print(f"Entry 8: Bool true")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 9: Bool False
    entries.append(serialize_bool(False))
    print(f"Entry 9: Bool false")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 10: Numeric TRAP (24-byte format per RFC-0112)
    entries.append(serialize_trap_numeric())
    print(f"Entry 10: Numeric TRAP (24-byte per RFC-0112)")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 11: Bool TRAP (1-byte 0xFF)
    entries.append(bytes([0xFF]))  # TRAP sentinel for bool/enum
    print(f"Entry 11: Bool TRAP (1-byte 0xFF)")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 12: I128 Positive (42)
    entries.append(serialize_i128(42))
    print(f"Entry 12: I128 positive (42)")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 13: I128 Negative (-42)
    entries.append(serialize_i128(-42))
    print(f"Entry 13: I128 negative (-42)")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 14: BIGINT Positive (42) - RFC-0110 BigIntEncoding
    entries.append(serialize_bigint(42))
    print(f"Entry 14: BIGINT positive (42)")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 15: DFP (42.0) - RFC-0104 DfpEncoding
    # DFP(42.0) = mantissa=42, exponent=0, class=Normal(0), sign=positive(0)
    entries.append(serialize_dfp(42, 0, 0, False))
    print(f"Entry 15: DFP 42.0 (mantissa=42, exp=0, class=Normal)")
    print(f"  Serialized: {entries[-1].hex()}")

    return entries


def main():
    """Main entry point."""
    print("=" * 70)
    print("DCS (Deterministic Canonical Serialization) Probe Root Computation")
    print("=" * 70)
    print()

    # Build the 15 probe entries
    entries = build_probe()

    print()
    print("=" * 70)
    print("Probe Entry Leaf Hashes (SHA256):")
    print("=" * 70)

    leaf_hashes = []
    for i, entry in enumerate(entries):
        leaf_hash = sha256(entry)
        leaf_hashes.append(leaf_hash)
        print(f"  Entry {i}: {leaf_hash.hex()}")

    print()
    print("=" * 70)
    print("Merkle Root Computation:")
    print("=" * 70)

    root = merkle_root(entries)
    print(f"  Merkle Root: {root.hex()}")
    print()

    # Verify determinism: run again
    print("=" * 70)
    print("Determinism Verification (re-running):")
    print("=" * 70)
    entries2 = build_probe()
    root2 = merkle_root(entries2)
    print(f"  Second Merkle Root: {root2.hex()}")
    print(f"  Deterministic: {root == root2}")

    if root != root2:
        raise RuntimeError("ERROR: Results are NOT deterministic!")

    print()
    print("=" * 70)
    print(f"AUTHORITATIVE MERKLE ROOT: {root.hex()}")
    print("=" * 70)

    # Verify against known root
    EXPECTED_ROOT = "f9103bb1250213f895f3633bc68e2e15eeebad5372160a0c9266cb90837956af"
    assert root.hex() == EXPECTED_ROOT, f"Merkle root mismatch: got {root.hex()}"
    print(f"  ✓ Root matches EXPECTED_ROOT")

    return root.hex()


if __name__ == "__main__":
    main()
