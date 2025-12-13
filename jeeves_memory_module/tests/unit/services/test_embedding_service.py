"""Unit tests for EmbeddingService."""

import pytest
from jeeves_memory_module.services.embedding_service import EmbeddingService


@pytest.mark.heavy
@pytest.mark.requires_ml
@pytest.mark.integration
class TestService:
    """Tests for EmbeddingService functionality."""

    @pytest.fixture
    def service(self):
        """Create EmbeddingService instance."""
        return EmbeddingService(model_name="all-MiniLM-L6-v2", cache_size=100)

    def test_init(self, service):
        """Test service initialization."""
        assert service.model_name == "all-MiniLM-L6-v2"
        assert service.cache_size == 100
        assert service.model is not None
        assert len(service._cache) == 0

    def test_embed_single_text(self, service):
        """Test embedding generation for single text."""
        text = "This is a test sentence"
        embedding = service.embed(text)

        assert isinstance(embedding, list)
        assert len(embedding) == 384  # all-MiniLM-L6-v2 produces 384-dim vectors
        assert all(isinstance(x, float) for x in embedding)

    def test_embed_empty_text(self, service):
        """Test embedding empty text returns zero vector."""
        embedding = service.embed("")
        assert len(embedding) == 384
        assert all(x == 0.0 for x in embedding)

    def test_embed_caching(self, service):
        """Test that embedding caching works."""
        text = "Cache test"

        # First call - cache miss
        embedding1 = service.embed(text)
        assert service._cache_misses == 1
        assert service._cache_hits == 0

        # Second call - cache hit
        embedding2 = service.embed(text)
        assert service._cache_misses == 1
        assert service._cache_hits == 1

        # Embeddings should be identical
        assert embedding1 == embedding2

    def test_embed_batch(self, service):
        """Test batch embedding generation."""
        texts = ["First sentence", "Second sentence", "Third sentence"]
        embeddings = service.embed_batch(texts)

        assert len(embeddings) == 3
        for embedding in embeddings:
            assert len(embedding) == 384
            assert all(isinstance(x, float) for x in embedding)

    def test_embed_batch_empty(self, service):
        """Test batch embedding with empty list."""
        embeddings = service.embed_batch([])
        assert embeddings == []

    def test_embed_batch_with_empty_texts(self, service):
        """Test batch embedding with some empty texts."""
        texts = ["Valid text", "", "Another valid text"]
        embeddings = service.embed_batch(texts)

        assert len(embeddings) == 3
        assert all(len(emb) == 384 for emb in embeddings)
        # Middle embedding should be zero vector
        assert all(x == 0.0 for x in embeddings[1])

    def test_similarity(self, service):
        """Test similarity calculation."""
        text1 = "The cat sits on the mat"
        text2 = "The feline rests on the rug"
        text3 = "Python programming language"

        # Similar texts should have higher similarity
        sim_similar = service.similarity(text1, text2)
        sim_different = service.similarity(text1, text3)

        assert 0.0 <= sim_similar <= 1.0
        assert 0.0 <= sim_different <= 1.0
        assert sim_similar > sim_different

    def test_similarity_identical(self, service):
        """Test similarity of identical texts."""
        text = "Identical text"
        similarity = service.similarity(text, text)
        assert similarity > 0.99  # Should be very close to 1.0

    def test_cache_stats(self, service):
        """Test cache statistics."""
        # Generate some embeddings
        service.embed("test1")
        service.embed("test2")
        service.embed("test1")  # Cache hit

        stats = service.get_cache_stats()
        assert stats['cache_size'] == 2
        assert stats['cache_hits'] == 1
        assert stats['cache_misses'] == 2
        assert stats['hit_rate'] == 1/3

    def test_clear_cache(self, service):
        """Test cache clearing."""
        service.embed("test")
        assert len(service._cache) > 0

        service.clear_cache()
        assert len(service._cache) == 0

    def test_cache_lru_eviction(self):
        """Test LRU cache eviction when full."""
        service = EmbeddingService(cache_size=2)

        # Fill cache
        service.embed("text1")
        service.embed("text2")
        assert service.get_cache_stats()['cache_size'] == 2

        # Add third item - should evict oldest
        service.embed("text3")
        assert service.get_cache_stats()['cache_size'] == 2

    def test_cache_key_generation(self, service):
        """Test cache key generation is consistent."""
        text = "Test text"
        key1 = service._get_cache_key(text)
        key2 = service._get_cache_key(text)

        assert key1 == key2
        assert isinstance(key1, str)
        assert len(key1) == 64  # SHA-256 produces 64 hex characters
