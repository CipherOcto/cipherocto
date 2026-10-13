#!/usr/bin/env bash
# scripts/lint_section_cites.sh
# Guard 1: §section_name hygiene linter (per long-horizon plan v1.3 Phase 2 §Guard 1; R37 P3)

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0
ACCEPTED=0
REJECTED=0
NON_CANONICAL=0
SCANNED=0
EXAMINED=0

echo "§section_name hygiene linter — Guard 1 (R37 P3)"
echo "RFC root: $RFC_ROOT"
echo ""

while IFS= read -r -d '' rfc_file; do
    SCANNED=$((SCANNED + 1))
    matches=$(grep -nEo '§[A-Za-z0-9._/(),;:]+' "$rfc_file" 2>/dev/null || true)
    [ -z "$matches" ] && continue

    while IFS= read -r match; do
        [ -z "$match" ] && continue
        EXAMINED=$((EXAMINED + 1))
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
echo "Summary: SCANNED=$SCANNED files, EXAMINED=$EXAMINED cite tokens"
echo "         ACCEPTED=$ACCEPTED NON_CANONICAL=$NON_CANONICAL REJECTED=$REJECTED"

# A lint that examined no files has verified nothing, and must not report that
# its corpus is clean. Each of these scripts takes its corpus root from the
# environment, so pointing it at a path that does not exist — which is exactly
# what happens on a CI runner when the default names a directory outside the
# repository — used to print PASS and exit 0.
#
# This guard tested ACCEPTED, which was wrong in both directions:
#
#   - a corpus whose cites are all alphabetic (§Identity) is fully examined and
#     still reports "no files were examined", because non-canonical cites never
#     reach the ACCEPTED counter. Confirmed: two cites examined, ACCEPTED=0,
#     and the lint claimed it had looked at nothing.
#   - ACCEPTED is the *result* of the classification, not evidence of work.
#
# EXAMINED counts cite tokens the classifier actually looked at, which is the
# population this guard means.
if [ "$EXAMINED" -eq 0 ] && [ "$SCANNED" -eq 0 ]; then
    echo ""
    echo "FAIL: no files were examined (RFC_ROOT=$RFC_ROOT)."
    echo "This lint verified nothing. Point RFC_ROOT at the intended corpus"
    echo "before treating a pass as meaningful."
    exit 1
fi

if [ "$SCANNED" -eq 0 ]; then
    echo ""
    echo "FAIL: RFC_ROOT=$RFC_ROOT contains no files."
    exit 1
fi

if [ "$EXAMINED" -eq 0 ]; then
    echo ""
    echo "NOTE: $SCANNED file(s) scanned, none contained a §cite token."
    echo "This is a real result, not an empty run."
fi

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: $REJECTED cite hygiene violation(s) found."
    exit 1
fi

echo "PASS: 0 cite hygiene violations."
exit 0
