# Mission: Allowed Fails

## Status

Open

## RFC

RFC-0954 (Economics): Advanced Routing Features

## Dependencies

None

## Acceptance Criteria

- [ ] Model removed after N consecutive failures
- [ ] Fail count resets outside time window
- [ ] Cooldown period prevents immediate retry
- [ ] Per-model overrides work
- [ ] /health/models endpoint returns ModelHealthResponse schema
- [ ] Cooldown times bounded (prevent infinite cooldown)
- [ ] Fail counts per-process (not shared across instances)
- [ ] Config: allowed_fails, allowed_fails_window, cooldown_time
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass

## Claimant

Unclaimed

## Pull Request

None

## Notes

- Default: 3 fails in 60s → 300s cooldown
- Per-model overrides in router_settings
- Health tracking per-process (not shared)
