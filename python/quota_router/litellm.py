# quota_router.litellm - LiteLLM compatibility alias
#
# Usage:
#   import quota_router.litellm as litellm
#   response = litellm.completion(model="gpt-4", messages=[...])
#
# Or:
#   from quota_router.litellm import completion, Router

from quota_router import *

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
    # Exceptions (LiteLLM names)
    "AuthenticationError",
    "RateLimitError",
    "BudgetExceededError",
    "InvalidRequestError",
    "ContextWindowExceededError",
    "ContentPolicyViolationError",
    "TimeoutError",
    "ProviderError",
    "ServiceUnavailableError",
    "APIConnectionError",
    "APIError",
    "NotFoundError",
    # Global settings
    "drop_params",
    "set_verbose",
    "api_key",
    "api_base",
    "num_retries",
    "request_timeout",
    "cache",
]
