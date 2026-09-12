#!/usr/bin/env bash
# dangerous-tokens.sh — SINGLE SOURCE OF TRUTH for what constitutes a
# dangerous token in git commit context.
#
# ALL layers (commit-msg hook, pre-receive hook, safe-commit wrapper,
# claude-bash-guard PreToolUse hook, CI lint workflow, memory card)
# MUST source this file. Centralizing the token list prevents drift —
# the 2026-09-08 incident was caused by inconsistent coverage between
# hook and wrapper. See [[no-backtick-in-commit-messages]].
#
# Usage:
#   source /path/to/dangerous-tokens.sh
#   if dangerous_present "$text"; then ...; fi
#
# === TOKEN FORMAT ===
# Each entry is:  TOKEN===CATEGORY===RATIONALE
# Use === (not |) because tokens contain | characters (|, ||, etc).
#
# Categories:
#   SHELL — bash metacharacter that triggers expansion in double-quoted -m
#   GIT_BYPASS — git flag that disables hooks (Layer 7 must block)
#   GIT_LOWLEVEL — git subcommand that bypasses hooks entirely
#   CONFIG_INJECT — git -c config that overrides hook path/editor
#   ENV_INJECT — environment variable that changes git behavior
#   WRAPPER — shell construct that wraps and re-invokes git commit
#   GH_CLI — GitHub CLI subcommand that mutates github.com (bypasses git)
#   UNICODE — invisible/homoglyph characters used to disguise metachars
#
# If you add an entry, add a test case to scripts/test-dangerous-tokens.sh.

DANGEROUS_TOKONS=(
  # === SHELL metacharacters (bash POSIX §2.6) ===
  '`===SHELL===backtick command substitution (POSIX §2.6.3)'
  '$(===SHELL===command substitution dollar-paren'
  '${===SHELL===parameter expansion braces'
  '&&===SHELL===AND-AND command chain'
  '||===SHELL===OR-OR command chain'
  '|===SHELL===pipe (matched with whitespace boundary in dangerous_match)'
  $'\x3b===SHELL===semicolon command separator'
  '>===SHELL===redirect stdout (matched at line start in dangerous_match)'
  '<===SHELL===redirect stdin (matched at line start)'
  '!===SHELL===history expansion (matched at line start in interactive bash)'
  '\===SHELL===backslash escape (can hide metachars from grep)'

  # === GIT hook bypass ===
  '--no-verify===GIT_BYPASS===skips ALL local hooks (commit-msg, pre-commit)'
  '--no-post-hook===GIT_BYPASS===skips server-side post-receive hooks'
  '--no-pre-receive-hook===GIT_BYPASS===skips server-side pre-receive'

  # === GIT low-level subcommands that bypass hooks ===
  'git commit-tree===GIT_LOWLEVEL===low-level commit creation, skips all hooks'
  'git fast-import===GIT_LOWLEVEL===bulk import, no hook invocation'
  'git send-pack===GIT_LOWLEVEL===raw protocol remote-ref write, no server hooks'
  'git push --mirror===GIT_LOWLEVEL===mirror push propagates all refs incl. tags'
  'git push --force===GIT_LOWLEVEL===force push rewrites remote history'
  'git filter-branch===GIT_LOWLEVEL===history rewrite can introduce bad msgs'
  'git bundle===GIT_LOWLEVEL===bundle file can be cloned with bad commits'
  # Plain `git push <remote> <refspec>` — the ACTUAL aggressor in the
  # 2026-09-08 incident (committed as `git push origin next`). The --mirror
  # and --force entries above are subsumed by this substring but kept for
  # documentation value (rationale per variant). The rationale is a
  # single-quoted string — bash treats it as literal, so this entry is
  # not a self-substitution vector.
  'git push===GIT_LOWLEVEL===plain push (ACTUAL INCIDENT VECTOR 2026-09-08: git push origin next)'

  # === GH_CLI — GitHub CLI commands that mutate remote state ===
  # These bypass git entirely. The ACTUAL aggressor in the 2026-09-08
  # incident was `gh pr create next:main`. Layer 7 catches via substring
  # match; any `gh <noun> <verb>` that mutates github.com is listed here.
  'gh pr create===GH_CLI===create PR (ACTUAL INCIDENT VECTOR 2026-09-08: gh pr create next:main)'
  'gh pr merge===GH_CLI===merge PR (mutates remote history, bypasses pre-receive hooks)'
  'gh pr close===GH_CLI===close PR (mutates remote state)'
  'gh pr reopen===GH_CLI===reopen PR (mutates remote state)'
  'gh issue create===GH_CLI===create issue (mutates remote state)'
  'gh issue close===GH_CLI===close issue (mutates remote state)'
  'gh release create===GH_CLI===create release (mutates remote tags/assets)'
  'gh repo create===GH_CLI===create repo (mutates remote org)'
  'gh workflow run===GH_CLI===trigger workflow (mutates remote CI state)'
  'gh api===GH_CLI===raw GitHub API call (arbitrary remote mutation vector)'

  # === CONFIG_INJECT — git -c key=value overrides ===
  'core.hooksPath===CONFIG_INJECT===redirects hook directory'
  'core.editor===CONFIG_INJECT===executes arbitrary editor'
  'alias.commit===CONFIG_INJECT===alias that overrides commit subcommand'
  'alias.ci===CONFIG_INJECT===common alias for commit, may be shell wrapper'

  # === ENV_INJECT — environment variables that change git behavior ===
  'GIT_EDITOR===ENV_INJECT===editor injection'
  'GIT_SEQUENCE_EDITOR===ENV_INJECT===sequence editor (rebase) injection'
  'GIT_COMMITTER_NAME===ENV_INJECT===author name injection'
  'GIT_AUTHOR_NAME===ENV_INJECT===author name injection'
  'GIT_EXTERNAL_DIFF===ENV_INJECT===external diff command injection'

  # === WRAPPER — shell constructs that re-invoke git commit ===
  'eval ===WRAPPER===eval re-invokes the wrapped command in a fresh shell'
  'xargs ===WRAPPER===xargs re-invokes the wrapped command'
  'bash -c===WRAPPER===subshell execution'
  'sh -c===WRAPPER===subshell execution (POSIX sh)'
  # env + git is checked separately below via regex (avoids false positives
  # on shebang lines like `#!/usr/bin/env bash`).

  # === UNICODE homoglyph / invisible characters (Gap H) ===
  # U+200B ZERO WIDTH SPACE
  $' ​===UNICODE===zero-width space (U+200B) — can break token scans'
  # U+200C ZERO WIDTH NON-JOINER
  $' ‌===UNICODE===zero-width non-joiner (U+200C)'
  # U+200D ZERO WIDTH JOINER
  $' ‍===UNICODE===zero-width joiner (U+200D)'
  # U+200E LEFT-TO-RIGHT MARK
  $' ‎===UNICODE===left-to-right mark (U+200E)'
  # U+200F RIGHT-TO-LEFT MARK
  $' ‏===UNICODE===right-to-left mark (U+200F)'
  # U+202A..U+202E directional formatting
  $' ‪===UNICODE===left-to-right embedding (U+202A)'
  $' ‫===UNICODE===right-to-left embedding (U+202B)'
  $' ‬===UNICODE===pop directional formatting (U+202C)'
  $' ‭===UNICODE===left-to-right override (U+202D)'
  $' ‮===UNICODE===right-to-left override (U+202E) — REVERSES token order visually'
  # U+FEFF BYTE ORDER MARK / zero-width no-break space
  $' ﻿===UNICODE===byte order mark / ZWNBSP (U+FEFF)'
  # U+180E MONGOLIAN VOWEL SEPARATOR
  $' ᠎===UNICODE===mongolian vowel separator (U+180E)'
)

# === Match function ===
# dangerous_present <text>
#   - returns 0 if any dangerous token found, 1 otherwise
#   - prints offending tokens to stdout (one per line)
dangerous_present() {
  local text="$1"
  local hit=()

  # Strip CRLF first so multi-line payloads don't hide from per-line scan.
  text="${text//$'\r'/}"

  # Scan each token (use === as separator so pipe-containing tokens work).
  for entry in "${DANGEROUS_TOKONS[@]}"; do
    # Skip entries that don't have the === separator (defensive).
    [[ "$entry" != *"==="* ]] && continue
    local token="${entry%%===*}"
    local cat="${entry#*===}"
    cat="${cat%%===*}"

    # Skip empty tokens (defensive).
    [[ -z "$token" ]] && continue

    case "$cat" in
      SHELL)
        case "$token" in
          '|')
            [[ "$text" =~ [[:space:]]\|[[:space:]] ]] && hit+=("pipe")
            ;;
          '>')
            [[ "$text" =~ $'^\>' ]] && hit+=("leading>")
            ;;
          '<')
            [[ "$text" =~ $'^\<' ]] && hit+=("leading<")
            ;;
          '!')
            [[ "$text" =~ $'^\!' ]] && hit+=("leading!")
            ;;
          '\')
            [[ "$text" == *'\'* ]] && hit+=("backslash")
            ;;
          *)
            [[ "$text" == *"$token"* ]] && hit+=("$token")
            ;;
        esac
        ;;
      *)
        # Substring match for flags, commands, env, wrappers.
        [[ "$text" == *"$token"* ]] && hit+=("$token")
        ;;
    esac
  done

  # === Bare $-expansion forms (not covered above) ===
  # $VAR (identifier), $N (positional), $? $# $* $$ etc.
  [[ "$text" =~ \$[A-Za-z_][A-Za-z0-9_]* ]] && hit+=('\$VAR')
  [[ "$text" =~ \$[0-9]+ ]] && hit+=('\$N')
  [[ "$text" =~ \$\? ]] && hit+=('\$?')
  [[ "$text" =~ \$\# ]] && hit+=('\$#')
  [[ "$text" =~ \$\* ]] && hit+=('\$*')
  [[ "$text" =~ \$\$ ]] && hit+=('\$\$')

  # === Strip zero-width chars and re-check for unicode-only attempts ===
  local stripped="$text"
  stripped="${stripped//‌/}"
  stripped="${stripped//‍/}"
  stripped="${stripped//‎/}"
  stripped="${stripped//‏/}"
  stripped="${stripped//‪/}"
  stripped="${stripped//‫/}"
  stripped="${stripped//‬/}"
  stripped="${stripped//‭/}"
  stripped="${stripped//‮/}"
  stripped="${stripped//﻿/}"
  stripped="${stripped//﻿/}"
  if [[ "$stripped" != "$text" ]]; then
    # Unicode was stripped — that itself means unicode was present.
    hit+=("unicode-invisible")
  fi

  # === env-prefixed git (Gap E variant) ===
  # Match `env` (start or whitespace-boundary) optionally followed by
  # flags (-i, -u VAR, etc.) and then `git`. Excludes shebang lines
  # `#!/usr/bin/env bash` which have `env` preceded by `/`.
  if [[ "$text" =~ (^|[[:space:]])env[[:space:]]+(-[a-zA-Z]+[[:space:]]+)*git([[:space:]]|$) ]]; then
    hit+=("env-wrap-git")
  fi

  # === Base64/hex encoded payloads (Gap I) ===
  # If text contains a 32+ char base64-looking blob, decode + re-scan.
  if [[ "$text" =~ ([A-Za-z0-9+/=]{32,}) ]]; then
    local blob="${BASH_REMATCH[1]}"
    if [[ "$blob" =~ [A-Z] ]] && [[ "$blob" =~ [a-z] ]] \
       && (echo "$blob" | base64 -d 2>/dev/null | grep -qE '`|\$\(|;|&&|\|\|'); then
      hit+=("base64-encoded-payload")
    fi
  fi

  if [[ ${#hit[@]} -gt 0 ]]; then
    printf '%s\n' "${hit[@]}"
    return 0
  fi
  return 1
}

# Convenience: dangerous_present_silent — same but no stdout.
dangerous_present_silent() {
  dangerous_present "$1" >/dev/null 2>&1
}
