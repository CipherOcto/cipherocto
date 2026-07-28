#!/usr/bin/env bash
# scripts/persist-contacts-bulk.sh
#
# Save every row in a detail table (e.g. liberdade_details) as a WA
# contact via contacts.save_contact. Pick the contact name from
# `verified_name` when populated, else fall back to the phone number.
# Skip members whose `member_lid` is the admin's local JID (we don't
# want to overwrite the operator's own saved contact).
#
# Pipeline:
#   1. sql.query SELECT member_lid, member_phone, verified_name FROM <source>
#   2. for each row: contacts.save_contact (deduped by phone)
#   3. final report: enumerated, unique, saved, skipped, failed
#
# Args:
#   $1  source table (default: liberdade_details)
#   $2  admin LID to skip (default: 80836284174444@lid — the operator's
#             local-identity-best-guess; override if you have a different
#             admin)
#   $3  limit (default: 1000)
#   $4  offset (default: 0)
#
# Env:
#   OCTO_WA_BIN       path to octo-whatsapp binary
#   OCTO_WA_SLEEP     seconds between every WA RPC (default: 3)
#
# Usage:
#   scripts/persist-contacts-bulk.sh liberdade_details
#   scripts/persist-contacts-bulk.sh liberdade_details 80836284174444@lid 50 0

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib-octo-wa.sh"

SOURCE="${1:-liberdade_details}"
ADMIN_LID="${2:-80836284174444@lid}"
LIMIT="${3:-1000}"
OFFSET="${4:-0}"

# === Token check (same shape as the other flex scripts) ====================

if ! [[ "$SOURCE" =~ ^[A-Za-z_][A-Za-z0-9_$#]*$ ]]; then
    echo "invalid source table: $SOURCE" >&2
    exit 3
fi

# === Pre-run health check ===============================================

if ! wa_health_check; then
    wa_log "aborting: daemon not connected"
    exit 1
fi

[ -x "$OCTO_WA_BIN" ] || { echo "binary not executable: $OCTO_WA_BIN" >&2; exit 1; }

# === MCP-over-stdio transport (provided by lib-octo-wa.sh) ============

# === Step 1: enumerate detail rows =======================================

echo "→ reading up to $LIMIT rows from $SOURCE (offset $OFFSET)" >&2
SELECT_SQL="SELECT member_lid, member_phone, verified_name FROM $SOURCE WHERE member_phone IS NOT NULL ORDER BY id LIMIT $LIMIT OFFSET $OFFSET"
esc_select=$(printf '%s' "$SELECT_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
RESP=$(mcp_call_sleep "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_select}}}")

# === Step 2: extract rows + dedup + save each as a contact ==============

ROWS_TMP=$(mktemp)
printf '%s' "$RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('SELECT failed:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(3)
txt = r['result']['content'][0]['text']
data = json.loads(txt)
rows = data.get('rows', [])
seen = set()
raw_count = 0
for row in rows:
    if not row:
        continue
    raw_count += 1
    # columns = [member_lid, member_phone, verified_name]
    lid = row[0] if len(row) > 0 else None
    phone = row[1] if len(row) > 1 else None
    name = row[2] if len(row) > 2 else None
    if not phone:
        continue
    # member_phone is '@s.whatsapp.net' suffixed already; pass as-is.
    # Dedup by the phone JID — same person once even if multiple rows.
    if phone in seen:
        continue
    seen.add(phone)
    # Replace any tab/newline in name with single space
    if name:
        name = name.replace('\t', ' ').replace('\n', ' ').strip()
    print(f'{lid or \"\"}\t{phone}\t{name or \"\"}')
print(f'# raw={raw_count} unique={len(seen)}', file=sys.stderr)
" > "$ROWS_TMP"
COUNT=$(wc -l < "$ROWS_TMP" | tr -d ' ')
echo "  enumerated $COUNT unique rows" >&2

if [ "$COUNT" = "0" ]; then
    echo "  nothing to do" >&2
    rm -f "$ROWS_TMP"
    exit 0
fi

# === Step 3: dispatch each row through contacts.save_contact ==============

declare -i processed=0 saved=0 skipped=0 failed=0
declare -i idx=0
while IFS=$'\t' read -r lid phone name; do
    idx=$((idx+1))
    # Skip the operator's own LID
    if [ -n "$lid" ] && [ "$lid" = "$ADMIN_LID" ]; then
        echo "→ [$idx/$COUNT] skip admin peer=$phone" >&2
        skipped=$((skipped+1))
        continue
    fi
    # Compute the contact name: verified_name if non-empty, else phone
    if [ -n "$name" ]; then
        full_name="$name"
    else
        # Phone is the JID form; strip the suffix for the name fallback
        full_name="${phone%@s.whatsapp.net}"
    fi
    processed=$((processed+1))
    echo "→ [$idx/$COUNT] save contact full_name='$full_name' peer=$phone" >&2
    esc_name=$(printf '%s' "$full_name" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
    esc_peer=$(printf '%s' "$phone" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
    RESP=$(mcp_call_sleep "{\"jsonrpc\":\"2.0\",\"id\":$idx,\"method\":\"tools/call\",\"params\":{\"name\":\"contacts.save_contact\",\"arguments\":{\"full_name\":$esc_name,\"peer\":$esc_peer}}}")
    # Inspect the response — 'status: saved' on success
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
    else
        failed=$((failed+1))
    fi
done < "$ROWS_TMP"
rm -f "$ROWS_TMP"

# === Step 4: report ======================================================

echo "→ done" >&2
echo "  source       = $SOURCE (offset $OFFSET, limit $LIMIT)" >&2
echo "  admin_lid    = $ADMIN_LID" >&2
echo "  enumerated   = $COUNT" >&2
echo "  processed    = $processed" >&2
echo "  saved        = $saved" >&2
echo "  skipped      = $skipped" >&2
echo "  failed       = $failed" >&2
