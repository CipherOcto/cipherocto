#!/usr/bin/env bash
# scripts/lint_status_vh_sync.sh
# Guard 3: Status header / VH row sync linter (per long-horizon plan v1.3 Phase 2 §Guard 3; M37 NEW P3)
# Behavior: Parse ## Status + ## Version History per RFC.
#           Detect OUT_OF_SYNC_STATUS_OLDER / OUT_OF_SYNC_STATUS_NEWER / NO_STATUS_VERSION / NO_VH.
#           Mirror M35 corpus audit logic.
# Coverage: 193 RFCs (96 accepted + 79 draft + 10 planned + 5 archived + 3 final).

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0
CHECKED=0
IN_SYNC=0
OUT_OF_SYNC_OLDER=0
OUT_OF_SYNC_NEWER=0
NO_STATUS_VERSION=0
NO_VH=0
DUAL_VH=0

echo "Status/VH sync linter — Guard 3 (M37 NEW P3)"
echo "RFC root: $RFC_ROOT"
echo ""

while IFS= read -r -d '' rfc_file; do
    CHECKED=$((CHECKED + 1))

    # Skip non-content files (CHANGELOG-only, README, etc.)
    basename=$(basename "$rfc_file")
    case "$basename" in
        CHANGELOG.md|README.md|INDEX.md) continue ;;
    esac

    # Find ## Status block
    status_line=$(grep -m1 -E '^## (Status|## Status|Status:)' "$rfc_file" 2>/dev/null || true)

    # Find ## Version History block (or equivalent)
    vh_line_num=$(grep -m1 -nE '^## (Version History|Changelog)$' "$rfc_file" 2>/dev/null | head -1 | cut -d: -f1 || true)

    # Extract Status version (e.g., "Draft v3.1", "Accepted (2026-08-20)", etc.)
    status_version=""
    if [ -n "$status_line" ]; then
        # Try multiple patterns
        status_version=$(grep -oE 'v[0-9]+(\.[0-9]+)*' <<< "$status_line" | head -1 | sed 's/^v//' || true)
    fi

    # If no ## Status block, try inline Status field in YAML frontmatter
    if [ -z "$status_version" ]; then
        yaml_status_version=$(awk '/^---$/{c++; next} c==1 && /^version:/ {print $2; exit}' "$rfc_file" 2>/dev/null || true)
        if [ -n "$yaml_status_version" ]; then
            status_version="${yaml_status_version#v}"
        fi
    fi

    # If still no Status version, this is NO_STATUS_VERSION
    if [ -z "$status_version" ]; then
        echo "NO_STATUS_VERSION: $rfc_file (no ## Status version found)"
        NO_STATUS_VERSION=$((NO_STATUS_VERSION + 1))
        # Continue to check VH even if no Status
    fi

    # Check for VH block
    if [ -z "$vh_line_num" ]; then
        # Special case: Accepted RFCs without VH = NO_VH
        if grep -qE '^## (Status|## Status|Status:).*Accepted' "$rfc_file" 2>/dev/null; then
            echo "NO_VH: $rfc_file (Accepted but no ## Version History)"
            NO_VH=$((NO_VH + 1))
            EXIT_CODE=1
        fi
        continue
    fi

    # Count VH tables (some RFCs have multiple - dual-VH pattern like RFC-0126)
    vh_count=$(grep -cE '^## (Version History|Changelog)$' "$rfc_file" 2>/dev/null || echo "0")
    if [ "$vh_count" -gt 1 ]; then
        DUAL_VH=$((DUAL_VH + 1))
    fi

    # Extract latest VH row version (first numeric row in VH table)
    vh_latest=$(awk '/^## (Version History|Changelog)$/{found=1; next} found && /^\| [0-9]+(\.[0-9]+)+ \|/ {print; exit}' "$rfc_file" 2>/dev/null | grep -oE '^\| [0-9]+(\.[0-9]+)+' | sed 's/^| //' | head -1 || true)

    if [ -z "$vh_latest" ]; then
        echo "NO_VH_ROWS: $rfc_file (## Version History exists but no data rows)"
        NO_VH=$((NO_VH + 1))
        continue
    fi

    # Compare Status version vs VH latest
    if [ -n "$status_version" ]; then
        if [ "$status_version" = "$vh_latest" ]; then
            IN_SYNC=$((IN_SYNC + 1))
        else
            # Determine direction: is Status older or newer than VH latest?
            status_major=$(grep -oE '^[0-9]+' <<< "$status_version" || echo "0")
            vh_major=$(grep -oE '^[0-9]+' <<< "$vh_latest" || echo "0")
            status_minor=$(grep -oE '\.[0-9]+' <<< "$status_version" | head -1 | sed 's/^\.//' || echo "0")
            vh_minor=$(grep -oE '\.[0-9]+' <<< "$vh_latest" | head -1 | sed 's/^\.//' || echo "0")

            if [ "$status_major" -lt "$vh_major" ] 2>/dev/null || \
               { [ "$status_major" -eq "$vh_major" ] && [ "$status_minor" -lt "$vh_minor" ] 2>/dev/null; }; then
                echo "OUT_OF_SYNC_STATUS_OLDER: $rfc_file (Status=v$status_version, VH latest=v$vh_latest)"
                OUT_OF_SYNC_OLDER=$((OUT_OF_SYNC_OLDER + 1))
                EXIT_CODE=1
            else
                echo "OUT_OF_SYNC_STATUS_NEWER: $rfc_file (Status=v$status_version, VH latest=v$vh_latest)"
                OUT_OF_SYNC_NEWER=$((OUT_OF_SYNC_NEWER + 1))
                EXIT_CODE=1
            fi
        fi
    fi
done < <(find "$RFC_ROOT" -type f -name '*.md' -print0 2>/dev/null || true)

echo ""
echo "Summary: CHECKED=$CHECKED IN_SYNC=$IN_SYNC"
echo "         OUT_OF_SYNC_STATUS_OLDER=$OUT_OF_SYNC_OLDER"
echo "         OUT_OF_SYNC_STATUS_NEWER=$OUT_OF_SYNC_NEWER"
echo "         NO_STATUS_VERSION=$NO_STATUS_VERSION"
echo "         NO_VH=$NO_VH"
echo "         DUAL_VH=$DUAL_VH (informational only, e.g., RFC-0126)"

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: Status/VH sync drift detected."
    echo "Fix: align ## Status version with latest ## Version History row."
    exit 1
fi

echo "PASS: Status/VH sync clean."
exit 0
