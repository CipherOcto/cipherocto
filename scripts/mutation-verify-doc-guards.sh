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

# -h/--help lists the cases instead of being treated as a name filter.
# `mutation-verify-guards.sh` had this defect: any argument was a substring
# filter, so `--help` matched nothing and the run reported a successful
# verification of zero guards. An unmatched filter now fails loudly; `--help`
# is the one argument that is meant to match nothing.
if [ "$FILTER" = "-h" ] || [ "$FILTER" = "--help" ]; then
    echo "Usage: scripts/mutation-verify-doc-guards.sh [case-name-substring]"
    echo ""
    echo "Cases:"
    grep -oE 'run_case "[a-z_]+/[a-z-]+"' "${BASH_SOURCE[0]}" \
        | sed -E 's/run_case "//; s/"$//' | sed 's/^/  /'
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

case_section_cites_overdeep() {
    prepare_case() { :; }
    write_defect() { write_section_cites_overdeep_defect; }
    run_case "section_cites/over-deep" section_cites_guard "REJECT [over-deep]"
}

case_status_vh() {
    write_defect() { write_status_vh_defect; }
    run_case "status_vh/out-of-sync" status_vh_guard "OUT_OF_SYNC_STATUS_OLDER"
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

case_no_vh_bullets() {
    prepare_case() { write_bullet_vh; }
    run_case "no_vh_accepted/bullet-history" no_vh_guard "MISSING_VH"
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

case_section_cites
case_section_cites_overdeep
case_status_vh
case_no_vh
case_no_vh_bullets
case_cites
case_frontmatter
case_anchors
case_type_refs

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