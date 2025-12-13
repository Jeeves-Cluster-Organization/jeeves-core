"""
Summarization Service for L4 Working Memory.

Uses LLM to compress conversation history into concise summaries
for short-term memory management.

Constitutional Alignment:
- P1: NLP-First (uses LLM for understanding, not patterns)
- P5: Deterministic spine (LLM shapes summary, code stores it)
- M4: Structured output with schema versioning
"""

from typing import Dict, Any, Optional, List
from datetime import datetime, timezone
import json

from jeeves_shared import get_component_logger
from jeeves_protocols import LoggerProtocol, LLMProviderProtocol



# System prompt for conversation summarization
SUMMARIZATION_PROMPT = '''You are summarizing a conversation between a user and an AI assistant.

Create a concise summary that captures:
1. Key topics discussed
2. Important entities mentioned (tasks, dates, people)
3. User preferences expressed
4. Commitments or follow-ups mentioned
5. Current context/focus

The summary should be useful for providing context in future turns.

Conversation to summarize:
{conversation}

Provide a summary in JSON format:
{{
    "summary": "Brief narrative summary (2-3 sentences)",
    "key_topics": ["topic1", "topic2"],
    "entities_mentioned": [
        {{"type": "task", "name": "example task"}},
        {{"type": "person", "name": "John"}}
    ],
    "user_preferences": ["any preferences expressed"],
    "follow_ups": ["any mentioned follow-ups"],
    "current_focus": "what the conversation is currently about"
}}

Output only valid JSON, no markdown.'''


class SummarizationResult:
    """Result of conversation summarization."""

    def __init__(
        self,
        summary: str,
        key_topics: Optional[List[str]] = None,
        entities_mentioned: Optional[List[Dict[str, str]]] = None,
        user_preferences: Optional[List[str]] = None,
        follow_ups: Optional[List[str]] = None,
        current_focus: Optional[str] = None,
        raw_response: str = "",
        schema_version: int = 1,
        logger: Optional[LoggerProtocol] = None
    ):
        """
        Initialize summarization result.

        Args:
            summary: Brief narrative summary
            key_topics: List of key topics discussed
            entities_mentioned: List of entities with type and name
            user_preferences: Any user preferences expressed
            follow_ups: Mentioned follow-up items
            current_focus: What the conversation is currently about
            raw_response: Raw LLM response for debugging
            schema_version: Version of the result schema

            logger: Optional logger instance (ADR-001 DI)
        """
        self.summary = summary
        self.key_topics = key_topics or []
        self.entities_mentioned = entities_mentioned or []
        self.user_preferences = user_preferences or []
        self.follow_ups = follow_ups or []
        self.current_focus = current_focus
        self.raw_response = raw_response
        self.schema_version = schema_version
        self.created_at = datetime.now(timezone.utc)

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary."""
        return {
            "summary": self.summary,
            "key_topics": self.key_topics,
            "entities_mentioned": self.entities_mentioned,
            "user_preferences": self.user_preferences,
            "follow_ups": self.follow_ups,
            "current_focus": self.current_focus,
            "schema_version": self.schema_version,
            "created_at": self.created_at.isoformat()
        }

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "SummarizationResult":
        """Create from dictionary."""
        return cls(
            summary=data.get("summary", ""),
            key_topics=data.get("key_topics", []),
            entities_mentioned=data.get("entities_mentioned", []),
            user_preferences=data.get("user_preferences", []),
            follow_ups=data.get("follow_ups", []),
            current_focus=data.get("current_focus"),
            schema_version=data.get("schema_version", 1)
        )

    def to_short_term_memory(self) -> str:
        """Convert to a compact string for short-term memory storage."""
        parts = [self.summary]

        if self.current_focus:
            parts.append(f"Focus: {self.current_focus}")

        if self.key_topics:
            parts.append(f"Topics: {', '.join(self.key_topics[:5])}")

        if self.entities_mentioned:
            entity_strs = [
                f"{e.get('type', 'item')}: {e.get('name', 'unknown')}"
                for e in self.entities_mentioned[:5]
            ]
            parts.append(f"Entities: {', '.join(entity_strs)}")

        return " | ".join(parts)


class SummarizationService:
    """
    Service for summarizing conversations.

    Uses LLM to create compressed summaries of conversation history
    for inclusion in short-term memory.
    """

    def __init__(
        self,
        provider: Optional[LLMProviderProtocol] = None,
        model: str = "llama3.1:8b-instruct-q4_0",
        temperature: float = 0.3,
        use_mock: bool = False,
        logger: Optional[LoggerProtocol] = None
    ):
        """
        Initialize service.

        Args:
            provider: LLM provider instance
            model: Model to use for summarization
            temperature: LLM temperature (default: 0.3 for consistency)
            use_mock: Use mock responses for testing
            logger: Optional logger instance (ADR-001 DI)
        """
        self.provider = provider
        self.model = model
        self.temperature = temperature
        self.use_mock = use_mock
        self._logger = get_component_logger("SummarizationService", logger)

    async def summarize_conversation(
        self,
        messages: List[Dict[str, str]],
        max_messages: int = 20
    ) -> SummarizationResult:
        """
        Summarize a conversation history.

        Args:
            messages: List of messages with 'role' and 'content'
            max_messages: Maximum messages to include (default: 20)

        Returns:
            SummarizationResult with structured summary
        """
        if not messages:
            return SummarizationResult(
                summary="No conversation to summarize.",
                current_focus="general"
            )

        # Limit messages
        recent_messages = messages[-max_messages:]

        # Format conversation
        conversation_text = self._format_conversation(recent_messages)

        if self.use_mock:
            return self._mock_summarize(recent_messages)

        if not self.provider:
            raise ValueError("LLM provider not configured for SummarizationService")

        # Build prompt
        prompt = SUMMARIZATION_PROMPT.format(conversation=conversation_text)

        try:
            options = {
                "temperature": self.temperature,
                "num_predict": 500,   # Limit response length
                "num_ctx": 16384,     # Qwen2.5-3B context window
            }

            raw_response = await self.provider.generate(
                model=self.model,
                prompt=prompt,
                options=options
            )

            result = self._parse_response(raw_response)

            self._logger.info(
                "conversation_summarized",
                message_count=len(recent_messages),
                summary_length=len(result.summary)
            )

            return result

        except Exception as e:
            self._logger.error(
                "summarization_failed",
                error=str(e),
                message_count=len(recent_messages)
            )
            # Return a basic fallback summary
            return SummarizationResult(
                summary=f"Conversation with {len(recent_messages)} messages.",
                current_focus="general",
                raw_response=f"Error: {str(e)}"
            )

    async def summarize_for_storage(
        self,
        messages: List[Dict[str, str]],
        existing_summary: Optional[str] = None
    ) -> str:
        """
        Create a compact summary suitable for database storage.

        Optionally incorporates an existing summary for continuity.

        Args:
            messages: Recent messages to summarize
            existing_summary: Previous summary to build upon (optional)

        Returns:
            Compact summary string
        """
        result = await self.summarize_conversation(messages)

        # If we have an existing summary, combine them
        if existing_summary:
            combined = f"Previous: {existing_summary[:200]}... Current: {result.summary}"
            return combined[:500]  # Limit total length

        return result.to_short_term_memory()[:500]

    async def extract_entities(
        self,
        messages: List[Dict[str, str]]
    ) -> List[Dict[str, str]]:
        """
        Extract mentioned entities from conversation.

        Useful for populating session state referenced entities.

        Args:
            messages: Messages to analyze

        Returns:
            List of entities with type and name
        """
        result = await self.summarize_conversation(messages)
        return result.entities_mentioned

    async def extract_follow_ups(
        self,
        messages: List[Dict[str, str]]
    ) -> List[str]:
        """
        Extract potential follow-up items from conversation.

        Useful for creating open loops.

        Args:
            messages: Messages to analyze

        Returns:
            List of follow-up items
        """
        result = await self.summarize_conversation(messages)
        return result.follow_ups

    def _format_conversation(self, messages: List[Dict[str, str]]) -> str:
        """Format messages into a conversation string."""
        lines = []
        for msg in messages:
            role = msg.get("role", "unknown").capitalize()
            content = msg.get("content", "")
            # Truncate very long messages
            if len(content) > 500:
                content = content[:500] + "..."
            lines.append(f"{role}: {content}")
        return "\n".join(lines)

    def _parse_response(self, raw_response: str) -> SummarizationResult:
        """Parse LLM response into SummarizationResult."""
        try:
            # Clean up response - extract JSON from markdown if present
            response_text = raw_response.strip()
            if "```json" in response_text:
                response_text = response_text.split("```json")[1].split("```")[0].strip()
            elif "```" in response_text:
                response_text = response_text.split("```")[1].split("```")[0].strip()

            data = json.loads(response_text)

            return SummarizationResult(
                summary=data.get("summary", ""),
                key_topics=data.get("key_topics", []),
                entities_mentioned=data.get("entities_mentioned", []),
                user_preferences=data.get("user_preferences", []),
                follow_ups=data.get("follow_ups", []),
                current_focus=data.get("current_focus"),
                raw_response=raw_response
            )

        except (json.JSONDecodeError, KeyError) as e:
            self._logger.warning(
                "summarization_parse_error",
                error=str(e),
                raw_response=raw_response[:200]
            )
            # Return basic result with raw text as summary
            return SummarizationResult(
                summary=raw_response[:200] if raw_response else "Unable to parse summary.",
                raw_response=raw_response
            )

    def _mock_summarize(self, messages: List[Dict[str, str]]) -> SummarizationResult:
        """Create mock summary for testing."""
        # Extract some basic info from messages
        topics = set()
        entities = []

        for msg in messages:
            content = msg.get("content", "").lower()
            # Simple keyword extraction
            if "task" in content:
                topics.add("tasks")
            if "reminder" in content or "remind" in content:
                topics.add("reminders")
            if "journal" in content:
                topics.add("journaling")

        return SummarizationResult(
            summary=f"Conversation with {len(messages)} messages.",
            key_topics=list(topics)[:5],
            entities_mentioned=entities,
            current_focus="general",
            raw_response="mock"
        )
