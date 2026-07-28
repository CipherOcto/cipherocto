#!/usr/bin/env bash
# scripts/persist-member-details-bulk.sh
#
# Bulk-runs persist-member-detail-flex.sh for every row in a source
# table (the one the GROUP flex script writes — schema `jid, member_lid,
# is_admin, member_phone, member_phone_source, ts_unix_ms`). Each row
# is dispatched as a peer, with the phone passed as the hint so the
# detail script can populate the full detail row even when the
# LID→phone usync lookup fails (privacy-hidden LIDs).
#
# Source table contract: must have columns `member_lid` (TEXT) and
# `member_phone` (TEXT, possibly `@s.whatsapp.net` suffixed). Rows
# missing both are skipped. Rows with only a phone (e.g. `@lid` was
# NULL) are dispatched by phone; the detail script's get_user_info
# resolves the LID back.
#
# Pipeline:
#   1.   sql.query    SELECT member_lid, member_phone FROM <source> LIMIT N OFFSET M
#   1.5. sql.query    SELECT member_lid, member_phone FROM <dest> (filter set)
#   2.   loop: detail-script <member_lid> <dest> <phone_hint>
#        (each call costs 5-7 WA RPCs, all 3s-spaced by the detail script)
#        rows already in <dest> (by lid OR phone) are skipped — no dispatch
#   3.   final report: already_in_dest, processed, succeeded, failed, skipped
#
# Args:
#   $1  source table name (e.g. liberdade, group_members, percurso_with_phone)
#   $2  destination table name (default: member_details)
#   $3  max rows (default: 1000) — caps the run to keep wall-clock bounded
#   $4  offset (default: 0) — for chunked runs
#
# Env:
#   OCTO_WA_BIN       path to octo-whatsapp binary (default: matches detail script)
#   OCTO_WA_DETAIL    path to persist-member-detail-flex.sh (default: ./scripts/...)
#   OCTO_WA_SLEEP     extra seconds between detail-script invocations (default: 3)
#
# Usage:
#   scripts/persist-member-details-bulk.sh liberdade
#   scripts/persist-member-details-bulk.sh liberdade member_details 50 0
#   scripts/persist-member-details-bulk.sh group_members member_details 100 0

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
. "$SCRIPT_DIR/lib-octo-wa.sh"

SOURCE="${1:?usage: persist-member-details-bulk.sh <source-table> [dest-table] [limit] [offset]}"
DEST="${2:-member_details}"
LIMIT="${3:-1000}"
OFFSET="${4:-0}"

# === Token check (same shape as the other flex scripts) ====================

if ! [[ "$SOURCE" =~ ^[A-Za-z_][A-Za-z0-9_$#]*$ ]]; then
    echo "invalid source table: $SOURCE" >&2
    exit 3
fi
if ! [[ "$DEST" =~ ^[A-Za-z_][A-Za-z0-9_$#]*$ ]]; then
    echo "invalid dest table: $DEST" >&2
    exit 3
fi

# === Pre-run health check ===============================================

if ! wa_health_check; then
    wa_log "aborting: daemon not connected"
    exit 1
fi

BIN="${OCTO_WA_BIN:-/home/mmacedoeu/_w/ai/cipherocto/target/debug/octo-whatsapp}"
DETAIL="${OCTO_WA_DETAIL:-$HOME/_w/ai/cipherocto/scripts/persist-member-detail-flex.sh}"
SLEEP_SECS="${OCTO_WA_SLEEP:-3}"

[ -x "$BIN" ] || { echo "binary not executable: $BIN" >&2; exit 1; }
[ -x "$DETAIL" ] || { echo "detail script not executable: $DETAIL" >&2; exit 1; }

# === MCP-over-stdio transport (provided by lib-octo-wa.sh) ============

# === Step 1: enumerate source rows =======================================

echo "→ reading up to $LIMIT rows from $SOURCE (offset $OFFSET)" >&2
# stoolap's cipherocto fork (feat/blockchain-sql) silently ignores
# DISTINCT when combined with ORDER BY + LIMIT together. Each clause
# alone works (regression-tested in the stoolap repo at
# tests/cipherocto_unique_distinct_regression_test.rs), but the
# triplet `DISTINCT ... ORDER BY col LIMIT N` returns duplicates.
# We can't use SQL DISTINCT here, so dedup in Python by
# (member_lid, member_phone) pair. The admin appearing in 12 JIDs
# would otherwise produce 12 detail-script invocations for the same
# person.
SELECT_SQL="SELECT member_lid, member_phone FROM $SOURCE WHERE member_lid IS NOT NULL OR member_phone IS NOT NULL ORDER BY member_lid LIMIT $LIMIT OFFSET $OFFSET"
esc_select=$(printf '%s' "$SELECT_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
RESP=$(mcp_call_sleep "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_select}}}")

# === Step 1.5: enumerate existing rows in $DEST (filter) ==============
#
# Pull every (member_lid, member_phone) pair already in $DEST so we skip
# re-dispatching the detail script for peers we've already persisted.
# Each detail invocation costs 5-7 WA RPCs × 3s sleep ≈ 15-25s, so
# skipping known rows is the biggest wall-clock win when re-running on
# a populated destination. (Previously the script re-fetched every row
# and the daemon's INSERT collided on the UNIQUE constraint, silently
# failing the row without notifying the operator.)
#
# The dest table has UNIQUE on member_lid (NULL-allowed per
# `persist-member-detail-flex.sh` schema), so we filter by both lid AND
# phone to catch the case where the lid is NULL (LID-MISSING:sentinel
# rows written by the detail script when usync lookup fails).

echo "→ reading existing rows from $DEST" >&2
EXISTING_SQL="SELECT member_lid, member_phone FROM $DEST"
esc_existing=$(printf '%s' "$EXISTING_SQL" | python3 -c "import json,sys; print(json.dumps(sys.stdin.read()))")
EXISTING_RESP=$(mcp_call_sleep "{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"tools/call\",\"params\":{\"name\":\"sql.query\",\"arguments\":{\"sql\":$esc_existing}}}")
EXISTING_TMP=$(mktemp)
printf '%s' "$EXISTING_RESP" | python3 -c "
import json, sys
r = json.load(sys.stdin)
if 'error' in r:
    print('EXISTING SELECT failed:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(0)
try:
    txt = r['result']['content'][0]['text']
    data = json.loads(txt)
    for row in data.get('rows', []):
        if not row:
            continue
        lid = row[0] if len(row) > 0 else None
        phone = row[1] if len(row) > 1 else None
        if lid:
            print(lid)
        if phone:
            # strip '@s.whatsapp.net' suffix (or anything after '@')
            print(phone.split('@', 1)[0])
except Exception as e:
    print('parse failed:', e, file=sys.stderr)
" > "$EXISTING_TMP"
EXISTING_COUNT=$(sort -u "$EXISTING_TMP" | wc -l | tr -d ' ')
echo "  already in $DEST: $EXISTING_COUNT unique (lid or phone)" >&2

# === Step 2: extract (lid, phone) pairs into a temp fifo/file ============

ROWS_TMP=$(mktemp)
SKIP_TMP=$(mktemp)
printf '%s' "$RESP" | EXISTING_RAW="$(cat "$EXISTING_TMP" 2>/dev/null)" SKIP_TMP="$SKIP_TMP" python3 -c "
import json, os, sys
existing = set(line.strip() for line in os.environ.get('EXISTING_RAW', '').splitlines() if line.strip())
r = json.load(sys.stdin)
if 'error' in r:
    print('SELECT failed:', r['error'].get('message','?'), file=sys.stderr)
    sys.exit(3)
txt = r['result']['content'][0]['text']
data = json.loads(txt)
rows = data.get('rows', [])
seen = set()
raw_count = 0
skipped_existing = 0
for row in rows:
    if not row:
        continue
    raw_count += 1
    # columns = [member_lid, member_phone]
    lid = row[0] if len(row) > 0 else None
    phone = row[1] if len(row) > 1 else None
    if not lid and not phone:
        continue
    # phone in the source table may be '@s.whatsapp.net' suffixed; strip
    # for the detail script's hint (digits-only).
    phone_hint = ''
    if phone:
        phone_hint = phone.split('@', 1)[0]
    # Skip if peer already exists in destination (either by lid or phone)
    if (lid and lid in existing) or (phone_hint and phone_hint in existing):
        skipped_existing += 1
        continue
    # Dedup by (lid, phone_hint) — same person across JIDs shares both
    key = (lid or '', phone_hint)
    if key in seen:
        continue
    seen.add(key)
    print(f'{lid or \"\"} {phone_hint}')
# Emit skip count to a file so the shell can read it
with open(os.environ.get('SKIP_TMP', '/dev/null'), 'w') as f:
    f.write(str(skipped_existing))
print(f'# raw={raw_count} unique={len(seen)} skipped_existing={skipped_existing}', file=sys.stderr)
" > "$ROWS_TMP"
COUNT=$(wc -l < "$ROWS_TMP" | tr -d ' ')
echo "  enumerated $COUNT rows" >&2

if [ "$COUNT" = "0" ]; then
    echo "  nothing to do" >&2
    rm -f "$ROWS_TMP"
    exit 0
fi

# === Step 3: dispatch each row through the detail script =================

declare -i processed=0 succeeded=0 failed=0 skipped=0
declare -i idx=0
while IFS=' ' read -r lid phone_hint; do
    idx=$((idx+1))
    # Empty peer (both NULL) — already filtered above, but defensive
    if [ -z "$lid" ] && [ -z "$phone_hint" ]; then
        skipped=$((skipped+1))
        continue
    fi
    # Choose the peer form: prefer LID when present (detail script's
    # resolution code prefers phone form of user_info first when given
    # a phone — but a LID is the canonical identity).
    if [ -n "$lid" ]; then
        peer="$lid"
    else
        peer="$phone_hint"
    fi
    # Dispatch
    processed=$((processed+1))
    echo "→ [$idx/$COUNT] peer=$peer hint=${phone_hint:-<none>}" >&2
    DETAIL_OUT=$(mktemp)
    OCTO_WA_BIN="$BIN" "$DETAIL" "$peer" "$DEST" "$phone_hint" > "$DETAIL_OUT" 2>&1 || true
    tail -3 "$DETAIL_OUT" >&2
    # Success = detail script exit 0 AND the row landed in the dest table
    # (UNIQUE on member_lid + the daemon's INSERT may silently fail).
    # Grep the detail script's output for any ERR: marker; if present,
    # count as failed.
    if grep -qE 'ERR:' "$DETAIL_OUT"; then
        failed=$((failed+1))
        echo "  FAILED peer=$peer (UNIQUE collision or RPC error)" >&2
    else
        succeeded=$((succeeded+1))
    fi
    rm -f "$DETAIL_OUT"
    # Inter-detail cooldown — the detail script paces INTERNALLY
    # (3s between every WA RPC), but we add a small buffer between
    # invocations to keep the WA server's anti-rate-limit happy when
    # the detail script's last RPC was right at the boundary.
    sleep "$SLEEP_SECS"
done < "$ROWS_TMP"
rm -f "$ROWS_TMP"
SKIPPED_EXISTING=$(cat "$SKIP_TMP" 2>/dev/null || echo 0)
rm -f "$SKIP_TMP" "$EXISTING_TMP"

# === Step 4: report ======================================================

echo "→ done" >&2
echo "  source       = $SOURCE (offset $OFFSET, limit $LIMIT)" >&2
echo "  destination  = $DEST" >&2
echo "  already_in_dest = ${SKIPPED_EXISTING:-0}" >&2
echo "  enumerated   = $COUNT" >&2
echo "  processed    = $processed" >&2
echo "  succeeded    = $succeeded" >&2
echo "  failed       = $failed" >&2
echo "  skipped      = $skipped" >&2
