#!/usr/bin/env python3
"""Compute RFC-0111 DECIMAL probe Merkle root."""

import struct
import hashlib

OPS = {
    'ADD': 1, 'SUB': 2, 'MUL': 3, 'DIV': 4, 'SQRT': 5, 'ROUND': 6,
    'CANONICALIZE': 7, 'CMP': 8, 'SERIALIZE': 9, 'DESERIALIZE': 10,
    'TO_DQA': 11, 'FROM_DQA': 12
}

MAX_DECIMAL = 10**36 - 1  # 999999999999999999999999999999999999999

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


def mk_entry(op: str, a_mantissa: int, a_scale: int, b_mantissa: int, b_scale: int) -> bytes:
    """Make probe entry: op_id (8) + input_a (24) + input_b (24) = 56 bytes."""
    op_bytes = OPS[op].to_bytes(8, 'little')  # op_id: 8 bytes little-endian
    a_bytes = encode_decimal(a_mantissa, a_scale)  # 24 bytes
    b_bytes = encode_decimal(b_mantissa, b_scale)  # 24 bytes
    return op_bytes + a_bytes + b_bytes  # 56 bytes total


# All 56 probe entries from RFC-0111
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

    # ROUND (entries 25-31)
    (25, 'ROUND', 1234, 3, 0, 1, "1.234 → scale=1 (round down)"),
    (26, 'ROUND', 1235, 3, 0, 1, "1.235 → scale=1 (RHE even)"),
    (27, 'ROUND', 1245, 3, 0, 1, "1.245 → scale=1 (RHE odd)"),
    (28, 'ROUND', 1255, 3, 0, 1, "1.255 → scale=1 (round up)"),
    (29, 'ROUND', -1235, 3, 0, 1, "-1.235 → scale=1"),
    (30, 'ROUND', -1245, 3, 0, 1, "-1.245 → scale=1"),
    (31, 'ROUND', -1255, 3, 0, 1, "-1.255 → scale=1"),

    # CANONICALIZE (entries 32-35)
    (32, 'CANONICALIZE', 1000, 3, 0, 0, "1000 (scale=3) → {1, 0}"),
    (33, 'CANONICALIZE', 0, 5, 0, 0, "0 (scale=5) → {0, 0}"),
    (34, 'CANONICALIZE', 100, 2, 0, 0, "100 (scale=2) → {1, 0}"),
    (35, 'CANONICALIZE', 0, 2, 0, 0, "0.0 (scale=2) → {0, 0}"),

    # CMP (entries 36-41)
    (36, 'CMP', 1, 0, 2, 0, "1.0 vs 2.0"),
    (37, 'CMP', 2, 0, 1, 0, "2.0 vs 1.0"),
    (38, 'CMP', 15, 1, 15, 1, "1.5 vs 1.5"),
    (39, 'CMP', -1, 0, 1, 0, "-1.0 vs 1.0"),
    (40, 'CMP', 1, 0, 100, 2, "1.0 vs 1.00"),
    (41, 'CMP', 1, 1, 10, 2, "0.1 vs 0.10"),

    # SERIALIZE/DESERIALIZE (entries 42-43)
    (42, 'SERIALIZE', 15, 1, 0, 0, "serialize(1.5)"),
    (43, 'DESERIALIZE', 15, 1, 0, 0, "deserialize(1.5)"),

    # TO_DQA (entries 44-45)
    (44, 'TO_DQA', 15, 1, 0, 0, "1.5 → DQA (scale≤18)"),
    (45, 'TO_DQA', 15, 20, 0, 0, "1.5 scale=20 → TRAP"),

    # FROM_DQA (entries 46-47)
    (46, 'FROM_DQA', 15, 1, 0, 0, "DQA(15,1) → 1.5"),
    (47, 'FROM_DQA', 0, 18, 0, 0, "DQA(0,18) → 0.0"),

    # Overflow/edge cases (entries 48-55)
    (48, 'ADD', MAX_DECIMAL, 0, 1, 0, "MAX + 1 → overflow"),
    (49, 'ADD', -MAX_DECIMAL, 0, 1, 0, "-MAX + 1 → underflow"),
    (50, 'MUL', 10**18, 0, 10**19, 0, "10^18 × 10^19 → overflow"),
    (51, 'DIV', 1, 0, 0, 0, "1.0 ÷ 0.0 → div by zero"),
    (52, 'SQRT', -1, 0, 0, 0, "√-1.0 → negative"),
    (53, 'ADD', 999999999999, 12, 1, 12, "0.999999999999 + 0.000000000001"),
    (54, 'MUL', 1, 12, 1000, 0, "0.000000000001 × 1000"),
    (55, 'DIV', 1, 36, 3, 0, "1.0 (scale=36) ÷ 3.0"),
]

print("Computing DECIMAL probe Merkle root...")
print("=" * 60)

hashes = []
for entry in DATA:
    idx, op, a_mant, a_scl, b_mant, b_scl, desc = entry
    try:
        raw = mk_entry(op, a_mant, a_scl, b_mant, b_scl)
        h = hashlib.sha256(raw).digest()
        hashes.append(h)
        print(f"{idx:2d} {op:14} OK: {desc}")
    except Exception as e:
        print(f"{idx:2d} {op:14} ERROR: {e}")
        hashes.append(b'\x00' * 32)

print(f"\n56 entries computed, building Merkle tree...")

# Build Merkle tree
while len(hashes) > 1:
    if len(hashes) % 2 == 1:
        hashes.append(hashes[-1])  # duplicate last for odd
    hashes = [hashlib.sha256(hashes[i] + hashes[i+1]).digest()
              for i in range(0, len(hashes), 2)]
    print(f"  Level complete: {len(hashes)} nodes")

print("\n" + "=" * 60)
print(f"DECIMAL PROBE MERKLE ROOT: {hashes[0].hex()}")
print("=" * 60)

# Also print each leaf hash for verification
print("\nLeaf hashes (first 8 bytes):")
for i, h in enumerate(hashes[:56]):
    print(f"  {i:2d}: {h[:8].hex()}")