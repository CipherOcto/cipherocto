# Mission: 0850p-a — session export to migrate a session DB between hosts

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work"

## Summary

Add a `session export <ACCOUNT_ID> --out <BUNDLE>` subcommand that produces a portable bundle (session DB + sidecar + config) that can be transferred to another host and re-imported via `session import <BUNDLE>`. Useful for migrating a paired bot to a new gateway host without re-pairing (which requires operator interaction with the phone).

## Design

The bundle is a tarball (gzip-compressed) containing:
- `session.db` — the stoolap session DB
- `session_meta.json` — the sidecar
- `config.json` — the per-account WhatsAppConfig

```bash
$ octo-whatsapp-onboard session export +15551234567 --out my-bot.tar.gz
$ scp my-bot.tar.gz new-gateway:
$ ssh new-gateway octo-whatsapp-onboard session import my-bot.tar.gz
```

The `session import` command decompresses, validates the sidecar (checksum match), registers the account in the multi-account index (F5), and exits 0.

## Acceptance Criteria

- [ ] `session export <ACCOUNT_ID> --out <BUNDLE>` produces a tarball
- [ ] `session import <BUNDLE>` registers the account
- [ ] Bundle includes checksum; import verifies it
- [ ] Unit test: round-trip export → import → list shows the account
- [ ] Documentation: security warning about exporting session DBs (they contain Signal keys)

## Dependencies

Depends on:
- Mission 0850p-a-multi-account (the stoolap store for cross-host import)
- RFC-0850p-a status: Accepted

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-whatsapp-onboard/src/session_export.rs` (new).

## Complexity

Low (~200 lines; bundle format, import/export CLI).

## Prerequisites

- Mission 0850p-a-multi-account (the target store for imports)

## Notes

### Why a portable bundle?

A bare TDLib SQLite DB is not portable across hosts without its sidecar (`session_meta.json`) and config. The bundle packages all three into a single `tar.gz`.

### Why not just copy the DB file?

Sidecar management is per-host; the bundle includes the sidecar so the import can recreate it on the new host.

### Type Coverage

| RFC-0850p-a Type | Implemented By |
|-----------------|----------------|
| `session export <DB> --out <FILE>` CLI subcommand | This mission |
| `session import <FILE>` CLI subcommand | This mission |
| Portable bundle format (DB + sidecar + config) | This mission |

### Implementation Guide

Reference: TDLib database format (`sqlite3` file); `tar` crate for the portable bundle.

## Mitigates

Operational scaling; not a security issue.

## Deadline

Post-launch
