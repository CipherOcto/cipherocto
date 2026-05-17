# quota_router.router - Router class wrapper
#
# Provides a LiteLLM-compatible Router class.

try:
    from quota_router_native import Router as NativeRouter
except ImportError:
    NativeRouter = None


class Router:
    """
    Multi-provider router for load balancing and fallback.

    Drop-in replacement for litellm.Router.

    Example:
        from quota_router import Router

        router = Router(
            model_list=[
                {
                    "model_name": "gpt-4",
                    "litellm_params": {
                        "model": "openai/gpt-4",
                        "api_key": "sk-...",
                    },
                },
                {
                    "model_name": "gpt-4",
                    "litellm_params": {
                        "model": "azure/gpt-4",
                        "api_key": "...",
                        "api_base": "https://...",
                    },
                },
            ],
            routing_strategy="least-busy",
        )

        response = await router.acompletion(
            model="gpt-4",
            messages=[{"role": "user", "content": "Hello!"}],
        )
    """

    def __init__(
        self,
        model_list=None,
        *,
        routing_strategy="least-busy",
        fallbacks=None,
        context_window_fallbacks=None,
        content_policy_fallbacks=None,
        cache=False,
        cache_params=None,
        set_verbose=False,
        num_retries=3,
        timeout=30,
        max_parallel_requests=None,
        **kwargs,
    ):
        if NativeRouter is None:
            raise ImportError(
                "quota_router_native not installed. "
                "Install with: pip install quota-router"
            )

        self._router = NativeRouter(
            model_list=model_list or [],
            routing_strategy=routing_strategy,
            fallbacks=fallbacks,
            num_retries=num_retries,
            timeout=timeout,
        )

    async def acompletion(self, model, messages, **kwargs):
        """Async chat completion with routing."""
        return await self._router.acompletion(model, messages, **kwargs)

    def completion(self, model, messages, **kwargs):
        """Sync chat completion with routing."""
        return self._router.completion(model, messages, **kwargs)

    async def aembedding(self, model, input, **kwargs):
        """Async embedding with routing."""
        return await self._router.aembedding(model, input, **kwargs)

    def embedding(self, model, input, **kwargs):
        """Sync embedding with routing."""
        return self._router.embedding(model, input, **kwargs)

    def get_available_deployment(self, model, messages=None):
        """Get a healthy deployment for the given model."""
        return self._router.get_available_deployment(model, messages)


__all__ = ["Router"]
