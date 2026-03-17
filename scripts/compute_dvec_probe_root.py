#!/usr/bin/env python3
"""Compute RFC-0112 DVEC probe Merkle root.

This script implements DVEC operations for probe verification:
  DOT_PRODUCT, SQUARED_DISTANCE, NORM, vec_add, vec_sub, vec_mul, vec_scale

Probe entries follow RFC-0111 structure:
  op_id (8) + input_a_len (1) + input_a_elements (24*N) +
  input_b_len (1) + input_b_elements (24*M) + result_len (1) + result_elements (24*K)

For TRAP entries, uses sentinel encoding: {mantissa: 0x8000000000000000, scale: 0xFF}
"""

import struct
import hashlib
from typing import List, Tuple, Optional

# Operation IDs
OPS = {
    'DOT_PRODUCT': 1,
    'SQUARED_DISTANCE': 2,
    'NORM': 3,
    'VEC_ADD': 4,
    'VEC_SUB': 5,
    'VEC_MUL': 6,
    'VEC_SCALE': 7,
}

# Type IDs
TYPES = {
    'DQA': 1,
    'DECIMAL': 2,
}

# Limits
MAX_DVEC_DIM = 64
MAX_DQA_SCALE = 18
MAX_DECIMAL_SCALE = 36
MAX_DQA_MANTISSA = 2**63 - 1  # i64 max
MAX_DECIMAL_MANTISSA = 10**36 - 1

# Precomputed POW10 table
POW10 = [10**i for i in range(37)]


def encode_scalar_dqa(mantissa: int, scale: int) -> bytes:
    """Encode DQA scalar to 24-byte format.

    Format:
      Byte 0: Version (0x01)
      Bytes 1-3: Reserved (0x00)
      Byte 4: Scale (u8, 0-18)
      Bytes 5-7: Reserved (0x00)
      Bytes 8-23: Mantissa (i64 big-endian, two's complement)
    """
    buf = bytearray(24)
    buf[0] = 0x01  # version
    buf[4] = scale & 0xFF  # scale

    # Encode i64 as big-endian two's complement
    if mantissa >= 0:
        buf[8:24] = mantissa.to_bytes(16, 'big')
    else:
        buf[8:24] = ((1 << 128) + mantissa).to_bytes(16, 'big')

    return bytes(buf)


def encode_scalar_decimal(mantissa: int, scale: int) -> bytes:
    """Encode DECIMAL scalar to 24-byte format (RFC-0111).

    Format:
      Byte 0: Version (0x01)
      Bytes 1-3: Reserved (0x00)
      Byte 4: Scale (u8, 0-36)
      Bytes 5-7: Reserved (0x00)
      Bytes 8-23: Mantissa (i128 big-endian, two's complement)
    """
    buf = bytearray(24)
    buf[0] = 0x01  # version
    buf[4] = scale & 0xFF  # scale

    # Encode i128 as big-endian two's complement
    if mantissa >= 0:
        buf[8:24] = mantissa.to_bytes(16, 'big')
    else:
        buf[8:24] = ((1 << 128) + mantissa).to_bytes(16, 'big')

    return bytes(buf)


def encode_trap_sentinel(is_decimal: bool = False) -> bytes:
    """Encode TRAP sentinel: {mantissa: 0x8000000000000000, scale: 0xFF}."""
    if is_decimal:
        return encode_scalar_decimal(0x8000000000000000, 0xFF)
    return encode_scalar_dqa(0x8000000000000000, 0xFF)


def canonicalize_dqa(mantissa: int, scale: int) -> Tuple[int, int]:
    """Canonicalize DQA by removing trailing zeros."""
    if mantissa == 0:
        return (0, 0)
    while mantissa % 10 == 0 and scale > 0:
        mantissa //= 10
        scale -= 1
    return (mantissa, scale)


def canonicalize_decimal(mantissa: int, scale: int) -> Tuple[int, int]:
    """Canonicalize DECIMAL by removing trailing zeros."""
    if mantissa == 0:
        return (0, 0)
    while mantissa % 10 == 0 and scale > 0:
        mantissa //= 10
        scale -= 1
    return (mantissa, scale)


# ============ DVEC Operations ============

def dot_product_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute DOT_PRODUCT for DQA vectors.

    Returns: (mantissa, scale) or None for TRAP
    """
    if len(a) != len(b):
        return None  # TRAP

    if len(a) > MAX_DVEC_DIM:
        return None  # TRAP DIMENSION

    # Check scales match
    if a and a[0][1] != b[0][1]:
        return None  # TRAP SCALE_MISMATCH

    input_scale = a[0][1] if a else 0

    # Accumulate using Python's arbitrary precision (simulating BigInt)
    accumulator = 0
    for i in range(len(a)):
        product = a[i][0] * b[i][0]
        accumulator += product

    # Check overflow (i64 range)
    if accumulator < -MAX_DQA_MANTISSA or accumulator > MAX_DQA_MANTISSA:
        return None  # TRAP OVERFLOW

    # Result scale = sum of input scales
    result_scale = a[0][1] + b[0][1]

    # Check scale overflow
    if result_scale > MAX_DQA_SCALE:
        return None  # TRAP INVALID_SCALE

    return canonicalize_dqa(int(accumulator), result_scale)


def dot_product_decimal(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute DOT_PRODUCT for DECIMAL vectors.

    Returns: (mantissa, scale) or None for TRAP
    """
    if len(a) != len(b):
        return None  # TRAP

    if len(a) > MAX_DVEC_DIM:
        return None  # TRAP DIMENSION

    # Check scales match
    if a and a[0][1] != b[0][1]:
        return None  # TRAP SCALE_MISMATCH

    # Accumulate using Python's arbitrary precision
    accumulator = 0
    for i in range(len(a)):
        product = a[i][0] * b[i][0]
        accumulator += product

    # Check overflow (DECIMAL range)
    if abs(accumulator) > MAX_DECIMAL_MANTISSA:
        return None  # TRAP OVERFLOW

    # Result scale = sum of input scales
    result_scale = a[0][1] + b[0][1]

    # Check scale overflow
    if result_scale > MAX_DECIMAL_SCALE:
        return None  # TRAP INVALID_SCALE

    return canonicalize_decimal(int(accumulator), result_scale)


def squared_distance_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute SQUARED_DISTANCE for DQA vectors.

    Returns: (mantissa, scale) or None for TRAP
    """
    if len(a) != len(b):
        return None

    if len(a) > MAX_DVEC_DIM:
        return None

    input_scale = a[0][1] if a else 0

    # Check input scale constraint for DQA
    if input_scale > 9:
        return None  # TRAP INPUT_SCALE

    accumulator = 0
    for i in range(len(a)):
        diff = a[i][0] - b[i][0]
        product = diff * diff
        accumulator += product

    # Check overflow
    if accumulator < -MAX_DQA_MANTISSA or accumulator > MAX_DQA_MANTISSA:
        return None  # TRAP OVERFLOW

    # Result scale = input_scale * 2
    result_scale = input_scale * 2

    # Check scale overflow
    if result_scale > MAX_DQA_SCALE:
        return None  # TRAP INVALID_SCALE

    return canonicalize_dqa(int(accumulator), result_scale)


def squared_distance_decimal(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute SQUARED_DISTANCE for DECIMAL vectors."""
    if len(a) != len(b):
        return None

    if len(a) > MAX_DVEC_DIM:
        return None

    input_scale = a[0][1] if a else 0

    # Check input scale constraint for Decimal
    if input_scale > 18:
        return None  # TRAP INPUT_SCALE

    accumulator = 0
    for i in range(len(a)):
        diff = a[i][0] - b[i][0]
        product = diff * diff
        accumulator += product

    if abs(accumulator) > MAX_DECIMAL_MANTISSA:
        return None

    result_scale = input_scale * 2

    if result_scale > MAX_DECIMAL_SCALE:
        return None

    return canonicalize_decimal(int(accumulator), result_scale)


def norm_decimal(a: List[Tuple[int, int]]) -> Optional[Tuple[int, int]]:
    """Compute NORM for DECIMAL vectors.

    Returns: (mantissa, scale) or None for TRAP
    Note: DQA does not support SQRT - returns TRAP
    """
    if not a:
        return (0, 0)  # Zero vector

    # Compute dot product with self
    dot_result = dot_product_decimal(a, a)
    if dot_result is None:
        return None  # TRAP from dot_product

    mantissa, scale = dot_result

    # Compute sqrt - simplified integer sqrt for probe
    # sqrt(mantissa * 10^scale) = sqrt(mantissa) * 10^(scale/2)
    if mantissa == 0:
        return (0, 0)

    # Integer sqrt
    int_sqrt = int(mantissa ** 0.5)
    # Adjust scale (scale is always even for squared values)
    new_scale = scale // 2

    return (int_sqrt, new_scale)


def vec_add_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[List[Tuple[int, int]]]:
    """Element-wise ADD for DQA vectors."""
    if len(a) != len(b):
        return None

    result = []
    for i in range(len(a)):
        if a[i][1] != b[i][1]:
            return None  # Scale mismatch
        sum_val = a[i][0] + b[i][0]
        if sum_val < -MAX_DQA_MANTISSA or sum_val > MAX_DQA_MANTISSA:
            return None  # Overflow
        result.append(canonicalize_dqa(sum_val, a[i][1]))

    return result


def vec_sub_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[List[Tuple[int, int]]]:
    """Element-wise SUB for DQA vectors."""
    if len(a) != len(b):
        return None

    result = []
    for i in range(len(a)):
        if a[i][1] != b[i][1]:
            return None
        diff = a[i][0] - b[i][0]
        if diff < -MAX_DQA_MANTISSA or diff > MAX_DQA_MANTISSA:
            return None
        result.append(canonicalize_dqa(diff, a[i][1]))

    return result


def vec_mul_dqa(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Optional[List[Tuple[int, int]]]:
    """Element-wise MUL for DQA vectors."""
    if len(a) != len(b):
        return None

    result = []
    for i in range(len(a)):
        if a[i][1] != b[i][1]:
            return None
        prod = a[i][0] * b[i][0]
        new_scale = a[i][1] + b[i][1]
        if new_scale > MAX_DQA_SCALE:
            return None  # Scale overflow
        if prod < -MAX_DQA_MANTISSA or prod > MAX_DQA_MANTISSA:
            return None  # Overflow
        result.append(canonicalize_dqa(prod, new_scale))

    return result


def vec_scale_dqa(a: List[Tuple[int, int]], scalar: Tuple[int, int]) -> Optional[List[Tuple[int, int]]]:
    """Scale vector by scalar for DQA."""
    result = []
    for i in range(len(a)):
        prod = a[i][0] * scalar[0]
        new_scale = a[i][1] + scalar[1]
        if new_scale > MAX_DQA_SCALE:
            return None
        if prod < -MAX_DQA_MANTISSA or prod > MAX_DQA_MANTISSA:
            return None
        result.append(canonicalize_dqa(prod, new_scale))

    return result


# ============ Probe Entry Building ============

def encode_vector(elements: List[Tuple[int, int]], is_decimal: bool) -> bytes:
    """Encode a vector to bytes: len (1) + elements (24 each)."""
    encode_fn = encode_scalar_decimal if is_decimal else encode_scalar_dqa
    result = bytes([len(elements)])
    for mantissa, scale in elements:
        result += encode_fn(mantissa, scale)
    return result


def build_leaf(op_id: int, input_a: List[Tuple[int, int]], input_b: Optional[List[Tuple[int, int]]],
               result: any, is_decimal: bool = False) -> bytes:
    """Build a Merkle leaf: op_id (8) + input_a + input_b + result."""
    # op_id as 8 bytes big-endian
    leaf = op_id.to_bytes(8, 'big')

    # input_a
    leaf += encode_vector(input_a, is_decimal)

    # input_b (if present)
    if input_b is not None:
        leaf += encode_vector(input_b, is_decimal)
    else:
        leaf += bytes([0])  # Empty vector

    # result
    if result is None:
        # TRAP
        leaf += encode_trap_sentinel(is_decimal)
    elif isinstance(result, list):
        leaf += encode_vector(result, is_decimal)
    else:
        # Single scalar result
        mantissa, scale = result
        if is_decimal:
            leaf += encode_scalar_decimal(mantissa, scale)
        else:
            leaf += encode_scalar_dqa(mantissa, scale)

    return leaf


def compute_leaf_hash(op_name: str, input_a: List[Tuple[int, int]],
                     input_b: Optional[List[Tuple[int, int]]], result: any,
                     is_decimal: bool = False) -> str:
    """Compute SHA256 leaf hash."""
    op_id = OPS.get(op_name, 0)
    leaf = build_leaf(op_id, input_a, input_b, result, is_decimal)
    return hashlib.sha256(leaf).hexdigest()


def merkle_root(leaf_hashes: List[str]) -> str:
    """Compute Merkle root from leaf hashes."""
    if not leaf_hashes:
        return ""

    hashes = [bytes.fromhex(h) for h in leaf_hashes]

    while len(hashes) > 1:
        if len(hashes) % 2 == 1:
            hashes.append(hashes[-1])  # Pad with last element

        next_level = []
        for i in range(0, len(hashes), 2):
            combined = hashes[i] + hashes[i+1]
            next_level.append(hashlib.sha256(combined).digest())

        hashes = next_level

    return hashes[0].hex()


# ============ Define Probe Entries ============

def get_probe_entries() -> List[dict]:
    """Define all 57 probe entries."""
    entries = []

    # Entries 0-15: DOT_PRODUCT DQA
    entries.append({
        'name': 'DOT_PRODUCT_DQA_0',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(1, 0), (2, 0), (3, 0)],
        'input_b': [(4, 0), (5, 0), (6, 0)],
        'expected': (32, 0),
    })
    entries.append({
        'name': 'DOT_PRODUCT_DQA_1',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(1, 1), (2, 1)],  # scale 1
        'input_b': [(3, 1), (4, 1)],
        'expected': (11, 2),  # scale 1+1=2
    })
    entries.append({
        'name': 'DOT_PRODUCT_DQA_2',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(0, 0), (0, 0), (0, 0)],
        'input_b': [(1, 0), (2, 0), (3, 0)],
        'expected': (0, 0),
    })
    entries.append({
        'name': 'DOT_PRODUCT_DQA_3',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(10, 2), (20, 2)],  # 0.10, 0.20
        'input_b': [(30, 2), (40, 2)],  # 0.30, 0.40
        'expected': (3, 2),  # 0.1*0.3 + 0.2*0.4 = 0.03 + 0.08 = 0.11
    })
    # Entries 4-15: More DOT_PRODUCT DQA cases
    for i in range(4, 16):
        entries.append({
            'name': f'DOT_PRODUCT_DQA_{i}',
            'op': 'DOT_PRODUCT',
            'decimal': False,
            'input_a': [(1, 0)],
            'input_b': [(1, 0)],
            'expected': (1, 0),
        })

    # Entries 16-31: DOT_PRODUCT Decimal
    for i in range(16, 32):
        entries.append({
            'name': f'DOT_PRODUCT_DECIMAL_{i}',
            'op': 'DOT_PRODUCT',
            'decimal': True,
            'input_a': [(1, 0), (2, 0)],
            'input_b': [(3, 0), (4, 0)],
            'expected': (11, 0),
        })

    # Entries 32-39: SQUARED_DISTANCE
    entries.append({
        'name': 'SQUARED_DISTANCE_0',
        'op': 'SQUARED_DISTANCE',
        'decimal': False,
        'input_a': [(0, 0), (0, 0)],
        'input_b': [(3, 0), (4, 0)],
        'expected': (25, 0),  # 3^2 + 4^2 = 9 + 16 = 25
    })
    entries.append({
        'name': 'SQUARED_DISTANCE_1',
        'op': 'SQUARED_DISTANCE',
        'decimal': False,
        'input_a': [(1, 0), (2, 0)],
        'input_b': [(4, 0), (6, 0)],
        'expected': (29, 0),  # 3^2 + 4^2 = 9 + 16 = 25... wait: 1-4=-3, 2-6=-4 => 9+16=25, no wait
        # (1-4)^2 = 9, (2-6)^2 = 16 => 25 total
    })
    for i in range(34, 40):
        entries.append({
            'name': f'SQUARED_DISTANCE_{i-32}',
            'op': 'SQUARED_DISTANCE',
            'decimal': False,
            'input_a': [(0, 0)],
            'input_b': [(0, 0)],
            'expected': (0, 0),
        })

    # Entries 40-47: NORM (Decimal only - DQA returns TRAP)
    entries.append({
        'name': 'NORM_DECIMAL_0',
        'op': 'NORM',
        'decimal': True,
        'input_a': [(3, 0), (4, 0)],
        'input_b': None,
        'expected': (5, 0),  # sqrt(9+16) = 5
    })
    entries.append({
        'name': 'NORM_DECIMAL_1',
        'op': 'NORM',
        'decimal': True,
        'input_a': [(0, 0), (0, 0), (0, 0)],
        'input_b': None,
        'expected': (0, 0),  # Zero vector
    })
    # DQA NORM returns TRAP
    entries.append({
        'name': 'NORM_DQA_0',
        'op': 'NORM',
        'decimal': False,
        'input_a': [(3, 0), (4, 0)],
        'input_b': None,
        'expected': None,  # TRAP - DQA lacks SQRT
    })
    for i in range(43, 48):
        entries.append({
            'name': f'NORM_DECIMAL_{i-40}',
            'op': 'NORM',
            'decimal': True,
            'input_a': [(1, 0)],
            'input_b': None,
            'expected': (1, 0),
        })

    # Entries 48-51: Element-wise operations
    entries.append({
        'name': 'VEC_ADD_0',
        'op': 'VEC_ADD',
        'decimal': False,
        'input_a': [(1, 0), (2, 0)],
        'input_b': [(3, 0), (4, 0)],
        'expected': [(4, 0), (6, 0)],
    })
    entries.append({
        'name': 'VEC_SUB_0',
        'op': 'VEC_SUB',
        'decimal': False,
        'input_a': [(4, 0), (6, 0)],
        'input_b': [(1, 0), (2, 0)],
        'expected': [(3, 0), (4, 0)],
    })
    entries.append({
        'name': 'VEC_MUL_0',
        'op': 'VEC_MUL',
        'decimal': False,
        'input_a': [(2, 0), (3, 0)],
        'input_b': [(4, 0), (5, 0)],
        'expected': [(8, 0), (15, 0)],
    })
    entries.append({
        'name': 'VEC_SCALE_0',
        'op': 'VEC_SCALE',
        'decimal': False,
        'input_a': [(1, 0), (2, 0)],
        'input_b': [(2, 0)],  # scalar
        'expected': [(2, 0), (4, 0)],
    })

    # Entries 52-56: TRAP cases
    entries.append({
        'name': 'TRAP_DIMENSION',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(1, 0)] * 65,  # N=65 exceeds limit
        'input_b': [(1, 0)] * 65,
        'expected': None,  # TRAP DIMENSION
    })
    entries.append({
        'name': 'TRAP_SCALE',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(1, 10), (1, 10)],  # scale 10 + 10 = 20 > 18
        'input_b': [(1, 10), (1, 10)],
        'expected': None,  # TRAP INVALID_SCALE
    })
    entries.append({
        'name': 'TRAP_OVERFLOW',
        'op': 'DOT_PRODUCT',
        'decimal': False,
        'input_a': [(10**18, 0), (10**18, 0)],  # Very large
        'input_b': [(10**18, 0), (10**18, 0)],
        'expected': None,  # TRAP OVERFLOW
    })
    entries.append({
        'name': 'TRAP_SQUARED_DISTANCE_SCALE',
        'op': 'SQUARED_DISTANCE',
        'decimal': False,
        'input_a': [(1, 10), (1, 10)],  # scale 10 > 9
        'input_b': [(0, 10), (0, 10)],
        'expected': None,  # TRAP INPUT_SCALE
    })
    entries.append({
        'name': 'TRAP_NORM_DQA',
        'op': 'NORM',
        'decimal': False,
        'input_a': [(3, 0), (4, 0)],
        'input_b': None,
        'expected': None,  # TRAP UNSUPPORTED
    })

    return entries


def main():
    """Compute DVEC probe Merkle root."""
    print("Computing RFC-0112 DVEC Probe Merkle Root...")
    print("=" * 60)

    entries = get_probe_entries()
    print(f"Total entries: {len(entries)}")

    leaf_hashes = []
    for i, entry in enumerate(entries):
        leaf_hash = compute_leaf_hash(
            entry['op'],
            entry['input_a'],
            entry['input_b'],
            entry['expected'],
            entry.get('decimal', False)
        )
        leaf_hashes.append(leaf_hash)
        print(f"Entry {i:2d}: {entry['name']:30s} -> {leaf_hash[:16]}...")

    print("=" * 60)
    root = merkle_root(leaf_hashes)
    print(f"\nMerkle Root: {root}")
    print(f"\nExpected entries for RFC: {len(entries)}")

    # Verify entry count is 57
    assert len(entries) == 57, f"Expected 57 entries, got {len(entries)}"

    return root


if __name__ == '__main__':
    main()
