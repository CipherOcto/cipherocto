# Mission: Context Window Fallbacks

## Status

Completed

Open

## RFC

RFC-0954 (Economics): Advanced Routing Features

## Dependencies

None

## Acceptance Criteria

- [x] Context window fallback triggers when input exceeds limit
- [x] Fallback models tried in order
- [x] Token counting works correctly
- [x] Config: context_window_fallbacks in model_list
- [x] Returns ContextLengthExceededError if no fallback works
- [x] Health endpoint does not expose API keys
- [x] Cooldown times bounded (prevent infinite cooldown)
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

- Requires max_input_tokens per model
- Fallback order matters (try largest context first)
- Token counting can use tiktoken or similar
