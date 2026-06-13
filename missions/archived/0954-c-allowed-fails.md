# Mission: Allowed Fails

## Status

Completed

Open

## RFC

RFC-0954 (Economics): Advanced Routing Features

## Dependencies

None

## Acceptance Criteria

- [x] Model removed after N consecutive failures
- [x] Fail count resets outside time window
- [x] Cooldown period prevents immediate retry
- [x] Per-model overrides work
- [x] /health/models endpoint returns ModelHealthResponse schema
- [x] Cooldown times bounded (prevent infinite cooldown)
- [x] Fail counts per-process (not shared across instances)
- [x] Config: allowed_fails, allowed_fails_window, cooldown_time
- [x] Works in litellm-mode (reqwest)
- [x] Works in any-llm-mode (py_bridge)
- [x] Unit tests pass
- [x] Integration tests pass

## Claimant

@claude

Unclaimed

## Pull Request

None

## Notes

- Default: 3 fails in 60s → 300s cooldown
- Per-model overrides in router_settings
- Health tracking per-process (not shared)
