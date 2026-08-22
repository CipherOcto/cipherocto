#!/usr/bin/env bash
# scripts/lint_section_cites.sh
# Guard 1: §section_name hygiene linter (per long-horizon plan v1.3 Phase 2 §Guard 1; R37 P3)

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0
ACCEPTED=0
REJECTED=0
NON_CANONICAL=0

echo "§section_name hygiene linter — Guard 1 (R37 P3)"
echo "RFC root: $RFC_ROOT"
echo ""

while IFS= read -r -d '' rfc_file; do
    matches=$(grep -nEo '§[A-Za-z0-9._/(),;:]+' "$rfc_file" 2>/dev/null || true)
    [ -z "$matches" ] && continue

    while IFS= read -r match; do
        [ -z "$match" ] && continue
        line_num="${match%%:*}"
        cite="${match#*:}"

        # Strip prefix from line_num
        line_num="${line_num##*:}"

        # Reject 1: trailing-punct (period/slash/comma/semicolon/colon/paren/bracket)
        if grep -qE '[.,/();:\]]$' <<< "$cite" 2>/dev/null; then
            echo "REJECT [trailing-punct]: $rfc_file:$line_num: $cite"
            REJECTED=$((REJECTED + 1))
            EXIT_CODE=1
            continue
        fi

        # Reject 2: over-deep numeric anchors (>5 levels)
        if [[ "$cite" =~ ^§[0-9]+(\.[0-9]+){6,} ]]; then
            echo "REJECT [over-deep]: $rfc_file:$line_num: $cite"
            REJECTED=$((REJECTED + 1))
            EXIT_CODE=1
            continue
        fi

        # Reject 3: numeric-then-alpha phantom-token (e.g., §1a, §1.2x)
        if [[ "$cite" =~ ^§[0-9]+[a-zA-Z] ]]; then
            echo "REJECT [phantom-token]: $rfc_file:$line_num: $cite"
            REJECTED=$((REJECTED + 1))
            EXIT_CODE=1
            continue
        fi

        # Non-canonical: alphabetic §section_name
        if [[ "$cite" =~ ^§[^0-9] ]]; then
            NON_CANONICAL=$((NON_CANONICAL + 1))
            continue
        fi

        # Accept: pure numeric-anchor
        ACCEPTED=$((ACCEPTED + 1))
    done <<< "$matches"
done < <(find "$RFC_ROOT" -type f -name '*.md' -print0 2>/dev/null || true)

echo ""
echo "Summary: ACCEPTED=$ACCEPTED NON_CANONICAL=$NON_CANONICAL REJECTED=$REJECTED"

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: $REJECTED cite hygiene violation(s) found."
    exit 1
fi

echo "PASS: 0 cite hygiene violations."
exit 0
