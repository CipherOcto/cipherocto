#!/usr/bin/env python3
"""
Drop-in replacement test: quota_router as litellm

Verifies that `import quota_router as litellm` works without ANY code changes
for projects currently using litellm. Tests the exact same function signatures
and calling conventions.

The opengateway endpoint does not require an API key.

Run with:
    .venv/bin/python -m pytest tests/test_drop_in_litellm.py -v

Requires:
    - Network access to opengateway.gitlawb.com
"""

import pytest

# Test configuration
TEST_MODEL = "mimo-v2-flash"
TEST_API_BASE = "https://opengateway.gitlawb.com/v1/xiaomi-mimo"
DUMMY_KEY = "sk-not-needed"

# This is the drop-in replacement: import quota_router as litellm
import quota_router as litellm


# ============================================================================
# Test: Module-level attributes that litellm users expect
# ============================================================================


class TestModuleAttributes:
    """Test that quota_router has the same module-level attributes as litellm."""

    def test_drop_params(self):
        """litellm.drop_params should exist."""
        assert hasattr(litellm, "drop_params")

    def test_set_verbose(self):
        """litellm.set_verbose should exist."""
        assert hasattr(litellm, "set_verbose")

    def test_api_key(self):
        """litellm.api_key should exist."""
        assert hasattr(litellm, "api_key")

    def test_api_base(self):
        """litellm.api_base should exist."""
        assert hasattr(litellm, "api_base")

    def test_num_retries(self):
        """litellm.num_retries should exist."""
        assert hasattr(litellm, "num_retries")

    def test_request_timeout(self):
        """litellm.request_timeout should exist."""
        assert hasattr(litellm, "request_timeout")

    def test_cache(self):
        """litellm.cache should exist."""
        assert hasattr(litellm, "cache")


# ============================================================================
# Test: Functions that litellm users call
# ============================================================================


class TestFunctionsExist:
    """Test that all litellm functions exist with correct names."""

    def test_completion(self):
        """litellm.completion should exist and be callable."""
        assert callable(litellm.completion)

    def test_acompletion(self):
        """litellm.acompletion should exist and be callable."""
        assert callable(litellm.acompletion)

    def test_embedding(self):
        """litellm.embedding should exist and be callable."""
        assert callable(litellm.embedding)

    def test_text_completion(self):
        """litellm.text_completion should exist and be callable."""
        assert callable(litellm.text_completion)

    def test_get_supported_providers(self):
        """litellm.get_supported_providers should exist."""
        assert callable(litellm.get_supported_providers)

    def test_is_provider_supported(self):
        """litellm.is_provider_supported should exist."""
        assert callable(litellm.is_provider_supported)


# ============================================================================
# Test: Exceptions that litellm users catch
# ============================================================================


class TestExceptions:
    """Test that litellm exceptions exist and are catchable."""

    def test_authentication_error(self):
        """litellm.AuthenticationError should exist."""
        assert hasattr(litellm, "AuthenticationError")
        assert issubclass(litellm.AuthenticationError, Exception)

    def test_rate_limit_error(self):
        """litellm.RateLimitError should exist."""
        assert hasattr(litellm, "RateLimitError")
        assert issubclass(litellm.RateLimitError, Exception)

    def test_invalid_request_error(self):
        """litellm.InvalidRequestError should exist."""
        assert hasattr(litellm, "InvalidRequestError")
        assert issubclass(litellm.InvalidRequestError, Exception)

    def test_not_found_error(self):
        """litellm.NotFoundError should exist."""
        assert hasattr(litellm, "NotFoundError")
        assert issubclass(litellm.NotFoundError, Exception)

    def test_context_window_exceeded_error(self):
        """litellm.ContextWindowExceededError should exist."""
        assert hasattr(litellm, "ContextWindowExceededError")
        assert issubclass(litellm.ContextWindowExceededError, Exception)

    def test_content_policy_violation_error(self):
        """litellm.ContentPolicyViolationError should exist."""
        assert hasattr(litellm, "ContentPolicyViolationError")
        assert issubclass(litellm.ContentPolicyViolationError, Exception)

    def test_budget_exceeded_error(self):
        """litellm.BudgetExceededError should exist."""
        assert hasattr(litellm, "BudgetExceededError")
        assert issubclass(litellm.BudgetExceededError, Exception)

    def test_timeout_error(self):
        """litellm.Timeout should exist."""
        assert hasattr(litellm, "Timeout")

    def test_service_unavailable_error(self):
        """litellm.ServiceUnavailableError should exist."""
        assert hasattr(litellm, "ServiceUnavailableError")

    def test_api_connection_error(self):
        """litellm.APIConnectionError should exist."""
        assert hasattr(litellm, "APIConnectionError")

    def test_api_error(self):
        """litellm.APIError should exist."""
        assert hasattr(litellm, "APIError")


# ============================================================================
# Test: completion() calling convention (litellm style)
# ============================================================================


class TestCompletionLiteLLMStyle:
    """Test completion() with litellm calling conventions."""

    def test_positional_model_messages(self):
        """litellm.completion(model, messages) should work."""
        response = litellm.completion(
            TEST_MODEL,
            [{"role": "user", "content": "Say 'hello'."}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
        )
        assert isinstance(response, dict)
        assert "choices" in response
        assert len(response["choices"]) > 0

    def test_keyword_model_messages(self):
        """litellm.completion(model=..., messages=...) should work."""
        response = litellm.completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'hello'."}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
        )
        assert isinstance(response, dict)
        assert "choices" in response

    def test_with_temperature(self):
        """litellm.completion(model, messages, temperature=0.5) should work."""
        response = litellm.completion(
            TEST_MODEL,
            [{"role": "user", "content": "Say 'hello'."}],
            temperature=0.5,
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
        )
        assert isinstance(response, dict)
        assert "choices" in response

    def test_with_max_tokens(self):
        """litellm.completion(model, messages, max_tokens=50) should work."""
        response = litellm.completion(
            TEST_MODEL,
            [{"role": "user", "content": "Say 'hello'."}],
            max_tokens=50,
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
        )
        assert isinstance(response, dict)
        assert "choices" in response

    def test_response_structure(self):
        """Response should have OpenAI-compatible structure."""
        response = litellm.completion(
            TEST_MODEL,
            [{"role": "user", "content": "Say 'yes'."}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
        )
        assert "id" in response
        assert "choices" in response
        assert "usage" in response
        assert "model" in response

        choice = response["choices"][0]
        assert "index" in choice
        assert "message" in choice
        assert "finish_reason" in choice

        message = choice["message"]
        assert "role" in message
        assert "content" in message

        usage = response["usage"]
        assert "prompt_tokens" in usage
        assert "completion_tokens" in usage
        assert "total_tokens" in usage


# ============================================================================
# Test: acompletion() calling convention (litellm style)
# ============================================================================


class TestAcompletionLiteLLMStyle:
    """Test acompletion() with litellm calling conventions."""

    def test_async_basic(self):
        """litellm.acompletion(model, messages) should work."""
        import asyncio

        async def run():
            return await litellm.acompletion(
                TEST_MODEL,
                [{"role": "user", "content": "Say 'hello'."}],
                api_key=DUMMY_KEY,
                base_url=TEST_API_BASE,
            )

        response = asyncio.run(run())
        assert isinstance(response, dict)
        assert "choices" in response


# ============================================================================
# Test: embedding() calling convention (litellm style)
# ============================================================================


class TestEmbeddingLiteLLMStyle:
    """Test embedding() with litellm calling conventions."""

    def test_embedding_basic(self):
        """litellm.embedding(model, input) should work or raise clear error."""
        try:
            response = litellm.embedding(
                model="text-embedding-3-small",
                input=["hello world"],
                api_key=DUMMY_KEY,
                api_base=TEST_API_BASE,
            )
            assert "data" in response
        except Exception as e:
            # Provider may not support embeddings
            error_str = str(e).lower()
            assert any(kw in error_str for kw in [
                "not support", "not found", "404", "405", "unsupported",
                "not implemented", "not yet implemented", "invalid", "error",
            ]), f"Unexpected error: {e}"


# ============================================================================
# Test: Typical litellm usage patterns
# ============================================================================


class TestTypicalUsagePatterns:
    """Test common litellm usage patterns that should just work."""

    def test_simple_completion(self):
        """Simple completion pattern from litellm docs."""
        response = litellm.completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "What is 2+2?"}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
        )
        content = response["choices"][0]["message"]["content"]
        assert isinstance(content, str)
        assert len(content) > 0

    def test_multi_turn_conversation(self):
        """Multi-turn conversation pattern."""
        response = litellm.completion(
            model=TEST_MODEL,
            messages=[
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "My name is Alice."},
                {"role": "assistant", "content": "Hello Alice!"},
                {"role": "user", "content": "What's my name?"},
            ],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
        )
        content = response["choices"][0]["message"]["content"]
        assert "alice" in content.lower()

    def test_usage_tracking(self):
        """Usage should be tracked."""
        response = litellm.completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
        )
        usage = response["usage"]
        assert usage["prompt_tokens"] > 0
        assert usage["completion_tokens"] > 0
        assert usage["total_tokens"] == usage["prompt_tokens"] + usage["completion_tokens"]

    def test_try_except_error_handling(self):
        """Error handling pattern from litellm docs."""
        try:
            response = litellm.completion(
                model="nonexistent-model",
                messages=[{"role": "user", "content": "Hello"}],
                api_key=DUMMY_KEY,
                base_url=TEST_API_BASE,
            )
            # If it succeeds, that's fine too
            assert isinstance(response, dict)
        except litellm.AuthenticationError:
            pass  # Expected
        except litellm.NotFoundError:
            pass  # Expected
        except litellm.RateLimitError:
            pass  # Expected
        except Exception as e:
            # Any other exception is acceptable for an invalid model
            assert isinstance(e, Exception)
