#!/usr/bin/env bash
# run-local-checks.sh — one command for every local check that has no CI step.
#
# Why this exists. Each of the suites below is runnable by hand and each has
# caught a real defect, but nothing in .github/workflows/ invokes any of them.
# That leaves the set of checks implicit: a session can run four of six and
# report "the battery is green" without anyone knowing two were skipped.
#
# This script makes the set explicit, states what it did not run, and fails if
# anything in the default set fails.
#
# NOT CI. Wiring this into a workflow is a maintainer decision; this file is
# the proposal's input, not the wiring.
#
# Usage:
#   bash scripts/run-local-checks.sh [--fast-only|--with-slow] [--with-cites]
#
#   --fast-only   (default) the checks that take seconds
#   --with-slow   also the two that compile or walk the whole corpus
#   --with-cites  also the three validate_cites cases, opt-in by rule
#
# Exit codes:
#   0   every selected check passed
#   1   at least one check failed
#   2   a check could not be run

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT" || exit 2

MODE="fast"
WITH_CITES=0
for arg in "$@"; do
    case "$arg" in
        --fast-only)  MODE="fast" ;;
        --with-slow)  MODE="slow" ;;
        --with-cites) WITH_CITES=1 ;;
        -h|--help)
            sed -n '2,/^set -uo/p' "${BASH_SOURCE[0]}" \
                | sed -e 's/^# \{0,1\}//' -e '/^$/q' | head -n -1
            exit 0
            ;;
        *)
            echo "unknown argument: $arg (try --help)" >&2
            exit 2
            ;;
    esac
done

PASSED=()
FAILED=()
UNRUNNABLE=()

# run <label> <command...>
run() {
    local label="$1"; shift
    local out rc
    printf '  %-34s ' "$label"
    out="$("$@" 2>&1)"; rc=$?
    if [[ $rc -eq 0 ]]; then
        # Deliberately no count here. An earlier version scraped each suite's
        # own total out of its output text, which meant six different output
        # formats to match and produced blank counts for four of them. Each
        # suite prints its own number; this reports only whether it passed.
        printf 'ok\n'
        PASSED+=("$label")
    elif [[ $rc -eq 127 ]]; then
        printf 'MISSING\n'
        UNRUNNABLE+=("$label")
    else
        printf 'FAIL\n'
        printf '%s\n' "$out" | tail -15 | sed 's/^/      /'
        FAILED+=("$label")
    fi
}

echo "local checks (mode: $MODE)"
echo "========================================="

echo ""
echo "guard suites"
run "dangerous tokens"        bash scripts/test-dangerous-tokens.sh
run "safe-commit arguments"   bash scripts/test-safe-commit.sh
run "installer"               bash scripts/install_test.sh
if [[ "$WITH_CITES" -eq 1 ]]; then
    run "document guards +cites" bash scripts/mutation-verify-doc-guards.sh --with-cites
else
    run "document guards"     bash scripts/mutation-verify-doc-guards.sh
fi

echo ""
echo "document linters"
run "rfc anchors"             python3 scripts/lint-rfc-anchors.py
run "rfc type refs"           python3 scripts/lint-rfc-type-refs.py
run "section cites"           bash scripts/lint_section_cites.sh
run "accepted rfc vh"         bash scripts/lint_no_vh_accepted.sh
run "frontmatter"             python3 scripts/lint_frontmatter.py

if [[ "$MODE" == "slow" ]]; then
    echo ""
    echo "slow checks"
    run "canonical blobs"      python3 scripts/verify_canonical_blobs.py
    run "zk guard mutations"   python3 scripts/mutate-zk-guards.py
else
    echo ""
    echo "skipped: canonical blobs, zk guard mutations"
    echo "  both compile or walk the whole tree; pass --with-slow to include them"
    echo "  the zk harness needs several GB of build headroom while it runs"
fi

if [[ "$WITH_CITES" -eq 0 ]]; then
    echo ""
    echo "not run: scripts/validate_cites.sh"
    echo "  user-initiated only; never automatic, including here."
    echo "  pass --with-cites to include its three harness cases."
fi

echo ""
echo "========================================="
printf '%d passed, %d failed' "${#PASSED[@]}" "${#FAILED[@]}"
if [[ ${#UNRUNNABLE[@]} -gt 0 ]]; then
    printf ', %d could not run' "${#UNRUNNABLE[@]}"
fi
printf '\n'

if [[ ${#UNRUNNABLE[@]} -gt 0 ]]; then
    echo ""
    echo "could not run:"
    printf '  - %s\n' "${UNRUNNABLE[@]}" >&2
fi

if [[ ${#FAILED[@]} -gt 0 ]]; then
    echo ""
    echo "failed:"
    printf '  - %s\n' "${FAILED[@]}" >&2
    exit 1
fi
if [[ ${#UNRUNNABLE[@]} -gt 0 ]]; then
    exit 2
fi
exit 0