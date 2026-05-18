# Mission: Model Group Alias

## Status

Open

## RFC

RFC-0954 (Economics): Advanced Routing Features

## Dependencies

None

## Acceptance Criteria

- [ ] Model alias resolves to correct model
- [ ] Weighted selection works for model groups
- [ ] Routing strategy applies to model groups
- [ ] Config: model_list with model_name alias
- [ ] Simple alias (single model) works
- [ ] Tiered alias (multiple models) works
- [ ] Per-model max_input_tokens supported
- [ ] Alias cannot bypass routing restrictions
- [ ] Works in litellm-mode (reqwest)
- [ ] Works in any-llm-mode (py_bridge)
- [ ] Unit tests pass
- [ ] Integration tests pass

## Claimant

Unclaimed

## Pull Request

None

## Notes

- Alias format: model_name: "best-model" → model_list: [...]
- Weight-based selection for load balancing
- Strategy-based selection (least-busy, round-robin)
