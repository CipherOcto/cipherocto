#!/usr/bin/env python3
"""
Production-Grade DFP Verifier

Runs 10,000+ determinism tests across all operations and edge cases.
This is the same approach used in blockchain VM arithmetic verification.

Usage:
    python verify_vectors.py              # Run full test suite
    python verify_vectors.py --count N    # Run N tests
"""

import subprocess
import random
import sys
import os
import json
from typing import Tuple, Optional


def get_cli_path() -> str:
    """Find or build the CLI binary"""
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    cli_dir = os.path.join(base_dir, "cli")

    binary_path = os.path.join(cli_dir, "target", "release", "dfp_cli")
    if os.path.exists(binary_path):
        return binary_path

    binary_path = os.path.join(cli_dir, "target", "debug", "dfp_cli")
    if os.path.exists(binary_path):
        return binary_path

    print(f"Building dfp_cli...")
    result = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=cli_dir,
        capture_output=True,
        text=True
    )
    if result.returncode != 0:
        raise RuntimeError(f"Build failed: {result.stderr}")

    return os.path.join(cli_dir, "target", "release", "dfp_cli")


def run_operation(op: str, m1: int, e1: int, m2: Optional[int] = None, e2: Optional[int] = None) -> Tuple[int, int, int]:
    """Run CLI and return (sign, mantissa, exponent)"""
    cli = get_cli_path()
    cmd = [cli, op, str(m1), str(e1)]
    if m2 is not None:
        cmd.extend([str(m2), str(e2)])

    result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
    if result.returncode != 0:
        raise RuntimeError(f"CLI error: {result.stderr}")

    parts = result.stdout.strip().split()
    return (int(parts[0]), int(parts[1]), int(parts[2]))


def test_determinism(op: str, m1: int, e1: int, m2: Optional[int] = None, e2: Optional[int] = None) -> Tuple[bool, str]:
    """Run operation 3 times and verify same result each time"""
    try:
        r1 = run_operation(op, m1, e1, m2, e2)
        r2 = run_operation(op, m1, e1, m2, e2)
        r3 = run_operation(op, m1, e1, m2, e2)

        if r1 == r2 == r3:
            return True, str(r1)
        else:
            return False, f"r1={r1}, r2={r2}, r3={r3}"
    except Exception as e:
        return False, f"Error: {e}"


# -----------------------------------------
# Test Categories
# -----------------------------------------

def edge_tests(verifier, count=500):
    """Edge values and special cases"""
    passed = 0
    failed = 0

    edge_mantissas = [0, 1, 2, 3, 5, 7, 9, 15, 31, 127, 255, 511, 1023, 2**112 - 1]
    edge_exponents = [-200, -100, -50, -10, -1, 0, 1, 10, 50, 100, 200]

    for _ in range(count):
        op = random.choice(["add", "sub", "mul", "div", "sqrt"])
        m1 = random.choice(edge_mantissas)
        e1 = random.choice(edge_exponents)

        if op == "sqrt":
            m1 = abs(m1) | 1  # Ensure positive odd
            success, msg = test_determinism(op, m1, e1)
        else:
            m2 = random.choice(edge_mantissas)
            e2 = random.choice(edge_exponents)
            success, msg = test_determinism(op, m1, e1, m2, e2)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"EDGE FAIL [{op}]: {msg}")

    return passed, failed


def rounding_tests(verifier, count=2000):
    """Guard/sticky/halfway rounding cases"""
    passed = 0
    failed = 0

    for _ in range(count):
        op = random.choice(["add", "sub"])

        # Powers of 2 near mantissa boundaries
        exp = random.randint(-100, 100)
        m1 = 1 << random.randint(0, 112)  # Power of 2
        e1 = exp

        # Small delta near rounding boundary
        delta_exp = e1 - 113
        m2 = random.randint(1, 10)
        e2 = delta_exp

        success, msg = test_determinism(op, m1, e1, m2, e2)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"ROUND FAIL [{op}]: {msg}")

    return passed, failed


def mul_carry_tests(count=1500):
    """Mantissa overflow in multiplication"""
    passed = 0
    failed = 0

    for _ in range(count):
        # Near-maximum mantissa
        m1 = (1 << 112) - 1
        e1 = random.randint(-50, 50)

        m2 = random.randint(1, 1 << 20)
        e2 = random.randint(-50, 50)

        success, msg = test_determinism("mul", m1, e1, m2, e2)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"MUL FAIL: {msg}")

    return passed, failed


def division_tests(count=2000):
    """Long division remainder cases"""
    passed = 0
    failed = 0

    divisors = [3, 7, 9, 11, 13, 17, 19, 23, 29, 31, 127, 255]

    for _ in range(count):
        m1 = random.randint(1, 1 << 112)
        e1 = random.randint(-50, 50)

        m2 = random.choice(divisors)
        e2 = random.randint(-20, 20)

        success, msg = test_determinism("div", m1, e1, m2, e2)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"DIV FAIL: {msg}")

    return passed, failed


def sqrt_tests(count=1000):
    """Square root precision stability"""
    passed = 0
    failed = 0

    for _ in range(count):
        # Various mantissa sizes
        m1 = random.randint(1, 1 << 112)
        e1 = random.randint(-50, 50)

        # Ensure odd for canonical form
        m1 = m1 | 1

        success, msg = test_determinism("sqrt", m1, e1)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"SQRT FAIL: {msg}")

    return passed, failed


def exponent_tests(count=1000):
    """Overflow/underflow boundaries"""
    passed = 0
    failed = 0

    for _ in range(count):
        m1 = random.randint(1, 1 << 60)
        m2 = random.randint(1, 1 << 60)

        # Extreme exponents
        e1 = random.choice([-1000, -500, -200, 200, 500, 1000])
        e2 = random.choice([-1000, -500, -200, 200, 500, 1000])

        op = random.choice(["add", "sub", "mul", "div"])

        success, msg = test_determinism(op, m1, e1, m2, e2)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"EXP FAIL [{op}]: {msg}")

    return passed, failed


def canonical_tests(count=500):
    """Odd mantissa invariant tests"""
    passed = 0
    failed = 0

    for _ in range(count):
        # Even mantissas (should be normalized to odd)
        m1 = random.randint(2, 1 << 60) * 2
        e1 = random.randint(-50, 50)

        m2 = random.randint(2, 1 << 60) * 2
        e2 = random.randint(-50, 50)

        op = random.choice(["add", "sub", "mul"])

        success, msg = test_determinism(op, m1, e1, m2, e2)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"CANON FAIL [{op}]: {msg}")

    return passed, failed


def fuzz_tests(count=2000):
    """General random fuzzing"""
    passed = 0
    failed = 0

    for _ in range(count):
        op = random.choice(["add", "sub", "mul", "div", "sqrt"])

        m1 = random.randint(1, 1 << 60)
        e1 = random.randint(-200, 200)

        # Ensure odd
        m1 = m1 | 1

        if op == "sqrt":
            success, msg = test_determinism(op, m1, e1)
        else:
            m2 = random.randint(1, 1 << 60)
            e2 = random.randint(-200, 200)
            m2 = m2 | 1
            success, msg = test_determinism(op, m1, e1, m2, e2)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"FUZZ FAIL [{op}]: {msg}")

    return passed, failed


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Production DFP Verifier")
    parser.add_argument("--count", type=int, default=None, help="Override total test count")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")

    args = parser.parse_args()
    random.seed(args.seed)

    print("=== Production-Grade DFP Verifier ===")
    print("Testing determinism across 10,500+ vectors\n")

    total_passed = 0
    total_failed = 0

    # Run all categories
    categories = [
        ("Edge Values", edge_tests, 500),
        ("Rounding Traps", rounding_tests, 2000),
        ("Mul Carry", mul_carry_tests, 1500),
        ("Division", division_tests, 2000),
        ("Sqrt", sqrt_tests, 1000),
        ("Exponent", exponent_tests, 1000),
        ("Canonical", canonical_tests, 500),
        ("Fuzz", fuzz_tests, 2000),
    ]

    for name, test_func, expected_count in categories:
        count = args.count if args.count else expected_count
        print(f"Running {name} ({count} tests)...")

        # Scale proportionally if count overridden
        if args.count:
            scale = args.count / 10500
            count = int(expected_count * scale)
            if count < 1:
                count = 1

        passed, failed = test_func(count)
        total_passed += passed
        total_failed += failed

        print(f"  {name}: {passed} passed, {failed} failed\n")

    print("=== Final Results ===")
    print(f"Passed: {total_passed}")
    print(f"Failed: {total_failed}")
    print(f"Total:  {total_passed + total_failed}")

    if total_failed > 0:
        print(f"\n⚠️  {total_failed} tests failed!")
        sys.exit(1)
    else:
        print("\n✓ All operations are deterministic!")
        sys.exit(0)


if __name__ == "__main__":
    main()
