"""Direct unit tests for routing_eval.py — all 13 operators, 5 FieldRef scopes, edge cases.

Tests evaluate_expr() and _resolve_field() directly (not via TestPipeline/MockKernelClient).
"""

import pytest
from jeeves_core.testing.routing_eval import evaluate_expr, _resolve_field, evaluate_routing


# =============================================================================
# Helpers
# =============================================================================

def _field(key, scope="Current", **kwargs):
    return {"scope": scope, "key": key, **kwargs}

def _agent_field(agent, key):
    return {"scope": "Agent", "agent": agent, "key": key}

def _meta_field(path):
    return {"scope": "Meta", "path": path}

def _interrupt_field(key):
    return {"scope": "Interrupt", "key": key}

def _state_field(key):
    return {"scope": "State", "key": key}

def _expr(op, field=None, value=None, **kwargs):
    d = {"op": op}
    if field is not None:
        d["field"] = field
    if value is not None:
        d["value"] = value
    d.update(kwargs)
    return d


# =============================================================================
# 1. Operator tests — each op: match + non-match
# =============================================================================

class TestEqOperator:
    def test_match(self):
        assert evaluate_expr(
            _expr("Eq", _field("x"), "hello"),
            {"agent": {"x": "hello"}}, {},
        ) is True

    def test_no_match(self):
        assert evaluate_expr(
            _expr("Eq", _field("x"), "hello"),
            {"agent": {"x": "world"}}, {},
        ) is False

    def test_missing_field(self):
        assert evaluate_expr(
            _expr("Eq", _field("x"), "hello"), {}, {},
        ) is False


class TestNeqOperator:
    def test_match(self):
        assert evaluate_expr(
            _expr("Neq", _field("x"), "hello"),
            {"agent": {"x": "world"}}, {},
        ) is True

    def test_no_match(self):
        assert evaluate_expr(
            _expr("Neq", _field("x"), "hello"),
            {"agent": {"x": "hello"}}, {},
        ) is False

    def test_none_returns_false(self):
        """Neq with None resolved → False (mirrors Rust: None != value is false)."""
        assert evaluate_expr(
            _expr("Neq", _field("missing"), "hello"), {}, {},
        ) is False


class TestGtOperator:
    def test_match(self):
        assert evaluate_expr(
            _expr("Gt", _field("score"), 5),
            {"a": {"score": 10}}, {},
        ) is True

    def test_no_match(self):
        assert evaluate_expr(
            _expr("Gt", _field("score"), 10),
            {"a": {"score": 5}}, {},
        ) is False

    def test_equal_returns_false(self):
        assert evaluate_expr(
            _expr("Gt", _field("score"), 5),
            {"a": {"score": 5}}, {},
        ) is False

    def test_none_returns_false(self):
        assert evaluate_expr(
            _expr("Gt", _field("missing"), 5), {}, {},
        ) is False


class TestLtOperator:
    def test_match(self):
        assert evaluate_expr(
            _expr("Lt", _field("score"), 10),
            {"a": {"score": 5}}, {},
        ) is True

    def test_no_match(self):
        assert evaluate_expr(
            _expr("Lt", _field("score"), 5),
            {"a": {"score": 10}}, {},
        ) is False

    def test_equal_returns_false(self):
        assert evaluate_expr(
            _expr("Lt", _field("score"), 5),
            {"a": {"score": 5}}, {},
        ) is False


class TestGteOperator:
    def test_match_greater(self):
        assert evaluate_expr(
            _expr("Gte", _field("score"), 5),
            {"a": {"score": 10}}, {},
        ) is True

    def test_match_equal(self):
        assert evaluate_expr(
            _expr("Gte", _field("score"), 5),
            {"a": {"score": 5}}, {},
        ) is True

    def test_no_match(self):
        assert evaluate_expr(
            _expr("Gte", _field("score"), 10),
            {"a": {"score": 5}}, {},
        ) is False


class TestLteOperator:
    def test_match_less(self):
        assert evaluate_expr(
            _expr("Lte", _field("score"), 10),
            {"a": {"score": 5}}, {},
        ) is True

    def test_match_equal(self):
        assert evaluate_expr(
            _expr("Lte", _field("score"), 5),
            {"a": {"score": 5}}, {},
        ) is True

    def test_no_match(self):
        assert evaluate_expr(
            _expr("Lte", _field("score"), 5),
            {"a": {"score": 10}}, {},
        ) is False


class TestContainsOperator:
    def test_string_match(self):
        assert evaluate_expr(
            _expr("Contains", _field("text"), "world"),
            {"a": {"text": "hello world"}}, {},
        ) is True

    def test_string_no_match(self):
        assert evaluate_expr(
            _expr("Contains", _field("text"), "xyz"),
            {"a": {"text": "hello world"}}, {},
        ) is False

    def test_list_match(self):
        assert evaluate_expr(
            _expr("Contains", _field("tags"), "python"),
            {"a": {"tags": ["python", "rust"]}}, {},
        ) is True

    def test_list_no_match(self):
        assert evaluate_expr(
            _expr("Contains", _field("tags"), "java"),
            {"a": {"tags": ["python", "rust"]}}, {},
        ) is False

    def test_type_mismatch_returns_false(self):
        assert evaluate_expr(
            _expr("Contains", _field("num"), "x"),
            {"a": {"num": 42}}, {},
        ) is False


class TestExistsOperator:
    def test_match(self):
        assert evaluate_expr(
            _expr("Exists", _field("x")),
            {"a": {"x": "value"}}, {},
        ) is True

    def test_no_match(self):
        assert evaluate_expr(
            _expr("Exists", _field("missing")), {}, {},
        ) is False


class TestNotExistsOperator:
    def test_match(self):
        assert evaluate_expr(
            _expr("NotExists", _field("missing")), {}, {},
        ) is True

    def test_no_match(self):
        assert evaluate_expr(
            _expr("NotExists", _field("x")),
            {"a": {"x": "value"}}, {},
        ) is False


class TestAlwaysOperator:
    def test_returns_true(self):
        assert evaluate_expr({"op": "Always"}, {}, {}) is True

    def test_empty_context(self):
        assert evaluate_expr({"op": "Always"}, {}, {}, None, None) is True


class TestAndOperator:
    def test_all_true(self):
        expr = {"op": "And", "exprs": [
            _expr("Eq", _field("x"), "a"),
            _expr("Gt", _field("y"), 5),
        ]}
        assert evaluate_expr(expr, {"o": {"x": "a", "y": 10}}, {}) is True

    def test_one_false(self):
        expr = {"op": "And", "exprs": [
            _expr("Eq", _field("x"), "a"),
            _expr("Gt", _field("y"), 100),
        ]}
        assert evaluate_expr(expr, {"o": {"x": "a", "y": 10}}, {}) is False

    def test_empty_exprs(self):
        """And with empty list → True (vacuous truth)."""
        assert evaluate_expr({"op": "And", "exprs": []}, {}, {}) is True


class TestOrOperator:
    def test_first_true(self):
        expr = {"op": "Or", "exprs": [
            _expr("Eq", _field("x"), "a"),
            _expr("Eq", _field("x"), "b"),
        ]}
        assert evaluate_expr(expr, {"o": {"x": "a"}}, {}) is True

    def test_second_true(self):
        expr = {"op": "Or", "exprs": [
            _expr("Eq", _field("x"), "a"),
            _expr("Eq", _field("x"), "b"),
        ]}
        assert evaluate_expr(expr, {"o": {"x": "b"}}, {}) is True

    def test_none_true(self):
        expr = {"op": "Or", "exprs": [
            _expr("Eq", _field("x"), "a"),
            _expr("Eq", _field("x"), "b"),
        ]}
        assert evaluate_expr(expr, {"o": {"x": "c"}}, {}) is False

    def test_empty_exprs(self):
        """Or with empty list → False."""
        assert evaluate_expr({"op": "Or", "exprs": []}, {}, {}) is False


class TestNotOperator:
    def test_true_becomes_false(self):
        expr = {"op": "Not", "expr": _expr("Eq", _field("x"), "hello")}
        assert evaluate_expr(expr, {"a": {"x": "hello"}}, {}) is False

    def test_false_becomes_true(self):
        expr = {"op": "Not", "expr": _expr("Eq", _field("x"), "hello")}
        assert evaluate_expr(expr, {"a": {"x": "world"}}, {}) is True


# =============================================================================
# 2. FieldRef scope tests
# =============================================================================

class TestFieldRefScopes:
    def test_current_scope_last_output(self):
        """Current scope resolves from the most recently added output."""
        outputs = {"agent_a": {"x": 1}, "agent_b": {"x": 2}}
        assert _resolve_field({"scope": "Current", "key": "x"}, outputs, {}) == 2

    def test_current_scope_missing(self):
        assert _resolve_field({"scope": "Current", "key": "missing"}, {}, {}) is None

    def test_agent_scope_resolves(self):
        outputs = {"understand": {"intent": "search"}, "other": {"intent": "chat"}}
        assert _resolve_field(_agent_field("understand", "intent"), outputs, {}) == "search"

    def test_agent_scope_missing_agent(self):
        assert _resolve_field(_agent_field("nonexistent", "key"), {}, {}) is None

    def test_agent_scope_missing_key(self):
        assert _resolve_field(
            _agent_field("understand", "missing"),
            {"understand": {"x": 1}}, {},
        ) is None

    def test_meta_scope_simple(self):
        assert _resolve_field(_meta_field("user_type"), {}, {"user_type": "admin"}) == "admin"

    def test_meta_scope_dot_notation(self):
        metadata = {"request": {"headers": {"auth": "bearer_token"}}}
        assert _resolve_field(_meta_field("request.headers.auth"), {}, metadata) == "bearer_token"

    def test_meta_scope_missing_path(self):
        assert _resolve_field(_meta_field("a.b.c"), {}, {"a": {"x": 1}}) is None

    def test_interrupt_scope(self):
        metadata = {"interrupt": {"kind": "HUMAN_REVIEW"}}
        assert _resolve_field(_interrupt_field("kind"), {}, metadata) == "HUMAN_REVIEW"

    def test_interrupt_scope_missing(self):
        assert _resolve_field(_interrupt_field("kind"), {}, {}) is None

    def test_state_scope(self):
        assert _resolve_field(_state_field("counter"), {}, {}, state={"counter": 42}) == 42

    def test_state_scope_missing(self):
        assert _resolve_field(_state_field("missing"), {}, {}, state={}) is None

    def test_state_scope_none(self):
        assert _resolve_field(_state_field("x"), {}, {}, state=None) is None


# =============================================================================
# 3. Edge cases
# =============================================================================

class TestEdgeCases:
    def test_unknown_op_raises(self):
        with pytest.raises(ValueError, match="Unknown routing op"):
            evaluate_expr({"op": "FooBar"}, {}, {})

    def test_no_op_returns_false(self):
        assert evaluate_expr({}, {}, {}) is False

    def test_current_agent_output_takes_priority(self):
        """When current_agent_output is set, Current scope prefers it."""
        expr = _expr("Eq", _field("intent"), "search")
        assert evaluate_expr(
            expr, {"old": {"intent": "chat"}}, {},
            current_agent_output={"intent": "search"},
        ) is True

    def test_nested_and_or(self):
        """And(Or(Eq, Eq), Not(Eq))."""
        expr = {"op": "And", "exprs": [
            {"op": "Or", "exprs": [
                _expr("Eq", _field("x"), "a"),
                _expr("Eq", _field("x"), "b"),
            ]},
            {"op": "Not", "expr": _expr("Eq", _field("y"), "blocked")},
        ]}
        assert evaluate_expr(expr, {"o": {"x": "b", "y": "open"}}, {}) is True

    def test_evaluate_routing_first_match_wins(self):
        rules = [
            {"expr": _expr("Eq", _field("x"), "a"), "target": "first"},
            {"expr": {"op": "Always"}, "target": "second"},
        ]
        assert evaluate_routing(rules, {"o": {"x": "a"}}, {}) == "first"

    def test_evaluate_routing_no_match(self):
        rules = [
            {"expr": _expr("Eq", _field("x"), "a"), "target": "first"},
        ]
        assert evaluate_routing(rules, {"o": {"x": "b"}}, {}) is None

    def test_evaluate_routing_empty_rules(self):
        assert evaluate_routing([], {}, {}) is None

    def test_eq_with_state_scope(self):
        expr = _expr("Eq", _state_field("status"), "active")
        assert evaluate_expr(expr, {}, {}, state={"status": "active"}) is True
        assert evaluate_expr(expr, {}, {}, state={"status": "inactive"}) is False

    def test_exists_with_agent_scope(self):
        expr = _expr("Exists", _agent_field("understand", "intent"))
        assert evaluate_expr(expr, {"understand": {"intent": "search"}}, {}) is True
        assert evaluate_expr(expr, {"understand": {}}, {}) is False
