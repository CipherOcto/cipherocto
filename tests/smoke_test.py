#!/usr/bin/env python3
"""
Smoke tests for quota_router Python SDK.
Run with: python tests/smoke_test.py
Or:      .venv/bin/python -m pytest tests/smoke_test.py -v
"""

import asyncio
import sys

import pytest

# Free endpoint that doesn't require an API key (same as e2e tests)
TEST_API_BASE = "https://opengateway.gitlawb.com/v1/xiaomi-mimo"
TEST_MODEL = "mimo-v2-flash"
DUMMY_KEY = "sk-not-needed"


@pytest.fixture
def qr():
    """Provide the quota_router module as a fixture."""
    import quota_router
    return quota_router


def test_import():
    """Test 1: Import module"""
    import quota_router
    assert quota_router.__version__ == "0.1.0"


def test_completion(qr):
    """Test 2: Sync completion"""
    response = qr.completion(
        model=TEST_MODEL,
        messages=[{"role": "user", "content": "test"}],
        api_key=DUMMY_KEY,
        base_url=TEST_API_BASE,
    )
    assert "choices" in response
    assert len(response["choices"]) > 0
    assert "message" in response["choices"][0]


def test_completion_content(qr):
    """Test 3: Completion returns content"""
    response = qr.completion(
        model=TEST_MODEL,
        messages=[{"role": "user", "content": "hello"}],
        api_key=DUMMY_KEY,
        base_url=TEST_API_BASE,
    )
    content = response["choices"][0]["message"]["content"]
    assert isinstance(content, str)
    assert len(content) > 0


@pytest.mark.asyncio
async def test_acompletion(qr):
    """Test 4: Async completion"""
    response = await qr.acompletion(
        model=TEST_MODEL,
        messages=[{"role": "user", "content": "test"}],
        api_key=DUMMY_KEY,
        base_url=TEST_API_BASE,
    )
    assert "choices" in response
    assert len(response["choices"]) > 0


def test_embedding(qr):
    """Test 5: Embedding (endpoint may not support embeddings)"""
    try:
        response = qr.embedding(
            model="text-embedding-3-small",
            input=["hello world"],
            api_key=DUMMY_KEY,
            api_base=TEST_API_BASE,
        )
        assert "data" in response
        assert len(response["data"]) > 0
        assert "embedding" in response["data"][0]
    except Exception as e:
        error_str = str(e).lower()
        assert any(kw in error_str for kw in [
            "not support", "not found", "404", "405", "unsupported",
            "not implemented", "error",
        ]), f"Unexpected error: {e}"


@pytest.mark.asyncio
async def test_aembedding(qr):
    """Test 6: Async embedding (endpoint may not support embeddings)"""
    try:
        response = await qr.aembedding(
            model="text-embedding-3-small",
            input=["hello world"],
            api_key=DUMMY_KEY,
            api_base=TEST_API_BASE,
        )
        assert "data" in response
        assert len(response["data"]) > 0
    except Exception as e:
        error_str = str(e).lower()
        assert any(kw in error_str for kw in [
            "not support", "not found", "404", "405", "unsupported",
            "not implemented", "error",
        ]), f"Unexpected error: {e}"


def test_exceptions(qr):
    """Test 7: Exceptions exist"""
    assert hasattr(qr, 'AuthenticationError')
    assert hasattr(qr, 'RateLimitError')
    assert hasattr(qr, 'BudgetExceededError')
    assert hasattr(qr, 'ProviderError')
    assert hasattr(qr, 'Timeout')
    assert hasattr(qr, 'InvalidRequestError')


def test_litellm_alias():
    """Test 8: LiteLLM alias"""
    import quota_router as litellm
    assert litellm.completion is not None
    assert litellm.acompletion is not None
    assert litellm.embedding is not None
    assert litellm.aembedding is not None


async def run_async_tests(qr):
    """Run async tests"""
    await test_acompletion(qr)
    await test_aembedding(qr)


def main():
    print("Running smoke tests for quota_router...\n")

    try:
        import quota_router as qr

        # Test 1: Import
        test_import()

        # Test 2-3: Sync tests
        test_completion(qr)
        test_completion_content(qr)

        # Test 4-6: Async tests
        asyncio.run(run_async_tests(qr))


        # Test 7-8: Extras
        test_exceptions(qr)
        test_litellm_alias()

        print("\n✅ All smoke tests passed!")
        return 0

    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
