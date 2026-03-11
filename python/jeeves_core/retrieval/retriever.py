"""Section-based retriever — dict-backed, for hello-world's knowledge section pattern.

Usage:
    retriever = SectionRetriever({
        "greeting": "Hello! I'm a helpful assistant.",
        "hours": "We're open 9am-5pm Monday through Friday.",
    })
    results = await retriever.retrieve("greeting")
    # [RetrievedContext(content="Hello! I'm a helpful assistant.", source="greeting", score=1.0)]
"""

from typing import Any, Dict, List, Optional

from jeeves_core.protocols.types import RetrievedContext


class SectionRetriever:
    """Dict-based retriever implementing ContextRetrieverProtocol.

    Sections are keyed by name. Retrieval is exact key match — no embedding search.
    Suitable for static knowledge bases where the caller knows the section key.
    """

    def __init__(self, sections: Optional[Dict[str, str]] = None):
        self._sections: Dict[str, str] = dict(sections) if sections else {}

    def add_section(self, key: str, content: str) -> None:
        """Add or update a section."""
        self._sections[key] = content

    def list_sections(self) -> List[str]:
        """Return sorted list of section keys."""
        return sorted(self._sections.keys())

    async def retrieve(
        self,
        query: str,
        *,
        limit: int = 10,
        filters: Optional[Dict[str, Any]] = None,
    ) -> List[RetrievedContext]:
        """Retrieve sections matching the query key.

        Exact key match — returns the section if it exists.

        Args:
            query: Section key to look up.
            limit: Maximum results (unused for exact match, kept for protocol compliance).
            filters: Optional filters (unused, kept for protocol compliance).

        Returns:
            List with one RetrievedContext if key exists, empty list otherwise.
        """
        if query in self._sections:
            return [RetrievedContext(
                content=self._sections[query],
                source=query,
                score=1.0,
            )]
        return []

    async def retrieve_multiple(self, keys: List[str]) -> List[RetrievedContext]:
        """Retrieve multiple sections by key.

        Args:
            keys: List of section keys to retrieve.

        Returns:
            List of RetrievedContext for all matching keys.
        """
        results = []
        for key in keys:
            if key in self._sections:
                results.append(RetrievedContext(
                    content=self._sections[key],
                    source=key,
                    score=1.0,
                ))
        return results
