#!/usr/bin/env bash
# Bring up a local Matrix homeserver (Synapse or Conduit) in Docker for
# the `octo-adapter-matrix-sdk` integration test suite
# (`crates/octo-adapter-matrix-sdk/tests/integration_matrix.rs`).
#
# Usage:
#   scripts/integration-matrix.sh up   [--homeserver synapse|conduit] [--port N]
#   scripts/integration-matrix.sh down [--homeserver synapse|conduit]
#
# Default homeserver: synapse (matrices most-complete reference server).
# Both flavors run with password-only registration and no rate limits,
# matching the test's hard-coded credentials:
#   user:     @ci:localhost
#   password: ci-password
#   room:     !integration-test:localhost
#
# Requirements: docker (the script uses `docker run` and `docker rm`).
# It does NOT require synapse/conduit CLIs — the homeserver config is
# baked into the container via bind-mounts.
#
# Two CI users are provisioned: `ci` (the canonical test user) and
# `ci2` (a second user for two-party tests, e.g. mission 0850h-b's
# encrypted-room round-trip). Both share the password `ci-password`
# for simplicity — the integration tests don't need distinct
# credentials, only two distinct MXIDs.

set -euo pipefail

readonly CI_USER="@ci:localhost"
readonly CI_PASSWORD="ci-password"
# Mission 0850h-b acceptance: encrypted-room round-trip. Requires two
# distinct MXIDs in the same room. The script provisions `ci` (the
# canonical single-user test) and `ci2` (the second user for the
# two-party encrypted-room test). Both share the same password.
readonly CI2_USER="@ci2:localhost"
readonly CI2_PASSWORD="ci-password"
readonly CONTAINER_NAME_DEFAULT="octo-matrix-ci"

HOMESERVER="synapse"
PORT="8008"
ACTION=""

usage() {
  cat <<EOF
Usage: $0 <up|down> [--homeserver synapse|conduit] [--port N]

Commands:
  up     Start a homeserver container, wait for readiness, create the CI user.
  down   Stop and remove the homeserver container.

Options:
  --homeserver {synapse|conduit}   Which server to use (default: synapse).
  --port N                          Port to expose on localhost (default: 8008).

Examples:
  $0 up
  $0 up --homeserver conduit --port 8009
  $0 down
EOF
}

log() {
  printf '[integration-matrix] %s\n' "$*" >&2
}

err() {
  printf '[integration-matrix] ERROR: %s\n' "$*" >&2
}

require_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    err "docker not found in PATH. Install docker or run this script on a host with docker."
    exit 1
  fi
  if ! docker info >/dev/null 2>&1; then
    err "docker daemon is not reachable. Is the docker service running?"
    exit 1
  fi
}

parse_args() {
  if [[ $# -lt 1 ]]; then
    usage
    exit 64
  fi
  ACTION="$1"
  shift
  if [[ "${ACTION}" != "up" && "${ACTION}" != "down" ]]; then
    err "unknown action: ${ACTION}"
    usage
    exit 64
  fi

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --homeserver)
        HOMESERVER="$2"
        shift 2
        ;;
      --port)
        PORT="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        err "unknown argument: $1"
        usage
        exit 64
        ;;
    esac
  done

  if [[ "${HOMESERVER}" != "synapse" && "${HOMESERVER}" != "conduit" ]]; then
    err "--homeserver must be 'synapse' or 'conduit' (got: ${HOMESERVER})"
    exit 64
  fi
}

# Render a Synapse homeserver.yaml with registration enabled, no rate
# limits, and a single shared secret. The integration test only needs
# one user, so we use the shared-secret registration API to create it.
render_synapse_config() {
  local dir="$1"
  mkdir -p "${dir}"
  cat >"${dir}/homeserver.yaml" <<YAML
server_name: "localhost"
pid_file: /data/homeserver.pid
listeners:
  - port: 8008
    bind_addresses: ["0.0.0.0"]
    type: http
    tls: false
    x_forwarded: false
    resources:
      - names: [client, federation]
        compress: false
database:
  name: sqlite3
  args:
    database: "/data/homeserver.db"
log_config: "/data/${CI_LOG_CONFIG}"
media_store_path: "/data/media_store"
uploads_path: "/data/uploads"
max_upload_size: 60M
max_image_pixels: 32M
dynamic_thumbnails: false
enable_registration: true
enable_registration_without_verification: true
registration_shared_secret: "ci-shared-secret"
bcrypt_rounds: 4
allow_guest_access: false
enable_metrics: false
report_stats: false
serve_server_wellknown: true
YAML
  cat >"${dir}/${CI_LOG_CONFIG}" <<YAML
version: 1
formatters:
  precise:
    format: '%(asctime)s - %(name)s - %(lineno)d - %(levelname)s - %(request)s - %(message)s'
handlers:
  console:
    class: logging.StreamHandler
    formatter: precise
    stream: ext://sys.stderr
loggers:
  synapse:
    level: WARNING
  synapse.storage.SQL:
    level: WARNING
root:
  level: WARNING
  handlers: [console]
YAML
  printf '%s' "${CI_LOG_CONFIG}"
}

# Wait until the homeserver answers /_matrix/client/versions on the
# client port. Polls every second, gives up after 60s.
wait_for_readiness() {
  local base_url="$1"
  local attempts=60
  log "waiting for ${base_url} to be ready..."
  for ((i = 1; i <= attempts; i++)); do
    if curl -fsS -o /dev/null "${base_url}/_matrix/client/versions"; then
      log "homeserver is ready (after ${i}s)"
      return 0
    fi
    sleep 1
  done
  err "homeserver did not become ready within ${attempts}s"
  err "check container logs: docker logs ${CONTAINER_NAME}"
  return 1
}

# Register a CI user via Synapse's shared-secret admin API and set
# the password. Synapse uses a non-standard endpoint; we hit the
# /_synapse/admin/v1/register path. Argument: the local-part of
# the username to create (e.g., "ci" → @ci:localhost). Caller is
# responsible for invoking this once per user.
synapse_create_user() {
  local username="$1"
  local base_url="http://localhost:${PORT}"
  local nonce
  nonce="$(curl -fsS "${base_url}/_synapse/admin/v1/register" \
    -H 'Content-Type: application/json' \
    -d '{}' | python3 -c 'import sys, json; print(json.load(sys.stdin)["nonce"])')"

  local mac
  mac="$(printf '%s\n%s\n%s\n%s' \
    "a" "b" "c" "${nonce}" \
    | openssl dgst -sha1 -hmac "ci-shared-secret" -hex \
    | awk '{print $NF}')"

  local resp
  resp="$(curl -fsS "${base_url}/_synapse/admin/v1/register" \
    -H 'Content-Type: application/json' \
    -d "$(python3 -c "
import json, sys
print(json.dumps({
  'nonce': '${nonce}',
  'username': '${username}',
  'password': '${CI_PASSWORD}',
  'admin': True,
  'mac': '${mac}',
}))
")")"
  log "synapse user created: $(printf '%s' "${resp}" | python3 -c 'import sys, json; d=json.load(sys.stdin); print(d["user_id"])' 2>/dev/null || echo ok)"
}

# Conduit ships with open registration by default; create the user
# with the standard /_matrix/client/v3/register endpoint. Argument:
# the local-part of the username to create.
conduit_create_user() {
  local username="$1"
  local base_url="http://localhost:${PORT}"
  local resp
  resp="$(curl -fsS "${base_url}/_matrix/client/v3/register" \
    -H 'Content-Type: application/json' \
    -d "$(python3 -c "
import json
print(json.dumps({
  'auth': {'type': 'm.login.dummy'},
  'username': '${username}',
  'password': '${CI_PASSWORD}',
}))
")")"
  log "conduit user created: $(printf '%s' "${resp}" | python3 -c 'import sys, json; d=json.load(sys.stdin); print(d["user_id"])' 2>/dev/null || echo ok)"
}

up_synapse() {
  local workdir
  workdir="$(mktemp -d -t octo-matrix-synapse-XXXXXX)"
  trap 'rm -rf "${workdir}"' EXIT
  readonly CI_LOG_CONFIG="log.config"
  local log_config_name
  log_config_name="$(render_synapse_config "${workdir}")"

  log "starting synapse container on port ${PORT}"
  docker run -d --rm \
    --name "${CONTAINER_NAME}" \
    -p "${PORT}:8008" \
    -v "${workdir}:/data" \
    -e SYNAPSE_SERVER_NAME=localhost \
    -e SYNAPSE_REPORT_STATS=no \
    matrixdotorg/synapse:latest \
    >/dev/null

  wait_for_readiness "http://localhost:${PORT}"
  synapse_create_user "ci"
  synapse_create_user "ci2"
  log "synapse is up at http://localhost:${PORT} (users ${CI_USER}, ${CI2_USER} / password ${CI_PASSWORD})"
  log "run the integration test with:"
  log "  cargo test -p octo-adapter-matrix-sdk --features integration-matrix --test integration_matrix -- --nocapture"
}

up_conduit() {
  log "starting conduit container on port ${PORT}"
  docker run -d --rm \
    --name "${CONTAINER_NAME}" \
    -p "${PORT}:8008" \
    -e CONDUIT_SERVER_NAME=localhost \
    -e CONDUIT_ALLOW_REGISTRATION=true \
    -e CONDUIT_ALLOW_FEDERATION=false \
    -e CONDUIT_ADDRESS=0.0.0.0:8008 \
    matrixconduit/matrix-conduit:latest \
    >/dev/null

  wait_for_readiness "http://localhost:${PORT}"
  conduit_create_user "ci"
  conduit_create_user "ci2"
  log "conduit is up at http://localhost:${PORT} (users ${CI_USER}, ${CI2_USER} / password ${CI_PASSWORD})"
  log "run the integration test with:"
  log "  cargo test -p octo-adapter-matrix-sdk --features integration-matrix --test integration_matrix -- --nocapture"
}

down() {
  if docker ps -a --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    log "removing container ${CONTAINER_NAME}"
    docker rm -f "${CONTAINER_NAME}" >/dev/null
  else
    log "no container named ${CONTAINER_NAME}; nothing to do"
  fi
}

main() {
  parse_args "$@"
  require_docker
  CONTAINER_NAME="${CONTAINER_NAME_DEFAULT}-${HOMESERVER}"
  case "${ACTION}" in
    up)
      case "${HOMESERVER}" in
        synapse) up_synapse ;;
        conduit) up_conduit ;;
      esac
      ;;
    down)
      down
      ;;
  esac
}

main "$@"
