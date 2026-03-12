# Quota Router Python SDK

Drop-in replacement for LiteLLM - AI Gateway with OCTO-W integration.

## Installation

### Prerequisites

- Python 3.12+
- Rust toolchain

### Build from Source

```bash
# Clone and setup
git clone https://github.com/cipherocto/cipherocto.git
cd cipherocto

# Create virtual environment
python -m venv .venv
source .venv/bin/activate

# Install maturin
pip install maturin

# Build and install
maturin develop --manifest-path crates/quota-router-pyo3/Cargo.toml
```

Or from the Python package:

```bash
pip install .
```

## Quick Start

```python
import quota_router as litellm

# Basic completion
response = litellm.completion(
    model="gpt-4",
    messages=[{"role": "user", "content": "Hello!"}]
)
print(response["choices"][0]["message"]["content"])

# Async version
import asyncio

async def main():
    response = await litellm.acompletion(
        model="gpt-4",
        messages=[{"role": "user", "content": "Hello!"}]
    )
    return response

response = asyncio.run(main())

# Embeddings
embedding = litellm.embedding(
    input=["hello world"],
    model="text-embedding-3-small"
)
print(embedding["data"][0]["embedding"][:5])  # First 5 values
```

## API Reference

### Completion

```python
# Sync
litellm.completion(
    model="gpt-4",
    messages=[{"role": "user", "content": "..."}],
    temperature=0.7,      # Optional
    max_tokens=1000,      # Optional
    top_p=1.0,            # Optional
    n=1,                  # Optional
    stream=False,         # Optional
    stop=None,            # Optional
    presence_penalty=0,  # Optional
    frequency_penalty=0, # Optional
    user=None,            # Optional
    api_key=None,         # Optional (quota-router specific)
)

# Async
await litellm.acompletion(...)
```

### Embedding

```python
# Sync
litellm.embedding(
    input="hello world",           # str or List[str]
    model="text-embedding-3-small",
    api_key=None,                  # Optional
)

# Async
await litellm.aembedding(...)
```

### Exceptions

```python
from quota_router import (
    AuthenticationError,
    RateLimitError,
    BudgetExceededError,
    ProviderError,
    TimeoutError,
    InvalidRequestError,
)

try:
    response = litellm.completion(model="gpt-4", messages=[...])
except RateLimitError as e:
    print(f"Rate limited: {e}")
except AuthenticationError as e:
    print(f"Auth failed: {e}")
```

## Configuration

### Environment Variables

```bash
# Provider API keys
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."

# quota-router specific
export QUOTA_ROUTER_CONFIG="/path/to/config.yaml"
```

### Config File

Create a `config.yaml`:

```yaml
balance: 1000
providers:
  - name: openai
    endpoint: https://api.openai.com/v1
  - name: anthropic
    endpoint: https://api.anthropic.com

proxy_port: 8080
```

## LiteLLM Compatibility

This SDK is designed as a drop-in replacement for LiteLLM:

```python
# Replace
import litellm

# With
import quota_router as litellm

# Or use directly
import quota_router as qr
```

All LiteLLM function signatures are supported.

## Development

### Setup Development Environment

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Python 3.12
pyenv install 3.12.9
pyenv local 3.12.9

# Create venv
python -m venv .venv
source .venv/bin/activate

# Install dependencies
pip install maturin pytest mypy

# Build
maturin develop --manifest-path crates/quota-router-pyo3/Cargo.toml
```

### Running Tests

```bash
# Python tests
pytest

# Rust tests
cargo test --package quota-router-pyo3

# All tests
cargo test --all

# Lint
cargo clippy --all-targets -- -D warnings
```

### Smoke Tests

```bash
# Test 1: Import
python -c "import quota_router; print(quota_router.__version__)"

# Test 2: Completion
python -c "
import quota_router
r = quota_router.completion(model='gpt-4', messages=[{'role': 'user', 'content': 'test'}])
assert 'choices' in r
print('completion: OK')
"

# Test 3: Async Completion
python -c "
import quota_router
import asyncio

async def test():
    r = await quota_router.acompletion(model='gpt-4', messages=[{'role': 'user', 'content': 'test'}])
    assert 'choices' in r

asyncio.run(test())
print('acompletion: OK')
"

# Test 4: Embedding
python -c "
import quota_router
r = quota_router.embedding(input=['test'], model='text-embedding-3-small')
assert 'data' in r
print('embedding: OK')
"

# Test 5: Async Embedding
python -c "
import quota_router
import asyncio

async def test():
    r = await quota_router.aembedding(input=['test'], model='text-embedding-3-small')
    assert 'data' in r

asyncio.run(test())
print('aembedding: OK')
"

# Test 6: Exceptions
python -c "
import quota_router
assert hasattr(quota_router, 'AuthenticationError')
assert hasattr(quota_router, 'RateLimitError')
assert hasattr(quota_router, 'BudgetExceededError')
print('exceptions: OK')
"

# Test 7: LiteLLM Alias
python -c "
import quota_router as litellm
assert litellm.completion is not None
print('LiteLLM alias: OK')
"

echo "All smoke tests passed!"
```

### Type Checking

```bash
# Install type stubs
pip install mypy

# Run mypy
mypy python/quota_router
```

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Python SDK                          │
│  import quota_router as litellm                        │
│  completion() / acompletion() / embedding()            │
└─────────────────────┬───────────────────────────────────┘
                      │ PyO3 (pyo3 0.21)
                      ▼
┌─────────────────────────────────────────────────────────┐
│               quota-router-pyo3 (Rust)                │
│  Exceptions, Types, Completion bindings                │
└─────────────────────┬───────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│               quota-router-core (Rust)                │
│  Balance, Providers, Config, Proxy                    │
└─────────────────────────────────────────────────────────┘
```

## Publishing to PyPI

```bash
# Build wheel
maturin build --manifest-path crates/quota-router-pyo3/Cargo.toml

# Publish
pip publish dist/*
```

## License

MIT OR Apache-2.0
