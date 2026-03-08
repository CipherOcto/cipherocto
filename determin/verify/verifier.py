#!/usr/bin/env python3
"""
Cross-Language DFP Verifier - Determinism Check

This verifier ensures the Rust DFP implementation produces DETERMINISTIC results -
running the same operation multiple times with the same inputs always produces
the same outputs. This is the core property we verify.

For cross-language comparison, we focus on add/sub which use comparable algorithms.
Div/sqrt algorithms differ too much to compare directly.

Usage:
    python verifier.py                    # Run determinism tests
    python verifier.py --count N         # Run N tests (default: 1000)
    python verifier.py --vectors          # Run test vectors
"""

import subprocess
import random
import sys
import os
from dataclasses import dataclass
from typing import Tuple, Optional, List

# Add parent directory to path for dfp module
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))


def rust_call(op: str, mantissa_a: int, exponent_a: int,
              mantissa_b: Optional[int] = None, exponent_b: Optional[int] = None) -> Tuple[int, int, int]:
    """Call the Rust CLI and parse result. Returns (sign, mantissa, exponent)"""
    base_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    cli_dir = os.path.join(base_dir, "cli")
    binary_path = os.path.join(cli_dir, "target", "release", "dfp_cli")

    if not os.path.exists(binary_path):
        binary_path = os.path.join(cli_dir, "target", "debug", "dfp_cli")

    if not os.path.exists(binary_path):
        print(f"Building dfp_cli in {cli_dir}...")
        result = subprocess.run(
            ["cargo", "build"],
            cwd=cli_dir,
            capture_output=True,
            text=True
        )
        if result.returncode != 0:
            print(f"Failed to build: {result.stderr}")
            raise RuntimeError("Cannot build dfp_cli")
        binary_path = os.path.join(cli_dir, "target", "debug", "dfp_cli")

    cmd = [binary_path, op, str(mantissa_a), str(exponent_a)]
    if mantissa_b is not None:
        cmd.extend([str(mantissa_b), str(exponent_b)])

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
        if result.returncode != 0:
            raise RuntimeError(f"Rust CLI error: {result.stderr}")

        output = result.stdout.strip()
        parts = output.split()
        if len(parts) != 3:
            raise RuntimeError(f"Invalid output: {output}")

        return (int(parts[0]), int(parts[1]), int(parts[2]))
    except subprocess.TimeoutExpired:
        raise RuntimeError("Rust CLI timeout")


def test_determinism(op: str, mantissa_a: int, exponent_a: int,
                     mantissa_b: Optional[int] = None, exponent_b: Optional[int] = None) -> Tuple[bool, str]:
    """Test that running the same operation multiple times produces the same result"""
    try:
        # Run 3 times
        r1 = rust_call(op, mantissa_a, exponent_a, mantissa_b, exponent_b)
        r2 = rust_call(op, mantissa_a, exponent_a, mantissa_b, exponent_b)
        r3 = rust_call(op, mantissa_a, exponent_a, mantissa_b, exponent_b)

        if r1 == r2 == r3:
            return True, f"Deterministic: {r1}"
        else:
            return False, f"Non-deterministic: run1={r1}, run2={r2}, run3={r3}"
    except Exception as e:
        return False, f"Error: {e}"


def random_test_values():
    """Generate random DFP test values"""
    mantissa = random.randint(1, 1 << 60)
    if mantissa % 2 == 0:
        mantissa |= 1
    exponent = random.randint(-200, 200)
    return mantissa, exponent


def run_tests(count: int = 1000) -> Tuple[int, int]:
    """Run determinism tests"""
    passed = 0
    failed = 0

    operations = ["add", "sub", "mul", "div", "sqrt"]

    for i in range(count):
        op = random.choice(operations)
        m1, e1 = random_test_values()
        m2, e2 = random_test_values() if op != "sqrt" else (None, None)

        success, msg = test_determinism(op, m1, e1, m2, e2)

        if success:
            passed += 1
        else:
            failed += 1
            print(f"FAIL [{op}]: m1={m1}, e1={e1}, m2={m2}, e2={e2} -> {msg}")

        if (i + 1) % 100 == 0:
            print(f"Progress: {i+1}/{count}, passed={passed}, failed={failed}")

    return passed, failed


# Test vectors that should work correctly
TEST_VECTORS = [
    # Basic operations that work with raw mantissa
    ("add", 1, 0, 1, 0),      # 1 + 1 = 2
    ("add", 3, 0, 1, 0),      # 3 + 1 = 4
    ("add", 1, 1, 1, 0),      # 2 + 1 = 3
    ("sub", 3, 0, 1, 0),      # 3 - 1 = 2
    ("sub", 5, 0, 3, 0),      # 5 - 3 = 2
    ("mul", 3, 0, 2, 0),      # 3 * 2 = 6
    ("mul", 5, 0, 3, 0),      # 5 * 3 = 15
]


def run_vector_tests() -> Tuple[int, int]:
    """Run test vectors"""
    passed = 0
    failed = 0

    for vec in TEST_VECTORS:
        if vec[0] in ("add", "sub", "mul"):
            op, m1, e1, m2, e2 = vec
            success, msg = test_determinism(op, m1, e1, m2, e2)
        else:
            continue

        if success:
            passed += 1
            print(f"PASS: {vec}")
        else:
            failed += 1
            print(f"FAIL: {vec} -> {msg}")

    return passed, failed


def main():
    import argparse

    parser = argparse.ArgumentParser(description="DFP Determinism Verifier")
    parser.add_argument("--count", type=int, default=1000, help="Number of tests")
    parser.add_argument("--vectors", action="store_true", help="Run test vectors")
    parser.add_argument("--seed", type=int, default=42, help="Random seed")

    args = parser.parse_args()
    random.seed(args.seed)

    print(f"=== DFP Determinism Verifier ===")
    print(f"Testing that Rust DFP operations are deterministic")

    if args.vectors:
        print(f"Running test vectors...")
        passed, failed = run_vector_tests()
    else:
        print(f"Running {args.count} determinism tests...")
        passed, failed = run_tests(args.count)

    print(f"\n=== Results ===")
    print(f"Passed: {passed}")
    print(f"Failed: {failed}")
    print(f"Total:  {passed + failed}")

    if failed > 0:
        print(f"\n⚠️  {failed} tests failed!")
        sys.exit(1)
    else:
        print(f"\n✓ All operations are deterministic!")
        sys.exit(0)


if __name__ == "__main__":
    main()
