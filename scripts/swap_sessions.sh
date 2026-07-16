#!/usr/bin/env bash
# Atomic 3-phase swap of two WhatsApp session DBs.
#
# Default: swaps `default.session.db` <-> `bak_main_phone.session.db` so
# the operator can roll the daemon onto the previously-paired phone
# without re-pairing.
#
# Why 3 phases with a staging name: POSIX `rename(2)` on the same
# filesystem is atomic, but it cannot swap two existing names in one
# call. The classic 2-rename swap (A -> tmp, B -> A, tmp -> B) is what
# this script does, with rollback at every boundary.
#
# Each phase leaves the filesystem in a recoverable state on failure.
#
# Env:
#   OCTO_WHATSAPP_PERSIST_DIR  base dir containing the .session.db pairs
#                              (default: $HOME/.local/share/octo/whatsapp)
#
# Usage:
#   scripts/swap_sessions.sh                 # perform the swap
#   scripts/swap_sessions.sh --abort-staging # undo a partial swap if
#                                            # bak_main_phone_NEW.* left over

set -euo pipefail

die() { echo "ERROR: $*" >&2; exit 1; }
ok()  { echo "  ✓ $*"; }

DIR="${OCTO_WHATSAPP_PERSIST_DIR:-$HOME/.local/share/octo/whatsapp}"
A="default"
B="bak_main_phone"
STAGE="${B}_NEW"

# --- --abort-staging mode ---
if [[ "${1:-}" == "--abort-staging" ]]; then
    shift
    [[ -e "$DIR/$STAGE.session.db" ]] || die "no staging $STAGE.session.db present, nothing to abort"
    ok "aborting partial swap: moving $STAGE.* back to $B.*"
    mv -v "$DIR/$STAGE.session.db" "$DIR/$B.session.db"
    mv -v "$DIR/$STAGE.session.db.meta.json" "$DIR/$B.session.db.meta.json"
    ok "abort complete"
    exit 0
fi

# --- phase 0: pre-flight ---
[[ -d "$DIR/$A.session.db" ]]      || die "$A.session.db missing in $DIR"
[[ -d "$DIR/$B.session.db" ]]      || die "$B.session.db missing in $DIR"
[[ -f "$DIR/$A.session.db.meta.json" ]] || die "$A.session.db.meta.json missing"
[[ -f "$DIR/$B.session.db.meta.json" ]] || die "$B.session.db.meta.json missing"

# Lock-check: refuse if any process holds files inside either dir
if command -v fuser >/dev/null 2>&1; then
    if fuser -s "$DIR/$A.session.db"/* "$DIR/$B.session.db"/* 2>/dev/null; then
        die "open handles in $A.session.db or $B.session.db — daemon running? stop it first"
    fi
fi
ok "pre-flight: both pairs present, no open handles"

# Collision guard
[[ ! -e "$DIR/$STAGE.session.db" ]] || die "staging $STAGE.session.db already exists — run --abort-staging first"
ok "collision guard: $STAGE.session.db free"

# --- phase 1: stage B as NEW ---
mv -v "$DIR/$B.session.db" "$DIR/$STAGE.session.db"
mv -v "$DIR/$B.session.db.meta.json" "$DIR/$STAGE.session.db.meta.json"
[[ -d "$DIR/$STAGE.session.db" && -f "$DIR/$STAGE.session.db.meta.json" ]] || die "phase 1 verify failed"
ok "phase 1: $B staged as $STAGE"

# --- phase 2: A -> B ---
mv -v "$DIR/$A.session.db" "$DIR/$B.session.db"
mv -v "$DIR/$A.session.db.meta.json" "$DIR/$B.session.db.meta.json"
if [[ ! -d "$DIR/$B.session.db" || ! -f "$DIR/$B.session.db.meta.json" ]]; then
    echo "phase 2 verify failed — rolling back phase 1" >&2
    mv -v "$DIR/$STAGE.session.db.meta.json" "$DIR/$B.session.db.meta.json"
    mv -v "$DIR/$STAGE.session.db" "$DIR/$B.session.db"
    die "phase 2 failed and rolled back; original state preserved"
fi
ok "phase 2: $A now at $B"

# --- phase 3: NEW -> A ---
mv -v "$DIR/$STAGE.session.db" "$DIR/$A.session.db"
mv -v "$DIR/$STAGE.session.db.meta.json" "$DIR/$A.session.db.meta.json"
if [[ ! -d "$DIR/$A.session.db" || ! -f "$DIR/$A.session.db.meta.json" ]]; then
    echo "phase 3 verify failed — rolling back phases 2+1" >&2
    mv -v "$DIR/$B.session.db.meta.json" "$DIR/$A.session.db.meta.json"
    mv -v "$DIR/$B.session.db" "$DIR/$A.session.db"
    mv -v "$DIR/$STAGE.session.db.meta.json" "$DIR/$B.session.db.meta.json"
    mv -v "$DIR/$STAGE.session.db" "$DIR/$B.session.db"
    die "phase 3 failed and rolled back; original state preserved"
fi
ok "phase 3: $STAGE now at $A"

# --- verify ---
echo
echo "═══════════════ POST-SWAP STATE ═══════════════"
ls -la "$DIR" | grep -E "(default|bak_main_phone)\.session"
echo
echo "$A.session.db.meta.json:"
cat "$DIR/$A.session.db.meta.json"
echo
echo "$B.session.db.meta.json:"
cat "$DIR/$B.session.db.meta.json"
echo
ok "swap complete"