#!/usr/bin/env bash
# scripts/persist-contacts-from-enriched-csv.sh
#
# Read a CSV file from an enriched table dump (will_details_enriched,
# percurso_details_enriched) and call contacts.save_contact for each
# row. Same pipeline as persist-contacts-from-csv.sh but accepts the
# enriched format (extra columns: group_jids, is_admin, etc.).
# Skips rows where the phone column matches a configurable admin phone.
#
# Usage:
#   scripts/persist-contacts-from-enriched-csv.sh /tmp/will_details_enriched.csv
#   scripts/persist-contacts-from-enriched-csv.sh /tmp/percurso_details_enriched.csv member_phone verified_name
#   scripts/persist-contacts-from-enriched-csv.sh /tmp/foo.csv phone_col name_col 5521995544743@s.whatsapp.net
#   scripts/persist-contacts-from-enriched-csv.sh --no-strict /tmp/foo.csv phone_col name_col
#
# Automatically skips rows where is_admin=1 (if the column exists).
# Optional admin phone arg skips a specific phone (no default).
#
# Args:
#   $1  CSV file path (required)
#   $2  phone column name in the CSV (default: member_phone)
#   $3  name  column name in the CSV (default: verified_name)
#   $4  admin phone to skip (default: 5521995544743@s.whatsapp.net)
#
# Flags:
#   --no-strict   skip the details-footprint check (default: strict ON;
#                 the CSV header MUST contain every column a *details
#                 table would produce — see EXPECTED_ENRICHED_COLUMNS
#                 below). Use this when the CSV is from a non-details
#                 table or a hand-edited subset.
#
# Env (all consumed by lib-octo-wa.sh):
#   OCTO_WA_BIN, OCTO_WA_SLEEP, OCTO_WA_TIMEOUT, OCTO_WA_RETRIES,
#   OCTO_WA_BACKOFF, OCTO_WA_SOCKET
#
# Reports: enumerated, saved, skipped, failed (with row # for triage).

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib-octo-wa.sh"

# === Expected footprints =================================================
# Accept either the base details format or the enriched format (extra
# group columns). Strict mode passes if ALL columns from EITHER set are
# present in the CSV.
EXPECTED_DETAILS_COLUMNS=(
    id member_lid member_phone user_info_jid is_business verified_name
    status_text picture_id picture_url picture_url_fetched_ts on_whatsapp
    devices_count business_profile_status business_description
    business_address business_hours business_website business_email
    business_categories fetched_at_ts
)
EXPECTED_ENRICHED_COLUMNS=(
    "${EXPECTED_DETAILS_COLUMNS[@]}"
    group_jids is_admin member_phone_sources group_ts_unix_ms
)

STRICT=1

# --verbose / -v: per-row tracing (index, phone, name, response status).
# Default OFF. Pure logging — no validation, no dedup, no filtering.
VERBOSE=0

# === Args (with --no-strict / --strict / --verbose flag) ==============

CSV_FILE=""
PHONE_COL="member_phone"
NAME_COL="verified_name"
ADMIN_PHONE=""

args=()
while [ $# -gt 0 ]; do
    case "$1" in
        --no-strict) STRICT=0; shift ;;
        --strict)    STRICT=1; shift ;;
        --verbose|-v) VERBOSE=1; shift ;;
        -h|--help)
            sed -n '2,30p' "$0"
            exit 0 ;;
        --) shift; while [ $# -gt 0 ]; do args+=("$1"); shift; done ;;
        *)  args+=("$1"); shift ;;
    esac
done

CSV_FILE="${args[0]:?usage: persist-contacts-from-csv.sh <csv-file> [phone-col] [name-col] [admin-phone]}"
[ "${#args[@]}" -ge 2 ] && PHONE_COL="${args[1]}"
[ "${#args[@]}" -ge 3 ] && NAME_COL="${args[2]}"
[ "${#args[@]}" -ge 4 ] && ADMIN_PHONE="${args[3]}"

[ -r "$CSV_FILE" ] || { echo "csv not readable: $CSV_FILE" >&2; exit 1; }

# === Pre-run health check ===============================================

if ! wa_health_check; then
    wa_log "aborting: daemon not connected"
    exit 1
fi

# === Footprint check: does this CSV look like a details-table dump? ====

ACTUAL_COLUMNS=$(python3 - "$CSV_FILE" <<'PY'
import csv, sys
with open(sys.argv[1], newline='') as f:
    rdr = csv.DictReader(f)
    print(','.join(rdr.fieldnames or []))
PY
)

if [ "$STRICT" = "1" ]; then
    # Accept enriched format (base + group cols) OR base details format.
    enriched_ok=true; for col in "${EXPECTED_ENRICHED_COLUMNS[@]}"; do
        case ",${ACTUAL_COLUMNS}," in *",${col},"*) ;; *) enriched_ok=false; break;; esac
    done
    details_ok=true; for col in "${EXPECTED_DETAILS_COLUMNS[@]}"; do
        case ",${ACTUAL_COLUMNS}," in *",${col},"*) ;; *) details_ok=false; break;; esac
    done
    if ! $enriched_ok && ! $details_ok; then
        echo "  footprint mismatch: CSV matches neither details nor enriched format" >&2
        echo "  re-run with --no-strict to bypass (NOT recommended for unknown CSVs)" >&2
        exit 4
    fi
    echo "  footprint OK ($( $enriched_ok && echo enriched || echo details ) format)" >&2
fi

# Always verify the requested phone/name columns exist in the CSV header
case ",${ACTUAL_COLUMNS}," in
    *",${PHONE_COL},"*) ;;
    *) echo "  phone column '$PHONE_COL' not in CSV header" >&2; exit 2 ;;
esac
case ",${ACTUAL_COLUMNS}," in
    *",${NAME_COL},"*) ;;
    *) echo "  name column '$NAME_COL' not in CSV header (will fall back to phone)" >&2 ;;
esac

# === Step 1: validate columns + enumerate rows =========================

echo "→ reading $CSV_FILE (phone=$PHONE_COL name=$NAME_COL admin=${ADMIN_PHONE:-none})" >&2
ROWS_TMP=$(mktemp)
STDERR_TMP=$(mktemp)
python3 - "$CSV_FILE" "$PHONE_COL" "$NAME_COL" "$ADMIN_PHONE" > "$ROWS_TMP" 2>"$STDERR_TMP" <<'PY'
import csv, sys
csv_path, phone_col, name_col, admin_phone = sys.argv[1:5]
has_is_admin = False
with open(csv_path, newline='') as f:
    rdr = csv.DictReader(f)
    if not rdr.fieldnames:
        print("# empty CSV", file=sys.stderr); sys.exit(0)
    if phone_col not in rdr.fieldnames:
        print(f"# missing phone column {phone_col}; columns={rdr.fieldnames}", file=sys.stderr)
        sys.exit(2)
    has_is_admin = 'is_admin' in rdr.fieldnames
    seen = set()
    raw = 0
    skipped_admin = 0
    for row in rdr:
        raw += 1
        phone = (row.get(phone_col) or '').strip()
        name  = (row.get(name_col)  or '').strip()
        if not phone:
            continue
        if phone in seen:
            continue
        # Skip by is_admin column (1 = admin)
        if has_is_admin and str(row.get('is_admin', '0')).strip() == '1':
            skipped_admin += 1
            continue
        # Skip by admin phone override
        if admin_phone and phone == admin_phone:
            skipped_admin += 1
            continue
        seen.add(phone)
        name = name.replace('\t', ' ').replace('\n', ' ').strip()
        if '@' not in phone:
            phone = f'{phone}@s.whatsapp.net'
        print(f'{phone}\t{name}')
print(f'# raw={raw} unique={len(seen)} skipped_admin={skipped_admin}', file=sys.stderr)
PY

# Print Python stderr to terminal for progress
cat "$STDERR_TMP" >&2
# Extract skipped_admin from Python stderr
SKIPPED_ADMIN=$(grep -oP 'skipped_admin=\K\d+' "$STDERR_TMP" || echo 0)
rm -f "$STDERR_TMP"

COUNT=$(wc -l < "$ROWS_TMP" | tr -d ' ')
if [ "$COUNT" = "0" ]; then
    echo "  nothing to do" >&2
    rm -f "$ROWS_TMP"
    exit 0
fi
echo "  enumerated $COUNT unique phones (skipped $SKIPPED_ADMIN admins)" >&2

# === State file: resume support =====================================
# On daemon disconnect mid-run, the script saves the next row index +
# CSV fingerprint to a JSON state file. The next invocation resumes
# from that row instead of re-processing already-saved phones.
STATE_FILE="$SCRIPT_DIR/persist-contacts-from-csv.state.json"
START_IDX=0
if [ -f "$STATE_FILE" ]; then
    SAVED=$(python3 -c "
import json, sys
try:
    s = json.load(open(sys.argv[1]))
    print(s.get('csv_path',''))
    print(s.get('csv_mtime',-1))
    print(s.get('next_idx',0))
except Exception as e:
    print('PARSE_ERROR')
" "$STATE_FILE" 2>/dev/null)
    if [ "$(echo "$SAVED" | head -1)" != "PARSE_ERROR" ]; then
        SAVED_CSV=$(echo "$SAVED" | sed -n 1p)
        SAVED_MTIME=$(echo "$SAVED" | sed -n 2p)
        SAVED_NEXT=$(echo "$SAVED" | sed -n 3p)
        CURR_MTIME=$(stat -c %Y "$CSV_FILE" 2>/dev/null || echo -1)
        if [ "$SAVED_CSV" = "$CSV_FILE" ] && [ "$SAVED_MTIME" = "$CURR_MTIME" ] && [ "$SAVED_NEXT" -gt 0 ] 2>/dev/null; then
            START_IDX=$SAVED_NEXT
            wa_log "resuming from idx=$START_IDX (state file matches CSV)"
        else
            wa_log "state file stale (CSV path or mtime changed); starting fresh"
            rm -f "$STATE_FILE"
        fi
    else
        wa_log "state file unparseable; starting fresh"
        rm -f "$STATE_FILE"
    fi
fi

# === Daemon restart helpers ==========================================

restart_daemon() {
    local tries="${1:-1}"
    local wait_secs="${2:-60}"
    local args=()
    if [ -n "$OCTO_WA_NAME" ]; then
        # The run-octo-whatsapp.sh arg parser only recognises --name=NAME
        # (with =), not --name NAME. Use the = form.
        args=(--name="$OCTO_WA_NAME")
    fi
    wa_log "restarting daemon (try $tries): run-octo-whatsapp.sh --restart ${args[*]}"
    "$SCRIPT_DIR/run-octo-whatsapp.sh" --restart "${args[@]}" 2>&1 | head -5
    # Wait for connected=true, up to wait_secs seconds
    local deadline=$((SECONDS + wait_secs))
    while [ $SECONDS -lt $deadline ]; do
        local status
        status=$("$SCRIPT_DIR/run-octo-whatsapp.sh" ${OCTO_WA_NAME:+--name="$OCTO_WA_NAME"} --status --json 2>/dev/null \
            | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('connected',False), d.get('session_valid',False), d.get('phase','?'))" 2>/dev/null)
        if [[ "$status" == "True True connected" || "$status" == "True True phase"* ]]; then
            # verify ready/synced too
            local extra
            extra=$("$SCRIPT_DIR/run-octo-whatsapp.sh" ${OCTO_WA_NAME:+--name="$OCTO_WA_NAME"} --status --json 2>/dev/null \
                | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('ready',False), d.get('bot_state','?'))" 2>/dev/null)
            if [[ "$extra" == "True Connected" || "$extra" == "True connected" ]]; then
                wa_log "daemon ready: $extra"
                return 0
            fi
        fi
        sleep 2
    done
    return 1
}

save_state() {
    local next_idx="$1"
    python3 -c "
import json, os, sys, datetime
state = {
    'csv_path': sys.argv[1],
    'csv_mtime': int(os.stat(sys.argv[1]).st_mtime),
    'next_idx': int(sys.argv[2]),
    'last_updated': datetime.datetime.now().isoformat(timespec='seconds'),
}
with open(sys.argv[3], 'w') as f:
    json.dump(state, f, indent=2)
" "$CSV_FILE" "$next_idx" "$STATE_FILE"
    wa_log "state saved at $STATE_FILE (next_idx=$next_idx)"
}

# === Step 2: dispatch each row through contacts.save_contact ===========

declare -i idx=0 saved=0 skipped=0 failed=0
declare -i restart_attempts=0
while IFS=$'\t' read -r phone name; do
    idx=$((idx+1))
    # Resume support: skip rows we've already processed.
    if [ "$idx" -le "$START_IDX" ]; then
        if [ "$VERBOSE" = "1" ]; then
            wa_log "[$idx/$COUNT] (resumed; skipping already-saved)"
        fi
        continue
    fi
    # Name: prefer the name column; fall back to digits-only phone
    if [ -z "$name" ]; then
        name="${phone%@s.whatsapp.net}"
    fi
    # Always print the row start so progress is visible even without --verbose.
    wa_log "[$idx/$COUNT] save phone=$phone name=$name"
    esc_name=$(printf '%s' "$name"  | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
    esc_peer=$(printf '%s' "$phone" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
    RESP=$(mcp_call_sleep "{\"jsonrpc\":\"2.0\",\"id\":$idx,\"method\":\"tools/call\",\"params\":{\"name\":\"contacts.save_contact\",\"arguments\":{\"full_name\":$esc_name,\"peer\":$esc_peer}}}")
    if [ "$VERBOSE" = "1" ]; then
        wa_log "[$idx/$COUNT] response (truncated): $(printf '%s' "$RESP" | head -c 240)"
    fi
    if printf '%s' "$RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('  ERROR:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(1)
try:
    txt = r['result']['content'][0]['text']
    body = json.loads(txt)
    if body.get('status') == 'saved':
        sys.exit(0)
    print('  unexpected:', body, file=sys.stderr)
    sys.exit(1)
except Exception as e:
    print('  parse error:', e, file=sys.stderr)
    sys.exit(1)
" 2>/dev/null; then
        saved=$((saved+1))
        wa_log "[$idx/$COUNT] OK saved $phone"
    else
        # Distinguish "daemon exhausted" (recoverable via restart) from a
        # clean server-side rejection (NOT recoverable, just count as fail).
        if echo "$RESP" | grep -q "exhausted"; then
            wa_log "[$idx/$COUNT] daemon-exhausted; will attempt restart"
            if [ "$restart_attempts" -lt 3 ]; then
                restart_attempts=$((restart_attempts+1))
                if restart_daemon "$restart_attempts" 90; then
                    wa_log "retrying row $idx after restart"
                    # Re-send the same save call once.
                    RESP=$(mcp_call_sleep "{\"jsonrpc\":\"2.0\",\"id\":$idx,\"method\":\"tools/call\",\"params\":{\"name\":\"contacts.save_contact\",\"arguments\":{\"full_name\":$esc_name,\"peer\":$esc_peer}}}")
                    if printf '%s' "$RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    sys.exit(1)
try:
    txt = r['result']['content'][0]['text']
    body = json.loads(txt)
    sys.exit(0 if body.get('status') == 'saved' else 1)
except Exception:
    sys.exit(1)
" 2>/dev/null; then
                        saved=$((saved+1))
                        wa_log "[$idx/$COUNT] OK saved (after restart) $phone"
                    else
                        failed=$((failed+1))
                        wa_log "[$idx/$COUNT] FAILED after restart; saving state + exit"
                        save_state "$idx"
                        exit 1
                    fi
                else
                    wa_log "daemon restart did NOT come up; saving state + exit"
                    save_state "$idx"
                    exit 1
                fi
            else
                wa_log "exceeded 3 restart attempts; saving state + exit"
                save_state "$idx"
                exit 1
            fi
        else
            failed=$((failed+1))
            wa_log "[$idx/$COUNT] FAILED phone=$phone"
        fi
    fi
done < "$ROWS_TMP"
rm -f "$ROWS_TMP"
# Clean exit: clear the state file (everything done).
rm -f "$STATE_FILE"

# === Step 3: report ====================================================

echo "→ done" >&2
echo "  csv             = $CSV_FILE" >&2
echo "  phone_col       = $PHONE_COL" >&2
echo "  name_col        = $NAME_COL" >&2
echo "  admin_phone     = ${ADMIN_PHONE:-none}" >&2
echo "  is_admin_skip   = yes (filtered in enumeration)" >&2
echo "  admins_skipped  = $SKIPPED_ADMIN" >&2
echo "  enumerated      = $COUNT" >&2
echo "  saved           = $saved" >&2
echo "  skipped         = $skipped" >&2
echo "  failed          = $failed" >&2
echo "  verbose     = $VERBOSE" >&2
