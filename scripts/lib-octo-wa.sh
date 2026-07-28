#!/usr/bin/env bash
# scripts/lib-octo-wa.sh
#
# Shared helpers for the flex scripts. Source from each script with:
#   . "$(dirname "$0")/lib-octo-wa.sh"
#
# Provides:
#   mcp_call <jsonrpc-request>          — single RPC, with timeout + retry.
#                                          Retries on transient errors
#                                          (timeout, JSON-RPC -32603,
#                                          daemon reconnecting). Honours
#                                          the WA RPC cooldown
#                                          (memory: whatsapp-rpc-cooldown).
#   mcp_call_sleep <jsonrpc-request>    — same as mcp_call but sleeps
#                                          OCTO_WA_SLEEP seconds after
#                                          the RPC (default 3s).
#   wa_health_check                     — pre-run daemon.health probe;
#                                          aborts if not connected.
#   wa_log <msg>                        — line to stderr with timestamp.
#   wa_now_ms                           — epoch milliseconds.
#
# Env consumed:
#   OCTO_WA_BIN       path to octo-whatsapp binary
#   OCTO_WA_SOCKET    unix socket path (for health probe)
#   OCTO_WA_NAME      daemon instance name (default: default)
#   OCTO_WA_SLEEP     seconds between every WA RPC (default: 3)
#   OCTO_WA_TIMEOUT   per-call timeout seconds (default: 30)
#   OCTO_WA_RETRIES   retry budget on transient errors (default: 2)
#   OCTO_WA_BACKOFF   extra sleep between retries seconds (default: 5)

# === Defaults (overridable via env) =====================================

: "${OCTO_WA_BIN:=/home/mmacedoeu/_w/ai/cipherocto/target/debug/octo-whatsapp}"
: "${OCTO_WA_NAME:=default}"
: "${OCTO_WA_SOCKET:=/tmp/octo-wa-run/octo-whatsapp-${OCTO_WA_NAME}.sock}"
: "${OCTO_WA_SLEEP:=3}"
: "${OCTO_WA_TIMEOUT:=30}"
: "${OCTO_WA_RETRIES:=2}"
: "${OCTO_WA_BACKOFF:=5}"

# === Helpers ============================================================

wa_log() { printf '[%s] %s\n' "$(date -Iseconds)" "$*" >&2; }
wa_now_ms() { date +%s%3N; }

# wa_health_check — abort the script if the daemon is not connected.
# Returns 0 if connected, 1 otherwise. Idempotent (cheap to call).
wa_health_check() {
    local resp
    resp=$(timeout 10 "$OCTO_WA_BIN" --name "$OCTO_WA_NAME" --socket "$OCTO_WA_SOCKET" status --json 2>/dev/null || true)
    if [ -z "$resp" ]; then
        wa_log "health: empty status response (daemon unreachable)"
        return 1
    fi
    # status --json returns plain JSON; check connected/ready fields.
    local connected
    connected=$(printf '%s' "$resp" | python3 -c "
import json, sys
try:
    r = json.loads(sys.stdin.read())
    print('1' if (r.get('connected') or r.get('ready')) else '0')
except Exception:
    print('0')
" 2>/dev/null)
    if [ "$connected" != "1" ]; then
        wa_log "health: daemon not connected (connected=$connected)"
        return 1
    fi
    return 0
}

# mcp_call <jsonrpc-request>
# Wraps the binary's mcp-over-stdio transport with timeout + retry.
# Outputs the JSON response on stdout. Returns the binary's exit code
# on the final attempt.
mcp_call() {
    local req="$1"
    local attempt=0
    local max_attempts=$((OCTO_WA_RETRIES + 1))
    local resp
    local rc
    while [ "$attempt" -lt "$max_attempts" ]; do
        attempt=$((attempt+1))
        local tmp
        tmp=$(mktemp)
        printf '%s\n' "$req" > "$tmp"
        # timeout kills the subprocess if it hangs; the response stays
        # valid for completed calls.
        if resp=$(timeout "$OCTO_WA_TIMEOUT" env \
            XDG_RUNTIME_DIR=/tmp/octo-wa-run \
            OCTO_WHATSAPP_DATA_DIR=/home/mmacedoeu/.local/share/octo/whatsapp \
            "$OCTO_WA_BIN" --name "$OCTO_WA_NAME" mcp < "$tmp" 2>/dev/null); then
            rc=0
        else
            rc=$?
            resp=""
        fi
        rm -f "$tmp"
        # Success path: response should be JSON.
        if [ "$rc" = "0" ] && [ -n "$resp" ] && printf '%s' "$resp" | python3 -c "
import json, sys
try:
    json.loads(sys.stdin.read())
    sys.exit(0)
except Exception:
    sys.exit(1)
" 2>/dev/null; then
            # Check for transient errors worth retrying.
            # Transient = JSON-RPC transport error (daemon socket closed,
            # timeout). Tool-level errors (isError: true or result.content
            # with a code field) are ALWAYS permanent — the app processed
            # the request and returned a definitive answer.
            if printf '%s' "$resp" | python3 -c "
import json, sys
r = json.loads(sys.stdin.read())
# Tool-level error with isError=true — permanent, never retry
result = r.get('result') or {}
if result.get('isError'):
    sys.exit(0)
# Tool-level error embedded in content[0].text — permanent
try:
    body = json.loads(result.get('content',[{}])[0].get('text',''))
    if isinstance(body, dict) and body.get('code') is not None:
        sys.exit(0)
except Exception:
    pass
# JSON-RPC transport errors — transient if reconnection codes
err = r.get('error') or {}
code = err.get('code', 0)
if code in (-32603, -32005, -32002):
    sys.exit(11)
sys.exit(0)
" 2>/dev/null; then
                echo "$resp"
                return 0
            else
                # Transient — sleep + retry
                if [ "$attempt" -lt "$max_attempts" ]; then
                    wa_log "mcp_call: transient error, retry $attempt/$OCTO_WA_RETRIES (sleep ${OCTO_WA_BACKOFF}s)"
                    sleep "$OCTO_WA_BACKOFF"
                    continue
                fi
            fi
        else
            # Hard failure (timeout, non-JSON, etc.)
            if [ "$attempt" -lt "$max_attempts" ]; then
                wa_log "mcp_call: call failed rc=$rc, retry $attempt/$OCTO_WA_RETRIES (sleep ${OCTO_WA_BACKOFF}s)"
                sleep "$OCTO_WA_BACKOFF"
                continue
            fi
        fi
    done
    # All retries exhausted — emit an error JSON envelope so callers
    # can parse `error` instead of crashing.
    cat <<EOF
{"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"octo-wa: mcp_call exhausted $max_attempts attempts (timeout=${OCTO_WA_TIMEOUT}s, last_rc=$rc)"}}
EOF
    return 1
}

# mcp_call_sleep <jsonrpc-request>
# Same as mcp_call but sleeps OCTO_WA_SLEEP after the call. Use this
# instead of `mcp_call; sleep 3` everywhere.
mcp_call_sleep() {
    local resp
    resp=$(mcp_call "$1")
    echo "$resp"
    sleep "$OCTO_WA_SLEEP"
}
