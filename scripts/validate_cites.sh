#!/usr/bin/env bash
# scripts/validate_cites.sh
# Guard 2: Pre-commit § cite validation hook (per long-horizon plan v1.3 Phase 2 §Guard 2; R37 P3)
# Behavior: For each RFC-XXXX §section cite in changed files, verify §section exists in cited RFC.
# Coverage: Cite instances across all RFCs + research doc + mission YAMLs.
# Blocking on commit: YES.

set -euo pipefail

RFC_ROOT="${RFC_ROOT:-rfcs}"
# Per-command timeout in seconds. Override via CITE_TIMEOUT env.
# Applied to: find (full tree walks), sort (large lists), grep over big files.
# Cheap per-cite parsing uses bash builtins ([[ =~ ]]) — no fork, no timeout needed.
CITE_TIMEOUT="${CITE_TIMEOUT:-10}"
# Max directory depth for find walks (defense-in-depth against pathological trees).
CITE_MAXDEPTH="${CITE_MAXDEPTH:-8}"
# Max files fed into the per-file loop (defense against accidental `find /`).
CITE_MAXFILES="${CITE_MAXFILES:-5000}"
EXIT_CODE=0
CHECKED=0
VALID=0
INVALID=0
PHANTOM=0
STALE=0

# Extract RFC-XXXX §section pattern (without version pin) and RFC-XXXX vN.N §section pattern
RFC_CITE_REGEX='RFC-[0-9]+(-[a-zA-Z0-9]+)?( v[0-9]+(\.[0-9]+)*)?( ?§[A-Za-z0-9._]+)?'

# RFC path cache: rfc_num → resolved file path. Eliminates N² find_rfc_path
# blowup (per-cite full-tree walk) when validating large file batches.
declare -A RFC_PATH_CACHE
# Sections cache: rfc_path → newline-separated section headings. Avoids
# re-running grep+sed+tr over the same RFC file for every cite.
declare -A RFC_SECTIONS_CACHE

echo "§cite validation hook — Guard 2 (R37 P3)"
echo "RFC root: $RFC_ROOT"
echo ""

# Helper: extract all section headings from an RFC file (cached).
# Avoids re-running grep+sed+tr over the same RFC file for every cite.
get_rfc_sections() {
    local rfc_file="$1"
    if [ -n "${RFC_SECTIONS_CACHE[$rfc_file]+set}" ]; then
        echo "${RFC_SECTIONS_CACHE[$rfc_file]}"
        return 0
    fi
    local sections
    sections=$(timeout "$CITE_TIMEOUT" grep -oE '^##+ [^#].*$' "$rfc_file" 2>/dev/null \
        | sed 's/^##* //' | tr -d '\r' || true)
    RFC_SECTIONS_CACHE[$rfc_file]="$sections"
    echo "$sections"
}

# Helper: find RFC file path from RFC number (cached).
# Cache eliminates N² full-tree walks when validating large file batches
# (per-cite find explosion: 1264 cites × full tree walk).
find_rfc_path() {
    local rfc_num="$1"
    if [ -n "${RFC_PATH_CACHE[$rfc_num]+set}" ]; then
        echo "${RFC_PATH_CACHE[$rfc_num]}"
        return 0
    fi

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
    matches=$(timeout "$CITE_TIMEOUT" find "$RFC_ROOT" -maxdepth "$CITE_MAXDEPTH" -type f -iname "${rfc_num}*.md" 2>/dev/null || true)

    # Sub-amendment cite with no own file (folded into parent): fall back to bare RFC.
    # Sub-amendment pattern: -aN / -dN / -rN (NOT -vN which is versioned-amendment pattern).
    if [ -z "$matches" ] && [ "$has_suffix" -eq 1 ]; then
        matches=$(timeout "$CITE_TIMEOUT" find "$RFC_ROOT" -maxdepth "$CITE_MAXDEPTH" -type f -iname "${prefix}*.md" 2>/dev/null | grep -iv "/[0-9]\+-\(a\|d\|r\)[0-9]\+-" || true)
        has_suffix=0
    fi

    if [ -z "$matches" ]; then
        # Try with category prefix (legacy fallback for old RFC layout)
        for cat in numeric proof-systems process economics networking storage; do
            local cat_file
            cat_file=$(timeout "$CITE_TIMEOUT" find "$RFC_ROOT/$cat" -maxdepth "$CITE_MAXDEPTH" -type f -iname "${rfc_num}*.md" 2>/dev/null | head -1 || true)
            if [ -n "$cat_file" ]; then
                RFC_PATH_CACHE[$rfc_num]="$cat_file"
                echo "$cat_file"
                return 0
            fi
        done
        return 1
    fi

    # For bare RFC cites: prefer accepted/ (more authoritative); exclude sub-amendment
    # files whose filename carries a -aN/-dN/-rN suffix (NOT -vN which is versioned).
    # Per A2 v0.2.0 pre-commit blocker (RFC-0968 cite lexical-first picked 0968-a2
    # before 0968 parent). Per A2 v0.8.1 promotion: same exclusion applies to
    # accepted/ files. Per asset-generic-payment-caveat-review-DRY-2026-08-26:
    # prefer the LATEST versioned amendment over the original numeric parent when
    # multiple accepted/ files exist (e.g., RFC-0105 v3.5 supersedes v3.4 and the
    # numeric parent).
    local resolved=""
    if [ "$has_suffix" -eq 0 ]; then
        local accepted
        accepted=$(echo "$matches" | grep "/accepted/" | grep -ivE "/${prefix}-(a|d|r)[0-9]+-" || true)
        if [ -n "$accepted" ]; then
            # Pick latest by version (highest semver first)
            local latest
            latest=$(echo "$accepted" | sort -t- -k2 -V -r | head -1 || true)
            resolved="$latest"
        else
            # No accepted/ match: take latest non-sub-amendment
            local non_subamend
            non_subamend=$(echo "$matches" | grep -ivE "/${prefix}-(a|d|r)[0-9]+-" | sort -t- -k2 -V -r | head -1 || true)
            resolved="$non_subamend"
        fi
    fi

    # Sub-amendment cite (or no non-sub-amendment fallback found): take first match.
    if [ -z "$resolved" ]; then
        resolved=$(echo "$matches" | head -1 || true)
    fi

    if [ -n "$resolved" ]; then
        RFC_PATH_CACHE[$rfc_num]="$resolved"
        echo "$resolved"
        return 0
    fi
    return 1
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
        # Cap file count to prevent runaway loops on accidental `find /` invocation.
        if [ "${#files_to_check[@]}" -ge "$CITE_MAXFILES" ]; then
            echo "WARN: hit CITE_MAXFILES=$CITE_MAXFILES; truncating file list" >&2
            break
        fi
    done < <(timeout "$CITE_TIMEOUT" find "$RFC_ROOT" docs -maxdepth "$CITE_MAXDEPTH" -type f -name '*.md' -print0 2>/dev/null || true)
fi

for file in "${files_to_check[@]}"; do
    [ -f "$file" ] || continue

    # Single grep -noE over the whole file emits `line:cite` pairs. Avoids
    # per-line awk + per-line grep fork storm on large RFC files (4800+ lines).
    # `timeout` only on the outer grep (file-bound); cite parsing below uses
    # bash builtins (no forks per cite).
    while IFS=: read -r line_num cite; do
        # cite may itself contain ':' if filename carries it (it does not for our
        # regex output); if cite is empty (line with no cite) skip.
        [ -z "$cite" ] && continue

        CHECKED=$((CHECKED + 1))

        # Parse RFC number (strip "RFC-"). Bash regex avoids 3 grep forks/cite.
        if [[ "$cite" =~ RFC-([0-9]+(-[a-zA-Z0-9]+)?) ]]; then
            rfc_num="${BASH_REMATCH[1]}"
        else
            continue
        fi

        # Has version pin?
        version=""
        if [[ "$cite" =~ \ v([0-9]+(\.[0-9]+)*) ]]; then
            version="${BASH_REMATCH[1]}"
        fi

        # Has §section?
        section=""
        if [[ "$cite" =~ (§[A-Za-z0-9._]+) ]]; then
            section="${BASH_REMATCH[1]}"
        fi

        # Find RFC file (cached)
        rfc_path=$(find_rfc_path "$rfc_num" 2>/dev/null || true)
        if [ -z "$rfc_path" ]; then
            echo "PHANTOM [RFC missing]: $file:$line_num: $cite"
            PHANTOM=$((PHANTOM + 1))
            EXIT_CODE=1
            continue
        fi

        # If §section present, verify it exists in cited RFC
        if [ -n "$section" ]; then
            sec_normalized=$(normalize_section "$section")
            sections=$(get_rfc_sections "$rfc_path")

            # Bash substring search avoids grep fork/cite.
            if [[ "$sections" != *"$sec_normalized"* ]]; then
                sec_int=""
                if [[ "$sec_normalized" =~ ^([0-9]+) ]]; then
                    sec_int="${BASH_REMATCH[1]}"
                fi
                if [ -n "$sec_int" ]; then
                    if [[ "$sections" != *"${sec_int}."* && "$sections" != *"${sec_int} "* ]]; then
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
            # First version pin in any "| N.N |" cell = latest VH row.
            vh_latest=$(timeout "$CITE_TIMEOUT" grep -oE '\| [0-9]+(\.[0-9]+)+ \|' "$rfc_path" 2>/dev/null | head -1 | grep -oE '[0-9]+(\.[0-9]+)+' || true)
            if [ -n "$vh_latest" ] && [ "$version" != "$vh_latest" ]; then
                if ! grep -qE "^\| $version \|" "$rfc_path" 2>/dev/null; then
                    echo "STALE [version pin mismatch]: $file:$line_num: $cite (cited v$version, latest v$vh_latest in $rfc_path)"
                    STALE=$((STALE + 1))
                    EXIT_CODE=1
                    continue
                fi
            fi
        fi

        VALID=$((VALID + 1))
    done < <(timeout "$CITE_TIMEOUT" grep -noE 'RFC-[0-9]+(-[a-zA-Z0-9]+)?( v[0-9]+(\.[0-9]+)*)?( ?§[A-Za-z0-9._]+)?' "$file" 2>/dev/null || true)
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
