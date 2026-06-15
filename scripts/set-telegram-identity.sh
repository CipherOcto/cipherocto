#!/usr/bin/env bash
# scripts/set-telegram-identity.sh
#
# Manually patch user_id and username into the Telegram config and session
# sidecar. Use this when the automatic get_me() flow fails or times out
# (e.g., TDLib is idle but never sends a response to getMe) and you need
# to populate the identity fields without re-authenticating.
#
# Why this script exists:
#   The TDLib session at $PERSIST_DIR/data/ is fully valid (auth reached
#   Ready) but the identity-fetch step (get_me) is unreliable in some
#   Docker/network environments. The config and sidecar are written with
#   user_id=0/username=null, which the adapter can detect and refuse. This
#   script lets you fill in the real values from outside the adapter.
#
# Usage:
#   scripts/set-telegram-identity.sh --user-id 123456789 --handle your_handle
#   TELEGRAM_USER_ID=123456789 TELEGRAM_USERNAME=your_handle \
#     scripts/set-telegram-identity.sh
#   scripts/set-telegram-identity.sh   # interactive: prompts for both
#
# Finding your user_id and username:
#   - user_id: open Telegram on your phone, Settings → tap your profile
#     photo at the top, the numeric ID is at the bottom of the profile
#     card (8-10 digits, no @ prefix).
#   - username: Settings → Edit Profile → Username. Drop the @ if any.
#   - Or message @userinfobot on Telegram — it replies with both.
#   - Or use the Telegram Web app and look at the URL of your own profile
#     page: https://web.telegram.org/a/#123456789
#
# What it patches:
#   1. $PERSIST_DIR/telegram.json         — main config
#   2. $PERSIST_DIR/data/session_meta.json — sidecar used by 'session list'
#
# Safety:
#   - Atomic write (tempfile + mv) so a crash mid-write can't leave a
#     half-written config.
#   - Validates user_id is a positive integer.
#   - Does NOT touch the TDLib database at data/database/ — the session
#     there is independent of these two JSONs.

set -euo pipefail

# === Defaults ===

PERSIST_DIR="${TELEGRAM_PERSIST_DIR:-$HOME/.local/share/octo/telegram/persistent}"
USER_ID="${TELEGRAM_USER_ID:-}"
USERNAME="${TELEGRAM_USERNAME:-}"

# === Argument parsing ===

usage() {
  cat <<EOF
Usage: $0 [--user-id ID] [--handle USERNAME] [--persist-dir DIR]

Options:
  --user-id ID        Telegram numeric user ID (e.g. 123456789)
  --handle USERNAME   Telegram username without the @ (e.g. your_handle)
  --persist-dir DIR   Path to the persistent dir (default: $HOME/.local/share/octo/telegram/persistent)
  -h, --help          Show this help

Environment variables (used as fallback when flags are absent):
  TELEGRAM_USER_ID, TELEGRAM_USERNAME, TELEGRAM_PERSIST_DIR
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --user-id)    USER_ID="$2"; shift 2 ;;
    --handle)     USERNAME="$2"; shift 2 ;;
    --persist-dir) PERSIST_DIR="$2"; shift 2 ;;
    -h|--help)    usage; exit 0 ;;
    *) echo "error: unknown arg: $1" >&2; usage >&2; exit 1 ;;
  esac
done

# === Validate persist dir ===

if [[ ! -d "$PERSIST_DIR" ]]; then
  echo "error: persist dir does not exist: $PERSIST_DIR" >&2
  echo "  create it first or pass --persist-dir" >&2
  exit 1
fi

CONFIG_FILE="$PERSIST_DIR/telegram.json"
META_FILE="$PERSIST_DIR/data/session_meta.json"

if [[ ! -f "$CONFIG_FILE" ]]; then
  echo "error: config not found: $CONFIG_FILE" >&2
  echo "  run the auth flow first to create the config" >&2
  exit 1
fi

# === Interactive fallback ===

if [[ -z "$USER_ID" ]]; then
  echo "Telegram user_id (8-10 digit number from Settings → your profile):" >&2
  read -r -p "> " USER_ID
fi

if [[ -z "$USERNAME" ]]; then
  echo "Telegram username without @ (can be empty if you have no username):" >&2
  read -r -p "> " USERNAME
fi

# === Validate ===

if ! [[ "$USER_ID" =~ ^[0-9]+$ ]] || [[ "$USER_ID" -le 0 ]]; then
  echo "error: user_id must be a positive integer, got: $USER_ID" >&2
  exit 1
fi

# Sanitize username: strip leading @, allow empty, alphanumeric + underscore
USERNAME="${USERNAME#@}"
if [[ -n "$USERNAME" ]] && ! [[ "$USERNAME" =~ ^[A-Za-z][A-Za-z0-9_]{3,31}$ ]]; then
  echo "warning: '$USERNAME' doesn't look like a valid Telegram username" >&2
  echo "  (Telegram rules: 5-32 chars, alphanumeric + underscore, must start with a letter)" >&2
  echo "  continuing anyway — set to empty to skip" >&2
  read -r -p "  use '$USERNAME' as-is? [y/N] " confirm
  if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
    echo "aborted" >&2
    exit 1
  fi
fi

# === Patch telegram.json ===

echo "patching $CONFIG_FILE"

# Build the patch as a tiny Python program so we don't need jq and can
# preserve the existing field order. The key insight: if user_id or
# username fields don't exist (the R16.3 partial-session case), we add
# them; if they exist with stale values, we overwrite.
python3 - "$CONFIG_FILE" "$USER_ID" "$USERNAME" <<'PY'
import json, sys, os
path, user_id, username = sys.argv[1], int(sys.argv[2]), sys.argv[3]
with open(path) as f:
    data = json.load(f)
data['user_id'] = user_id
data['username'] = username if username else None
# Atomic write: tmp in same dir, fsync, rename.
tmp = path + '.tmp'
with open(tmp, 'w') as f:
    json.dump(data, f, indent=2)
    f.flush()
    os.fsync(f.fileno())
os.replace(tmp, path)
# Restrict to 0600 — the config may eventually hold the bot token too.
os.chmod(path, 0o600)
PY

echo "  user_id  = $USER_ID"
echo "  username = ${USERNAME:-<empty>}"

# === Patch session_meta.json (best-effort) ===

if [[ -f "$META_FILE" ]]; then
  echo "patching $META_FILE"
  python3 - "$META_FILE" "$USER_ID" "$USERNAME" <<'PY'
import json, sys, os
path, user_id, username = sys.argv[1], int(sys.argv[2]), sys.argv[3]
with open(path) as f:
    data = json.load(f)
data['user_id'] = user_id
data['username'] = username if username else None
tmp = path + '.tmp'
with open(tmp, 'w') as f:
    json.dump(data, f, indent=2)
    f.flush()
    os.fsync(f.fileno())
os.replace(tmp, path)
os.chmod(path, 0o600)
PY
  echo "  user_id  = $USER_ID"
  echo "  username = ${USERNAME:-<empty>}"
else
  echo "note: $META_FILE not found, skipped (run 'session verify' to regenerate)" >&2
fi

# === Summary ===

echo
echo "done. verify with:"
echo "  cat $CONFIG_FILE | python3 -m json.tool"
echo "  scripts/telegram-onboard-qr.sh session verify $PERSIST_DIR/data"
