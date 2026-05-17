# quota_router - Python SDK for quota-router
#
# Drop-in replacement for LiteLLM and any-llm
#
# Example:
#   import quota_router as litellm
#   response = litellm.completion(model="gpt-4", messages=[...])
#
# Or:
#   import quota_router as any_llm
#   response = any_llm.completion(model="openai/gpt-4", messages=[...])

__version__ = "0.1.0"

# Import from native extension (installed by maturin)
try:
    from quota_router_native import (
        # Core completion functions
        completion,
        acompletion,
        text_completion,
        atext_completion,
        # Embedding functions
        embedding,
        aembedding,
        # Anthropic Messages API
        messages,
        amessages,
        # OpenAI Responses API
        responses,
        aresponses,
        # Model functions
        list_models,
        alist_models,
        parse_model,
        parse_model_strict,
        # Batch functions
        create_batch,
        retrieve_batch,
        cancel_batch,
        list_batches,
        retrieve_batch_results,
        batch_completion,
        # SDK management
        set_api_key,
        get_budget_status,
        get_metrics,
        # Provider functions
        get_supported_providers,
        is_provider_supported,
        get_provider_info,
        # Router class
        Router,
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
        AllModelsFailedError,
        BatchPartialFailureError,
    )
except ImportError:
    # Native extension not installed — stub functions for development
    pass

# Exception aliases for LiteLLM compatibility
# LiteLLM uses different names for some exceptions
from quota_router.exceptions import (
    BudgetExceededError,
    ServiceUnavailableError,
    APIConnectionError,
    APIError,
    NotFoundError,
    ContextWindowExceededError,
    ContentPolicyViolationError,
)

# Global settings (LiteLLM compatibility)
drop_params = False
set_verbose = False
api_key = None
api_base = None
num_retries = 3
request_timeout = 30
cache = False

__all__ = [
    # Core functions
    "completion",
    "acompletion",
    "text_completion",
    "atext_completion",
    "embedding",
    "aembedding",
    "messages",
    "amessages",
    "responses",
    "aresponses",
    # Model functions
    "list_models",
    "alist_models",
    "parse_model",
    "parse_model_strict",
    # Batch functions
    "create_batch",
    "retrieve_batch",
    "cancel_batch",
    "list_batches",
    "retrieve_batch_results",
    "batch_completion",
    # SDK management
    "set_api_key",
    "get_budget_status",
    "get_metrics",
    # Provider functions
    "get_supported_providers",
    "is_provider_supported",
    "get_provider_info",
    # Router
    "Router",
    # Exceptions (quota-router names)
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
    # Exceptions (LiteLLM compatible aliases)
    "BudgetExceededError",
    "ServiceUnavailableError",
    "APIConnectionError",
    "APIError",
    "NotFoundError",
    "ContextWindowExceededError",
    "ContentPolicyViolationError",
    # Global settings
    "drop_params",
    "set_verbose",
    "api_key",
    "api_base",
    "num_retries",
    "request_timeout",
    "cache",
]
