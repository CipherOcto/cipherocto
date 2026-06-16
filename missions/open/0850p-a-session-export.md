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

## Mitigates

Operational scaling; not a security issue.

## Deadline

Post-launch
