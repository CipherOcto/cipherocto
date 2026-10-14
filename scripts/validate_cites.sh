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
# The §section part matches multi-word names: a space followed by another
# capitalised word continues the section, a space followed by lowercase starts
# prose. `§[A-Za-z0-9._]+` alone stopped at the first space, so `§Adversary
# Analysis` was looked up as `Adversary`, reported INVALID, and the failure
# message told a reader to fix a citation that was already correct.
#
# This repo writes `§section_name` in Title Case (CLAUDE.md §RFC Reference
# Conventions), which is what makes the capitalisation test usable as a
# boundary. 5,408 cites in the corpus carry a multi-word section name and
# every one of them was truncated before the fix.
#
# Each word must END on an alphanumeric, not merely contain one. The looser
# form `[A-Za-z0-9._]+([ ][A-Z]...)*` matched `§Adversary Analysis. The` out of
# `RFC-0909 §Adversary Analysis. The attacker must hold stake` — the `.` was
# absorbed into the word and the following capitalised word of the next sentence
# was taken as part of the section name. Requiring the final char to be
# alphanumeric drops trailing sentence punctuation (`§Identity.` -> `§Identity`)
# and stops at the sentence break, so the two error directions cancel.
RFC_SECTION='§[A-Za-z0-9._]*[A-Za-z0-9]([ ][A-Z][A-Za-z0-9._]*[A-Za-z0-9])*'
RFC_CITE_REGEX="RFC-[0-9]+(-[a-zA-Z0-9]+)?( v[0-9]+(\\.[0-9]+)*)?( ?${RFC_SECTION})?"

# The Version History heading, in the four shapes this corpus writes it: bare,
# numbered (`## 6. Version History`), and each of those with a leading section
# sign (`## §6 Version History`). `lint_status_vh_sync.sh` and
# `lint_no_vh_accepted.sh` were both taught all four; this script had no such
# pattern at all, and the STALE check grepped the whole file instead of finding
# a section.
#
# A missing pattern is not a harmless omission here. It is why the version this
# linter reported as "latest" was the first version-shaped cell in the document
# rather than the newest Version History row.
VH_HEADING='^##+ (§)?([0-9]+(\.[0-9]+)*\.? )?(Version History|Changelog|§Version History|§Changelog)'

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
# When no args, pre-filter to files that contain at least one RFC- cite pattern.
# Skips files with zero cites (e.g., most of docs/) entirely; avoids per-file
# bash + grep fork for empty input.
files_to_check=("$@")
if [ ${#files_to_check[@]} -eq 0 ]; then
    while IFS= read -r -d '' f; do
        files_to_check+=("$f")
        # Cap file count to prevent runaway loops on accidental `find /` invocation.
        if [ "${#files_to_check[@]}" -ge "$CITE_MAXFILES" ]; then
            echo "WARN: hit CITE_MAXFILES=$CITE_MAXFILES; truncating file list" >&2
            break
        fi
    done < <(timeout "$CITE_TIMEOUT" find "$RFC_ROOT" docs -maxdepth "$CITE_MAXDEPTH" -type f -name '*.md' -print0 2>/dev/null \
        | timeout "$CITE_TIMEOUT" xargs -0 -r grep -lEZ 'RFC-[0-9]+' 2>/dev/null || true)
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
        if [[ "$cite" =~ ($RFC_SECTION) ]]; then
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
            #
            # Case-insensitively. A prose citation is not obliged to reproduce
            # a heading's capitalisation, and this corpus writes both:
            # `§Data Structures` and `§data structures` against headings in
            # Title Case. The comparison was case-sensitive, so a correctly
            # cited section was reported missing purely on capitalisation.
            # Demonstrated directly against RFC-0110: with a heading
            # `Schoolbook MUL Limb Reduction`, `§Schoolbook MUL Limb
            # Reduction` matched and `§schoolbook mul limb reduction` did not.
            #
            # Lowercasing both sides is safe here in a way it is not for the
            # trailing-qualifier rule: case carries no meaning that could
            # distinguish two different sections, so no citation is laundered.
            if [[ "${sections,,}" != *"${sec_normalized,,}"* ]]; then
                sec_int=""
                if [[ "$sec_normalized" =~ ^([0-9]+) ]]; then
                    sec_int="${BASH_REMATCH[1]}"
                fi
                if [ -n "$sec_int" ]; then
                    if [[ "${sections,,}" != *"${sec_int}."* && "${sections,,}" != *"${sec_int} "* ]]; then
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
            # The latest row of the Version History TABLE.
            #
            # This was `grep -oE '\|[ \t]+[0-9]+(\.[0-9]+)+[ \t]+\|' ... | head -1`,
            # which has two defects:
            #
            # `head -1` takes the FIRST such cell in the file. A Version History
            # is append-ordered oldest-first, so the first row is the OLDEST
            # entry. RFC-0009's history runs 0.1, 0.2, 0.3 ... 1.2, and the
            # linter reported its latest as 0.1. Measured across the corpus:
            # for 41 of 163 RFC numbers the version reported as latest is not
            # the latest.
            #
            # The grep was also FILE-WIDE, so a phase table, migration table or
            # deprecation matrix was read as a Version History. One file,
            # RFC-0206-v40, accounts for 529 of the 1007 STALE findings and was
            # reported at `latest v0.1` while its own row describes the
            # v2.3 to v2.4 amendment.
            #
            # Now: find the Version History section, take every versioned row in
            # it, sort by version, keep the highest. `sort -V` rather than
            # lexicographic, because 1.10.0 is above 1.9.0 numerically and below
            # it as a string. The table ends at its last row, so a later pipe
            # table cannot leak in.
            #
            # This does not change WHICH documents are reported — 997 of the
            # 1007 findings are genuinely stale, measured against the true
            # latest row. It changes the message from one a maintainer cannot
            # act on into one they can.
            vh_latest=$(timeout "$CITE_TIMEOUT" awk -v h="$VH_HEADING" '
                $0 ~ h {found=1; next}
                found && /^\|/ {if (match($0, /^\|[ \t]+v?[0-9]+(\.[0-9]+)*(-[A-Za-z0-9.]+)?[ \t]*\|/)) print substr($0, RSTART, RLENGTH); next}
                found && NF {exit}' "$rfc_path" 2>/dev/null \
                | grep -oE '[0-9]+(\.[0-9]+)*(-[A-Za-z0-9.]+)?' | sort -V | tail -1 || true)

            if [ -n "$vh_latest" ] && [ "$version" != "$vh_latest" ]; then
                # The cited version must appear in the SAME Version History, not
                # anywhere in the file. This test was also file-wide, so a pin
                # that happened to match a row in some unrelated table
                # suppressed the finding against a document that never carried
                # that version.
                cited_in_vh=$(timeout "$CITE_TIMEOUT" awk -v h="$VH_HEADING" -v want="$version" '
                    $0 ~ h {found=1; next}
                    found && /^\|/ {if (match($0, /^\|[ \t]+v?[0-9]+(\.[0-9]+)*(-[A-Za-z0-9.]+)?[ \t]*\|/)) {
                        v=substr($0, RSTART, RLENGTH); gsub(/^[ \t]*\|[ \t]*v?|[ \t]*\|$/, "", v)
                        if (v == want) {print v; exit}} next}
                    found && NF {exit}' "$rfc_path" 2>/dev/null || true)
                if [ -z "$cited_in_vh" ]; then
                    echo "STALE [version pin mismatch]: $file:$line_num: $cite (cited v$version, latest v$vh_latest in $rfc_path)"
                    STALE=$((STALE + 1))
                    EXIT_CODE=1
                    continue
                fi
            fi
        fi

        VALID=$((VALID + 1))
    # Uses $RFC_CITE_REGEX. The pattern used to be hardcoded here *and* defined
    # as a constant above, with the constant referenced nowhere — so editing the
    # named source of truth changed nothing and the script kept running on the
    # inline copy. Two copies of one pattern, one of them decorative.
    done < <(timeout "$CITE_TIMEOUT" grep -noE "$RFC_CITE_REGEX" "$file" 2>/dev/null || true)
done

echo ""
# What share of the repository this run actually looked at.
#
# Stated in files rather than tokens on purpose. CHECKED counts RFC-plus-
# section cite pairs, which is not the same population as a count of section
# marks, and dividing one by the other would produce a confident percentage
# describing nothing. The real blind spot is structural: this linter walks
# "$RFC_ROOT docs", so every tracked markdown file outside those two roots is
# unexamined no matter how many cites it carries.
#
# 789 of the 1256 tracked markdown files are missions, which is the bulk of
# what goes unseen. Widening the roots changes what this linter rejects, so
# that is a maintainer decision; making the omission visible is not.
SCOPE_ALL=$(git ls-files '*.md' 2>/dev/null | grep -vc '^\.claude/worktrees/' || true)
SCOPE_IN=$(git ls-files '*.md' 2>/dev/null | grep -v '^\.claude/worktrees/' | grep -cE "^(rfcs|docs)/" || true)
if [ -n "$SCOPE_ALL" ] && [ "$SCOPE_ALL" -gt 0 ] 2>/dev/null; then
    echo "Summary: CHECKED=$CHECKED VALID=$VALID PHANTOM=$PHANTOM INVALID=$INVALID STALE=$STALE"
    echo "         SCOPE: $SCOPE_IN of $SCOPE_ALL tracked markdown files lie under (rfcs docs); $((SCOPE_ALL - SCOPE_IN)) unexamined"
else
    echo "Summary: CHECKED=$CHECKED VALID=$VALID PHANTOM=$PHANTOM INVALID=$INVALID STALE=$STALE"
fi

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: cite validation found issues."
    echo "Fix: per BLUEPRINT.md §RFC Reference Conventions, all RFC-XXXX §section cites must resolve on disk."
    exit 1
fi

echo "PASS: all cites valid."
exit 0
