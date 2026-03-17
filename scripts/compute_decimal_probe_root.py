#!/usr/bin/env python3
"""Compute RFC-0111 DECIMAL probe Merkle root (80-byte entries with results).

This script extends the original 56-byte format to 80-byte format:
  op_id (8) + input_a (24) + input_b (24) + result (24) = 80 bytes

For TRAP entries, uses sentinel encoding: {mantissa: 0x8000000000000000, scale: 0xFF}
"""

import struct
import hashlib
import math

OPS = {
    'ADD': 1, 'SUB': 2, 'MUL': 3, 'DIV': 4, 'SQRT': 5, 'ROUND': 6,
    'CANONICALIZE': 7, 'CMP': 8, 'SERIALIZE': 9, 'DESERIALIZE': 10,
    'TO_DQA': 11, 'FROM_DQA': 12
}

MAX_DECIMAL = 10**36 - 1  # 999999999999999999999999999999999999999
MAX_DECIMAL_MANTISSA = 10**36 - 1
MAX_DECIMAL_SCALE = 36

# Precomputed POW10 table
POW10 = [10**i for i in range(37)]


def encode_decimal(mantissa: int, scale: int) -> bytes:
    """Encode DECIMAL to 24-byte canonical format (big-endian).

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
        # Two's complement for negative: ~(-mantissa - 1) + 1 = 2^128 + mantissa
        buf[8:24] = ((1 << 128) + mantissa).to_bytes(16, 'big')

    return bytes(buf)


def encode_trap_sentinel() -> bytes:
    """Encode TRAP sentinel: {mantissa: 0x8000000000000000, scale: 0xFF}."""
    return encode_decimal(0x8000000000000000, 0xFF)


def canonicalize(mantissa: int, scale: int) -> tuple:
    """Canonicalize DECIMAL by removing trailing zeros."""
    if mantissa == 0:
        return (0, 0)
    while mantissa % 10 == 0 and scale > 0:
        mantissa //= 10
        scale -= 1
    return (mantissa, scale)


def decimal_add(a_mantissa, a_scale, b_mantissa, b_scale):
    """Compute ADD operation."""
    target_scale = max(a_scale, b_scale)
    diff_a = target_scale - a_scale
    diff_b = target_scale - b_scale

    if diff_a > 0:
        a_val = a_mantissa * POW10[diff_a]
    else:
        a_val = a_mantissa

    if diff_b > 0:
        b_val = b_mantissa * POW10[diff_b]
    else:
        b_val = b_mantissa

    result = a_val + b_val

    # Check overflow
    if abs(result) > MAX_DECIMAL_MANTISSA:
        return None  # TRAP

    return canonicalize(result, target_scale)


def decimal_sub(a_mantissa, a_scale, b_mantissa, b_scale):
    """Compute SUB operation."""
    # Same as ADD but subtract
    target_scale = max(a_scale, b_scale)
    diff_a = target_scale - a_scale
    diff_b = target_scale - b_scale

    if diff_a > 0:
        a_val = a_mantissa * POW10[diff_a]
    else:
        a_val = a_mantissa

    if diff_b > 0:
        b_val = b_mantissa * POW10[diff_b]
    else:
        b_val = b_mantissa

    result = a_val - b_val

    # Check overflow
    if abs(result) > MAX_DECIMAL_MANTISSA:
        return None  # TRAP

    return canonicalize(result, target_scale)


def decimal_mul(a_mantissa, a_scale, b_mantissa, b_scale):
    """Compute MUL operation."""
    raw_scale = a_scale + b_scale

    if raw_scale > MAX_DECIMAL_SCALE:
        # Need to round
        scale_reduction = raw_scale - MAX_DECIMAL_SCALE
        intermediate = a_mantissa * b_mantissa
        divisor = POW10[scale_reduction]
        product = intermediate // divisor
        remainder = abs(intermediate % divisor)

        # RoundHalfEven
        half = divisor // 2
        # BUG-1 Fix: sign-aware rounding for negative products
        if remainder > half:
            if product >= 0:
                product += 1
            else:
                product -= 1
        elif remainder == half and product % 2 != 0:
            if product >= 0:
                product += 1
            else:
                product -= 1

        if abs(product) > MAX_DECIMAL_MANTISSA:
            return None  # TRAP

        return canonicalize(product, MAX_DECIMAL_SCALE)
    else:
        result = a_mantissa * b_mantissa
        if abs(result) > MAX_DECIMAL_MANTISSA:
            return None  # TRAP
        return canonicalize(result, raw_scale)


def decimal_div(a_mantissa, a_scale, b_mantissa, b_scale, target_scale=6):
    """Compute DIV operation with RoundHalfEven."""
    if b_mantissa == 0:
        return None  # TRAP division by zero

    result_scale = min(MAX_DECIMAL_SCALE, max(a_scale, b_scale) + 6)
    scale_diff = result_scale + b_scale - a_scale

    abs_a = abs(a_mantissa)
    abs_b = abs(b_mantissa)
    result_sign = (a_mantissa < 0) != (b_mantissa < 0)

    if scale_diff > 0:
        # Scale up the dividend to get precision at result_scale
        scaled_dividend = abs_a * POW10[scale_diff]
    elif scale_diff < 0:
        scale_reduction = -scale_diff
        divisor = POW10[scale_reduction]
        # First scale down, then round
        quotient_pre = abs_a // divisor
        remainder_pre = abs_a % divisor
        half = divisor // 2
        if remainder_pre > half:
            scaled_dividend = quotient_pre + 1
        elif remainder_pre == half and quotient_pre % 2 != 0:
            scaled_dividend = quotient_pre + 1
        else:
            scaled_dividend = quotient_pre
    else:
        scaled_dividend = abs_a

    quotient = scaled_dividend // abs_b
    remainder = scaled_dividend % abs_b

    # RoundHalfEven
    half = abs_b // 2
    if remainder > half:
        quotient += 1
    elif remainder == half and quotient % 2 != 0:
        quotient += 1

    if result_sign:
        quotient = -quotient

    if abs(quotient) > MAX_DECIMAL_MANTISSA:
        return None  # TRAP

    return canonicalize(quotient, result_scale)


def isqrt(n):
    """Integer square root using Newton's method."""
    if n < 0:
        return None
    if n == 0:
        return 0
    # Initial guess
    x = 1 << ((n.bit_length() + 1) // 2)
    # Iterate
    for _ in range(40):
        x = (x + n // x) // 2
    return x


def decimal_sqrt(a_mantissa, a_scale):
    """Compute SQRT operation."""
    if a_mantissa < 0:
        return None  # TRAP

    if a_mantissa == 0:
        return (0, 0)

    P = min(MAX_DECIMAL_SCALE, a_scale + 6)
    scale_factor = 2 * P - a_scale

    if scale_factor < 0:
        return None  # TRAP

    # Compute scaled_n
    if scale_factor > 36:
        lo = POW10[scale_factor - 36]
        hi = POW10[36]
        scaled_n = a_mantissa * lo * hi
    else:
        scaled_n = a_mantissa * POW10[scale_factor]

    # Compute integer sqrt
    x = isqrt(scaled_n)

    # BUG-6 Fix: Newton-Raphson can overshoot by 1
    if x > 0 and x * x > scaled_n:
        x -= 1
    if x > MAX_DECIMAL_MANTISSA:
        return None  # TRAP

    return canonicalize(x, P)


def decimal_round(mantissa, scale, target_scale):
    """Compute ROUND operation with RoundHalfEven."""
    if target_scale >= scale:
        return canonicalize(mantissa, scale)

    diff = scale - target_scale
    divisor = POW10[diff]
    q = mantissa // divisor
    r = mantissa % divisor
    abs_r = abs(r)
    half = divisor // 2

    if abs_r < half:
        result = q
    elif abs_r > half:
        result = q + (1 if mantissa >= 0 else -1)
    else:
        # Tie - round to even
        if q % 2 == 0:
            result = q
        else:
            result = q + (1 if mantissa >= 0 else -1)

    return canonicalize(result, target_scale)


def decimal_canonicalize(mantissa, scale):
    """Compute CANONICALIZE operation."""
    return canonicalize(mantissa, scale)


def decimal_cmp(a_mantissa, a_scale, b_mantissa, b_scale):
    """Compute CMP operation - returns encoded comparison result."""
    target_scale = max(a_scale, b_scale)
    diff_a = target_scale - a_scale
    diff_b = target_scale - b_scale

    a_val = a_mantissa * POW10[diff_a]
    b_val = b_mantissa * POW10[diff_b]

    # For probe, encode as Decimal with cmp result in mantissa
    if a_val < b_val:
        cmp_result = -1
    elif a_val > b_val:
        cmp_result = 1
    else:
        cmp_result = 0

    # Encode as decimal for probe entry (result field)
    return (cmp_result, 0)


def compute_result(op, a_mantissa, a_scale, b_mantissa, b_scale):
    """Compute the result for a given operation."""
    try:
        if op == 'ADD':
            return decimal_add(a_mantissa, a_scale, b_mantissa, b_scale)
        elif op == 'SUB':
            return decimal_sub(a_mantissa, a_scale, b_mantissa, b_scale)
        elif op == 'MUL':
            return decimal_mul(a_mantissa, a_scale, b_mantissa, b_scale)
        elif op == 'DIV':
            return decimal_div(a_mantissa, a_scale, b_mantissa, b_scale)
        elif op == 'SQRT':
            return decimal_sqrt(a_mantissa, a_scale)
        elif op == 'ROUND':
            # b_mantissa holds target_scale for ROUND
            return decimal_round(a_mantissa, a_scale, b_mantissa)
        elif op == 'CANONICALIZE':
            return decimal_canonicalize(a_mantissa, a_scale)
        elif op == 'CMP':
            return decimal_cmp(a_mantissa, a_scale, b_mantissa, b_scale)
        elif op == 'SERIALIZE':
            # Returns bytes, for probe we encode the decimal
            return (a_mantissa, a_scale)
        elif op == 'DESERIALIZE':
            # Inverse of serialize
            return (a_mantissa, a_scale)
        elif op == 'TO_DQA':
            # For scale <= 18, can convert
            if a_scale <= 18:
                return (a_mantissa, a_scale)
            else:
                return None  # TRAP
        elif op == 'FROM_DQA':
            return canonicalize(a_mantissa, a_scale)
        else:
            return None
    except Exception as e:
        print(f"  Error computing {op}: {e}")
        return None


def mk_entry(op, a_mantissa, a_scale, b_mantissa, b_scale, result):
    """Make probe entry: op_id (8) + input_a (24) + input_b (24) + result (24) = 80 bytes."""
    op_bytes = OPS[op].to_bytes(8, 'little')  # op_id: 8 bytes little-endian
    a_bytes = encode_decimal(a_mantissa, a_scale)  # 24 bytes
    b_bytes = encode_decimal(b_mantissa, b_scale)  # 24 bytes

    if result is None:
        r_bytes = encode_trap_sentinel()
    else:
        r_mantissa, r_scale = result
        r_bytes = encode_decimal(r_mantissa, r_scale)  # 24 bytes

    return op_bytes + a_bytes + b_bytes + r_bytes  # 80 bytes total


# All 57 probe entries from RFC-0111 v1.19
# Format: (index, operation, a_mantissa, a_scale, b_mantissa, b_scale, description)
DATA = [
    # ADD (entries 0-3)
    (0, 'ADD', 1, 0, 2, 0, "1.0 + 2.0"),
    (1, 'ADD', 15, 1, 2, 0, "1.5 + 2.0 (scale alignment)"),
    (2, 'ADD', 100, 2, 1, 0, "1.00 + 1.0 (trailing zeros)"),
    (3, 'ADD', 1, 1, 2, 1, "0.1 + 0.2"),

    # SUB (entries 4-7)
    (4, 'SUB', 5, 0, 2, 0, "5.0 - 2.0"),
    (5, 'SUB', 15, 1, 15, 1, "1.5 - 1.5 (zero)"),
    (6, 'SUB', 1, 1, 2, 1, "0.1 - 0.2 (negative)"),
    (7, 'SUB', -15, 1, -5, 1, "-1.5 - (-0.5)"),

    # MUL (entries 8-13)
    (8, 'MUL', 2, 0, 3, 0, "2.0 × 3.0"),
    (9, 'MUL', 15, 1, 2, 0, "1.5 × 2.0"),
    (10, 'MUL', 1, 1, 2, 1, "0.1 × 0.2"),
    (11, 'MUL', MAX_DECIMAL, 0, 1, 0, "MAX × 1.0"),
    (12, 'MUL', -2, 0, 3, 0, "-2.0 × 3.0"),
    (13, 'MUL', -2, 0, -3, 0, "-2.0 × -3.0"),

    # DIV (entries 14-19)
    (14, 'DIV', 6, 0, 2, 0, "6.0 ÷ 2.0"),
    (15, 'DIV', 1000, 3, 3, 0, "1.000 ÷ 3.0"),
    (16, 'DIV', 1000, 2, 3, 0, "10.00 ÷ 3.0"),
    (17, 'DIV', 10, 1, 2, 0, "1.0 ÷ 2.0"),
    (18, 'DIV', -6, 0, 2, 0, "-6.0 ÷ 2.0"),
    (19, 'DIV', 6, 0, -2, 0, "6.0 ÷ -2.0"),

    # SQRT (entries 20-24)
    (20, 'SQRT', 4, 0, 0, 0, "√4.0"),
    (21, 'SQRT', 2, 0, 0, 0, "√2.0"),
    (22, 'SQRT', 4, 2, 0, 0, "√0.04"),
    (23, 'SQRT', 1, 4, 0, 0, "√0.0001"),
    (24, 'SQRT', 0, 0, 0, 0, "√0"),

    # SQRT (entry 25) - BUG-4 Fix: High scale SQRT now valid
    (25, 'SQRT', 1, 25, 0, 0, "√(10^-25) — high scale split multiplication"),

    # ROUND (entries 26-32) - b_mantissa holds target_scale
    (26, 'ROUND', 1234, 3, 1, 0, "1.234 → scale=1 (round down)"),
    (27, 'ROUND', 1235, 3, 1, 0, "1.235 → scale=1 (RHE even)"),
    (28, 'ROUND', 1245, 3, 1, 0, "1.245 → scale=1 (RHE odd)"),
    (29, 'ROUND', 1255, 3, 1, 0, "1.255 → scale=1 (round up)"),
    (30, 'ROUND', -1235, 3, 1, 0, "-1.235 → scale=1"),
    (31, 'ROUND', -1245, 3, 1, 0, "-1.245 → scale=1"),
    (32, 'ROUND', -1255, 3, 1, 0, "-1.255 → scale=1"),

    # CANONICALIZE (entries 33-36)
    (33, 'CANONICALIZE', 1000, 3, 0, 0, "1000 (scale=3) → {1, 0}"),
    (34, 'CANONICALIZE', 0, 5, 0, 0, "0 (scale=5) → {0, 0}"),
    (35, 'CANONICALIZE', 100, 2, 0, 0, "100 (scale=2) → {1, 0}"),
    (36, 'CANONICALIZE', 0, 2, 0, 0, "0.0 (scale=2) → {0, 0}"),

    # CMP (entries 37-42)
    (37, 'CMP', 1, 0, 2, 0, "1.0 vs 2.0"),
    (38, 'CMP', 2, 0, 1, 0, "2.0 vs 1.0"),
    (39, 'CMP', 15, 1, 15, 1, "1.5 vs 1.5"),
    (40, 'CMP', -1, 0, 1, 0, "-1.0 vs 1.0"),
    (41, 'CMP', 1, 0, 100, 2, "1.0 vs 1.00"),
    (42, 'CMP', 1, 1, 10, 2, "0.1 vs 0.10"),

    # SERIALIZE/DESERIALIZE (entries 43-44)
    (43, 'SERIALIZE', 15, 1, 0, 0, "serialize(1.5)"),
    (44, 'DESERIALIZE', 15, 1, 0, 0, "deserialize(1.5)"),

    # TO_DQA (entries 45-46)
    (45, 'TO_DQA', 15, 1, 0, 0, "1.5 → DQA (scale≤18)"),
    (46, 'TO_DQA', 15, 20, 0, 0, "1.5 scale=20 → TRAP"),

    # FROM_DQA (entries 47-48)
    (47, 'FROM_DQA', 15, 1, 0, 0, "DQA(15,1) → 1.5"),
    (48, 'FROM_DQA', 0, 18, 0, 0, "DQA(0,18) → 0.0"),

    # Overflow/edge cases (entries 49-56)
    (49, 'ADD', MAX_DECIMAL, 0, 1, 0, "MAX + 1 → overflow"),
    (50, 'ADD', -MAX_DECIMAL, 0, -1, 0, "-MAX + (-1) → underflow TRAP"),
    (51, 'MUL', 10**18, 0, 10**19, 0, "10^18 × 10^19 → overflow"),
    (52, 'DIV', 1, 0, 0, 0, "1.0 ÷ 0.0 → div by zero"),
    (53, 'SQRT', -1, 0, 0, 0, "√-1.0 → negative"),
    (54, 'ADD', 999999999999, 12, 1, 12, "0.999999999999 + 0.000000000001"),
    (55, 'MUL', 1, 12, 1000, 0, "0.000000000001 × 1000"),
    (56, 'DIV', 1, 36, 3, 0, "1.0 (scale=36) ÷ 3.0"),
]

print("Computing DECIMAL probe Merkle root (80-byte entries with results)...")
print("=" * 60)

hashes = []
for entry in DATA:
    idx, op, a_mant, a_scl, b_mant, b_scl, desc = entry
    try:
        # Compute the result
        result = compute_result(op, a_mant, a_scl, b_mant, b_scl)

        raw = mk_entry(op, a_mant, a_scl, b_mant, b_scl, result)
        h = hashlib.sha256(raw).digest()
        hashes.append(h)

        if result is None:
            print(f"{idx:2d} {op:14} TRAP: {desc}")
        else:
            r_mant, r_scl = result
            print(f"{idx:2d} {op:14} OK: {desc} → {r_mant}e{r_scl}")
    except Exception as e:
        print(f"{idx:2d} {op:14} ERROR: {e} - {desc}")
        hashes.append(b'\x00' * 32)

print(f"\n57 entries computed, building Merkle tree...")

# Build Merkle tree
while len(hashes) > 1:
    if len(hashes) % 2 == 1:
        hashes.append(hashes[-1])  # duplicate last for odd
    hashes = [hashlib.sha256(hashes[i] + hashes[i+1]).digest()
              for i in range(0, len(hashes), 2)]
    print(f"  Level complete: {len(hashes)} nodes")

print("\n" + "=" * 60)
print(f"DECIMAL PROBE MERKLE ROOT (80-byte format): {hashes[0].hex()}")
print("=" * 60)

# Also print each leaf hash for verification
print("\nLeaf hashes (first 8 bytes):")
for i, h in enumerate(hashes[:56]):
    print(f"  {i:2d}: {h[:8].hex()}")
