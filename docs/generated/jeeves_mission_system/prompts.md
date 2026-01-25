# mission_system.prompts - Prompt Management

**Package:** `mission_system.prompts`  
**Purpose:** Centralized prompt registry and template system  
**Updated:** 2026-01-23

---

## Overview

The `prompts` package provides centralized prompt management for the Mission System. It includes:
- Versioned prompt registry
- Shared prompt building blocks
- Template utilities

### Constitutional Alignment

- **P1 (NLP-First):** Intent-based prompts, no pattern matching
- **P5 (Deterministic Spine):** Prompts are contracts at LLM boundaries
- **P6 (Observable):** Version tracking, usage logging

---

## Module Index

| Directory/Module | Description |
|------------------|-------------|
| `core/__init__.py` | Package exports |
| `core/registry.py` | `PromptRegistry`, `PromptVersion` |
| `core/templates.py` | Template utilities |
| `core/blocks/` | Shared prompt building blocks |
| `core/versions/` | Versioned prompt definitions |
| `*.txt` | Legacy prompt files (perception, intent, planner, etc.) |

---

## Prompt Registry (`core/registry.py`)

### `PromptVersion`

Versioned prompt template.

```python
@dataclass
class PromptVersion:
    name: str
    version: str
    template: str
    created_at: datetime
    description: str
    constitutional_compliance: str  # Which principles this addresses
```

### `PromptRegistry`

Central registry for all LLM prompts.

```python
class PromptRegistry:
    """
    Central registry for all LLM prompts.
    
    Usage:
        registry = PromptRegistry.get_instance()
        prompt = registry.get("planner.tool_selection", version="1.0")
    """
    
    @classmethod
    def get_instance(cls) -> 'PromptRegistry':
        """Get singleton instance."""
    
    def register(self, prompt_version: PromptVersion) -> None:
        """Register a prompt version."""
    
    def get(
        self,
        name: str,
        version: str = "latest",
        context: Optional[Dict] = None
    ) -> str:
        """Get a prompt by name and version, optionally with context interpolation."""
    
    def list_prompts(self) -> Dict[str, list]:
        """List all registered prompts and their versions."""
```

### `@register_prompt` Decorator

```python
@register_prompt(
    name="planner.tool_selection",
    version="1.0",
    description="Tool selection prompt for planner agent",
    constitutional_compliance="P1, P5"
)
def planner_tool_selection() -> str:
    return """
    You are the Planner agent...
    """
```

---

## Prompt Blocks (`core/blocks/`)

Shared prompt components ensuring consistency across agents.

### `IDENTITY_BLOCK` (`identity_block.py`)

```python
IDENTITY_BLOCK = """You are the Code Analysis Agent - a specialized system for understanding codebases.

CORE PRINCIPLES (in priority order):
1. ACCURACY FIRST: Never hallucinate code. Every claim must be backed by actual source.
2. EVIDENCE-BASED: Cite specific file:line references for all assertions.
3. READ-ONLY: You analyze and explain. You do not modify files or manage tasks.

Your responses must be:
- Verifiable: Claims can be checked against actual code
- Cited: Use format `path/to/file.py:42` for all references
- Honest: If uncertain, say so. If you can't find something, say that."""
```

### `SAFETY_BLOCK` (`safety_block.py`)

```python
SAFETY_BLOCK = """Safety Rules:
- All operations are READ-ONLY - you cannot modify files
- When uncertain, ask for clarification rather than guess
- Never claim code exists without having read it
- Respect context bounds to prevent runaway exploration
- If a query is too broad, suggest narrowing the scope
- Acknowledge when the codebase is too large to fully analyze"""
```

### `STYLE_BLOCK` (`style_block.py`)

Response voice and formatting rules.

### `ROLE_INVARIANTS` (`role_invariants.py`)

Universal constraints for all agents.

### Usage

```python
from mission_system.prompts.core import (
    IDENTITY_BLOCK, STYLE_BLOCK, ROLE_INVARIANTS, SAFETY_BLOCK
)

prompt = f'''
{IDENTITY_BLOCK}

**Your Role:** Planner Agent

{STYLE_BLOCK}

{ROLE_INVARIANTS}

{SAFETY_BLOCK}
'''
```

---

## Template Utilities (`core/templates.py`)

### `intent_based_tool_guidance()`

```python
def intent_based_tool_guidance(tools_description: str) -> str:
    """
    Standard intent-based tool selection guidance.
    
    Constitutional Alignment: P1 (NLP-First)
    - Focuses on intent, not keywords
    - Provides semantic context, not pattern rules
    """
```

### `confidence_based_response()`

```python
def confidence_based_response(action_type: str) -> str:
    """
    Standard confidence-based decision guidance.
    
    Confidence guidelines:
    - 0.9-1.0: Very confident
    - 0.7-0.89: Confident but minor ambiguity
    - 0.5-0.69: Uncertain
    - 0.0-0.49: Very uncertain
    """
```

---

## Prompt Versions (`core/versions/`)

Per-agent prompt definitions (auto-registered on import):

| Module | Agent | Purpose |
|--------|-------|---------|
| `intent.py` | Intent | Intent extraction and clarification |
| `planner.py` | Planner | Plan generation and tool selection |
| `critic.py` | Critic | Response validation |
| `confirmation.py` | Confirmation | User confirmation handling |

---

## Legacy Prompt Files

Located in `prompts/`:

| File | Description |
|------|-------------|
| `perception.txt` | Perception agent prompt |
| `intent.txt` | Intent agent prompt |
| `planner.txt` | Planner agent prompt |
| `critic.txt` | Critic agent prompt |
| `synthesizer.txt` | Synthesizer agent prompt |
| `integration.txt` | Integration agent prompt |

---

## All Exports

```python
from mission_system.prompts.core import (
    PromptRegistry,
    PromptVersion,
    register_prompt,
    IDENTITY_BLOCK,
    STYLE_BLOCK,
    ROLE_INVARIANTS,
    SAFETY_BLOCK,
)
```

---

## Usage Example

```python
from mission_system.prompts.core import (
    PromptRegistry,
    IDENTITY_BLOCK,
    SAFETY_BLOCK,
)

# Get prompt from registry
registry = PromptRegistry.get_instance()
intent_prompt = registry.get("intent.classification", version="1.0", context={
    "user_query": "How does authentication work?",
    "session_context": "Previous discussion about login",
})

# Or build prompt manually with blocks
custom_prompt = f"""
{IDENTITY_BLOCK}

**Your Role:** Custom Agent

{SAFETY_BLOCK}

Now analyze: {{user_query}}
"""
```

---

## Navigation

- [← Back to README](README.md)
- [← Config Documentation](config.md)
- [→ Services Documentation](services.md)
