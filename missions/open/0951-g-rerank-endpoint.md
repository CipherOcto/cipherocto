# Mission: /v1/rerank Endpoint

## Status

Open

## RFC

RFC-0951 (Economics): Extended API Endpoints

## Dependencies

None

## Acceptance Criteria

- [ ] POST /v1/rerank accepts RerankRequest
- [ ] Supports Cohere rerank API
- [ ] Supports Jina rerank API
- [ ] Returns ranked results with relevance scores
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

- Cohere: https://api.cohere.ai/v1/rerank
- Jina: https://api.jina.ai/v1/rerank
- Used for RAG/search re-ranking
