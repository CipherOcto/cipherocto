#!/usr/bin/env bash
# scripts/install.sh
#
# One-command installer for the octo-whatsapp runtime + Claude/Cursor/
# Continue/Windsurf MCP configs + Claude Code skills + (optional) Aider
# shell shim.
#
# Behaviour:
#   1. Detect platform (linux/macos) and arch (x86_64/aarch64).
#   2. Install `octo-whatsapp` binary. Prefer `cargo install` if cargo
#      is present; otherwise copy a prebuilt binary from the repo's
#      `target/release/octo-whatsapp` (operator-built) or skip with
#      `--skip-binary` (config-only mode for upgrades).
#   3. Detect which AI-agent environments are installed (check well-
#      known config dirs).
#   4. For each detected env, merge the matching MCP config snippet
#      into the agent's config file. Existing MCP server entries are
#      preserved; only the `octo-whatsapp` block is overwritten.
#   5. For Claude Code, also copy the 5 skill files into
#      `~/.claude/skills/` (mkdir -p first).
#   6. With `--with-aider`, install the Aider shell shim to
#      `~/.local/bin/wa` (chmod +x).
#   7. Print a summary listing what changed.
#
# Flags:
#   --dry-run        print plan, change nothing, exit 0
#   --with-aider     also install the Aider shim
#   --skip-binary    skip binary install (config-only upgrade)
#   --uninstall      reverse everything the installer did
#   -h|--help        this help
#
# Exit codes:
#   0   success (including dry-run and "nothing to do")
#   1   prerequisite missing (jq absent, unrecoverable)
#   2   install failed (binary copy, JSON write, permission)
#
# Operator notes:
#   - All file writes are atomic (temp file + mv).
#   - JSON merge is safe to run repeatedly (idempotent).
#   - `--dry-run` is hermetic — no writes, no `cargo install`,
#     no outbound HTTP.
#   - The installer is local-first: no GitHub release fallback
#     (this binary is not yet published). Operators build with
#     `cargo build --release -p octo-whatsapp` and the installer
#     picks up `target/release/octo-whatsapp`.

set -euo pipefail

# === Path resolution ========================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ASSETS_DIR="$REPO_ROOT/crates/octo-whatsapp/assets"
SKILLS_SRC="$ASSETS_DIR/skills"
MCP_SRC="$ASSETS_DIR/mcp-configs"

STATE_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/octo-whatsapp-install"
LOG_FILE="${OCTO_WHATSAPP_INSTALL_LOG:-$STATE_DIR/install.log}"

# === Logging ===============================================================

LOG_TS='date +%Y-%m-%dT%H:%M:%S%z'

log_init() {
  # Dry-run must be hermetic — no filesystem writes.
  if [[ ${DRY_RUN:-0} -eq 1 ]]; then
    return 0
  fi
  mkdir -p "$STATE_DIR"
}

log_info()  {
  if [[ ${DRY_RUN:-0} -eq 1 ]]; then
    printf '%s [info]  %s\n'  "$($LOG_TS)" "$*" >&2
  else
    printf '%s [info]  %s\n'  "$($LOG_TS)" "$*" | tee -a "$LOG_FILE" >&2
  fi
}
log_warn()  {
  if [[ ${DRY_RUN:-0} -eq 1 ]]; then
    printf '%s [warn]  %s\n'  "$($LOG_TS)" "$*" >&2
  else
    printf '%s [warn]  %s\n'  "$($LOG_TS)" "$*" | tee -a "$LOG_FILE" >&2
  fi
}
log_error() {
  if [[ ${DRY_RUN:-0} -eq 1 ]]; then
    printf '%s [error] %s\n'  "$($LOG_TS)" "$*" >&2
  else
    printf '%s [error] %s\n'  "$($LOG_TS)" "$*" | tee -a "$LOG_FILE" >&2
  fi
}
log_step() {
  if [[ ${DRY_RUN:-0} -eq 1 ]]; then
    printf '\n=== %s ===\n' "$*" >&2
  else
    printf '\n=== %s ===\n' "$*" | tee -a "$LOG_FILE" >&2
  fi
}

# Print to stdout only (for --dry-run) so the operator sees it clean.
dry() { printf '%s\n' "$*"; }

# === Flag parsing ==========================================================

WITH_AIDER=0
SKIP_BINARY=0
DRY_RUN=0
UNINSTALL=0
PRINT_HELP=0

usage() {
  sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" \
    | sed -e 's/^# \{0,1\}//' -e '/^$/q' \
    | head -n -1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)     DRY_RUN=1 ;;
    --with-aider)  WITH_AIDER=1 ;;
    --skip-binary) SKIP_BINARY=1 ;;
    --uninstall)   UNINSTALL=1 ;;
    -h|--help)     PRINT_HELP=1 ;;
    *) log_error "unknown flag: $1"; PRINT_HELP=1 ;;
  esac
  shift
done

if [[ $PRINT_HELP -eq 1 ]]; then
  usage
  exit 0
fi

# === Prereqs ===============================================================

log_init

for tool in bash jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    log_error "$tool not found in PATH"
    exit 1
  fi
done

# === Platform detection ====================================================

detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m)"
  case "$os" in
    linux)  OS=linux ;;
    darwin) OS=macos ;;
    *)      log_error "unsupported OS: $os"; exit 1 ;;
  esac
  case "$arch" in
    x86_64|amd64)  ARCH=x86_64 ;;
    aarch64|arm64) ARCH=aarch64 ;;
    *)             log_error "unsupported arch: $arch"; exit 1 ;;
  esac
  log_info "platform: $OS $ARCH"
}

# === Env detection =========================================================

CLAUDE_CODE_HOME="${CLAUDE_HOME:-$HOME/.claude}"
CURSOR_HOME="$HOME/.cursor"
CONTINUE_HOME="$HOME/.continue"
WINDSURF_HOME="${XDG_CONFIG_HOME:-$HOME/.config}/Codium/User"
AIDER_BIN_DST="${AIDER_DEST:-$HOME/.local/bin/wa}"

detect_envs() {
  DETECTED_ENVS=()
  [[ -d "$CLAUDE_CODE_HOME" || -d "$REPO_ROOT/.claude" ]] && DETECTED_ENVS+=(claude_code)
  [[ -d "$CURSOR_HOME" ]]    && DETECTED_ENVS+=(cursor)
  [[ -d "$CONTINUE_HOME" ]]  && DETECTED_ENVS+=(continue)
  # Windsurf defaults to XDG path; some installs use ~/.codeium instead.
  if [[ -d "$WINDSURF_HOME" || -d "$HOME/.codeium/windsurf" || -d "$HOME/.config/Codium" ]]; then
    DETECTED_ENVS+=(windsurf)
  fi
  log_info "detected envs: ${DETECTED_ENVS[*]:-<none>}"
}

# === Binary install ========================================================

BIN_NAME="octo-whatsapp"
BIN_SRC_CARGO="$REPO_ROOT/target/release/$BIN_NAME"
BIN_DST_DIR="${CARGO_INSTALL_ROOT:-$HOME/.cargo}/bin"
BIN_DST="$BIN_DST_DIR/$BIN_NAME"

install_binary() {
  if [[ $SKIP_BINARY -eq 1 ]]; then
    log_info "binary: skip-binary set, no install"
    return 0
  fi
  if command -v cargo >/dev/null 2>&1 && [[ -f "$REPO_ROOT/crates/octo-whatsapp/Cargo.toml" ]]; then
    if [[ $DRY_RUN -eq 1 ]]; then
      log_info "binary: cargo install --path crates/octo-whatsapp --root $BIN_DST_DIR --quiet (dry-run)"
    else
      log_step "installing $BIN_NAME via cargo"
      (cd "$REPO_ROOT" && cargo install --path crates/octo-whatsapp --root "$BIN_DST_DIR" --quiet)
      log_info "installed: $BIN_DST"
    fi
    return 0
  fi
  if [[ -x "$BIN_SRC_CARGO" ]]; then
    if [[ $DRY_RUN -eq 1 ]]; then
      log_info "binary: cp $BIN_SRC_CARGO $BIN_DST (dry-run)"
    else
      log_step "copying prebuilt $BIN_NAME"
      mkdir -p "$BIN_DST_DIR"
      cp -f "$BIN_SRC_CARGO" "$BIN_DST"
      chmod +x "$BIN_DST"
      log_info "installed: $BIN_DST"
    fi
    return 0
  fi
  log_warn "no cargo and no $BIN_SRC_CARGO; skipping binary install (use --skip-binary to silence)"
}

uninstall_binary() {
  if [[ -f "$BIN_DST" ]]; then
    if [[ $DRY_RUN -eq 1 ]]; then
      dry "[uninstall] rm $BIN_DST"
    else
      rm -f "$BIN_DST"
      log_info "removed: $BIN_DST"
    fi
  fi
}

# === JSON merge helpers ====================================================

# merge_snippet <snippet.json> <target-config.json> -> emits merged JSON
# Preserves any pre-existing MCP servers in target; overwrites only the
# `octo-whatsapp` block (and keeps nested `experimental.mcpServers`
# shape for Continue).
merge_snippet() {
  local snippet="$1"
  local target="$2"
  local existing
  if [[ -f "$target" ]]; then
    existing="$(cat "$target")"
  else
    existing='null'
  fi
  local snippet_json
  snippet_json="$(cat "$snippet")"
  # Build a self-contained jq program. Output is always a single object;
  # we never use jq -s because that wraps multi-file inputs in an array
  # and breaks first-time installs (null target + snippet -> [{...}]).
  local merged
  merged="$(jq -n \
    --argjson existing "$existing" \
    --argjson snippet  "$snippet_json" '
      ($existing // {}) as $e | $snippet as $s
      | if ($s | has("mcpServers"))
          then $e | .mcpServers = ((.mcpServers // {}) * $s.mcpServers)
          elif ($s | has("experimental"))
          then $e | .experimental = (
            (.experimental // {})
            | .mcpServers = ((.mcpServers // {}) * $s.experimental.mcpServers)
          )
          else $e
        end
    ' | jq -S .)"
  local tmp
  tmp="$(mktemp "${target%.json}.XXXXXX.json")"
  printf '%s\n' "$merged" > "$tmp"
  mv -f "$tmp" "$target"
}

# emit_mcp_config <env_name>
#   - resolves target config path
#   - selects matching snippet under $MCP_SRC
#   - calls merge_snippet
emit_mcp_config() {
  local env_name="$1"
  local snippet target snippet_path
  case "$env_name" in
    claude_code)
      snippet_path="$MCP_SRC/claude-code.json"
      target="$CLAUDE_CODE_HOME/.mcp.json"
      ;;
    cursor)
      snippet_path="$MCP_SRC/cursor.json"
      target="$CURSOR_HOME/mcp.json"
      ;;
    continue)
      snippet_path="$MCP_SRC/continue.json"
      target="$CONTINUE_HOME/config.json"
      ;;
    windsurf)
      snippet_path="$MCP_SRC/windsurf.json"
      # Two common Windsurf paths; pick whichever directory exists.
      if [[ -d "$HOME/.codeium/windsurf" ]]; then
        target="$HOME/.codeium/windsurf/mcp_config.json"
      else
        target="$WINDSURF_HOME/mcp_config.json"
      fi
      ;;
    *)
      log_warn "unknown env: $env_name"
      return 0
      ;;
  esac

  if [[ ! -f "$snippet_path" ]]; then
    log_warn "missing snippet for $env_name: $snippet_path"
    return 0
  fi

  if [[ $DRY_RUN -eq 1 ]]; then
    log_info "mcp config: $env_name -> $target (dry-run)"
    return 0
  fi

  mkdir -p "$(dirname "$target")"
  merge_snippet "$snippet_path" "$target"
  chmod 600 "$target"
  log_info "mcp config: $env_name -> $target"
}

uninstall_mcp_config() {
  local env_name="$1"
  local target
  case "$env_name" in
    claude_code) target="$CLAUDE_CODE_HOME/.mcp.json" ;;
    cursor)      target="$CURSOR_HOME/mcp.json" ;;
    continue)    target="$CONTINUE_HOME/config.json" ;;
    windsurf)
      if [[ -d "$HOME/.codeium/windsurf" ]]; then
        target="$HOME/.codeium/windsurf/mcp_config.json"
      else
        target="$WINDSURF_HOME/mcp_config.json"
      fi
      ;;
  esac
  if [[ -f "$target" ]]; then
    if [[ $DRY_RUN -eq 1 ]]; then
      dry "[uninstall] strip octo-whatsapp from $target"
      return 0
    fi
    local tmp
    tmp="$(mktemp "${target%.json}.XXXXXX.json")"
    # Keep the file but drop the octo-whatsapp entry. For Continue the
    # entry lives under .experimental.mcpServers.
    jq '
      if has("mcpServers")
        then del(.mcpServers["octo-whatsapp"])
        elif has("experimental")
        then .experimental |= (if has("mcpServers") then del(.mcpServers["octo-whatsapp"]) else . end)
        else .
      end
    | if (.experimental.mcpServers // null) == {} or (.experimental.mcpServers // null) == null
      then del(.experimental)
      else . end
    | if .mcpServers == {} or (.mcpServers // null) == null
      then del(.mcpServers)
      else . end
    ' "$target" > "$tmp"
    if jq -e 'has("mcpServers") or has("experimental")' "$tmp" >/dev/null 2>&1; then
      mv -f "$tmp" "$target"
    else
      # File is now empty of MCP-relevant keys; remove it entirely.
      rm -f "$tmp" "$target"
    fi
    log_info "stripped octo-whatsapp from $target"
  fi
}

# === Skills emit (Claude Code) =============================================

CLAUDE_SKILLS_DST="$CLAUDE_CODE_HOME/skills"

emit_skills() {
  # Skills are a Claude Code surface only. Skip when no Claude env was
  # detected to avoid creating spurious ~/.claude/skills/ directories
  # on hosts where Claude Code isn't installed.
  local has_claude=0
  for env_name in "${DETECTED_ENVS[@]:-}"; do
    [[ "$env_name" == "claude_code" ]] && has_claude=1
  done
  if [[ $has_claude -eq 0 ]]; then
    dry "[skills] skipped (no Claude Code env detected)"
    return 0
  fi
  if [[ ! -d "$SKILLS_SRC" ]]; then
    log_warn "skills source dir missing: $SKILLS_SRC"
    return 0
  fi
  if [[ $DRY_RUN -eq 1 ]]; then
    log_info "skills: $SKILLS_SRC/*.md -> $CLAUDE_SKILLS_DST/ (dry-run)"
    return 0
  fi
  mkdir -p "$CLAUDE_SKILLS_DST"
  local copied=0
  for skill_file in "$SKILLS_SRC"/*.md; do
    [[ -f "$skill_file" ]] || continue
    cp -f "$skill_file" "$CLAUDE_SKILLS_DST/"
    copied=$((copied + 1))
  done
  chmod -R u+rwX,g-rwx,o-rwx "$CLAUDE_SKILLS_DST"
  log_info "skills: $copied file(s) -> $CLAUDE_SKILLS_DST/"
}

uninstall_skills() {
  if [[ ! -d "$CLAUDE_SKILLS_DST" ]]; then
    return 0
  fi
  for skill_file in "$SKILLS_SRC"/*.md; do
    [[ -f "$skill_file" ]] || continue
    local base
    base="$(basename "$skill_file")"
    if [[ -f "$CLAUDE_SKILLS_DST/$base" ]]; then
      if [[ $DRY_RUN -eq 1 ]]; then
        dry "[uninstall] rm $CLAUDE_SKILLS_DST/$base"
      else
        rm -f "$CLAUDE_SKILLS_DST/$base"
      fi
    fi
  done
  log_info "skills: removed octo-whatsapp entries from $CLAUDE_SKILLS_DST/"
}

# === Aider shim ============================================================

emit_aider_shim() {
  if [[ $WITH_AIDER -ne 1 ]]; then
    return 0
  fi
  local src="$MCP_SRC/aider.sh"
  if [[ ! -f "$src" ]]; then
    log_warn "aider shim missing: $src"
    return 0
  fi
  if [[ $DRY_RUN -eq 1 ]]; then
    dry "[aider] cp $src $AIDER_BIN_DST"
    return 0
  fi
  mkdir -p "$(dirname "$AIDER_BIN_DST")"
  cp -f "$src" "$AIDER_BIN_DST"
  chmod 755 "$AIDER_BIN_DST"
  log_info "aider shim: $AIDER_BIN_DST"
}

uninstall_aider_shim() {
  if [[ -f "$AIDER_BIN_DST" ]]; then
    if [[ $DRY_RUN -eq 1 ]]; then
      dry "[uninstall] rm $AIDER_BIN_DST"
    else
      rm -f "$AIDER_BIN_DST"
      log_info "removed: $AIDER_BIN_DST"
    fi
  fi
}

# === Summary ===============================================================

print_summary() {
  local mode="$1"
  log_step "summary ($mode)"
  echo "  detected envs : ${DETECTED_ENVS[*]:-<none>}"
  if [[ $UNINSTALL -eq 0 ]]; then
    echo "  binary        : $([[ $SKIP_BINARY -eq 1 ]] && echo skipped || echo installed-at \"$BIN_DST\")"
    echo "  aider shim    : $([[ $WITH_AIDER -eq 1 ]] && echo installed-at \"$AIDER_BIN_DST\" || echo skipped)"
    echo
    echo "  Next steps:"
    if [[ " ${DETECTED_ENVS[*]} " == *" claude_code "* ]]; then
      echo "    - Restart Claude Code. In any session, run /wa-mcp to load the full MCP"
      echo "      tool catalog. Or invoke a thin playbook: /wa-send, /wa-monitor,"
      echo "      /wa-recover, /wa-config."
    fi
    if [[ " ${DETECTED_ENVS[*]} " == *" cursor "* ]]; then
      echo "    - Restart Cursor. Open Settings -> MCP Servers; verify 'octo-whatsapp'"
      echo "      is listed and connected."
    fi
    if [[ " ${DETECTED_ENVS[*]} " == *" continue "* ]]; then
      echo "    - Reload VS Code. Continue re-reads config.json on restart."
    fi
    if [[ " ${DETECTED_ENVS[*]} " == *" windsurf "* ]]; then
      echo "    - Restart Windsurf. The octo-whatsapp MCP server should appear in"
      echo "      the MCP panel."
    fi
    if [[ $WITH_AIDER -eq 1 ]]; then
      echo "    - Aider users: 'wa send-text +15551234567 \"hi\"' now works if"
      echo "      $AIDER_BIN_DST is on PATH."
    fi
    if [[ ${#DETECTED_ENVS[@]} -eq 0 ]]; then
      echo "    - No AI-agent environments detected. Re-run after installing"
      echo "      Claude Code / Cursor / Continue.dev / Windsurf."
    fi
  else
    echo "  uninstall complete."
  fi
}

# === Main ==================================================================

detect_platform
detect_envs

if [[ $UNINSTALL -eq 1 ]]; then
  log_step "uninstalling octo-whatsapp"
  uninstall_binary
  for env_name in "${DETECTED_ENVS[@]}"; do
    uninstall_mcp_config "$env_name"
  done
  uninstall_skills
  uninstall_aider_shim
  print_summary "uninstall"
  exit 0
fi

log_step "installing octo-whatsapp"
install_binary
for env_name in "${DETECTED_ENVS[@]}"; do
  emit_mcp_config "$env_name"
done
emit_skills
emit_aider_shim

MODE="install"
[[ $DRY_RUN -eq 1 ]] && MODE="dry-run"
print_summary "$MODE"

exit 0
