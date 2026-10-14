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

installed=()
failed=()

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
      echo "  preserved existing $(basename "$dst") -> $(basename "$dst").preserved"
    fi
  fi

  # Prefer symlink so subsequent edits to the tracked hook take effect
  # on the next `git` invocation without re-running this script.
  ln -sf "$src" "$dst"
  chmod +x "$src"
  installed+=("$dst")

  # Prove the installed hook can actually run. Installing is not the same
  # as working: every hook here sources the shared token library by a path
  # relative to its own location, and a hook whose resolution disagrees
  # with its sibling installs cleanly and then fails on first use. This
  # script used to print "installed" and "done." for such a hook without
  # ever running it.
  #
  # Each hook gets the minimal input its own contract requires.
  probe=$(mktemp)
  # The probe runs inside `if` rather than bare: under `set -e` a bare
  # non-zero command aborts this script before the diagnostic below can
  # print, so the installer would exit 1 with no explanation of which hook
  # was broken. Failing closed without saying why is its own defect.
  case "$(basename "$dst")" in
    commit-msg)
      if bash "$dst" "$probe" >/dev/null 2>&1; then rc=0; else rc=$?; fi
      ;;
    *)
      if bash "$dst" </dev/null >/dev/null 2>&1; then rc=0; else rc=$?; fi
      ;;
  esac
  rm -f "$probe"

  if [[ $rc -eq 0 ]]; then
    echo "  installed $(basename "$dst") -> $src (verified)"
  else
    echo "  installed $(basename "$dst") -> $src BUT IT DOES NOT RUN (exit $rc)" >&2
    failed+=("$dst")
  fi
done

if (( ${#failed[@]} > 0 )); then
  echo "" >&2
  echo "install-hooks: ${#failed[@]} of ${#installed[@]} hook(s) installed but unusable:" >&2
  printf '  %s\n' "${failed[@]}" >&2
  echo "A hook that cannot run is worse than no hook: it looks like the" >&2
  echo "guard is present. Check that the hook resolves its token library." >&2
  exit 1
fi

echo "done."
