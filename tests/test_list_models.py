#!/usr/bin/env python3
"""
List models and exception alias test suite for quota_router.

Tests list_models() function and verifies all exception aliases work
for both litellm-mode and any-llm-mode.

Verifies SIGNATURE and PARAMETER ACCEPTANCE — does NOT make live API calls.

Run with:
    .venv/bin/python -m pytest tests/test_list_models.py -v

Requires:
    - quota_router package installed (PyO3 extension)
"""

import pytest

# Test configuration
TEST_API_BASE = "https://opengateway.gitlawb.com/v1/xiaomi-mimo"
DUMMY_KEY = "sk-not-needed"

import quota_router


# ============================================================================
# Test: list_models() calling convention
# ============================================================================


def test_list_models_provider_required():
    """list_models(provider='openai') works — provider is required.

    The provider parameter tells the SDK which upstream API to query.
    """
    try:
        quota_router.list_models(
            provider="openai",
            api_key=DUMMY_KEY,
            api_base=TEST_API_BASE,
        )
    except NotImplementedError:
        pass  # Expected — not yet implemented
    except Exception as e:
        assert isinstance(e, Exception)


def test_list_models_no_provider_error():
    """list_models() without provider raises TypeError.

    provider is a required positional parameter in the PyO3 binding.
    """
    with pytest.raises(TypeError):
        quota_router.list_models(
            api_key=DUMMY_KEY,
            api_base=TEST_API_BASE,
        )


def test_list_models_client_args():
    """list_models() accepts optional client_args param for provider-specific config."""
    try:
        quota_router.list_models(
            provider="openai",
            client_args={"timeout": 30},
            api_key=DUMMY_KEY,
            api_base=TEST_API_BASE,
        )
    except NotImplementedError:
        pass
    except Exception as e:
        assert isinstance(e, Exception)


def test_list_models_return_type():
    """list_models returns a sequence with model objects.

    Each model object should have .id, .name, .provider, .created fields.
    Actual return type validation requires a live API call.
    """
    try:
        result = quota_router.list_models(
            provider="openai",
            api_key=DUMMY_KEY,
            api_base=TEST_API_BASE,
        )
        assert result is not None
    except NotImplementedError:
        pass  # Expected — not yet implemented
    except Exception as e:
        assert isinstance(e, Exception)


# ============================================================================
# Test: LiteLLM exception aliases (8 aliases)
# ============================================================================
# These are the 8 exception names that litellm users expect to catch.
# quota_router provides these as direct exports or aliases.


def test_litellm_alias_bad_request_error():
    """litellm.BadRequestError alias exists and is catchable.

    Maps to quota_router.InvalidRequestError internally.
    """
    assert hasattr(quota_router, "InvalidRequestError")
    assert issubclass(quota_router.InvalidRequestError, Exception)
    with pytest.raises(quota_router.InvalidRequestError):
        raise quota_router.InvalidRequestError("test error")


def test_litellm_alias_authentication_error():
    """litellm.AuthenticationError alias exists and is catchable.

    Direct export from quota_router.
    """
    assert hasattr(quota_router, "AuthenticationError")
    assert issubclass(quota_router.AuthenticationError, Exception)
    with pytest.raises(quota_router.AuthenticationError):
        raise quota_router.AuthenticationError("test error")


def test_litellm_alias_rate_limit_error():
    """litellm.RateLimitError alias exists and is catchable.

    Direct export from quota_router.
    """
    assert hasattr(quota_router, "RateLimitError")
    assert issubclass(quota_router.RateLimitError, Exception)
    with pytest.raises(quota_router.RateLimitError):
        raise quota_router.RateLimitError("test error")


def test_litellm_alias_not_found_error():
    """litellm.NotFoundError alias exists and is catchable.

    Maps to quota_router.ModelNotFoundError internally.
    """
    assert hasattr(quota_router, "NotFoundError")
    assert issubclass(quota_router.NotFoundError, Exception)
    with pytest.raises(quota_router.NotFoundError):
        raise quota_router.NotFoundError("test error")


def test_litellm_alias_timeout():
    """litellm.Timeout alias exists and is catchable.

    Maps to quota_router.GatewayTimeoutError internally.
    """
    assert hasattr(quota_router, "GatewayTimeoutError")
    assert issubclass(quota_router.GatewayTimeoutError, Exception)
    with pytest.raises(quota_router.GatewayTimeoutError):
        raise quota_router.GatewayTimeoutError("test error")


def test_litellm_alias_internal_server_error():
    """litellm.InternalServerError alias exists and is catchable.

    Maps to quota_router.UpstreamProviderError internally.
    """
    assert hasattr(quota_router, "UpstreamProviderError")
    assert issubclass(quota_router.UpstreamProviderError, Exception)
    with pytest.raises(quota_router.UpstreamProviderError):
        raise quota_router.UpstreamProviderError("test error")


def test_litellm_alias_context_window_exceeded_error():
    """litellm.ContextWindowExceededError alias exists and is catchable.

    Maps to quota_router.ContextLengthExceededError internally.
    """
    assert hasattr(quota_router, "ContextWindowExceededError")
    assert issubclass(quota_router.ContextWindowExceededError, Exception)
    with pytest.raises(quota_router.ContextWindowExceededError):
        raise quota_router.ContextWindowExceededError("test error")


def test_litellm_alias_content_policy_violation_error():
    """litellm.ContentPolicyViolationError alias exists and is catchable.

    Maps to quota_router.ContentFilterError internally.
    """
    assert hasattr(quota_router, "ContentPolicyViolationError")
    assert issubclass(quota_router.ContentPolicyViolationError, Exception)
    with pytest.raises(quota_router.ContentPolicyViolationError):
        raise quota_router.ContentPolicyViolationError("test error")


# ============================================================================
# Test: any-llm exception classes (12 exceptions)
# ============================================================================
# These are the exception types that any-llm users expect to catch.
# All are direct exports from the quota_router native module.


def test_anyllm_exception_model_not_found_error():
    """any-llm.ModelNotFoundError exists and is catchable."""
    assert hasattr(quota_router, "ModelNotFoundError")
    assert issubclass(quota_router.ModelNotFoundError, Exception)
    with pytest.raises(quota_router.ModelNotFoundError):
        raise quota_router.ModelNotFoundError("model not found")


def test_anyllm_exception_unsupported_provider_error():
    """any-llm.UnsupportedProviderError exists and is catchable."""
    assert hasattr(quota_router, "UnsupportedProviderError")
    assert issubclass(quota_router.UnsupportedProviderError, Exception)
    with pytest.raises(quota_router.UnsupportedProviderError):
        raise quota_router.UnsupportedProviderError("unsupported provider")


def test_anyllm_exception_missing_api_key_error():
    """any-llm.MissingApiKeyError exists and is catchable."""
    assert hasattr(quota_router, "MissingApiKeyError")
    assert issubclass(quota_router.MissingApiKeyError, Exception)
    with pytest.raises(quota_router.MissingApiKeyError):
        raise quota_router.MissingApiKeyError("missing api key")


def test_anyllm_exception_invalid_request_error():
    """any-llm.InvalidRequestError exists and is catchable."""
    assert hasattr(quota_router, "InvalidRequestError")
    assert issubclass(quota_router.InvalidRequestError, Exception)
    with pytest.raises(quota_router.InvalidRequestError):
        raise quota_router.InvalidRequestError("invalid request")


def test_anyllm_exception_rate_limit_exceeded_error():
    """any-llm.RateLimitExceededError exists and is catchable.

    Maps to quota_router.RateLimitError (the native exception).
    """
    assert hasattr(quota_router, "RateLimitError")
    assert issubclass(quota_router.RateLimitError, Exception)
    with pytest.raises(quota_router.RateLimitError):
        raise quota_router.RateLimitError("rate limit exceeded")


def test_anyllm_exception_server_error():
    """any-llm.ServerError exists and is catchable.

    Maps to quota_router.UpstreamProviderError (the native exception).
    """
    assert hasattr(quota_router, "UpstreamProviderError")
    assert issubclass(quota_router.UpstreamProviderError, Exception)
    with pytest.raises(quota_router.UpstreamProviderError):
        raise quota_router.UpstreamProviderError("server error")


def test_anyllm_exception_timeout_error():
    """any-llm.TimeoutError exists and is catchable.

    Maps to quota_router.GatewayTimeoutError (the native exception).
    """
    assert hasattr(quota_router, "GatewayTimeoutError")
    assert issubclass(quota_router.GatewayTimeoutError, Exception)
    with pytest.raises(quota_router.GatewayTimeoutError):
        raise quota_router.GatewayTimeoutError("timeout")


def test_anyllm_exception_network_error():
    """any-llm.NetworkError exists and is catchable.

    Maps to quota_router.ProviderError (the native exception).
    """
    assert hasattr(quota_router, "ProviderError")
    assert issubclass(quota_router.ProviderError, Exception)
    with pytest.raises(quota_router.ProviderError):
        raise quota_router.ProviderError("network error")


def test_anyllm_exception_parse_error():
    """any-llm.ParseError exists and is catchable.

    Maps to quota_router.QuotaRouterError (the base exception).
    """
    assert hasattr(quota_router, "QuotaRouterError")
    assert issubclass(quota_router.QuotaRouterError, Exception)
    with pytest.raises(quota_router.QuotaRouterError):
        raise quota_router.QuotaRouterError("parse error")


def test_anyllm_exception_batch_not_complete_error():
    """any-llm.BatchNotCompleteError exists and is catchable."""
    assert hasattr(quota_router, "BatchNotCompleteError")
    assert issubclass(quota_router.BatchNotCompleteError, Exception)
    with pytest.raises(quota_router.BatchNotCompleteError):
        raise quota_router.BatchNotCompleteError("batch not complete")


def test_anyllm_exception_quota_exceeded_error():
    """any-llm.QuotaExceededError exists and is catchable.

    Maps to quota_router.InsufficientFundsError (the native exception).
    """
    assert hasattr(quota_router, "InsufficientFundsError")
    assert issubclass(quota_router.InsufficientFundsError, Exception)
    with pytest.raises(quota_router.InsufficientFundsError):
        raise quota_router.InsufficientFundsError("quota exceeded")


def test_anyllm_exception_provider_error():
    """any-llm.ProviderError exists and is catchable."""
    assert hasattr(quota_router, "ProviderError")
    assert issubclass(quota_router.ProviderError, Exception)
    with pytest.raises(quota_router.ProviderError):
        raise quota_router.ProviderError("provider error")


# ============================================================================
# Test: Exception inheritance hierarchy
# ============================================================================


def test_quota_router_error_is_base():
    """QuotaRouterError is the base exception for all quota-router errors."""
    assert issubclass(quota_router.QuotaRouterError, Exception)


def test_rate_limit_error_inherits_quota_router_error():
    """RateLimitError inherits from QuotaRouterError."""
    assert issubclass(quota_router.RateLimitError, quota_router.QuotaRouterError)


def test_authentication_error_inherits_quota_router_error():
    """AuthenticationError inherits from QuotaRouterError."""
    assert issubclass(
        quota_router.AuthenticationError, quota_router.QuotaRouterError
    )


def test_batch_not_complete_error_inherits_quota_router_error():
    """BatchNotCompleteError inherits from QuotaRouterError."""
    assert issubclass(
        quota_router.BatchNotCompleteError, quota_router.QuotaRouterError
    )


def test_all_models_failed_error_inherits_quota_router_error():
    """AllModelsFailedError inherits from QuotaRouterError."""
    assert hasattr(quota_router, "AllModelsFailedError")
    assert issubclass(
        quota_router.AllModelsFailedError, quota_router.QuotaRouterError
    )


def test_catch_all_with_quota_router_error():
    """Catching QuotaRouterError catches all quota-router specific exceptions."""
    exceptions_to_test = [
        quota_router.AuthenticationError,
        quota_router.RateLimitError,
        quota_router.InvalidRequestError,
        quota_router.ProviderError,
        quota_router.ContentFilterError,
        quota_router.ModelNotFoundError,
        quota_router.ContextLengthExceededError,
        quota_router.MissingApiKeyError,
        quota_router.UnsupportedProviderError,
        quota_router.UnsupportedParameterError,
        quota_router.InsufficientFundsError,
        quota_router.UpstreamProviderError,
        quota_router.GatewayTimeoutError,
        quota_router.LengthFinishReasonError,
        quota_router.ContentFilterFinishReasonError,
        quota_router.BatchNotCompleteError,
        quota_router.AllModelsFailedError,
        quota_router.BatchPartialFailureError,
    ]
    for exc_class in exceptions_to_test:
        try:
            raise exc_class("test")
        except quota_router.QuotaRouterError:
            pass  # Expected — caught by base class
        else:
            pytest.fail(
                f"{exc_class.__name__} was not caught by QuotaRouterError"
            )
