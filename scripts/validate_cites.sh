#!/usr/bin/env bash
# scripts/validate_cites.sh
# Guard 2: Pre-commit § cite validation hook (per long-horizon plan v1.3 Phase 2 §Guard 2; R37 P3)
# Behavior: For each RFC-XXXX §section cite in changed files, verify §section exists in cited RFC.
# Coverage: Cite instances across all RFCs + research doc + mission YAMLs.
# Blocking on commit: YES.

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
EXIT_CODE=0
CHECKED=0
VALID=0
INVALID=0
PHANTOM=0
STALE=0

# Extract RFC-XXXX §section pattern (without version pin) and RFC-XXXX vN.N §section pattern
RFC_CITE_REGEX='RFC-[0-9]+(-[a-zA-Z0-9]+)?( v[0-9]+(\.[0-9]+)*)?( ?§[A-Za-z0-9._]+)?'

echo "§cite validation hook — Guard 2 (R37 P3)"
echo "RFC root: $RFC_ROOT"
echo ""

# Helper: extract all section headings from an RFC file
get_rfc_sections() {
    local rfc_file="$1"
    grep -oE '^##+ [^#].*$' "$rfc_file" 2>/dev/null | sed 's/^##* //' | tr -d '\r' || true
}

# Helper: find RFC file path from RFC number
find_rfc_path() {
    local rfc_num="$1"
    local rfc_id="RFC-${rfc_num}"

    # Detect sub-amendment suffix (-a1, -a2, -d1, -r1, etc.) per R16 cite-validator
    # audit + A2 v0.2.0 pre-commit blocker: bare RFC cite to parent must not lexically
    # pick a sub-amendment file. Sub-amendment cite with no own file (folded into parent,
    # e.g., RFC-0968-A1) falls back to parent.
    local prefix="${rfc_num%%-*}"
    local has_suffix=0
    if [ "$prefix" != "$rfc_num" ]; then
        has_suffix=1
    fi

    # Case-insensitive per R16 cite-validator audit: RFC-0967-A1 sub-amendment lives at
    # rfcs/.../0967-a1-policy-registry.md (lowercase a1) per Linux filename convention;
    # prior case-sensitive find produced false PHANTOM for every cite referencing
    # sub-amendments A1/A2/R1/etc.
    local matches
    matches=$(find "$RFC_ROOT" -type f -iname "${rfc_num}*.md" 2>/dev/null || true)

    # Sub-amendment cite with no own file (folded into parent): fall back to bare RFC.
    if [ -z "$matches" ] && [ "$has_suffix" -eq 1 ]; then
        matches=$(find "$RFC_ROOT" -type f -iname "${prefix}*.md" 2>/dev/null | grep -iv "/[0-9]\+-[a-z][0-9]*-" || true)
        has_suffix=0
    fi

    if [ -z "$matches" ]; then
        # Try with category prefix (legacy fallback for old RFC layout)
        for cat in numeric proof-systems process economics networking storage; do
            local cat_file
            cat_file=$(find "$RFC_ROOT/$cat" -type f -iname "${rfc_num}*.md" 2>/dev/null | head -1 || true)
            if [ -n "$cat_file" ]; then
                echo "$cat_file"
                return 0
            fi
        done
        return 1
    fi

    # For bare RFC cites: prefer accepted/ (more authoritative); exclude sub-amendment
    # files whose filename carries a -aN/-dN/-rN suffix. Per A2 v0.2.0 pre-commit
    # blocker (RFC-0968 cite lexical-first picked 0968-a2 before 0968 parent).
    if [ "$has_suffix" -eq 0 ]; then
        local accepted
        accepted=$(echo "$matches" | grep "/accepted/" | head -1 || true)
        if [ -n "$accepted" ]; then
            echo "$accepted"
            return 0
        fi
        # No accepted/ match: take first non-sub-amendment
        local non_subamend
        non_subamend=$(echo "$matches" | grep -ivE "/${prefix}-[a-z][0-9]*-" | head -1 || true)
        if [ -n "$non_subamend" ]; then
            echo "$non_subamend"
            return 0
        fi
    fi

    # Sub-amendment cite (or no non-sub-amendment fallback found): take first match.
    echo "$matches" | head -1
    return 0
}

# Helper: normalize §section text for matching
normalize_section() {
    local sec="$1"
    # Strip leading §, normalize spaces
    sec="${sec#§}"
    sec="${sec#"${sec%%[![:space:]]*}"}"
    sec="${sec%"${sec##*[![:space:]]}"}"
    echo "$sec"
}

# Scan changed files (or all files if no args)
files_to_check=("$@")
if [ ${#files_to_check[@]} -eq 0 ]; then
    while IFS= read -r -d '' f; do
        files_to_check+=("$f")
    done < <(find "$RFC_ROOT" docs -type f -name '*.md' -print0 2>/dev/null || true)
fi

for file in "${files_to_check[@]}"; do
    [ -f "$file" ] || continue

    while IFS=: read -r line_num line; do
        # Extract all RFC-XXXX §section cite instances
        cites=$(grep -oE 'RFC-[0-9]+(-[a-zA-Z0-9]+)?( v[0-9]+(\.[0-9]+)*)?( ?§[A-Za-z0-9._]+)?' <<< "$line" || true)

        while IFS= read -r cite; do
            [ -z "$cite" ] && continue

            # Skip self-cites (RFC-XXXX citing itself)
            CHECKED=$((CHECKED + 1))

            # Parse RFC number and section
            rfc_id=$(grep -oE 'RFC-[0-9]+(-[a-zA-Z0-9]+)?' <<< "$cite" || true)
            [ -z "$rfc_id" ] && continue

            rfc_num="${rfc_id#RFC-}"

            # Has version pin?
            version=$(grep -oE ' v[0-9]+(\.[0-9]+)*' <<< "$cite" | sed 's/^ v//' || true)

            # Has §section?
            section=$(grep -oE '§[A-Za-z0-9._]+' <<< "$cite" || true)

            # Find RFC file
            rfc_path=$(find_rfc_path "$rfc_num" 2>/dev/null || true)
            if [ -z "$rfc_path" ]; then
                # PHANTOM: RFC-XXXX does not exist on disk
                echo "PHANTOM [RFC missing]: $file:$line_num: $cite"
                PHANTOM=$((PHANTOM + 1))
                EXIT_CODE=1
                continue
            fi

            # If §section present, verify it exists in cited RFC
            if [ -n "$section" ]; then
                sec_normalized=$(normalize_section "$section")
                sections=$(get_rfc_sections "$rfc_path")

                # Match against canonical section heading
                if ! grep -qF "$sec_normalized" <<< "$sections" 2>/dev/null; then
                    # Try fuzzy match (section might be numbered differently)
                    sec_int=$(grep -oE '^[0-9]+' <<< "$sec_normalized" || true)
                    if [ -n "$sec_int" ]; then
                        # Check if any section starts with this integer
                        if ! grep -qE "^${sec_int}(\.| |$)" <<< "$sections" 2>/dev/null; then
                            echo "INVALID [section missing]: $file:$line_num: $cite (looking for '$sec_normalized' in $rfc_path)"
                            INVALID=$((INVALID + 1))
                            EXIT_CODE=1
                            continue
                        fi
                    else
                        echo "INVALID [section missing]: $file:$line_num: $cite (looking for '$sec_normalized' in $rfc_path)"
                        INVALID=$((INVALID + 1))
                        EXIT_CODE=1
                        continue
                    fi
                fi
            fi

            # If version pin present, verify against on-disk VH latest
            if [ -n "$version" ]; then
                # Find latest version in VH table
                vh_latest=$(grep -oE '\| [0-9]+(\.[0-9]+)+ \|' "$rfc_path" 2>/dev/null | head -1 | grep -oE '[0-9]+(\.[0-9]+)+' || true)
                if [ -n "$vh_latest" ] && [ "$version" != "$vh_latest" ]; then
                    # Check if version is in VH table (any row)
                    if ! grep -qE "^\| $version \|" "$rfc_path" 2>/dev/null; then
                        echo "STALE [version pin mismatch]: $file:$line_num: $cite (cited v$version, latest v$vh_latest in $rfc_path)"
                        STALE=$((STALE + 1))
                        EXIT_CODE=1
                        continue
                    fi
                fi
            fi

            VALID=$((VALID + 1))
        done <<< "$cites"
    done < <(awk '{ printf "%d:%s\n", NR, $0 }' "$file" 2>/dev/null)
done

echo ""
echo "Summary: CHECKED=$CHECKED VALID=$VALID PHANTOM=$PHANTOM INVALID=$INVALID STALE=$STALE"

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: cite validation found issues."
    echo "Fix: per BLUEPRINT.md §RFC Reference Conventions, all RFC-XXXX §section cites must resolve on disk."
    exit 1
fi

echo "PASS: all cites valid."
exit 0
