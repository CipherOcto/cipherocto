# Type stubs for quota_router
# Provides IDE support and type checking

from typing import Any, Dict, List, Optional, Union

__version__: str

# Type definitions
Message = Dict[str, str]
ModelResponse = Dict[str, Any]
EmbeddingResponse = Dict[str, Any]

# Completion functions
def completion(
    model: str,
    messages: List[Message],
    *,
    temperature: Optional[float] = None,
    max_tokens: Optional[int] = None,
    top_p: Optional[float] = None,
    n: Optional[int] = None,
    stream: Optional[bool] = False,
    stop: Optional[Union[str, List[str]]] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
    user: Optional[str] = None,
    api_key: Optional[str] = None,
    **kwargs
) -> ModelResponse: ...

async def acompletion(
    model: str,
    messages: List[Message],
    *,
    temperature: Optional[float] = None,
    max_tokens: Optional[int] = None,
    top_p: Optional[float] = None,
    n: Optional[int] = None,
    stream: Optional[bool] = False,
    stop: Optional[Union[str, List[str]]] = None,
    presence_penalty: Optional[float] = None,
    frequency_penalty: Optional[float] = None,
    user: Optional[str] = None,
    api_key: Optional[str] = None,
    **kwargs
) -> ModelResponse: ...

# Embedding functions
def embedding(
    input: Union[str, List[str]],
    model: str,
    *,
    api_key: Optional[str] = None,
    **kwargs
) -> EmbeddingResponse: ...

async def aembedding(
    input: Union[str, List[str]],
    model: str,
    *,
    api_key: Optional[str] = None,
    **kwargs
) -> EmbeddingResponse: ...

# Exception classes
class AuthenticationError(Exception):
    def __init__(self, message: str, llm_provider: Optional[str] = None): ...

class RateLimitError(Exception):
    def __init__(self, message: str, llm_provider: Optional[str] = None): ...

class BudgetExceededError(Exception):
    def __init__(self, message: str, budget: float): ...

class ProviderError(Exception):
    def __init__(self, message: str, llm_provider: str): ...

class TimeoutError(Exception):
    def __init__(self, message: str): ...

class InvalidRequestError(Exception):
    def __init__(self, message: str, llm_provider: Optional[str] = None): ...
