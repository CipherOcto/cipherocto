#!/usr/bin/env bash
# test-dangerous-tokens.sh — canonical test for the dangerous-token
# library. Run before any commit touching the prevention framework.
#
# IMPORTANT: this test data file uses SINGLE-QUOTED HEREDOC to avoid
# bash command substitution in the test inputs (the very bug the
# library prevents in commit messages). Edit with care.

set -uo pipefail
# shellcheck source=scripts/wrappers/dangerous-tokens.sh
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck disable=SC1090
source "$SCRIPT_DIR/wrappers/dangerous-tokens.sh"

PASS=0
FAIL=0

# Test data lives in a single-quoted heredoc so the literals are
# preserved byte-for-byte. NO expansion happens to the heredoc body.
read_test_data() {
  cat <<'TESTDATA'
CLEAN_PLAIN:fix: clean message body
CLEAN_RFC:fix: per RFC-0011-c §Roles and Auth.
CLEAN_PATH:fix: crates/octo-cli/src/commands
CLEAN_DASH:fix: -dash- and -dash-
CLEAN_SQUOTE:fix: use 'foo bar' as key
CLEAN_DQUOTE:fix: add "alpha" parameter
CLEAN_EQ:fix: set key=value pair
CLEAN_PAREN:fix: handle (parens) in arg

SEMI_BODY:fix: cleanup; remove old
SEMI_SPACE:fix: a ; b

DOLLAR_VAR:fix: $HOME is set
DOLLAR_UNDER:fix: $_ reserved
DOLLAR_POS:fix: arg $1
DOLLAR_QUEST:fix: exit $?
DOLLAR_HASH:fix: argc $#
DOLLAR_STAR:fix: args $*
DOLLAR_DOLLAR:fix: pid $$
DOLLAR_BRACE:fix: ${HOME}/bin

NOVERIFY:git commit --no-verify -m fix
NO_POST_HOOK:git push --no-post-hook
HOOKSPATH:git -c core.hooksPath=/tmp/evil commit -m x
CORE_EDITOR:git -c core.editor=vim -c :!bash commit -m x
ALIAS_COMMIT:git config --global alias.commit !bash

EVAL:eval git commit -m fix
XARGS:echo msg | xargs git commit -m
BASH_C:bash -c "git commit -m fix"
SH_C:sh -c 'git commit -m fix'
ENV:env git commit -m fix

COMMIT_TREE:git commit-tree
FAST_IMPORT:git fast-import
SEND_PACK:git send-pack
PUSH_MIRROR:git push --mirror origin
PUSH_FORCE:git push --force
FILTER_BRANCH:git filter-branch
BUNDLE:git bundle create
GIT_PUSH_PLAIN:git push origin next
GH_PR_CREATE:gh pr create next:main
GH_PR_MERGE:gh pr merge 123
GH_RELEASE:gh release create v1.0
GH_REPO_CREATE:gh repo create my-org/my-repo --public
GH_WORKFLOW_RUN:gh workflow run ci.yml
GH_API:gh api repos/my-org/my-repo/issues -X POST
GH_ISSUE_CLOSE:gh issue close 42

GIT_EDITOR:GIT_EDITOR=vim -c :!bash git commit
GIT_COMMITTER_NAME:GIT_COMMITTER_NAME=x git commit
GIT_EXTERNAL_DIFF:GIT_EXTERNAL_DIFF=sh git diff

ZERO_WIDTH_SPACE:fix: ​clean body
RTL_OVERRIDE:fix: ‮ attack
LRM_MARK:fix: ‎ text
BOM:fix: ﻿ body

BASE64_ENCODED:fix: see YGVjaG8gaGlgZWNobyBoaWBlY2hvIGhpYGVjaG8gaGlg payload

LITERAL_BACKTICK:fix: bad `echo HI`
DOLLAR_PAREN:fix: with $(date) embed
AND_AND:fix: a && b
OR_OR:fix: a || b
PIPE_SPACE:fix: a | b
TESTDATA
}

test_case() {
  local desc="$1" text="$2" want_dangerous="$3"
  if dangerous_present_silent "$text"; then
    got_dangerous="yes"
  else
    got_dangerous="no"
  fi
  if [[ "$got_dangerous" == "$want_dangerous" ]]; then
    printf '  PASS  %s\n' "$desc"
    PASS=$((PASS+1))
  else
    printf '  FAIL  %s (got=%s want=%s)\n' "$desc" "$got_dangerous" "$want_dangerous"
    printf '        input hex: '
    printf '%s' "$text" | xxd | head -3
    FAIL=$((FAIL+1))
  fi
}

# Expected matrix: TAG:want (yes = dangerous, no = clean)
declare -A WANTS=(
  [CLEAN_PLAIN]=no
  [CLEAN_RFC]=no
  [CLEAN_PATH]=no
  [CLEAN_DASH]=no
  [CLEAN_SQUOTE]=no
  [CLEAN_DQUOTE]=no
  [CLEAN_EQ]=no
  [CLEAN_PAREN]=no
  [SEMI_BODY]=yes
  [SEMI_SPACE]=yes
  [DOLLAR_VAR]=yes
  [DOLLAR_UNDER]=yes
  [DOLLAR_POS]=yes
  [DOLLAR_QUEST]=yes
  [DOLLAR_HASH]=yes
  [DOLLAR_STAR]=yes
  [DOLLAR_DOLLAR]=yes
  [DOLLAR_BRACE]=yes
  [NOVERIFY]=yes
  [NO_POST_HOOK]=yes
  [HOOKSPATH]=yes
  [CORE_EDITOR]=yes
  [ALIAS_COMMIT]=yes
  [EVAL]=yes
  [XARGS]=yes
  [BASH_C]=yes
  [SH_C]=yes
  [ENV]=yes
  [COMMIT_TREE]=yes
  [FAST_IMPORT]=yes
  [SEND_PACK]=yes
  [PUSH_MIRROR]=yes
  [PUSH_FORCE]=yes
  [FILTER_BRANCH]=yes
  [BUNDLE]=yes
  [GIT_PUSH_PLAIN]=yes
  [GH_PR_CREATE]=yes
  [GH_PR_MERGE]=yes
  [GH_RELEASE]=yes
  [GH_REPO_CREATE]=yes
  [GH_WORKFLOW_RUN]=yes
  [GH_API]=yes
  [GH_ISSUE_CLOSE]=yes
  [GIT_EDITOR]=yes
  [GIT_COMMITTER_NAME]=yes
  [GIT_EXTERNAL_DIFF]=yes
  [ZERO_WIDTH_SPACE]=yes
  [RTL_OVERRIDE]=yes
  [LRM_MARK]=yes
  [BOM]=yes
  [BASE64_ENCODED]=yes
  [LITERAL_BACKTICK]=yes
  [DOLLAR_PAREN]=yes
  [AND_AND]=yes
  [OR_OR]=yes
  [PIPE_SPACE]=yes
)

echo "Test matrix for dangerous-token library:"
echo "========================================="

# Iterate test data, mapping TAG to actual text + expected.
while IFS=':' read -r tag text; do
  [[ -z "$tag" ]] && continue
  [[ "$tag" == \#* ]] && continue
  want="${WANTS[$tag]:-no}"
  test_case "$tag" "$text" "$want"
done < <(read_test_data)

# Cases the one-line matrix above cannot express, plus the regressions for
# two defects that the matrix failed to catch.
#
# 1. The `>`, `<` and `!` tokens were written as `[[ "$text" =~ $'^\>' ]]`.
#    `$'...'` is a *quoted* expansion, so bash compares the right-hand side
#    LITERALLY: the pattern is the three bytes `^`, `\`, `>`, and no text
#    begins with that. All three tokens could never fire. The matrix had no
#    case starting with one of those characters, so 56/56 passed over dead
#    code. The documented intent is "matched at line start", so these assert
#    BOTH string start and body-line start.
#
# 2. The base64 branch examined only `BASH_REMATCH[1]`, the leftmost 32+ char
#    run, while its verdict was reported for the whole message. A commit that
#    cites a 40-char SHA before the payload — which is `[A-Za-z0-9]`, so it
#    matches the run pattern — made the encoded payload undetectable. The one
#    existing BASE64_ENCODED case put the payload first, the only ordering
#    that worked.
#
# The CLEAN_ cases below are the false-positive side of fix 1: a `>` or `<`
# that is not at the start of a line is ordinary prose and must stay clean.
B64_PAYLOAD='YnVpbGQgb2s7IGdpdCBwdXNoIG9yaWdpbiBuZXh0'   # "build ok; git push origin next"
SHA_LIKE='deadbeefcafebabe0123456789abcdef01234567'      # 40 hex chars, all in [A-Za-z0-9]
B64_NOISE='QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFB'  # decodes to AAAA...

echo "Exhaustive + regression cases:"
echo "-----------------------------------------"

test_case "LEADING_GT_TOP" "> out.txt" "yes"
test_case "LEADING_LT_TOP" "< in.txt" "yes"
test_case "LEADING_BANG_TOP" "!important" "yes"
test_case "LEADING_GT_BODY" "Subject line

> out.txt" "yes"
test_case "LEADING_LT_BODY" "Subject line

< in.txt" "yes"
test_case "LEADING_BANG_BODY" "Subject line

!note" "yes"

test_case "BASE64_AFTER_SHA" "Fixes $SHA_LIKE. artifact: $B64_PAYLOAD" "yes"
test_case "BASE64_AFTER_NOISE" "artifact: $B64_NOISE and $B64_PAYLOAD" "yes"
test_case "BASE64_ALONE" "artifact: $B64_PAYLOAD" "yes"

test_case "CLEAN_GT_MIDLINE" "3 > 2 holds here" "no"
test_case "CLEAN_LT_MIDLINE" "y is less than < z" "no"
test_case "CLEAN_BANG_MIDLINE" "wow! that worked" "no"

echo "========================================="
echo "Pass=$PASS Fail=$FAIL"
[[ $FAIL -eq 0 ]] || exit 1
