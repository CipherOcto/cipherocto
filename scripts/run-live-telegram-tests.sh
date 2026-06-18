#!/usr/bin/env bash
# scripts/run-live-telegram-tests.sh
#
# Run the live Telegram integration tests inside an Ubuntu 24.04 Docker
# container. Loads the existing authenticated session from the mounted
# TDLib database and exercises it through the real adapter.
#
# What it runs:
#   - RealTelegramClient::new(config) — auto-loads the auth key from
#     TELEGRAM_DATA_DIR/database/, waits for Ready, calls get_me
#   - TelegramAdapter::with_self_handle(...) — shares the populated
#     SelfHandle for self-loop filtering
#   - 3 #[ignore]'d tests:
#       live_session_health_check
#       live_session_get_me_returns_real_identity
#       live_session_domain_id_round_trip
#
# Usage:
#   export TELEGRAM_PHONE="+15551234567"
#   ./scripts/run-live-telegram-tests.sh
#
# Or pass via flag:
#   ./scripts/run-live-telegram-tests.sh --phone "+15551234567"
#
# Prerequisite:
#   - An existing authenticated TDLib session at
#     $HOME/.local/share/octo/telegram/persistent/data/ (created by
#     scripts/telegram-onboard-qr.sh qr-link).
#   - Or manually patched telegram.json + session_meta.json (see
#     scripts/set-telegram-identity.sh).
#
# Time cost:
#   First run:  ~30s apt + ~3min cargo build (cold, with real-tdlib feature)
#   Subsequent: ~30s cargo relink + ~10s test execution
#
# Defaults (TDesktop mainline, config.h:88-89 @ e505b391e1):
#   api_id=17349, api_hash=344583e45741c457fe1862106095a5eb
# Override by exporting TELEGRAM_API_ID / TELEGRAM_API_HASH.

set -euo pipefail

# TELEGRAM_PHONE is OPTIONAL — the test reads it from the on-disk
# telegram.json (written by the auth flow) and uses the env var only
# as an override. Pass --phone or export TELEGRAM_PHONE to override
# the phone in the config without re-running the auth flow.
PHONE="${TELEGRAM_PHONE:-}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --phone) PHONE="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [--phone +15551234567]"
      echo "Env: TELEGRAM_PHONE (optional override), TELEGRAM_API_ID, TELEGRAM_API_HASH"
      echo ""
      echo "The test reads config from the mounted telegram.json."
      echo "TELEGRAM_PHONE is only needed if the config is missing the phone field."
      exit 0
      ;;
    *) echo "error: unknown arg: $1" >&2; exit 1 ;;
  esac
done

# === Defaults ===

if [[ -z "${TELEGRAM_API_ID+x}" ]]; then
  echo "notice: TELEGRAM_API_ID not set, using TDesktop default (17349)" >&2
  TELEGRAM_API_ID=17349
fi
if [[ -z "${TELEGRAM_API_HASH+x}" ]]; then
  echo "notice: TELEGRAM_API_HASH not set, using TDesktop default" >&2
  TELEGRAM_API_HASH=344583e45741c457fe1862106095a5eb
fi
export TELEGRAM_API_ID TELEGRAM_API_HASH
if [[ -n "$PHONE" ]]; then
  export TELEGRAM_PHONE
fi
export TELEGRAM_MODE=user

# === Prereq checks ===

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker not found" >&2; exit 1
fi

if [[ ! -d "$HOME/.local/share/octo/telegram/persistent/data/database" ]]; then
  echo "error: no TDLib session found at $HOME/.local/share/octo/telegram/persistent/data/database" >&2
  echo "  run scripts/telegram-onboard-qr.sh qr-link first" >&2
  exit 1
fi

# === Paths ===

HOST_PERSIST="$HOME/.local/share/octo/telegram/persistent"
HOST_WORKSPACE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOST_CARGO="$HOME/.cargo"
HOST_RUSTUP="$HOME/.rustup"
LOG_FILE="$HOST_PERSIST/live_tests.log"

if ! docker image inspect ubuntu:24.04 >/dev/null 2>&1; then
  echo "Pulling ubuntu:24.04 image..."
  docker pull ubuntu:24.04
fi

# === Run the container ===
#
# Flags:
#   --rm                            clean up container on exit
#   -v PERSIST:…                    mount the TDLib session at /octo-state
#   -v WORKSPACE:…                  mount the repo so cargo sees the source
#   -v CARGO:…                      mount the host cargo cache (cold → hot)
#   -v RUSTUP:…                     mount the host rustup (toolchain)
#   -e TELEGRAM_*                   propagate test config to the container
#   -w /workspace                   set working dir
#   2>&1 | tee LOG                  mirror all output to live_tests.log
#
# The test binary is invoked with --test-threads=1 because
# tdlib_rs::receive() is process-global; parallel tests would race.

docker run --rm \
  -e HOME=/home/ci \
  -e CARGO_HOME=/home/ci/.cargo \
  -e RUSTUP_HOME=/home/ci/.rustup \
  -e TELEGRAM_MODE \
  -e TELEGRAM_API_ID \
  -e TELEGRAM_API_HASH \
  -e TELEGRAM_PHONE \
  -v "$HOST_PERSIST:/octo-state" \
  -v "$HOST_WORKSPACE:/workspace" \
  -v "$HOST_CARGO:/home/ci/.cargo" \
  -v "$HOST_RUSTUP:/home/ci/.rustup" \
  -w /workspace \
  ubuntu:24.04 \
  bash -c '
    set -e
    mkdir -p /home/ci
    chown -R 1000:1000 /home/ci
    echo "ci:x:1000:" >> /etc/group
    echo "ci:x:1000:1000::/home/ci:/bin/bash" >> /etc/passwd
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq >/dev/null
    apt-get install -y -qq libc++1 libc++abi1 build-essential pkg-config libssl-dev cmake >/dev/null
    exec runuser -u ci -- bash -c "
      export PATH=\"\$HOME/.cargo/bin:\$PATH\"
      # real-tdlib downloads libtdjson.so.1.8.61 into build artifacts.
      # The test binary needs it on LD_LIBRARY_PATH.
      LIBDIR=\$(find /workspace/target/release/build/tdlib-rs-*/out/tdlib/lib -name 'libtdjson.so.1.8.61' 2>/dev/null | head -1 | xargs dirname)
      export LD_LIBRARY_PATH=\"\$LIBDIR:\$LD_LIBRARY_PATH\"
      cargo test -p octo-adapter-telegram \
        --features real-tdlib \
        --test live_session_test \
        -- --include-ignored --nocapture --test-threads=1
    "
  ' 2>&1 | tee "$LOG_FILE"
