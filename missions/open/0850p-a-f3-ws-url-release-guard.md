# Mission: 0850p-a F3 — --ws-url release-build guard

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work" F3

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

## Mitigates

D-WA-5 (`--ws-url` flag for test injection in Adversary Analysis)

## Deadline

Pre-public-launch
