# Mission: 0850p-a F1 — serve-qr over HTTP

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work" F1

## Summary

Add a `serve-qr` subcommand to `octo-whatsapp-onboard` that exposes the QR code over HTTP for headless / no-TTY deployments (SSH without TTY, containerized gateways, CI runners with browser access). The CLI binds to `127.0.0.1:PORT` (configurable via `--bind`), serves a minimal HTML page that displays the QR PNG, and exits after the first successful `Event::Connected` (or after `--timeout`, default 300s).

## Design

- New subcommand in `crates/octo-whatsapp-onboard/src/cli.rs`:
  - `serve-qr --bind <ADDR> --port <PORT> --timeout <SECS> --session-path <DIR> --out <CONFIG>`
- Reuses the existing `qr_link::run` core function (no core changes) by:
  1. Starting a `tokio::net::TcpListener` on the configured bind/port
  2. Spawning `qr_link::run` as a task that produces the QR (via existing `Event::PairingQrCode` → `qrcode::QrCode::new(...)` → PNG bytes)
  3. The HTTP handler streams a minimal HTML page with a `<img>` tag that polls `/qr.png` every 1s; the PNG endpoint returns the current QR or a 204 if the link completed
  4. On `Event::Connected`, the HTTP server is shut down (the operator has scanned), the config is written, and the CLI exits 0
- Bind defaults to `127.0.0.1` (loopback only) to avoid accidental public exposure. `--bind 0.0.0.0` is allowed with a CLI warning ("publicly exposing QR codes is a security risk").
- HTML page contains a `<meta http-equiv="refresh" content="1">` auto-refresh; the QR PNG rotates every 60s per WhatsApp protocol.

## Acceptance Criteria

- [ ] `octo-whatsapp-onboard serve-qr` subcommand added
- [ ] Binds to `127.0.0.1:PORT` by default; `--bind` and `--port` configurable
- [ ] Serves a self-contained HTML page (no external CDN/JS) that displays the rotating QR
- [ ] Exits 0 on first `Event::Connected`; exits 2 on `Event::LoggedOut` or timeout
- [ ] Reuses `qr_link::run` (no changes to `octo-whatsapp-onboard-core`)
- [ ] Unit test: HTTP handler returns 200 + PNG when QR is available
- [ ] Integration test: spin up the CLI, fetch the HTML page, simulate scan via adapter mock
- [ ] Help text documents the security warning for `--bind 0.0.0.0`

## Mitigates

D-WA-4 (headless / SSH-without-TTY assumption in Implicit Assumptions Audit)

## Deadline

Pre-public-launch
