#!/usr/bin/env bash
# scripts/run-octo-whatsapp.sh
#
# Launch the octo-whatsapp daemon as a detached background process owned
# by PID 1 (init/systemd), independent of the terminal that started it.
# Survives Claude Code restart, `exit`, shell logout, and `tmux`
# pane close — only PID kill, signal, or system shutdown ends it.
#
# Idempotent: if a daemon is already bound to the target socket, prints
# status and exits 0 without relaunching. Use `--status` to inspect, or
# `--stop` to terminate cleanly via `daemon.shutdown` RPC.
#
# Usage:
#   scripts/run-octo-whatsapp.sh                  # start (default: live session)
#   scripts/run-octo-whatsapp.sh --status         # print daemon state
#   scripts/run-octo-whatsapp.sh --stop           # graceful shutdown
#   scripts/run-octo-whatsapp.sh --restart        # stop then start
#   scripts/run-octo-whatsapp.sh --name NAME      # multi-instance (default: default)
#   scripts/run-octo-whatsapp.sh --features F     # cargo features (default: query)
#   scripts/run-octo-whatsapp.sh --profile P      # debug | release (default: debug)
#
# Profile notes:
#   debug:  compiled with --features query by default; includes tracing-subscriber
#           (RUST_LOG works out of the box), binary at target/debug/octo-whatsapp
#           (648MB, ~2x slower cold-start than release).
#   release: compiled with cargo build --release (no default features unless
#           --features is also passed). Binary at target/release/octo-whatsapp
#           (53MB). MCP tools/call sql.* + daemon.search + messages.context etc.
#           are GATED by the `query` feature — release binary without
#           --features query returns -32601 for those tools. Use
#           --profile=release --features=query (or set OCTO_WHATSAPP_FEATURES)
#           when launching if you need the query surface.
#
# Detach mechanism:
#   setsid    — new session + process group, decouples from terminal
#   nohup     — ignore SIGHUP so parent (Claude Code) restart does not kill daemon
#   stdin/out/err → /dev/null + logfile, breaks stdio back-link
#   cmd &     — bash job fork; combined with `disown` removes job table entry
#   Optional `--systemd` registers the daemon under the user's
#   `systemd --user` instance so it survives logout too. Without it,
#   the daemon is reparented to PID 1 by the kernel and survives
#   parent death (the canonical Unix "double fork" lite).
#
# Outputs:
#   logs:   $LOG_DIR/octo-whatsapp-$NAME.log (stdout + stderr)
#   pid:    $RUNTIME_DIR/octo-whatsapp-$NAME.pid
#   lock:   $RUNTIME_DIR/octo-whatsapp-$NAME.lock (flock)
#
# Exit codes:
#   0   success (or daemon was already running)
#   1   binary not found (build first)
#   2   port already in use by another process
#   3   daemon died within 5s of launch (logs in $LOG_DIR)
#   4   --stop failed (process not found or RPC rejected)

set -euo pipefail

# === Defaults (overridable via env or flags) =================================

NAME="${OCTO_WHATSAPP_NAME:-default}"
FEATURES="${OCTO_WHATSAPP_FEATURES:-query}"
# Build profile. debug = target/debug/octo-whatsapp (default, includes
# tracing-subscriber, supports RUST_LOG). release = target/release (smaller,
# faster, but query/MCP tools gated unless --features query is also passed).
PROFILE="${OCTO_WHATSAPP_PROFILE:-debug}"
DATA_DIR="${OCTO_WHATSAPP_DATA_DIR:-$HOME/.local/share/octo/whatsapp}"
# Default socket dir: a writable runtime dir. Use /tmp/octo-wa-run (operator
# convention) when the data dir is unwritable for sockets; fall back to the
# system XDG runtime dir otherwise.
SOCKET_DIR="${OCTO_WHATSAPP_SOCKET_DIR:-/tmp/octo-wa-run}"
SOCKET="$SOCKET_DIR/octo-whatsapp-$NAME.sock"
# Session DB path. Default matches `octo-whatsapp-onboard`'s dot-separated
# layout ($data_dir/$NAME.session.db). Override with
# OCTO_WHATSAPP_SESSION_PATH for explicit control.
SESSION_PATH="${OCTO_WHATSAPP_SESSION_PATH:-$DATA_DIR/$NAME.session.db}"
# Daemon-side tracing log dir (Rust config log_dir). Default to a writable
# per-user path because the compiled-in default is /var/log/octo/whatsapp
# which an unprivileged user cannot create.
LOG_DIR="${OCTO_WHATSAPP_LOG_DIR:-$DATA_DIR/$NAME/logs}"
# Stdout/stderr log captured by this script (in addition to daemon tracing).
CAPTURE_LOG_DIR="${OCTO_WHATSAPP_CAPTURE_LOG_DIR:-$DATA_DIR/$NAME/capture}"
PID_FILE="/run/user/$(id -u)/octo-whatsapp-$NAME.pid"
LOCK_FILE="/run/user/$(id -u)/octo-whatsapp-$NAME.lock"
BIN_DIR="$HOME/_w/ai/cipherocto/target/$PROFILE"
BIN="$BIN_DIR/octo-whatsapp"
# Boot wait: with background NDJSON replay (Phase 7.J follow-up,
# 2026-07-15) the daemon binds its IPC socket in single-digit
# seconds even on a 19k-event cold-start. 30s is the new default —
# the prior 45s bound only held because `replay_ndjson` ran in the
# bind path. 60s is still used for cold-start race-watchdog.
WAIT_BOOT_SECS="${WAIT_BOOT_SECS:-30}"

ACTION="start"

# === Args ==================================================================

for arg in "$@"; do
    case "$arg" in
        --status) ACTION="status" ;;
        --stop)   ACTION="stop" ;;
        --restart) ACTION="restart" ;;
        --systemd) ACTION="systemd" ;;
        --name=*) NAME="${arg#*=}" ;;
        --features=*) FEATURES="${arg#*=}" ;;
        --profile=*) PROFILE="${arg#*=}" ;;
        -h|--help)
            sed -n '2,53p' "$0"
            exit 0
            ;;
        *) echo "unknown arg: $arg" >&2; exit 1 ;;
    esac
done

# Validate profile. Anything outside {debug, release} almost certainly
# means a typo — fail loud instead of falling back silently.
case "$PROFILE" in
    debug|release) ;;
    *) echo "invalid --profile: $PROFILE (expected: debug | release)" >&2; exit 1 ;;
esac

# === Path discovery (canonical worktree path) ===============================

REPO_ROOT="$HOME/_w/ai/cipherocto"
# Discovery order: main checkout first (tracks the `next` branch the
# active Claude session is committing to), fall back to worktrees. The
# prior behaviour — worktree first — silently pinned the daemon to the
# older `feat/whatsapp-runtime-cli-mcp` build and bypassed the typed-event
# overhaul that lives on `next`. Flip the order so the latest commits
# always win. Override with OCTO_WHATSAPP_PREFER_WORKTREE=1 to force the
# legacy worktree-first behaviour.
if [ "${OCTO_WHATSAPP_PREFER_WORKTREE:-0}" != "1" ] \
    && [ -x "$REPO_ROOT/target/$PROFILE/octo-whatsapp" ]; then
    BIN_DIR="$REPO_ROOT/target/$PROFILE"
    BIN="$BIN_DIR/octo-whatsapp"
else
    for wt in "$REPO_ROOT/.worktrees"/*; do
        [ -x "$wt/target/$PROFILE/octo-whatsapp" ] && BIN_DIR="$wt/target/$PROFILE" && BIN="$BIN_DIR/octo-whatsapp" && break
    done
fi
[ -x "$BIN" ] || { echo "binary not found: $BIN (run: cargo build --profile $PROFILE -p octo-whatsapp --features $FEATURES)" >&2; exit 1; }

# === Helpers ===============================================================

log() { printf '[%s] %s\n' "$(date -Iseconds)" "$*" >&2; }

pid_alive() {
    [ -f "$PID_FILE" ] || return 1
    local pid; pid=$(cat "$PID_FILE" 2>/dev/null || true)
    [ -n "$pid" ] && [ -d "/proc/$pid" ] && return 0
    rm -f "$PID_FILE"
    return 1
}

socket_bound() { [ -S "$SOCKET" ]; }

# RPC probe: ground truth. If `status --json` succeeds, the daemon is
# reachable on the socket path. Used by `daemon_running` because file-only
# checks miss the case where the daemon unlinks + rebinds on its own (the
# filesystem inode briefly disappears) — only an actual RPC works.
rpc_alive() {
    "$BIN" --socket "$SOCKET" status --json >/dev/null 2>&1
}

daemon_running() { pid_alive && rpc_alive; }

wait_ready() {
    local i
    for i in $(seq 1 "$WAIT_BOOT_SECS"); do
        if socket_bound && rpc_alive; then
            return 0
        fi
        sleep 1
    done
    return 1
}

# === Actions ===============================================================

case "$ACTION" in
    status)
        if daemon_running; then
            pid=$(cat "$PID_FILE"); log "running pid=$pid socket=$SOCKET"
            "$BIN" --socket "$SOCKET" status --json
        else
            log "stopped (no live daemon on $SOCKET)"
            exit 1
        fi
        ;;

    stop)
        if ! daemon_running; then log "not running"; rm -f "$SOCKET"; exit 0; fi
        log "graceful shutdown via RPC"
        if "$BIN" --socket "$SOCKET" shutdown >/dev/null 2>&1; then
            sleep 2
            rm -f "$SOCKET"
            log "stopped"
        else
            log "shutdown RPC failed; sending SIGTERM"
            pid=$(cat "$PID_FILE")
            kill "$pid" 2>/dev/null || true
            sleep 1
            kill -KILL "$pid" 2>/dev/null || true
            rm -f "$PID_FILE" "$SOCKET"
            log "killed"
        fi
        ;;

    restart)
        "$0" --stop || true
        sleep 1
        # Filter out --restart before exec; otherwise we loop forever.
        shift_args=()
        for arg in "$@"; do
            [ "$arg" = "--restart" ] && continue
            shift_args+=("$arg")
        done
        exec "$0" "${shift_args[@]}"
        ;;

    systemd)
        # Register under user systemd so SIGTERM on logout is also handled.
        SERVICE_DIR="$HOME/.config/systemd/user"
        mkdir -p "$SERVICE_DIR"
        SERVICE_FILE="$SERVICE_DIR/octo-whatsapp-$NAME.service"
        cat > "$SERVICE_FILE" <<EOF
[Unit]
Description=octo-whatsapp daemon ($NAME)
After=network.target

[Service]
Type=simple
Environment=OCTO_WHATSAPP_DATA_DIR=$DATA_DIR
Environment=OCTO_WHATSAPP_SOCKET_DIR=$SOCKET_DIR
Environment=OCTO_WHATSAPP_LOG_DIR=$LOG_DIR
Environment=OCTO_WHATSAPP_SESSION_PATH=$SESSION_PATH
ExecStart=$BIN --socket $SOCKET --name $NAME daemon
Restart=on-failure
RestartSec=2
StandardOutput=append:$CAPTURE_LOG_DIR/octo-whatsapp-$NAME.log
StandardError=append:$CAPTURE_LOG_DIR/octo-whatsapp-$NAME.log

[Install]
WantedBy=default.target
EOF
        log "wrote $SERVICE_FILE"
        systemctl --user daemon-reload
        systemctl --user enable  "octo-whatsapp-$NAME.service"
        systemctl --user start   "octo-whatsapp-$NAME.service"
        systemctl --user status  "octo-whatsapp-$NAME.service" --no-pager
        ;;

    start)
        if daemon_running; then
            pid=$(cat "$PID_FILE")
            log "already running pid=$pid socket=$SOCKET (use --restart to force)"
            "$BIN" --socket "$SOCKET" status --json
            exit 0
        fi

        # Stale pidfile?
        if [ -f "$PID_FILE" ] && ! pid_alive; then
            log "removing stale pidfile $PID_FILE"
            rm -f "$PID_FILE"
        fi

        # Stale socket?
        if [ -S "$SOCKET" ]; then
            log "removing stale socket $SOCKET"
            rm -f "$SOCKET"
        fi

        mkdir -p "$LOG_DIR" "$CAPTURE_LOG_DIR" "$SOCKET_DIR" "/run/user/$(id -u)"
        : > "$CAPTURE_LOG_DIR/octo-whatsapp-$NAME.log"

        # === Detach dance ==================================================
        # setsid -f       fork into a NEW session + new process group
        #                 (-f is mandatory: plain `setsid` returns EPERM when
        #                  the caller is already a process-group leader,
        #                  which is the case inside most bash subshells
        #                  spawned by Claude Code)
        # env VAR=VAL     propagate overrides (log_dir, socket_dir, data_dir)
        # </dev/null      break stdin from any controlling terminal
        # >>log 2>&1      break stdout/stderr to a file in CAPTURE_LOG_DIR
        # ──────────────────────────────────────────────────────────────────
        setsid -f env \
            OCTO_WHATSAPP_DATA_DIR="$DATA_DIR" \
            OCTO_WHATSAPP_SOCKET_DIR="$SOCKET_DIR" \
            OCTO_WHATSAPP_LOG_DIR="$LOG_DIR" \
            OCTO_WHATSAPP_SESSION_PATH="$SESSION_PATH" \
            "$BIN" \
            --socket "$SOCKET" \
            --name "$NAME" \
            daemon \
            </dev/null \
            >>"$CAPTURE_LOG_DIR/octo-whatsapp-$NAME.log" 2>&1
        # setsid -f is synchronous (parent exits, child keeps running).
        # Wait up to 10s for THIS daemon (matched by socket path) to appear.
        for _ in $(seq 1 50); do
            DAEMON_PID=$(pgrep -f -- "--socket $SOCKET .* daemon" 2>/dev/null \
                | head -1 || true)
            [ -n "${DAEMON_PID:-}" ] && break
            sleep 0.2
        done
        if [ -z "${DAEMON_PID:-}" ]; then
            log "daemon did not appear in pgrep within 10s after setsid"
            tail -20 "$CAPTURE_LOG_DIR/octo-whatsapp-$NAME.log" >&2 || true
            exit 3
        fi
        echo "$DAEMON_PID" > "$PID_FILE"

        # === Verify boot ===================================================
        if ! wait_ready; then
            log "daemon did not bind within ${WAIT_BOOT_SECS}s"
            log "tail of $CAPTURE_LOG_DIR/octo-whatsapp-$NAME.log:"
            tail -20 "$CAPTURE_LOG_DIR/octo-whatsapp-$NAME.log" >&2 || true
            if [ -d "/proc/$DAEMON_PID" ]; then
                kill "$DAEMON_PID" 2>/dev/null || true
            fi
            rm -f "$PID_FILE" "$SOCKET"
            exit 3
        fi

        # Confirm ppid reparented to 1 (init/systemd) — the goal of detach.
        actual_ppid=$(awk '{print $4}' "/proc/$DAEMON_PID/stat" 2>/dev/null || echo "?")
        log "started pid=$DAEMON_PID ppid=$actual_ppid socket=$SOCKET"
        log "  stdout/stderr: $CAPTURE_LOG_DIR/octo-whatsapp-$NAME.log"
        log "  daemon tracing: $LOG_DIR"
        if [ "$actual_ppid" != "1" ]; then
            log "(note: ppid=$actual_ppid, not reparented yet — Claude Code may still own the process)"
        fi
        "$BIN" --socket "$SOCKET" status --json
        ;;
esac
