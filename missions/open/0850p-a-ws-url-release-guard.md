# Mission: 0850p-a — --ws-url release-build guard

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work"

## Summary

Refuse the `--ws-url` CLI flag in release builds (compiled with `cfg!(not(debug_assertions))`) unless the environment variable `OCTO_WHATSAPP_ALLOW_WS_URL=1` is set. The flag is a debug-only escape hatch for testing against a mock WhatsApp Web server; in production it is an unnecessary attack surface (an attacker who controls the operator's config could redirect the noise handshake to their own server and harvest all future messages).

## Design

In `crates/octo-whatsapp-onboard/src/cli.rs`, after arg parsing:

```rust
fn check_ws_url_allowed(args: &Cli) -> Result<(), CliError> {
    if args.ws_url.is_some() {
        if cfg!(not(debug_assertions)) {
            if std::env::var("OCTO_WHATSAPP_ALLOW_WS_URL").ok().as_deref() != Some("1") {
                return Err(CliError::WsUrlReleaseForbidden);
            }
        }
    }
    Ok(())
}
```

`CliError::WsUrlReleaseForbidden` is a new variant; the help text for `--ws-url` documents the override and warns that it should never be set in production.

## Acceptance Criteria

- [ ] `CliError::WsUrlReleaseForbidden` variant added with actionable message
- [ ] `check_ws_url_allowed` runs for all subcommands that accept `--ws-url` (qr-link, pair-link, serve-qr)
- [ ] Debug builds: `--ws-url` works unconditionally (no env-var check)
- [ ] Release builds: `--ws-url` returns `Err(CliError::WsUrlReleaseForbidden)` unless `OCTO_WHATSAPP_ALLOW_WS_URL=1`
- [ ] `--help` text for `--ws-url` documents the env-var override
- [ ] Unit test: simulated release build (compile-time check) refuses without env-var


### Implementation Guide

Reference: `crates/octo-whatsapp-onboard/src/cli.rs` (CLI arg parsing); `cfg!(debug_assertions)` macro.


### Type Coverage

| RFC-0850p-a Type | Implemented By |
|-----------------|----------------|
| `CliError::WsUrlReleaseForbidden` variant | This mission |
| `OCTO_WHATSAPP_ALLOW_WS_URL` env var check | This mission |

## Dependencies

Depends on the base 0850p-a RFC being Accepted. No prerequisite missions; this is a release-build guard on the CLI.

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-whatsapp-onboard/src/cli.rs` (add release guard after arg parsing).

## Complexity

Trivial (~10 lines; one check).

## Prerequisites

- RFC-0850p-a status: Accepted

## Notes

### Why release-only?

Debug builds may use `--ws-url` for local testing (a custom WebSocket endpoint, e.g., a test server). Release builds should never use a custom URL because the official WhatsApp servers are the only trusted endpoints.

### Why an env-var override?

Operators running a custom proxy (e.g., a corporate MITM) can opt-in with the env var. The check is opt-out (default: forbidden in release).

## Mitigates

D-WA-5 (`--ws-url` flag for test injection in Adversary Analysis)

## Deadline

Pre-public-launch
