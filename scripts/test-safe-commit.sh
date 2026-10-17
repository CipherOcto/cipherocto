#!/usr/bin/env bash
# test-safe-commit.sh — hermetic tests for scripts/wrappers/safe-commit.
#
# The wrapper re-parses git's own argument list to decide which values are
# commit messages, so the thing worth testing is its coverage of git's
# argument syntax, not the token library underneath it. The library has its
# own suite in scripts/test-dangerous-tokens.sh.
#
# Every form of `git commit -m` must be scanned, including the combined short
# flag `git commit -am "msg"`. That form was not recognised, so the message was
# never added to the scan list and the wrapper handed it straight to git:
# Layer 3 of the commit-message defence was bypassed by ordinary usage.
#
# Run from the repo root:
#   bash scripts/test-safe-commit.sh
#
# Exit codes:
#   0   all tests passed
#   1   at least one assertion failed

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WRAPPER="$SCRIPT_DIR/wrappers/safe-commit"
LIB="$SCRIPT_DIR/wrappers/dangerous-tokens.sh"

PASS=0
FAIL=0
FAILED=()

# A message containing a semicolon, which the token library rejects. The
# literal is assembled at runtime so this file does not itself contain the
# token it is testing for.
DANGEROUS="subject body $(printf '\073') semicolon"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT INT TERM

fresh_repo() {
    local d
    d="$(mktemp -d "$WORK/XXXXXXXX")"
    mkdir -p "$d"
    (
        cd "$d" || exit 1
        git init -q .
        git config user.email t@example.invalid
        git config user.name test
        printf 'seed\n' > file
        git add file
        GIT_EDITOR=true git commit -q -m "seed commit"
    ) >/dev/null 2>&1
    cp "$WRAPPER" "$LIB" "$d/"
    chmod +x "$d/safe-commit"
    echo "$d"
}

# must_refuse <name> <flag...>
# A dangerous message must be rejected AND must not reach a commit.
must_refuse() {
    local name="$1"; shift
    local d; d="$(fresh_repo "$name")"
    printf 'change\n' >>"$d/file"
    local out rc
    out="$(cd "$d" && ./safe-commit "$@" "$DANGEROUS" 2>&1)"; rc=$?
    local refused; refused="$(grep -c 'REFUSED by safe-commit' <<<"$out")"
    local count; count="$(cd "$d" && git rev-list --count HEAD)"
    if [[ "$rc" -eq 0 || "$refused" -eq 0 || "$count" -ne 1 ]]; then
        printf '  FAIL  %-28s rc=%s refused=%s commits=%s (want rc!=0 refused>=1 commits=1)\n' \
            "$name" "$rc" "$refused" "$count"
        FAIL=$((FAIL + 1)); FAILED+=("$name"); return
    fi
    printf '  PASS  %-28s refused, no commit created\n' "$name"
    PASS=$((PASS + 1))
}

# must_commit <name> <flag...>
# A clean message must still be allowed through.
must_commit() {
    local name="$1"; shift
    local d; d="$(fresh_repo "$name")"
    printf 'change\n' >>"$d/file"
    git -C "$d" add file
    local rc
    (cd "$d" && ./safe-commit "$@" "clean subject" >/dev/null 2>&1); rc=$?
    local count; count="$(cd "$d" && git rev-list --count HEAD)"
    if [[ "$rc" -ne 0 || "$count" -ne 2 ]]; then
        printf '  FAIL  %-28s rc=%s commits=%s (want rc=0 commits=2)\n' "$name" "$rc" "$count"
        FAIL=$((FAIL + 1)); FAILED+=("$name"); return
    fi
    printf '  PASS  %-28s committed\n' "$name"
    PASS=$((PASS + 1))
}

echo "safe-commit argument-coverage tests:"
echo "========================================="

# Every spelling git accepts for a commit message.
must_refuse "short -m"        -m
must_refuse "long --message"  --message
must_refuse "combined -am"    -am
must_refuse "combined -qam"   -qam
must_refuse "separate -a -m"  -a -m
must_refuse "equals form"     --message="$DANGEROUS"

must_commit "short -m"        -m
must_commit "combined -am"    -am
must_commit "combined -qam"   -qam
# The equals form carries its value inside the flag, so it cannot go through
# must_commit, which appends the message as a separate argument.
_d="$(fresh_repo "equals-commit")"
printf 'change\n' >>"$_d/file"
git -C "$_d" add file
if (cd "$_d" && ./safe-commit "--message=clean subject" >/dev/null 2>&1) \
   && [[ "$(git -C "$_d" rev-list --count HEAD)" -eq 2 ]]; then
    printf '  PASS  %-28s committed\n' "equals form"
    PASS=$((PASS + 1))
else
    printf '  FAIL  %-28s (want rc=0 commits=2)\n' "equals form"
    FAIL=$((FAIL + 1)); FAILED+=("equals form")
fi

echo "========================================="
printf '%s passed\n' "$PASS"
if [[ $FAIL -gt 0 ]]; then
    printf '%s failed\n' "$FAIL" >&2
    printf 'failed tests:\n' >&2
    printf '  - %s\n' "${FAILED[@]}" >&2
    exit 1
fi
exit 0
