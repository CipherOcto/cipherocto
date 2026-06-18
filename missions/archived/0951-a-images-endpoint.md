# Mission: /v1/images/generations Endpoint

## Status

Completed

Open

## RFC

RFC-0951 (Economics): Extended API Endpoints

## Dependencies

None

## Acceptance Criteria

- [x] POST /v1/images/generations accepts ImageGenerationRequest
- [x] Supports OpenAI (DALL-E) provider
- [x] Supports Stability AI provider
- [x] Returns valid image URLs or base64 encoded data
- [x] Error handling follows RFC-0920 taxonomy
- [x] Image format validation (size, type)
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

- OpenAI uses https://api.openai.com/v1/images/generations
- Stability AI uses https://api.stability.ai/v1/generation/{engine}/text-to-image
- Response format: url or b64_json
