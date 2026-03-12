# Mission: Python SDK - PyO3 Core Bindings

## Status

Open

## RFC

RFC-0908 (Economics): Python SDK and PyO3 Bindings

## Dependencies

None - this is the foundational mission for Python SDK

## Acceptance Criteria

- [ ] PyO3 Cargo crate setup in quota-router-cli
- [ ] Basic module exports (`__init__.py`)
- [ ] Exception classes matching LiteLLM
- [ ] Completion function binding (sync)
- [ ] Completion function binding (async)
- [ ] Basic error handling
- [ ] Unit tests for core functions
- [ ] Type stubs (.pyi) for IDE support

## Description

Create the core PyO3 bindings for the Rust quota-router, enabling Python to call Rust functions directly. This is the foundation for the drop-in replacement SDK.

## Technical Details

### PyO3 Crate Structure

```toml
# crates/quota-router-pyo3/Cargo.toml
[package]
name = "quota-router-pyo3"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
pyo3 = { version = "0.20", features = ["extension-module"] }
quota-router-core = { path = "../quota-router-core" }
```

### Core Exports

```python
# quota_router/__init__.py
from quota_router import (
    completion,
    acompletion,
    AuthenticationError,
    RateLimitError,
    BudgetExceededError,
)
```

## Notes

This mission blocks all other Python SDK missions (0908-b, 0908-c, 0908-d).

---

**Claimant:** Open

**Related Missions:**
- Mission-0908-b: Python SDK Router Class
- Mission-0908-c: Embedding Functions
- Mission-0908-d: PyPI Package Release
