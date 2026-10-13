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
NO_VH_VERSION=0

# The Version History heading is written in four shapes across this corpus:
# bare, numbered (`## 6. Version History`), and each of those with a leading
# section sign. `lint_no_vh_accepted.sh` was taught all four; this script was
# not, and the gap was worse here than in the row patterns.
#
# `vh_line_num` matched only the bare form, so a numbered or signed section
# left it empty and the per-file loop hit its `continue` before reaching any
# comparison. RFC-0206 §8 Version History was never examined by this guard at
# all — it reported nothing for the file because it did not read it, which is
# the same defect as a lint that reports success on an empty corpus, and this
# script already carries a guard for that at the end of the file.
VH_HEADING='^##+ ([0-9]+(\.[0-9]+)*\.? )?(Version History|Changelog|§Version History|§Changelog)'

# Normalise a version to three numeric components so that `2.14` and `2.14.0`
# compare equal. Missing components are zero, which is what they mean in every
# version scheme this corpus uses.
pad3() {
    local v="${1#v}" i comp
    local IFS=.
    local -a parts=()
    read -ra parts <<< "$v"
    for ((i = 0; i < 3; i++)); do
        # A trailing `-rN`, `-rc1`, `-draft` is a review-round or draft marker
        # on the same version, not a different one: this corpus writes
        # `v3.7-r3a` in a history table against a `v3.7` Status. Comparing the
        # marker reported six Accepted RFCs as out of sync with themselves.
        #
        # Read through `${parts[$i]:-}` rather than `${parts[$i]%%...}`. Under
        # `set -u` the suffix-strip form aborts on an unset element, and this
        # script runs `set -euo pipefail`. It aborted on exactly the two-
        # component versions the function exists to handle — `2.14` — so
        # RFC-0110 stayed out of sync after the fix that was supposed to clear
        # it, and the abort printed to stderr where the report did not show it.
        comp="${parts[$i]:-}"
        comp="${comp%%[-+]*}"
        parts[$i]="${comp:-0}"
    done
    printf '%s' "${parts[0]}.${parts[1]}.${parts[2]}"
}
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
    vh_line_num=$(grep -m1 -nE "$VH_HEADING" "$rfc_file" 2>/dev/null | head -1 | cut -d: -f1 || true)

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
    vh_count=$(grep -cE "$VH_HEADING" "$rfc_file" 2>/dev/null || echo "0")
    if [ "$vh_count" -gt 1 ]; then
        DUAL_VH=$((DUAL_VH + 1))
    fi

    # Extract latest VH row version.
    #
    # This used `print; exit`, which stops at the FIRST numeric row in the
    # table. A Version History table is append-ordered oldest-first, so the
    # first row is the *oldest* entry, not the latest. Every RFC whose table
    # had more than one row therefore compared its Status version against its
    # initial version and was reported as out of sync: 8 of the 9 findings on
    # the current tree were this bug, on files that are correctly in sync
    # (e.g. Status v1.1.0 against a table whose first row is 1.0.0 and whose
    # newest row is 1.1.0).
    #
    # Take every versioned row and sort -V, newest last. Version sorting is
    # required, not lexicographic: 1.10.0 > 1.9.0 numerically but < it as a
    # string.
        # "Does this section have data rows" and "what version is the latest" are
    # two different questions, and this conflated them. A Version History is
    # not always a table: this corpus also writes dated bullets, which is the
    # rendering `lint_no_vh_accepted.sh` accepts in four forms. Because only
    # table rows were read here, those documents were reported as having no
    # rows at all — 27 of 141 reports on the current tree, verified by
    # checking each file for a bullet row the sibling guard recognises.
    #
    # So: recognise a row in any of the four renderings, and only then ask
    # whether it carries a version. A history that has dated rows but no
    # version column is not a missing table; it has nothing to compare a
    # Status version against, and saying so is more useful than claiming the
    # section is empty.
    vh_has_rows=$(awk -v h="$VH_HEADING" '$0 ~ h {found=1; next}
        found && /^\| v?[0-9]+(\.[0-9]+)*(-[A-Za-z0-9.]+)?[ ]*\|/ {print; exit}
        found && /^[ \t]*[-*][ \t]+\*\*v[0-9]/ {print; exit}
        found && /^[ \t]*[-*][ \t]+v?[0-9]+\.[0-9]+/ {print; exit}
        found && /^[ \t]*[-*][ \t]+[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]/ {print; exit}' "$rfc_file" 2>/dev/null || true)

    if [ -z "$vh_has_rows" ]; then
        echo "NO_VH_ROWS: $rfc_file (## Version History exists but no data rows)"
        NO_VH=$((NO_VH + 1))
        continue
    fi

    # The row pattern must end the cell at the version. Without the trailing
    # `[ ]*\|`, a table whose first column holds a date rather than a version
    # matched on its year: `| 2026-07-20 | **Promoted to Accepted.** |` in
    # RFC-0853 yielded a "latest version" of 2026, and `sort -V` ranked it
    # above 1.1.0, manufacturing an out-of-sync finding against a document
    # that is perfectly in sync. Relaxing the pattern to fix 18 false
    # NO_VH_ROWS reports created this one; both are fixed by requiring the
    # number to be followed by the cell boundary rather than by anything.
    # Version extraction stays table-only, because a dated bullet carries no
    # version to sort. Where rows exist but no version does, the comparison a
    # Status version would feed into cannot be made, and that is reported
    # rather than silently treated as a clean pass.
    #
    # The row pattern stops at the version and does not require a space and a
    # pipe immediately after it. Requiring ` |` matched only tables whose first
    # column is exactly one character wider than its widest entry, so every
    # column-aligned table — `| 1.0     | 2026-04-11 | ...` — was read as
    # having no version at all. The optional `v` prefix matters for the same
    # reason `lint_no_vh_accepted.sh` allows it: 28 Accepted RFCs write
    # `| v2.12 |`.
    vh_latest=$(awk -v h="$VH_HEADING" '$0 ~ h {found=1; next} found && /^\| v?[0-9]+(\.[0-9]+)*(-[A-Za-z0-9.]+)?[ ]*\|/ {print}' "$rfc_file" 2>/dev/null | grep -oE '^\| v?[0-9]+(\.[0-9]+)*(-[A-Za-z0-9.]+)?' | sed -E 's/^\| v?//' | sort -V | tail -1 || true)

    if [ -z "$vh_latest" ]; then
        echo "NO_VH_VERSION: $rfc_file (Version History has dated rows but no version column to compare)"
        NO_VH_VERSION=$((NO_VH_VERSION + 1))
        continue
    fi

    # Compare Status version vs VH latest.
    #
    # Compared as numbers, not as strings. `2.14` and `2.14.0` are the same
    # version written two ways, and this corpus writes both: RFC-0110 declares
    # v2.14.0 in Status against a history whose latest row is 2.14. A string
    # comparison called that out of sync and has been reporting it ever since,
    # which is the failure mode of a check that cannot tell a difference from a
    # spelling.
    if [ -n "$status_version" ]; then
        if [ "$(pad3 "$status_version")" = "$(pad3 "$vh_latest")" ]; then
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
echo "         NO_VH_VERSION=$NO_VH_VERSION (dated rows, no version column; not comparable)"
echo "         DUAL_VH=$DUAL_VH (informational only, e.g., RFC-0126)"

# A lint that examined no files has verified nothing, and must not report that
# its corpus is clean. Each of these scripts takes its corpus root from the
# environment, so pointing it at a path that does not exist — which is exactly
# what happens on a CI runner when the default names a directory outside the
# repository — used to print PASS and exit 0.
if [ "$CHECKED" -eq 0 ]; then
    echo ""
    echo "FAIL: no files were examined (RFC_ROOT=$RFC_ROOT)."
    echo "This lint verified nothing. Point RFC_ROOT at the intended corpus"
    echo "before treating a pass as meaningful."
    exit 1
fi

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: Status/VH sync drift detected."
    echo "Fix: align ## Status version with latest ## Version History row."
    exit 1
fi

echo "PASS: Status/VH sync clean."
exit 0
