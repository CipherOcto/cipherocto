# Mission: /v1/responses Endpoint

## Status

Open

## RFC

RFC-0951 (Economics): Extended API Endpoints

## Dependencies

None

## Acceptance Criteria

- [ ] POST /v1/responses creates response
- [ ] GET /v1/responses/{response_id} gets response
- [ ] DELETE /v1/responses/{response_id} deletes response
- [ ] Supports OpenAI Responses API format
- [ ] Input accepts both string and Vec<InputItem> formats
- [ ] Streaming works via SSE
- [ ] Response content sanitized for logging
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

- OpenAI Responses API: stateful conversations
- Input can be text or list of items
- Supports function calling via tools
