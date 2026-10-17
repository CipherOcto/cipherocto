#!/usr/bin/env python3
"""Compute RFC-0111 DECIMAL arithmetic config hash."""

import hashlib

POW10 = [10**i for i in range(37)]

config_bytes = bytearray()
for p in POW10:
    config_bytes += p.to_bytes(16, 'big')
config_bytes += b"RoundHalfEven"   # 13 bytes
config_bytes += b"RoundHalfEven"   # 13 bytes
config_bytes += bytes([36])         # MAX_DECIMAL_SCALE
config_bytes += b"TRAP"             # 4 bytes
config_bytes += bytes([40])         # SQRT_ITERATIONS
config_bytes += bytes([6])          # PRECISION_CAP

# Validate length before computing hash
assert len(config_bytes) == 625, f"Expected 625 bytes, got {len(config_bytes)}"

import sys

result = hashlib.sha256(bytes(config_bytes)).hexdigest()

# RFC-0111 pins this value. Naming it once removes the duplication where the
# printed expectation and the comparison were two separate literals that could
# drift apart unnoticed.
EXPECTED = "b071fa37d62a50318fde35fa5064464db49c2faaf03a5e2a58c209251f400a14"

print(f"DECIMAL_ARITHMETIC_CONFIG_HASH: {result}")
print()
print(f"Expected from RFC-0111: {EXPECTED}")

# This script used to print "Match: False" and exit 0. Anything that called it
# and checked the exit code — a shell loop, a CI step, an agent — saw success
# while it reported a mismatch. A verdict that is not in the exit status is a
# comment, not a check.
if result != EXPECTED:
    print("Match: False")
    print()
    print("MISMATCH: the computed config hash differs from the value RFC-0111")
    print("pins. Do not paste the computed value into a test vector.")
    sys.exit(1)

print("Match: True")
