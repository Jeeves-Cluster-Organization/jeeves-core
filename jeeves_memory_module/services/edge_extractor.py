"""
Edge Extractor for L5 Relationship Memory.

Uses LLM to extract relationships from text content:
- Entity mentions
- Relationship types
- Cross-references

Constitutional Alignment:
- P1: NLP-First (uses LLM for extraction)
- P5: Deterministic spine (LLM extracts, code stores)
"""

from typing import Dict, Any, Optional, List
from datetime import datetime, timezone
import json

from jeeves_memory_module.repositories.graph_repository import RelationType, Edge
from jeeves_shared import get_component_logger
from jeeves_protocols import LoggerProtocol, LLMProviderProtocol


# System prompt for relationship extraction
EXTRACTION_PROMPT = '''You are extracting relationships from text.

Analyze the text and identify:
1. Entities mentioned (tasks, people, topics, dates, etc.)
2. Relationships between entities
3. Cross-references to other items

Text to analyze:
"{text}"

Context:
- Source type: {source_type}
- Source ID: {source_id}

Relationship types to detect:
- references: Entity A mentions/refers to Entity B
- related_to: General topical relation
- depends_on: Entity A depends on Entity B
- blocks: Entity A blocks Entity B
- parent_of / child_of: Hierarchical relationships
- preceded_by / followed_by: Temporal order
- similar_to: Semantic similarity
- contradicts: Conflicting information
- supports: Supporting information

Output valid JSON only:
{{
    "entities": [
        {{"type": "task|person|topic|date|item", "name": "entity name", "id": "if known"}}
    ],
    "relationships": [
        {{
            "source_type": "type of source entity",
            "source_name": "name or id of source",
            "target_type": "type of target entity",
            "target_name": "name or id of target",
            "relation": "references|related_to|depends_on|...",
            "confidence": 0.0-1.0,
            "context": "short context quote"
        }}
    ]
}}

If no relationships are found, return: {{"entities": [], "relationships": []}}

Output ONLY valid JSON, no markdown.'''


class ExtractionResult:
    """Result of relationship extraction."""

    def __init__(
        self,
        entities: Optional[List[Dict[str, str]]] = None,
        relationships: Optional[List[Dict[str, Any]]] = None,
        raw_response: str = "",
        extraction_time_ms: int = 0
    ):
        """
        Initialize extraction result.

        Args:
            entities: List of extracted entities
            relationships: List of extracted relationships
            raw_response: Raw LLM response
            extraction_time_ms: Time taken for extraction
        """
        self.entities = entities or []
        self.relationships = relationships or []
        self.raw_response = raw_response
        self.extraction_time_ms = extraction_time_ms
        self.extracted_at = datetime.now(timezone.utc)

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "entities": self.entities,
            "relationships": self.relationships,
            "extraction_time_ms": self.extraction_time_ms,
            "extracted_at": self.extracted_at.isoformat()
        }


class EdgeExtractor:
    """
    Extracts relationships from text using LLM.

    Constitutional alignment:
    - P1: NLP-First (uses LLM, not patterns)
    - P5: Deterministic spine (LLM extracts, code validates and stores)
    """

    def __init__(
        self,
        provider: Optional[LLMProviderProtocol] = None,
        model: str = "llama3.1:8b-instruct-q4_0",
        temperature: float = 0.2,
        use_mock: bool = False,
        logger: Optional[LoggerProtocol] = None
    ):
        """
        Initialize extractor.

        Args:
            provider: LLM provider instance
            model: Model to use for extraction
            temperature: LLM temperature
            use_mock: Use mock extraction for testing
            logger: Optional logger instance (ADR-001 DI)
        """
        self._logger = get_component_logger("EdgeExtractor", logger)
        self.provider = provider
        self.model = model
        self.temperature = temperature
        self.use_mock = use_mock

    async def extract(
        self,
        text: str,
        source_type: str,
        source_id: str
    ) -> ExtractionResult:
        """
        Extract relationships from text.

        Args:
            text: Text to analyze
            source_type: Type of source document
            source_id: Source document ID

        Returns:
            ExtractionResult with entities and relationships
        """
        import time
        start_time = time.time()

        if not text or not text.strip():
            return ExtractionResult(
                extraction_time_ms=0
            )

        if self.use_mock:
            result = self._mock_extract(text, source_type, source_id)
            result.extraction_time_ms = int((time.time() - start_time) * 1000)
            return result

        if not self.provider:
            raise ValueError("LLM provider not configured for EdgeExtractor")

        prompt = EXTRACTION_PROMPT.format(
            text=text[:2000],  # Limit input length
            source_type=source_type,
            source_id=source_id
        )

        try:
            options = {
                "temperature": self.temperature,
                "num_predict": 800,
                "num_ctx": 16384,  # Qwen2.5-3B context window
            }

            raw_response = await self.provider.generate(
                model=self.model,
                prompt=prompt,
                options=options
            )

            result = self._parse_response(raw_response)
            result.extraction_time_ms = int((time.time() - start_time) * 1000)

            self._logger.info(
                "extraction_complete",
                source_type=source_type,
                source_id=source_id,
                entities_found=len(result.entities),
                relationships_found=len(result.relationships)
            )

            return result

        except Exception as e:
            self._logger.error(
                "extraction_failed",
                error=str(e),
                source_type=source_type
            )
            return ExtractionResult(
                raw_response=f"Error: {str(e)}",
                extraction_time_ms=int((time.time() - start_time) * 1000)
            )

    async def extract_and_create_edges(
        self,
        text: str,
        user_id: str,
        source_type: str,
        source_id: str,
        graph_service: Any  # GraphService
    ) -> List[Edge]:
        """
        Extract relationships and create edges in the graph.

        Convenience method that extracts and stores in one call.

        Args:
            text: Text to analyze
            user_id: Owner user ID
            source_type: Source type
            source_id: Source ID
            graph_service: GraphService instance for storing edges

        Returns:
            List of created edges
        """
        result = await self.extract(text, source_type, source_id)

        created_edges: List[Edge] = []

        for rel in result.relationships:
            try:
                # Map relationship type
                rel_type_str = rel.get("relation", "related_to")
                try:
                    rel_type = RelationType(rel_type_str)
                except ValueError:
                    rel_type = RelationType.RELATED_TO

                # Create edge
                edge = await graph_service.create_relationship(
                    user_id=user_id,
                    source_type=rel.get("source_type", source_type),
                    source_id=rel.get("source_name", source_id),
                    target_type=rel.get("target_type", "unknown"),
                    target_id=rel.get("target_name", "unknown"),
                    relation_type=rel_type,
                    weight=rel.get("confidence", 0.8),
                    extracted_by="llm",
                    context=rel.get("context")
                )
                created_edges.append(edge)

            except Exception as e:
                self._logger.warning(
                    "edge_creation_failed",
                    error=str(e),
                    relationship=rel
                )

        return created_edges

    def _parse_response(self, raw_response: str) -> ExtractionResult:
        """Parse LLM response into ExtractionResult."""
        try:
            response_text = raw_response.strip()
            if "```json" in response_text:
                response_text = response_text.split("```json")[1].split("```")[0].strip()
            elif "```" in response_text:
                response_text = response_text.split("```")[1].split("```")[0].strip()

            data = json.loads(response_text)

            return ExtractionResult(
                entities=data.get("entities", []),
                relationships=data.get("relationships", []),
                raw_response=raw_response
            )

        except (json.JSONDecodeError, KeyError) as e:
            self._logger.warning(
                "extraction_parse_error",
                error=str(e),
                raw_response=raw_response[:200]
            )
            return ExtractionResult(
                raw_response=raw_response
            )

    def _mock_extract(
        self,
        text: str,
        source_type: str,
        source_id: str
    ) -> ExtractionResult:
        """Mock extraction for testing."""
        entities = []
        relationships = []

        # Simple keyword detection for mock
        text_lower = text.lower()

        # Detect task mentions
        if "task" in text_lower:
            entities.append({"type": "task", "name": "mentioned task"})

        # Detect people mentions (common patterns)
        if any(word in text_lower for word in ["john", "mary", "bob", "alice"]):
            entities.append({"type": "person", "name": "mentioned person"})

        # Detect date mentions
        if any(word in text_lower for word in ["tomorrow", "monday", "next week", "today"]):
            entities.append({"type": "date", "name": "mentioned date"})

        # Create mock relationships
        if len(entities) >= 2:
            relationships.append({
                "source_type": source_type,
                "source_name": source_id,
                "target_type": entities[0]["type"],
                "target_name": entities[0]["name"],
                "relation": "references",
                "confidence": 0.8,
                "context": text[:50]
            })

        return ExtractionResult(
            entities=entities,
            relationships=relationships,
            raw_response="mock"
        )
