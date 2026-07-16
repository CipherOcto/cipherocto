#!/usr/bin/env bash
# aider.sh — Aider shell shim for octo-whatsapp.
#
# Aider has no native MCP support. This shim translates common Aider-style
# commands to `octo-whatsapp` CLI subcommands so an Aider user can drive
# WhatsApp from a shell with familiar names.
#
# Install: cp aider.sh ~/.local/bin/wa && chmod +x ~/.local/bin/wa
# Then use:  wa send-text +15551234567 "hello"
#            wa status
#            wa send-image +15551234567 ./photo.jpg
#
# NOTE: this shim does NOT spawn an MCP server. It only calls the daemon's
# CLI surface. For full tool access (100 tools), use Claude Code / Cursor /
# Continue.dev / Windsurf with the corresponding JSON snippet from this
# directory.

set -euo pipefail

if ! command -v octo-whatsapp >/dev/null 2>&1; then
  echo "error: octo-whatsapp binary not found in PATH" >&2
  echo "       install via: cargo install --path crates/octo-whatsapp" >&2
  exit 127
fi

usage() {
  cat <<'USAGE'
usage: wa <command> [args...]

Common commands:
  send-text <peer> <text>     Send text to peer
  send-image <peer> <file>    Send image (jpg/png/webp)
  send-video <peer> <file>    Send video (mp4)
  send-audio <peer> <file>    Send audio (mp3/aac/ogg)
  send-voice <peer> <file>    Send voice-note (opus/ogg, ptt=true)
  send-sticker <peer> <file>  Send sticker (webp)
  send-poll <peer> <q> <opt1> <opt2> [...]
  send-contact <peer> <name> <phone>
  send-location <peer> <lat> <lon>
  react <peer> <msg_id> <emoji>
  delete-msg <peer> <msg_id>
  status                      Daemon status snapshot
  health                      Daemon health probe
  version                     Daemon version info
  reconnect                   Force reconnect
  shutdown                    Graceful shutdown
  chats-list                  List recent chats
  messages-list <peer> [limit]
  messages-search <query>
  events-tail [limit]         Tail events table
  rules-list                  List rules
  triggers-list               List triggers
  audit-tail [limit]          Tail audit log
  whoami                      Show account id/phone
  accounts-list               List linked accounts
  accounts-use <id>           Switch active account
  help                        Show octo-whatsapp CLI help

Any unknown command is passed through verbatim to `octo-whatsapp`.
USAGE
}

case "${1:-help}" in
  help|--help|-h|"")
    usage
    ;;

  send-text)
    shift; octo-whatsapp send text --peer "$1" --text "$2"
    ;;
  send-image)
    shift; octo-whatsapp send image "$1" "$2"
    ;;
  send-video)
    shift; octo-whatsapp send video "$1" "$2"
    ;;
  send-audio)
    shift; octo-whatsapp send audio "$1" "$2"
    ;;
  send-voice)
    shift; octo-whatsapp send voice "$1" "$2"
    ;;
  send-sticker)
    shift; octo-whatsapp send sticker "$1" "$2"
    ;;
  send-poll)
    shift
    peer="$1"; shift
    q="$1"; shift
    opts=""
    for o in "$@"; do opts="${opts:+$opts,}\"$o\""; done
    octo-whatsapp send poll --peer "$peer" --question "$q" --options "[$opts]"
    ;;
  send-contact)
    shift; octo-whatsapp send contact --peer "$1" --name "$2" --phone "$3"
    ;;
  send-location)
    shift; octo-whatsapp send location --peer "$1" --lat "$2" --lon "$3"
    ;;
  react)
    shift; octo-whatsapp send reaction --peer "$1" --msg-id "$2" --emoji "$3"
    ;;
  delete-msg)
    shift; octo-whatsapp send delete --peer "$1" --msg-id "$2"
    ;;

  status)        shift; octo-whatsapp status "$@" ;;
  health)        shift; octo-whatsapp health "$@" ;;
  version)       shift; octo-whatsapp version "$@" ;;
  reconnect)     shift; octo-whatsapp reconnect "$@" ;;
  shutdown)      shift; octo-whatsapp shutdown "$@" ;;

  chats-list)    shift; octo-whatsapp chats list "$@" ;;
  chats-info)    shift; octo-whatsapp chats info "$@" ;;
  messages-list)
    shift
    octo-whatsapp messages list --peer "$1" ${2:+--limit "$2"}
    ;;
  messages-search)
    shift; octo-whatsapp messages search --query "$1"
    ;;
  events-tail)
    shift; octo-whatsapp events list --limit "${1:-20}"
    ;;

  rules-list)    shift; octo-whatsapp rules list "$@" ;;
  triggers-list) shift; octo-whatsapp triggers list "$@" ;;
  audit-tail)
    shift; octo-whatsapp audit tail --limit "${1:-20}"
    ;;

  whoami)        shift; octo-whatsapp onboard whoami "$@" ;;
  accounts-list) shift; octo-whatsapp accounts list "$@" ;;
  accounts-use)
    shift; octo-whatsapp accounts use --id "$1"
    ;;

  *)
    # Unknown subcommand: pass through to the binary verbatim.
    octo-whatsapp "$@"
    ;;
esac