"""llama.cpp-based LLM provider for high-performance local inference.

Uses llama-cpp-python bindings for direct access to llama.cpp inference engine.
Significantly faster than Ollama for local deployments.

Performance characteristics:
- 2-3x faster than Ollama (C++ vs Python overhead)
- 30-50% lower memory usage
- Better GPU utilization
- Supports GGUF model format

Installation:
    # CPU only:
    pip install llama-cpp-python

    # With CUDA (NVIDIA GPU):
    CMAKE_ARGS="-DLLAMA_CUBLAS=on" pip install llama-cpp-python

    # With Metal (Apple Silicon):
    CMAKE_ARGS="-DLLAMA_METAL=on" pip install llama-cpp-python
"""

import asyncio
from typing import AsyncIterator, Dict, Any, Optional
from pathlib import Path

from .base import LLMProvider, TokenChunk
from jeeves_avionics.logging import get_current_logger
from jeeves_protocols import LoggerProtocol


class LlamaCppProvider(LLMProvider):
    """llama.cpp-based LLM provider matching LLMProvider interface.

    Thread-safe via async lock (llama.cpp is NOT thread-safe).
    Provides high-performance local inference using llama.cpp C++ backend.

    Example:
        provider = LlamaCppProvider(
            model_path="./models/llama-3.1-8b-q4_0.gguf",
            n_ctx=4096,
            n_gpu_layers=35  # GPU offload
        )

        response = await provider.generate(
            model="",  # Ignored (uses model_path from init)
            prompt="Hello!",
            options={"temperature": 0.7}
        )
    """

    def __init__(
        self,
        model_path: str,
        n_ctx: int = 4096,
        n_gpu_layers: int = 0,
        n_threads: Optional[int] = None,
        verbose: bool = False,
        logger: Optional[LoggerProtocol] = None,
    ):
        """Initialize llama.cpp provider.

        Args:
            model_path: Path to GGUF model file
            n_ctx: Context window size (default 4096)
            n_gpu_layers: Number of layers to offload to GPU (0=CPU only)
            n_threads: CPU threads to use (None=auto)
            verbose: Enable verbose logging from llama.cpp
            logger: Logger for DI (uses context logger if not provided)

        Raises:
            ImportError: If llama-cpp-python not installed
            ValueError: If model_path doesn't exist
        """
        self._logger = logger or get_current_logger()

        try:
            from llama_cpp import Llama
        except ImportError:
            raise ImportError(
                "llama-cpp-python not installed. "
                "Install: pip install llama-cpp-python"
            )

        # Validate model file exists
        if not Path(model_path).exists():
            raise ValueError(
                f"Model file not found: {model_path}\n"
                f"Download GGUF models from HuggingFace or convert with llama.cpp"
            )

        self.model_path = model_path
        self.n_ctx = n_ctx
        self.n_gpu_layers = n_gpu_layers

        # Thread safety lock (llama.cpp is NOT thread-safe)
        self._lock = asyncio.Lock()

        # Auto-detect optimal thread count if not specified
        if n_threads is None:
            import multiprocessing
            n_threads = max(1, multiprocessing.cpu_count() - 1)

        self._logger.info(
            "initializing_llamacpp_provider",
            model_path=model_path,
            n_ctx=n_ctx,
            n_gpu_layers=n_gpu_layers,
            n_threads=n_threads
        )

        # Initialize llama.cpp model with GPU fallback
        try:
            self.llm = Llama(
                model_path=model_path,
                n_ctx=n_ctx,
                n_gpu_layers=n_gpu_layers,
                n_threads=n_threads,
                verbose=verbose,
                # Performance optimizations
                use_mmap=True,  # Memory-map model file
                use_mlock=False,  # Don't lock model in RAM (allows swap)
                n_batch=512,  # Batch size for prompt processing
            )
        except RuntimeError as e:
            if "cuda" in str(e).lower():
                # Fallback to CPU if GPU fails
                self._logger.warning(
                    "llamacpp_gpu_fallback",
                    error=str(e),
                    action="falling_back_to_cpu"
                )
                self.llm = Llama(
                    model_path=model_path,
                    n_ctx=n_ctx,
                    n_gpu_layers=0,  # CPU only
                    n_threads=n_threads,
                    verbose=verbose
                )
            else:
                raise

        self._logger.info(
            "llamacpp_provider_initialized",
            model_path=model_path,
            context_size=n_ctx,
            gpu_layers=n_gpu_layers
        )

    async def generate(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> str:
        """Generate text completion using llama.cpp.

        IMPORTANT: This method matches the LLMProvider interface exactly.
        The 'model' parameter is ignored (uses model_path from __init__).

        Args:
            model: Model name (ignored, uses model_path from init)
            prompt: Input prompt text
            options: Generation options (temperature, num_predict, stop)

        Returns:
            Generated text string
        """
        async with self._lock:  # Thread safety
            # Extract options
            opts = options or {}
            temperature = opts.get("temperature", 0.7)
            max_tokens = opts.get("num_predict", 512)
            stop = opts.get("stop", [])

            self._logger.debug(
                "llamacpp_generating",
                prompt_length=len(prompt),
                temperature=temperature,
                max_tokens=max_tokens
            )

            # Run inference (blocking call, wrapped in thread)
            # llama-cpp-python's __call__ is synchronous
            result = await asyncio.to_thread(
                self.llm,
                prompt,
                temperature=temperature,
                max_tokens=max_tokens,
                stop=stop,
                echo=False  # Don't echo prompt in output
            )

            # Extract response text
            text = result["choices"][0]["text"]
            prompt_tokens = result["usage"]["prompt_tokens"]
            completion_tokens = result["usage"]["completion_tokens"]
            stop_reason = result["choices"][0].get("finish_reason")

            self._logger.info(
                "llamacpp_generated",
                prompt_tokens=prompt_tokens,
                completion_tokens=completion_tokens,
                total_tokens=prompt_tokens + completion_tokens,
                stop_reason=stop_reason
            )

            return text

    async def generate_stream(
        self,
        model: str,
        prompt: str,
        options: Optional[Dict[str, Any]] = None,
    ) -> AsyncIterator[TokenChunk]:
        """Generate text with streaming using llama.cpp.

        Uses llama-cpp-python's native streaming support.

        Args:
            model: Model name (ignored, uses model_path from init)
            prompt: Input prompt text
            options: Generation options (temperature, num_predict, stop)

        Yields:
            TokenChunk objects with incremental text
        """
        async with self._lock:  # Thread safety
            opts = options or {}
            temperature = opts.get("temperature", 0.7)
            max_tokens = opts.get("num_predict", 512)
            stop = opts.get("stop", [])

            self._logger.debug(
                "llamacpp_streaming",
                prompt_length=len(prompt),
                temperature=temperature,
                max_tokens=max_tokens,
            )

            # llama-cpp-python streaming returns an iterator
            # We need to wrap it in async to maintain our interface
            def stream_sync():
                return self.llm(
                    prompt,
                    temperature=temperature,
                    max_tokens=max_tokens,
                    stop=stop,
                    echo=False,
                    stream=True,  # Enable streaming
                )

            # Get the sync iterator in a thread
            stream_iter = await asyncio.to_thread(stream_sync)

            # Process streaming chunks
            async def process_chunks():
                for chunk in stream_iter:
                    text = chunk["choices"][0].get("text", "")
                    finish_reason = chunk["choices"][0].get("finish_reason")
                    is_final = finish_reason is not None

                    if text or is_final:
                        yield TokenChunk(
                            text=text,
                            is_final=is_final,
                            token_count=1 if text else 0,
                            metadata={
                                "finish_reason": finish_reason,
                            } if finish_reason else None,
                        )

            # Yield chunks from the async generator
            async for chunk in process_chunks():
                yield chunk

    @property
    def supports_streaming(self) -> bool:
        """llama.cpp supports native streaming."""
        return True

    async def health_check(self) -> bool:
        """Check if llama.cpp provider is healthy.

        Returns:
            True if model is loaded and ready, False otherwise
        """
        try:
            # Simple health check: verify model is loaded
            return self.llm is not None
        except Exception as e:
            self._logger.error("llamacpp_health_check_failed", error=str(e))
            return False

    def get_stats(self) -> Dict[str, Any]:
        """Get model statistics.

        Returns:
            Dictionary with model info and performance stats
        """
        return {
            "model_path": self.model_path,
            "context_size": self.n_ctx,
            "gpu_layers": self.n_gpu_layers,
            "backend": "llama.cpp"
        }

    def __repr__(self) -> str:
        """String representation."""
        return (
            f"LlamaCppProvider(model={self.model_path}, "
            f"ctx={self.n_ctx}, gpu_layers={self.n_gpu_layers})"
        )
