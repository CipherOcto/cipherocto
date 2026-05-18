# Mission: Databricks Provider

## Status

Open

## RFC

RFC-0952 (Economics): Additional Providers

## Dependencies

None

## Acceptance Criteria

- [ ] DatabricksProvider implements HttpProvider trait
- [ ] Supports chat/completions endpoint
- [ ] Supports streaming via SSE
- [ ] Model string inference works ("databricks/*")
- [ ] Environment variable config (DATABRICKS_BASE_URL, DATABRICKS_API_KEY)
- [ ] Workspace URL validated (HTTPS only)
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

- Databricks uses OpenAI-compatible API format
- Endpoint: https://{workspace}.databricks.com/serving-endpoints/{endpoint}/invocations
- Auth: Bearer token (Databricks PAT)
- Supports DBRX models
