#!/usr/bin/env python3
"""
DMAT Probe Root Computation

Computes Merkle root for RFC-0113 DMAT verification probe.
Reference implementation - the script is canonical.

Usage: python3 scripts/compute_dmat_probe_root.py
"""

import hashlib
from typing import Tuple, List, Optional, Union

# TRAP sentinel for probe encoding
TRAP = (-(1 << 63), 0xFF)   # i64::MIN as signed Python int

def dqa_encode(mantissa: int, scale: int) -> bytes:
    """Encode DQA scalar as 24-byte probe element.

    Format: version(1) || reserved(3) || scale(1) || reserved(3) || mantissa(16)
    """
    if mantissa < 0:
        # Sign-extend to i128 two's complement
        mantissa = (1 << 128) + mantissa
    return (
        b'\x01' +                    # version
        b'\x00' * 3 +               # reserved
        bytes([scale]) +            # scale
        b'\x00' * 3 +               # reserved
        mantissa.to_bytes(16, 'big')  # mantissa as big-endian i128
    )

def mat_encode(rows: int, cols: int, elements: List[Tuple[int, int]]) -> bytes:
    """Encode matrix for probe.

    Format: rows(1) || cols(1) || element[0] || element[1] || ...
    """
    result = bytes([rows, cols])
    for mantissa, scale in elements:
        result += dqa_encode(mantissa, scale)
    return result

def vec_encode(elements: List[Tuple[int, int]]) -> bytes:
    """Encode vector for probe."""
    result = bytes([len(elements), 1])  # len and dummy cols
    for mantissa, scale in elements:
        result += dqa_encode(mantissa, scale)
    return result

def encode_data(data):
    """Encode matrix, vector, or scalar data for probe.

    Matrix: tuple of (rows, cols, elements) where elements is list of (mantissa, scale)
    Vector: list of (mantissa, scale) tuples
    Scalar: tuple of (mantissa, scale) - single DQA value
    """
    if data is None:
        return bytes([0, 0])  # empty for unary ops
    if isinstance(data, list):
        # Vector: list of (mantissa, scale) tuples
        return vec_encode(data)
    elif isinstance(data, tuple) and len(data) == 2 and isinstance(data[1], int):
        # Scalar: tuple of (mantissa, scale) - single DQA value
        return bytes([1, 1]) + dqa_encode(data[0], data[1])  # MED-4: rows/cols prefix for wire format
    else:
        # Matrix: tuple of (rows, cols, elements)
        return mat_encode(*data)

def leaf_hash(
    op_id: int,
    type_id: int,
    a_data: Tuple[int, int, List[Tuple[int, int]]],  # (rows, cols, elements)
    b_data: Optional[Union[Tuple[int, int, List[Tuple[int, int]]], List[Tuple[int, int]]]],  # Matrix or Vector (MED-4)
    c_data: Union[Tuple[int, int, List[Tuple[int, int]]], List[Tuple[int, int]]]  # Matrix or Vector
) -> bytes:
    """Compute SHA256 leaf hash for probe entry.

    Format: op_id(8) || type_id(1) || a_mat || b_mat || c_mat
    """
    a_mat = mat_encode(*a_data)
    b_mat = encode_data(b_data)
    c_mat = encode_data(c_data)

    leaf_input = (
        op_id.to_bytes(8, 'big') +
        bytes([type_id]) +
        a_mat +
        b_mat +
        c_mat
    )
    return hashlib.sha256(leaf_input).digest()

def merkle_root(leaves: List[bytes]) -> bytes:
    """Compute Merkle root from leaf hashes using SHA256."""
    if not leaves:
        return bytes(32)

    current_level = leaves[:]
    while len(current_level) > 1:
        if len(current_level) % 2 == 1:
            current_level.append(current_level[-1])  # Duplicate last for odd

        next_level = []
        for left, right in zip(current_level[0::2], current_level[1::2]):
            next_level.append(hashlib.sha256(left + right).digest())

        current_level = next_level

    return current_level[0]

# Operation IDs
OP_MAT_ADD = 0x0100
OP_MAT_SUB = 0x0101
OP_MAT_MUL = 0x0102
OP_MAT_VEC_MUL = 0x0103
OP_MAT_TRANSPOSE = 0x0104
OP_MAT_SCALE = 0x0105

# Type IDs
TYPE_DQA = 1
TYPE_DECIMAL = 2

def dqa(mantissa: int, scale: int = 0) -> Tuple[int, int]:
    """Helper to create DQA scalar tuple."""
    return (mantissa, scale)

def mat(rows: int, cols: int, *elements) -> Tuple[int, int, List[Tuple[int, int]]]:
    """Helper to create matrix tuple."""
    return (rows, cols, list(elements))

# Probe entries (64 total)
# Format: (op_id, type_id, a_mat, b_mat, c_mat)
# b_mat is None for unary operations
PROBE_ENTRIES = [
    # Entries 0-9: MAT_ADD DQA
    (OP_MAT_ADD, TYPE_DQA, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                        mat(2, 2, dqa(5), dqa(6), dqa(7), dqa(8)),
                        mat(2, 2, dqa(6), dqa(8), dqa(10), dqa(12))),
    (OP_MAT_ADD, TYPE_DQA, mat(1, 2, dqa(1), dqa(2)),
                        mat(1, 2, dqa(3), dqa(4)),
                        mat(1, 2, dqa(4), dqa(6))),
    # MED-1: Cross-matrix scale TRAP test — a has scale=5, b has scale=10
    (OP_MAT_ADD, TYPE_DQA, mat(2, 2, dqa(1, 5), dqa(2, 5), dqa(3, 5), dqa(4, 5)),
                        mat(2, 2, dqa(5, 10), dqa(6, 10), dqa(7, 10), dqa(8, 10)),
                        mat(2, 2, TRAP, TRAP, TRAP, TRAP)),  # SCALE_MISMATCH
    (OP_MAT_ADD, TYPE_DQA, mat(2, 2, dqa(10), dqa(20), dqa(30), dqa(40)),
                        mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                        mat(2, 2, dqa(11), dqa(22), dqa(33), dqa(44))),
    (OP_MAT_ADD, TYPE_DQA, mat(3, 2, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6)),
                        mat(3, 2, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6)),
                        mat(3, 2, dqa(2), dqa(4), dqa(6), dqa(8), dqa(10), dqa(12))),
    (OP_MAT_ADD, TYPE_DQA, mat(2, 3, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6)),
                        mat(2, 3, dqa(6), dqa(5), dqa(4), dqa(3), dqa(2), dqa(1)),
                        mat(2, 3, dqa(7), dqa(7), dqa(7), dqa(7), dqa(7), dqa(7))),
    (OP_MAT_ADD, TYPE_DQA, mat(4, 1, dqa(1), dqa(2), dqa(3), dqa(4)),
                        mat(4, 1, dqa(4), dqa(3), dqa(2), dqa(1)),
                        mat(4, 1, dqa(5), dqa(5), dqa(5), dqa(5))),
    (OP_MAT_ADD, TYPE_DQA, mat(1, 4, dqa(1), dqa(2), dqa(3), dqa(4)),
                        mat(1, 4, dqa(1), dqa(2), dqa(3), dqa(4)),
                        mat(1, 4, dqa(2), dqa(4), dqa(6), dqa(8))),
    (OP_MAT_ADD, TYPE_DQA, mat(2, 2, dqa(100), dqa(200), dqa(300), dqa(400)),
                        mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                        mat(2, 2, dqa(101), dqa(202), dqa(303), dqa(404))),
    (OP_MAT_ADD, TYPE_DQA, mat(3, 3, dqa(1), dqa(1), dqa(1), dqa(1), dqa(1), dqa(1), dqa(1), dqa(1), dqa(1)),
                        mat(3, 3, dqa(2), dqa(2), dqa(2), dqa(2), dqa(2), dqa(2), dqa(2), dqa(2), dqa(2)),
                        mat(3, 3, dqa(3), dqa(3), dqa(3), dqa(3), dqa(3), dqa(3), dqa(3), dqa(3), dqa(3))),

    # Entries 10-19: MAT_MUL DQA
    (OP_MAT_MUL, TYPE_DQA, mat(2, 2, dqa(1), dqa(0), dqa(0), dqa(1)),
                        mat(2, 2, dqa(2), dqa(3), dqa(4), dqa(5)),
                        mat(2, 2, dqa(2), dqa(3), dqa(4), dqa(5))),
    (OP_MAT_MUL, TYPE_DQA, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                        mat(2, 2, dqa(5), dqa(6), dqa(7), dqa(8)),
                        mat(2, 2, dqa(19), dqa(22), dqa(43), dqa(50))),
    (OP_MAT_MUL, TYPE_DQA, mat(1, 3, dqa(1), dqa(2), dqa(3)),
                        mat(3, 1, dqa(1), dqa(2), dqa(3)),
                        mat(1, 1, dqa(14))),
    (OP_MAT_MUL, TYPE_DQA, mat(2, 2, dqa(2), dqa(2), dqa(2), dqa(2)),
                        mat(2, 2, dqa(3), dqa(3), dqa(3), dqa(3)),
                        mat(2, 2, dqa(12), dqa(12), dqa(12), dqa(12))),
    (OP_MAT_MUL, TYPE_DQA, mat(2, 3, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6)),  # 2×3
                        mat(3, 2, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6)),  # 3×2
                        mat(2, 2, dqa(22), dqa(28), dqa(49), dqa(64))),  # MED-3/4: 2×2 = [[22,28],[49,64]]
    # MED-1: Result should be [[1,2],[3,4]] - A extracts rows 0,1 of B (identity rows)
    # A = [[1,0,0,0],[0,1,0,0]] selects rows 0,1 of B = [[1,2],[3,4]]
    (OP_MAT_MUL, TYPE_DQA, mat(2, 4, dqa(1), dqa(0), dqa(0), dqa(0), dqa(0), dqa(1), dqa(0), dqa(0)),
                        mat(4, 2, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6), dqa(7), dqa(8)),
                        mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4))),
    (OP_MAT_MUL, TYPE_DQA, mat(1, 2, dqa(10), dqa(20)),
                        mat(2, 1, dqa(3), dqa(4)),
                        mat(1, 1, dqa(110))),
    (OP_MAT_MUL, TYPE_DQA, mat(2, 1, dqa(3), dqa(4)),
                        mat(1, 2, dqa(10), dqa(20)),
                        mat(2, 2, dqa(30), dqa(60), dqa(40), dqa(80))),
    (OP_MAT_MUL, TYPE_DQA, mat(3, 1, dqa(1), dqa(2), dqa(3)),
                        mat(1, 3, dqa(1), dqa(2), dqa(3)),
                        mat(3, 3, dqa(1), dqa(2), dqa(3), dqa(2), dqa(4), dqa(6), dqa(3), dqa(6), dqa(9))),
    (OP_MAT_MUL, TYPE_DQA, mat(2, 2, dqa(5), dqa(5), dqa(5), dqa(5)),
                        mat(2, 2, dqa(5), dqa(5), dqa(5), dqa(5)),
                        mat(2, 2, dqa(50), dqa(50), dqa(50), dqa(50))),

    # Entries 20-29: MAT_VEC_MUL and MAT_TRANSPOSE DQA
    (OP_MAT_VEC_MUL, TYPE_DQA, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                         [dqa(1), dqa(1)],
                         [dqa(3), dqa(7)]),
    (OP_MAT_VEC_MUL, TYPE_DQA, mat(2, 3, dqa(1), dqa(0), dqa(0), dqa(0), dqa(1), dqa(0)),
                         [dqa(1), dqa(2), dqa(3)],
                         [dqa(1), dqa(2)]),
    # CRIT-1: [[1,2,3],[4,5,6],[7,8,9]] × [1,1,1] = [6,15,24] (not [12,15,18])
    (OP_MAT_VEC_MUL, TYPE_DQA, mat(3, 3, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6), dqa(7), dqa(8), dqa(9)),
                         [dqa(1), dqa(1), dqa(1)],
                         [dqa(6), dqa(15), dqa(24)]),
    # DIMENSION_MISMATCH - matrix 1×4, vector [2] has 1 element (cols=4 ≠ vec_len=1)
    (OP_MAT_VEC_MUL, TYPE_DQA, mat(1, 4, dqa(2), dqa(4), dqa(6), dqa(8)),
                         [dqa(2)],
                         [TRAP]),
    # MED-2: Change Input A from 4×1 to 1×4 to match vector [1,2,3,4]
    # Result: 1×4 dot 1×4 = 1×1 = 1+4+9+16 = 30
    # MED-3: Result should be [30] - 1×4 dot 4×1 = 1×1 = 1+4+9+16 = 30
    (OP_MAT_VEC_MUL, TYPE_DQA, mat(1, 4, dqa(1), dqa(2), dqa(3), dqa(4)),
                         [dqa(1), dqa(2), dqa(3), dqa(4)],
                         [dqa(30)]),
    (OP_MAT_TRANSPOSE, TYPE_DQA, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)), None,
                         mat(2, 2, dqa(1), dqa(3), dqa(2), dqa(4))),
    (OP_MAT_TRANSPOSE, TYPE_DQA, mat(1, 3, dqa(1), dqa(2), dqa(3)), None,
                         mat(3, 1, dqa(1), dqa(2), dqa(3))),
    (OP_MAT_TRANSPOSE, TYPE_DQA, mat(3, 1, dqa(1), dqa(2), dqa(3)), None,
                         mat(1, 3, dqa(1), dqa(2), dqa(3))),
    (OP_MAT_TRANSPOSE, TYPE_DQA, mat(2, 3, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6)), None,
                         mat(3, 2, dqa(1), dqa(4), dqa(2), dqa(5), dqa(3), dqa(6))),
    (OP_MAT_TRANSPOSE, TYPE_DQA, mat(4, 2, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6), dqa(7), dqa(8)), None,
                         mat(2, 4, dqa(1), dqa(3), dqa(5), dqa(7), dqa(2), dqa(4), dqa(6), dqa(8))),

    # Entries 30-39: MAT_SCALE and Decimal operations
    (OP_MAT_SCALE, TYPE_DQA, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                         dqa(2),
                         mat(2, 2, dqa(2), dqa(4), dqa(6), dqa(8))),
    (OP_MAT_SCALE, TYPE_DQA, mat(2, 2, dqa(1), dqa(1), dqa(1), dqa(1)),
                         dqa(0),
                         mat(2, 2, dqa(0), dqa(0), dqa(0), dqa(0))),
    (OP_MAT_SCALE, TYPE_DQA, mat(3, 2, dqa(5), dqa(5), dqa(5), dqa(5), dqa(5), dqa(5)),
                         dqa(3),
                         mat(3, 2, dqa(15), dqa(15), dqa(15), dqa(15), dqa(15), dqa(15))),
    (OP_MAT_SCALE, TYPE_DQA, mat(1, 4, dqa(10), dqa(20), dqa(30), dqa(40)),
                         dqa(2),
                         mat(1, 4, dqa(20), dqa(40), dqa(60), dqa(80))),
    (OP_MAT_SCALE, TYPE_DQA, mat(4, 1, dqa(3), dqa(3), dqa(3), dqa(3)),
                         dqa(3),
                         mat(4, 1, dqa(9), dqa(9), dqa(9), dqa(9))),
    (OP_MAT_ADD, TYPE_DECIMAL, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                            mat(2, 2, dqa(5), dqa(6), dqa(7), dqa(8)),
                            mat(2, 2, dqa(6), dqa(8), dqa(10), dqa(12))),
    (OP_MAT_SUB, TYPE_DECIMAL, mat(2, 2, dqa(5), dqa(6), dqa(7), dqa(8)),
                            mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                            mat(2, 2, dqa(4), dqa(4), dqa(4), dqa(4))),
    (OP_MAT_MUL, TYPE_DECIMAL, mat(2, 2, dqa(1), dqa(0), dqa(0), dqa(1)),
                            mat(2, 2, dqa(2), dqa(3), dqa(4), dqa(5)),
                            mat(2, 2, dqa(2), dqa(3), dqa(4), dqa(5))),
    (OP_MAT_MUL, TYPE_DECIMAL, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                            mat(2, 2, dqa(5), dqa(6), dqa(7), dqa(8)),
                            mat(2, 2, dqa(19), dqa(22), dqa(43), dqa(50))),
    (OP_MAT_VEC_MUL, TYPE_DECIMAL, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                               [dqa(1), dqa(1)],
                               [dqa(3), dqa(7)]),

    # Entries 40-49: Decimal continued and TRAP cases
    # CRIT-1: [[1,2,3],[4,5,6],[7,8,9]] × [1,1,1] = [6,15,24] (same arithmetic as DQA)
    (OP_MAT_VEC_MUL, TYPE_DECIMAL, mat(3, 3, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6), dqa(7), dqa(8), dqa(9)),
                               [dqa(1), dqa(1), dqa(1)],
                               [dqa(6), dqa(15), dqa(24)]),
    (OP_MAT_TRANSPOSE, TYPE_DECIMAL, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)), None,
                                mat(2, 2, dqa(1), dqa(3), dqa(2), dqa(4))),
    (OP_MAT_SCALE, TYPE_DECIMAL, mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                                dqa(2),
                                mat(2, 2, dqa(2), dqa(4), dqa(6), dqa(8))),
    (OP_MAT_ADD, TYPE_DECIMAL, mat(2, 2, dqa(10), dqa(20), dqa(30), dqa(40)),
                            mat(2, 2, dqa(1), dqa(2), dqa(3), dqa(4)),
                            mat(2, 2, dqa(11), dqa(22), dqa(33), dqa(44))),
    (OP_MAT_SUB, TYPE_DECIMAL, mat(2, 2, dqa(100), dqa(200), dqa(300), dqa(400)),
                            mat(2, 2, dqa(10), dqa(20), dqa(30), dqa(40)),
                            mat(2, 2, dqa(90), dqa(180), dqa(270), dqa(360))),
    (OP_MAT_MUL, TYPE_DECIMAL, mat(1, 3, dqa(1), dqa(2), dqa(3)),
                            mat(3, 1, dqa(1), dqa(2), dqa(3)),
                            mat(1, 1, dqa(14))),
    # MED-5: RFC Table Entry 46 - 3×2 × 2×3 = 3×3
    (OP_MAT_MUL, TYPE_DECIMAL, mat(3, 2, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6)),
                            mat(2, 3, dqa(1), dqa(2), dqa(3), dqa(4), dqa(5), dqa(6)),
                            mat(3, 3, dqa(9), dqa(12), dqa(15), dqa(19), dqa(26), dqa(33), dqa(29), dqa(40), dqa(51))),
    (OP_MAT_SCALE, TYPE_DECIMAL, mat(1, 4, dqa(10), dqa(20), dqa(30), dqa(40)),
                                dqa(3),
                                mat(1, 4, dqa(30), dqa(60), dqa(90), dqa(120))),

    # Entries 50-56: TRAP and boundary cases
    (OP_MAT_MUL, TYPE_DQA, mat(9, 9), mat(9, 9), mat(1, 1, TRAP)),  # DIMENSION_ERROR
    (OP_MAT_MUL, TYPE_DQA, mat(2, 3), mat(2, 3), mat(1, 1, TRAP)),   # DIMENSION_MISMATCH
    (OP_MAT_ADD, TYPE_DQA, mat(2, 2), mat(2, 3), mat(1, 1, TRAP)),   # DIMENSION_MISMATCH
    # MED-2/LOW-2: Uniform TRAP encoding - all result elements are TRAP sentinel
    # Expected output dimension = matrix rows (2), so [TRAP, TRAP]
    (OP_MAT_VEC_MUL, TYPE_DQA, mat(2, 3), [dqa(1), dqa(2)], [TRAP, TRAP]),
    # CRIT-2: C[0][0] = 2*(2^31)^2 = 2^63 = i64::MAX + 1 → OVERFLOW
    (OP_MAT_MUL, TYPE_DQA, mat(2, 2, dqa(2147483648), dqa(2147483648), dqa(2147483648), dqa(2147483648)),
                         mat(2, 2, dqa(2147483648), dqa(2147483648), dqa(2147483648), dqa(2147483648)),
                         mat(2, 2, TRAP, TRAP, TRAP, TRAP)),  # OVERFLOW
    # CRIT-2: element * scalar = 9223372038 * 10^9 > i64::MAX → OVERFLOW
    (OP_MAT_SCALE, TYPE_DQA, mat(2, 2, dqa(9223372038), dqa(9223372038), dqa(9223372038), dqa(9223372038)),
                                dqa(1000000000),
                                mat(2, 2, TRAP, TRAP, TRAP, TRAP)),  # OVERFLOW
    (OP_MAT_ADD, TYPE_DQA, mat(2, 2, dqa(1, 10), dqa(2), dqa(3), dqa(4)),
                            mat(2, 2, dqa(5), dqa(6), dqa(7), dqa(8)),
                            mat(2, 2, TRAP, TRAP, TRAP, TRAP)),  # SCALE_MISMATCH
    # MED-1: Uniform scale=10 → result_scale=10+10=20 > 18 → clean INVALID_SCALE
    (OP_MAT_MUL, TYPE_DQA, mat(2, 2, dqa(1, 10), dqa(2, 10), dqa(3, 10), dqa(4, 10)),
                         mat(2, 2, dqa(1, 10), dqa(2, 10), dqa(3, 10), dqa(4, 10)),
                         mat(2, 2, TRAP, TRAP, TRAP, TRAP)),  # INVALID_SCALE (10+10=20 > 18)
    # Entry 56: TRAP sentinel verification
    (OP_MAT_ADD, TYPE_DQA, mat(1, 1, TRAP),
                         mat(1, 1, dqa(0)),
                         mat(1, 1, TRAP)),  # TRAP propagated
    # MED-NEW-FINAL-1: Mixed-scale MAT_VEC_MUL tests
    # Entry 57: Successful mixed-scale (matrix scale=3, vector scale=7)
    # C[0][0] = 10*1 + 20*2 = 50, C[1][0] = 30*1 + 40*2 = 110, result_scale = 10
    (OP_MAT_VEC_MUL, TYPE_DQA,
     mat(2, 2, dqa(10,3), dqa(20,3), dqa(30,3), dqa(40,3)),   # matrix scale=3
     [dqa(1,7), dqa(2,7)],                                        # vector scale=7
     mat(2, 1, dqa(50,10), dqa(110,10))),                        # result scale=10
    # Entry 58: Vector internally non-uniform → SCALE_MISMATCH
    (OP_MAT_VEC_MUL, TYPE_DQA,
     mat(2, 2, dqa(1,0), dqa(1,0), dqa(1,0), dqa(1,0)),       # matrix uniform scale=0
     [dqa(1,0), dqa(2,5)],                                       # vector mixed scales (0, 5)
     mat(2, 1, TRAP, TRAP)),                                      # SCALE_MISMATCH

    # Entry 59: MAT_SCALE canonicalization test
    # 1000 * 10^-3 * 1 * 10^0 = 1000 * 10^-3 = 1.0 → canonicalizes to (1, 0)
    (OP_MAT_SCALE, TYPE_DQA,
     mat(1, 1, dqa(1000, 3)),                                     # 1000 * 10^-3 = 1.0
     dqa(1, 0),
     mat(1, 1, dqa(1, 0))),                                       # must canonicalize to 1 * 10^0

    # Entry 60: MAT_MUL at MAX_SCALE boundary (valid)
    # 2 * 10^-9 * 3 * 10^-9 = 6 * 10^-18 → result_scale = 18 (MAX_SCALE, valid)
    (OP_MAT_MUL, TYPE_DQA,
     mat(1, 1, dqa(2, 9)),
     mat(1, 1, dqa(3, 9)),
     mat(1, 1, dqa(6, 18))),                                      # result_scale = 18 = MAX_SCALE (valid)

    # Entry 61: TRAP propagation chain (MAT_ADD with TRAP at [0][0])
    # Tests that TRAP at one position doesn't affect processing of other elements
    # A has TRAP at [0][0], valid values elsewhere; B all valid
    # Result must be TRAP, all elements must still be processed per phase ordering
    (OP_MAT_ADD, TYPE_DQA,
     mat(2, 2, TRAP, dqa(1, 0), dqa(2, 0), dqa(3, 0)),        # A: TRAP at [0][0]
     mat(2, 2, dqa(4, 0), dqa(5, 0), dqa(6, 0), dqa(7, 0)),  # B: all valid
     mat(2, 2, TRAP, TRAP, TRAP, TRAP)),                        # Result: all TRAP

    # Entry 62: TRAP at last index [1][1] — forces full traversal past first element
    # Ensures traversal continues to last index, not just prefix scan
    (OP_MAT_ADD, TYPE_DQA,
     mat(2, 2, dqa(1, 0), dqa(2, 0), dqa(3, 0), TRAP),        # A: TRAP at [1][1] (last)
     mat(2, 2, dqa(4, 0), dqa(5, 0), dqa(6, 0), dqa(7, 0)),  # B: all valid
     mat(2, 2, TRAP, TRAP, TRAP, TRAP)),                        # Result: all TRAP

    # Entry 63: TRAP scalar + oversized matrix — TRAP_INPUT_ERROR wins over DIMENSION_ERROR
    # Scalar checked first in Phase 0; 9x9 dimension never evaluated by Phase 1
    (OP_MAT_SCALE, TYPE_DQA,
     mat(9, 9),         # oversized — would be DIMENSION_ERROR in Phase 1
     TRAP,              # scalar is TRAP sentinel
     mat(1, 1, TRAP)), # result: TRAP_INPUT_ERROR (not DIMENSION_ERROR)
]

def compute_probe_root() -> str:
    """Compute and return Merkle root as hex string."""
    leaves = [leaf_hash(*entry) for entry in PROBE_ENTRIES]
    root = merkle_root(leaves)
    return root.hex()

def main():
    root = compute_probe_root()
    print(f"DMAT Probe Merkle Root: {root}")
    print(f"Number of entries: {len(PROBE_ENTRIES)}")

if __name__ == '__main__':
    main()
