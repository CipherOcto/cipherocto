#!/usr/bin/env bash
# scripts/mtproto-onboard-qr.sh
#
# Run the Telegram MTProto onboarding CLI in QR-login mode.
# Pure Rust (grammers) — no Docker, no TDLib, no C++ deps.
#
# The CLI renders a Unicode half-block QR code in the terminal;
# scan it from your already-logged-in Telegram app on your phone
# (Settings → Devices → Link Desktop Device).
#
# Usage:
#   ./scripts/mtproto-onboard-qr.sh
#
# Prerequisites:
#   - rustup + cargo (any recent stable)
#   - A TTY (real terminal for QR rendering)
#   - Telegram API credentials from https://my.telegram.org/apps
#     (any app name, "Desktop" platform is fine)
#
# Defaults:
#   If TELEGRAM_API_ID / TELEGRAM_API_HASH are unset, this script
#   uses TDesktop's currently-registered api_id/api_hash pair.
#   Override by exporting either var before running:
#     export TELEGRAM_API_ID=12345
#     export TELEGRAM_API_HASH=my_own_32_char_hex
#     ./scripts/mtproto-onboard-qr.sh
#
# Persistence model:
#   Session data is stored in:
#     ~/.local/share/octo/telegram-mtproto/
#       config.json        — MtprotoTelegramConfig (consumed by adapter)
#       session.db         — StoolapSession (auth keys, MTProto state)
#       data.meta.json     — Session sidecar (user_id, username, linked_at)
#
#   On re-run, the existing session is detected and the QR step is
#   skipped — the CLI reports the existing identity and exits.
#
# Time cost:
#   First run:  ~3-5min cargo build (cold cache) + auth
#   Subsequent: ~5s (cargo cache is hot) + auth

set -euo pipefail

# === Defaults (TDesktop mainline, config.h:88-89) ===

if [[ -z "${TELEGRAM_API_ID+x}" ]]; then
  echo "notice: TELEGRAM_API_ID not set, using TDesktop default (17349)" >&2
  echo "  override with: export TELEGRAM_API_ID=<your-app-id>" >&2
  TELEGRAM_API_ID=17349
fi

if [[ -z "${TELEGRAM_API_HASH+x}" ]]; then
  echo "notice: TELEGRAM_API_HASH not set, using TDesktop default" >&2
  echo "  override with: export TELEGRAM_API_HASH=<your-32-char-hex>" >&2
  TELEGRAM_API_HASH=344583e45741c457fe1862106095a5eb
fi
export TELEGRAM_API_ID
export TELEGRAM_API_HASH

# === Prerequisite checks ===

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found in PATH" >&2
  echo "  install Rust: https://rustup.rs/" >&2
  exit 1
fi

if ! command -v rustup >/dev/null 2>&1; then
  echo "error: rustup not found in PATH" >&2
  echo "  install Rust: https://rustup.rs/" >&2
  exit 1
fi

# Soft warning for non-TTY
if [[ ! -t 0 || ! -t 1 ]]; then
  echo "warning: stdin/stdout is not a TTY; QR code may not render correctly" >&2
  echo "  run from a real terminal, not piped or redirected" >&2
fi

# === Resolve paths ===

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE="$(cd "$SCRIPT_DIR/.." && pwd)"
DATA_DIR="$HOME/.local/share/octo/telegram-mtproto"
LOG_FILE="$DATA_DIR/onboard.log"

mkdir -p "$DATA_DIR"

echo "=== Telegram MTProto QR Login ===" >&2
echo "workspace: $WORKSPACE" >&2
echo "data_dir:  $DATA_DIR" >&2
echo "api_id:    $TELEGRAM_API_ID" >&2
echo "api_hash:  ${TELEGRAM_API_HASH:0:8}..." >&2
echo "" >&2

# === Build the onboard binary ===

echo "Building octo-telegram-mtproto-onboard (release)..." >&2
cargo build -p octo-telegram-mtproto-onboard --release 2>&1 | tail -3

BINARY="$WORKSPACE/target/release/octo-telegram-mtproto-onboard"

if [[ ! -x "$BINARY" ]]; then
  echo "error: binary not found at $BINARY" >&2
  exit 1
fi

echo "Binary: $BINARY" >&2
echo "" >&2

# === Run QR login ===
#
# The binary handles:
#   - Connecting to Telegram DC via MTProto (pure Rust, grammers)
#   - Exporting a QR login token
#   - Rendering the QR as Unicode half-blocks in the terminal
#   - Polling for scan completion (default: 2s interval, 300s timeout)
#   - Writing config.json + session on success
#   - SIGINT handling (Ctrl-C exits cleanly)

echo "Starting QR login (scan the code with your phone)..." >&2
echo "  Timeout: 300s | Poll: 2s" >&2
echo "" >&2

exec "$BINARY" qr-login \
  --data-dir "$DATA_DIR" \
  --timeout-secs 300 \
  --poll-interval-secs 2 \
  --force \
  "$@" \
  2>&1 | tee "$LOG_FILE"
