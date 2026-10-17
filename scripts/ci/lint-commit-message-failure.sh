#!/usr/bin/env bash
# Render the commit-message lint failure banner for the
# commit-message-lint.yml CI workflow.
#
# Layer 6 of no-backtick-in-commit-messages. The body used to
# live inline in the workflow YAML, but the heredoc delimiter
# required column-1 placement, which collided with YAML literal
# block scalar parsing (Python yaml.safe_load reports "while
# scanning a simple key ... could not find expected ':' at the
# first separator line"). Moving the body here unblocks the
# workflow.
#
# Usage:
#   bash scripts/ci/lint-commit-message-failure.sh "$BAD"
# where $BAD is a space-separated list of offending commit SHAs.

set -euo pipefail

BAD="${1:-}"

case "${1:-}" in
    -h|--help)
        cat <<'USAGE'
Render the commit-message lint failure banner for commit-message-lint.yml.

Usage:
  bash scripts/ci/lint-commit-message-failure.sh "$BAD"
  bash scripts/ci/lint-commit-message-failure.sh --help

  $BAD  space-separated list of offending commit SHAs. Required.

Refuses to run with no list. Rendering "COMMIT MESSAGE LINT FAILED" and then
naming no commits states a failure about a population the caller never
supplied, and exits 0, so a caller checking the exit code reads it as
success.
USAGE
        exit 0
        ;;
esac

# An argument this script does not understand is a commit SHA as far as it can
# tell, and it renders. `--help` printed a failure banner listing a commit
# called "--help", which is how a banner that names its own input can be
# trusted to name real commits.
if [ -z "${BAD//[[:space:]]/}" ]; then
    echo "Refusing to render a commit-message lint failure banner naming no commits." >&2
    echo "" >&2
    echo "Called with '${1-<no argument>}' and no offending commit SHAs." >&2
    echo "A banner headed COMMIT MESSAGE LINT FAILED that lists nothing asserts a" >&2
    echo "failure about a population the caller did not supply. This script exits 1" >&2
    echo "there rather than print one." >&2
    echo "" >&2
    echo "Invoke it only with the SHAs to report. Run with --help for usage." >&2
    exit 1
fi

cat <<EOF

================================================================================
COMMIT MESSAGE LINT FAILED (Layer 6)
================================================================================
The following commits contain dangerous tokens in their message bodies:

${BAD}

RFC-0011-c 2026-09-08 lesson: bash POSIX §2.6.3 expands backticks / \$(...)
inside double-quoted \`git commit -m "..."\` BEFORE git sees the message.
4 fix commits were pushed inadvertently that day. See
[[no-backtick-in-commit-messages]] for the full incident write-up.

Token list: scripts/wrappers/dangerous-tokens.sh

Fix per offending commit:
  git commit --amend -F /tmp/clean-msg.txt

Where /tmp/clean-msg.txt is written via:

  cat > /tmp/clean-msg.txt <<'EOF'
  <subject>

  <body - single-quoted heredoc, no shell expansion>
  EOF

Or, use the local safe-commit wrapper:
  safe-commit --amend -F /tmp/clean-msg.txt
================================================================================
EOF
