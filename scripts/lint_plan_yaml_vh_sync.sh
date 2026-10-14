#!/usr/bin/env bash
# scripts/lint_plan_yaml_vh_sync.sh
# Guard 7: Plan YAML header / VH row sync linter (per long-horizon plan v1.3 Phase 2 §Guard 7; P39 NEW-PATTERN)
# Behavior: Parse YAML frontmatter `version:` field + ## Version History latest row per plan file.
#           Detect OUT_OF_SYNC_YAML_OLDER / OUT_OF_SYNC_YAML_NEWER / NO_VH_NO_YAML.
#           Mirror M35 RFC Status/VH sync pattern adapted for plans (no ## Status header).
# Coverage: Plan files corpus-wide.
#
# PLAN_ROOT previously defaulted to /home/mmacedoeu/.claude/plans — an absolute
# path in one developer's home directory, committed to a shared repository. That
# made the guard's default corpus unresolvable on every other machine and on a
# CI runner, where the path does not exist.
#
# It failed closed rather than silently: the empty-corpus guard at the end of
# this file exits 1 with "no plan files were examined". So the failure mode was
# a permanently red guard on any machine but one, which is a check nobody can
# act on, rather than a false pass.
#
# The plans directory is a local scratchpad and is deliberately NOT in the
# repository — CLAUDE.md treats it as such, and RFC-0113 references
# `docs-plans-scratchpad`. So there is no in-repo default that would be correct.
#
# What this can do is resolve the conventional location relative to $HOME, so
# the guard works for whoever runs it, and say plainly that the corpus is
# local-only. A caller on a machine with no plans directory gets the existing
# "examined nothing" failure, which is the honest answer.
set -euo pipefail

PLAN_ROOT="${PLAN_ROOT:-${HOME:-}/.claude/plans}"
EXIT_CODE=0
CHECKED=0
IN_SYNC=0
OUT_OF_SYNC_OLDER=0
OUT_OF_SYNC_NEWER=0
NO_VH_NO_YAML=0
NO_VH=0
NO_YAML=0
KEEP_AS_IS=0
COMPARED=0

echo "Plan YAML/VH sync linter — Guard 7 (P39 NEW-PATTERN)"
echo "Plan root: $PLAN_ROOT"
echo ""

# Threshold: short plans (<50 lines) are exempt from VH requirement
LINE_THRESHOLD=50

while IFS= read -r -d '' plan_file; do
    basename=$(basename "$plan_file")
    # Skip non-plan files
    case "$basename" in
        *.md) ;;
        *) continue ;;
    esac

    # CHECKED counts plan files actually examined. It used to increment before
    # the .md filter, so any non-markdown file in the plan root inflated it
    # without being looked at, and CHECKED no longer matched the sum of the
    # classification counters. Confirmed with a .txt alongside one .md:
    # CHECKED=1, IN_SYNC=0, and no NO_VH_NO_YAML either — one file counted and
    # zero examined. Harmless only because the directory happens to contain
    # nothing but .md today.
    CHECKED=$((CHECKED + 1))

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
    #
    # Two defects, both inherited from the pattern this was copied from before
    # the sibling guard `lint_status_vh_sync.sh` was corrected.
    #
    # `print; exit` stops at the FIRST matching row. A Version History is
    # append-ordered oldest-first, so the first row is the OLDEST entry. A plan
    # at `version: 2.0` whose table runs `| 1.0 |` then `| 2.0 |` was reported
    # `YAML=v2.0, VH latest=v1.0` — out of sync with itself, against a
    # document that agrees. Reproduced directly before the fix.
    #
    # The row pattern also required exactly one space around the version, so
    # every column-aligned table — `| 1.0     | 2026-04-11 |` — matched nothing
    # and the plan was reported as having no Version History at all.
    #
    # So: take EVERY versioned row, sort them, and keep the highest. Version
    # sorting rather than lexicographic, since 1.10.0 is above 1.9.0
    # numerically and below it as a string.
    #
    # The scan is also bounded to the table. `found` latching on at the heading
    # and never clearing would read every later pipe table in the document as
    # Version History rows, which is the defect already repaired in the sibling
    # guard and would be reintroduced here by the naive fix.
    vh_latest=$(awk '/^##+ ([0-9]+(\.[0-9]+)*\.? )?(Version History|Changelog)/{found=1; next}
        found && /^\|/ {if (match($0, /^\|[ \t]+v?[0-9]+(\.[0-9]+)*([ \t]*\|)/)) print substr($0, RSTART, RLENGTH); next}
        found && NF {exit}' "$plan_file" 2>/dev/null \
        | grep -oE '[0-9]+(\.[0-9]+)*' | sed 's/^v//' | sort -V | tail -1 || true)

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
            # Counted under NO_VH, not NO_VH_NO_YAML. Both branches below
            # incremented the combined counter, so NO_VH and NO_YAML were
            # declared, never incremented, and never printed — three distinct
            # diagnoses reported under one name, so a reader could not tell a
            # plan with no Version History from one with no YAML version.
            NO_VH=$((NO_VH + 1))
            EXIT_CODE=1
            continue
        fi
    fi

    # No YAML version on long plan with VH = governance issue
    if [ -z "$yaml_version" ]; then
        echo "NO_YAML: $plan_file (long plan, VH v$vh_latest but no YAML version)"
        NO_YAML=$((NO_YAML + 1))
        EXIT_CODE=1
        continue
    fi

    # Compare YAML version vs VH latest
    #
    # COMPARED increments here and nowhere else, so it counts pairs actually
    # put side by side. CHECKED counts plan files read. They are different
    # populations and the summary used to print only the first beside
    # comparison outcomes, so a run that examined six plans and compared none
    # of them read as CHECKED=6 with IN_SYNC=0 — six comparisons, none in sync.
    COMPARED=$((COMPARED + 1))
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
echo "Summary: SCANNED=$CHECKED plan file(s) read, COMPARED=$COMPARED pair(s)"
echo "         IN_SYNC=$IN_SYNC"
echo "         OUT_OF_SYNC_YAML_OLDER=$OUT_OF_SYNC_OLDER"
echo "         OUT_OF_SYNC_YAML_NEWER=$OUT_OF_SYNC_NEWER"
echo "         NO_VH_NO_YAML=$NO_VH_NO_YAML (neither Version History nor YAML version)"
echo "         NO_VH=$NO_VH (YAML version present, no Version History)"
echo "         NO_YAML=$NO_YAML (Version History present, no YAML version)"
echo "         KEEP_AS_IS (short plans, no VH required)=$KEEP_AS_IS"
#
# Every plan read lands in exactly one outcome bucket. The three no-version
# buckets used to be one, so this could not have failed; now that they are
# counted separately it can, which is the point of separating them.
if [ $((IN_SYNC + OUT_OF_SYNC_OLDER + OUT_OF_SYNC_NEWER + NO_VH_NO_YAML + NO_VH + NO_YAML + KEEP_AS_IS)) -ne "$CHECKED" ]; then
    echo "FAIL: population does not reconcile: the outcome buckets sum to"
    echo "      $((IN_SYNC + OUT_OF_SYNC_OLDER + OUT_OF_SYNC_NEWER + NO_VH_NO_YAML + NO_VH + NO_YAML + KEEP_AS_IS)),"
    echo "      but SCANNED=$CHECKED. A plan was read and placed in no bucket."
    EXIT_CODE=1
fi

# A lint that examined no files has verified nothing. Its default PLAN_ROOT
# is a path outside the repository, so on a CI runner it resolves to a
# directory that does not exist, `find` yields nothing, and every counter
# stays at zero — which prints PASS and exits 0. That is the failure mode
# this guard exists to make impossible: a check that cannot see its corpus
# must not report that its corpus is clean.
if [ "$CHECKED" -eq 0 ]; then
    echo ""
    echo "FAIL: no plan files were examined (PLAN_ROOT=$PLAN_ROOT)."
    echo "This lint verified nothing. Point PLAN_ROOT at the intended corpus"
    echo "before treating a pass as meaningful."
    exit 1
fi

if [ "$EXIT_CODE" -ne 0 ]; then
    echo ""
    echo "FAIL: plan YAML/VH sync drift detected."
    echo 'Fix: align YAML frontmatter `version:` with latest ## Version History row.'
    exit 1
fi

echo "PASS: plan YAML/VH sync clean."
exit 0
