#!/usr/bin/env bash
# install-hooks.sh — install repo hooks from scripts/hooks/ into .git/hooks/
#
# Layer 2 of the [[no-backtick-in-commit-messages]] prevention framework.
# Companion hook: scripts/hooks/commit-msg-no-backtick
#
# Idempotent: re-running replaces existing symlinks/copies.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
HOOKS_DIR="$REPO_ROOT/.git/hooks"
SOURCE_DIR="$REPO_ROOT/scripts/hooks"

if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "no hooks to install: $SOURCE_DIR missing" >&2
  exit 0
fi

echo "installing hooks from $SOURCE_DIR -> $HOOKS_DIR"

for src in "$SOURCE_DIR"/*; do
  [[ -f "$src" ]] || continue
  base="$(basename "$src")"
  dst="$HOOKS_DIR/$base"

  # Drop the descriptive "-no-backtick" suffix so the hook is named after
  # the git lifecycle event (commit-msg) not the implementation detail.
  case "$base" in
    *-no-backtick) dst="$HOOKS_DIR/${base%-no-backtick}" ;;
  esac

  if [[ -e "$dst" && ! -L "$dst" ]]; then
    # Existing non-symlink hook — preserve by renaming to .preserved
    if [[ ! -e "$dst.preserved" ]]; then
      mv "$dst" "$dst.preserved"
      echo "  preserved existing $base -> ${base}.preserved"
    fi
  fi

  # Prefer symlink so subsequent edits to the tracked hook take effect
  # on the next `git` invocation without re-running this script.
  ln -sf "$src" "$dst"
  chmod +x "$src"
  echo "  installed $(basename "$dst") -> $src"
done

echo "done."
