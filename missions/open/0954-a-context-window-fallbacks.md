# Mission: Context Window Fallbacks

## Status

Open

## RFC

RFC-0954 (Economics): Advanced Routing Features

## Dependencies

None

## Acceptance Criteria

- [ ] Context window fallback triggers when input exceeds limit
- [ ] Fallback models tried in order
- [ ] Token counting works correctly
- [ ] Config: context_window_fallbacks in model_list
- [ ] Returns ContextLengthExceededError if no fallback works
- [ ] Health endpoint does not expose API keys
- [ ] Cooldown times bounded (prevent infinite cooldown)
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass

## Claimant

Unclaimed

## Pull Request

None

## Notes

- Requires max_input_tokens per model
- Fallback order matters (try largest context first)
- Token counting can use tiktoken or similar
