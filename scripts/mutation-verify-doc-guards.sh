#!/usr/bin/env bash
# scripts/mutation-verify-doc-guards.sh
# Mutation-verify the numbered document guards in scripts/.
#
# WHY THIS IS A SEPARATE SCRIPT FROM mutation-verify-guards.sh
# ------------------------------------------------------------
# `mutation-verify-guards.sh` covers the 10 cargo-test guards in
# crates/octo-cli/tests/security_regressions.rs. It plants defects in the
# operator guide and two workflow files and re-runs a compiled test binary.
#
# The seven numbered document guards are not covered by it at all. Nothing
# mutation-verifies them. A defect like the one in
# `the-status-version-sync-guard-compares-one-file-in-seven-and-its-no-vh-
# branch-can-never-fire` — a guard whose Status read matched a heading line
# that never carries a version, so 210 of 247 files were skipped and it still
# reported PASS — was found by reading, and a year from now nobody would know
# whether the same class of defect had returned.
#
# WHY FIXTURES INSTEAD OF MUTATING TRACKED FILES
# ---------------------------------------------
# The cargo harness backs up tracked files and restores them. Document guards
# do not need that, and it is the reason they were never automated: mutating
# `rfcs/` to test a document linter risks leaving a defect in the corpus, and
# it cannot be done while the corpus itself is red.
#
# Four of the guards take their corpus root from the environment
# (`RFC_ROOT`), and `validate_cites.sh` additionally takes explicit file
# arguments. So each case here writes a tiny fixture corpus to a temp
# directory and points the guard at it. No tracked file is read for its
# defects, none is written, and an interrupted run cannot damage the worktree:
# there is nothing in the worktree to restore.
#
# That also means the fixtures declare the *expected* state, so every case is
# a two-sided assertion. A guard that is green because it examined nothing
# fails here, because a fixture with a defect must turn it red.
#
# USAGE:
#   scripts/mutation-verify-doc-guards.sh              # all guards
#   scripts/mutation-verify-doc-guards.sh section_cites # substring filter
#
# EXIT: 0 only if every case detected its planted defect AND every case
# reported the fixture clean before the defect was added.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

FILTER="${1:-}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT INT TERM

PASS=0
FAIL=0
SKIP=0

# ---------------------------------------------------------------------------
# assertion helpers
# ---------------------------------------------------------------------------

# fixture_is_clean <name> <guard> — the guard must pass on the bare fixture.
# This is the direction most guards are never tested in: a guard that is red
# for an unrelated reason (or that cannot fail at all) is caught by the
# planted-defect case, but one that is red on a clean corpus would otherwise
# pass the whole suite while being useless.
assert_clean() {
    local name="$1" guard_fn="$2"
    if "$guard_fn" >"$WORK/out" 2>&1; then
        printf '%-34s %-8s %s\n' "$name" "clean" "ok"
    else
        printf '%-34s %-8s %s\n' "$name" "clean" "FALSE POSITIVE on a clean fixture"
        sed 's/^/      /' "$WORK/out" | head -6
        FAIL=$((FAIL + 1))
        return 1
    fi
    return 0
}

# assert_contains <name> <guard> <substring> <must NOT contain>
# An ACCEPT-direction assertion: the guard must exit 0 AND its output must
# contain the first substring while omitting the second.
#
# This exists because `run_case` can only assert a planted defect, and a counter
# that lies about its own population is not a defect the guard detects — the
# guard is green precisely when it is lying. A population case expressed as a
# run_case passes against the regressed counter, which was verified by doing
# exactly that.
assert_contains() {
    local name="$1" guard_fn="$2" want="$3" forbid="$4"
    local out
    if ! out="$("$guard_fn" 2>&1)"; then
        printf '%-34s %-8s %s\n' "$name" "accept" "expected exit 0 on a clean fixture"
        FAIL=$((FAIL + 1))
        return 1
    fi
    if ! grep -qF "$want" <<< "$out"; then
        printf '%-34s %-8s %s\n' "$name" "accept" "output did not contain '$want'"
        FAIL=$((FAIL + 1))
        return 1
    fi
    if [ -n "$forbid" ] && grep -qF "$forbid" <<< "$out"; then
        printf '%-34s %-8s %s\n' "$name" "accept" "output claimed '$forbid' it must not"
        FAIL=$((FAIL + 1))
        return 1
    fi
    printf '%-34s %-8s %s\n' "$name" "accept" "ok"
    return 0
}

# assert_red <name> <guard> <expected substring>
# The guard must go red AND name the specific finding. Checking only the exit
# code would accept a guard that failed for the wrong reason — which is how
# "green while measuring nothing" survives an exit-code-only test.
#
# The match is against the FINDING lines only, never the whole output.
#
# Matching the whole output is a bug this harness had. Two of the expected
# strings are also counter names, and every guard prints its counters on a
# clean run:
#
#     PASS=1 FAIL=0 MISSING_VH=0
#     OUT_OF_SYNC_STATUS_OLDER=0
#
# So `grep -qF MISSING_VH` succeeded against `MISSING_VH=0` on a guard that had
# found nothing at all, and the assertion passed. The case reported "ok" while
# the guard never emitted a finding.
#
# The counter is a statement about the run; the finding is the run's output.
# Asserting on the output alone lets a guard report 0 and still be believed.
#
# The finding-line prefixes span three output styles, and narrowing to the
# shell guards' names made three cases fail — correctly, because the Python
# guards report their findings in their own shapes:
#
#     lint_frontmatter.py   "1  unquoted value containing ': ' — quote the value"
#     lint-rfc-anchors.py   "  §invented anchor"
#     lint-rfc-type-refs.py "  TotallyMadeUpTypeName"
#
# None of those lines starts with a shell-guard prefix, so requiring one would
# reject genuine detections. `lint_frontmatter.py` groups its findings by cause
# and prefixes each group with a COUNT, so its cause lines start with a digit.
# What all three share is that the finding is not a bare `NAME=value` counter,
# which is the shape every guard prints on a clean run.
FINDING_RE='^(REJECT|INVALID|PHANTOM|STALE|MISSING_VH|NO_VH|NO_STATUS_VERSION|NO_VH_ROWS|NO_VH_VERSION|OUT_OF_SYNC|NEW |  |[0-9]+  )'

assert_red() {
    local name="$1" guard_fn="$2" expect="$3"
    local out
    if ! out="$("$guard_fn" 2>&1)"; then
        local findings
        findings="$(grep -E "$FINDING_RE" <<< "$out" | grep -vE '^[A-Z_]+=' )"
        if [ -z "$findings" ]; then
            printf '%-34s %-8s %s\n' "$name" "detect" "went red but emitted NO finding line"
            printf '%s\n' "$out" | head -6 | sed 's/^/      /'
            FAIL=$((FAIL + 1))
            return 1
        fi
        if ! grep -qF "$expect" <<< "$findings"; then
            printf '%-34s %-8s %s\n' "$name" "detect" "went red, but not with '$expect'"
            printf '%s\n' "$findings" | head -4 | sed 's/^/      /'
            FAIL=$((FAIL + 1))
            return 1
        fi
        printf '%-34s %-8s %s\n' "$name" "detect" "ok"
        return 0
    fi
    printf '%-34s %-8s %s\n' "$name" "detect" "MISSED the planted defect"
    FAIL=$((FAIL + 1))
    return 1
}

# case <name> <guard-fn> <expected-finding>
#
# Each case gets a FRESH corpus directory. This used to share one directory
# across all four cases, and the defects piled up: case 1 planted a phantom
# cite, case 2 then measured that cite during its own "clean" phase and
# reported a false positive. A harness whose fixtures leak into each other
# measures the previous case's leftovers, not the guard.
#
# The clean assertion runs first, on the fixture alone. Then the defect is
# planted and the same guard must go red AND name the specific finding.
run_case() {
    local name="$1" guard_fn="$2" expect="$3"
    if [ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-$(printf '%s' "$name" | tr -c 'a-zA-Z0-9' '_')"
    rm -rf "$own"
    mkdir -p "$own/rfcs" "$own/docs"
    FIX="$own"
    write_base_fixture
    prepare_case
    assert_clean "$name" "$guard_fn" || return 0
    write_defect
    assert_red "$name" "$guard_fn" "$expect"
    PASS=$((PASS + 1))
}

# Optional per-case corpus setup, run AFTER the base fixture and AFTER $FIX is
# set. Cases that need extra fixture files override this; it exists as a hook
# because writing them outside run_case means writing them into the PREVIOUS
# case's directory, since $FIX does not exist until run_case creates it.
prepare_case() { :; }

# Each case overrides `write_defect` with the defect its guard must catch.
#
# The hook used to be named `before_defect` and was called BEFORE the clean
# assertion, which planted the defect first and then asserted the guard was
# green on a corpus containing it. All four cases reported a false positive
# for that reason. The name was the bug: it said "before" and was invoked
# before, and the clean assertion sat after it. Now there is one hook, it runs
# after the clean assertion, and it says what it does.
write_defect() { :; }

# ---------------------------------------------------------------------------
# fixtures
# ---------------------------------------------------------------------------

# The clean corpus every case starts from. A FUNCTION, not a one-shot write:
# each case gets its own directory, so a base fixture written once at startup
# would belong to a corpus no case is looking at.
#
# The document is deliberately correct in every respect these guards check. A
# "clean" failure therefore means the guard rejects a VALID document, which is
# the false-positive direction every one of these guards has shipped at some
# point.
#
# Three constructs in here exist to pin specific false positives that were
# fixed, rather than to make the document look realistic. Each was a real
# defect, and each would otherwise have nothing stopping its reintroduction:
#
#   `§5a. Platform Loss` — a numeric-then-alpha cite whose subsection heading
#   exists. The rule that rejected it rejected exactly the class it was written
#   to catch (real subsections like `§5a` and `§9b`) while passing all 344
#   review-finding ids (`§A7`, `§F4`) it was written to reject. Inverted rule,
#   58 false rejections. A regression here now fails this fixture.
#
#   `| v1.0.0 |` — a `v`-prefixed version column. Requiring a bare digit
#   reported 28 Accepted RFCs with a populated Version History as having no
#   data rows at all.
#
#   Status version AFTER the `## Status` heading, matching the VH row. Reading
#   the version off the heading line skipped 210 of 247 files and still
#   reported PASS.
#
#   Cites wrapped in sentence punctuation — `(per RFC-0957 §Identity),` and a
#   line-final `§Algorithms.` The extraction used to absorb the closing paren
#   and the period into the cite token, and Reject 1 could not match them
#   because its character class never closed. Repairing the class surfaced
#   1431 findings, all of them prose. These two lines are that finding's
#   smallest reproduction: a greedy extraction reports them, a correct one
#   passes them.
write_base_fixture() {
    cat >"$FIX/rfcs/0001-base.md" <<'EOF'
---
status: accepted
---

# RFC-0001 Base

## Status

Accepted (v1.0.0) — promoted 2026-01-01

## 1. Identity

Body text. The platform loss is described in §5a. Platform Loss.

Written as (per RFC-0957 §Identity), and the method in §Algorithms.

### 5a. Platform Loss

Subsection text.

## Version History

| Version | Date | Changes |
| --- | --- | --- |
| v1.0.0 | 2026-01-01 | Initial |
EOF
}

# The Accept-direction counterweight for §section_name: a cite whose target
# section does not exist. `lint_section_cites.sh` must reject it.
write_section_cites_defect() {
    printf '\nSee RFC-0001 §7z for a section that was never written.\n' >>"$FIX/rfcs/0001-base.md"
}

# The counterweight for the Status/VH guard: a Status version that disagrees
# with the latest Version History row.
write_status_vh_defect() {
    cat >"$FIX/rfcs/0002-drift.md" <<'EOF'
# RFC-0002 Drift

## Status

Accepted (v1.0.0) — promoted

## Version History

| Version | Date | Changes |
| --- | --- | --- |
| 1.0.0 | 2026-01-01 | Initial |
| 2.0.0 | 2026-02-01 | Second |
EOF
}

# An Accepted RFC with no Version History at all. This is the fixture that
# proves the `NO_VH` branch is reachable — the branch read `Accepted` off the
# heading line, which this corpus never does, so it was structurally zero.
write_no_vh_defect() {
    cat >"$FIX/rfcs/0003-novh.md" <<'EOF'
# RFC-0003 NoVH

## Status

Accepted (v1.0.0) — promoted 2026-01-01

## 1. Scope

Text.
EOF
}

# A citation to a section that does not exist in the cited RFC. Also the
# multi-word section name that the extraction used to truncate at the first
# space, so the clean pass proves `§Adversary Analysis` resolves whole.
write_cites_defect() {
    cat >"$FIX/rfcs/0004-target.md" <<'EOF'
# RFC-0004 Target

## Adversary Analysis

The attacker must hold stake.
EOF
    cat >"$FIX/docs/cites.md" <<'EOF'
See RFC-0004 §Adversary Analysis for the threat model.

See RFC-0004 §Nonexistent Section for a citation that cannot resolve.
EOF
}

# ---------------------------------------------------------------------------
# cases
# ---------------------------------------------------------------------------

# The case inventory, derived from every case-name literal in this file.
#
# It used to be `grep 'run_case "..."'`, which finds only the cases written in
# that one style. The accept-direction cases print their id directly instead,
# because they assert on the guard's own output rather than planting a defect
# and expecting the guard to notice — so four cases were missing from the
# listing, and `--help` described 10 of the 14 cases the harness actually ran.
#
# That is the same defect class this harness exists to detect, in the harness:
# reporting a population narrower than the one examined, with nothing in the
# output marking the gap. The count a maintainer reads to decide what is
# covered came from the narrow list, so "8 guards, 13 cases" was itself a
# figure produced by the instrument underreporting.
CASE_FUNCTIONS=0
CASE_UNREPORTED=0

# Every case function must move a counter.
#
# The inventory check below catches a case that runs without naming itself.
# This catches the other half: a case that runs and names itself but whose body
# returns before reporting anything. Neither its pass nor its fail is recorded,
# it prints no row, and the run still exits 0 — so a case that silently stopped
# testing anything is indistinguishable from a case that passed.
#
# The skip path counts as a report, because a filtered run is expected to say
# so rather than go quiet.
run_case_fn() {
    local fn="$1"
    local before_p=$PASS before_f=$FAIL before_s=$SKIP
    CASE_FUNCTIONS=$((CASE_FUNCTIONS + 1))
    "$fn"
    if [ "$PASS" -eq "$before_p" ] && [ "$FAIL" -eq "$before_f" ] \
        && [ "$SKIP" -eq "$before_s" ]; then
        CASE_UNREPORTED=$((CASE_UNREPORTED + 1))
        printf '%-34s %-8s %s\n' "${fn#case_}" "silent" \
            "executed but reported nothing — it counted neither a pass nor a fail"
    fi
}

case_ids() {
    grep -oE '"[a-z_]+/[a-z-]+"' "${BASH_SOURCE[0]}" | tr -d '"' | sort -u
}

# -h/--help lists the cases instead of being treated as a name filter.
# `mutation-verify-guards.sh` had this defect: any argument was a substring
# filter, so `--help` matched nothing and the run reported a successful
# verification of zero guards. An unmatched filter now fails loudly; `--help`
# is the one argument that is meant to match nothing.
if [ "$FILTER" = "-h" ] || [ "$FILTER" = "--help" ]; then
    echo "Usage: scripts/mutation-verify-doc-guards.sh [case-name-substring]"
    echo ""
    echo "Cases:"
    case_ids | sed 's/^/  /'
    printf '  (%s total)\n' "$(case_ids | wc -l | tr -d ' ')"
    exit 0
fi

printf '%-34s %-8s %s\n' "GUARD" "PHASE" "RESULT"
printf '%s\n' "------------------------------------------------------------"

# Each guard runs as a shell FUNCTION. It started as a command string, which
# the assertion helpers quoted as a single word, so the shell searched for a
# file literally named "env RFC_ROOT=... bash scripts/..." and every case
# failed with "No such file or directory".
#
# That failure was loud rather than silent, which is the one mercy: the run
# reported four false positives and exited 1 instead of claiming success. A
# harness that cannot invoke its guard must fail, not pass quietly.
section_cites_guard() { env RFC_ROOT="$FIX/rfcs" bash scripts/lint_section_cites.sh; }
status_vh_guard()      { env RFC_ROOT="$FIX/rfcs" bash scripts/lint_status_vh_sync.sh; }
no_vh_guard()          { env RFC_ROOT="$FIX/rfcs" bash scripts/lint_no_vh_accepted.sh; }
cites_guard()          { env RFC_ROOT="$FIX/rfcs" bash scripts/validate_cites.sh "$FIX/docs/cites.md"; }

# `lint_frontmatter.py` takes `--tree`, but it RESOLVES that tree against the
# repository root, so a temp path under /tmp is not reachable through a
# relative tree name. It is reachable through an absolute one, because
# `pathlib`'s `/` operator returns the right operand when it is absolute.
#
# Reaching it that way exposed a crash: `path.relative_to(ROOT)` raised
# ValueError on the first bad document outside the repository, after the cause
# had been classified and before any counter printed. Fixed in
# `display_path`.
frontmatter_guard() {
    python3 scripts/lint_frontmatter.py --tree "$FIX/docs" --report
}

# `lint-rfc-anchors.py` computed its corpus root from `__file__` with no
# override, which is what made it the last guard nothing could test. It now
# takes `--root`, and `--baseline` points at a file inside the fixture so a
# fixture run can never be measured against the repository's 588 entries.
#
# Both flags default to today's behaviour, so the repository run is unchanged.
anchors_guard() {
    python3 scripts/lint-rfc-anchors.py --root "$FIX" --baseline "$FIX/baseline.txt"
}

# Each case names the defect its guard must catch, and asserts both directions:
# clean fixture first (must pass), then the planted defect (must fail AND name
# the specific finding).
# A cite deeper than five levels. This rule worked all along, but it is pinned
# here because it shares its loop with Reject 1 and Reject 3, and the extractor
# feeding that loop was rewritten.
write_section_cites_overdeep_defect() {
    printf '\nSee §1.2.3.4.5.6.7 for an anchor deeper than five levels.\n' \
        >>"$FIX/rfcs/0001-base.md"
}

case_section_cites() {
    write_defect() { write_section_cites_defect; }
    run_case "section_cites/phantom-token" section_cites_guard "REJECT [phantom-token]"
}

# The QUOTED citation name, `§"Roles and Authorities"`. 264 sections are cited
# this way in the corpus and the extraction matched none of them, so a file
# containing only quoted citations reported EXAMINED=0 and printed "none
# contained a cite token" — a clean result produced by examining nothing.
#
# The fixture deliberately contains NOTHING else, because a fixture with one
# unquoted cite would still pass while the quoted arm was broken. The count is
# the assertion: EXAMINED must equal the number of quoted citations.
write_quoted_cites_only() {
    cat >"$FIX/rfcs/0005-quoted.md" <<'EOF'
# RFC-0005 Quoted

## 1. Scope

See §"Roles and Authorities" and §"Slash Reason Codes" for the normative rules.
EOF
}

case_section_cites_quoted() {
    if [ -n "$FILTER" ] && [[ "section_cites/quoted-names" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-quoted"
    rm -rf "$own"
    mkdir -p "$own/rfcs"
    FIX="$own"
    write_quoted_cites_only
    local out n
    out="$(section_cites_guard 2>&1)"
    n="$(grep -oE 'EXAMINED=[0-9]+' <<< "$out" | grep -oE '[0-9]+' || echo 0)"
    if [ "$n" -eq 2 ]; then
        printf '%-34s %-8s %s\n' "section_cites/quoted-names" "extract" "ok (both quoted names read)"
    else
        printf '%-34s %-8s %s\n' "section_cites/quoted-names" "extract" \
            "examined $n of 2 quoted citations — the quoted-name rendering is invisible"
        FAIL=$((FAIL + 1))
    fi
    PASS=$((PASS + 1))
}

# A coverage figure must describe the population the run actually examined.
#
# `lint_section_cites.sh` divides EXAMINED — counted from RFC_ROOT — by a
# repository-wide `git ls-files` total. On the real tree those are the same
# corpus and the figure is meaningful. Against a fixture they are not: a
# fixture whose every citation was read reported COVERAGE=0%, which reads as
# "nothing was examined". That is the exact misreading the figure was added to
# prevent, reproduced by the figure itself.
#
# This is an accept-direction assertion. The defect is a number that reports,
# not a defect that goes unreported, so planting one will never provoke it. The
# assertion is on the guard's own output for a run that examined a whole
# fixture — a population it can describe completely, and decline to.
case_section_cites_coverage() {
    if [ -n "$FILTER" ] && [[ "section_cites/coverage-suppressed" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-section-cites-coverage"
    rm -rf "$own"
    mkdir -p "$own/rfcs"
    FIX="$own"
    write_base_fixture
    local out
    out="$(section_cites_guard 2>&1)"
    # A numeric figure on this run is a denominator drawn from a corpus the
    # run did not read, presented as a measurement of the one it did.
    if grep -qE 'COVERAGE=[0-9]+%' <<< "$out"; then
        printf '%-34s %-8s %s\n' "section_cites/coverage-suppressed" "accept" \
            "printed a numeric coverage figure for a corpus it shared no denominator with"
        FAIL=$((FAIL + 1))
    elif ! grep -q 'COVERAGE not reported' <<< "$out"; then
        printf '%-34s %-8s %s\n' "section_cites/coverage-suppressed" "accept" \
            "coverage figure dropped without saying why — silent suppression reads as no measurement"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "section_cites/coverage-suppressed" "accept" \
            "ok (suppressed, with reason)"
    fi
    PASS=$((PASS + 1))
}

# Every guard that examines a subset must SAY so. The dominant defect class in
# this repository has been a guard reporting a total beside a result computed
# over a narrower population, with nothing in the output marking the gap.
#
# These assertions are accept-direction: a guard that quietly drops the
# population statement is not a guard that fails, so a planted defect cannot
# detect it.
case_no_vh_population() {
    if [ -n "$FILTER" ] && [[ "no_vh_accepted/population-statement" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-nvhpop"
    rm -rf "$own"
    mkdir -p "$own/rfcs"
    FIX="$own"
    write_base_fixture
    # One Accepted and one Draft, so the two populations differ and the
    # arithmetic has something to check.
    cat >"$FIX/rfcs/0006-draft.md" <<'EOF'
---
status: draft
---

# RFC-0006 Draft

## Status

Draft

## Body

No Version History, and none is required of a Draft.
EOF
    local out n
    out="$(no_vh_guard 2>&1)"
    # The non-Accepted file must be named, and the count must match it.
    if ! grep -q 'NOT_ACCEPTED=' <<< "$out"; then
        printf '%-34s %-8s %s\n' "no_vh_accepted/population-statement" "accept" \
            "no NOT_ACCEPTED line: the guard does not say what it skipped"
        FAIL=$((FAIL + 1))
    else
        n="$(grep -oE 'NOT_ACCEPTED=[0-9]+' <<< "$out" | grep -oE '[0-9]+')"
        if [ "$n" -ge 1 ]; then
            printf '%-34s %-8s %s\n' "no_vh_accepted/population-statement" "accept" \
                "ok (NOT_ACCEPTED=${n})"
        else
            printf '%-34s %-8s %s\n' "no_vh_accepted/population-statement" "accept" \
                "NOT_ACCEPTED=0 while the fixture holds a Draft"
            FAIL=$((FAIL + 1))
        fi
    fi
    PASS=$((PASS + 1))
}

case_section_cites_overdeep() {
    prepare_case() { :; }
    write_defect() { write_section_cites_overdeep_defect; }
    run_case "section_cites/over-deep" section_cites_guard "REJECT [over-deep]"
}

# The Version History heading may carry a section sign BEFORE its number, as in
# `## §6 Version History`. The guard's alternation named `§Version History`
# beside `6. Version History` and had no arm for the combination, so a file
# written that way had its Version History skipped entirely: `vh_line_num` came
# back empty, the NO_VH branch counted only Accepted files, and the loop reached
# its `continue` having incremented nothing. Two files in this corpus write that
# form, and the guard compared neither.
#
# This is a detect-direction case, because the defect's whole effect is silence.
# A fixture whose Status and Version History disagree is reported only if the
# heading was recognised at all.
write_signed_vh_clean() {
    cat >"$FIX/rfcs/0003-signed.md" <<'EOF'
# RFC-0003 Signed Heading

## Status

Accepted (v2.0.0) — promoted 2026-02-01

## §6 Version History

| Version | Date | Changes |
| --- | --- | --- |
| 1.0.0 | 2026-01-01 | Initial |
| 2.0.0 | 2026-02-01 | Second |
EOF
}

write_signed_vh_drift() {
    cat >"$FIX/rfcs/0003-signed.md" <<'EOF'
# RFC-0003 Signed Heading

## Status

Accepted (v1.0.0) — promoted 2026-01-01

## §6 Version History

| Version | Date | Changes |
| --- | --- | --- |
| 1.0.0 | 2026-01-01 | Initial |
| 2.0.0 | 2026-02-01 | Second |
EOF
}

case_status_vh_signed_heading() {
    prepare_case() { write_signed_vh_clean; }
    write_defect() { write_signed_vh_drift; }
    run_case "status_vh/signed-numbered-heading" status_vh_guard "OUT_OF_SYNC_STATUS_OLDER"
}

# Every file the guard reads must land in a category, and the categories must
# add back up to the population it scanned.
#
# This is the counterweight to a guard that reports a total beside a narrower
# population. Before this, three paths in the loop incremented nothing: a
# basename skip, a Status-version file with no Version History heading that is
# not Accepted, and two `continue`s that set a counter without tallying the file.
# On the real tree SCANNED=247 while the reported categories accounted for 240
# and a half, and the residual was invisible.
case_status_vh_reconciles() {
    if [ -n "$FILTER" ] && [[ "status_vh/population-reconciles" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-status-vh-recon"
    rm -rf "$own"
    mkdir -p "$own/rfcs"
    FIX="$own"
    write_base_fixture
    write_signed_vh_clean
    # A Version History written as dated bullets. The row reader accepts this
    # shape while the latest-version reader reads only pipe rows, so the file
    # lands on the NO_VH_VERSION path. This is the fixture that shape has to
    # come from: a date-column TABLE is rejected earlier as NO_VH_ROWS, and a
    # fixture reaching the wrong branch would pass against a guard with this
    # tally removed.
    cat >"$FIX/rfcs/0004-date-column.md" <<'EOF'
# RFC-0004 Date Column

## Status

Draft

## Version History

- 2026-01-01 — Initial
- 2026-02-01 — Second
EOF
    local out scanned nrc cls unc
    out="$(status_vh_guard 2>&1)"
    scanned="$(grep -oE 'SCANNED=[0-9]+' <<< "$out" | head -1 | grep -oE '[0-9]+' || true)"
    nrc="$(grep -oE 'NOT_RFC=[0-9]+' <<< "$out" | head -1 | grep -oE '[0-9]+' || true)"
    cls="$(grep -oE 'CLASSIFIED=[0-9]+' <<< "$out" | head -1 | grep -oE '[0-9]+' || true)"
    unc="$(grep -oE 'UNCLASSIFIED=[0-9]+' <<< "$out" | head -1 | grep -oE '[0-9]+' || true)"
    if [ -z "$nrc" ] || [ -z "$cls" ] || [ -z "$unc" ] || [ -z "$scanned" ]; then
        printf '%-34s %-8s %s\n' "status_vh/population-reconciles" "accept" \
            "no reconciliation counters reported — the population cannot be checked"
        FAIL=$((FAIL + 1))
    elif [ $((nrc + cls + unc)) -ne "$scanned" ]; then
        printf '%-34s %-8s %s\n' "status_vh/population-reconciles" "accept" \
            "$nrc + $cls + $unc = $((nrc + cls + unc)) against SCANNED=$scanned"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "status_vh/population-reconciles" "accept" \
            "ok ($nrc + $cls + $unc = SCANNED=$scanned)"
    fi
    PASS=$((PASS + 1))
}

cross_rfc_guard() {
    RFC_ROOT="$FIX/rfcs" DOCS_ROOT="$FIX/docs" \
        bash scripts/cross_rfc_propagate.sh 9001 1.0 2.0
}

# Guard 5's corpus override controlled only half its corpus.
#
# It read RFC_ROOT from the environment, which made it look fixture-drivable,
# and then scanned a hardcoded `docs` alongside it. A caller pointing RFC_ROOT
# at a fixture therefore still got the repository's real docs/ scanned — so the
# guard could not be isolated, could not be given a case, and could not be
# checked at all without reading the live corpus.
#
# The assertion is on the paths the guard reports, not on its findings. Against
# the old code the real docs/ files appear in its output and this fails, which
# is the only observable difference: the guard still does the same work, it
# just did it over a corpus the caller had not chosen.
case_cross_rfc_corpus() {
    if [ -n "$FILTER" ] && [[ "cross_rfc/fixture-corpus-only" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-cross-rfc"
    rm -rf "$own"
    mkdir -p "$own/rfcs" "$own/docs"
    FIX="$own"
    cat >"$FIX/rfcs/9001-source.md" <<'EOF'
# RFC-9001 Source

## 1. Alpha

The section a cross-RFC citation points at.
EOF
    cat >"$FIX/docs/9001-citer.md" <<'EOF'
# Doc

See RFC-9001 v1.0 §Alpha for the definition.
EOF
    local out
    out="$(cross_rfc_guard 2>&1)"
    # The fixture's docs/ file is the only place in this repository where a
    # v1.0 cite of section Alpha exists, so finding it proves the second tree
    # was actually scanned.
    #
    # An earlier version of this case asserted the opposite direction — that no
    # live-corpus path leaks into the output — and it passed against the
    # regressed guard. Real docs/ files only appear when they match the
    # fixture's cite, which they never do, so the leak is unobservable and the
    # assertion proved nothing. Asserting on what the guard FINDS in the
    # fixture is observable in both directions.
    if ! grep -qF "$FIX/docs/9001-citer.md" <<< "$out"; then
        printf '%-34s %-8s %s\n' "cross_rfc/fixture-corpus-only" "detect" \
            "did not scan the fixture docs tree — only RFC_ROOT is honoured"
        printf '%s\n' "$(grep -E '^(Summary|PROPAGATE)' <<< "$out" | head -3)" | sed 's/^/      /'
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "cross_rfc/fixture-corpus-only" "detect" \
            "ok (both fixture trees scanned)"
    fi
    PASS=$((PASS + 1))
}

inflation_guard() {
    LC_ALL="${1:-}" bash scripts/projection_inflation_metric.sh "${@:2}"
}

# Guard 6's verdict depended on the operator's locale.
#
# awk's printf honours LC_NUMERIC, so under a comma-decimal locale the rates
# came out as "100,0" instead of "100.0". Feeding that back with -v made it a
# string, and the comparison silently became lexicographic — so a 100%
# realization rate, the best possible outcome, was reported as FAIL, in a
# message that said "100,0% < 50%" three lines after printing 100,0%.
#
# The first fix pinned LC_NUMERIC, which is wrong whenever the caller has LC_ALL
# set, because LC_ALL overrides every other category including that one. Only
# testing across locales caught it: C, de_DE and en_US passed while an inherited
# LC_ALL=pt_BR still failed. LC_ALL is what has to be set.
#
# Both assertions are here because the locale alone is not enough: the case also
# requires that the script's own output never carries a comma decimal separator,
# which holds whatever the ambient locale is and fails the moment the pinning is
# removed from a machine that uses commas.
case_inflation_locale() {
    if [ -n "$FILTER" ] && [[ "projection_metric/locale-independent" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local out bad=""
    out="$(inflation_guard pt_BR.UTF-8 1 1 2>&1)"
    if ! grep -q 'Result: PASS' <<< "$out"; then
        bad="a 100% realization rate is rejected under a comma-decimal locale"
    fi
    if grep -qE '[0-9],[0-9]' <<< "$out"; then
        bad="${bad:+$bad; }output carries a comma decimal separator"
    fi
    out="$(inflation_guard pt_BR.UTF-8 200 70 2>&1)"
    if ! grep -q 'Result: FAIL' <<< "$out"; then
        bad="${bad:+$bad; }a 35% realization rate is accepted"
    fi
    if [ -n "$bad" ]; then
        printf '%-34s %-8s %s\n' "projection_metric/locale-independent" "accept" "$bad"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "projection_metric/locale-independent" "accept" \
            "ok (verdict identical under a comma-decimal locale)"
    fi
    PASS=$((PASS + 1))
}

case_status_vh() {
    write_defect() { write_status_vh_defect; }
    run_case "status_vh/out-of-sync" status_vh_guard "OUT_OF_SYNC_STATUS_OLDER"
}

# The same heading shape, seen from the other guard.
#
# The alternation is written in three places and the sign-before-number form
# was missing from all three, so both guards were blind to the same files and
# neither could report the other's blindness. This case pins the no-VH guard's
# copy: an Accepted RFC whose Version History heading carries a leading section
# sign before its number must not be reported as having none.
#
# The clean phase is the assertion. With the three-shape alternation it fails,
# because the guard emits MISSING_VH for a file that has a populated Version
# History. The defect phase removes the section entirely, so the case also
# proves it is not passing vacuously.
write_no_vh_signed_heading() {
    cat >"$FIX/rfcs/0005-signed-heading.md" <<'EOF'
---
status: accepted
---

# RFC-0005 Signed Heading

## Status

Accepted — promoted 2026-02-01

## §6 Version History

| Version | Date | Changes |
| --- | --- | --- |
| 1.0.0 | 2026-02-01 | Initial |
EOF
}

write_no_vh_heading_absent() {
    cat >"$FIX/rfcs/0005-signed-heading.md" <<'EOF'
---
status: accepted
---

# RFC-0005 Signed Heading

## Status

Accepted — promoted 2026-02-01

## Rationale

This RFC has no Version History section at all.
EOF
}

# Every file the guard reads must land in exactly one group.
#
# The non-Accepted figure used to be derived as a remainder of the scanned
# count, which reconciles by construction and therefore cannot detect anything:
# a file skipped by a path that increments nothing is absorbed into it and
# reported as an ordinary non-Accepted Draft. It is counted directly now, and
# this is the assertion that makes the counted figure worth having.
#
# The fixture holds one Accepted RFC and one Draft, so the two groups differ
# and the arithmetic has something to distinguish. A fixture of only Accepted
# files would reconcile under a guard that never counted Drafts at all.
case_no_vh_reconciles() {
    if [ -n "$FILTER" ] && [[ "no_vh_accepted/population-reconciles" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-no-vh-recon"
    rm -rf "$own"
    mkdir -p "$own/rfcs"
    FIX="$own"
    write_base_fixture
    cat >"$FIX/rfcs/0006-draft.md" <<'EOF'
---
status: draft
---

# RFC-0006 Draft

## Status

Draft

## Body

A Draft RFC needs no Version History, and this file is here so the fixture has
a file in the group the guard must count rather than skip silently.
EOF
    local out scanned sk ch na
    out="$(no_vh_guard 2>&1)"
    scanned="$(grep -oE 'SCANNED=[0-9]+' <<< "$out" | head -1 | grep -oE '[0-9]+' || true)"
    sk="$(grep -oE 'SKIPPED_BY_NAME=[0-9]+' <<< "$out" | head -1 | grep -oE '[0-9]+' || true)"
    ch="$(grep -oE 'CHECKED=[0-9]+' <<< "$out" | head -1 | grep -oE '[0-9]+' || true)"
    na="$(grep -oE 'NOT_ACCEPTED=[0-9]+' <<< "$out" | head -1 | grep -oE '[0-9]+' || true)"
    if [ -z "$na" ] || [ -z "$sk" ] || [ -z "$ch" ] || [ -z "$scanned" ]; then
        printf '%-34s %-8s %s\n' "no_vh_accepted/population-reconciles" "accept" \
            "no group counters reported — the population cannot be checked"
        FAIL=$((FAIL + 1))
    elif [ "$na" -lt 1 ]; then
        printf '%-34s %-8s %s\n' "no_vh_accepted/population-reconciles" "accept" \
            "NOT_ACCEPTED=0 while the fixture holds a Draft — a remainder, not a count"
        FAIL=$((FAIL + 1))
    elif [ $((sk + ch + na)) -ne "$scanned" ]; then
        printf '%-34s %-8s %s\n' "no_vh_accepted/population-reconciles" "accept" \
            "$sk + $ch + $na = $((sk + ch + na)) against SCANNED=$scanned"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "no_vh_accepted/population-reconciles" "accept" \
            "ok ($sk + $ch + $na = SCANNED=$scanned)"
    fi
    PASS=$((PASS + 1))
}

case_no_vh_signed_heading() {
    prepare_case() { write_no_vh_signed_heading; }
    write_defect() { write_no_vh_heading_absent; }
    run_case "no_vh_accepted/signed-numbered-heading" no_vh_guard "MISSING_VH"
}

# Every heading shape the corpus uses, in one fixture, asserted against BOTH
# guards that carry a copy of the alternation.
#
# The per-shape cases above pin the shape that regressed. This one is the
# general form: it does not care which shape a future heading takes, only that
# a corpus using all four is read by both guards. A shape added to one copy and
# not the other shows up here, which is the disagreement the duplicated pattern
# has no other way to reveal.
write_vh_all_shapes() {
    local n=0 h
    for h in '## Version History' '## 6. Version History' '## §Version History' '## §6 Version History'; do
        n=$((n + 1))
        cat >"$FIX/rfcs/00$((n + 10))-shape.md" <<EOF
---
status: accepted
---

# RFC-00$((n + 10)) Shape

## Status

Accepted (v1.0.0) — promoted 2026-02-01

$h

| Version | Date | Changes |
| --- | --- | --- |
| 1.0.0 | 2026-02-01 | Initial |
EOF
    done
}

case_vh_heading_all_shapes() {
    if [ -n "$FILTER" ] && [[ "vh_heading/all-four-shapes" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-vh-all-shapes"
    rm -rf "$own"
    mkdir -p "$own/rfcs"
    FIX="$own"
    write_vh_all_shapes
    local novh svh bad=""
    novh="$(no_vh_guard 2>&1)"
    if grep -q 'MISSING_VH: ' <<< "$novh"; then
        bad="no-VH guard reported a populated Version History as missing"
    fi
    svh="$(status_vh_guard 2>&1)"
    if grep -q 'UNCLASSIFIED: ' <<< "$svh"; then
        bad="${bad:+$bad; }status guard could not place a file using a known heading shape"
    elif ! grep -qE 'CHECKED=4 ' <<< "$svh"; then
        bad="${bad:+$bad; }status guard did not compare all four"
    fi
    if [ -n "$bad" ]; then
        printf '%-34s %-8s %s\n' "vh_heading/all-four-shapes" "accept" "$bad"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "vh_heading/all-four-shapes" "accept" \
            "ok (all four shapes read by both guards)"
    fi
    PASS=$((PASS + 1))
}

case_no_vh() {
    write_defect() { write_no_vh_defect; }
    run_case "no_vh_accepted/missing-vh" no_vh_guard "MISSING_VH"
}

# A Version History is not always a table. Six Accepted RFCs write it as a
# bullet list, and the row reader only understood `| v1.0 |`, so it reported
# those documents as having no data rows at all. This case is a clean-only
# assertion: there is no defect to plant, because the document is correct and
# the guard must not object to it. Requiring one rendering is measuring the
# renderer rather than the document.
write_bullet_vh() {
    cat >"$FIX/rfcs/0002-bullets.md" <<'EOF'
---
status: accepted
---

# RFC-0002 Bullets

## Status

Accepted (v3.0) — promoted 2026-02-01

## Version History

- **v3** (2026-02-01): Third round.
- **v2** (2026-01-01): Second round.
EOF
}

# The population counters must describe what was COMPARED, not what was found.
# This is an assertion on the guard's own output rather than on a planted
# defect, because the defect this catches is a counter that reports success.
#
# `lint_status_vh_sync.sh` used to increment CHECKED once per markdown file, so
# its summary read CHECKED=247 against 74 files actually compared. A fixture
# where one file is comparable and one is not pins the relationship.
write_population_mix() {
    cat >"$FIX/rfcs/0003-comparable.md" <<'EOF'
# RFC-0003 Comparable

## Status

Accepted (v1.0.0)

## Version History

| 1.0.0 | x |
EOF
    cat >"$FIX/rfcs/0004-uncomparable.md" <<'EOF'
# RFC-0004 Uncomparable

## Status

Draft

## Body

No version is declared in this Status block, so there is nothing to compare.
EOF
}

case_population_coverage() {
    if [ -n "$FILTER" ] && [[ "status_vh/population-coverage" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-population"
    rm -rf "$own"
    mkdir -p "$own/rfcs"
    FIX="$own"
    write_base_fixture
    write_population_mix
    # Must report a coverage figure, and it must be a PLAUSIBLE one.
    #
    # The first version of this assertion forbade the literal string
    # "COVERAGE=100%". That passed against the regressed counter, because a
    # counter that inflates CHECKED past SCANNED never produces 100% — it
    # produces 150%. An assertion written against a value the broken code cannot
    # produce detects nothing.
    #
    # So the assertion is numeric: coverage must not exceed 100, and must be
    # under 100 here because one of the two files carries no Status version and
    # so nothing about it was compared.
    local out cov
    out="$(status_vh_guard 2>&1)"
    cov="$(grep -oE 'COVERAGE=[0-9]+%' <<< "$out" | grep -oE '[0-9]+' || echo "")"
    if [ -z "$cov" ]; then
        printf '%-34s %-8s %s\n' "status_vh/population-coverage" "accept" "no COVERAGE figure reported"
        FAIL=$((FAIL + 1))
    elif [ "$cov" -gt 100 ]; then
        printf '%-34s %-8s %s\n' "status_vh/population-coverage" "accept" \
            "COVERAGE=${cov}% exceeds the files scanned — the counter is not describing the population"
        FAIL=$((FAIL + 1))
    elif [ "$cov" -ge 100 ]; then
        printf '%-34s %-8s %s\n' "status_vh/population-coverage" "accept" \
            "claimed 100% coverage of a corpus with an uncomparable file"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "status_vh/population-coverage" "accept" "ok (COVERAGE=${cov}%)"
    fi
    PASS=$((PASS + 1))
}

case_no_vh_bullets() {
    prepare_case() { write_bullet_vh; }
    run_case "no_vh_accepted/bullet-history" no_vh_guard "MISSING_VH"
}

# Guard 7 reads YAML `version:` against the latest Version History row. It
# honours PLAN_ROOT, so it is fixture-driven like the others.
plan_guard() {
    PLAN_ROOT="$FIX/plans" bash scripts/lint_plan_yaml_vh_sync.sh
}

write_plan_drift() {
    cat >"$FIX/plans/q-drift.md" <<'EOF'
---
version: 1.0
---

# Plan Q

## Version History

| Version | Date |
| --- | --- |
| 1.0 | a |
| 3.0.0 | c |
EOF
}

# The in-sync plan carries an ALIGNED table and its `version:` equals its
# newest row. That is the shape the old reader got wrong twice: it took the
# first (oldest) row, and it required exactly one space around the version, so
# an aligned table matched nothing at all.
write_plan_clean() {
    cat >"$FIX/plans/p-sync.md" <<'EOF'
---
version: 2.0
---

# Plan P

## Version History

| Version | Date     |
| ------- | -------- |
| 1.0     | a        |
| 2.0     | b        |
EOF
}

# The three no-version buckets must be distinguishable.
#
# NO_VH and NO_YAML were declared in the plan guard, never incremented, and
# never printed: both of their branches incremented the combined
# NO_VH_NO_YAML counter instead, so a plan with no Version History and a plan
# with no YAML version were reported under one name. Two diagnostics, one
# number, and a reader with no way to tell which condition was found.
#
# Detect-direction, because the defect was a name that lied rather than a
# finding that went missing. The first plan is long enough not to be excused as
# a short plan, which is the other way its finding could be absorbed.
write_plan_yaml_no_vh() {
    {
        printf -- '---\nversion: 1.0\n---\n\n# Plan R\n\n## Body\n\n'
        for i in $(seq 1 60); do
            printf 'Filler line %s, long enough to exceed the short-plan threshold.\n' "$i"
        done
    } >"$FIX/plans/r-yaml-no-vh.md"
}

write_plan_vh_no_yaml() {
    cat >"$FIX/plans/s-vh-no-yaml.md" <<'EOF'
# Plan S

## Version History

| Version | Date |
| --- | --- |
| 2.0 | a |
EOF
}

case_plan_buckets() {
    if [ -n "$FILTER" ] && [[ "plan_yaml/distinct-no-version-buckets" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-plan-buckets"
    rm -rf "$own"
    mkdir -p "$own/plans"
    FIX="$own"
    write_plan_yaml_no_vh
    write_plan_vh_no_yaml
    local out nv ny nvn bad=""
    out="$(plan_guard 2>&1)"
    nv="$(grep -oE '^ +NO_VH=[0-9]+' <<< "$out" | grep -oE '[0-9]+' || true)"
    ny="$(grep -oE '^ +NO_YAML=[0-9]+' <<< "$out" | grep -oE '[0-9]+' || true)"
    nvn="$(grep -oE '^ +NO_VH_NO_YAML=[0-9]+' <<< "$out" | grep -oE '[0-9]+' || true)"
    if [ -z "$nv" ] || [ -z "$ny" ]; then
        bad="NO_VH and NO_YAML are not reported, so the buckets cannot be told apart"
    elif [ "$nv" -ne 1 ]; then
        bad="NO_VH=$nv where one plan has a YAML version and no Version History"
    elif [ "$ny" -ne 1 ]; then
        bad="NO_YAML=$ny where one plan has a Version History and no YAML version"
    elif [ "$nvn" -ne 0 ]; then
        bad="NO_VH_NO_YAML=$nvn where neither plan lacks both — the combined bucket absorbed them"
    fi
    if [ -n "$bad" ]; then
        printf '%-34s %-8s %s\n' "plan_yaml/distinct-no-version-buckets" "detect" "$bad"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "plan_yaml/distinct-no-version-buckets" "detect" \
            "ok (NO_VH=1 NO_YAML=1 NO_VH_NO_YAML=0)"
    fi
    PASS=$((PASS + 1))
}

case_plan_yaml() {
    prepare_case() {
        mkdir -p "$FIX/plans"
        write_plan_clean
    }
    write_defect() { write_plan_drift; }
    run_case "plan_yaml/version-drift" plan_guard "OUT_OF_SYNC_YAML_OLDER"
}

case_cites() {
    # The cites corpus is not part of the base fixture, because it is the only
    # case that needs `docs/` to exist as a separate root. Writing the clean
    # pair here (correct cite only) and letting write_defect add the dangling
    # one keeps the clean assertion meaningful.
    write_defect() {
        write_cites_defect
    }
    prepare_case() { prepare_cites_clean; }
    run_case "validate_cites/dangling-section" cites_guard "INVALID [section missing]"
}

# The clean cites corpus: a correct multi-word citation. A pass on this also
# proves the section name is not truncated at the first space, which it used to
# be — `§Adversary` was looked up against a `## Adversary Analysis` heading,
# reported INVALID, and told a reader to fix a citation that was already right.
prepare_cites_clean() {
    cat >"$FIX/rfcs/0004-target.md" <<'EOF'
# RFC-0004 Target

## Adversary Analysis

The attacker must hold stake.
EOF
    cat >"$FIX/docs/cites.md" <<'EOF'
See RFC-0004 §Adversary Analysis for the threat model.
EOF
}

# The clean frontmatter corpus: one document whose YAML parses.
prepare_frontmatter_clean() {
    printf -- '---\nname: fixture\nv: "1.0"\n---\n\n# Parses\n' >"$FIX/docs/fm-good.md"
}

# An unquoted value carrying `: `, which is the single most common unparseable
# frontmatter shape in this corpus (27 of the 33 blocks repaired under
# `thirty-three-mission-and-rfc-frontmatter-blocks-are-unparseable-yaml`).
write_frontmatter_defect() {
    printf -- '---\nname: broken: value\n---\n\n# Does not parse\n' >"$FIX/docs/fm-bad.md"
}

# Every document this guard reads must land in exactly one of two groups.
#
# A document with no frontmatter block is legitimately outside the guard's
# subject and needs no finding, but it is still a document that was read. The
# summary printed the scanned count beside the with-frontmatter count and left
# the reader to subtract: on this repository that is 1933 against 255, and the
# two are not supposed to match because most RFCs carry no frontmatter at all.
# Nothing said so, so the gap read as work the guard never reached.
#
# The fixture holds one document with frontmatter and one without, so the two
# groups differ and the arithmetic has something to distinguish. A fixture of
# only frontmatter documents would reconcile under a guard that never counted
# the rest at all.
case_frontmatter_reconciles() {
    if [ -n "$FILTER" ] && [[ "frontmatter/population-reconciles" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-frontmatter-recon"
    rm -rf "$own"
    mkdir -p "$own/docs"
    FIX="$own"
    cat >"$own/docs/0001-with.md" <<'EOF'
---
status: draft
---

# With frontmatter

Body.
EOF
    cat >"$own/docs/0002-without.md" <<'EOF'
# Without frontmatter

Body, with no leading block at all.
EOF
    local out scanned with_ without
    out="$(frontmatter_guard 2>&1)"
    scanned="$(grep -oE '^SCANNED +[0-9]+' <<< "$out" | grep -oE '[0-9]+' || true)"
    with_="$(grep -oE '^WITH_FRONTMATTER +[0-9]+' <<< "$out" | grep -oE '[0-9]+' || true)"
    without="$(grep -oE '^NO_FRONTMATTER +[0-9]+' <<< "$out" | grep -oE '[0-9]+' || true)"
    if [ -z "$without" ] || [ -z "$with_" ] || [ -z "$scanned" ]; then
        printf '%-34s %-8s %s\n' "frontmatter/population-reconciles" "accept" \
            "no group counters reported — the population cannot be checked"
        FAIL=$((FAIL + 1))
    elif [ "$without" -lt 1 ]; then
        printf '%-34s %-8s %s\n' "frontmatter/population-reconciles" "accept" \
            "NO_FRONTMATTER=0 while the fixture holds a document without frontmatter"
        FAIL=$((FAIL + 1))
    elif [ $((with_ + without)) -ne "$scanned" ]; then
        printf '%-34s %-8s %s\n' "frontmatter/population-reconciles" "accept" \
            "$with_ + $without = $((with_ + without)) against SCANNED=$scanned"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "frontmatter/population-reconciles" "accept" \
            "ok ($with_ + $without = SCANNED=$scanned)"
    fi
    PASS=$((PASS + 1))
}

case_frontmatter() {
    prepare_case() { prepare_frontmatter_clean; }
    write_defect() { write_frontmatter_defect; }
    run_case "frontmatter/unquoted-colon" frontmatter_guard \
        "unquoted value containing"
}

# The clean anchor corpus: one heading, and one cite that resolves to it.
prepare_anchors_clean() {
    cat >"$FIX/rfcs/0001-anchor.md" <<'EOF'
# RFC-0001 Anchor

## Identity

Body text.

See §Identity for a citation that resolves.
EOF
}

write_anchors_defect() {
    printf '\nSee §Invented Anchor for a citation that cannot resolve.\n' \
        >>"$FIX/rfcs/0001-anchor.md"
}

# A baseline is a population the reader cannot otherwise see, and a linter that
# prints nothing about it looks identical to a linter with no baseline at all.
#
# Both of these report their unresolved counts and then, on a healthy run, say
# only "no new findings". The anchors linter printed a coverage figure but
# guarded the stale count behind a non-zero test, and the type-refs linter
# printed no coverage figure whatsoever — so the case that matters, an
# abandoned or drifting baseline, is the one both stayed silent about.
#
# The fixture uses an EMPTY baseline over a corpus with one real finding, so
# every count is zero-or-one and the stale count is genuinely zero. That is the
# state the old conditionals swallowed.
case_baseline_coverage() {
    if [ -n "$FILTER" ] && [[ "baselines/report-coverage" != *"$FILTER"* ]]; then
        SKIP=$((SKIP + 1))
        return 0
    fi
    local own="$WORK/case-baseline-coverage"
    rm -rf "$own"
    mkdir -p "$own/rfcs"
    FIX="$own"
    : >"$FIX/baseline.txt"
    cat >"$FIX/rfcs/0001-debt.md" <<'EOF'
# RFC-0001 Debt

## Body

See §Invented Anchor for a citation that resolves to nothing here.

```rust
let x: TotallyUndeclaredType = y;
```
EOF
    local a t bad=""
    a="$(anchors_guard 2>&1)"
    t="$(types_guard 2>&1)"
    if ! grep -q 'stale baseline entries: 0' <<< "$a"; then
        bad="anchors linter omits the stale count when it is zero"
    fi
    if ! grep -q 'stale baseline entries: 0' <<< "$t"; then
        bad="${bad:+$bad; }type-refs linter reports no baseline coverage at all"
    elif ! grep -qE 'baselined: [0-9]+ of [0-9]+ finding' <<< "$t"; then
        bad="${bad:+$bad; }type-refs linter gives no baselined-of-total figure"
    fi
    if [ -n "$bad" ]; then
        printf '%-34s %-8s %s\n' "baselines/report-coverage" "accept" "$bad"
        FAIL=$((FAIL + 1))
    else
        printf '%-34s %-8s %s\n' "baselines/report-coverage" "accept" \
            "ok (both linters state coverage and staleness at zero)"
    fi
    PASS=$((PASS + 1))
}

case_anchors() {
    prepare_case() { prepare_anchors_clean; }
    write_defect() { write_anchors_defect; }
    run_case "rfc_anchors/dangling-anchor" anchors_guard "invented anchor"
}

prepare_types_clean() {
    cat >"$FIX/rfcs/0001-type.md" <<'EOF'
# RFC-0001 Type

## Identity

```rust
pub struct FixtureType { x: u64 }
```

References FixtureType, which is declared above in this same file.
EOF
}

write_types_defect() {
    cat >"$FIX/rfcs/0001-type.md" <<'EOF'
# RFC-0001 Type

```rust
let x: TotallyMadeUpTypeName = y;
```
EOF
}

case_type_refs() {
    prepare_case() { prepare_types_clean; }
    write_defect() { write_types_defect; }
    run_case "rfc_type_refs/undeclared-type" types_guard "TotallyMadeUpTypeName"
}

# The clean type-refs corpus must genuinely resolve. The first attempt used a
# type name invented for the fixture, which the linter correctly reported as
# undeclared — so the "clean" case was testing a guard that was right. A clean
# fixture has to declare the type it references, or the case proves nothing.
types_guard() {
    python3 scripts/lint-rfc-type-refs.py --root "$FIX" --baseline "$FIX/baseline.txt"
}

run_case_fn case_section_cites
run_case_fn case_section_cites_overdeep
run_case_fn case_section_cites_quoted
run_case_fn case_section_cites_coverage
run_case_fn case_status_vh
run_case_fn case_status_vh_signed_heading
run_case_fn case_status_vh_reconciles
run_case_fn case_no_vh
run_case_fn case_no_vh_signed_heading
run_case_fn case_vh_heading_all_shapes
run_case_fn case_no_vh_bullets
run_case_fn case_no_vh_population
run_case_fn case_no_vh_reconciles
run_case_fn case_population_coverage
run_case_fn case_plan_yaml
run_case_fn case_plan_buckets
run_case_fn case_inflation_locale
run_case_fn case_cross_rfc_corpus
run_case_fn case_cites
run_case_fn case_frontmatter
run_case_fn case_frontmatter_reconciles
run_case_fn case_anchors
run_case_fn case_baseline_coverage
run_case_fn case_type_refs

# The inventory must describe the run that produced this result.
#
# One id per executed case is the invariant. A case that runs without naming
# itself, or names itself without running, leaves the two counts apart, and
# the apart-ness is invisible in the output — a reported total beside a
# narrower population, in the tool whose entire purpose is to catch that.
#
# Counted before the summary prints, so a mismatch is visible in the same
# block as the pass and fail counts it contradicts.
_id_count=$(case_ids | wc -l | tr -d ' ')
# Counted as they run, by the wrapper, rather than by grepping the run list for
# bare `case_*` lines. The wrapper replaced those lines, so the grep found none
# and reported a mismatch on the first run — which is the invariant working, on
# a count that described the file's former shape rather than the run.
_run_count=$CASE_FUNCTIONS
if [ "$_id_count" -ne "$_run_count" ]; then
    # Deliberately NOT printed in the `guard/case  phase  result` shape used by
    # cases. A case id is a `guard/case` literal, and `case_ids` counts those —
    # so naming this violation `harness/self-inventory` would add a fifteenth
    # id that no `case_*` function ever runs, and the invariant would report
    # itself as its own first violation. It is an assertion about the harness,
    # not a case in it.
    printf '%s\n' "INVARIANT FAIL: $_run_count case(s) executed but $_id_count named"
    printf '%s\n' "  --help lists $_id_count cases; the listing does not describe the run"
    FAIL=$((FAIL + 1))
fi

# And every case that ran must have said something. Counted against the number
# of functions actually invoked, which the wrapper counts as it goes rather
# than by grepping the file — so a case removed from the run list cannot leave
# this satisfied by a count that no longer describes the run.
if [ "$CASE_UNREPORTED" -gt 0 ]; then
    printf '%s\n' "INVARIANT FAIL: $CASE_UNREPORTED of $CASE_FUNCTIONS executed case(s) reported nothing"
    printf '%s\n' "  a case that counts neither pass nor fail is not a case that passed"
    FAIL=$((FAIL + 1))
fi

printf '%s\n' "------------------------------------------------------------"
printf 'passed=%d failed=%d skipped=%d\n' "$PASS" "$FAIL" "$SKIP"

if [ "$FAIL" -ne 0 ]; then
    exit 1
fi

if [ -n "$FILTER" ] && [ "$PASS" -eq 0 ] && [ "$FAIL" -eq 0 ]; then
    echo ""
    echo "FAIL: filter '$FILTER' matched no guard."
    echo "This run verified nothing. Run with no argument to list the cases."
    exit 1
fi

if [ "$PASS" -eq 0 ] && [ "$FAIL" -eq 0 ]; then
    echo ""
    echo "FAIL: no cases were defined."
    exit 1
fi

if [ "$SKIP" -gt 0 ]; then
    echo ""
    echo "NOTE: $SKIP case(s) skipped by filter '$FILTER'."
fi

exit 0