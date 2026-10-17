#!/usr/bin/env bash
# scripts/dump-table.sh
#
# Dump the full contents of a detail table (or any SQL table) to a
# file on disk. Mirrors the MCP-over-stdio transport used by the other
# flex scripts; same table-name token check.
#
# The script uses sql.query, which is hard-capped at 10000 rows by the
# daemon. For larger tables, chunk with OFFSET — the operator can wrap
# the call in a loop if needed.
#
# Args:
#   $1  source table name (e.g. liberdade_details, group_members)
#   $2  output file path (default: ./<table>.csv in the cwd)
#   $3  format: csv|json (default: csv)
#
# Env:
#   OCTO_WA_BIN       path to octo-whatsapp binary
#
# Usage:
#   scripts/dump-table.sh liberdade_details
#   scripts/dump-table.sh liberdade_details /tmp/liberdade.csv
#   scripts/dump-table.sh liberdade_details /tmp/liberdade.json json
#   scripts/dump-table.sh group_members /tmp/gm.csv csv

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib-octo-wa.sh"

TABLE="${1:?usage: dump-table.sh <table> [out-file] [csv|json]}"
OUT_FILE="${2:-./${TABLE}.${3:-csv}}"
FORMAT="${3:-csv}"

# === Token check (same shape as the other flex scripts) ====================

if ! [[ "$TABLE" =~ ^[A-Za-z_][A-Za-z0-9_$#]*$ ]]; then
    echo "invalid table name: $TABLE" >&2
    exit 3
fi

case "$FORMAT" in
    csv|json) ;;
    *) echo "invalid format: $FORMAT (expected: csv | json)" >&2; exit 3 ;;
esac

# === Pre-run health check ===============================================

if ! wa_health_check; then
    wa_log "aborting: daemon not connected"
    exit 1
fi

[ -x "$OCTO_WA_BIN" ] || { echo "binary not executable: $OCTO_WA_BIN" >&2; exit 1; }

# === MCP-over-stdio transport (provided by lib-octo-wa.sh) ============

# === Step 1: SELECT ========================================================

echo "→ dumping $TABLE to $OUT_FILE (format=$FORMAT)" >&2
SELECT_SQL="SELECT * FROM $TABLE"
esc_select=$(printf '%s' "$SELECT_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
RESP=$(mcp_call "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_select}}}")

# === Step 2: format and write ============================================

printf '%s' "$RESP" | python3 -c "
import json, sys, csv
r = json.load(sys.stdin)
if 'error' in r:
    print('SELECT failed:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(3)
txt = r['result']['content'][0]['text']
data = json.loads(txt)
columns = data.get('columns', [])
rows = data.get('rows', [])
truncated = data.get('truncated', False)
format = '$FORMAT'
out_path = '$OUT_FILE'
if format == 'csv':
    with open(out_path, 'w', newline='') as f:
        w = csv.writer(f)
        w.writerow(columns)
        for row in rows:
            w.writerow(row)
else:
    payload = {'columns': columns, 'rows': rows, 'truncated': truncated}
    with open(out_path, 'w') as f:
        json.dump(payload, f, indent=2)
print(f'  rows={len(rows)} truncated={truncated} out={out_path}', file=sys.stderr)
"