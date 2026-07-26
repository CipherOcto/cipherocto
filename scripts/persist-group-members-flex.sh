#!/usr/bin/env bash
# scripts/persist-group-members-flex.sh
#
# Generic variant of persist-group-members.sh that lets the caller pick
# the destination SQL table. Same pipeline:
#   groups.info (single round-trip; members_with_phone rides along)
#   sql.query    existing rows for this jid
#   classify each member into {insert, update, skip}
#   sql.execute  batches of INSERT + UPDATE … WHERE … IN (...)
#   sql.query    verify count / admins / with_phone
#
# Source tag for `member_phone_source` is fixed at 'group_info'.
#
# Note on PK: the script declares (jid, member_lid) PRIMARY KEY in the
# CREATE TABLE IF NOT EXISTS, but stoolap does NOT enforce composite
# TEXT PRIMARY KEYs at INSERT time (probe confirmed). The
# classification logic still operates at the value level, so the
# right values land in the table; the script does not attempt to
# delete existing duplicate rows.
#
# Args:
#   $1  group JID (e.g. 120363425575546925@g.us)
#   $2  target table name (e.g. group_members_percursorj)
#       MUST be a valid SQL identifier (we just stamp it into DDL/DML
#       after a token check — no quoting, no escaping). Default:
#       "group_members".
#
# Env (override as needed):
#   OCTO_WA_BIN        path to octo-whatsapp binary
#   OCTO_WA_SOCKET     unix socket path
#   OCTO_WA_NAME       daemon instance name (default: default)
#   OCTO_WA_BATCH      row keys per IN-list UPDATE batch (default: 200)
#
# Usage:
#   scripts/persist-group-members-flex.sh 120363425575546925@g.us group_members_percursorj

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib-octo-wa.sh"

JID="${1:?usage: persist-group-members-flex.sh <jid> <table>}"
TABLE="${2:-group_members}"

# Token check
if ! [[ "$TABLE" =~ ^[A-Za-z_][A-Za-z0-9_$#]*$ ]]; then
    echo "invalid table name: $TABLE" >&2
    echo "  must match ^[A-Za-z_][A-Za-z0-9_$#]*$" >&2
    exit 3
fi

BIN="${OCTO_WA_BIN:-/home/mmacedoeu/_w/ai/cipherocto/.worktrees/whatsapp-runtime-cli-mcp/target/debug/octo-whatsapp}"
SOCKET="${OCTO_WA_SOCKET:-/tmp/octo-wa-run/octo-whatsapp-default.sock}"
NAME="${OCTO_WA_NAME:-default}"
BATCH="${OCTO_WA_BATCH:-200}"
NOW_MS="$(date +%s%3N)"

# === Pre-run health check ==========================================

if ! wa_health_check; then
    wa_log "aborting: daemon not connected"
    exit 1
fi

[ -x "$BIN" ] || { echo "binary not executable: $BIN" >&2; exit 1; }
[ -S "$SOCKET" ] || { echo "socket not bound: $SOCKET" >&2; exit 2; }

# === MCP-over-stdio transport (provided by lib-octo-wa.sh) =========

# === Step 1: fetch the group metadata ===
echo "→ fetching members for $JID  (target table: $TABLE)" >&2
RAW_TMP=$(mktemp)
mcp_call "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"groups.info\",\"arguments\":{\"jid\":\"$JID\"}}}" > "$RAW_TMP"

# === Step 1b: ensure the table exists ===
echo "→ ensuring table $TABLE exists" >&2
DDL_SQL="CREATE TABLE IF NOT EXISTS $TABLE (jid TEXT, member_lid TEXT, is_admin INTEGER, member_phone TEXT, member_phone_source TEXT, ts_unix_ms INTEGER, PRIMARY KEY (jid, member_lid))"
esc_ddl=$(printf '%s' "$DDL_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
DDL_RESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.execute\",\"arguments\":{\"sql\":$esc_ddl}}}")
echo "$DDL_RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('DDL failed:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(1)
print('  ok' if 'result' in r else '?')
" 2>&1 | tail -1

# === Step 2: load existing rows for this jid ===
echo "→ loading existing rows for $JID from $TABLE" >&2
SELECT_SQL="SELECT member_lid, member_phone, member_phone_source, is_admin FROM $TABLE WHERE jid = '$JID'"
esc_select=$(printf '%s' "$SELECT_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
SELECT_RESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_select}}}")
EXISTING_TMP=$(mktemp)
printf '%s' "$SELECT_RESP" > "$EXISTING_TMP"

# === Step 3: classify + emit batches ===
echo "→ classifying rows into insert/update/skip" >&2
WORK=$(python3 - "$JID" "$NOW_MS" "$BATCH" "$TABLE" "$RAW_TMP" "$EXISTING_TMP" <<'PY'
import json, sys

jid, now_ms, batch, table = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
raw_path, existing_path = sys.argv[5], sys.argv[6]

with open(raw_path) as f:
    raw = json.load(f)
if "error" in raw:
    print("groups.info failed:", raw["error"], file=sys.stderr)
    sys.exit(3)
text = raw["result"]["content"][0]["text"]
g = json.loads(text)

members = g.get("members", [])
admins = set(g.get("admins", []))

phone_for_member = {}
for entry in g.get("members_with_phone", []):
    phone_for_member[entry["jid"]] = entry["phone"]

existing_map = {}
with open(existing_path) as f:
    raw_e = json.load(f)
if "error" in raw_e:
    print("sql.query failed:", raw_e["error"], file=sys.stderr)
    sys.exit(3)
text_e = raw_e["result"]["content"][0]["text"]
try:
    data_e = json.loads(text_e)
except Exception:
    data_e = {}
for row in data_e.get("rows", []):
    ml, ph, ph_src, is_a = (row + [None, None, None])[:4]
    existing_map.setdefault(ml, []).append((ph, ph_src, is_a))

def esc(s):
    return s.replace(chr(39), chr(39)+chr(39))

inserts = []
to_update = []
skipped = inserted = updated = 0

for m in members:
    phone = phone_for_member.get(m)
    is_a = 1 if m in admins else 0
    rows_for_m = existing_map.get(m, [])
    if not rows_for_m:
        if phone:
            inserts.append(
                f"('{jid}', '{esc(m)}', {is_a}, '{esc(phone)}', "
                f"'group_info', {now_ms})"
            )
        else:
            inserts.append(
                f"('{jid}', '{esc(m)}', {is_a}, NULL, NULL, {now_ms})"
            )
        inserted += 1
        continue
    cur_phone, cur_src, cur_is_a = rows_for_m[0]
    needs_phone_update = bool(phone) and (cur_phone is None or cur_phone != phone)
    needs_admin_update = cur_is_a != is_a
    if not needs_phone_update and not needs_admin_update:
        skipped += 1
        continue
    to_update.append((m, phone, is_a))
    updated += 1

print(
        f"# {len(members)} members, {len(admins)} admins, "
        f"{sum(1 for m in members if m in phone_for_member)} with phone; "
        f"insert={inserted} update={updated} skip={skipped}",
        file=sys.stderr)

def chunks_of(seq):
    for i in range(0, len(seq), batch):
        yield seq[i:i+batch]

for chunk in chunks_of(inserts):
    sql = (
        f"INSERT INTO {table} "
        f"(jid, member_lid, is_admin, member_phone, member_phone_source, ts_unix_ms) "
        f"VALUES {', '.join(chunk)}"
    )
    print(sql + "\n__STMT_END__")

for chunk in chunks_of(to_update):
    is_admin_pieces = []
    phone_pieces = []
    src_pieces = []
    in_keys = []
    for (m, phone, is_a) in chunk:
        key = f"'{esc(m)}'"
        in_keys.append(key)
        is_admin_pieces.append(
            f"WHEN member_lid = {key} THEN {is_a}"
        )
        if phone:
            phone_pieces.append(
                f"WHEN member_lid = {key} THEN '{esc(phone)}'"
            )
            src_pieces.append(
                f"WHEN member_lid = {key} THEN 'group_info'"
            )
        else:
            phone_pieces.append(
                f"WHEN member_lid = {key} THEN NULL"
            )
            src_pieces.append(
                f"WHEN member_lid = {key} THEN NULL"
            )
    in_clause = ", ".join(in_keys)
    sql = (
        f"UPDATE {table} SET "
        f"is_admin = CASE {' '.join(is_admin_pieces)} ELSE is_admin END, "
        f"member_phone = CASE {' '.join(phone_pieces)} ELSE member_phone END, "
        f"member_phone_source = CASE {' '.join(src_pieces)} ELSE member_phone_source END, "
        f"ts_unix_ms = {now_ms} "
        f"WHERE jid = '{jid}' AND member_lid IN ({in_clause})"
    )
    print(sql + "\n__STMT_END__")

print(f"# batches: insert_chunks={((len(inserts) + batch - 1) // batch)} "
      f"update_chunks={((len(to_update) + batch - 1) // batch)}",
      file=sys.stderr)
PY
)
rm -f "$RAW_TMP" "$EXISTING_TMP"

# === Step 4: dispatch each statement ===
declare -i total_affected=0
declare -i stmt_idx=0
declare cur_sql=""
while IFS= read -r line; do
    if [ "$line" = "__STMT_END__" ]; then
        stmt_idx=$((stmt_idx+1))
        kind="INSERT"
        [[ "$cur_sql" == UPDATE\ * ]] && kind="UPDATE"
        echo "→ $kind $stmt_idx" >&2
        esc_sql=$(printf '%s' "$cur_sql" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
        RESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":$stmt_idx,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.execute\",\"arguments\":{\"sql\":$esc_sql}}}")
        affected=$(printf '%s' "$RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('ERR:' + r['error'].get('message','?'))
    sys.exit(0)
try:
    txt = r['result']['content'][0]['text']
    print(json.loads(txt).get('rows_affected', '?'))
except Exception as e:
    print('?', e)
" 2>/dev/null || echo "?")
        echo "  rows_affected=$affected" >&2
        [[ "$affected" =~ ^[0-9]+$ ]] && total_affected=$((total_affected + affected))
        cur_sql=""
    elif [ -n "$line" ]; then
        cur_sql="${cur_sql:+$cur_sql }$line"
    fi
done <<< "$WORK"

# === Step 5: verify ===
echo "→ verifying count in $TABLE" >&2
COUNT_SQL="SELECT COUNT(*) AS n, SUM(is_admin) AS admins, COUNT(member_phone) AS with_phone FROM $TABLE WHERE jid = '$JID'"
esc_count=$(printf '%s' "$COUNT_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
VRESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":99,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_count}}}")
SUMMARY=$(printf '%s' "$VRESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
txt = r['result']['content'][0]['text']
data = json.loads(txt)
print(data['rows'][0])
")

echo "→ done" >&2
echo "  jid           = $JID" >&2
echo "  table         = $TABLE" >&2
echo "  affected_total= $total_affected" >&2
echo "  table_state   = $SUMMARY" >&2