"""Tests for RoutingExpr structural validation (Phase 1)."""

import pytest
from jeeves_core.protocols.routing import (
    validate_expr, eq, neq, always, not_, and_, or_, exists, not_exists,
    agent, meta, state, contains,
)
from jeeves_core.protocols.types import RoutingRule


class TestValidateExpr:
    """validate_expr() structural checks."""

    def test_valid_eq(self):
        assert validate_expr(eq("key", "value")) == []

    def test_valid_always(self):
        assert validate_expr(always()) == []

    def test_valid_not(self):
        assert validate_expr(not_(eq("done", True))) == []

    def test_valid_and_or(self):
        assert validate_expr(and_(eq("a", 1), neq("b", 2))) == []
        assert validate_expr(or_(exists("x"), not_exists("y"))) == []

    def test_valid_agent_scope(self):
        assert validate_expr(eq(agent("understand", "topic"), "time")) == []

    def test_valid_meta_scope(self):
        assert validate_expr(eq(meta("user.role"), "admin")) == []

    def test_valid_state_scope(self):
        assert validate_expr(eq(state("count"), 3)) == []

    def test_valid_contains(self):
        assert validate_expr(contains("tags", "important")) == []

    def test_unknown_op(self):
        errors = validate_expr({"op": "Eqq"})
        assert len(errors) == 1
        assert "Unknown RoutingExpr op 'Eqq'" in errors[0]

    def test_missing_op(self):
        errors = validate_expr({"field": {"scope": "Current", "key": "x"}, "value": 1})
        assert any("missing 'op'" in e for e in errors)

    def test_missing_field(self):
        errors = validate_expr({"op": "Eq", "value": 1})
        assert any("requires 'field'" in e for e in errors)

    def test_missing_value(self):
        errors = validate_expr({"op": "Eq", "field": {"scope": "Current", "key": "x"}})
        assert any("requires 'value'" in e for e in errors)

    def test_field_is_string_not_dict(self):
        errors = validate_expr({"op": "Eq", "field": "just_a_string", "value": 1})
        assert any("must be a dict" in e for e in errors)

    def test_invalid_scope(self):
        errors = validate_expr({"op": "Eq", "field": {"scope": "Unknown"}, "value": 1})
        assert any("unknown FieldRef scope" in e for e in errors)

    def test_scope_missing_required_key(self):
        # Agent scope requires 'agent' and 'key'
        errors = validate_expr({"op": "Eq", "field": {"scope": "Agent", "key": "x"}, "value": 1})
        assert any("requires 'agent'" in e for e in errors)

    def test_recursive_and_validation(self):
        expr = {"op": "And", "exprs": [{"op": "Eqq"}]}
        errors = validate_expr(expr)
        assert any("Unknown" in e for e in errors)

    def test_recursive_not_validation(self):
        expr = {"op": "Not", "expr": {"op": "Eq"}}
        errors = validate_expr(expr)
        assert len(errors) > 0  # Missing field and value

    def test_not_a_dict(self):
        errors = validate_expr("not_a_dict")
        assert any("must be a dict" in e for e in errors)

    def test_and_missing_exprs(self):
        errors = validate_expr({"op": "And"})
        assert any("requires 'exprs'" in e for e in errors)

    def test_not_missing_expr(self):
        errors = validate_expr({"op": "Not"})
        assert any("requires 'expr'" in e for e in errors)


class TestRoutingRulePostInit:
    """RoutingRule.__post_init__ eagerly validates expr."""

    def test_valid_rule_passes(self):
        rule = RoutingRule(expr=eq("key", "value"), target="next")
        assert rule.target == "next"

    def test_invalid_rule_raises(self):
        with pytest.raises(ValueError, match="RoutingRule.*target='bad'"):
            RoutingRule(expr={"op": "Eqq"}, target="bad")

    def test_field_string_raises(self):
        with pytest.raises(ValueError, match="must be a dict"):
            RoutingRule(expr={"op": "Eq", "field": "raw", "value": 1}, target="x")
