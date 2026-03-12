"""Pure-Python RoutingExpr evaluator — mirrors Rust kernel evaluate_routing().

Evaluates RoutingExpr dicts against agent outputs and envelope metadata.
Used by MockKernelClient for testing without the Rust kernel.
"""

from typing import Any, Dict, List, Optional

__all__ = ["evaluate_routing", "evaluate_expr"]


def _resolve_field(
    field_ref: Dict[str, str],
    outputs: Dict[str, Dict],
    metadata: Dict,
    state: Optional[Dict[str, Any]] = None,
) -> Any:
    """Resolve a FieldRef to its value from outputs/metadata/state."""
    scope = field_ref.get("scope", "Current")

    if scope == "Current":
        key = field_ref["key"]
        # Current scope: look in the most recently added output
        for output in reversed(list(outputs.values())):
            if isinstance(output, dict) and key in output:
                return output[key]
        return None

    elif scope == "Agent":
        agent_name = field_ref["agent"]
        key = field_ref["key"]
        agent_output = outputs.get(agent_name, {})
        return agent_output.get(key) if isinstance(agent_output, dict) else None

    elif scope == "Meta":
        path = field_ref["path"]
        parts = path.split(".")
        current = metadata
        for part in parts:
            if isinstance(current, dict):
                current = current.get(part)
            else:
                return None
        return current

    elif scope == "Interrupt":
        key = field_ref["key"]
        interrupt = metadata.get("interrupt", {})
        return interrupt.get(key) if isinstance(interrupt, dict) else None

    elif scope == "State":
        key = field_ref.get("key", "")
        return (state or {}).get(key)

    return None


def evaluate_expr(
    expr: Dict[str, Any],
    outputs: Dict[str, Dict],
    metadata: Dict,
    current_agent_output: Optional[Dict] = None,
    state: Optional[Dict[str, Any]] = None,
) -> bool:
    """Evaluate a RoutingExpr dict. Returns True if the expression matches."""
    op = expr.get("op")

    if op == "Always":
        return True

    elif op in ("Eq", "Neq", "Gt", "Lt", "Gte", "Lte", "Contains"):
        field_ref = expr.get("field", {})
        value = expr.get("value")

        # For Current scope, check current agent output first
        if field_ref.get("scope") == "Current" and current_agent_output is not None:
            resolved = current_agent_output.get(field_ref["key"])
        else:
            resolved = _resolve_field(field_ref, outputs, metadata, state)

        if op == "Eq":
            return resolved == value
        elif op == "Neq":
            return resolved is not None and resolved != value
        elif op == "Gt":
            return resolved is not None and resolved > value
        elif op == "Lt":
            return resolved is not None and resolved < value
        elif op == "Gte":
            return resolved is not None and resolved >= value
        elif op == "Lte":
            return resolved is not None and resolved <= value
        elif op == "Contains":
            if isinstance(resolved, str) and isinstance(value, str):
                return value in resolved
            if isinstance(resolved, (list, tuple)):
                return value in resolved
            return False

    elif op == "Exists":
        field_ref = expr.get("field", {})
        if field_ref.get("scope") == "Current" and current_agent_output is not None:
            resolved = current_agent_output.get(field_ref["key"])
        else:
            resolved = _resolve_field(field_ref, outputs, metadata, state)
        return resolved is not None

    elif op == "NotExists":
        field_ref = expr.get("field", {})
        if field_ref.get("scope") == "Current" and current_agent_output is not None:
            resolved = current_agent_output.get(field_ref["key"])
        else:
            resolved = _resolve_field(field_ref, outputs, metadata, state)
        return resolved is None

    elif op == "And":
        return all(
            evaluate_expr(sub, outputs, metadata, current_agent_output, state)
            for sub in expr.get("exprs", [])
        )

    elif op == "Or":
        return any(
            evaluate_expr(sub, outputs, metadata, current_agent_output, state)
            for sub in expr.get("exprs", [])
        )

    elif op == "Not":
        return not evaluate_expr(expr.get("expr", {}), outputs, metadata, current_agent_output, state)

    if op is not None:
        raise ValueError(f"Unknown routing op: {op!r} — Python evaluator may be out of sync with Rust")
    return False


def evaluate_routing(
    routing_rules: List[Dict[str, Any]],
    outputs: Dict[str, Dict],
    metadata: Dict,
    current_agent_output: Optional[Dict] = None,
    state: Optional[Dict[str, Any]] = None,
) -> Optional[str]:
    """Evaluate routing rules (first match wins). Returns target stage or None."""
    for rule in routing_rules:
        expr = rule.get("expr", {})
        target = rule.get("target", "")
        if evaluate_expr(expr, outputs, metadata, current_agent_output, state):
            return target
    return None
