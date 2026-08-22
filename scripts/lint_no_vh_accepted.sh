#!/usr/bin/env bash
# scripts/lint_no_vh_accepted.sh
# Guard 4: NO_VH_ACCEPTED linter (per long-horizon plan v1.3 Phase 2 §Guard 4; M37 NEW P3)
# Behavior: For each Accepted RFC, verify presence of ## Version History table.
#           Reject RFC promotion to Accepted without VH.
# Coverage: Accepted RFCs corpus-wide + promotion gates.
# BLOCKING promotion: YES.

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0
CHECKED=0
PASS=0
FAIL=0
MISSING_VH=0

echo "NO_VH_ACCEPTED linter — Guard 4 (M37 NEW P3)"
echo "RFC root: $RFC_ROOT"
echo ""

while IFS= read -r -d '' rfc_file; do
    CHECKED=$((CHECKED + 1))

    basename=$(basename "$rfc_file")

    # Skip non-content files
    case "$basename" in
        CHANGELOG.md|README.md|INDEX.md) continue ;;
    esac

    # Check if RFC is Accepted
    is_accepted=0
    if grep -qE '^## (Status|## Status|Status:).*Accepted' "$rfc_file" 2>/dev/null; then
        is_accepted=1
    fi
    # Also check YAML frontmatter for status: Accepted
    if awk '/^---$/{c++; next} c==1 && /^status:/ {tolower($2) ~ /accepted/; exit 0} END{exit 1}' "$rfc_file" 2>/dev/null; then
        is_accepted=1
    fi

    # Skip non-Accepted RFCs
    [ "$is_accepted" -eq 0 ] && continue

    # Check for ## Version History (or equivalent ## Changelog)
    if ! grep -qE '^## (Version History|Changelog)$' "$rfc_file" 2>/dev/null; then
        echo "MISSING_VH: $rfc_file (Accepted but no ## Version History / ## Changelog)"
        MISSING_VH=$((MISSING_VH + 1))
        FAIL=$((FAIL + 1))
        EXIT_CODE=1
        continue
    fi

    # Verify VH has at least one data row
    vh_has_rows=$(awk '/^## (Version History|Changelog)$/{found=1; next} found && /^\| [0-9]+/ {print; exit}' "$rfc_file" 2>/dev/null || true)
    if [ -z "$vh_has_rows" ]; then
        echo "MISSING_VH_ROWS: $rfc_file (## Version History exists but no data rows)"
        MISSING_VH=$((MISSING_VH + 1))
        FAIL=$((FAIL + 1))
        EXIT_CODE=1
        continue
    fi

    PASS=$((PASS + 1))
done < <(find "$RFC_ROOT" -type f -name '*.md' -print0 2>/dev/null || true)

echo ""
echo "Summary: CHECKED=$CHECKED (Accepted RFCs scanned)"
echo "         PASS=$PASS FAIL=$FAIL MISSING_VH=$MISSING_VH"

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: $MISSING_VH Accepted RFC(s) missing ## Version History."
    echo "Fix: per BLUEPRINT.md §RFC Process, append ## Version History table before promotion to Accepted."
    exit 1
fi

echo "PASS: all Accepted RFCs have ## Version History."
exit 0
