#!/usr/bin/env bash
# Mutation-verify every guide/harness guard added by the operator-guide audit
# remediation (audit §8, plan §B, plan line ~142).
#
# WHY THIS IS A SCRIPT AND NOT A PROSE CLAIM
# ------------------------------------------
# The plan requires every guard to be mutation-verified: "introduce the defect
# it hunts, watch the guard go red, restore, watch it go green." Performing
# that by hand produces a *process claim* — a statement about what someone did
# once — which is not checkable by anyone else afterwards. This script makes
# the same claim re-runnable, so a reviewer can execute the discipline rather
# than trust the report of it.
#
# For each guard it:
#   1. backs up every file it is about to touch
#   2. plants a defect the guard is supposed to catch
#   3. asserts the guard goes RED (and that the failure names the right thing)
#   4. restores from backup
#   5. asserts the guard goes GREEN again
#
# If a mutation does not turn its guard red, that guard is a guard that cannot
# fail. That is the whole point: this script exists to be able to fail.
#
# SAFETY: every touched file is copied to a temp dir before mutation and
# restored by an EXIT trap, so an interrupted run (Ctrl-C, timeout, a test
# that hangs) cannot leave the worktree with a planted defect in it. The
# working tree is restored even on SIGKILL-adjacent failure of the test
# process itself, because restoration is driven by the shell trap rather than
# by the test.
#
# USAGE:
#   scripts/mutation-verify-guards.sh              # all guards
#   scripts/mutation-verify-guards.sh build_output # substring filter
#   VERIFY_GREEN=0 scripts/mutation-verify-guards.sh   # skip the green pass
#
# EXIT: 0 only if every mutation turned its guard red AND every restore left
# the guard green.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 1

GUIDE="docs/06-operations/operator-guide.md"
NIGHTLY=".github/workflows/multi-node-nightly.yml"
CI=".github/workflows/ci.yml"

BACKUP_DIR="$(mktemp -d)"
FILTER="${1:-}"
VERBOSE="${VERBOSE:-0}"
VERIFY_GREEN="${VERIFY_GREEN:-1}"


# Files the mutations may touch. Anything a mutation writes outside this list
# is a bug in the mutation, not an accident to be cleaned up later.
WATCHED=("$GUIDE" "$NIGHTLY" "$CI")

restore() {
  local f
  for f in "${WATCHED[@]}"; do
    if [ -f "$BACKUP_DIR/$(echo "$f" | tr '/' '_')" ]; then
      cp -p "$BACKUP_DIR/$(echo "$f" | tr '/' '_')" "$f"
    fi
  done
}

cleanup() {
  restore
  rm -rf "$BACKUP_DIR"
}
trap cleanup EXIT INT TERM

backup() {
  local f
  for f in "${WATCHED[@]}"; do
    cp -p "$f" "$BACKUP_DIR/$(echo "$f" | tr '/' '_')"
  done
}

# run_guard <test-binary-selector> <test-name>
# Echoes combined output; returns cargo's exit status.
run_guard() {
  local suite="$1" name="$2"
  cargo test -q -p octo-cli --test "$suite" -- --exact "$name" 2>&1
}

# ---------------------------------------------------------------------------
# mutation table
# ---------------------------------------------------------------------------
# Each entry: NAME | SUITE | TEST | PYTHON_MUTATION
# PYTHON_MUTATION mutates files in place; it must exit non-zero if the anchor
# text it needs to change was not found, because a mutation that silently
# changes nothing would "pass" the red check vacuously.

MUTATIONS=(
# R12, NOW FIXED. This row was a KNOWN_GAP asserting the guard stayed green
# when `.payload.` became `.metadata.` — a real wrong-layer read that the
# guard named for layers did not catch, because `metadata` is a genuine
# payload key (network.rs declares `pub metadata`) and a two-hop chain falls
# below the three-hop threshold of the nesting check.
#
# The guard now asserts each filter's ROOT is `payload` (or a NON_OCTO key, or
# the filter has no chains at all), with an anti-vacuity floor so the check
# cannot pass on an extractor regression. Back to expecting RED.
"jq_field_reads|guide_examples|guide_jq_field_reads_resolve_against_the_payload_layer|$(cat <<'PY'
import sys
p = "docs/06-operations/operator-guide.md"
s = open(p, encoding="utf-8").read()
old, new = ".payload.", ".metadata."
if old not in s:
    sys.exit("anchor .payload. not found")
open(p, "w", encoding="utf-8").write(s.replace(old, new, 3))
PY
)"

"absence_claims|guide_examples|guide_absence_claims_name_something_actually_absent|$(cat <<'PY'
import sys
p = "docs/06-operations/operator-guide.md"
s = open(p, encoding="utf-8").read()
# A `[SUBSTRATE-NEW] ... is NOT wired` claim pointed at a command that IS
# wired. `octo identity list` resolves in the command tree, so claiming it
# is unwired is exactly the false absence claim the guard hunts.
old = "`octo network peers add` is NOT wired"
if old not in s:
    sys.exit("anchor SUBSTRATE-NEW claim not found")
open(p, "w", encoding="utf-8").write(s.replace(old, "`octo identity list` is NOT wired", 1))
PY
)"

"env_var_readers|guide_examples|guide_documented_env_vars_have_readers|$(cat <<'PY'
import sys
p = "docs/06-operations/operator-guide.md"
s = open(p, encoding="utf-8").read()
# Document an env var nothing reads. Prepended to the flags appendix.
marker = "## Appendix A — Operator flags reference"
if marker not in s:
    sys.exit("appendix marker not found")
plant = "```bash\nexport OCTO_TOTALLY_FAKE_VAR_FOR_MUTATION=1\n```\n\n"
open(p, "w", encoding="utf-8").write(s.replace(marker, plant + marker, 1))
PY
)"

"non_octo_invocations|guide_examples|guide_non_octo_invocations_resolve|$(cat <<'PY'
import sys
p = "docs/06-operations/operator-guide.md"
s = open(p, encoding="utf-8").read()
# A column-0 invocation of a binary no crate declares.
#
# NOT a mutation of the `export QUOTA_ROUTER_BIN=...` line: the guard skips
# `$`-prefixed commands outright ("nothing static to resolve"), so that shape
# is structurally invisible here. It is held by
# guide_build_output_paths_are_real_binaries instead — mutating it here would
# have produced a false "guard cannot fail".
marker = "```bash"
if marker not in s:
    sys.exit("no bash fence found")
plant = marker + "\nocto-not-a-declared-bin do-the-thing\n```\n"
open(p, "w", encoding="utf-8").write(s.replace(marker, plant, 1))
PY
)"

"unit_dispositions|guide_examples|guide_unit_dispositions_are_complete|$(cat <<'PY'
import sys
p = "docs/06-operations/operator-guide.md"
s = open(p, encoding="utf-8").read()
# A disposition naming a test that does not exist.
old = "`tv_c3_every_multi_node_target_is_scheduled`"
if old not in s:
    sys.exit("anchor disposition cell not found")
open(p, "w", encoding="utf-8").write(s.replace(old, "`tv_c3_test_that_does_not_exist`", 1))
PY
)"

"jq_runtime|guide_examples|guide_jq_filters_run_against_real_envelopes|$(cat <<'PY'
import sys
p = "docs/06-operations/operator-guide.md"
s = open(p, encoding="utf-8").read()
# The R6 class: a filter that compiles, every field it reads exists, and it
# still fails at runtime. Indexing the NUMBER `.payload.count_returned` is
# exactly that — jq rejects it with "Cannot index number with number" (exit 5).
#
# Two earlier attempts at this mutation were vacuous, and both are worth
# recording because they are easy to repeat:
#   * mutating inside a `[]` iteration — a freshly registered wallet has an
#     empty agents list, so the body never runs;
#   * reading a missing field (`.payload.nope_missing[0]`) — jq is LENIENT
#     here and returns null for an index of null, exit 0. A field that does
#     not exist is not a runtime error in jq.
# A number index is the shape that actually fails.
old = "jq '.payload.count_returned'"
if old not in s:
    sys.exit("anchor .payload.count_returned not found")
open(p, "w", encoding="utf-8").write(s.replace(old, "jq '.payload.count_returned[0]'", 1))
PY
)"

"build_outputs|guide_examples|guide_build_output_paths_are_real_binaries|$(cat <<'PY'
import sys
p = "docs/06-operations/operator-guide.md"
s = open(p, encoding="utf-8").read()
# The exact R7 defect: name the package instead of the bin.
old = "target/release/quota-router"
if old not in s:
    sys.exit("anchor not found")
open(p, "w", encoding="utf-8").write(s.replace(old, "target/release/quota-router-cli", 1))
PY
)"

"multi_node_scheduled|harness_invariants|tv_c3_every_multi_node_target_is_scheduled|$(cat <<'PY'
import sys
p = ".github/workflows/multi-node-nightly.yml"
s = open(p, encoding="utf-8").read()
# Replace the whole token. Appending a suffix would leave the original name
# as a substring of the mutated one, and the guard matches by substring — so
# the mutation would have "passed" vacuously.
old = "--test e2e_l4_multinode_docker"
if old not in s:
    sys.exit("anchor target not found")
open(p, "w", encoding="utf-8").write(s.replace(old, "--test MUTATED-away", 1))
PY
)"

# R13, NOW FIXED. This row was a KNOWN_GAP asserting the guard stayed green
# when the nightly's `schedule:` trigger was removed — which is exactly the
# invisibility C3 exists to prevent, reached through the guard meant to
# prevent it. The guard now asserts the trigger, so this mutation must turn
# it RED; the row is back to expecting a failure, which is what keeps a
# closed gap from silently reopening.
#
# The guard matches the key WITH its indentation ("\n  schedule:\n") rather
# than the bare word, because the workflow's header comment explains what the
# trigger is for and a substring match would be satisfied by that prose.
"workflow_triggers|harness_invariants|tv_c3_ci_and_nightly_workflows_declare_their_triggers|$(cat <<'PY'
import sys
p = ".github/workflows/multi-node-nightly.yml"
s = open(p, encoding="utf-8").read()
# Target the indented YAML key. The FIRST occurrence of `schedule:` in this
# file is inside a prose comment explaining what the trigger is for; mutating
# that changes nothing and the mutation would report a false "cannot fail".
old = "\n  schedule:\n"
if old not in s:
    sys.exit("indented schedule: key not found")
open(p, "w", encoding="utf-8").write(s.replace(old, "\n  schedul_MUTATED:\n", 1))
PY
)"

"disposition_agrees|harness_invariants|tv_c3_guide_disposition_table_agrees_with_the_harness|$(cat <<'PY'
import sys
p = "docs/06-operations/operator-guide.md"
s = open(p, encoding="utf-8").read()
# Re-point a covered row at a test that exists but proves nothing about it.
old = "`tv_c3_every_multi_node_target_is_scheduled`"
if old not in s:
    sys.exit("anchor not found")
open(p, "w", encoding="utf-8").write(s.replace(old, "`l3_teardown_removes_every_peer_idempotently`", 1))
PY
)"
)

# -h/--help prints usage and lists the available guards. Previously any
# argument was treated as a substring filter, so `--help` silently matched
# nothing and the script reported a successful run that verified nothing.
if [ "$FILTER" = "-h" ] || [ "$FILTER" = "--help" ]; then
  sed -n '31,37p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  echo ""
  echo "Available guards (use as a substring filter):"
  for entry in "${MUTATIONS[@]}"; do
    printf '  %s\n' "${entry%%|*}"
  done
  rm -rf "$BACKUP_DIR"
  exit 0
fi

# ---------------------------------------------------------------------------
# run
# ---------------------------------------------------------------------------

printf '%-24s %-8s %s\n' "GUARD" "PHASE" "RESULT"
printf '%s\n' "------------------------------------------------------------"

PASS=0
FAIL=0
SKIP=0

for entry in "${MUTATIONS[@]}"; do
  name="${entry%%|*}"
  rest="${entry#*|}"
  suite="${rest%%|*}"
  rest="${rest#*|}"
  test="${rest%%|*}"
  mutation="${rest#*|}"

  if [ -n "$FILTER" ] && [[ "$name" != *"$FILTER"* ]]; then
    SKIP=$((SKIP + 1))
    continue
  fi

  backup

  # --- phase 1: plant the defect, expect RED -----------------------------
  if ! printf '%s' "$mutation" | python3 - 2>/tmp/mutation-err.txt; then
    printf '%-24s %-8s %s\n' "$name" "plant" "MUTATION FAILED TO APPLY: $(cat /tmp/mutation-err.txt)"
    FAIL=$((FAIL + 1))
    restore
    continue
  fi

  out="$(run_guard "$suite" "$test")"
  guard_status=$?

  # A KNOWN_GAP row is inverted on purpose: the guard is EXPECTED to stay
  # green for a defect it genuinely misses. If it ever goes red, the gap has
  # been closed and this row must be reclassified — reporting that as a
  # failure stops a fixed guard from being mistaken for a broken script.
  if [[ "$name" == KNOWN_GAP.* ]]; then
    if [ $guard_status -eq 0 ]; then
      printf '%-24s %-8s %s\n' "$name" "gap" "ok (still green — gap open, as documented)"
    else
      printf '%-24s %-8s %s\n' "$name" "gap" "NOW FAILS — the gap is closed, reclassify this row"
      FAIL=$((FAIL + 1))
      restore
      continue
    fi
  else
    if [ $guard_status -eq 0 ]; then
      printf '%-24s %-8s %s\n' "$name" "red" "GUARD DID NOT FAIL — this guard cannot fail"
      FAIL=$((FAIL + 1))
      restore
      continue
    fi
    printf '%-24s %-8s %s\n' "$name" "red" "ok (guard failed as required)"
  fi

  # --- phase 2: restore, expect GREEN ------------------------------------
  restore
  if [ "$VERIFY_GREEN" = "1" ]; then
    if run_guard "$suite" "$test" >/dev/null 2>&1; then
      printf '%-24s %-8s %s\n' "$name" "green" "ok (restored)"
    else
      printf '%-24s %-8s %s\n' "$name" "green" "STILL RED AFTER RESTORE — restore is broken"
      FAIL=$((FAIL + 1))
      continue
    fi
  else
    printf '%-24s %-8s %s\n' "$name" "green" "skipped (VERIFY_GREEN=0)"
  fi

  PASS=$((PASS + 1))
done

printf '%s\n' "------------------------------------------------------------"
printf 'passed=%d failed=%d skipped=%d\n' "$PASS" "$FAIL" "$SKIP"

# A filter that matches no guard must not exit 0. `--help` used to be treated
# as a substring filter, matched nothing, and produced `passed=0 skipped=10`
# with a success exit: a script that verified nothing while reporting success.
# A typo in a guard name failed the same way, silently.
#
# SKIP is only benign when no filter was supplied, in which case skipped
# entries are individual guards excluded by design rather than by a mismatch.
if [ "$FAIL" -ne 0 ]; then
  exit 1
fi

if [ -n "$FILTER" ] && [ "$PASS" -eq 0 ] && [ "$FAIL" -eq 0 ]; then
  echo ""
  echo "FAIL: filter '$FILTER' matched no guard."
  echo "This run verified nothing. Check the guard name; available guards are"
  echo "listed by running this script with no argument."
  exit 1
fi

if [ "$PASS" -eq 0 ] && [ "$FAIL" -eq 0 ]; then
  echo ""
  echo "FAIL: no guards were defined."
  exit 1
fi

if [ "$SKIP" -gt 0 ]; then
  echo ""
  echo "NOTE: $SKIP guard(s) skipped by filter '$FILTER'."
fi

exit 0
