# Mission: 0850p-a — CI-mode non-interactive pair-link

## Status

Open (2026-06-16) — pre-public-launch follow-up

## RFC

RFC-0850p-a (Networking): WhatsApp Auth Onboarding — §"Future Work"

## Summary

Add a `--ci` flag to `pair-link` that bypasses the `Event::Connected` wait and uses a pre-shared session DB (typically mounted as a Kubernetes Secret or CI artifact). For CI/CD environments where the phone is not available, the bot is pre-paired by an operator and the resulting session DB is checked into the CI's secret store.

## Design

```bash
$ octo-whatsapp-onboard pair-link --ci --session-path <MOUNTED_SECRET> --out <CONFIG>
```

The CI mode:
1. Skips the `Event::Connected` wait
2. Verifies the session DB is valid (calls `has_valid_session()` from F6 once that's implemented, or checks `self_handle().is_some()`)
3. Writes the sidecar and config
4. Exits 0 on success, 1 on invalid session DB

The mode is opt-in (`--ci`); a normal `pair-link` invocation still requires the operator to scan the QR or type the code.

## Acceptance Criteria

- [ ] `pair-link --ci --session-path <PATH>` works without operator interaction
- [ ] Validates the session DB before writing the config
- [ ] Unit test: `--ci` with a valid pre-paired DB exits 0
- [ ] Unit test: `--ci` with an empty/invalid DB exits 1 with a clear error
- [ ] Documentation: CI integration guide (Kubernetes Secret, GitHub Actions encrypted secret, etc.)

## Dependencies

Depends on:
- Mission 0850p-a-session-export (the source of pre-paired session DBs)
- Mission 0850p-a-multi-account (the CI host's account store)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-whatsapp-onboard/src/pair_link.rs` (add `--no-wait` flag).

## Complexity

Trivial (~30 lines; one new flag, no-wait path).

## Prerequisites

- Mission 0850p-a-session-export (to produce the pre-paired DB)

## Notes

### Why pre-paired DBs?

CI environments have no human to scan a QR code. The pre-paired DB is created on a host that does have a human, exported via `session export`, and mounted into the CI container.

### Why is this LOW severity?

The CI use case is narrow. Most operators use `octo-whatsapp-onboard` interactively.

### Type Coverage

| RFC-0850p-a Type | Implemented By |
|-----------------|----------------|
| `pair-link --no-wait --session-db <PATH>` flag | This mission |
| CI-friendly error codes | This mission |

### Implementation Guide

Reference: `crates/octo-whatsapp-onboard/src/pair_link.rs` (existing `pair-link` subcommand).

## Mitigates

Operational scaling; not a security issue. **Security note:** the session DB contains Signal keys; CI must use encrypted secret storage.

## Deadline

Post-launch
