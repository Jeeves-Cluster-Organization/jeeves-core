"""
Infrastructure Tools Module - Generic Tool Infrastructure.

This module provides the infrastructure layer for tool registration and
execution. Capability-specific tool identifiers and implementations are
owned by each capability layer.

Usage:
    from jeeves_core.tools import (
        ToolCatalog,
        ToolCatalogEntry,
        ToolDefinition,
        ToolCategory,
        RiskSemantic,
        RiskSeverity,
    )
"""

from jeeves_core.tools.catalog import (
    # Classification (from protocols)
    ToolCategory,
    RiskSemantic,
    RiskSeverity,
    # Dataclasses
    ToolCatalogEntry,
    ToolDefinition,
    # Catalog
    ToolCatalog,
)

from jeeves_core.tools.decorator import tool


def normalize_execution_result(execution: dict) -> str:
    """Extract tool result from _dispatch_tool output as JSON string."""
    import json
    if execution.get("result"):
        result_data = execution["result"]
        if hasattr(result_data, "model_dump_json"):
            return result_data.model_dump_json()
        elif isinstance(result_data, dict):
            return json.dumps(result_data)
        return str(result_data)
    elif execution.get("status") == "skipped":
        return json.dumps({"status": "SUCCESS", "message": "No tool needed for this request"})
    elif execution.get("error"):
        return json.dumps({"status": "ERROR", "message": execution["error"]})
    return json.dumps({"status": "SUCCESS", "message": "No tool result"})


__all__ = [
    # Classification
    "ToolCategory",
    "RiskSemantic",
    "RiskSeverity",
    # Dataclasses
    "ToolCatalogEntry",
    "ToolDefinition",
    # Catalog
    "ToolCatalog",
    # Decorator
    "tool",
    # Helpers
    "normalize_execution_result",
]
