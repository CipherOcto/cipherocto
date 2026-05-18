# Mission: Config Hot Reload

## Status

Completed

Open

## RFC

RFC-0907 (Economics): Configuration Management

## Dependencies

None

## Acceptance Criteria

- [x] YAML config file loads successfully
- [x] Environment variable overrides work (os.environ['VAR_NAME'] syntax)
- [x] Dollar syntax works (${VAR_NAME})
- [x] Environment variable overrides take precedence over file values
- [x] Config file watcher detects changes
- [x] Hot reload via SIGHUP signal
- [x] Hot reload via file watcher (notify crate)
- [x] Config validation catches missing required fields
- [x] Config validation catches invalid port numbers
- [x] Graceful error handling (invalid config rejected, previous preserved)
- [x] Provider inference from model string works
- [x] api_base fallback chain works (4 tiers)
- [x] CLI commands work: config validate, config show, config reload
- [x] Config snapshots stored in stoolap
- [x] API keys stored securely (not in plaintext)
- [x] Config file permissions restricted (600 or 640)
- [x] No downtime during reload
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

- Use notify crate for file watching
- Validate new config before applying
- Rollback to previous config on failure
- Log reload events
