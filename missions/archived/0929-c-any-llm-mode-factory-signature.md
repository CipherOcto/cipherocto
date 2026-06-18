# Mission: 0929-c — any-llm-mode Factory Signature Update

## Status

Archived (superseded by 0929-c claimed — `missions/archived/0929-c-any-llm-mode-factory-api-base.md`)

## RFC

RFC-0929 (Economics): GatewayConfig Provider Dispatch Mapping

## Dependencies

- Mission-0929-a: DispatchInfo Struct and to_provider_map Implementation (must be complete)

## Acceptance Criteria

- [ ] py_bridge::factory::completion() signature updated to accept api_base: Option<&str>
- [ ] api_base parameter forwarded through completion call chain without logging
- [ ] Provider.with_api_base() applied at completion call time (builder pattern: `provider.with_api_key(key).with_api_base(api_base).completion(...)`)
- [ ] No log statement in the call chain contains api_base value (verify via code review + grep for "log" + "api_base" co-occurrence)
- [ ] Clippy passes with zero warnings
- [ ] Existing tests pass

## Claimant

#

## Notes

RFC-0929 REQUIRED change: The current `py_bridge::factory::completion()` signature has 4 args (provider, model, messages, api_key). This mission adds the 5th arg (api_base) per the RFC specification.

The factory signature update enables per-deployment api_base support in any-llm-mode (PyO3 path).

**Builder pattern:** `with_api_base()` follows the same builder pattern as `with_api_key()`:
```rust
// Current (without api_base):
p.with_api_key(key.to_string()).completion(model, messages)

// After (with api_base):
p.with_api_key(key.to_string())
    .with_api_base(api_base.to_string())
    .completion(model, messages)
```

Each provider (OpenAI, Anthropic, etc.) has `with_api_base(api_base: String) -> Self` which sets `self.api_base = Some(api_base)`. The api_base is applied at completion call time, not at provider creation.