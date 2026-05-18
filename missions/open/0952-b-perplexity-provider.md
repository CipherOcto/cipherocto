# Mission: Perplexity Provider

## Status

Open

## RFC

RFC-0952 (Economics): Additional Providers

## Dependencies

None

## Acceptance Criteria

- [ ] PerplexityProvider implements HttpProvider trait
- [ ] Supports chat/completions endpoint
- [ ] Supports streaming via SSE
- [ ] Model string inference works ("perplexity/*")
- [ ] Environment variable config (PERPLEXITY_API_KEY)
- [ ] Citations preserved in response
- [ ] Perplexity-specific params work (return_citations, search_domain_filter, search_recency_filter)
- [ ] API key masked in logs
- [ ] Error mapping follows RFC-0920 taxonomy
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass (with mock server)

## Claimant

Unclaimed

## Pull Request

None

## Notes

- Perplexity uses OpenAI-compatible API format
- Endpoint: https://api.perplexity.ai/chat/completions
- Auth: Bearer token
- Models: sonar-small-online, sonar-medium-online, sonar-large-online
- Extra fields: citations, search_domain_filter, search_recency_filter
