# Mission: Databricks Provider

## Status

Claimed

## RFC

RFC-0952 (Economics): Additional Providers

## Dependencies

None

## Acceptance Criteria

- [x] DatabricksProvider implements HttpProvider trait
- [x] Supports chat/completions endpoint
- [x] Supports streaming via SSE
- [x] Model string inference works ("databricks/*")
- [x] Environment variable config (DATABRICKS_BASE_URL, DATABRICKS_API_KEY)
- [ ] Workspace URL validated (HTTPS only)
- [x] Error mapping follows RFC-0920 taxonomy
- [x] Works in litellm-mode (reqwest)
- [x] Works in any-llm-mode (py_bridge)
- [x] Unit tests pass
- [ ] Integration tests pass (with mock server)

## Claimant

@claude

## Pull Request

None

## Notes

- Databricks uses OpenAI-compatible API format
- Endpoint: https://{workspace}.databricks.com/serving-endpoints/{endpoint}/invocations
- Auth: Bearer token (Databricks PAT)
- Supports DBRX models
