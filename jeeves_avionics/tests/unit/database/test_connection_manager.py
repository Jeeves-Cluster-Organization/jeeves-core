"""Unit tests for connection manager and state backends."""

import pytest
import asyncio
from jeeves_avionics.database.connection_manager import (
    InMemoryStateBackend,
    ConnectionManager
)
from jeeves_avionics.settings import Settings


class TestInMemoryStateBackend:
    """Test in-memory state backend for single-node mode."""

    @pytest.mark.asyncio
    async def test_rate_limit_allows_within_limit(self):
        """Should allow requests within rate limit."""
        backend = InMemoryStateBackend()

        # First 5 requests should be allowed (limit=5)
        for i in range(5):
            allowed, remaining = await backend.rate_limit_check(
                user_id="user_123",
                limit=5,
                window_seconds=60
            )
            assert allowed is True
            assert remaining == (4 - i)

    @pytest.mark.asyncio
    async def test_rate_limit_blocks_over_limit(self):
        """Should block requests over rate limit."""
        backend = InMemoryStateBackend()

        # Fill up the limit
        for _ in range(5):
            await backend.rate_limit_check("user_123", limit=5, window_seconds=60)

        # Next request should be blocked
        allowed, remaining = await backend.rate_limit_check(
            "user_123",
            limit=5,
            window_seconds=60
        )
        assert allowed is False
        assert remaining == 0

    @pytest.mark.asyncio
    async def test_rate_limit_sliding_window(self):
        """Rate limit should use sliding window."""
        backend = InMemoryStateBackend()

        # Make 5 requests
        for _ in range(5):
            await backend.rate_limit_check("user_123", limit=5, window_seconds=1)

        # Wait for window to expire
        await asyncio.sleep(1.1)

        # Should allow requests again
        allowed, remaining = await backend.rate_limit_check(
            "user_123",
            limit=5,
            window_seconds=1
        )
        assert allowed is True

    @pytest.mark.asyncio
    async def test_lock_acquisition(self):
        """Should acquire lock successfully."""
        backend = InMemoryStateBackend()

        acquired = await backend.acquire_lock("job:123", ttl_seconds=10)
        assert acquired is True

    @pytest.mark.asyncio
    async def test_lock_prevents_double_acquisition(self):
        """Should prevent acquiring same lock twice."""
        backend = InMemoryStateBackend()

        # First acquisition succeeds
        acquired1 = await backend.acquire_lock("job:123", ttl_seconds=10)
        assert acquired1 is True

        # Second acquisition fails
        acquired2 = await backend.acquire_lock("job:123", ttl_seconds=10)
        assert acquired2 is False

    @pytest.mark.asyncio
    async def test_lock_release(self):
        """Should release lock and allow re-acquisition."""
        backend = InMemoryStateBackend()

        # Acquire lock
        await backend.acquire_lock("job:123", ttl_seconds=10)

        # Release lock
        released = await backend.release_lock("job:123")
        assert released is True

        # Can acquire again
        acquired = await backend.acquire_lock("job:123", ttl_seconds=10)
        assert acquired is True

    @pytest.mark.asyncio
    async def test_lock_auto_expires(self):
        """Locks should auto-expire after TTL."""
        backend = InMemoryStateBackend()

        # Acquire lock with 1 second TTL
        await backend.acquire_lock("job:123", ttl_seconds=1)

        # Wait for expiry
        await asyncio.sleep(1.1)

        # Should be able to acquire again
        acquired = await backend.acquire_lock("job:123", ttl_seconds=1)
        assert acquired is True

    @pytest.mark.asyncio
    async def test_embedding_cache_hit(self):
        """Should cache and retrieve embeddings."""
        backend = InMemoryStateBackend()

        vector = [0.1, 0.2, 0.3]
        text_hash = "abc123"

        # Cache embedding
        await backend.cache_embedding(text_hash, vector, ttl=10)

        # Retrieve cached embedding
        cached = await backend.get_cached_embedding(text_hash)
        assert cached == vector

    @pytest.mark.asyncio
    async def test_embedding_cache_miss(self):
        """Should return None for cache miss."""
        backend = InMemoryStateBackend()

        cached = await backend.get_cached_embedding("nonexistent")
        assert cached is None

    @pytest.mark.asyncio
    async def test_embedding_cache_expiry(self):
        """Cached embeddings should expire after TTL."""
        backend = InMemoryStateBackend()

        vector = [0.1, 0.2, 0.3]
        text_hash = "abc123"

        # Cache with 1 second TTL
        await backend.cache_embedding(text_hash, vector, ttl=1)

        # Wait for expiry
        await asyncio.sleep(1.1)

        # Should be expired
        cached = await backend.get_cached_embedding(text_hash)
        assert cached is None


class TestConnectionManager:
    """Test connection manager deployment mode selection."""

    @pytest.mark.asyncio
    async def test_single_node_uses_in_memory_backend(self):
        """Single-node mode should use in-memory backend."""
        settings = Settings(deployment_mode="single_node")
        manager = ConnectionManager(settings)

        backend = await manager.get_state_backend()
        assert isinstance(backend, InMemoryStateBackend)

    @pytest.mark.asyncio
    async def test_manager_caches_backend_instance(self):
        """Manager should reuse same backend instance."""
        settings = Settings(deployment_mode="single_node")
        manager = ConnectionManager(settings)

        backend1 = await manager.get_state_backend()
        backend2 = await manager.get_state_backend()

        assert backend1 is backend2
