"""Embedding-based classifier — base class extracted from game-mvp's pattern.

Usage:
    class MyClassifier(EmbeddingClassifierBase):
        pass

    classifier = MyClassifier(embedding_provider)
    classifier.add_bank("intent", {
        "greeting": ["hello", "hi", "hey"],
        "farewell": ["bye", "goodbye", "see you"],
    })
    result = classifier.classify("hi there", bank="intent")
    # ClassificationResult(label="greeting", score=0.95, all_scores={...})
"""

import math
from typing import Any, Dict, List, Optional

from jeeves_core.protocols.types import ClassificationResult


def _cosine_similarity(a: List[float], b: List[float]) -> float:
    """Pure-Python cosine similarity (no numpy dependency)."""
    dot = sum(x * y for x, y in zip(a, b))
    norm_a = math.sqrt(sum(x * x for x in a))
    norm_b = math.sqrt(sum(x * x for x in b))
    if norm_a == 0 or norm_b == 0:
        return 0.0
    return dot / (norm_a * norm_b)


class EmbeddingClassifierBase:
    """Cosine-similarity classifier over pre-embedded example banks.

    Subclass to add domain-specific logic. Base class handles:
    - Bank management (add_bank / list_banks)
    - Pre-embedding labeled examples via EmbeddingProviderProtocol
    - Classification via average cosine similarity
    """

    def __init__(self, embedding_provider: Any):
        """Initialize with an EmbeddingProviderProtocol-compatible provider."""
        self._provider = embedding_provider
        # bank_name -> label -> List[List[float]] (pre-embedded examples)
        self._banks: Dict[str, Dict[str, List[List[float]]]] = {}

    def add_bank(self, name: str, examples: Dict[str, List[str]]) -> None:
        """Pre-embed labeled example banks.

        Args:
            name: Bank name (e.g. "intent", "topic").
            examples: {label: [example_text, ...]} mapping.
        """
        bank: Dict[str, List[List[float]]] = {}
        for label, texts in examples.items():
            if texts:
                bank[label] = self._provider.embed(texts)
        self._banks[name] = bank

    def list_banks(self) -> List[str]:
        """Return registered bank names."""
        return list(self._banks.keys())

    def classify(
        self,
        text: str,
        *,
        bank: str = "default",
    ) -> ClassificationResult:
        """Classify text against a pre-embedded bank.

        Returns the label with the highest average cosine similarity.

        Args:
            text: Input text to classify.
            bank: Bank name to classify against.

        Returns:
            ClassificationResult with label, score, and all_scores.

        Raises:
            KeyError: If bank name not found.
        """
        if bank not in self._banks:
            raise KeyError(
                f"Bank '{bank}' not found. Available: {self.list_banks()}"
            )

        [query_vec] = self._provider.embed([text])
        bank_data = self._banks[bank]

        all_scores: Dict[str, float] = {}
        for label, embeddings in bank_data.items():
            if not embeddings:
                all_scores[label] = 0.0
                continue
            similarities = [_cosine_similarity(query_vec, emb) for emb in embeddings]
            all_scores[label] = sum(similarities) / len(similarities)

        if not all_scores:
            return ClassificationResult(label="", score=0.0, all_scores={})

        best_label = max(all_scores, key=all_scores.get)  # type: ignore[arg-type]
        return ClassificationResult(
            label=best_label,
            score=all_scores[best_label],
            all_scores=all_scores,
        )
