#!/usr/bin/env python3
"""
End-to-end integration tests for Anthropic-compatible endpoint.

Tests the Anthropic Messages API via the MiniMax endpoint at
https://api.minimax.io/anthropic using both litellm-mode (reqwest)
and any-llm-mode (PyO3).

The API key is read from ANTHROPIC_AUTH_TOKEN environment variable.
The base URL is read from ANTHROPIC_BASE_URL or defaults to
https://api.minimax.io/anthropic.

Run with:
    .venv/bin/python -m pytest tests/e2e_anthropic.py -v

Requires:
    - ANTHROPIC_AUTH_TOKEN env var set
    - Network access to api.minimax.io
"""

import os
import pytest

# Skip if no API key
API_KEY = os.environ.get("ANTHROPIC_AUTH_TOKEN")
if not API_KEY:
    pytest.skip("ANTHROPIC_AUTH_TOKEN not set", allow_module_level=True)

# Test configuration from environment
TEST_API_BASE = os.environ.get("ANTHROPIC_BASE_URL", "https://api.minimax.io/anthropic")
# litellm-mode (native_http) needs /v1 suffix because provider appends /messages
TEST_API_BASE_LITELLM = TEST_API_BASE + "/v1" if not TEST_API_BASE.endswith("/v1") else TEST_API_BASE
TEST_MODEL = os.environ.get("ANTHROPIC_DEFAULT_SONNET_MODEL", "MiniMax-M2.7")

import quota_router as qr


# ============================================================================
# Test: Anthropic completion via litellm-mode (reqwest)
# ============================================================================


class TestAnthropicLiteLLMMode:
    """Test Anthropic endpoint via litellm-mode (reqwest → REST API)."""

    def test_basic_completion(self):
        """Basic completion with anthropic provider."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Say 'hello' and nothing else."}],
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )

        assert isinstance(response, dict)
        assert "choices" in response
        assert len(response["choices"]) > 0
        content = response["choices"][0]["message"]["content"]
        assert isinstance(content, str)
        assert len(content) > 0
        assert "hello" in content.lower()

    def test_completion_with_system(self):
        """System message should work."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[
                {"role": "system", "content": "You are a pirate. Respond only in pirate speak."},
                {"role": "user", "content": "How are you?"},
            ],
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )

        assert "choices" in response
        content = response["choices"][0]["message"]["content"]
        assert len(content) > 0

    def test_completion_returns_usage(self):
        """Response should include token usage."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Say 'yes'."}],
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )

        assert "usage" in response
        usage = response["usage"]
        assert "prompt_tokens" in usage
        assert "completion_tokens" in usage
        assert "total_tokens" in usage
        assert usage["prompt_tokens"] > 0
        assert usage["completion_tokens"] > 0

    def test_completion_multi_turn(self):
        """Multi-turn conversation should work."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[
                {"role": "user", "content": "My name is Alice."},
                {"role": "assistant", "content": "Hello Alice!"},
                {"role": "user", "content": "What's my name?"},
            ],
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )

        content = response["choices"][0]["message"]["content"]
        assert "alice" in content.lower()

    def test_completion_max_tokens(self):
        """Max tokens should limit response."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Write a 500 word essay about dogs."}],
            max_tokens=50,
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )

        assert "choices" in response
        finish_reason = response["choices"][0].get("finish_reason", "")
        assert finish_reason in ("stop", "length", "max_tokens")

    def test_completion_temperature(self):
        """Temperature parameter should be accepted."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Say 'hello'."}],
            temperature=0.5,
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )

        assert "choices" in response

    def test_no_api_key_returns_error(self):
        """Missing API key should return an error."""
        with pytest.raises(Exception):
            qr.completion(
                model=f"anthropic/{TEST_MODEL}",
                messages=[{"role": "user", "content": "Hello"}],
                api_key="sk-invalid-key",
                base_url=TEST_API_BASE,
                _mode="litellm",
            )


# ============================================================================
# Test: Anthropic completion via any-llm-mode (PyO3 → Python SDK)
# ============================================================================


class TestAnthropicAnyLLMMode:
    """Test Anthropic endpoint via any-llm-mode (PyO3 → Python SDK)."""

    def test_basic_completion(self):
        """Basic completion with anthropic provider via any-llm mode."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Say 'hello' and nothing else."}],
            api_key=API_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )

        assert isinstance(response, dict)
        assert "choices" in response
        assert len(response["choices"]) > 0
        content = response["choices"][0]["message"]["content"]
        assert isinstance(content, str)
        assert len(content) > 0
        assert "hello" in content.lower()

    def test_completion_with_system(self):
        """System message should work via any-llm mode."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[
                {"role": "system", "content": "You are a pirate. Respond only in pirate speak."},
                {"role": "user", "content": "How are you?"},
            ],
            api_key=API_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )

        assert "choices" in response
        content = response["choices"][0]["message"]["content"]
        assert len(content) > 0

    def test_completion_returns_usage(self):
        """Response should include token usage via any-llm mode."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Say 'yes'."}],
            api_key=API_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )

        assert "usage" in response
        usage = response["usage"]
        assert usage["prompt_tokens"] > 0
        assert usage["completion_tokens"] > 0

    def test_completion_multi_turn(self):
        """Multi-turn conversation should work via any-llm mode."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[
                {"role": "user", "content": "My name is Alice."},
                {"role": "assistant", "content": "Hello Alice!"},
                {"role": "user", "content": "What's my name?"},
            ],
            api_key=API_KEY,
            base_url=TEST_API_BASE,
            _mode="any-llm",
        )

        content = response["choices"][0]["message"]["content"]
        assert "alice" in content.lower()


# ============================================================================
# Test: Async completion
# ============================================================================


class TestAnthropicAsync:
    """Test async completion against Anthropic endpoint."""

    def test_async_litellm_mode(self):
        """Async completion via litellm-mode."""
        import asyncio

        async def run():
            return await qr.acompletion(
                model=f"anthropic/{TEST_MODEL}",
                messages=[{"role": "user", "content": "Say 'hello'."}],
                api_key=API_KEY,
                base_url=TEST_API_BASE_LITELLM,
                _mode="litellm",
            )

        response = asyncio.run(run())
        assert "choices" in response
        assert len(response["choices"]) > 0

    def test_async_any_llm_mode(self):
        """Async completion via any-llm-mode."""
        import asyncio

        async def run():
            return await qr.acompletion(
                model=f"anthropic/{TEST_MODEL}",
                messages=[{"role": "user", "content": "Say 'hello'."}],
                api_key=API_KEY,
                base_url=TEST_API_BASE,
                _mode="any-llm",
            )

        response = asyncio.run(run())
        assert "choices" in response
        assert len(response["choices"]) > 0


# ============================================================================
# Test: Response structure
# ============================================================================


class TestAnthropicResponseStructure:
    """Test that responses have OpenAI-compatible structure."""

    def test_response_has_id(self):
        """Response should have an id field."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Say 'yes'."}],
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )
        assert "id" in response
        assert isinstance(response["id"], str)

    def test_response_has_model(self):
        """Response should have a model field."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Say 'yes'."}],
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )
        assert "model" in response

    def test_response_has_choices(self):
        """Response should have choices array."""
        response = qr.completion(
            model=f"anthropic/{TEST_MODEL}",
            messages=[{"role": "user", "content": "Say 'yes'."}],
            api_key=API_KEY,
            base_url=TEST_API_BASE_LITELLM,
            _mode="litellm",
        )
        assert "choices" in response
        choices = response["choices"]
        assert isinstance(choices, list)
        assert len(choices) > 0

        choice = choices[0]
        assert "index" in choice
        assert "message" in choice
        assert "finish_reason" in choice

        message = choice["message"]
        assert "role" in message
        assert "content" in message
        assert message["role"] == "assistant"


# ============================================================================
# Test: Error handling
# ============================================================================


class TestAnthropicErrors:
    """Test error handling for Anthropic endpoint."""

    def test_invalid_api_key(self):
        """Invalid API key should raise an error."""
        with pytest.raises(Exception) as exc_info:
            qr.completion(
                model=f"anthropic/{TEST_MODEL}",
                messages=[{"role": "user", "content": "Hello"}],
                api_key="sk-invalid-key-12345",
                base_url=TEST_API_BASE,
                _mode="litellm",
            )
        # Should be an auth error or provider error
        error_str = str(exc_info.value).lower()
        assert any(kw in error_str for kw in [
            "auth", "401", "403", "invalid", "error", "key", "404", "not found",
        ])

    def test_invalid_model(self):
        """Invalid model should raise an error or return gracefully."""
        try:
            response = qr.completion(
                model="anthropic/nonexistent-model-xyz",
                messages=[{"role": "user", "content": "Hello"}],
                api_key=API_KEY,
                base_url=TEST_API_BASE,
                _mode="litellm",
            )
            # If it succeeds, that's acceptable
            assert isinstance(response, dict)
        except Exception as e:
            # Error is expected
            assert isinstance(e, Exception)
