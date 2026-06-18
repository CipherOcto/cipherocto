# Mission: Model Group Alias

## Status

Completed

Open

## RFC

RFC-0954 (Economics): Advanced Routing Features

## Dependencies

None

## Acceptance Criteria

- [x] Model alias resolves to correct model
- [x] Weighted selection works for model groups
- [x] Routing strategy applies to model groups
- [x] Config: model_list with model_name alias
- [x] Simple alias (single model) works
- [x] Tiered alias (multiple models) works
- [x] Per-model max_input_tokens supported
- [x] Alias cannot bypass routing restrictions
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

- Alias format: model_name: "best-model" → model_list: [...]
- Weight-based selection for load balancing
- Strategy-based selection (least-busy, round-robin)
