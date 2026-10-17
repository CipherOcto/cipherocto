#!/usr/bin/env bash
# Brace-balance CI lint (RFC-0969 §Security, AC-B3 fix).
#
# Invoked on every PR that touches the named function in
# `crates/octo-wallet/src/capability/gateway_authenticator.rs`. Counts
# `{` and `}` in the function body using `rustc`'s AST — we delegate to
# the in-source structural test (`authenticate_function_braces_balanced`)
# which is brace-aware (skips strings + comments).
#
# Usage:
#   bash .github/linters/braces-balanced.sh authenticate
#
# Exits non-zero if braces are unbalanced.

set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <function-name>" >&2
    exit 2
fi

NAME="$1"
FILE="crates/octo-wallet/src/capability/gateway_authenticator.rs"

if [[ ! -f "$FILE" ]]; then
    echo "missing $FILE" >&2
    exit 2
fi

if ! grep -q "pub fn ${NAME}(" "$FILE"; then
    echo "function ${NAME} not found in $FILE" >&2
    exit 2
fi

# Run the in-source structural test that is brace-aware (skips strings +
# line/block comments). The test is named
# `${name}_function_braces_balanced` per the convention in
# `gateway_authenticator.rs` §tests.
TEST_NAME="${NAME}_function_braces_balanced"

cd "$(dirname "$0")/../.."

if ! cargo test -p octo-wallet --lib "capability::gateway_authenticator::tests::${TEST_NAME}" \
    --quiet -- --nocapture 2>&1; then
    echo "FAIL: ${NAME}() braces unbalanced per in-source test ${TEST_NAME}" >&2
    exit 1
fi

echo "OK: ${NAME}() braces balanced"