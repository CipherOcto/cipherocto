# Mission: Config Hot Reload

## Status

Open

## RFC

RFC-0907 (Economics): Configuration Management

## Dependencies

None

## Acceptance Criteria

- [ ] YAML config file loads successfully
- [ ] Environment variable overrides work (os.environ['VAR_NAME'] syntax)
- [ ] Dollar syntax works (${VAR_NAME})
- [ ] Environment variable overrides take precedence over file values
- [ ] Config file watcher detects changes
- [ ] Hot reload via SIGHUP signal
- [ ] Hot reload via file watcher (notify crate)
- [ ] Config validation catches missing required fields
- [ ] Config validation catches invalid port numbers
- [ ] Graceful error handling (invalid config rejected, previous preserved)
- [ ] Provider inference from model string works
- [ ] api_base fallback chain works (4 tiers)
- [ ] CLI commands work: config validate, config show, config reload
- [ ] Config snapshots stored in stoolap
- [ ] API keys stored securely (not in plaintext)
- [ ] Config file permissions restricted (600 or 640)
- [ ] No downtime during reload
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass

## Claimant

Unclaimed

## Pull Request

None

## Notes

- Use notify crate for file watching
- Validate new config before applying
- Rollback to previous config on failure
- Log reload events
