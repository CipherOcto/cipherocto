# Mission: /v1/rerank Endpoint

## Status

Completed

Open

## RFC

RFC-0951 (Economics): Extended API Endpoints

## Dependencies

None

## Acceptance Criteria

- [x] POST /v1/rerank accepts RerankRequest
- [x] Supports Cohere rerank API
- [x] Supports Jina rerank API
- [x] Returns ranked results with relevance scores
- [x] Error handling follows RFC-0920 taxonomy
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

- Cohere: https://api.cohere.ai/v1/rerank
- Jina: https://api.jina.ai/v1/rerank
- Used for RAG/search re-ranking
