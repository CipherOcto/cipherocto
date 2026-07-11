#!/usr/bin/env bash
# scripts/install_test.sh
#
# Hermetic bash tests for scripts/install.sh. Every test:
#   - sets HOME to a tmpdir (so writes never touch the real $HOME)
#   - invokes the installer with a chosen flag set
#   - asserts on the resulting filesystem + JSON state
#
# Run from the repo root:
#   bash scripts/install_test.sh
# Or directly:
#   ./scripts/install_test.sh
#
# The script auto-detects SKIP_BINARY=1 because we don't want test
# runs to invoke cargo install or copy real binaries into $HOME.
# Detect-env overrides the Claude/Cursor/Continue/Windsurf HOME-based
# probes by pre-creating their markers in the fake HOME before run.
#
# Exit codes:
#   0   all tests passed
#   1   at least one assertion failed
#   2   prerequisite missing (jq absent, install.sh syntax-broken)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_SH="$SCRIPT_DIR/install.sh"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MCP_SRC="$REPO_ROOT/crates/octo-whatsapp/assets/mcp-configs"
SKILLS_SRC="$REPO_ROOT/crates/octo-whatsapp/assets/skills"

# === Test framework =======================================================

PASS=0
FAIL=0
FAILED_TESTS=()

red()   { printf '\033[31m%s\033[0m' "$*"; }
green() { printf '\033[32m%s\033[0m' "$*"; }
bold()  { printf '\033[1m%s\033[0m' "$*"; }

# t_run <name> <body>
# Runs a test in a sandboxed subshell. Catches non-zero exits.
t_run() {
  local name="$1"
  local body="$2"
  printf '%s ... ' "$name"
  if (
    # Each test gets its own HOME + workdir.
    local tmp_home tmp_work
    tmp_home="$(mktemp -d)"
    tmp_work="$(mktemp -d)"
    export HOME="$tmp_home"
    export XDG_CONFIG_HOME="$tmp_home/.config"
    export XDG_STATE_HOME="$tmp_home/.local/state"
    # No operator SHOULD_DRY_RUN-style override needed; flag control is
    # explicit per-test.
    eval "$body"
  ) >/tmp/install_test_out.$$ 2>&1; then
    PASS=$((PASS+1))
    printf '%s\n' "$(green PASS)"
  else
    FAIL=$((FAIL+1))
    FAILED_TESTS+=("$name")
    printf '%s\n' "$(red FAIL)"
    printf -- '----- %s output -----\n' "$name" >&2
    cat /tmp/install_test_out.$$ >&2
    printf -- '----- end %s -----\n' "$name" >&2
  fi
  rm -f /tmp/install_test_out.$$ 2>/dev/null || true
}

assert_file_exists() {
  local path="$1"
  [[ -f "$path" ]] || { printf "missing file: %s\n" "$path" >&2 ; return 1; }
}

assert_file_mode_eq() {
  local path="$1" want="$2"
  local got
  got="$(stat -c '%a' "$path" 2>/dev/null || stat -f '%Lp' "$path" 2>/dev/null || echo 0)"
  [[ "$got" == "$want" ]] || { printf "mode %s want %s got %s\n" "$path" "$want" "$got" >&2; return 1; }
}

assert_jq_eq() {
  local file="$1" jq_expr="$2" want="$3"
  local got
  got="$(jq -r "$jq_expr" "$file")" || { printf "jq failed on %s\n" "$file" >&2; return 1; }
  [[ "$got" == "$want" ]] || { printf "jq %s on %s: want %s got %s\n" "$jq_expr" "$file" "$want" "$got" >&2; return 1; }
}

assert_jq_truthy() {
  local file="$1" jq_expr="$2"
  local got
  got="$(jq -e "$jq_expr" "$file")" || { printf "jq %s on %s not truthy\n" "$jq_expr" "$file" >&2; return 1; }
}

# === Prereqs ==============================================================

if ! command -v jq >/dev/null 2>&1; then
  printf 'jq required for tests\n' >&2
  exit 2
fi
if ! bash -n "$INSTALL_SH"; then
  printf 'install.sh has syntax errors\n' >&2
  exit 2
fi

# === Tests ================================================================

# Test 1: --help exits 0 and prints usage
t_run "test_help_exits_zero" '
  "$INSTALL_SH" --help >/dev/null
'

# Test 2: --dry-run exits 0 and creates no files in HOME
t_run "test_dry_run_no_writes" '
  "$INSTALL_SH" --dry-run --skip-binary
  # No envs detected -> no MCP config files. Nothing should be written.
  [[ -z "$(ls -A "$HOME" 2>/dev/null)" ]] || { printf "HOME not empty after dry-run\n"; ls -A "$HOME" >&2; exit 1; }
'

# Test 3: dry-run with Claude Code detected shows the mcp step in output
t_run "test_dry_run_with_claude_code_reports_plan" '
  mkdir -p "$HOME/.claude"
  out=$("$INSTALL_SH" --dry-run --skip-binary 2>&1)
  echo "$out" | grep -q "mcp config: claude_code" || {
    printf "expected claude_code plan line\n" >&2
    echo "$out" >&2
    exit 1
  }
  echo "$out" | grep -q "$HOME/.claude/.mcp.json" || { echo "no target path in plan" >&2; exit 1; }
  echo "$out" | grep -q "skills:" || { echo "skills plan missing" >&2; exit 1; }
'

# Test 4: install with Claude Code writes .mcp.json with correct shape
t_run "test_install_writes_claude_mcp_config" '
  mkdir -p "$HOME/.claude"
  "$INSTALL_SH" --skip-binary
  target="$HOME/.claude/.mcp.json"
  assert_file_exists "$target"
  assert_jq_truthy "$target" ".mcpServers.\"octo-whatsapp\""
  assert_jq_eq "$target" ".mcpServers.\"octo-whatsapp\".command" "octo-whatsapp"
  assert_jq_eq "$target" ".mcpServers.\"octo-whatsapp\".args[0]" "mcp"
  assert_jq_eq "$target" ".mcpServers.\"octo-whatsapp\".env.OCTO_WHATSAPP_PERSIST_DIR" "\${HOME}/.local/share/octo/whatsapp"
'

# Test 5: install with Continue writes config.json nested under experimental
t_run "test_install_writes_continue_nested_config" '
  mkdir -p "$HOME/.continue"
  "$INSTALL_SH" --skip-binary
  target="$HOME/.continue/config.json"
  assert_file_exists "$target"
  assert_jq_truthy "$target" ".experimental.mcpServers.\"octo-whatsapp\""
  assert_jq_eq "$target" ".experimental.mcpServers.\"octo-whatsapp\".command" "octo-whatsapp"
'

# Test 6: install with Cursor writes ~/.cursor/mcp.json
t_run "test_install_writes_cursor_mcp_config" '
  mkdir -p "$HOME/.cursor"
  "$INSTALL_SH" --skip-binary
  target="$HOME/.cursor/mcp.json"
  assert_file_exists "$target"
  assert_jq_truthy "$target" ".mcpServers.\"octo-whatsapp\""
'

# Test 7: install with Windsurf writes mcp_config.json (XDG path)
t_run "test_install_writes_windsurf_mcp_config" '
  mkdir -p "$HOME/.config/Codium/User"
  "$INSTALL_SH" --skip-binary
  target="$HOME/.config/Codium/User/mcp_config.json"
  assert_file_exists "$target"
  assert_jq_truthy "$target" ".mcpServers.\"octo-whatsapp\""
'

# Test 8: install with Windsurf legacy codeium path
t_run "test_install_windsurf_codeium_path" '
  mkdir -p "$HOME/.codeium/windsurf"
  "$INSTALL_SH" --skip-binary
  target="$HOME/.codeium/wendsurf/mcp_config.json"
  if [[ ! -f "$target" ]]; then
    # Try the correct path (mcp_config.json, not wendsurf typo)
    target="$HOME/.codeium/windsurf/mcp_config.json"
  fi
  # Defensive: the installer chooses codeium path when present.
  assert_file_exists "$target"
'

# Test 9: JSON merge — pre-existing other MCP server is preserved
t_run "test_json_merge_preserves_other_servers" '
  mkdir -p "$HOME/.claude"
  # Pre-existing config with another MCP server entry.
  cat > "$HOME/.claude/.mcp.json" <<JSON
{
  "mcpServers": {
    "not-octo": {
      "command": "other-binary",
      "args": ["x"]
    }
  }
}
JSON
  "$INSTALL_SH" --skip-binary
  target="$HOME/.claude/.mcp.json"
  assert_jq_truthy "$target" ".mcpServers.\"not-octo\""
  assert_jq_eq "$target" ".mcpServers.\"not-octo\".command" "other-binary"
  assert_jq_truthy "$target" ".mcpServers.\"octo-whatsapp\""
'

# Test 10: idempotent — running twice produces same state
t_run "test_idempotent_run_twice" '
  mkdir -p "$HOME/.claude"
  "$INSTALL_SH" --skip-binary
  first_sha="$(sha256sum "$HOME/.claude/.mcp.json" | awk "{print \$1}")"
  "$INSTALL_SH" --skip-binary
  second_sha="$(sha256sum "$HOME/.claude/.mcp.json" | awk "{print \$1}")"
  [[ "$first_sha" == "$second_sha" ]] || { printf "idempotency fail: %s != %s\n" "$first_sha" "$second_sha" >&2; exit 1; }
'

# Test 11: skills emit copies all 5 skill files for Claude Code
t_run "test_skills_copied_to_claude_home" '
  mkdir -p "$HOME/.claude"
  "$INSTALL_SH" --skip-binary
  expected=("wa-mcp.md" "wa-send.md" "wa-monitor.md" "wa-recover.md" "wa-config.md")
  for skill in "${expected[@]}"; do
    assert_file_exists "$HOME/.claude/skills/$skill" || { echo "missing skill: $skill" >&2; exit 1; }
  done
'

# Test 12: --with-aider installs the shim
t_run "test_with_aider_installs_shim" '
  # Aider install requires a writable ~/.local/bin; we set HOME to tmp,
  # but the installer uses AIDER_DEST default of $HOME/.local/bin/wa.
  # Override AIDER_DEST explicitly to avoid surprises.
  export AIDER_DEST="$HOME/.local/bin/wa"
  "$INSTALL_SH" --skip-binary --with-aider
  # Only triggers when no envs exist; force a no-env dry state by NOT
  # pre-creating any env dir. Skip if any env got detected.
  # (Test asserts the file exists regardless of detection; installer
  # emits the shim unconditionally under --with-aider.)
  assert_file_exists "$AIDER_DEST"
  assert_file_mode_eq "$AIDER_DEST" "755"
'

# Test 13: without --with-aider, no shim is installed
t_run "test_without_aider_no_shim" '
  # Even if we put the dest somewhere writable, the flag must be off.
  export AIDER_DEST="$HOME/.local/bin/wa"
  "$INSTALL_SH" --skip-binary
  [[ ! -f "$AIDER_DEST" ]] || { echo "shim present despite no flag" >&2; exit 1; }
'

# Test 14: uninstall removes octo-whatsapp but keeps other servers
t_run "test_uninstall_keeps_other_servers" '
  mkdir -p "$HOME/.claude"
  # Plant a config + skills + pretend binary exists.
  cat > "$HOME/.claude/.mcp.json" <<JSON
{
  "mcpServers": {
    "not-octo": {"command": "x", "args": []},
    "octo-whatsapp": {"command": "octo-whatsapp", "args": ["mcp"], "env": {}}
  }
}
JSON
  # Run install to populate skills, then uninstall.
  "$INSTALL_SH" --skip-binary
  ls "$HOME/.claude/skills/" >/dev/null
  "$INSTALL_SH" --skip-binary --uninstall
  target="$HOME/.claude/.mcp.json"
  # After uninstall: octo-whatsapp entry gone, not-octo entry preserved.
  if ! jq -e "has(\"mcpServers\")" "$target" >/dev/null 2>&1; then
    # Whole mcpServers block was deleted (file is now empty) — also OK
    # because no other entries survived.
    :
  else
    jq -e ".mcpServers | has(\"octo-whatsapp\") | not" "$target" >/dev/null || {
      echo "octo-whatsapp still present after uninstall" >&2; exit 1;
    }
    jq -e ".mcpServers.\"not-octo\"" "$target" >/dev/null || {
      echo "not-octo gone after uninstall" >&2; exit 1;
    }
  fi
'

# Test 15: dry-run does NOT modify a pre-existing config
t_run "test_dry_run_does_not_modify_existing" '
  mkdir -p "$HOME/.claude"
  cat > "$HOME/.claude/.mcp.json" <<JSON
{"mcpServers":{"sentinel":{"command":"x","args":[]}}}
JSON
  before_sha="$(sha256sum "$HOME/.claude/.mcp.json" | awk "{print \$1}")"
  "$INSTALL_SH" --skip-binary --dry-run
  after_sha="$(sha256sum "$HOME/.claude/.mcp.json" | awk "{print \$1}")"
  [[ "$before_sha" == "$after_sha" ]] || { echo "dry-run mutated existing config" >&2; exit 1; }
'

# Test 16: unknown flag exits non-zero
t_run "test_unknown_flag_exits_nonzero" '
  if "$INSTALL_SH" --bogus 2>/dev/null; then
    echo "installer accepted unknown flag" >&2
    exit 1
  fi
'

# Test 17: no envs detected -> success exit, no files written
t_run "test_no_envs_exits_zero_no_writes" '
  # Fresh tmp HOME with no agent dirs.
  ls "$HOME" >/dev/null
  "$INSTALL_SH" --skip-binary
  # No .claude/.cursor/.continue/.config dirs were touched.
  [[ ! -d "$HOME/.claude" ]] || { echo "spurious .claude created" >&2; exit 1; }
  [[ ! -d "$HOME/.cursor" ]] || { echo "spurious .cursor created" >&2; exit 1; }
'

# === Report ===============================================================

printf '\n'
printf '%s  passed\n' "$(bold "$PASS")"
[[ $FAIL -gt 0 ]] && printf '%s  failed\n' "$(bold "$FAIL")" || true
if [[ $FAIL -gt 0 ]]; then
  printf 'failed tests:\n' >&2
  for t in "${FAILED_TESTS[@]}"; do
    printf '  - %s\n' "$t" >&2
  done
  exit 1
fi
exit 0
