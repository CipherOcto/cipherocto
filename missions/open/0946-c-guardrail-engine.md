# Mission: 0946-c — Guardrail Engine

## Status

Open

## RFC

RFC-0946 (Economics): Guardrails Framework

## Dependencies

- Mission-0946-a: Guardrail Trait and Registry
- Mission-0946-b: Built-in Guardrails

## Acceptance Criteria

- [ ] Wire `GuardrailExecutor` into `proxy.rs` for pre-call checks (before provider call)
- [ ] Wire `GuardrailExecutor` into `proxy.rs` for post-call checks (after provider response)
- [ ] Implement per-key guardrail overrides (via virtual key metadata)
- [ ] Implement per-model guardrail overrides
- [ ] Log guardrail events via structured logging (RFC-0905)
- [ ] Add Prometheus metrics: `guardrail_checks_total`, `guardrail_blocks_total`, `guardrail_errors_total`, `guardrail_latency_seconds`
- [ ] Add LiteLLM-compatible `input_guardrails`/`output_guardrails` config
- [ ] Clippy passes with zero warnings
- [ ] All existing tests pass

## Claimant

(unclaimed)

## Pull Request

#

## Notes

Key files:
- `crates/quota-router-core/src/guardrails/engine.rs` — New
- `crates/quota-router-core/src/proxy.rs` — Integrate guardrails
