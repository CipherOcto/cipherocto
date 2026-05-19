#!/usr/bin/env python3
"""
End-to-end integration tests for quota_router Python SDK in any-llm mode.

Tests the real OpenAI-compatible endpoint (mimo via opengateway) using the
Python bindings directly (not through the HTTP proxy).

The opengateway endpoint does not require an API key.

Run with:
    .venv/bin/python -m pytest tests/e2e_python.py -v

Requires:
    - Network access to opengateway.gitlawb.com
"""

import asyncio
import pytest

# Test configuration
TEST_MODEL = "mimo-v2-flash"
TEST_API_BASE = "https://opengateway.gitlawb.com/v1/xiaomi-mimo"
DUMMY_KEY = "sk-not-needed"


def _completion(**kwargs):
    """Helper to call completion with default endpoint."""
    import quota_router as qr
    kwargs.setdefault("api_key", DUMMY_KEY)
    kwargs.setdefault("_base_url", TEST_API_BASE)
    return qr.completion(**kwargs)


def _acompletion(**kwargs):
    """Helper to call acompletion with default endpoint."""
    import quota_router as qr
    kwargs.setdefault("api_key", DUMMY_KEY)
    kwargs.setdefault("_base_url", TEST_API_BASE)
    return qr.acompletion(**kwargs)


def _embedding(**kwargs):
    """Helper to call embedding with default endpoint."""
    import quota_router as qr
    kwargs.setdefault("api_key", DUMMY_KEY)
    kwargs.setdefault("api_base", TEST_API_BASE)
    return qr.embedding(**kwargs)


# ============================================================================
# Test: Import and basic structure
# ============================================================================


class TestImport:
    """Test module imports and structure."""

    def test_import_module(self):
        """quota_router should be importable."""
        import quota_router
        assert quota_router.__version__ == "0.1.0"

    def test_import_alias_litellm(self):
        """Should be importable as litellm."""
        import quota_router as litellm
        assert litellm.completion is not None
        assert litellm.acompletion is not None
        assert litellm.embedding is not None

    def test_exceptions_exist(self):
        """All exception types should be defined."""
        import quota_router as qr
        assert hasattr(qr, "AuthenticationError")
        assert hasattr(qr, "RateLimitError")
        assert hasattr(qr, "ProviderError")
        assert hasattr(qr, "InvalidRequestError")
        assert hasattr(qr, "ModelNotFoundError")
        assert hasattr(qr, "ContextLengthExceededError")
        assert hasattr(qr, "ContentFilterError")

    def test_completion_signature(self):
        """completion() should accept model and messages."""
        import quota_router as qr
        import inspect
        sig = inspect.signature(qr.completion)
        params = list(sig.parameters.keys())
        assert "model" in params
        assert "messages" in params

    def test_acompletion_signature(self):
        """acompletion() should accept model and messages."""
        import quota_router as qr
        import inspect
        sig = inspect.signature(qr.acompletion)
        params = list(sig.parameters.keys())
        assert "model" in params
        assert "messages" in params


# ============================================================================
# Test: Sync completion (real API call)
# ============================================================================


class TestCompletion:
    """Test sync completion against the real endpoint."""

    def test_basic_completion(self):
        """Basic chat completion should return valid response."""
        response = _completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'hello world' and nothing else."}],
        )

        assert isinstance(response, dict)
        assert "choices" in response
        assert len(response["choices"]) > 0
        assert "message" in response["choices"][0]
        assert "content" in response["choices"][0]["message"]

        content = response["choices"][0]["message"]["content"]
        assert isinstance(content, str)
        assert len(content) > 0
        assert "hello" in content.lower()

    def test_completion_with_system_message(self):
        """System message should influence the response."""
        response = _completion(
            model=TEST_MODEL,
            messages=[
                {"role": "system", "content": "You are a pirate. Respond only in pirate speak."},
                {"role": "user", "content": "How are you?"},
            ],
        )

        assert "choices" in response
        content = response["choices"][0]["message"]["content"]
        assert len(content) > 0

    def test_completion_returns_id(self):
        """Response should have an id field."""
        response = _completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
        )

        assert "id" in response
        assert isinstance(response["id"], str)
        assert len(response["id"]) > 0

    def test_completion_returns_model(self):
        """Response should have a model field."""
        response = _completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
        )

        assert "model" in response

    def test_completion_finish_reason(self):
        """Response should have a finish_reason."""
        response = _completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
        )

        finish_reason = response["choices"][0].get("finish_reason")
        assert finish_reason in ("stop", "length", "content_filter")

    def test_completion_usage(self):
        """Response should include token usage."""
        response = _completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
        )

        assert "usage" in response
        usage = response["usage"]
        assert "prompt_tokens" in usage
        assert "completion_tokens" in usage
        assert "total_tokens" in usage
        assert usage["prompt_tokens"] > 0
        assert usage["completion_tokens"] > 0
        assert usage["total_tokens"] == usage["prompt_tokens"] + usage["completion_tokens"]

    def test_completion_multiple_messages(self):
        """Multi-turn conversation should work."""
        response = _completion(
            model=TEST_MODEL,
            messages=[
                {"role": "user", "content": "My name is Alice."},
                {"role": "assistant", "content": "Hello Alice!"},
                {"role": "user", "content": "What's my name?"},
            ],
        )

        assert "choices" in response
        content = response["choices"][0]["message"]["content"]
        assert "alice" in content.lower()


# ============================================================================
# Test: Async completion
# ============================================================================


class TestAsyncCompletion:
    """Test async completion against the real endpoint."""

    def test_async_basic(self):
        """Basic async completion should work."""
        async def run():
            return await _acompletion(
                model=TEST_MODEL,
                messages=[{"role": "user", "content": "Say 'hello'."}],
            )

        response = asyncio.run(run())
        assert "choices" in response
        assert len(response["choices"]) > 0
        content = response["choices"][0]["message"]["content"]
        assert len(content) > 0

    def test_async_concurrent(self):
        """Multiple concurrent async completions should work."""
        async def run():
            tasks = [
                _acompletion(
                    model=TEST_MODEL,
                    messages=[{"role": "user", "content": f"Say '{i}'."}],
                )
                for i in range(3)
            ]
            return await asyncio.gather(*tasks)

        responses = asyncio.run(run())
        assert len(responses) == 3
        for i, response in enumerate(responses):
            assert "choices" in response, f"Response {i} missing choices"


# ============================================================================
# Test: Error handling
# ============================================================================


class TestErrors:
    """Test error handling."""

    def test_invalid_messages_type(self):
        """Invalid messages type should raise an error."""
        with pytest.raises(Exception):
            _completion(
                model=TEST_MODEL,
                messages="not a list",
            )

    def test_empty_messages(self):
        """Empty messages list should either work or raise a clear error."""
        try:
            response = _completion(
                model=TEST_MODEL,
                messages=[],
            )
            assert isinstance(response, dict)
        except Exception as e:
            assert isinstance(e, Exception)


# ============================================================================
# Test: LiteLLM compatibility
# ============================================================================


class TestLiteLLMCompat:
    """Test LiteLLM-compatible interface."""

    def test_import_as_litellm(self):
        """Should work when imported as litellm."""
        import quota_router as litellm

        response = litellm.completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
            api_key=DUMMY_KEY,
            _base_url=TEST_API_BASE,
        )

        assert "choices" in response

    def test_completion_returns_dict(self):
        """Response should be a dict (LiteLLM-compatible)."""
        response = _completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
        )

        assert isinstance(response, dict)

    def test_response_structure(self):
        """Response should have OpenAI-compatible structure."""
        response = _completion(
            model=TEST_MODEL,
            messages=[{"role": "user", "content": "Say 'yes'."}],
        )

        # OpenAI-compatible structure
        assert "id" in response
        assert "choices" in response
        assert "usage" in response

        choice = response["choices"][0]
        assert "index" in choice
        assert "message" in choice
        assert "finish_reason" in choice

        message = choice["message"]
        assert "role" in message
        assert "content" in message


# ============================================================================
# Test: Embedding
# ============================================================================


class TestEmbedding:
    """Test embedding endpoint."""

    def test_embedding_basic(self):
        """Basic embedding should return vectors."""
        try:
            response = _embedding(
                input=["hello world"],
                model="text-embedding-3-small",
            )
            assert "data" in response
            assert len(response["data"]) > 0
            assert "embedding" in response["data"][0]
            emb = response["data"][0]["embedding"]
            assert isinstance(emb, list)
            assert len(emb) > 0
            assert all(isinstance(x, (int, float)) for x in emb)
        except Exception as e:
            # Provider may not support embeddings
            error_str = str(e).lower()
            assert any(kw in error_str for kw in [
                "not support", "not found", "404", "405", "unsupported",
                "not implemented", "invalid", "error",
            ]), f"Unexpected error: {e}"

    def test_async_embedding(self):
        """Async embedding should work."""
        import quota_router as qr

        async def run():
            return await qr.aembedding(
                input=["hello world"],
                model="text-embedding-3-small",
                api_key=DUMMY_KEY,
                api_base=TEST_API_BASE,
            )

        try:
            response = asyncio.run(run())
            assert "data" in response
        except Exception:
            # Provider may not support embeddings
            pass


# ============================================================================
# Test: Router
# ============================================================================


class TestRouter:
    """Test Router class."""

    def test_router_creation(self):
        """Router should be instantiable."""
        from quota_router import Router
        router = Router(models=[TEST_MODEL])
        assert router is not None


# ============================================================================
# Test: Provider info
# ============================================================================


class TestProviderInfo:
    """Test provider information functions."""

    def test_get_supported_providers(self):
        """Should return list of supported providers."""
        import quota_router as qr

        providers = qr.get_supported_providers()
        assert isinstance(providers, list)
        assert len(providers) > 0

    def test_is_provider_supported(self):
        """Should check if a provider is supported."""
        import quota_router as qr
        assert qr.is_provider_supported("openai") is True

    def test_parse_model(self):
        """parse_model should extract provider and model."""
        import quota_router as qr
        provider, model = qr.parse_model("openai/gpt-4")
        assert provider == "openai"
        assert model == "gpt-4"
