"""Database constants for PostgreSQL client.

Centralizes column type definitions to avoid duplication across
insert, update, and query operations.

Constitutional Alignment:
- P1: Single source of truth for column types
- M5: Consistent data handling
"""

from typing import FrozenSet

# ============================================================================
# UUID Columns
# ============================================================================
# Columns that should be treated as UUIDs and converted to strings
# Used for parameter preparation in queries, inserts, and updates

UUID_COLUMNS: FrozenSet[str] = frozenset({
    # Core identifiers
    'session_id',
    'request_id',
    'task_id',
    'entry_id',
    'plan_id',
    'response_id',
    'source_request_id',
    'fact_id',
    # Memory module identifiers
    'chunk_id',
    'trace_id',
    'event_id',
    'user_id',
    'agent_id',
    'edge_id',
    # Loop and metric identifiers
    'loop_id',
    'metric_id',
    'originating_session_id',
    'resolution_session_id',
    # Correlation
    'correlation_id',
    'envelope_id',
})

# ============================================================================
# JSONB Columns
# ============================================================================
# Columns that should be serialized as JSON for PostgreSQL JSONB storage

JSONB_COLUMNS: FrozenSet[str] = frozenset({
    # Plan and execution data
    'plan_json',
    'execution_plan_json',
    'action_json',
    # Metadata and configuration
    'metadata',
    'metadata_json',
    'config_json',
    'parameters',
    'payload',
    # Results and reports
    'result_data',
    'error_details',
    'validation_report',
    'issues_json',
    # Session state
    'focus_context',
    'referenced_entities',
    'structured_facts',
    'rag_results',
    # Other
    'tags',
    'examples_json',
    'synonyms_json',
})

# ============================================================================
# Vector Columns
# ============================================================================
# Columns that should be cast to pgvector type

VECTOR_COLUMNS: FrozenSet[str] = frozenset({
    'embedding',
})

# ============================================================================
# All Special Columns
# ============================================================================
# Union of all columns that need special handling

SPECIAL_COLUMNS: FrozenSet[str] = UUID_COLUMNS | JSONB_COLUMNS | VECTOR_COLUMNS


__all__ = [
    'UUID_COLUMNS',
    'JSONB_COLUMNS',
    'VECTOR_COLUMNS',
    'SPECIAL_COLUMNS',
]
