#!/usr/bin/env python3
"""
compute_dact_probe_root.py — Deterministic Activation Functions probe verifier.

RFC-0114 v2.2 canonical reference implementation.

Produces:
  - SIGMOID_LUT_V2_SHA256
  - TANH_LUT_V2_SHA256
  - 16 probe leaf hashes
  - Merkle root

Usage:
  python3 scripts/compute_dact_probe_root.py

Verification:
  Compare output Merkle root against RFC-0114 §Verification Probe MERKLE_ROOT.
"""

import hashlib
import math
import struct
from typing import List, Tuple

# =============================================================================
# CONSTANTS
# =============================================================================

STEP = 0.01
START = -4.00
N = 801  # entries 0..800

TARGET_SCALE = 4  # Q8.8 → DQA conversion scale

TRAP_VALUE = -(1 << 63)
TRAP_SCALE = 0xFF

# =============================================================================
# SERIALIZATION
# =============================================================================

def serialize_dqa(value: int, scale: int) -> bytes:
    """Serialize DQA to canonical 16-byte RFC-0105 DqaEncoding format."""
    return value.to_bytes(8, "big", signed=True) + bytes([scale]) + bytes(7)

def serialize_trap() -> bytes:
    """Serialize canonical TRAP sentinel (RFC-0105)."""
    return serialize_dqa(TRAP_VALUE, TRAP_SCALE)

def serialize_i16(v: int) -> bytes:
    """Serialize Q8.8 i16 big-endian (2 bytes)."""
    return v.to_bytes(2, "big", signed=True)

def sha256(data: bytes) -> bytes:
    return hashlib.sha256(data).digest()

# =============================================================================
# LUT GENERATION
# =============================================================================

def q88_to_dqa(q: int, target_scale: int = TARGET_SCALE) -> Tuple[int, int]:
    """
    Convert Q8.8 value to DQA (value, scale).

    Uses floor division toward zero. Deterministic, no float.
    """
    return (q * (10 ** target_scale)) // 256, target_scale

def generate_lut(fn) -> List[int]:
    """
    Generate Q8.8 LUT for a function.

    Off-chain only (float permitted here).
    """
    table = []
    x = START
    for _ in range(N):
        y = fn(x)
        q = int(round(y * 256))
        table.append(q)
        x += STEP
    return table

def lut_bytes(table: List[int]) -> bytes:
    """Serialize LUT as raw concatenation of i16 big-endian Q8.8 values."""
    return b"".join(serialize_i16(q) for q in table)

# =============================================================================
# MERKLE
# =============================================================================

def merkle_root(leaves: List[bytes]) -> bytes:
    """
    Compute Merkle root over a list of leaf data.

    Each leaf is hashed individually, then pairs are hashed until root.
    If odd number of leaves, last leaf is duplicated (RFC-0113 convention).
    """
    level = [sha256(l) for l in leaves]
    while len(level) > 1:
        if len(level) % 2 == 1:
            level.append(level[-1])
        level = [sha256(level[i] + level[i + 1]) for i in range(0, len(level), 2)]
    return level[0]

# =============================================================================
# ACTIVATION FUNCTIONS (probe only — outputs serialized as DQA)
# =============================================================================

def mul_dqa(a_val: int, a_scale: int, b_val: int, b_scale: int) -> Tuple[int, int]:
    """DQA multiplication per RFC-0105."""
    return a_val * b_val, a_scale + b_scale

def leaky_relu_output(x_val: int, x_scale: int) -> Tuple[int, int]:
    """Compute leaky_relu(x) output as DQA (value, scale). alpha = Dqa(1, 2) = 0.01."""
    if x_val < 0:
        return mul_dqa(x_val, x_scale, 1, 2)
    return x_val, x_scale

def build_probe(sigmoid_lut: List[int], tanh_lut: List[int]) -> List[bytes]:
    """
    Build 16 probe leaves per RFC-0114 §Verification Probe.

    All entries are serialized as specified in the RFC.
    """
    leaves = []

    # Entries 0-3: ReLU / ReLU6
    leaves.append(serialize_dqa(500, 2))    # relu(5.0)   = 5.00
    leaves.append(serialize_dqa(0, 2))       # relu(-5.0)  = 0.00
    leaves.append(serialize_dqa(600, 2))    # relu6(10.0) → clamp → 6.00
    leaves.append(serialize_dqa(300, 2))    # relu6(3.0)  = 3.00

    # Entries 4-6: Sigmoid outputs (Q8.8 → DQA converted)
    for idx in [400, 800, 0]:  # 0.0, 4.0, -4.0
        val, sc = q88_to_dqa(sigmoid_lut[idx], TARGET_SCALE)
        leaves.append(serialize_dqa(val, sc))

    # Entries 7-9: Tanh outputs (Q8.8 → DQA converted)
    for idx in [400, 600, 200]:  # 0.0, 2.0, -2.0
        val, sc = q88_to_dqa(tanh_lut[idx], TARGET_SCALE)
        leaves.append(serialize_dqa(val, sc))

    # Entries 10-11: LeakyReLU OUTPUTS (not inputs)
    lr_neg_val, lr_neg_sc = leaky_relu_output(-100, 2)  # -1.00 → -0.01
    lr_pos_val, lr_pos_sc = leaky_relu_output(100, 2)   # 1.00 → 1.00
    leaves.append(serialize_dqa(lr_neg_val, lr_neg_sc))
    leaves.append(serialize_dqa(lr_pos_val, lr_pos_sc))

    # Entries 12-13: First 4 entries of each LUT (raw Q8.8 bytes)
    leaves.append(lut_bytes(sigmoid_lut[:4]))
    leaves.append(lut_bytes(tanh_lut[:4]))

    # Entry 14: Normalization invariant test value
    leaves.append(serialize_dqa(12340, 3))  # 12.340

    # Entry 15: TRAP sentinel
    leaves.append(serialize_trap())

    return leaves

# =============================================================================
# MAIN
# =============================================================================

def main():
    # Generate LUTs (off-chain float generation is deterministic via round())
    sigmoid_lut = generate_lut(lambda x: 1 / (1 + math.exp(-x)))
    tanh_lut = generate_lut(math.tanh)

    # Serialize LUTs
    sig_lut_b = lut_bytes(sigmoid_lut)
    tanh_lut_b = lut_bytes(tanh_lut)

    # Compute LUT SHA-256 commitments
    sig_hash = sha256(sig_lut_b).hex()
    tanh_hash = sha256(tanh_lut_b).hex()

    print("=" * 70)
    print("RFC-0114 v2.2 — DACT Probe Verification")
    print("=" * 70)
    print()
    print(f";; LUT Configuration")
    print(f";;   Domain: x ∈ [-4.00, 4.00], step = 0.01, entries = {N}")
    print(f";;   Q8.8 → DQA target scale = {TARGET_SCALE}")
    print(f";;   LUT byte lengths: sigmoid={len(sig_lut_b)}, tanh={len(tanh_lut_b)}")
    print()
    print(f"SIGMOID_LUT_V2_SHA256 = {sig_hash}")
    print(f"TANH_LUT_V2_SHA256    = {tanh_hash}")
    print()

    # Critical tanh validation
    print(f";; Critical tanh validation (idx 200 = x=-2.0, idx 600 = x=2.0)")
    print(f"tanh_lut[200] = {tanh_lut[200]}  (expected ~-247)")
    print(f"tanh_lut[400] = {tanh_lut[400]}  (expected 0)")
    print(f"tanh_lut[600] = {tanh_lut[600]}  (expected ~247)")
    print()

    # Build and serialize probe
    probe = build_probe(sigmoid_lut, tanh_lut)

    print(f";; Probe Entries ({len(probe)} total)")
    print()
    for i, leaf in enumerate(probe):
        print(f"LEAF[{i:2d}] ({len(leaf):2d}B): {sha256(leaf).hex()}")

    # Merkle root
    root = merkle_root(probe)
    print()
    print(f"MERKLE_ROOT = {root.hex()}")
    print()
    print("=" * 70)
    print(";; Verification: compare MERKLE_ROOT against RFC-0114 §Verification Probe")
    print(";; Expected: 7a4b3b434b104f33ff823b988d28723fd24730b25f6784fd03d090c0be991eed")
    print("=" * 70)

    # Sanity checks
    assert len(sigmoid_lut) == N, f"sigmoid LUT has {len(sigmoid_lut)} entries, expected {N}"
    assert len(tanh_lut) == N, f"tanh LUT has {len(tanh_lut)} entries, expected {N}"
    assert tanh_lut[200] == -247, f"tanh_lut[200]={tanh_lut[200]}, expected -247"
    assert tanh_lut[600] == 247, f"tanh_lut[600]={tanh_lut[600]}, expected 247"
    assert len(probe) == 16, f"probe has {len(probe)} entries, expected 16"

    expected_root = "7a4b3b434b104f33ff823b988d28723fd24730b25f6784fd03d090c0be991eed"
    assert root.hex() == expected_root, f"Merkle root mismatch: got {root.hex()}, expected {expected_root}"

    print()
    print(";; ALL CHECKS PASSED")

if __name__ == "__main__":
    main()
