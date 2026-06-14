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
#
# Defaults:
#   If TELEGRAM_API_ID / TELEGRAM_API_HASH are unset, this script
#   uses TDesktop's currently-registered api_id/api_hash pair
#   (sourced from /home/mmacedoeu/_w/tools/tdesktop at commit
#   e505b391e1, Telegram/SourceFiles/config.h:88-89). Override
#   by exporting either var before running this script — e.g.:
#     export TELEGRAM_API_ID=12345
#     export TELEGRAM_API_HASH=my_own_32_char_hex
#     ./scripts/telegram-onboard-qr.sh
#   Note: using TDesktop's values violates Telegram API Terms §4
#   (must use your own credentials). Fine for personal one-shots;
#   register your own at https://my.telegram.org/apps for anything
#   you intend to keep running.

set -euo pipefail

# === Defaults (TDesktop mainline, config.h:88-89 @ e505b391e1) ===

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

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker not found in PATH" >&2
  echo "  install Docker: https://docs.docker.com/engine/install/" >&2
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
  -e HOME=/home/ci \
  -e CARGO_HOME=/home/ci/.cargo \
  -e RUSTUP_HOME=/home/ci/.rustup \
  -v "$HOST_PERSIST:/octo-state" \
  -v "$HOST_WORKSPACE:/workspace" \
  -v "$HOST_CARGO:/home/ci/.cargo" \
  -v "$HOST_RUSTUP:/home/ci/.rustup" \
  -e TELEGRAM_API_ID \
  -e TELEGRAM_API_HASH \
  -w /workspace \
  ubuntu:24.04 \
  bash -c '
    set -e
    mkdir -p /home/ci
    chown -R 1000:1000 /home/ci
    # Add a uid-1000 user entry so runuser can switch to it. The
    # ubuntu:24.04 base image has a "ubuntu" user at uid 1000 already,
    # but adding a second entry with the same uid lets us name it
    # "ci" for clarity and ensures the home dir is correct.
    echo "ci:x:1000:" >> /etc/group
    echo "ci:x:1000:1000::/home/ci:/bin/bash" >> /etc/passwd
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >/dev/null
    apt-get install -y -qq libc++1 libc++abi1 build-essential pkg-config libssl-dev cmake >/dev/null
    exec runuser -u ci -- bash -c "
      export PATH=\"\$HOME/.cargo/bin:\$PATH\"
      rustup default 1.92 2>&1 | tail -1
      cargo build -p octo-telegram-onboard --release 2>&1 | tail -1
      LIBDIR=\$(find /workspace/target/release/build/tdlib-rs-*/out/tdlib/lib -name 'libtdjson.so.1.8.61' 2>/dev/null | head -1 | xargs dirname)
      export LD_LIBRARY_PATH=\"\$LIBDIR\"
      mkdir -p /octo-state/data
      exec /workspace/target/release/octo-telegram-onboard qr-link \
        --data-dir /octo-state/data \
        --out /octo-state/telegram.json \
        --force
    "
  '
