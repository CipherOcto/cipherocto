#!/usr/bin/env bash
# scripts/persist-member-detail-flex.sh
#
# Persist ONE member's full detail row (LID/phone + all info RPCs).
# Mirrors persist-group-members-flex.sh: same mcp_call transport,
# same Python-heredoc-emits-SQL-terminated-with-__STMT_END__ pattern,
# same dispatcher loop. Differences: single row, table name is a
# pure DDL dest (no PK that the daemon enforces), and every WA RPC
# is padded with `sleep 3` to respect the WA anti-rate-limit cooldown
# captured in memory.
#
# Pipeline (every WA RPC padded with `sleep 3`):
#   1. resolve peer → both LID and phone (whichever is missing)
#      - contacts.get_pn_lid_mappings if a phone was given
#      - contacts.get_lid_pn_mappings if a LID was given
#   2. contacts.get_user_info           (phone form preferred; falls back to LID)
#   3. contacts.is_on_whatsapp          (phone form)
#   4. contacts.get_business_profile    (phone form, if is_business)
#   5. contacts.get_profile_picture     (LID form)
#   6. sql.execute CREATE TABLE IF NOT EXISTS
#   7. sql.execute INSERT (single row)
#   8. sql.query    SELECT to verify
#
# Schema (one row per peer, dedicated table — NO composite PK). stoolap
# ONLY supports INTEGER PRIMARY KEY (no TEXT PK, no AUTOINCREMENT), so
# we use an AUTO-INCREMENT id surrogate + UNIQUE on member_lid. The
# Python helper reserves the next id via SELECT MAX(id)+1 before INSERT.
#   id                        INTEGER PRIMARY KEY
#   member_lid                TEXT UNIQUE NOT NULL
#   member_phone              TEXT
#   user_info_jid             TEXT
#   is_business               INTEGER
#   verified_name             TEXT
#   status_text               TEXT
#   picture_id                TEXT
#   picture_url               TEXT
#   picture_url_fetched_ts    INTEGER
#   on_whatsapp               INTEGER
#   devices_count             INTEGER
#   business_profile_status   TEXT
#   business_description      TEXT
#   business_address          TEXT
#   business_hours            TEXT
#   business_website          TEXT
#   business_email            TEXT
#   business_categories       TEXT
#   fetched_at_ts             INTEGER
#
# Args:
#   $1  peer — EITHER a phone (digits-only, gets `@s.whatsapp.net`
#             appended) OR a LID (`...@lid`). The script auto-detects.
#   $2  target table name (e.g. member_details). MUST be a valid SQL
#             identifier (same token check as the group flex script).
#             Default: "member_details".
#   $3  optional phone hint (digits-only). If the script can't resolve
#             LID → phone via get_lid_pn_mappings (privacy-hidden LIDs
#             fail the usync IQ), the operator can pass the phone here
#             so the row is fully populated. Format: digit string ONLY
#             (no `@s.whatsapp.net`).
#
# Env:
#   OCTO_WA_BIN       path to octo-whatsapp binary
#   OCTO_WA_SLEEP     seconds between every WA RPC (default: 3)
#
# Usage:
#   scripts/persist-member-detail-flex.sh 163174481453092@lid member_details
#   scripts/persist-member-detail-flex.sh 5521964073308 member_details
#   scripts/persist-member-detail-flex.sh 163174481453092@lid member_details 5521964073308

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib-octo-wa.sh"

PEER="${1:?usage: persist-member-detail-flex.sh <phone-or-lid> <table> [phone-hint]}"
TABLE="${2:-member_details}"
PHONE_HINT="${3:-}"

# === Flags ==============================================================

VERBOSE=0
args=()
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=1 ;;
        --) shift; while [ $# -gt 0 ]; do args+=("$1"); shift; done ;;
        *)  args+=("$arg") ;;
    esac
done
# Re-parse positional from filtered args
if [ "${#args[@]}" -ge 1 ]; then PEER="${args[0]}"; fi
if [ "${#args[@]}" -ge 2 ]; then TABLE="${args[1]}"; fi
if [ "${#args[@]}" -ge 3 ]; then PHONE_HINT="${args[2]}"; fi

# === Table-name token check ============================================

if ! [[ "$TABLE" =~ ^[A-Za-z_][A-Za-z0-9_$#]*$ ]]; then
    echo "invalid table name: $TABLE" >&2
    echo "  must match ^[A-Za-z_][A-Za-z0-9_$#]*$" >&2
    exit 3
fi

BIN="${OCTO_WA_BIN:-/home/mmacedoeu/_w/ai/cipherocto/target/debug/octo-whatsapp}"
SLEEP_SECS="${OCTO_WA_SLEEP:-3}"
NOW_MS="$(date +%s%3N)"
MAX_RESTARTS="${OCTO_WA_MAX_RESTARTS:-3}"

[ -x "$BIN" ] || { echo "binary not executable: $BIN" >&2; exit 1; }

# === Daemon restart helper =============================================

restart_daemon() {
    local tries="${1:-1}"
    local wait_secs="${2:-90}"
    local args=()
    if [ -n "$OCTO_WA_NAME" ]; then
        args=(--name="$OCTO_WA_NAME")
    fi
    wa_log "restarting daemon (try $tries): run-octo-whatsapp.sh --restart ${args[*]}"
    "$SCRIPT_DIR/run-octo-whatsapp.sh" --restart "${args[@]}" 2>&1 | head -5
    local deadline=$((SECONDS + wait_secs))
    while [ $SECONDS -lt $deadline ]; do
        local status
        status=$("$SCRIPT_DIR/run-octo-whatsapp.sh" ${OCTO_WA_NAME:+--name="$OCTO_WA_NAME"} --status --json 2>/dev/null \
            | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('connected',False), d.get('session_valid',False), d.get('phase','?'))" 2>/dev/null)
        if [[ "$status" == "True True connected" || "$status" == "True True phase"* ]]; then
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

# === Pre-run health check ===========================================

if ! wa_health_check; then
    wa_log "aborting: daemon not connected"
    exit 1
fi

# === Peer shape detection ===============================================

# digits-only → phone; contains '@' → LID / phone-JID
if [[ "$PEER" =~ ^[0-9]+$ ]]; then
    PHONE_RAW="$PEER"
    LID_RAW=""
elif [[ "$PEER" =~ @lid$ ]]; then
    PHONE_RAW=""
    LID_RAW="$PEER"
elif [[ "$PEER" =~ @s\.whatsapp\.net$ ]]; then
    # full phone JID passed; strip the suffix
    PHONE_RAW="${PEER%@s.whatsapp.net}"
    LID_RAW=""
else
    echo "unrecognised peer format: $PEER" >&2
    echo "  expected: digits (phone), ...@lid, or ...@s.whatsapp.net" >&2
    exit 4
fi

# === Retry wrapper =====================================================

RESTART_COUNT=0
pipeline_done=false

while [ "$RESTART_COUNT" -lt "$MAX_RESTARTS" ] && ! $pipeline_done; do
    if [ "$RESTART_COUNT" -gt 0 ]; then
        wa_log "pipeline retry $RESTART_COUNT/$MAX_RESTARTS"
    fi

# === Step 1: gather contact info (Python helper, padded sleeps) =======

RAW=$(PEER="$PEER" PHONE_RAW="$PHONE_RAW" LID_RAW="$LID_RAW" PHONE_HINT="$PHONE_HINT" \
       TABLE="$TABLE" \
       BIN="$BIN" SLEEP_SECS="$SLEEP_SECS" \
       python3 - <<'PY'
import json, os, subprocess, sys, time

peer = os.environ["PEER"]
phone_raw = os.environ["PHONE_RAW"]
lid_raw = os.environ["LID_RAW"]
phone_hint = os.environ.get("PHONE_HINT", "").strip()
if phone_hint and not phone_hint.isdigit():
    print(f"# invalid phone hint (non-digit): {phone_hint}", file=sys.stderr)
    phone_hint = ""
bin_path = os.environ["BIN"]
sleep_s = float(os.environ["SLEEP_SECS"])
retries = int(os.environ.get("OCTO_WA_RETRIES", "2"))
backoff = float(os.environ.get("OCTO_WA_BACKOFF", "5"))
timeout = int(os.environ.get("OCTO_WA_TIMEOUT", "30"))

def _is_transient(r):
    """True when response carries a retryable transport error.
    Tool-level errors (isError=true or body.code present) are
    permanent — the app processed the request."""
    result = r.get("result") or {}
    if result.get("isError"):
        return False
    try:
        body = json.loads(result.get("content",[{}])[0].get("text",""))
        if isinstance(body, dict) and body.get("code") is not None:
            return False
    except Exception:
        pass
    err = (r.get("error") or {})
    if err.get("code") in (-32603, -32005, -32002):
        return True
    return False

def call(method, args):
    """Single WA RPC with timeout + retry on transient errors.
    Returns the parsed JSON response or an error envelope."""
    req = {"jsonrpc":"2.0","id":1,"method":"tools/call",
           "params":{"name":method,"arguments":args}}
    env = os.environ.copy()
    env["XDG_RUNTIME_DIR"] = "/tmp/octo-wa-run"
    env["OCTO_WHATSAPP_DATA_DIR"] = os.path.expanduser("~/.local/share/octo/whatsapp")
    payload = json.dumps(req)
    last = {"_raw": "", "_stderr": ""}
    for attempt in range(retries + 1):
        try:
            p = subprocess.run([bin_path, "--name", "default", "mcp"],
                               input=payload, capture_output=True,
                               text=True, env=env, timeout=timeout)
            out = (p.stdout or "").strip()
            try:
                r = json.loads(out)
            except Exception:
                r = {"_raw": out, "_stderr": p.stderr}
            last = r
            if _is_transient(r) and attempt < retries:
                print(f"  # transient, retry {attempt+1}/{retries} (sleep {backoff}s)", file=sys.stderr)
                time.sleep(backoff)
                continue
            return r
        except subprocess.TimeoutExpired:
            print(f"  # timeout after {timeout}s, retry {attempt+1}/{retries} (sleep {backoff}s)", file=sys.stderr)
            time.sleep(backoff)
            last = {"_raw": "", "_stderr": "TimeoutExpired"}
            continue
    return last

def paced(method, args):
    """Single WA RPC then mandatory sleep(SLEEP_SECS)."""
    r = call(method, args)
    time.sleep(sleep_s)
    print(f"  rpc={method} sleep={sleep_s}s", file=sys.stderr)
    return r

def body_or_blank(r):
    try:
        return json.loads(r["result"]["content"][0]["text"])
    except Exception:
        return {}

# --- record layout ---
rec = {
    "member_lid": lid_raw or None,
    "member_phone": (f"{phone_raw}@s.whatsapp.net" if phone_raw else None),
}

# --- 1a. resolve missing side (LID <-> phone) ---
# The user may pass a phone hint (3rd arg) to bypass the usync lookup
# when the WA server has the LID privacy-hidden. Use it as a fallback.
if rec["member_lid"] and not rec["member_phone"]:
    lid_only = rec["member_lid"].split("@", 1)[0]
    r = paced("contacts.get_lid_pn_mappings", {"lids": [lid_only]})
    body = body_or_blank(r)
    resolved = False
    for m in body.get("mappings", []) or []:
        if str(m.get("lid","")).split("@",1)[0] == lid_only:
            phone = m.get("phone_number")
            if phone:
                rec["member_phone"] = f"{phone}@s.whatsapp.net"
                resolved = True
            break
    if not resolved and phone_hint:
        print(f"  # LP lookup failed, using phone hint: {phone_hint}", file=sys.stderr)
        rec["member_phone"] = f"{phone_hint}@s.whatsapp.net"

if rec["member_phone"] and not rec["member_lid"]:
    r = paced("contacts.get_pn_lid_mappings", {"phones": [phone_raw]})
    body = body_or_blank(r)
    for m in body.get("mappings", []) or []:
        if str(m.get("phone","")) == phone_raw:
            lid = m.get("lid")
            if lid:
                rec["member_lid"] = f"{lid}@lid"
            break

# --- 2. user_info: phone form first (richer — returns `lid` field),
#               fall back to LID form. Merge any non-null fields across
#               both responses so we don't lose info.
ui_merged = {}
def merge_info(target, src):
    for k, v in src.items():
        if target.get(k) in (None, "") and v not in (None, ""):
            target[k] = v

# Always try phone form first if we have a phone — it returns the LID
# back AND the JID, devices, status, picture_id, etc.
if rec["member_phone"]:
    r = paced("contacts.get_user_info", {"peer": rec["member_phone"]})
    body = body_or_blank(r)
    if body.get("found"):
        ui_merged.update(body.get("info") or {})
    # If the merged response gave us a LID and we didn't have one, take it
    if not rec["member_lid"] and ui_merged.get("lid"):
        rec["member_lid"] = f"{ui_merged['lid']}@lid"

# Fall back / augment with LID form (catches privacy fields the phone
# form may redact when called from a non-contact session).
if rec["member_lid"]:
    r = paced("contacts.get_user_info", {"peer": rec["member_lid"]})
    body = body_or_blank(r)
    if body.get("found"):
        merge_info(ui_merged, body.get("info") or {})

ui = ui_merged
rec["user_info_jid"] = ui.get("jid")
rec["is_business"]   = 1 if ui.get("is_business") else 0
rec["verified_name"] = ui.get("verified_name")
rec["status_text"]   = ui.get("status")
rec["picture_id"]    = ui.get("picture_id")
devices = ui.get("devices") or []
rec["devices_count"] = len(devices)

# --- 3. is_on_whatsapp ---
if rec["member_phone"]:
    r = paced("contacts.is_on_whatsapp", {"peer": rec["member_phone"]})
    body = body_or_blank(r)
    rec["on_whatsapp"] = 1 if body.get("on_whatsapp") else 0
else:
    rec["on_whatsapp"] = None

# --- 4. business_profile (only if is_business) ---
if rec["is_business"] and rec["member_phone"]:
    r = paced("contacts.get_business_profile", {"jid": rec["member_phone"]})
    body = body_or_blank(r)
    rec["business_profile_status"] = body.get("status")
    bp = body.get("profile") or {}
    rec["business_description"] = bp.get("description")
    rec["business_address"]     = bp.get("address")
    rec["business_hours"]       = json.dumps(bp.get("hours")) if bp.get("hours") else None
    web = bp.get("website") or (bp.get("websites") or [None])[0]
    rec["business_website"]     = web
    rec["business_email"]       = bp.get("email")
    cats = bp.get("categories") or []
    # Categories may be list of dicts, list of strings, or mixed. Normalise
    # to strings before joining.
    cats_norm = []
    for c in cats:
        if isinstance(c, str):
            cats_norm.append(c)
        elif isinstance(c, dict):
            cats_norm.append(c.get("id") or c.get("name") or json.dumps(c))
        else:
            cats_norm.append(str(c))
    rec["business_categories"]  = ",".join(cats_norm) if cats_norm else None
else:
    rec["business_profile_status"] = "skipped_non_business" if not rec["is_business"] else None

# --- 5. profile_picture: try LID form first, then phone form. The
# WA server returns the same URL for both forms when the user is
# visible to us, but the privacy gates differ.
def fetch_picture(peer_form):
    r = paced("contacts.get_profile_picture", {"peer": peer_form, "preview": True})
    body = body_or_blank(r)
    if body.get("found"):
        return body.get("url")
    return None

if rec["member_lid"]:
    rec["picture_url"] = fetch_picture(rec["member_lid"])
if rec["picture_url"] is None and rec["member_phone"]:
    rec["picture_url"] = fetch_picture(rec["member_phone"])
if rec["picture_url"]:
    rec["picture_url_fetched_ts"] = int(time.time() * 1000)

# --- emit envelope for the shell: a JSON-RPC pair to call next_id + INSERT ---
# Two SQL statements end with __STMT_END__. The dispatcher will fire each
# in turn, parsing the response. Layout mirrors the group flex script.
import re
def esc(s):
    if s is None:
        return "NULL"
    if isinstance(s, (int, bool)):
        return str(int(s))
    s = str(s).replace("'", "''")
    return f"'{s}'"

cols = ["member_lid","member_phone","user_info_jid","is_business",
        "verified_name","status_text","picture_id","picture_url",
        "picture_url_fetched_ts","on_whatsapp","devices_count",
        "business_profile_status","business_description","business_address",
        "business_hours","business_website","business_email",
        "business_categories","fetched_at_ts"]
vals = [esc(rec.get(c)) for c in cols]
# PK fallback: if member_lid is missing, mint a synthetic id so the row
# remains addressable (the UNIQUE constraint on member_lid allows NULLs
# differently across engines; we always set member_lid).
if rec.get("member_lid") is None and rec.get("member_phone"):
    rec["member_lid"] = f"LID-MISSING:{rec['member_phone']}"
    vals[0] = esc(rec["member_lid"])

# Emit a SELECT to reserve the next id (template with __NEXT_ID__ marker)
table = os.environ.get("TABLE", "member_details")
NEXT_ID_SQL = f"SELECT COALESCE(MAX(id), 0) + 1 FROM {table}"
print(NEXT_ID_SQL + "\n__STMT_END__")

# Emit the INSERT with __NEXT_ID__ placeholder in the id slot
INSERT_SQL = f"INSERT INTO {table} (id, {', '.join(cols)}) VALUES (__NEXT_ID__, {', '.join(vals)})"
print(INSERT_SQL + "\n__STMT_END__")

# Emit the SELECT for verify (uses rec.member_lid which the shell will
# fill in via WAIT_FOR_VERIFY)
import sys as _sys
_sys.stderr.write(f"# rec={json.dumps(rec)}\n")
PY
)
PY_RC=$?

# If Python failed (non-zero or empty output), daemon likely down.
if [ "$PY_RC" != "0" ] || [ -z "$RAW" ]; then
    wa_log "Python helper failed (rc=$PY_RC); daemon may be down"
    if [ $((RESTART_COUNT + 1)) -lt "$MAX_RESTARTS" ]; then
        if restart_daemon "$((RESTART_COUNT + 1))" 90; then
            RESTART_COUNT=$((RESTART_COUNT + 1))
            continue
        fi
    fi
    wa_log "max restarts reached; aborting"
    exit 1
fi

# Check if any Python RPC returned exhaustion
if echo "$RAW" | grep -q "exhausted"; then
    wa_log "Python RPC exhausted; restarting daemon"
    if [ $((RESTART_COUNT + 1)) -lt "$MAX_RESTARTS" ]; then
        if restart_daemon "$((RESTART_COUNT + 1))" 90; then
            RESTART_COUNT=$((RESTART_COUNT + 1))
            continue
        fi
    fi
    wa_log "max restarts reached; aborting"
    exit 1
fi

# === Step 2: ensure table exists ======================================

echo "→ ensuring table $TABLE exists" >&2
DDL_SQL="CREATE TABLE IF NOT EXISTS $TABLE (id INTEGER PRIMARY KEY, member_lid TEXT UNIQUE NOT NULL, member_phone TEXT, user_info_jid TEXT, is_business INTEGER, verified_name TEXT, status_text TEXT, picture_id TEXT, picture_url TEXT, picture_url_fetched_ts INTEGER, on_whatsapp INTEGER, devices_count INTEGER, business_profile_status TEXT, business_description TEXT, business_address TEXT, business_hours TEXT, business_website TEXT, business_email TEXT, business_categories TEXT, fetched_at_ts INTEGER)"
esc_ddl=$(printf '%s' "$DDL_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
DDL_RESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.execute\",\"arguments\":{\"sql\":$esc_ddl}}}")
if echo "$DDL_RESP" | grep -q "exhausted"; then
    wa_log "DDL mcp_call exhausted; restarting daemon"
    if [ $((RESTART_COUNT + 1)) -lt "$MAX_RESTARTS" ]; then
        if restart_daemon "$((RESTART_COUNT + 1))" 90; then
            RESTART_COUNT=$((RESTART_COUNT + 1))
            continue
        fi
    fi
    wa_log "max restarts reached; aborting"
    exit 1
fi
echo "$DDL_RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('DDL failed:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(1)
print('  ok' if 'result' in r else '?')
" 2>&1 | tail -1
sleep "$SLEEP_SECS"

# === Step 3: dispatch the helper's SQL (reserve id + INSERT) ==========

declare cur_sql=""
declare -i stmt_idx=0
declare next_id=0
declare insert_sql=""
while IFS= read -r line; do
    # If daemon went down mid-dispatch, stop processing
    ${SQL_EXHAUSTED:-false} && break
    if [ "$line" = "__STMT_END__" ]; then
        stmt_idx=$((stmt_idx+1))
        if [ -z "$cur_sql" ]; then
            cur_sql=""
            continue
        fi
        esc_sql=$(printf '%s' "$cur_sql" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
        if [ "$stmt_idx" = "1" ]; then
            # Reserve the next id via SELECT
            echo "→ reserving id" >&2
            RESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":$stmt_idx,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_sql}}}")
            if echo "$RESP" | grep -q "exhausted"; then SQL_EXHAUSTED=true; fi
            sleep "$SLEEP_SECS"
            next_id=$(printf '%s' "$RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print(0); sys.exit(0)
try:
    txt = r['result']['content'][0]['text']
    body = json.loads(txt)
    rows = body.get('rows', [])
    print(rows[0][0] if rows else 0)
except Exception:
    print(0)
")
            echo "  reserved id = $next_id" >&2
        else
            INSERT_SQL=$(printf '%s' "$cur_sql" | sed "s/__NEXT_ID__/${next_id}/")
            echo "→ INSERT 1" >&2
            esc_sql=$(printf '%s' "$INSERT_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
            RESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":$stmt_idx,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.execute\",\"arguments\":{\"sql\":$esc_sql}}}")
            if echo "$RESP" | grep -q "exhausted"; then SQL_EXHAUSTED=true; fi
            sleep "$SLEEP_SECS"
            affected=$(printf '%s' "$RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('ERR:' + r['error'].get('message','?'))
    sys.exit(0)
try:
    txt = r['result']['content'][0]['text']
    body = json.loads(txt)
    if body.get('code') is not None and body.get('code') != 0:
        print('ERR:' + body.get('message','?'))
    else:
        print(body.get('rows_affected', '?'))
except Exception as e:
    print('?', e)
")
            echo "  rows_affected=$affected" >&2
        fi
        cur_sql=""
    elif [ -n "$line" ]; then
        cur_sql="${cur_sql:+$cur_sql }$line"
    fi
done <<< "$RAW"

# === SQL dispatch exhaustion gate ====================================
# If any mcp_call returned "exhausted", the dispatch loop set SQL_EXHAUSTED.
SQL_EXHAUSTED="${SQL_EXHAUSTED:-false}"
if $SQL_EXHAUSTED; then
    wa_log "SQL dispatch exhausted; restarting daemon"
    if [ $((RESTART_COUNT + 1)) -lt "$MAX_RESTARTS" ]; then
        if restart_daemon "$((RESTART_COUNT + 1))" 90; then
            RESTART_COUNT=$((RESTART_COUNT + 1))
            continue
        fi
    fi
    wa_log "max restarts reached; aborting"
    exit 1
fi

# === Step 4: verify (SELECT) ==========================================

echo "→ verifying row in $TABLE" >&2
if [ -n "$LID_RAW" ]; then
    WHERE_LID="$LID_RAW"
elif [ -n "$PHONE_RAW" ]; then
    WHERE_LID="LID-MISSING:${PHONE_RAW}@s.whatsapp.net"
else
    WHERE_LID=""
fi

if [ -z "$WHERE_LID" ]; then
    echo "  (no member_lid available, skipping verify)" >&2
else
    SELECT_SQL="SELECT * FROM $TABLE WHERE member_lid = '$(printf '%s' "$WHERE_LID" | sed "s/'/''/g")'"
    esc_select=$(printf '%s' "$SELECT_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
    VRESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_select}}}")
    sleep "$SLEEP_SECS"
    printf '%s' "$VRESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('  SELECT failed:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(0)
txt = r['result']['content'][0]['text']
data = json.loads(txt)
print('  columns =', data.get('columns'))
rows = data.get('rows', [])
if rows:
    print('  row     =', rows[0])
else:
    print('  row     = (none)')
"
fi

    pipeline_done=true
done  # while loop

if ! $pipeline_done; then
    echo "  FAILED after $MAX_RESTARTS restart attempts" >&2
    exit 1
fi

echo "→ done" >&2
echo "  peer     = $PEER" >&2
echo "  table    = $TABLE" >&2
