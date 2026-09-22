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
