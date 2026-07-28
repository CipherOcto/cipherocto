#!/usr/bin/env bash
# scripts/join-enrich-table.sh
#
# JOIN two tables on member_lid and create <table2>_enriched with all
# columns. Overlapping columns (member_lid, member_phone) deduped from
# table2 (the "detail" side). Table1 adds: jid, is_admin,
# member_phone_source, ts_unix_ms.
#
# Args:
#   $1  table1 (e.g. group_members) — the "group membership" side
#   $2  table2 (e.g. will_details)  — the "detail" side
#
# Env:
#   OCTO_WA_BIN, OCTO_WA_SOCKET, OCTO_WA_NAME
#
# Usage:
#   scripts/join-enrich-table.sh group_members will_details

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib-octo-wa.sh"

T1="${1:?usage: join-enrich-table.sh <table1> <table2>}"
T2="${2:?usage: join-enrich-table.sh <table1> <table2>}"
OUT="${T2}_enriched"

for t in "$T1" "$T2" "$OUT"; do
    [[ "$t" =~ ^[A-Za-z_][A-Za-z0-9_$#]*$ ]] || { echo "invalid name: $t" >&2; exit 3; }
done

# Pre-run health check
wa_health_check || { wa_log "aborting: daemon not connected"; exit 1; }
[ -x "$OCTO_WA_BIN" ] || { echo "binary not executable: $OCTO_WA_BIN" >&2; exit 1; }

# Helper: sql_exec runs sql.execute via direct MCP call (no retry)
sql_exec() {
    local label="$1" sql="$2"
    echo "→ $label" >&2
    esc=$(printf '%s' "$sql" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
    req="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.execute\",\"arguments\":{\"sql\":$esc}}}"
    tmp=$(mktemp)
    printf '%s\n' "$req" > "$tmp"
    RESP=$(env \
        XDG_RUNTIME_DIR=/tmp/octo-wa-run \
        OCTO_WHATSAPP_DATA_DIR=/home/mmacedoeu/.local/share/octo/whatsapp \
        "$OCTO_WA_BIN" --name "$OCTO_WA_NAME" mcp < "$tmp" 2>/dev/null)
    rc=$?
    rm -f "$tmp"
    if [ "$rc" != "0" ] || [ -z "$RESP" ]; then
        echo "  FAILED (rc=$rc)" >&2
        exit 1
    fi
    echo "$RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('  FAILED:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(1)
try:
    txt = r['result']['content'][0]['text']
    data = json.loads(txt)
    aff = data.get('rows_affected', '?')
    print('  rows_affected=' + str(aff), file=sys.stderr)
except Exception:
    print('  ok', file=sys.stderr)
" || exit 1
}

echo "→ joining $T1 + $T2 into $OUT" >&2

# Drop old enriched table so we rebuild from scratch
sql_exec "dropping old $OUT" "DROP TABLE IF EXISTS $OUT"

sql_exec "creating $OUT" \
"CREATE TABLE $OUT AS
SELECT
  MAX(t2.id)              AS id,
  MAX(t2.member_lid)       AS member_lid,
  MAX(t2.member_phone)     AS member_phone,
  MAX(t2.user_info_jid)    AS user_info_jid,
  MAX(t2.is_business)      AS is_business,
  MAX(t2.verified_name)    AS verified_name,
  MAX(t2.status_text)      AS status_text,
  MAX(t2.picture_id)       AS picture_id,
  MAX(t2.picture_url)      AS picture_url,
  CAST(MAX(t2.picture_url_fetched_ts) AS TEXT) AS picture_url_fetched_ts,
  MAX(t2.on_whatsapp)      AS on_whatsapp,
  MAX(t2.devices_count)    AS devices_count,
  MAX(t2.business_profile_status) AS business_profile_status,
  MAX(t2.business_description) AS business_description,
  MAX(t2.business_address) AS business_address,
  MAX(t2.business_hours)   AS business_hours,
  MAX(t2.business_website) AS business_website,
  MAX(t2.business_email)   AS business_email,
  MAX(t2.business_categories) AS business_categories,
  MAX(t2.fetched_at_ts)    AS fetched_at_ts,
  GROUP_CONCAT(DISTINCT t1.jid) AS group_jids,
  COALESCE(MAX(t1.is_admin), 0) AS is_admin,
  GROUP_CONCAT(DISTINCT t1.member_phone_source) AS member_phone_sources,
  MAX(t1.ts_unix_ms)       AS group_ts_unix_ms
FROM $T2 t2
LEFT JOIN $T1 t1 ON t1.member_lid = t2.member_lid
GROUP BY t2.id"

# Verify
echo "→ verifying $OUT" >&2
esc_verify=$(printf '%s' "SELECT COUNT(*) AS cnt FROM $OUT" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
vreq="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_verify}}}"
vtmp=$(mktemp)
printf '%s\n' "$vreq" > "$vtmp"
VRESP=$(env XDG_RUNTIME_DIR=/tmp/octo-wa-run OCTO_WHATSAPP_DATA_DIR=/home/mmacedoeu/.local/share/octo/whatsapp \
    "$OCTO_WA_BIN" --name "$OCTO_WA_NAME" mcp < "$vtmp" 2>/dev/null)
rm -f "$vtmp"
echo "$VRESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
txt = r['result']['content'][0]['text']
data = json.loads(txt)
row = data['rows'][0]
print(f'  rows={row[0]}', file=sys.stderr)
"

echo "→ done"
echo "  table = $OUT"
echo "  source1 = $T1"
echo "  source2 = $T2"
