"""Unit tests for EmbeddingService.

Clean, modern tests using mocked SentenceTransformer model.
All external dependencies are mocked - no ML model required.
"""

import pytest
import numpy as np
from unittest.mock import MagicMock, patch, Mock
from memory_module.services.embedding_service import EmbeddingService


@pytest.fixture
def mock_sentence_transformer():
    """Mock SentenceTransformer class."""
    with patch('jeeves_memory_module.services.embedding_service.SentenceTransformer') as mock_st:
        # Create mock model instance
        mock_model = MagicMock()

        # Mock encode to return deterministic numpy arrays
        def mock_encode(text, convert_to_numpy=False):
            if isinstance(text, str):
                # Single text - return 384-dim vector based on hash of text
                hash_val = hash(text) % 1000 / 1000.0
                return np.array([hash_val] * 384, dtype=np.float32)
            else:
                # Batch - return array of vectors
                return np.array([[hash(t) % 1000 / 1000.0] * 384 for t in text], dtype=np.float32)

        mock_model.encode = MagicMock(side_effect=mock_encode)
        mock_st.return_value = mock_model

        yield mock_st


@pytest.fixture
def service(mock_sentence_transformer, mock_logger):
    """Create EmbeddingService instance with mocked dependencies."""
    return EmbeddingService(model_name="all-MiniLM-L6-v2", cache_size=100, logger=mock_logger)


# =============================================================================
# INITIALIZATION TESTS
# =============================================================================


def test_init_success(mock_sentence_transformer, mock_logger):
    """Test successful initialization with mocked model."""
    service = EmbeddingService(model_name="all-MiniLM-L6-v2", cache_size=50, logger=mock_logger)

    # Verify model loaded
    mock_sentence_transformer.assert_called_once_with("all-MiniLM-L6-v2")

    # Verify attributes
    assert service.model_name == "all-MiniLM-L6-v2"
    assert service.cache_size == 50
    assert service.model is not None
    assert len(service._cache) == 0
    assert service._cache_hits == 0
    assert service._cache_misses == 0


def test_init_permission_error(mock_logger):
    """Test PermissionError with actionable message."""
    with patch('jeeves_memory_module.services.embedding_service.SentenceTransformer') as mock_st:
        mock_st.side_effect = PermissionError("Access denied to cache directory")

        with pytest.raises(PermissionError, match="PermissionError when downloading"):
            EmbeddingService(logger=mock_logger)


def test_init_generic_error(mock_logger):
    """Test RuntimeError on model load failure."""
    with patch('jeeves_memory_module.services.embedding_service.SentenceTransformer') as mock_st:
        mock_st.side_effect = ValueError("Invalid model name")

        with pytest.raises(RuntimeError, match="Failed to initialize embedding model"):
            EmbeddingService(model_name="invalid-model", logger=mock_logger)


# =============================================================================
# EMBED TESTS
# =============================================================================


def test_embed_success(service):
    """Test embedding generation for single text."""
    text = "This is a test sentence"
    embedding = service.embed(text)

    # Verify result
    assert isinstance(embedding, list)
    assert len(embedding) == 384
    assert all(isinstance(x, float) for x in embedding)

    # Verify cache updated
    assert service._cache_misses == 1
    assert len(service._cache) == 1


def test_embed_empty_text(service):
    """Test empty text returns zero vector."""
    embedding = service.embed("")

    assert len(embedding) == 384
    assert all(x == 0.0 for x in embedding)

    # Should not cache empty text
    assert len(service._cache) == 0


def test_embed_cache_hit(service):
    """Test cache hit returns cached embedding."""
    text = "Cache test"

    # First call - cache miss
    embedding1 = service.embed(text)
    assert service._cache_misses == 1
    assert service._cache_hits == 0

    # Second call - cache hit
    embedding2 = service.embed(text)
    assert service._cache_misses == 1
    assert service._cache_hits == 1

    # Should be identical
    assert embedding1 == embedding2


def test_embed_cache_eviction(mock_sentence_transformer, mock_logger):
    """Test LRU eviction when cache full."""
    service = EmbeddingService(cache_size=2, logger=mock_logger)

    # Fill cache
    service.embed("text1")
    service.embed("text2")
    assert len(service._cache) == 2

    # Add third - should evict oldest
    service.embed("text3")
    assert len(service._cache) == 2

    # text1 should be evicted (oldest)
    cache_keys = list(service._cache.keys())
    text1_key = service._get_cache_key("text1")
    assert text1_key not in cache_keys


def test_embed_error_handling(service):
    """Test embed handles model encoding errors."""
    service.model.encode = MagicMock(side_effect=RuntimeError("Model encoding failed"))

    with pytest.raises(RuntimeError, match="Model encoding failed"):
        service.embed("test text")


# =============================================================================
# EMBED_BATCH TESTS
# =============================================================================


def test_embed_batch_success(service):
    """Test batch embedding generation."""
    texts = ["First sentence", "Second sentence", "Third sentence"]
    embeddings = service.embed_batch(texts)

    # Verify results
    assert len(embeddings) == 3
    for embedding in embeddings:
        assert len(embedding) == 384
        assert all(isinstance(x, float) for x in embedding)

    # All should be cache misses
    assert service._cache_misses == 3


def test_embed_batch_empty_list(service):
    """Test batch with empty list returns empty list."""
    embeddings = service.embed_batch([])
    assert embeddings == []


def test_embed_batch_all_empty_texts(service):
    """Test batch with all empty texts returns zero vectors."""
    texts = ["", "", ""]
    embeddings = service.embed_batch(texts)

    assert len(embeddings) == 3
    for embedding in embeddings:
        assert all(x == 0.0 for x in embedding)

    # Empty texts not cached
    assert len(service._cache) == 0


def test_embed_batch_mixed_cached_uncached(service):
    """Test batch with mix of cached and new texts."""
    # Pre-cache one text
    service.embed("cached text")
    assert service._cache_misses == 1

    # Batch with cached and new texts
    texts = ["cached text", "new text 1", "new text 2"]
    embeddings = service.embed_batch(texts)

    assert len(embeddings) == 3

    # Should have 1 cache hit and 2 new misses
    assert service._cache_hits == 1
    assert service._cache_misses == 3  # 1 from pre-cache + 2 from batch


def test_embed_batch_with_empty_texts(service):
    """Test batch with some empty texts."""
    texts = ["Valid text", "", "Another valid text"]
    embeddings = service.embed_batch(texts)

    assert len(embeddings) == 3
    assert all(len(emb) == 384 for emb in embeddings)

    # Middle embedding should be zero vector
    assert all(x == 0.0 for x in embeddings[1])


def test_embed_batch_error_handling(service):
    """Test batch handles encoding errors."""
    service.model.encode = MagicMock(side_effect=RuntimeError("Batch encoding failed"))

    with pytest.raises(RuntimeError, match="Batch encoding failed"):
        service.embed_batch(["text1", "text2"])


# =============================================================================
# SIMILARITY TESTS
# =============================================================================


def test_similarity_calculation(service):
    """Test cosine similarity calculation."""
    text1 = "The cat sits on the mat"
    text2 = "The feline rests on the rug"

    similarity = service.similarity(text1, text2)

    # Should be between 0 and 1
    assert 0.0 <= similarity <= 1.0
    assert isinstance(similarity, float)


def test_similarity_identical_texts(service):
    """Test similarity of identical texts is close to 1.0."""
    text = "Identical text"
    similarity = service.similarity(text, text)

    # Should be very close to 1.0
    assert similarity > 0.99


def test_similarity_empty_text(service):
    """Test similarity with empty text returns 0.0."""
    similarity = service.similarity("", "test")
    assert similarity == 0.0

    similarity = service.similarity("test", "")
    assert similarity == 0.0


def test_similarity_error_handling(service):
    """Test similarity handles calculation errors."""
    # Mock embed to raise error
    service.embed = MagicMock(side_effect=RuntimeError("Embedding failed"))

    with pytest.raises(RuntimeError):
        service.similarity("text1", "text2")


# =============================================================================
# CACHE OPERATION TESTS
# =============================================================================


def test_get_cache_stats(service):
    """Test cache statistics calculation."""
    # Generate some embeddings
    service.embed("test1")
    service.embed("test2")
    service.embed("test1")  # Cache hit

    stats = service.get_cache_stats()

    assert stats['cache_size'] == 2
    assert stats['cache_max_size'] == 100
    assert stats['cache_hits'] == 1
    assert stats['cache_misses'] == 2
    assert stats['hit_rate'] == 1/3


def test_get_cache_stats_empty(service):
    """Test cache stats when cache is empty."""
    stats = service.get_cache_stats()

    assert stats['cache_size'] == 0
    assert stats['cache_hits'] == 0
    assert stats['cache_misses'] == 0
    assert stats['hit_rate'] == 0.0


def test_clear_cache(service):
    """Test cache clearing."""
    # Populate cache
    service.embed("test1")
    service.embed("test2")
    assert len(service._cache) > 0

    # Clear cache
    service.clear_cache()

    assert len(service._cache) == 0


def test_cache_key_generation(service):
    """Test SHA-256 hash generation for cache keys."""
    text = "Test text"

    key1 = service._get_cache_key(text)
    key2 = service._get_cache_key(text)

    # Should be consistent
    assert key1 == key2

    # Should be SHA-256 hex string (64 chars)
    assert isinstance(key1, str)
    assert len(key1) == 64

    # Different text should produce different key
    key3 = service._get_cache_key("Different text")
    assert key3 != key1


def test_cache_key_uniqueness(service):
    """Test cache keys are unique for different texts."""
    key1 = service._get_cache_key("text1")
    key2 = service._get_cache_key("text2")

    assert key1 != key2


# =============================================================================
# HELPER METHOD TESTS
# =============================================================================


def test_get_cache_location(service):
    """Test _get_cache_location returns valid path."""
    location = service._get_cache_location()

    assert isinstance(location, str)
    assert len(location) > 0
