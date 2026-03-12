"""Tests for retrieval module — RetrievedContext, ClassificationResult, SectionRetriever, EmbeddingClassifierBase."""

import pytest
from dataclasses import FrozenInstanceError
from typing import Any, Dict, List

from jeeves_core.protocols.types import (
    RetrievedContext,
    ClassificationResult,
    AgentConfig,
    stage,
)
from jeeves_core.protocols.interfaces import ContextRetrieverProtocol, EmbeddingProviderProtocol
from jeeves_core.retrieval import EmbeddingClassifierBase, SectionRetriever


# =============================================================================
# Frozen dataclass tests
# =============================================================================


class TestRetrievedContext:
    def test_frozen(self):
        ctx = RetrievedContext(content="hello", source="doc1", score=0.9)
        with pytest.raises(FrozenInstanceError):
            ctx.content = "modified"

    def test_defaults(self):
        ctx = RetrievedContext(content="text")
        assert ctx.source == ""
        assert ctx.score == 0.0
        assert ctx.metadata == {}


class TestClassificationResult:
    def test_frozen(self):
        result = ClassificationResult(label="greeting", score=0.95)
        with pytest.raises(FrozenInstanceError):
            result.label = "farewell"

    def test_all_scores(self):
        result = ClassificationResult(
            label="greeting", score=0.95,
            all_scores={"greeting": 0.95, "farewell": 0.3},
        )
        assert result.all_scores["farewell"] == 0.3


# =============================================================================
# SectionRetriever
# =============================================================================


class TestSectionRetriever:
    @pytest.mark.asyncio
    async def test_retrieve_existing_key(self):
        retriever = SectionRetriever({"greeting": "Hello!", "hours": "9-5"})
        results = await retriever.retrieve("greeting")

        assert len(results) == 1
        assert results[0].content == "Hello!"
        assert results[0].source == "greeting"
        assert results[0].score == 1.0

    @pytest.mark.asyncio
    async def test_retrieve_missing_key(self):
        retriever = SectionRetriever({"greeting": "Hello!"})
        results = await retriever.retrieve("nonexistent")
        assert results == []

    @pytest.mark.asyncio
    async def test_retrieve_multiple(self):
        retriever = SectionRetriever({
            "a": "content_a",
            "b": "content_b",
            "c": "content_c",
        })
        results = await retriever.retrieve_multiple(["a", "c", "missing"])

        assert len(results) == 2
        sources = [r.source for r in results]
        assert "a" in sources
        assert "c" in sources

    def test_add_section(self):
        retriever = SectionRetriever()
        retriever.add_section("new", "new content")
        assert "new" in retriever.list_sections()

    def test_list_sections_sorted(self):
        retriever = SectionRetriever({"z": "z", "a": "a", "m": "m"})
        assert retriever.list_sections() == ["a", "m", "z"]

    @pytest.mark.asyncio
    async def test_protocol_compliance(self):
        """SectionRetriever satisfies ContextRetrieverProtocol."""
        retriever = SectionRetriever({"key": "value"})
        assert isinstance(retriever, ContextRetrieverProtocol)


# =============================================================================
# EmbeddingClassifierBase
# =============================================================================


class MockEmbeddingProvider:
    """Mock embedding provider for testing."""

    def __init__(self, dim: int = 3):
        self._dim = dim
        self._counter = 0

    def embed(self, texts: List[str]) -> List[List[float]]:
        """Simple deterministic embeddings: different texts get different vectors."""
        results = []
        for text in texts:
            # Use hash to produce deterministic but distinct vectors
            h = hash(text) % 1000
            vec = [float((h + i) % 7) for i in range(self._dim)]
            # Normalize
            norm = sum(x * x for x in vec) ** 0.5
            if norm > 0:
                vec = [x / norm for x in vec]
            results.append(vec)
        return results

    @property
    def dimension(self) -> int:
        return self._dim


class IdentityEmbeddingProvider:
    """Provider that returns fixed vectors for testing classification logic."""

    def __init__(self):
        self._mappings: Dict[str, List[float]] = {}

    def set_mapping(self, text: str, vec: List[float]) -> None:
        self._mappings[text] = vec

    def embed(self, texts: List[str]) -> List[List[float]]:
        return [self._mappings.get(t, [0.0, 0.0, 0.0]) for t in texts]

    @property
    def dimension(self) -> int:
        return 3


class TestEmbeddingClassifierBase:
    def test_add_bank_and_list(self):
        provider = MockEmbeddingProvider()
        classifier = EmbeddingClassifierBase(provider)
        classifier.add_bank("intent", {
            "greeting": ["hello", "hi"],
            "farewell": ["bye", "goodbye"],
        })

        assert "intent" in classifier.list_banks()

    def test_classify_returns_result(self):
        provider = MockEmbeddingProvider()
        classifier = EmbeddingClassifierBase(provider)
        classifier.add_bank("intent", {
            "greeting": ["hello", "hi", "hey"],
            "farewell": ["bye", "goodbye"],
        })

        result = classifier.classify("hello", bank="intent")
        assert isinstance(result, ClassificationResult)
        assert result.label in ("greeting", "farewell")
        assert result.score > 0
        assert len(result.all_scores) == 2

    def test_classify_picks_correct_label(self):
        """With controlled embeddings, classifier picks the most similar label."""
        provider = IdentityEmbeddingProvider()
        # greeting examples are [1,0,0], farewell examples are [0,1,0]
        provider.set_mapping("hello", [1.0, 0.0, 0.0])
        provider.set_mapping("hi", [1.0, 0.0, 0.0])
        provider.set_mapping("bye", [0.0, 1.0, 0.0])
        provider.set_mapping("goodbye", [0.0, 1.0, 0.0])
        # query is close to greeting
        provider.set_mapping("hey there", [0.9, 0.1, 0.0])

        classifier = EmbeddingClassifierBase(provider)
        classifier.add_bank("intent", {
            "greeting": ["hello", "hi"],
            "farewell": ["bye", "goodbye"],
        })

        result = classifier.classify("hey there", bank="intent")
        assert result.label == "greeting"
        assert result.score > result.all_scores["farewell"]

    def test_classify_unknown_bank_raises(self):
        provider = MockEmbeddingProvider()
        classifier = EmbeddingClassifierBase(provider)

        with pytest.raises(KeyError, match="not found"):
            classifier.classify("text", bank="nonexistent")

    def test_protocol_compliance(self):
        """MockEmbeddingProvider satisfies EmbeddingProviderProtocol."""
        provider = MockEmbeddingProvider()
        assert isinstance(provider, EmbeddingProviderProtocol)
