# Mission: Perplexity Provider

## Status

Completed

## RFC

RFC-0952 (Economics): Additional Providers

## Dependencies

None

## Acceptance Criteria

- [x] PerplexityProvider implements HttpProvider trait
- [x] Supports chat/completions endpoint
- [x] Supports streaming via SSE
- [x] Model string inference works ("perplexity/*")
- [x] Environment variable config (PERPLEXITY_API_KEY)
- [x] Citations preserved in response
- [ ] Perplexity-specific params work (deferred to Python SDK) (return_citations, search_domain_filter, search_recency_filter)
- [x] API key masked in logs
- [x] Error mapping follows RFC-0920 taxonomy
- [x] Works in litellm-mode (reqwest)
- [x] Works in any-llm-mode (py_bridge)
- [x] Unit tests pass
- [x] Integration tests pass (with mock server)

## Claimant

@claude

## Pull Request

None

## Notes

- Perplexity uses OpenAI-compatible API format
- Endpoint: https://api.perplexity.ai/chat/completions
- Auth: Bearer token
- Models: sonar-small-online, sonar-medium-online, sonar-large-online
- Extra fields: citations, search_domain_filter, search_recency_filter
