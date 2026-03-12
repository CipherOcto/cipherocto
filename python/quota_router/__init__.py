# quota_router - Python SDK for quota-router
#
# Drop-in replacement for LiteLLM
#
# Example:
#   import quota_router as litellm
#   response = litellm.completion(model="gpt-4", messages=[...])

# The native implementation is in the Rust extension
# This package provides a thin wrapper for pip installability

__version__ = "0.1.0"

__all__ = [
    "completion",
    "acompletion",
    "embedding",
    "aembedding",
    "AuthenticationError",
    "RateLimitError",
    "BudgetExceededError",
    "ProviderError",
    "TimeoutError",
    "InvalidRequestError",
]

# Import from native extension (installed by maturin)
# Use absolute import to avoid circular reference
from quota_router_native import (
    completion,
    acompletion,
    embedding,
    aembedding,
    AuthenticationError,
    RateLimitError,
    BudgetExceededError,
    ProviderError,
    TimeoutError,
    InvalidRequestError,
)
