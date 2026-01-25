"""Unit tests for IntentClassifier."""

import pytest
import json
from unittest.mock import AsyncMock, Mock
from memory_module.intent_classifier import IntentClassifier


class MockLLMProvider:
    """Mock LLM provider for testing."""

    async def generate(self, model: str, prompt: str, options: dict = None):
        """Mock generate method."""
        # Extract the actual user input from the prompt
        # The prompt format is: Input: "{content}"
        content = ""
        try:
            if 'Input: "' in prompt:
                start = prompt.find('Input: "') + 8
                end = prompt.find('"', start)
                content = prompt[start:end]
        except (ValueError, IndexError):
            content = ""

        # Return different responses based on actual content
        if "Buy milk" in content or "buy" in content.lower():
            return json.dumps({
                "is_task": 0.95,
                "is_journal": 0.1,
                "is_fact": 0.05,
                "is_message": 0.0,
                "task_attributes": {
                    "title": "Buy milk",
                    "priority": "medium",
                    "tags": ["shopping"]
                },
                "journal_attributes": {},
                "fact_attributes": {}
            })
        elif "great day" in content or "day" in content.lower():
            return json.dumps({
                "is_task": 0.1,
                "is_journal": 0.9,
                "is_fact": 0.0,
                "is_message": 0.1,
                "task_attributes": {},
                "journal_attributes": {
                    "category": "reflection",
                    "sentiment": "positive"
                },
                "fact_attributes": {}
            })
        elif "prefer dark mode" in content or "prefer" in content.lower():
            return json.dumps({
                "is_task": 0.0,
                "is_journal": 0.2,
                "is_fact": 0.95,
                "is_message": 0.05,
                "task_attributes": {},
                "journal_attributes": {},
                "fact_attributes": {
                    "key": "ui_preference",
                    "value": "dark mode",
                    "domain": "settings"
                }
            })
        else:
            # Default to message
            return json.dumps({
                "is_task": 0.0,
                "is_journal": 0.0,
                "is_fact": 0.0,
                "is_message": 1.0,
                "task_attributes": {},
                "journal_attributes": {},
                "fact_attributes": {}
            })


class TestClassifier:
    """Tests for IntentClassifier classification logic."""

    @pytest.fixture
    def mock_llm(self):
        """Create mock LLM provider."""
        return MockLLMProvider()

    @pytest.fixture
    def classifier(self, mock_llm):
        """Create IntentClassifier instance."""
        return IntentClassifier(
            llm_provider=mock_llm,
            task_threshold=0.7,
            journal_threshold=0.5,
            fact_threshold=0.8
        )

    @pytest.mark.asyncio
    async def test_classify_task(self, classifier):
        """Test classifying task intent."""
        result = await classifier.classify("Buy milk tomorrow")

        assert result["is_task"] >= 0.7
        assert result["primary_type"] == "task"
        assert "task_attributes" in result
        assert result["task_attributes"].get("title") == "Buy milk"

    @pytest.mark.asyncio
    async def test_classify_journal(self, classifier):
        """Test classifying journal intent."""
        result = await classifier.classify("Had a great day today")

        assert result["is_journal"] >= 0.5
        assert result["primary_type"] == "journal"
        assert result["journal_attributes"].get("sentiment") == "positive"

    @pytest.mark.asyncio
    async def test_classify_fact(self, classifier):
        """Test classifying fact intent."""
        result = await classifier.classify("I prefer dark mode")

        assert result["is_fact"] >= 0.8
        assert result["primary_type"] == "fact"
        assert "fact_attributes" in result

    @pytest.mark.asyncio
    async def test_classify_message(self, classifier):
        """Test classifying message intent."""
        result = await classifier.classify("Hello, how are you?")

        assert result["is_message"] == 1.0
        assert result["primary_type"] == "message"

    @pytest.mark.asyncio
    async def test_classify_empty_content(self, classifier):
        """Test classifying empty content returns default."""
        result = await classifier.classify("")

        assert result["primary_type"] == "message"
        assert result["is_message"] == 1.0

    @pytest.mark.asyncio
    async def test_classify_with_context(self, classifier):
        """Test classification with context."""
        context = {"user_id": "user123", "session_id": "session456"}
        result = await classifier.classify("Buy milk", context=context)

        assert result is not None
        assert "primary_type" in result

    @pytest.mark.asyncio
    async def test_classify_batch(self, classifier):
        """Test batch classification."""
        contents = [
            "Buy milk tomorrow",
            "Had a great day",
            "I prefer dark mode"
        ]

        results = await classifier.classify_batch(contents)

        assert len(results) == 3
        assert results[0]["primary_type"] == "task"
        assert results[1]["primary_type"] == "journal"
        assert results[2]["primary_type"] == "fact"

    @pytest.mark.asyncio
    async def test_classify_batch_empty(self, classifier):
        """Test batch classification with empty list."""
        results = await classifier.classify_batch([])
        assert results == []

    @pytest.mark.asyncio
    async def test_classify_batch_with_contexts(self, classifier):
        """Test batch classification with contexts."""
        contents = ["Buy milk", "Hello"]
        contexts = [{"user_id": "user1"}, {"user_id": "user2"}]

        results = await classifier.classify_batch(contents, contexts)

        assert len(results) == 2

    def test_parse_llm_response_valid(self, classifier):
        """Test parsing valid LLM response."""
        response = '''{
            "is_task": 0.9,
            "is_journal": 0.1,
            "is_fact": 0.0,
            "is_message": 0.0,
            "task_attributes": {"title": "Test"},
            "journal_attributes": {},
            "fact_attributes": {}
        }'''

        result = classifier._parse_llm_response(response)

        assert result["is_task"] == 0.9
        assert result["task_attributes"]["title"] == "Test"

    def test_parse_llm_response_with_markdown(self, classifier):
        """Test parsing response with markdown code blocks."""
        response = '''```json
{
    "is_task": 0.8,
    "is_journal": 0.0,
    "is_fact": 0.0,
    "is_message": 0.2,
    "task_attributes": {},
    "journal_attributes": {},
    "fact_attributes": {}
}
```'''

        result = classifier._parse_llm_response(response)

        assert result["is_task"] == 0.8

    def test_parse_llm_response_invalid_json(self, classifier):
        """Test parsing invalid JSON returns default."""
        response = "This is not valid JSON"

        result = classifier._parse_llm_response(response)

        assert result["primary_type"] == "message"
        assert result["is_message"] == 1.0

    def test_parse_llm_response_missing_fields(self, classifier):
        """Test parsing response with missing fields."""
        response = '''{
            "is_task": 0.9
        }'''

        result = classifier._parse_llm_response(response)

        # Should fill in missing fields
        assert "is_journal" in result
        assert "is_fact" in result
        assert "is_message" in result

    def test_determine_primary_type_task(self, classifier):
        """Test determining primary type as task."""
        classification = {
            "is_task": 0.9,
            "is_journal": 0.3,
            "is_fact": 0.1,
            "is_message": 0.0
        }

        primary = classifier._determine_primary_type(classification)
        assert primary == "task"

    def test_determine_primary_type_below_threshold(self, classifier):
        """Test primary type when all below threshold."""
        classification = {
            "is_task": 0.5,  # Below 0.7 threshold
            "is_journal": 0.3,  # Below 0.5 threshold
            "is_fact": 0.6,  # Below 0.8 threshold
            "is_message": 0.1
        }

        primary = classifier._determine_primary_type(classification)
        assert primary == "message"  # Should default to message

    def test_get_default_classification(self, classifier):
        """Test getting default classification."""
        default = classifier._get_default_classification()

        assert default["is_message"] == 1.0
        assert default["primary_type"] == "message"
        assert default["is_task"] == 0.0
        assert default["is_journal"] == 0.0
        assert default["is_fact"] == 0.0
