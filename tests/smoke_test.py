#!/usr/bin/env python3
"""
Smoke tests for quota_router Python SDK.
Run with: python tests/smoke_test.py
"""

import asyncio
import sys


def test_import():
    """Test 1: Import module"""
    import quota_router
    assert quota_router.__version__ == "0.1.0"
    print("✓ test_import: OK")
    return quota_router


def test_completion(qr):
    """Test 2: Sync completion"""
    response = qr.completion(
        model="gpt-4",
        messages=[{"role": "user", "content": "test"}]
    )
    assert "choices" in response
    assert len(response["choices"]) > 0
    assert "message" in response["choices"][0]
    print("✓ test_completion: OK")


def test_completion_content(qr):
    """Test 3: Completion returns content"""
    response = qr.completion(
        model="gpt-4",
        messages=[{"role": "user", "content": "hello"}]
    )
    content = response["choices"][0]["message"]["content"]
    assert isinstance(content, str)
    assert len(content) > 0
    print("✓ test_completion_content: OK")


async def test_acompletion(qr):
    """Test 4: Async completion"""
    response = await qr.acompletion(
        model="gpt-4",
        messages=[{"role": "user", "content": "test"}]
    )
    assert "choices" in response
    assert len(response["choices"]) > 0
    print("✓ test_acompletion: OK")


def test_embedding(qr):
    """Test 5: Embedding"""
    response = qr.embedding(
        input=["hello world"],
        model="text-embedding-3-small"
    )
    assert "data" in response
    assert len(response["data"]) > 0
    assert "embedding" in response["data"][0]
    print("✓ test_embedding: OK")


async def test_aembedding(qr):
    """Test 6: Async embedding"""
    response = await qr.aembedding(
        input=["hello world"],
        model="text-embedding-3-small"
    )
    assert "data" in response
    assert len(response["data"]) > 0
    print("✓ test_aembedding: OK")


def test_exceptions(qr):
    """Test 7: Exceptions exist"""
    assert hasattr(qr, 'AuthenticationError')
    assert hasattr(qr, 'RateLimitError')
    assert hasattr(qr, 'BudgetExceededError')
    assert hasattr(qr, 'ProviderError')
    assert hasattr(qr, 'TimeoutError')
    assert hasattr(qr, 'InvalidRequestError')
    print("✓ test_exceptions: OK")


def test_litellm_alias():
    """Test 8: LiteLLM alias"""
    import quota_router as litellm
    assert litellm.completion is not None
    assert litellm.acompletion is not None
    assert litellm.embedding is not None
    assert litellm.aembedding is not None
    print("✓ test_litellm_alias: OK")


async def run_async_tests(qr):
    """Run async tests"""
    await test_acompletion(qr)
    await test_aembedding(qr)


def main():
    print("Running smoke tests for quota_router...\n")

    try:
        # Test 1: Import
        qr = test_import()

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
