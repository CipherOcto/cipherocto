# Mission: Align Rust CLI/Library with Python SDK Exports

## Status
Archived
Completed

## RFC

RFC-0908 (Economics): Python SDK and PyO3 Bindings

## Dependencies

- Mission-0908-a: Python SDK - PyO3 Core Bindings (completed)

## Acceptance Criteria

- [x] Audit current `quota-router-cli` exports vs Python SDK expected exports
- [x] Extract quota-router-core crate (done)
- [x] Add `completion()` / `acompletion()` functions to Rust library (via PyO3)
- [x] Add `embedding()` / `aembedding()` functions to Rust library (via PyO3)
- [x] Add exception types matching LiteLLM (AuthenticationError, RateLimitError, BudgetExceededError, ProviderError)
- [x] Add `Router` struct with routing strategies to quota-router-core
- [x] Add completion functions to quota-router-core library
- [x] Update CLI to match LiteLLM-style commands
- [x] Add OpenAI-compatible `/v1/chat/completions` endpoint to proxy
- [x] Add `/v1/embeddings` endpoint to proxy
- [x] Implement config loading from YAML (RFC-0907)
- [x] Add routing strategies: least-busy, latency-based, cost-based
- [x] Add fallback provider logic
- [ ] Add response caching (RFC-0906) - deferred
- [x] Unit tests for all new functions

## Description

Update the current Rust CLI and library implementation to match the export signatures defined in the Python SDK (RFC-0908). The Rust CLI should expose the same functionality as the Python SDK, ensuring both can be used interchangeably.

## Current State vs Target

### Current Exports (quota-router-cli)

```rust
// lib.rs
pub mod balance;
pub mod cli;
pub mod commands;
pub mod config;
pub mod providers;
pub mod proxy;
```

```rust
// CLI Commands
enum Commands {
    Init,
    AddProvider { name: String },
    Balance,
    List { prompts: u64, price: u64 },
    Proxy { port: u16 },
    Route { provider: String, prompt: String },
}
```

### Target Exports (matching Python SDK)

```rust
// Core functions (must match Python signatures)
pub async fn acompletion(
    model: String,
    messages: Vec<Message>,
    // ... params
) -> Result<ModelResponse, Error>;

pub fn completion(model: String, messages: Vec<Message>) -> Result<ModelResponse, Error>;

pub async fn aembedding(
    input: Vec<String>,
    model: String,
) -> Result<EmbeddingResponse, Error>;

pub fn embedding(input: Vec<String>, model: String) -> Result<EmbeddingResponse, Error>;

// Router class
pub struct Router {
    // routing strategy
    // fallbacks
    // cache settings
}

// Exceptions
pub struct AuthenticationError;
pub struct RateLimitError;
pub struct BudgetExceededError;
pub struct ProviderError;
```

### Target CLI Commands (LiteLLM-style)

```bash
# Start proxy with config
quota-router --config config.yaml
# or
litellm --config config.yaml

# Health check
quota-router health

# Call embedding
quota-router embed --model text-embedding-3-small --input "hello world"
```

## Technical Details

### Steps

1. **Audit Phase**
   - Compare current lib.rs exports with RFC-0908 Python SDK signatures
   - Identify missing functions/structs

2. **Core Functions Implementation**
   - Add `completion.rs` with acompletion/completion functions
   - Add `embedding.rs` with aembedding/embedding functions
   - Add `router.rs` with Router struct
   - Add `exceptions.rs` with LiteLLM-compatible errors

3. **Proxy Enhancement**
   - Update proxy to handle OpenAI-compatible endpoints:
     - `POST /v1/chat/completions`
     - `POST /v1/embeddings`
     - `GET /v1/models`
   - Implement proper request/response handling

4. **CLI Update**
   - Add subcommands matching LiteLLM CLI
   - Add `--config` flag support
   - Add `--model` flag support

## Notes

This mission ensures Rust and Python implementations stay aligned. The Rust CLI should be usable as:
- Standalone CLI tool
- Library for embedding in other Rust applications
- Backend for PyO3 Python bindings

This mission blocks the PyO3 binding missions as they depend on the Rust core having the correct exports.

---

## Claimant

@claude-code

## Pull Request

https://github.com/CipherOcto/cipherocto/pull/36

## Related RFCs

- RFC-0902: Multi-Provider Routing and Load Balancing
- RFC-0903: Virtual API Key System
- RFC-0904: Real-Time Cost Tracking
- RFC-0905: Observability and Logging
- RFC-0906: Response Caching
- RFC-0907: Configuration Management
