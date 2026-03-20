#!/usr/bin/env python3
"""
compute_dact_probe_root.py — Deterministic Activation Functions probe verifier.

RFC-0114 v2.12 canonical reference implementation.

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

def canonicalize(value: int, scale: int) -> Tuple[int, int]:
    """
    Canonicalize a DQA value per RFC-0105 CANONICALIZE rules:
    1. If value == 0: return (0, 0)
    2. Strip trailing zeros: while value % 10 == 0 and scale > 0: value //= 10; scale -= 1
    3. Return canonical form.
    """
    if value == 0:
        return (0, 0)
    while value % 10 == 0 and scale > 0:
        value //= 10
        scale -= 1
    return (value, scale)

def normalize_to_scale(value: int, current_scale: int, target_scale: int) -> Tuple[int, int]:
    """
    Normalize a DQA value to a target scale per RFC-0114 §normalize_to_scale.

    If already at target scale, return as-is.
    If delta > 0: upscale (multiply mantissa by 10^delta)
    If delta < 0: downscale (floor divide mantissa by 10^|delta|)
    """
    if current_scale == target_scale:
        return (value, current_scale)
    delta = target_scale - current_scale
    if delta > 0:
        return (value * (10 ** delta), target_scale)
    else:
        return (value // (10 ** -delta), target_scale)

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

    Uses floor division (Python // semantics). Deterministic, no float.
    Note: For negative inputs, floor division rounds toward negative infinity,
    which differs from Rust's truncation toward zero. This matches the test
    vectors and expected Merkle root.
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

    All DQA entries are CANONICALIZED before serialization per RFC-0105
    VM Canonicalization Rule: values must be canonicalized before hashing.
    """
    leaves = []

    # Entries 0-3: ReLU / ReLU6 — canonicalize (e.g., 500,2 → 5,0)
    leaves.append(serialize_dqa(*canonicalize(500, 2)))    # relu(5.0)   = 5.00 → Dqa(5, 0)
    leaves.append(serialize_dqa(*canonicalize(0, 2)))       # relu(-5.0)  = 0.00 → Dqa(0, 0)
    leaves.append(serialize_dqa(*canonicalize(600, 2)))    # relu6(10.0) → clamp → 6.00 → Dqa(6, 0)
    leaves.append(serialize_dqa(*canonicalize(300, 2)))    # relu6(3.0)  = 3.00 → Dqa(3, 0)

    # Entries 4-6: Sigmoid outputs (Q8.8 → DQA converted) — canonicalize
    for idx in [400, 800, 0]:  # 0.0, 4.0, -4.0
        val, sc = q88_to_dqa(sigmoid_lut[idx], TARGET_SCALE)
        leaves.append(serialize_dqa(*canonicalize(val, sc)))

    # Entries 7-9: Tanh outputs (Q8.8 → DQA converted) — canonicalize
    for idx in [400, 600, 200]:  # 0.0, 2.0, -2.0
        val, sc = q88_to_dqa(tanh_lut[idx], TARGET_SCALE)
        leaves.append(serialize_dqa(*canonicalize(val, sc)))

    # Entries 10-11: LeakyReLU OUTPUTS (not inputs) — canonicalize
    lr_neg_val, lr_neg_sc = leaky_relu_output(-100, 2)  # -1.00 → -0.01
    lr_pos_val, lr_pos_sc = leaky_relu_output(100, 2)   # 1.00 → 1.00
    leaves.append(serialize_dqa(*canonicalize(lr_neg_val, lr_neg_sc)))
    leaves.append(serialize_dqa(*canonicalize(lr_pos_val, lr_pos_sc)))

    # Entries 12-13: First 4 entries of each LUT (raw Q8.8 bytes — not DQA)
    leaves.append(lut_bytes(sigmoid_lut[:4]))
    leaves.append(lut_bytes(tanh_lut[:4]))

    # Entry 14: Normalization invariant test value — canonicalize
    leaves.append(serialize_dqa(*canonicalize(12340, 3)))  # 12.340 → Dqa(1234, 2)

    # Entry 15: TRAP sentinel — already canonical
    leaves.append(serialize_trap())

    # =============================================================================
    # EXTENDED PROBE (entries 16-24 — non-normative verification aid)
    # These additional entries are documented for completeness and can be used
    # by implementations for extended self-verification. They are NOT part
    # of the committed 16-entry Merkle root.
    # =============================================================================

    # Entries 16-17: Sigmoid clamp boundary outputs
    # sigmoid(-4.5) → clamp → Dqa(0, 4)
    leaves.append(serialize_dqa(*canonicalize(0, 4)))
    # sigmoid(4.5) → clamp → Dqa(10000, 4)
    leaves.append(serialize_dqa(*canonicalize(10000, 4)))

    # Entries 18-19: Tanh clamp boundary outputs
    # tanh(-4.5) → clamp → Dqa(-10000, 4)
    leaves.append(serialize_dqa(*canonicalize(-10000, 4)))
    # tanh(4.5) → clamp → Dqa(10000, 4)
    leaves.append(serialize_dqa(*canonicalize(10000, 4)))

    # Entries 20-22: TRAP propagation tests (TRAP input → TRAP output)
    # ReLU(TRAP) → TRAP
    leaves.append(serialize_trap())
    # ReLU6(TRAP) → TRAP
    leaves.append(serialize_trap())
    # LeakyReLU(TRAP) → TRAP
    leaves.append(serialize_trap())

    # Entry 23: normalize_to_scale downscale — Dqa(25000, 4) normalized to scale=1 → Dqa(25, 1)
    # normalize_to_scale(Dqa(25000, 4), 1): delta = -3, 25000 // 10^3 = 25 → Dqa(25, 1)
    n23_val, n23_sc = normalize_to_scale(25000, 4, 1)
    leaves.append(serialize_dqa(*canonicalize(n23_val, n23_sc)))

    # Entry 24: normalize_to_scale downscale — Dqa(-153, 3) normalized to scale=2 → Dqa(-16, 2)
    # normalize_to_scale(Dqa(-153, 3), 2): delta = -1, -153 // 10 = -16 → Dqa(-16, 2)
    n24_val, n24_sc = normalize_to_scale(-153, 3, 2)
    leaves.append(serialize_dqa(*canonicalize(n24_val, n24_sc)))

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
    print("RFC-0114 v2.12 — DACT Probe Verification")
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

    print(f";; Probe Entries ({len(probe)} total — entries 0-15 are normatively committed)")
    print()
    for i, leaf in enumerate(probe):
        tag = " (committed)" if i < 16 else " (extended)"
        print(f"LEAF[{i:2d}] ({len(leaf):2d}B){tag}: {sha256(leaf).hex()}")

    # Merkle root (computed over all {len(probe)} entries for completeness)
    root = merkle_root(probe)
    print()
    print(f"FULL_PROBE_ROOT ({len(probe)} entries) = {root.hex()}")
    print(f"MERKLE_ROOT (16 entries) = {merkle_root(probe[:16]).hex()}")
    print()
    print("=" * 70)
    print(";; Verification: compare MERKLE_ROOT (16 entries) against RFC-0114 §Verification Probe")
    print(";; Expected: 4904af886aac5b581fefcf5d275c0753a0f804bc749d47bdd5bed74565c09fce")
    print(";; Extended entries (16-24) are non-normative verification aid.")
    print("=" * 70)

    # Sanity checks
    assert len(sigmoid_lut) == N, f"sigmoid LUT has {len(sigmoid_lut)} entries, expected {N}"
    assert len(tanh_lut) == N, f"tanh LUT has {len(tanh_lut)} entries, expected {N}"
    assert tanh_lut[200] == -247, f"tanh_lut[200]={tanh_lut[200]}, expected -247"
    assert tanh_lut[600] == 247, f"tanh_lut[600]={tanh_lut[600]}, expected 247"
    assert len(probe) == 25, f"probe has {len(probe)} entries, expected 25"

    # Verify committed 16-entry Merkle root
    committed_root = merkle_root(probe[:16])
    expected_root = "4904af886aac5b581fefcf5d275c0753a0f804bc749d47bdd5bed74565c09fce"
    assert committed_root.hex() == expected_root, f"Merkle root mismatch: got {committed_root.hex()}, expected {expected_root}"

    print()
    print(";; ALL CHECKS PASSED")

if __name__ == "__main__":
    main()
