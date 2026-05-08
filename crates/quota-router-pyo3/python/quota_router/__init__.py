"""
quota-router: Drop-in replacement for LiteLLM with quota routing

This package provides Python bindings for quota-router-core,
enabling drop-in replacement for LiteLLM users.

Example:
    >>> import quota_router
    >>> response = quota_router.completion(
    ...     model="gpt-4o",
    ...     messages=[{"role": "user", "content": "Hello!"}]
    ... )
    >>> print(response["choices"][0]["message"]["content"])
"""

# Import all public API from the Rust extension
from ._quota_router import (
    # Version
    __version__,
    # Completion functions
    completion,
    acompletion,
    # Messages functions
    messages,
    amessages,
    # Responses functions
    responses,
    aresponses,
    # Embedding functions
    embedding,
    aembedding,
    # Model listing
    list_models,
    alist_models,
    # Batch operations
    create_batch,
    acreate_batch,
    retrieve_batch,
    aretrieve_batch,
    cancel_batch,
    acancel_batch,
    list_batches,
    alist_batches,
    retrieve_batch_results,
    aretrieve_batch_results,
    # Model parsing
    parse_model,
    parse_model_strict,
    # SDK management
    set_api_key,
    get_budget_status,
    get_metrics,
    # Exceptions
    QuotaRouterError,
    RateLimitError,
    AuthenticationError,
    InvalidRequestError,
    ProviderError,
    ContentFilterError,
    ModelNotFoundError,
    ContextLengthExceededError,
    MissingApiKeyError,
    UnsupportedProviderError,
    UnsupportedParameterError,
    InsufficientFundsError,
    UpstreamProviderError,
    GatewayTimeoutError,
    LengthFinishReasonError,
    ContentFilterFinishReasonError,
    BatchNotCompleteError,
)

# Drop-in alias for any-llm compatibility
AnyLLMError = QuotaRouterError

__all__ = [
    # Version
    "__version__",
    # Completion
    "completion",
    "acompletion",
    # Messages
    "messages",
    "amessages",
    # Responses
    "responses",
    "aresponses",
    # Embeddings
    "embedding",
    "aembedding",
    # Model listing
    "list_models",
    "alist_models",
    # Batch
    "create_batch",
    "acreate_batch",
    "retrieve_batch",
    "aretrieve_batch",
    "cancel_batch",
    "acancel_batch",
    "list_batches",
    "alist_batches",
    "retrieve_batch_results",
    "aretrieve_batch_results",
    # Model parsing
    "parse_model",
    "parse_model_strict",
    # SDK management
    "set_api_key",
    "get_budget_status",
    "get_metrics",
    # Exceptions
    "QuotaRouterError",
    "RateLimitError",
    "AuthenticationError",
    "InvalidRequestError",
    "ProviderError",
    "ContentFilterError",
    "ModelNotFoundError",
    "ContextLengthExceededError",
    "MissingApiKeyError",
    "UnsupportedProviderError",
    "UnsupportedParameterError",
    "InsufficientFundsError",
    "UpstreamProviderError",
    "GatewayTimeoutError",
    "LengthFinishReasonError",
    "ContentFilterFinishReasonError",
    "BatchNotCompleteError",
    "AnyLLMError",  # drop-in alias for any-llm
]
