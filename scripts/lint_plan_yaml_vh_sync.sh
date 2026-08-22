#!/usr/bin/env bash
# scripts/lint_plan_yaml_vh_sync.sh
# Guard 7: Plan YAML header / VH row sync linter (per long-horizon plan v1.3 Phase 2 §Guard 7; P39 NEW-PATTERN)
# Behavior: Parse YAML frontmatter `version:` field + ## Version History latest row per plan file.
#           Detect OUT_OF_SYNC_YAML_OLDER / OUT_OF_SYNC_YAML_NEWER / NO_VH_NO_YAML.
#           Mirror M35 RFC Status/VH sync pattern adapted for plans (no ## Status header).
# Coverage: Plan files corpus-wide in /home/mmacedoeu/.claude/plans/.

set -euo pipefail

PLAN_ROOT="${PLAN_ROOT:-/home/mmacedoeu/.claude/plans}"
EXIT_CODE=0
CHECKED=0
IN_SYNC=0
OUT_OF_SYNC_OLDER=0
OUT_OF_SYNC_NEWER=0
NO_VH_NO_YAML=0
NO_VH=0
NO_YAML=0
KEEP_AS_IS=0

echo "Plan YAML/VH sync linter — Guard 7 (P39 NEW-PATTERN)"
echo "Plan root: $PLAN_ROOT"
echo ""

# Threshold: short plans (<50 lines) are exempt from VH requirement
LINE_THRESHOLD=50

while IFS= read -r -d '' plan_file; do
    CHECKED=$((CHECKED + 1))

    basename=$(basename "$plan_file")
    # Skip non-plan files
    case "$basename" in
        *.md) ;;
        *) continue ;;
    esac

    # Extract YAML frontmatter version
    yaml_version=""
    in_yaml=0
    yaml_end=0
    while IFS= read -r line; do
        if [[ "$line" == "---" ]]; then
            if [ "$in_yaml" -eq 0 ]; then
                in_yaml=1
            elif [ "$yaml_end" -eq 0 ]; then
                yaml_end=1
                break
            fi
        elif [ "$in_yaml" -eq 1 ] && [[ "$line" =~ ^version:[[:space:]]*(.*) ]]; then
            yaml_version="${BASH_REMATCH[1]}"
            yaml_version="${yaml_version#v}"
            yaml_version="${yaml_version// /}"
        fi
    done < "$plan_file"

    # Extract ## Version History latest row
    vh_latest=$(awk '/^## Version History/{found=1; next} found && /^\| [0-9]+(\.[0-9]+)+ \|/ {print; exit}' "$plan_file" 2>/dev/null | grep -oE '^\| [0-9]+(\.[0-9]+)+' | sed 's/^| //' | head -1 || true)

    # Short plan = no VH required (per P39 classification)
    line_count=$(wc -l < "$plan_file" 2>/dev/null || echo "0")
    if [ -z "$vh_latest" ] && [ "$line_count" -lt "$LINE_THRESHOLD" ]; then
        # Short plan with no VH = legitimate convention
        KEEP_AS_IS=$((KEEP_AS_IS + 1))
        continue
    fi

    # No VH on long plan = governance issue
    if [ -z "$vh_latest" ]; then
        if [ -z "$yaml_version" ]; then
            echo "NO_VH_NO_YAML: $plan_file (long plan, no ## Version History or YAML version)"
            NO_VH_NO_YAML=$((NO_VH_NO_YAML + 1))
            EXIT_CODE=1
            continue
        else
            # YAML version exists, no VH on long plan = also a gap
            echo "NO_VH: $plan_file (long plan, YAML v$yaml_version but no ## Version History)"
            NO_VH_NO_YAML=$((NO_VH_NO_YAML + 1))
            EXIT_CODE=1
            continue
        fi
    fi

    # No YAML version on long plan with VH = governance issue
    if [ -z "$yaml_version" ]; then
        echo "NO_YAML: $plan_file (long plan, VH v$vh_latest but no YAML version)"
        NO_VH_NO_YAML=$((NO_VH_NO_YAML + 1))
        EXIT_CODE=1
        continue
    fi

    # Compare YAML version vs VH latest
    if [ "$yaml_version" = "$vh_latest" ]; then
        IN_SYNC=$((IN_SYNC + 1))
    else
        # Determine direction
        yaml_major=$(grep -oE '^[0-9]+' <<< "$yaml_version" || echo "0")
        vh_major=$(grep -oE '^[0-9]+' <<< "$vh_latest" || echo "0")
        yaml_minor=$(grep -oE '\.[0-9]+' <<< "$yaml_version" | head -1 | sed 's/^\.//' || echo "0")
        vh_minor=$(grep -oE '\.[0-9]+' <<< "$vh_latest" | head -1 | sed 's/^\.//' || echo "0")

        if [ "$yaml_major" -lt "$vh_major" ] 2>/dev/null || \
           { [ "$yaml_major" -eq "$vh_major" ] && [ "$yaml_minor" -lt "$vh_minor" ] 2>/dev/null; }; then
            echo "OUT_OF_SYNC_YAML_OLDER: $plan_file (YAML=v$yaml_version, VH latest=v$vh_latest)"
            OUT_OF_SYNC_OLDER=$((OUT_OF_SYNC_OLDER + 1))
            EXIT_CODE=1
        else
            echo "OUT_OF_SYNC_YAML_NEWER: $plan_file (YAML=v$yaml_version, VH latest=v$vh_latest)"
            OUT_OF_SYNC_NEWER=$((OUT_OF_SYNC_NEWER + 1))
            EXIT_CODE=1
        fi
    fi
done < <(find "$PLAN_ROOT" -maxdepth 2 -type f -name '*.md' -print0 2>/dev/null || true)

echo ""
echo "Summary: CHECKED=$CHECKED"
echo "         IN_SYNC=$IN_SYNC"
echo "         OUT_OF_SYNC_YAML_OLDER=$OUT_OF_SYNC_OLDER"
echo "         OUT_OF_SYNC_YAML_NEWER=$OUT_OF_SYNC_YAML_NEWER"
echo "         NO_VH_NO_YAML=$NO_VH_NO_YAML"
echo "         KEEP_AS_IS (short plans, no VH required)=$KEEP_AS_IS"

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: plan YAML/VH sync drift detected."
    echo "Fix: align YAML frontmatter `version:` with latest ## Version History row."
    exit 1
fi

echo "PASS: plan YAML/VH sync clean."
exit 0
