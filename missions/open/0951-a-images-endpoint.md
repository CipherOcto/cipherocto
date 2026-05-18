# Mission: /v1/images/generations Endpoint

## Status

Open

## RFC

RFC-0951 (Economics): Extended API Endpoints

## Dependencies

None

## Acceptance Criteria

- [ ] POST /v1/images/generations accepts ImageGenerationRequest
- [ ] Supports OpenAI (DALL-E) provider
- [ ] Supports Stability AI provider
- [ ] Returns valid image URLs or base64 encoded data
- [ ] Error handling follows RFC-0920 taxonomy
- [ ] Image format validation (size, type)
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass

## Claimant

Unclaimed

## Pull Request

None

## Notes

- OpenAI uses https://api.openai.com/v1/images/generations
- Stability AI uses https://api.stability.ai/v1/generation/{engine}/text-to-image
- Response format: url or b64_json
