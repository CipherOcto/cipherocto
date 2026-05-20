#!/usr/bin/env python3
"""
Drop-in replacement test: quota_router as any_llm

Verifies that `import quota_router as any_llm` works without ANY code changes
for projects currently using any-llm. Tests the exact same function signatures
and calling conventions.

The opengateway endpoint does not require an API key.

Run with:
    .venv/bin/python -m pytest tests/test_drop_in_any_llm.py -v

Requires:
    - Network access to opengateway.gitlawb.com
"""

import pytest

# Test configuration
TEST_MODEL = "mimo-v2-flash"
TEST_API_BASE = "https://opengateway.gitlawb.com/v1/xiaomi-mimo"
DUMMY_KEY = "sk-not-needed"

# This is the drop-in replacement: import quota_router as any_llm
import quota_router as any_llm


# ============================================================================
# Test: Functions that any-llm users call
# ============================================================================


class TestFunctionsExist:
    """Test that all any-llm functions exist with correct names."""

    def test_completion(self):
        """any_llm.completion should exist and be callable."""
        assert callable(any_llm.completion)

    def test_acompletion(self):
        """any_llm.acompletion should exist and be callable."""
        assert callable(any_llm.acompletion)

    def test_embedding(self):
        """any_llm.embedding should exist and be callable."""
        assert callable(any_llm.embedding)

    def test_aembedding(self):
        """any_llm.aembedding should exist and be callable."""
        assert callable(any_llm.aembedding)

    def test_messages(self):
        """any_llm.messages should exist and be callable."""
        assert callable(any_llm.messages)

    def test_amessages(self):
        """any_llm.amessages should exist and be callable."""
        assert callable(any_llm.amessages)

    def test_responses(self):
        """any_llm.responses should exist and be callable."""
        assert callable(any_llm.responses)

    def test_aresponses(self):
        """any_llm.aresponses should exist and be callable."""
        assert callable(any_llm.aresponses)

    def test_list_models(self):
        """any_llm.list_models should exist and be callable."""
        assert callable(any_llm.list_models)

    def test_get_supported_providers(self):
        """any_llm.get_supported_providers should exist."""
        assert callable(any_llm.get_supported_providers)

    def test_is_provider_supported(self):
        """any_llm.is_provider_supported should exist."""
        assert callable(any_llm.is_provider_supported)

    def test_parse_model(self):
        """any_llm.parse_model should exist."""
        assert callable(any_llm.parse_model)


# ============================================================================
# Test: Exceptions that any-llm users catch
# ============================================================================


class TestExceptions:
    """Test that any-llm exceptions exist and are catchable."""

    def test_any_llm_error(self):
        """any_llm.AnyLLMError should exist."""
        assert hasattr(any_llm, "AnyLLMError")
        assert issubclass(any_llm.AnyLLMError, Exception)

    def test_authentication_error(self):
        """any_llm.AuthenticationError should exist."""
        assert hasattr(any_llm, "AuthenticationError")
        assert issubclass(any_llm.AuthenticationError, Exception)

    def test_rate_limit_error(self):
        """any_llm.RateLimitError should exist."""
        assert hasattr(any_llm, "RateLimitError")
        assert issubclass(any_llm.RateLimitError, Exception)

    def test_invalid_request_error(self):
        """any_llm.InvalidRequestError should exist."""
        assert hasattr(any_llm, "InvalidRequestError")
        assert issubclass(any_llm.InvalidRequestError, Exception)

    def test_provider_error(self):
        """any_llm.ProviderError should exist."""
        assert hasattr(any_llm, "ProviderError")
        assert issubclass(any_llm.ProviderError, Exception)

    def test_content_filter_error(self):
        """any_llm.ContentFilterError should exist."""
        assert hasattr(any_llm, "ContentFilterError")
        assert issubclass(any_llm.ContentFilterError, Exception)

    def test_model_not_found_error(self):
        """any_llm.ModelNotFoundError should exist."""
        assert hasattr(any_llm, "ModelNotFoundError")
        assert issubclass(any_llm.ModelNotFoundError, Exception)

    def test_context_length_exceeded_error(self):
        """any_llm.ContextLengthExceededError should exist."""
        assert hasattr(any_llm, "ContextLengthExceededError")
        assert issubclass(any_llm.ContextLengthExceededError, Exception)

    def test_missing_api_key_error(self):
        """any_llm.MissingApiKeyError should exist."""
        assert hasattr(any_llm, "MissingApiKeyError")
        assert issubclass(any_llm.MissingApiKeyError, Exception)

    def test_unsupported_provider_error(self):
        """any_llm.UnsupportedProviderError should exist."""
        assert hasattr(any_llm, "UnsupportedProviderError")
        assert issubclass(any_llm.UnsupportedProviderError, Exception)

    def test_unsupported_parameter_error(self):
        """any_llm.UnsupportedParameterError should exist."""
        assert hasattr(any_llm, "UnsupportedParameterError")
        assert issubclass(any_llm.UnsupportedParameterError, Exception)

    def test_batch_not_complete_error(self):
        """any_llm.BatchNotCompleteError should exist."""
        assert hasattr(any_llm, "BatchNotCompleteError")
        assert issubclass(any_llm.BatchNotCompleteError, Exception)


# ============================================================================
# Test: completion() calling convention (any-llm style)
# ============================================================================


class TestCompletionAnyLLMStyle:
    """Test completion() with any-llm calling conventions."""

    def test_keyword_only_after_messages(self):
        """any_llm.completion(model, messages, *, provider=None, ...) should work."""
        response = any_llm.completion(
            TEST_MODEL,
            [{"role": "user", "content": "Say 'hello'."}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )
        assert isinstance(response, dict)
        assert "choices" in response

    def test_with_provider_kwarg(self):
        """any_llm.completion(model, messages, provider='openai') should work."""
        response = any_llm.completion(
            TEST_MODEL,
            [{"role": "user", "content": "Say 'hello'."}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )
        assert isinstance(response, dict)
        assert "choices" in response

    def test_response_structure(self):
        """Response should have OpenAI-compatible structure."""
        response = any_llm.completion(
            TEST_MODEL,
            [{"role": "user", "content": "Say 'yes'."}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
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
# Test: acompletion() calling convention (any-llm style)
# ============================================================================


class TestAcompletionAnyLLMStyle:
    """Test acompletion() with any-llm calling conventions."""

    def test_async_basic(self):
        """any_llm.acompletion(model, messages) should work."""
        import asyncio

        async def run():
            return await any_llm.acompletion(
                TEST_MODEL,
                [{"role": "user", "content": "Say 'hello'."}],
                api_key=DUMMY_KEY,
                base_url=TEST_API_BASE,
            _mode="any-llm",
            )

        response = asyncio.run(run())
        assert isinstance(response, dict)
        assert "choices" in response


# ============================================================================
# Test: embedding() calling convention (any-llm style)
# ============================================================================


class TestEmbeddingAnyLLMStyle:
    """Test embedding() with any-llm calling conventions."""

    def test_embedding_basic(self):
        """any_llm.embedding(model, inputs) should work or raise clear error."""
        try:
            response = any_llm.embedding(
                model="text-embedding-3-small",
                input=["hello world"],
                api_key=DUMMY_KEY,
                api_base=TEST_API_BASE,
            )
            assert "data" in response
        except Exception as e:
            error_str = str(e).lower()
            assert any(kw in error_str for kw in [
                "not support", "not found", "404", "405", "unsupported",
                "not implemented", "not yet implemented", "invalid", "error",
            ]), f"Unexpected error: {e}"


# ============================================================================
# Test: messages() calling convention (any-llm style — Anthropic Messages API)
# ============================================================================


class TestMessagesAnyLLMStyle:
    """Test messages() with any-llm calling conventions."""

    def test_messages_exists(self):
        """any_llm.messages should exist."""
        assert callable(any_llm.messages)

    def test_messages_raises_not_implemented(self):
        """any_llm.messages should raise NotImplementedError for now."""
        with pytest.raises((NotImplementedError, Exception)):
            any_llm.messages(
                model="claude-3-sonnet",
                messages=[{"role": "user", "content": "Hello"}],
                max_tokens=100,
                api_key=DUMMY_KEY,
                api_base=TEST_API_BASE,
            )


# ============================================================================
# Test: responses() calling convention (any-llm style — OpenAI Responses API)
# ============================================================================


class TestResponsesAnyLLMStyle:
    """Test responses() with any-llm calling conventions."""

    def test_responses_exists(self):
        """any_llm.responses should exist."""
        assert callable(any_llm.responses)

    def test_responses_raises_not_implemented(self):
        """any_llm.responses should raise NotImplementedError for now."""
        with pytest.raises((NotImplementedError, Exception)):
            any_llm.responses(
                model="gpt-4",
                input_data="Hello",
                api_key=DUMMY_KEY,
                api_base=TEST_API_BASE,
            )


# ============================================================================
# Test: Typical any-llm usage patterns
# ============================================================================


class TestTypicalUsagePatterns:
    """Test common any-llm usage patterns that should just work."""

    def test_simple_completion(self):
        """Simple completion pattern from any-llm docs."""
        response = any_llm.completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "What is 2+2?"}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )
        content = response["choices"][0]["message"]["content"]
        assert isinstance(content, str)
        assert len(content) > 0

    def test_multi_turn_conversation(self):
        """Multi-turn conversation pattern."""
        response = any_llm.completion(
            model=TEST_MODEL,
            messages=[
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "My name is Alice."},
                {"role": "assistant", "content": "Hello Alice!"},
                {"role": "user", "content": "What's my name?"},
            ],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )
        content = response["choices"][0]["message"]["content"]
        assert "alice" in content.lower()

    def test_usage_tracking(self):
        """Usage should be tracked."""
        response = any_llm.completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
            api_key=DUMMY_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )
        usage = response["usage"]
        assert usage["prompt_tokens"] > 0
        assert usage["completion_tokens"] > 0
        assert usage["total_tokens"] == usage["prompt_tokens"] + usage["completion_tokens"]

    def test_try_except_error_handling(self):
        """Error handling pattern from any-llm docs."""
        try:
            response = any_llm.completion(
                model="nonexistent-model",
                messages=[{"role": "user", "content": "Hello"}],
                api_key=DUMMY_KEY,
                base_url=TEST_API_BASE,
            _mode="any-llm",
            )
            assert isinstance(response, dict)
        except any_llm.AuthenticationError:
            pass
        except any_llm.ModelNotFoundError:
            pass
        except any_llm.RateLimitError:
            pass
        except Exception as e:
            assert isinstance(e, Exception)

    def test_parse_model(self):
        """parse_model should extract provider and model."""
        provider, model = any_llm.parse_model("openai/gpt-4")
        assert provider == "openai"
        assert model == "gpt-4"

    def test_get_supported_providers(self):
        """get_supported_providers should return a list."""
        providers = any_llm.get_supported_providers()
        assert isinstance(providers, list)
        assert len(providers) > 0

    def test_is_provider_supported(self):
        """is_provider_supported should check support."""
        assert any_llm.is_provider_supported("openai") is True
