# Design: PyO3 Python SDK Bindings (RFC-0908)

**Date:** 2026-03-12
**RFC:** RFC-0908 (Economics): Python SDK and PyO3 Bindings
**Mission:** Mission-0908-a: Python SDK - PyO3 Core Bindings

## Overview

Create PyO3 Python bindings for the Rust quota-router implementation, enabling drop-in replacement for LiteLLM users.

## Architecture

### Crate Structure

```
crates/
├── quota-router-core/       # NEW - Core library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── balance.rs       # Moved from CLI
│       ├── providers.rs     # Moved from CLI
│       ├── config.rs       # Moved from CLI
│       └── proxy.rs        # Moved from CLI
│
├── quota-router-cli/        # Updated - CLI app
│   ├── Cargo.toml          # Depends on core
│   └── src/
│       ├── lib.rs          # Re-export core
│       ├── cli.rs
│       ├── commands.rs
│       └── main.rs
│
└── quota-router-pyo3/       # NEW - PyO3 bindings
    ├── Cargo.toml
    └── src/
        ├── lib.rs          # PyModule entry
        ├── exceptions.rs    # LiteLLM exceptions
        ├── types.rs        # Message, Response types
        └── completion.rs   # completion/acompletion
```

### Dependencies

- **pyo3** "0.20" with features: extension-module
- **pyo3-asyncio** for async Python ↔ Rust bridging
- **quota-router-core** path dependency

## Design Decisions

### D1: Tokio Runtime

Using `pyo3-asyncio` for async bridging (not new Tokio runtime):
- Better performance (no runtime overhead per call)
- Non-blocking
- Compatible with Python's asyncio event loop

### D2: Exception Handling

LiteLLM-compatible exception classes:
- `AuthenticationError`
- `RateLimitError`
- `BudgetExceededError`
- `ProviderError`
- `TimeoutError`
- `InvalidRequestError`

### D3: Return Types

Return native Python `dict` objects (not custom classes) for LiteLLM compatibility.

## Implementation Steps

### Step 1: Create quota-router-core

- [ ] 1.1 Create `crates/quota-router-core/Cargo.toml`
- [ ] 1.2 Create `crates/quota-router-core/src/lib.rs`
- [ ] 1.3 Move `balance.rs` from CLI
- [ ] 1.4 Move `providers.rs` from CLI
- [ ] 1.5 Move `config.rs` from CLI
- [ ] 1.6 Move `proxy.rs` from CLI
- [ ] 1.7 Update workspace `Cargo.toml` to include new crate
- [ ] 1.8 Update CLI `Cargo.toml` to depend on core
- [ ] 1.9 Update CLI `lib.rs` to re-export from core
- [ ] 1.10 Verify build passes

### Step 2: Create quota-router-pyo3 crate

- [ ] 2.1 Create `crates/quota-router-pyo3/Cargo.toml`
- [ ] 2.2 Add pyo3 dependencies
- [ ] 2.3 Create `src/lib.rs` with PyModule setup

### Step 3: Implement exceptions

- [ ] 3.1 Create `src/exceptions.rs`
- [ ] 3.2 Implement AuthenticationError
- [ ] 3.3 Implement RateLimitError
- [ ] 3.4 Implement BudgetExceededError
- [ ] 3.5 Implement ProviderError
- [ ] 3.6 Implement conversion traits to PyErr
- [ ] 3.7 Register exceptions in PyModule

### Step 4: Implement types

- [ ] 4.1 Create `src/types.rs`
- [ ] 4.2 Implement Message struct
- [ ] 4.3 Implement ChatCompletion struct
- [ ] 4.4 Implement Choice struct
- [ ] 4.5 Implement Usage struct
- [ ] 4.6 Implement ToPyObject for response types

### Step 5: Implement completion functions

- [ ] 5.1 Create `src/completion.rs`
- [ ] 5.2 Implement acompletion (async)
- [ ] 5.3 Implement completion (sync wrapper)
- [ ] 5.4 Add parameter support (temperature, max_tokens, etc.)
- [ ] 5.5 Wire to quota-router-core

### Step 6: Testing

- [ ] 6.1 Build wheel locally
- [ ] 6.2 Test `import quota_router`
- [ ] 6.3 Test exception raising
- [ ] 6.4 Test completion call (mock)
- [ ] 6.5 Add unit tests

### Step 7: Type stubs

- [ ] 7.1 Generate .pyi stubs
- [ ] 7.2 Verify mypy compatibility

## Testing Strategy

```python
# Test import
import quota_router

# Test exceptions
try:
    raise quota_router.AuthenticationError("test")
except quota_router.AuthenticationError:
    pass

# Test completion
response = quota_router.completion(
    model="gpt-4",
    messages=[{"role": "user", "content": "hello"}]
)
assert response["choices"][0]["message"]["content"]
```

## Success Criteria

- [ ] PyPI-installable wheel
- [ ] `import quota_router` works
- [ ] Exception parity with LiteLLM
- [ ] completion() returns LiteLLM-compatible response
- [ ] Type stubs for IDE support
- [ ] <10ms function call overhead

## Related RFCs

- RFC-0908: Python SDK and PyO3 Bindings
- RFC-0902: Multi-Provider Routing (future)
- RFC-0903: Virtual API Key System (future)
- RFC-0906: Response Caching (future)
