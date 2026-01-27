"""Capability servicer for Telegram Homelab integration."""

import json
import logging
from typing import Any, AsyncIterator, Dict, Optional

from .config import get_config
from .tools import (
    calendar_query,
    file_list,
    file_read,
    file_search,
    notes_search,
    ssh_execute,
)
from .prompts.agent_prompts import (
    INTENT_PROMPT,
    PLANNER_PROMPT,
    SYNTHESIZER_PROMPT,
)

logger = logging.getLogger(__name__)


class TelegramHomelabServicer:
    """
    Servicer for Telegram Homelab capability.

    Implements a simplified 3-agent pipeline:
    1. Intent: Classify user intent and extract parameters
    2. Planner: Create execution plan
    3. Traverser: Execute tools
    4. Synthesizer: Format response for Telegram
    """

    def __init__(self, llm_provider=None):
        self.config = get_config()
        self.llm_provider = llm_provider
        self.tools = {
            "ssh_execute": ssh_execute,
            "file_list": file_list,
            "file_read": file_read,
            "file_search": file_search,
            "calendar_query": calendar_query,
            "notes_search": notes_search,
        }

    async def process_request(
        self,
        user_id: str,
        session_id: Optional[str],
        message: str,
        context: Optional[dict],
    ) -> AsyncIterator[Dict[str, Any]]:
        """
        Process Telegram message through agent pipeline.

        Args:
            user_id: Telegram user ID
            session_id: Telegram chat/conversation ID
            message: User message text
            context: Additional context (telegram_user, chat_type, etc.)

        Yields:
            Events representing agent execution stages
        """
        try:
            # Yield start event
            yield {
                "type": "start",
                "data": {"user_id": user_id, "session_id": session_id, "message": message},
            }

            # Stage 1: Intent - Classify user intent
            yield {"type": "stage_start", "data": {"stage": "intent"}}
            intent_result = await self._run_intent_agent(message)
            yield {"type": "stage_complete", "data": {"stage": "intent", "result": intent_result}}

            # Stage 2: Planner - Create execution plan
            yield {"type": "stage_start", "data": {"stage": "planner"}}
            plan_result = await self._run_planner_agent(message, intent_result)
            yield {"type": "stage_complete", "data": {"stage": "planner", "result": plan_result}}

            # Stage 3: Traverser - Execute tools
            yield {"type": "stage_start", "data": {"stage": "traverser"}}
            execution_result = await self._run_traverser_agent(plan_result)
            yield {
                "type": "stage_complete",
                "data": {"stage": "traverser", "result": execution_result},
            }

            # Stage 4: Synthesizer - Format response
            yield {"type": "stage_start", "data": {"stage": "synthesizer"}}
            synthesis_result = await self._run_synthesizer_agent(
                message, intent_result, execution_result
            )
            yield {
                "type": "stage_complete",
                "data": {"stage": "synthesizer", "result": synthesis_result},
            }

            # Yield final response
            yield {
                "type": "response",
                "data": {
                    "text": synthesis_result.get("response", ""),
                    "citations": synthesis_result.get("citations", []),
                },
            }

            yield {"type": "complete", "data": {"status": "success"}}

        except Exception as e:
            logger.exception(f"Error processing Telegram request: {e}")
            yield {
                "type": "error",
                "data": {"error": str(e), "stage": "servicer"},
            }

    async def _run_intent_agent(self, message: str) -> Dict[str, Any]:
        """Run intent classification agent."""
        if not self.llm_provider:
            # Fallback: simple keyword matching
            return self._fallback_intent_classification(message)

        try:
            prompt = INTENT_PROMPT.format(user_message=message)
            response = await self.llm_provider.generate(model="", prompt=prompt, options={})

            # Parse JSON response
            intent_data = self._parse_json_response(response)
            return intent_data

        except Exception as e:
            logger.error(f"Intent agent failed: {e}")
            return self._fallback_intent_classification(message)

    def _fallback_intent_classification(self, message: str) -> Dict[str, Any]:
        """Fallback intent classification using keywords."""
        message_lower = message.lower()

        if any(kw in message_lower for kw in ["ssh", "execute", "run", "command"]):
            return {
                "intent": "ssh_command",
                "goals": ["Execute SSH command"],
                "parameters": {},
            }
        elif any(kw in message_lower for kw in ["file", "list", "read", "show"]):
            return {
                "intent": "file_access",
                "goals": ["Access file system"],
                "parameters": {},
            }
        elif any(kw in message_lower for kw in ["calendar", "event", "meeting", "schedule"]):
            return {
                "intent": "calendar_query",
                "goals": ["Query calendar"],
                "parameters": {},
            }
        elif any(kw in message_lower for kw in ["note", "search", "find"]):
            return {
                "intent": "notes_search",
                "goals": ["Search notes"],
                "parameters": {},
            }
        else:
            return {
                "intent": "general_query",
                "goals": ["Answer general query"],
                "parameters": {},
            }

    async def _run_planner_agent(
        self, message: str, intent_result: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Run planning agent."""
        if not self.llm_provider:
            # Fallback: simple planning based on intent
            return self._fallback_planning(message, intent_result)

        try:
            prompt = PLANNER_PROMPT.format(
                user_message=message,
                intent=intent_result.get("intent", ""),
                goals=json.dumps(intent_result.get("goals", [])),
                parameters=json.dumps(intent_result.get("parameters", {})),
            )
            response = await self.llm_provider.generate(model="", prompt=prompt, options={})

            # Parse JSON response
            plan_data = self._parse_json_response(response)
            return plan_data

        except Exception as e:
            logger.error(f"Planner agent failed: {e}")
            return self._fallback_planning(message, intent_result)

    def _fallback_planning(
        self, message: str, intent_result: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Fallback planning based on intent."""
        intent = intent_result.get("intent", "general_query")
        params = intent_result.get("parameters", {})

        if intent == "ssh_command":
            return {
                "steps": [
                    {
                        "step": 1,
                        "description": "Execute SSH command",
                        "tool": "ssh_execute",
                        "params": {
                            "hostname": params.get("hostname", ""),
                            "command": params.get("command", ""),
                        },
                    }
                ]
            }
        elif intent == "file_access":
            return {
                "steps": [
                    {
                        "step": 1,
                        "description": "List files",
                        "tool": "file_list",
                        "params": {"path": params.get("path", ".")},
                    }
                ]
            }
        elif intent == "calendar_query":
            return {
                "steps": [
                    {
                        "step": 1,
                        "description": "Query calendar",
                        "tool": "calendar_query",
                        "params": {},
                    }
                ]
            }
        elif intent == "notes_search":
            return {
                "steps": [
                    {
                        "step": 1,
                        "description": "Search notes",
                        "tool": "notes_search",
                        "params": {"query": params.get("query", message)},
                    }
                ]
            }
        else:
            return {"steps": []}

    async def _run_traverser_agent(self, plan_result: Dict[str, Any]) -> Dict[str, Any]:
        """Run tool execution agent."""
        steps = plan_result.get("steps", [])
        results = []

        for step in steps:
            tool_name = step.get("tool")
            params = step.get("params", {})

            if tool_name not in self.tools:
                results.append({
                    "step": step.get("step"),
                    "status": "error",
                    "error": f"Unknown tool: {tool_name}",
                })
                continue

            try:
                # Execute tool
                tool_func = self.tools[tool_name]
                tool_result = await tool_func(**params)

                results.append({
                    "step": step.get("step"),
                    "status": tool_result.status,
                    "data": tool_result.data,
                    "citations": tool_result.citations,
                    "error": tool_result.error_message,
                })

            except Exception as e:
                logger.exception(f"Tool execution failed for {tool_name}: {e}")
                results.append({
                    "step": step.get("step"),
                    "status": "error",
                    "error": str(e),
                })

        return {"results": results, "total_steps": len(steps)}

    async def _run_synthesizer_agent(
        self, message: str, intent_result: Dict[str, Any], execution_result: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Run synthesis agent to format response."""
        if not self.llm_provider:
            # Fallback: simple formatting
            return self._fallback_synthesis(execution_result)

        try:
            prompt = SYNTHESIZER_PROMPT.format(
                user_message=message,
                intent=intent_result.get("intent", ""),
                execution_results=json.dumps(execution_result.get("results", []), indent=2),
            )
            response = await self.llm_provider.generate(model="", prompt=prompt, options={})

            # Parse JSON response
            synthesis_data = self._parse_json_response(response)
            return synthesis_data

        except Exception as e:
            logger.error(f"Synthesizer agent failed: {e}")
            return self._fallback_synthesis(execution_result)

    def _fallback_synthesis(self, execution_result: Dict[str, Any]) -> Dict[str, Any]:
        """Fallback synthesis - simple formatting."""
        results = execution_result.get("results", [])

        if not results:
            return {
                "response": "No results to display.",
                "citations": [],
            }

        # Format results as text
        response_parts = []
        all_citations = []

        for result in results:
            status = result.get("status", "unknown")

            if status == "success":
                data = result.get("data", {})
                response_parts.append(f"**Step {result.get('step')}:** Success")
                response_parts.append(f"```json\n{json.dumps(data, indent=2)}\n```")
                all_citations.extend(result.get("citations", []))
            else:
                error = result.get("error", "Unknown error")
                response_parts.append(f"**Step {result.get('step')}:** Error - {error}")

        response_text = "\n\n".join(response_parts)

        # Truncate if too long for Telegram
        max_length = self.config.telegram.max_message_length - 100
        if len(response_text) > max_length:
            response_text = response_text[:max_length] + "\n\n... (truncated)"

        return {
            "response": response_text,
            "citations": all_citations,
        }

    def _parse_json_response(self, response: str) -> Dict[str, Any]:
        """Parse JSON from LLM response, handling markdown code blocks."""
        # Remove markdown code fences if present
        response = response.strip()
        if response.startswith("```json"):
            response = response[7:]
        elif response.startswith("```"):
            response = response[3:]

        if response.endswith("```"):
            response = response[:-3]

        response = response.strip()

        try:
            return json.loads(response)
        except json.JSONDecodeError as e:
            logger.warning(f"Failed to parse JSON response: {e}")
            return {}
