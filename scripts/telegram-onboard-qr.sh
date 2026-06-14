#!/usr/bin/env bash
# scripts/telegram-onboard-qr.sh
#
# Run the Telegram onboarding CLI in QR-code login mode inside an
# Ubuntu 24.04 Docker container. The CLI renders a Unicode half-block
# QR code in the terminal; scan it from your already-logged-in
# Telegram app on another device (Settings → Devices → Link Desktop).
#
# Usage:
#   export TELEGRAM_API_ID=12345
#   export TELEGRAM_API_HASH=abc123def456...
#   ./scripts/telegram-onboard-qr.sh
#
# Prerequisites:
#   - docker (any recent version)
#   - A TTY (real terminal, not a pipe — the QR renders as
#     Unicode half-block characters)
#   - Telegram API credentials from https://my.telegram.org/apps
#     (any app name, "Desktop" platform is fine)
#
# Persistence model:
#   The host directory ~/.local/share/octo/telegram/persistent/ is
#   mounted at /octo-state inside the container. Both TDLib's
#   persistent database (data_dir/database + data_dir/files) and
#   the TelegramConfig JSON end up here, so re-runs reuse the
#   session without rescanning.
#
#   After successful auth, the host will have:
#     ~/.local/share/octo/telegram/persistent/telegram.json
#       The TelegramConfig JSON (consumed by octo-adapter-telegram)
#     ~/.local/share/octo/telegram/persistent/data/
#       TDLib's persistent database (auth keys, key data)
#     ~/.local/share/octo/telegram/persistent/data.meta.json
#       Session sidecar (self_phone, user_id, linked_at)
#
#   On the second run, TDLib finds the existing session in the
#   database and skips the QR step — you'll see it briefly, the
#   CLI rewrites the JSON with fresh self_phone/user_id, and exits.
#
# PATH CAVEAT:
#   The JSON's data_dir field is written with the *container* path
#   (/octo-state/data). If octo-adapter-telegram runs on the host
#   (not in the same container), it won't find that path. Two fixes:
#     (a) Run the adapter inside the same container with the same
#         mount (paths match).
#     (b) After docker exits, sed-replace the path:
#         sed -i 's|/octo-state/data|/home/<you>/.local/share/octo/telegram/persistent/data|' \
#           ~/.local/share/octo/telegram/persistent/telegram.json
#
# Time cost:
#   First run:  ~30s apt + ~5min cargo build (cold cache) + auth
#   Subsequent: ~5s (cargo cache is hot, just relinks) + auth

set -euo pipefail

# === Prerequisite checks ===

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker not found in PATH" >&2
  echo "  install Docker: https://docs.docker.com/engine/install/" >&2
  exit 1
fi

if [[ -z "${TELEGRAM_API_ID:-}" ]]; then
  echo "error: TELEGRAM_API_ID env var is not set" >&2
  echo "  get API credentials at https://my.telegram.org/apps" >&2
  exit 1
fi

if [[ -z "${TELEGRAM_API_HASH:-}" ]]; then
  echo "error: TELEGRAM_API_HASH env var is not set" >&2
  echo "  get API credentials at https://my.telegram.org/apps" >&2
  exit 1
fi

# Soft warning for non-TTY (don't fail — scripted use might be valid
# for some users, and the QR will still render in most terminals).
if [[ ! -t 0 || ! -t 1 ]]; then
  echo "warning: stdin/stdout is not a TTY; QR code may not render correctly" >&2
  echo "  run from a real terminal, not piped or redirected" >&2
fi

# === Resolve host paths relative to this script's location ===

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOST_WORKSPACE="$(cd "$SCRIPT_DIR/.." && pwd)"
HOST_PERSIST="$HOME/.local/share/octo/telegram/persistent"
HOST_CARGO="$HOME/.cargo"
HOST_RUSTUP="$HOME/.rustup"

mkdir -p "$HOST_PERSIST"

# === Pull base image lazily (cached after first run) ===

if ! docker image inspect ubuntu:24.04 >/dev/null 2>&1; then
  echo "Pulling ubuntu:24.04 image..."
  docker pull ubuntu:24.04
fi

# === Run the container ===
#
# Flags:
#   -it               — allocate a TTY for the QR code
#   --rm              — clean up the container on exit
#   -v PERSIST:…      — mount the host persistence dir at /octo-state
#   -v WORKSPACE:…    — mount the repo so cargo can see the source
#   -v CARGO:…        — mount the host's cargo cache (cold → hot builds)
#   -v RUSTUP:…       — mount the host's rustup (toolchain install)
#   -e …              — propagate the API credentials to the container
#   -w /workspace     — set the working dir
#   exec … qr-link    — replace the bash process with the CLI so
#                       Ctrl-C goes straight to the CLI signal handler

exec docker run -it --rm \
  -v "$HOST_PERSIST:/octo-state" \
  -v "$HOST_WORKSPACE:/workspace" \
  -v "$HOST_CARGO:/root/.cargo" \
  -v "$HOST_RUSTUP:/root/.rustup" \
  -e TELEGRAM_API_ID \
  -e TELEGRAM_API_HASH \
  -w /workspace \
  ubuntu:24.04 \
  bash -c '
    set -e
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >/dev/null
    apt-get install -y -qq libc++1 libc++abi1 build-essential pkg-config libssl-dev cmake >/dev/null
    export PATH="$HOME/.cargo/bin:$PATH"
    rustup default 1.92 2>&1 | tail -1
    cargo build -p octo-telegram-onboard --release 2>&1 | tail -1
    LIBDIR=$(find /workspace/target/release/build/tdlib-rs-*/out/tdlib/lib -name "libtdjson.so.1.8.61" 2>/dev/null | head -1 | xargs dirname)
    export LD_LIBRARY_PATH="$LIBDIR"
    mkdir -p /octo-state/data
    exec /workspace/target/release/octo-telegram-onboard qr-link \
      --data-dir /octo-state/data \
      --out /octo-state/telegram.json \
      --force
  '
