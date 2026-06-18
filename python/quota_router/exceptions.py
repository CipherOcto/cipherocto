# quota_router.exceptions - Exception classes
#
# Maps quota-router exceptions to LiteLLM-compatible names.

try:
    from quota_router_native import (
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
    # Stub classes when native extension not installed
    class QuotaRouterError(Exception): pass
    class RateLimitError(QuotaRouterError): pass
    class AuthenticationError(QuotaRouterError): pass
    class InvalidRequestError(QuotaRouterError): pass
    class ProviderError(QuotaRouterError): pass
    class ContentFilterError(QuotaRouterError): pass
    class ModelNotFoundError(QuotaRouterError): pass
    class ContextLengthExceededError(QuotaRouterError): pass
    class MissingApiKeyError(QuotaRouterError): pass
    class UnsupportedProviderError(QuotaRouterError): pass
    class UnsupportedParameterError(QuotaRouterError): pass
    class InsufficientFundsError(QuotaRouterError): pass
    class UpstreamProviderError(QuotaRouterError): pass
    class GatewayTimeoutError(QuotaRouterError): pass
    class LengthFinishReasonError(QuotaRouterError): pass
    class ContentFilterFinishReasonError(QuotaRouterError): pass
    class BatchNotCompleteError(QuotaRouterError): pass
    class AllModelsFailedError(QuotaRouterError): pass
    class BatchPartialFailureError(QuotaRouterError): pass

# LiteLLM-compatible aliases
BudgetExceededError = InsufficientFundsError
ServiceUnavailableError = UpstreamProviderError
APIConnectionError = GatewayTimeoutError
APIError = QuotaRouterError
NotFoundError = ModelNotFoundError
ContextWindowExceededError = ContextLengthExceededError
ContentPolicyViolationError = ContentFilterError

__all__ = [
    # quota-router exception names
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
    "AllModelsFailedError",
    "BatchPartialFailureError",
    # LiteLLM-compatible aliases
    "BudgetExceededError",
    "ServiceUnavailableError",
    "APIConnectionError",
    "APIError",
    "NotFoundError",
    "ContextWindowExceededError",
    "ContentPolicyViolationError",
]
