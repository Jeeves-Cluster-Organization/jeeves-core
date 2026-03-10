"""@tool decorator for declarative tool catalog metadata.

Stores catalog metadata on the function object, enabling
CapabilityToolCatalog.from_decorated() to auto-build catalogs.

Usage:
    @tool(description="Create a new task", category="task", risk="write/low")
    async def add_task(user_id: str, title: str, priority: int = 1, *, db=None):
        ...

Parameters are auto-inferred from type hints. Keyword-only params
named db, event_emitter, or llm_provider are skipped (injected deps).
"""

import inspect
from typing import Any, Callable, Dict, List, Optional, get_type_hints


# Keyword-only parameter names that are injected dependencies, not user params
_INJECTED_DEPS = frozenset({"db", "event_emitter", "llm_provider"})

# Map Python types to type strings matching CapabilityToolCatalog conventions
_TYPE_MAP = {
    "str": "string",
    "int": "int",
    "float": "float",
    "bool": "bool",
    "list": "list",
    "dict": "dict",
    "Any": "any",
}


def _resolve_type_name(annotation) -> str:
    """Convert a type annotation to a catalog type string."""
    if annotation is inspect.Parameter.empty:
        return "any"

    # Handle string annotations
    name = getattr(annotation, "__name__", None)
    if name and name in _TYPE_MAP:
        return _TYPE_MAP[name]

    # Handle typing generics (Optional[X], List[X], etc.)
    origin = getattr(annotation, "__origin__", None)
    if origin is not None:
        origin_name = getattr(origin, "__name__", str(origin))
        if origin_name in _TYPE_MAP:
            return _TYPE_MAP[origin_name]
        # typing.Union with None = Optional
        if str(origin) == "typing.Union":
            args = getattr(annotation, "__args__", ())
            non_none = [a for a in args if a is not type(None)]
            if len(non_none) == 1:
                return _resolve_type_name(non_none[0])

    # Fallback
    return _TYPE_MAP.get(str(annotation), "any")


def _infer_parameters(func: Callable) -> Dict[str, str]:
    """Infer parameter types from function signature.

    Skips:
    - 'self' parameter
    - keyword-only parameters that are injected deps (db, event_emitter, llm_provider)
    - **kwargs
    - *args

    Optional params (those with defaults) get '?' suffix.
    """
    sig = inspect.signature(func)
    try:
        hints = get_type_hints(func)
    except Exception:
        hints = {}

    params = {}
    for name, param in sig.parameters.items():
        # Skip self, *args, **kwargs
        if name == "self":
            continue
        if param.kind in (param.VAR_POSITIONAL, param.VAR_KEYWORD):
            continue
        # Skip injected dependencies (keyword-only with specific names)
        if param.kind == param.KEYWORD_ONLY and name in _INJECTED_DEPS:
            continue
        # Skip return annotation
        if name == "return":
            continue

        type_str = _resolve_type_name(hints.get(name, param.annotation))

        # Mark optional if has default value (not keyword-only deps)
        has_default = param.default is not inspect.Parameter.empty
        if has_default:
            type_str += "?"

        params[name] = type_str

    return params


def tool(description: str, category: str, risk: str) -> Callable:
    """Decorator storing catalog metadata on the function.

    Args:
        description: Human-readable tool description for LLM prompts.
        category: Tool category (e.g., "task", "journal", "loop", "kv").
        risk: Risk in "semantic/severity" format (e.g., "write/low", "read_only/low").

    Returns:
        Decorator that adds _tool_meta attribute to the function.
    """
    parts = risk.split("/", 1)
    if len(parts) != 2:
        raise ValueError(
            f"risk must be 'semantic/severity' format, got: {risk!r}. "
            f"Examples: 'read_only/low', 'write/low', 'write/high', 'delete/high'"
        )
    risk_semantic, risk_severity = parts

    def decorator(func: Callable) -> Callable:
        func._tool_meta = {
            "tool_id": func.__name__,
            "description": description,
            "parameters": _infer_parameters(func),
            "category": category,
            "risk_semantic": risk_semantic,
            "risk_severity": risk_severity,
        }
        return func

    return decorator


__all__ = ["tool"]
