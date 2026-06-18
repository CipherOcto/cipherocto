# Mission: /v1/messages Endpoint

## Status

Open

## RFC

RFC-0951 (Economics): Extended API Endpoints

## Dependencies

None

## Acceptance Criteria

- [ ] POST /v1/messages creates message
- [ ] Supports Anthropic Messages API format
- [ ] Returns MessagesResponse with content blocks
- [ ] Streaming works via SSE
- [ ] System prompt support
- [ ] Tool use support
- [ ] Error handling follows RFC-0920 taxonomy
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass

## Claimant

Unclaimed

## Pull Request

None

## Notes

- Anthropic: https://api.anthropic.com/v1/messages
- Native Anthropic format (not OpenAI compatible)
- Content blocks: text, image, tool_use, tool_result
