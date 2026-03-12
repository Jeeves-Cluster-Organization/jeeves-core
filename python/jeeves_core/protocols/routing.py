"""Routing expression builders for kernel pipeline orchestration.

These functions produce serializable dicts that match the Rust RoutingExpr/FieldRef
serde format. The kernel evaluates these expressions during pipeline routing.

Usage:
    from jeeves_core.protocols.routing import eq, neq, not_, always, agent, meta

    RoutingRule(expr=eq("intent", "getting_started"), target="think_tools")
    RoutingRule(expr=not_(eq("completed", True)), target="understand")
    RoutingRule(expr=eq(agent("understand", "topic"), "time"), target="think_tools")
    RoutingRule(expr=always(), target="respond")
"""

from typing import Any, Dict, List, Union

# Type alias for a serializable RoutingExpr dict
RoutingExpr = Dict[str, Any]
# Type alias for a serializable FieldRef dict
FieldRef = Dict[str, str]


# =============================================================================
# FieldRef constructors
# =============================================================================

def current(key: str) -> FieldRef:
    """FieldRef scoping to the current agent's output."""
    return {"scope": "Current", "key": key}


def agent(agent_name: str, key: str) -> FieldRef:
    """FieldRef scoping to a specific agent's output."""
    return {"scope": "Agent", "agent": agent_name, "key": key}


def meta(path: str) -> FieldRef:
    """FieldRef scoping to envelope metadata (dot-notation for nested access)."""
    return {"scope": "Meta", "path": path}


def interrupt(key: str) -> FieldRef:
    """FieldRef scoping to interrupt response fields."""
    return {"scope": "Interrupt", "key": key}


def state(key: str) -> FieldRef:
    """FieldRef scoping to merged accumulator state."""
    return {"scope": "State", "key": key}


# =============================================================================
# Internal: normalize field argument
# =============================================================================

def _field(field_or_key: Union[str, FieldRef]) -> FieldRef:
    """Normalize: str → Current scope, dict → pass through."""
    if isinstance(field_or_key, dict) and "scope" in field_or_key:
        return field_or_key
    return current(field_or_key)


# =============================================================================
# Comparison operators
# =============================================================================

def eq(field_or_key: Union[str, FieldRef], value: Any) -> RoutingExpr:
    """Equality: field == value."""
    return {"op": "Eq", "field": _field(field_or_key), "value": value}


def neq(field_or_key: Union[str, FieldRef], value: Any) -> RoutingExpr:
    """Inequality: field != value (field must exist)."""
    return {"op": "Neq", "field": _field(field_or_key), "value": value}


def gt(field_or_key: Union[str, FieldRef], value: Any) -> RoutingExpr:
    """Greater than (numeric)."""
    return {"op": "Gt", "field": _field(field_or_key), "value": value}


def lt(field_or_key: Union[str, FieldRef], value: Any) -> RoutingExpr:
    """Less than (numeric)."""
    return {"op": "Lt", "field": _field(field_or_key), "value": value}


def gte(field_or_key: Union[str, FieldRef], value: Any) -> RoutingExpr:
    """Greater than or equal (numeric)."""
    return {"op": "Gte", "field": _field(field_or_key), "value": value}


def lte(field_or_key: Union[str, FieldRef], value: Any) -> RoutingExpr:
    """Less than or equal (numeric)."""
    return {"op": "Lte", "field": _field(field_or_key), "value": value}


def contains(field_or_key: Union[str, FieldRef], value: Any) -> RoutingExpr:
    """String contains substring, or array contains element."""
    return {"op": "Contains", "field": _field(field_or_key), "value": value}


# =============================================================================
# Existence operators
# =============================================================================

def exists(field_or_key: Union[str, FieldRef]) -> RoutingExpr:
    """Field exists and is not null."""
    return {"op": "Exists", "field": _field(field_or_key)}


def not_exists(field_or_key: Union[str, FieldRef]) -> RoutingExpr:
    """Field is absent or null."""
    return {"op": "NotExists", "field": _field(field_or_key)}


# =============================================================================
# Logical combinators
# =============================================================================

def and_(*exprs: RoutingExpr) -> RoutingExpr:
    """All sub-expressions must be true."""
    return {"op": "And", "exprs": list(exprs)}


def or_(*exprs: RoutingExpr) -> RoutingExpr:
    """At least one sub-expression must be true."""
    return {"op": "Or", "exprs": list(exprs)}


def not_(expr: RoutingExpr) -> RoutingExpr:
    """Negate a sub-expression."""
    return {"op": "Not", "expr": expr}


# =============================================================================
# Unconditional
# =============================================================================

def always() -> RoutingExpr:
    """Unconditional match (always true)."""
    return {"op": "Always"}


# =============================================================================
# Structural validation
# =============================================================================

_COMPARISON_OPS = {"Eq", "Neq", "Gt", "Lt", "Gte", "Lte", "Contains"}
_EXISTENCE_OPS = {"Exists", "NotExists"}
_VALID_OPS = _COMPARISON_OPS | _EXISTENCE_OPS | {"And", "Or", "Not", "Always"}

_SCOPE_REQUIRED_KEYS: Dict[str, List[str]] = {
    "Current": ["key"],
    "Agent": ["agent", "key"],
    "Meta": ["path"],
    "Interrupt": ["key"],
    "State": ["key"],
}


def _validate_fieldref(errors: List[str], field_value: Any, context: str) -> None:
    """Validate a FieldRef dict has the correct structure."""
    if not isinstance(field_value, dict):
        errors.append(f"{context}: 'field' must be a dict (FieldRef), got {type(field_value).__name__}")
        return
    scope = field_value.get("scope")
    if scope is None:
        errors.append(f"{context}: FieldRef missing 'scope' key")
        return
    required = _SCOPE_REQUIRED_KEYS.get(scope)
    if required is None:
        errors.append(f"{context}: unknown FieldRef scope '{scope}'")
        return
    for key in required:
        if key not in field_value:
            errors.append(f"{context}: FieldRef scope '{scope}' requires '{key}'")


def validate_expr(expr: Dict[str, Any]) -> List[str]:
    """Validate a RoutingExpr dict structurally.

    Returns a list of error strings (empty = valid).
    """
    errors: List[str] = []
    if not isinstance(expr, dict):
        errors.append(f"RoutingExpr must be a dict, got {type(expr).__name__}")
        return errors

    op = expr.get("op")
    if op is None:
        errors.append("RoutingExpr missing 'op' key")
        return errors
    if op not in _VALID_OPS:
        errors.append(f"Unknown RoutingExpr op '{op}'")
        return errors

    if op in _COMPARISON_OPS:
        if "field" not in expr:
            errors.append(f"Op '{op}' requires 'field'")
        else:
            _validate_fieldref(errors, expr["field"], f"Op '{op}'")
        if "value" not in expr:
            errors.append(f"Op '{op}' requires 'value'")
    elif op in _EXISTENCE_OPS:
        if "field" not in expr:
            errors.append(f"Op '{op}' requires 'field'")
        else:
            _validate_fieldref(errors, expr["field"], f"Op '{op}'")
    elif op == "Not":
        sub = expr.get("expr")
        if sub is None:
            errors.append("Op 'Not' requires 'expr'")
        else:
            errors.extend(validate_expr(sub))
    elif op in ("And", "Or"):
        subs = expr.get("exprs")
        if subs is None:
            errors.append(f"Op '{op}' requires 'exprs'")
        elif not isinstance(subs, list):
            errors.append(f"Op '{op}': 'exprs' must be a list")
        else:
            for i, sub in enumerate(subs):
                errors.extend(validate_expr(sub))
    # "Always" — no additional fields required

    return errors
