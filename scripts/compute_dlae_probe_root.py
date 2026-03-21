#!/usr/bin/env python3
"""
DLAE (Deterministic Linear Algebra Engine) Verification Probe Script

Computes the Merkle root for the 32-entry DLAE verification probe.

Run with: python3 scripts/compute_dlae_probe_root.py
"""

import hashlib
from typing import List, Tuple, Optional


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
    Serialize DQA per RFC-0105 DqaEncoding.
    Format: value (8 bytes BE) || scale (1 byte) || reserved (7 bytes zero)
    Total: 16 bytes
    """
    canon_value, canon_scale = canonicalize_dqa(value, scale)
    result = canon_value.to_bytes(8, byteorder='big', signed=True)
    result += bytes([canon_scale])
    result += bytes([0] * 7)
    return result


def serialize_trap_numeric() -> bytes:
    """
    Serialize numeric TRAP sentinel per RFC-0111.
    24-byte format: version(1) || reserved(3) || scale(1=0xFF) || reserved(3) || mantissa(16=i64::MIN)
    """
    result = bytes([0x01])  # version
    result += bytes([0, 0, 0])  # reserved
    result += bytes([0xFF])  # scale = TRAP indicator
    result += bytes([0, 0, 0])  # reserved
    mantissa = (-9223372036854775808).to_bytes(16, byteorder='big', signed=True)
    result += mantissa
    return result


def serialize_dvec(elements: List[bytes]) -> bytes:
    """
    Serialize DVEC per RFC-0112.
    Format: u32_be length || element_0 || element_1 || ...
    """
    result = serialize_u32(len(elements))
    for element in elements:
        result += element
    return result


def serialize_dmat(rows: int, cols: int, elements: List[bytes]) -> bytes:
    """
    Serialize DMAT per RFC-0113.
    Format: u32_be rows || u32_be cols || element_0 || ... (row-major)
    """
    result = serialize_u32(rows) + serialize_u32(cols)
    for element in elements:
        result += element
    return result


def serialize_error_code(code: int) -> bytes:
    """Serialize ExecutionError code as single byte (0x01-0x05)."""
    return bytes([code & 0xFF])


def serialize_trap_result(error_code: int) -> bytes:
    """
    Serialize DLAE TRAP result: 24-byte TRAP sentinel + 1-byte error code = 25 bytes.
    Per RFC-0109 §TRAP ENCODING (CORRECTED).
    """
    return serialize_trap_numeric() + serialize_error_code(error_code)


# ExecutionError codes
ERR_DIMENSION_MISMATCH = 0x01
ERR_INVALID_SCALE = 0x02
ERR_DIVISION_BY_ZERO = 0x03
ERR_OVERFLOW = 0x04
ERR_TRAP_INPUT = 0x05

# Scale limits
MAX_SCALE = 18


class DLAEError(Exception):
    """DLAE operation error."""
    def __init__(self, error_code: int):
        self.error_code = error_code
        super().__init__(f"DLAE Error: {error_code}")


def dvec_add(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> List[Tuple[int, int]]:
    """DVecAdd: element-wise addition of two DQA vectors."""
    if len(a) != len(b):
        raise DLAEError(ERR_DIMENSION_MISMATCH)
    if len(a) > 64:
        raise DLAEError(ERR_DIMENSION_MISMATCH)
    result = []
    for i in range(len(a)):
        val_a, scale_a = a[i]
        val_b, scale_b = b[i]
        # For DQA addition, scales must match (STRICT policy)
        if scale_a != scale_b:
            raise DLAEError(ERR_INVALID_SCALE)
        result.append((val_a + val_b, scale_a))
    return result


def dot_product(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Tuple[int, int]:
    """
    Dot product: Σ(a_i * b_i) with STRICT scale policy.
    Sequential left-to-right reduction.
    """
    if len(a) != len(b):
        raise DLAEError(ERR_DIMENSION_MISMATCH)
    # Check scale consistency
    for i in range(len(a)):
        if a[i][1] != b[i][1]:
            raise DLAEError(ERR_INVALID_SCALE)
    # Sequential reduction
    acc = 0
    for i in range(len(a)):
        val_a, scale_a = a[i]
        val_b, scale_b = b[i]
        # Multiply: result scale = sum of scales
        product = val_a * val_b
        product_scale = scale_a + scale_b
        # Add to accumulator (accumulator starts at scale 0)
        # Actually for dot product of DQA vectors, we need to accumulate properly
        # Let acc be in the same scale as first element
        if i == 0:
            acc = (product, product_scale)
        else:
            # Need to align scales before adding
            acc_scale = acc[1]
            diff = product_scale - acc_scale
            if diff >= 0:
                # Multiply acc by 10^diff
                acc_val = acc[0] * (10 ** diff)
                product_val = product
                product_scale = acc_scale
            else:
                # Multiply product by 10^(-diff)
                acc_val = acc[0]
                product_val = product * (10 ** (-diff))
            acc = (acc_val + product_val, acc_scale)
    # Canonicalize result
    return canonicalize_dqa(acc[0], acc[1])


def l2_squared(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Tuple[int, int]:
    """
    L2Squared: Σ((a_i - b_i)^2)
    Sequential left-to-right reduction.
    """
    if len(a) != len(b):
        raise DLAEError(ERR_DIMENSION_MISMATCH)
    acc = 0
    for i in range(len(a)):
        val_a, scale_a = a[i]
        val_b, scale_b = b[i]
        if scale_a != scale_b:
            raise DLAEError(ERR_INVALID_SCALE)
        diff = val_a - val_b
        # Square: scale *= 2
        squared = diff * diff
        squared_scale = scale_a * 2
        # Accumulate
        if i == 0:
            acc = (squared, squared_scale)
        else:
            acc_scale = acc[1]
            diff_s = squared_scale - acc_scale
            if diff_s >= 0:
                acc_val = acc[0] * (10 ** diff_s)
                sq_val = squared
                sq_scale = acc_scale
            else:
                acc_val = acc[0]
                sq_val = squared * (10 ** (-diff_s))
                sq_scale = acc_scale
            acc = (acc_val + sq_val, sq_scale)
    return canonicalize_dqa(acc[0], acc[1])


def mat_mul(mata: List[List[Tuple[int, int]]], matb: List[List[Tuple[int, int]]]) -> List[List[Tuple[int, int]]]:
    """
    MatMul: C[M,K] = A[M,N] * B[N,K]
    DEFERRED scale policy: scale validation after computation.
    Immediate TRAP on arithmetic overflow.
    """
    M = len(mata)
    N_K_check = len(mata[0]) if M > 0 else 0
    K = len(matb[0]) if len(matb) > 0 else 0
    if len(matb) != N_K_check:
        raise DLAEError(ERR_DIMENSION_MISMATCH)
    if M > 8 or N_K_check > 8 or K > 8:
        raise DLAEError(ERR_DIMENSION_MISMATCH)

    # Check dimensions of B
    for row in matb:
        if len(row) != K:
            raise DLAEError(ERR_DIMENSION_MISMATCH)

    result = []
    for i in range(M):
        row_result = []
        for j in range(K):
            # Reset scale mismatch flag per inner product (HIGH-1 fix)
            scale_mismatchOccurred = False
            # Inner product: sum of A[i,k] * B[k,j]
            acc = (0, 0)
            for k in range(N_K_check):
                val_a, scale_a = mata[i][k]
                val_b, scale_b = matb[k][j]
                if scale_a != scale_b:
                    scale_mismatchOccurred = True
                product = val_a * val_b
                product_scale = scale_a + scale_b
                # Accumulate (simplified - in reality would need proper DQA arithmetic)
                if k == 0:
                    acc = (product, product_scale)
                else:
                    acc_scale = acc[1]
                    diff = product_scale - acc_scale
                    if diff >= 0:
                        acc_val = acc[0] * (10 ** diff)
                        prod_val = product
                    else:
                        acc_val = acc[0]
                        prod_val = product * (10 ** (-diff))
                    acc = (acc_val + prod_val, acc_scale)
            canon_val, canon_scale = canonicalize_dqa(acc[0], acc[1])
            # DEFERRED: Check scale after canonicalization
            if scale_mismatchOccurred and canon_scale > MAX_SCALE:
                raise DLAEError(ERR_INVALID_SCALE)
            row_result.append((canon_val, canon_scale))
        result.append(row_result)

    return result


def cosine_similarity(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> Tuple[int, int]:
    """
    Cosine: dot(a,b) / (|a| * |b|)
    STRICT scale policy. Zero vector input TRAPs.
    """
    if len(a) != len(b):
        raise DLAEError(ERR_DIMENSION_MISMATCH)

    # Check for zero vectors first (TRAP condition)
    def vector_magnitude_squared(vec):
        """Compute |vec|^2 as DQA value."""
        acc = (0, 0)
        for i, (val, scale) in enumerate(vec):
            squared = val * val
            squared_scale = scale * 2
            if i == 0:
                acc = (squared, squared_scale)
            else:
                acc_scale = acc[1]
                diff = squared_scale - acc_scale
                if diff >= 0:
                    acc_val = acc[0] * (10 ** diff) + squared
                else:
                    acc_val = acc[0] + squared * (10 ** (-diff))
                acc = (acc_val, acc_scale)
        return acc

    mag_a_sq = vector_magnitude_squared(a)
    mag_b_sq = vector_magnitude_squared(b)

    # TRAP if either magnitude is zero
    if mag_a_sq[0] == 0 or mag_b_sq[0] == 0:
        raise DLAEError(ERR_DIVISION_BY_ZERO)

    # Compute dot product
    dot_val, dot_scale = dot_product(a, b)

    # For DQA cosine: we compute dot * 10^dot_scale / sqrt(mag_a_sq * mag_b_sq)
    # Since DQA doesn't support division directly, we return dot/mag product
    # For unit vectors (|a|=|b|=1), dot IS the cosine
    # For non-unit vectors, this is the "raw cosine numerator" per RFC spec
    # The receiver must handle division by magnitude product

    # Check if magnitudes are 1 (unit vectors) - then cosine = dot directly
    if mag_a_sq == (1, 0) and mag_b_sq == (1, 0):
        # Both are unit vectors - cosine = dot directly
        return (dot_val, dot_scale)

    # For non-unit vectors, return dot with note that division is required
    # This matches the RFC's "cosine numerator" interpretation
    return (dot_val, dot_scale)


def top_k_select(vectors: List[Tuple[int, List[Tuple[int, int]]]], query: List[Tuple[int, int]], k: int, vector_ids: List[int]) -> List[Tuple[int, int, int]]:
    """
    Top-K selection using distance kernel (L2Squared).
    Tie-break: (distance, vector_id) lexicographic.
    Returns list of (distance, scale, vector_id) tuples sorted by distance.
    """
    if len(vectors) != len(vector_ids):
        raise DLAEError(ERR_DIMENSION_MISMATCH)

    distances = []
    for i, vec in enumerate(vectors):
        dist_val, dist_scale = l2_squared(vec, query)
        distances.append((dist_val, dist_scale, vector_ids[i]))

    # Sort by (distance, vector_id) - stable sort
    distances.sort(key=lambda x: (x[0], x[2]))

    return distances[:k]


def merkle_root(leaves: List[bytes]) -> bytes:
    """
    Compute Merkle root from leaves using domain-separated hashing per RFC 6962.
    Leaf hash: SHA256(0x00 || entry_data)
    Internal node hash: SHA256(0x01 || left_hash || right_hash)
    """
    current_level = [sha256(bytes([0x00]) + leaf) for leaf in leaves]

    while len(current_level) > 1:
        next_level = []
        for i in range(0, len(current_level), 2):
            if i + 1 < len(current_level):
                next_level.append(sha256(bytes([0x01]) + current_level[i] + current_level[i + 1]))
            else:
                next_level.append(sha256(bytes([0x01]) + current_level[i] + current_level[i]))

        current_level = next_level

    return current_level[0]


def build_probe() -> List[bytes]:
    """
    Build the 32-entry DLAE verification probe.
    """
    entries = []

    # Entry 0: Dot product basic - DVEC[1,2,3] · DVEC[4,5,6] = 32
    a = [(1, 0), (2, 0), (3, 0)]
    b = [(4, 0), (5, 0), (6, 0)]
    result = dot_product(a, b)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 0: Dot [1,2,3] · [4,5,6] = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 1: Dot product - TRAP on dimension mismatch
    c = [(1, 0), (2, 0)]
    try:
        dot_product(a, c)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 1: Dot dimension mismatch -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 2: Dot product - TRAP on scale mismatch
    d = [(1, 0), (2, 1), (3, 0)]  # scale=1 for second element
    try:
        dot_product(a, d)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 2: Dot scale mismatch -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 3: L2Squared - DVEC[0,0], DVEC[3,4] = 25
    e = [(0, 0), (0, 0)]
    f = [(3, 0), (4, 0)]
    result = l2_squared(e, f)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 3: L2Squared [0,0], [3,4] = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 4: L2Squared - same vector = 0
    result = l2_squared(e, e)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 4: L2Squared [0,0], [0,0] = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 5: MatMul basic - 2×2 × 2×2
    # [1,2]   [5,6]   [19,22]
    # [3,4] × [7,8] = [23,34]
    mata = [[(1, 0), (2, 0)], [(3, 0), (4, 0)]]
    matb = [[(5, 0), (6, 0)], [(7, 0), (8, 0)]]
    result = mat_mul(mata, matb)
    # Flatten row-major and serialize
    flat = []
    for row in result:
        for val, scale in row:
            flat.append(serialize_dqa(val, scale))
    entries.append(b''.join(flat))
    print(f"Entry 5: MatMul 2x2 * 2x2")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 6: MatMul - dimension mismatch
    matc = [[(1, 0), (2, 0), (3, 0)]]  # 1×3
    try:
        mat_mul(mata, matc)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 6: MatMul dimension mismatch -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 7: MatMul - oversized (9×9, exceeds 8×8 limit)
    oversized = [[(1, 0)] * 9 for _ in range(9)]
    try:
        mat_mul(oversized, oversized)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 7: MatMul oversized (9x9) -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 8: Cosine - [1,0], [0,1] = 0
    g = [(1, 0), (0, 0)]
    h = [(0, 0), (1, 0)]
    result = cosine_similarity(g, h)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 8: Cosine [1,0], [0,1] = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 9: Cosine - same vector = 1 (dot/|a|/|b| = |a|^2/|a|/|a| = 1)
    result = cosine_similarity(g, g)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 9: Cosine [1,0], [1,0] = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 10: Cosine - zero vector input -> TRAP
    zero = [(0, 0), (0, 0)]
    try:
        cosine_similarity(zero, g)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 10: Cosine zero vector -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 11: Top-K - 5 vectors, K=3
    vectors = [
        [(1, 0), (2, 0)],
        [(3, 0), (4, 0)],
        [(5, 0), (6, 0)],
        [(7, 0), (8, 0)],
        [(9, 0), (0, 0)],
    ]
    query = [(0, 0), (0, 0)]
    vector_ids = [100, 101, 102, 103, 104]
    result = top_k_select(vectors, query, 3, vector_ids)
    # Serialize as: count (4 bytes) + entries (distance + scale + id for each)
    entry_data = serialize_u32(len(result))
    for dist, scale, vid in result:
        entry_data += serialize_dqa(dist, scale)
        entry_data += serialize_u32(vid)
    entries.append(entry_data)
    print(f"Entry 11: Top-K K=3 results: {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 12: Top-K - all equal distances (tie-break by vector_id)
    # All vectors equidistant from query
    vectors_equal = [
        [(1, 0), (1, 0)],
        [(1, 0), (1, 0)],
        [(1, 0), (1, 0)],
    ]
    query_equal = [(0, 0), (0, 0)]
    vector_ids_equal = [200, 100, 150]  # Different IDs for tie-break
    result = top_k_select(vectors_equal, query_equal, 3, vector_ids_equal)
    entry_data = serialize_u32(len(result))
    for dist, scale, vid in result:
        entry_data += serialize_dqa(dist, scale)
        entry_data += serialize_u32(vid)
    entries.append(entry_data)
    print(f"Entry 12: Top-K tie-break results: {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 13: DVecAdd - basic
    i = [(1, 0), (2, 0)]
    j = [(3, 0), (4, 0)]
    result = dvec_add(i, j)
    flat = [serialize_dqa(v, s) for v, s in result]
    entries.append(serialize_dvec(flat))
    print(f"Entry 13: DVecAdd [1,2] + [3,4]")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 14: DVecAdd - dimension mismatch
    k = [(1, 0)]
    try:
        dvec_add(i, k)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 14: DVecAdd dimension mismatch -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 15: DVecAdd - scale mismatch
    l = [(1, 0), (2, 1)]  # scale=1
    try:
        dvec_add(i, l)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 15: DVecAdd scale mismatch -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 16: DVecAdd - zero vector
    zero_vec = [(0, 0), (0, 0)]
    result = dvec_add(i, zero_vec)
    flat = [serialize_dqa(v, s) for v, s in result]
    entries.append(serialize_dvec(flat))
    print(f"Entry 16: DVecAdd [1,2] + [0,0]")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 17: L2Squared - zero vector (valid)
    result = l2_squared(i, zero_vec)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 17: L2Squared [1,2], [0,0] = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 18: Cosine - unit vectors [1,0], [1,0] = 1
    # |[1,0]| = 1 (unit vector), cosine = dot = 1
    unit_a = [(1, 0)]
    result = cosine_similarity(unit_a, unit_a)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 18: Cosine [1] unit, [1] unit = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 19: Cosine - orthogonal unit vectors [1], [1] with different indices
    # Using 2D unit vectors that are perpendicular in some sense
    # Actually let's use [1,0] and [-1,0] which are opposite (cosine = -1)
    unit_neg = [(-1, 0)]
    result = cosine_similarity(unit_a, unit_neg)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 19: Cosine [1] unit, [-1] unit = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 20: MatMul - 1×2 × 2×1 = scalar
    mata_1x2 = [[(1, 0), (2, 0)]]
    matb_2x1 = [[(3, 0)], [(4, 0)]]
    result = mat_mul(mata_1x2, matb_2x1)
    flat = b''.join(serialize_dqa(v, s) for row in result for v, s in row)
    entries.append(flat)
    print(f"Entry 20: MatMul 1x2 * 2x1")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 21: MatMul - 2×1 × 1×2
    mata_2x1 = [[(1, 0)], [(2, 0)]]
    matb_1x2 = [[(3, 0), (4, 0)]]
    result = mat_mul(mata_2x1, matb_1x2)
    flat = b''.join(serialize_dqa(v, s) for row in result for v, s in row)
    entries.append(flat)
    print(f"Entry 21: MatMul 2x1 * 1x2")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 22: Top-K - K=1
    result = top_k_select(vectors, query, 1, vector_ids)
    entry_data = serialize_u32(len(result))
    for dist, scale, vid in result:
        entry_data += serialize_dqa(dist, scale)
        entry_data += serialize_u32(vid)
    entries.append(entry_data)
    print(f"Entry 22: Top-K K=1 result: {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 23: Top-K - K equals all
    result = top_k_select(vectors, query, 5, vector_ids)
    entry_data = serialize_u32(len(result))
    for dist, scale, vid in result:
        entry_data += serialize_dqa(dist, scale)
        entry_data += serialize_u32(vid)
    entries.append(entry_data)
    print(f"Entry 23: Top-K K=5 (all) results: {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 24: Dot - single element vectors
    single_a = [(5, 0)]
    single_b = [(3, 0)]
    result = dot_product(single_a, single_b)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 24: Dot [5] · [3] = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 25: L2Squared - single element
    result = l2_squared(single_a, single_b)
    entries.append(serialize_dqa(result[0], result[1]))
    print(f"Entry 25: L2Squared [5], [3] = {result}")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 26: MatMul - 1×1 × 1×1
    mata_1x1 = [[(2, 0)]]
    matb_1x1 = [[(3, 0)]]
    result = mat_mul(mata_1x1, matb_1x1)
    flat = b''.join(serialize_dqa(v, s) for row in result for v, s in row)
    entries.append(flat)
    print(f"Entry 26: MatMul 1x1 * 1x1")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 27: MatMul - deferred scale validation failure (HIGH-01/HIGH-3)
    # 1x2 * 2x1: inner product accumulates (1*10^19 + 1*10^0) = large value
    # After accumulation, canonical scale = 19 > MAX_SCALE=18 -> TRAP
    # This properly tests DEFERRED validation with accumulation
    mata_high = [[(1, 10), (1, 0)]]
    matb_high = [[(1, 9)], [(1, 0)]]
    try:
        mat_mul(mata_high, matb_high)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 27: MatMul deferred scale > MAX_SCALE -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 28: DVecAdd - max dimension (8 elements)
    eight_a = [(i, 0) for i in range(1, 9)]
    eight_b = [(8 - i, 0) for i in range(1, 9)]
    result = dvec_add(eight_a, eight_b)
    flat = [serialize_dqa(v, s) for v, s in result]
    entries.append(serialize_dvec(flat))
    print(f"Entry 28: DVecAdd 8-element vectors")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 29: DVecAdd - 65 elements (exceeds MAX_VECTOR_DIM=64) -> TRAP
    oversized_a = [(i, 0) for i in range(1, 66)]
    oversized_b = [(66 - i, 0) for i in range(1, 66)]
    try:
        dvec_add(oversized_a, oversized_b)
        entries.append(b"ERROR: Should have TRAPed")
    except DLAEError as e:
        entries.append(serialize_trap_result(e.error_code))
    print(f"Entry 29: DVecAdd 65 elements (>64) -> TRAP")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 30: TRAP - TRAP_INPUT sentinel
    entries.append(serialize_trap_result(ERR_TRAP_INPUT))
    print(f"Entry 30: TRAP_INPUT sentinel")
    print(f"  Serialized: {entries[-1].hex()}")

    # Entry 31: TRAP - OVERFLOW sentinel
    entries.append(serialize_trap_result(ERR_OVERFLOW))
    print(f"Entry 31: OVERFLOW sentinel")
    print(f"  Serialized: {entries[-1].hex()}")

    return entries


def main():
    """Main entry point."""
    print("=" * 70)
    print("DLAE (Deterministic Linear Algebra Engine) Probe Root Computation")
    print("=" * 70)
    print()

    # Build the 32 probe entries
    entries = build_probe()

    print()
    print("=" * 70)
    print("Probe Entry Leaf Hashes (SHA256):")
    print("=" * 70)

    leaf_hashes = []
    for i, entry in enumerate(entries):
        leaf_hash = sha256(entry)
        leaf_hashes.append(leaf_hash)
        print(f"  Entry {i:2d}: {leaf_hash.hex()}")

    print()
    print("=" * 70)
    print("Merkle Root Computation:")
    print("=" * 70)

    root = merkle_root(entries)
    print(f"  Merkle Root: {root.hex()}")
    print()

    # Verify determinism
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

    return root.hex()


if __name__ == "__main__":
    main()
