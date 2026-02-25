"""
Infrastructure Tools Module - Generic Tool Infrastructure.

This module provides the infrastructure layer for tool registration and
execution. Capability-specific tool identifiers and implementations are
owned by each capability layer.

Usage:
    from jeeves_infra.tools import (
        ToolCatalog,
        ToolCatalogEntry,
        ToolDefinition,
        ToolCategory,
        RiskSemantic,
        RiskSeverity,
    )
"""

from jeeves_infra.tools.catalog import (
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
]
