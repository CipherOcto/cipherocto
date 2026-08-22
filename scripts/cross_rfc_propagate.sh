#!/usr/bin/env bash
# scripts/cross_rfc_propagate.sh
# Guard 5: Cross-RFC propagation automation script (per long-horizon plan v1.3 Phase 2 §Guard 5; R37 P3)
# Behavior: Given RFC-XXXX vN.N → vM.M promotion, scan all RFCs for `RFC-XXXX vN.N §section` cites
#           and propagate to vM.M if §section exists in promoted version. Emit diff for review.
# Coverage: Corpus-wide.
# Manual review required: YES (script = automation, not auto-apply).

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0

usage() {
    cat <<EOF
Usage: $0 <rfc_num> <old_version> <new_version>

Example:
    $0 0206 3.0 3.3

Scans all RFCs for RFC-0206 v3.0 §section cites and propagates to v3.3 §section.
Emits diff to stdout for manual review.

NO auto-apply: review diff, then apply manually with Edit tool.
EOF
    exit 1
}

if [ $# -ne 3 ]; then
    usage
fi

rfc_num="$1"
old_version="$2"
new_version="$3"

echo "Cross-RFC propagation — Guard 5 (R37 P3)"
echo "RFC: RFC-$rfc_num"
echo "Old version: v$old_version"
echo "New version: v$new_version"
echo ""

# Find the source RFC file
rfc_file=$(find "$RFC_ROOT" -type f -name "${rfc_num}*.md" 2>/dev/null | head -1 || true)
if [ -z "$rfc_file" ]; then
    echo "ERROR: RFC-$rfc_num not found in $RFC_ROOT"
    exit 1
fi

echo "Source RFC: $rfc_file"
echo ""

# Verify the new version exists in the VH table of source RFC
if ! grep -qE "^\| $new_version \|" "$rfc_file" 2>/dev/null; then
    echo "WARNING: v$new_version not found in VH table of $rfc_file"
    echo "         Continuing with propagation anyway (new version may not yet be in VH)"
fi

# Extract all sections from source RFC (numeric-anchor canonical pattern)
source_sections=$(grep -oE '^##+ [0-9]+(\.[0-9]+)*\.' "$rfc_file" 2>/dev/null | sed 's/^##* //' | sed 's/\.$//' || true)

echo "Source sections found:"
echo "$source_sections" | head -10
echo "..."

# Find all files citing RFC-rfc_num with old version
echo ""
echo "Scanning corpus for RFC-$rfc_num v$old_version §section cites..."
echo ""

cite_pattern="RFC-$rfc_num v$old_version §"

propagated=0
skipped=0
stale=0

while IFS= read -r -d '' file; do
    basename=$(basename "$file")
    case "$basename" in
        CHANGELOG.md|README.md|INDEX.md) continue ;;
    esac

    # Find lines with old version cite
    if ! grep -qE "$cite_pattern" "$file" 2>/dev/null; then
        continue
    fi

    # For each matching line, check if §section still exists in new version
    while IFS=: read -r line_num line; do
        if ! grep -qE "$cite_pattern" <<< "$line" 2>/dev/null; then
            continue
        fi

        # Extract section number
        section=$(grep -oE "RFC-$rfc_num v$old_version §[0-9]+(\.[0-9]+)*" <<< "$line" | head -1 | grep -oE '§[0-9]+(\.[0-9]+)*$' || true)
        if [ -z "$section" ]; then
            # Cite without §section — propagate version only
            section=""
        fi

        # Check if section still exists in source RFC
        if [ -n "$section" ]; then
            sec_num="${section#§}"
            # Look for matching section heading
            if ! grep -qE "^##+ $sec_num(\.|\s|$)" "$rfc_file" 2>/dev/null; then
                echo "STALE: $file:$line_num — §section '$section' not in new version"
                echo "    OLD: $line"
                echo "    ACTION: Manual review required (section removed/renamed)"
                stale=$((stale + 1))
                continue
            fi
        fi

        # Emit diff suggestion
        new_line="${line//RFC-$rfc_num v$old_version/RFC-$rfc_num v$new_version}"
        echo "PROPAGATE: $file:$line_num"
        echo "    OLD: $line"
        echo "    NEW: $new_line"
        propagated=$((propagated + 1))
    done < <(awk '{ printf "%d:%s\n", NR, $0 }' "$file" 2>/dev/null)
done < <(find "$RFC_ROOT" docs -type f -name '*.md' -print0 2>/dev/null || true)

echo ""
echo "Summary: PROPAGATE=$propagated STALE=$stale SKIPPED=$skipped"

if [ "$propagated" -gt 0 ]; then
    echo ""
    echo "MANUAL REVIEW REQUIRED: $propagated cite(s) can be propagated."
    echo "Apply with Edit tool: replace 'v$old_version' with 'v$new_version' on indicated lines."
    echo ""
    echo "WARNING: $stale cite(s) reference sections removed/renamed in new version."
    echo "         These need manual section mapping or removal."
fi

exit 0
