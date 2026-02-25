"""Cross-language enum parity tests.

Parses Rust enum definitions from src/envelope/enums.rs and asserts
Python enums in jeeves_infra/protocols/types.py have matching variants.
Catches enum drift at CI time without code generation.
"""

import re
from pathlib import Path

import pytest

RUST_ENUMS_FILE = Path(__file__).resolve().parent.parent.parent.parent / "src" / "envelope" / "enums.rs"

# Rust enum name -> (Python enum name, serde rename strategy)
ENUM_MAP = {
    "TerminalReason": ("TerminalReason", "SCREAMING_SNAKE_CASE"),
    "InterruptKind": ("InterruptKind", "snake_case"),
    "RiskSemantic": ("RiskSemantic", "snake_case"),
    "RiskSeverity": ("RiskSeverity", "snake_case"),
    "ToolCategory": ("ToolCategory", "snake_case"),
    "HealthStatus": ("HealthStatus", "snake_case"),
    "LoopVerdict": ("LoopVerdict", "snake_case"),
    "RiskApproval": ("RiskApproval", "snake_case"),
    "ToolAccess": ("ToolAccess", "snake_case"),
    "OperationStatus": ("OperationStatus", "snake_case"),
}

# Python may have these extra sentinel variants that Rust doesn't define
ALLOWED_PYTHON_EXTRAS = {"unspecified", "UNSPECIFIED"}


def _parse_rust_enum_variants(source: str, enum_name: str) -> list[str]:
    """Extract PascalCase variant names from a Rust enum definition."""
    pattern = rf"pub enum {enum_name}\s*\{{([^}}]*)\}}"
    match = re.search(pattern, source, re.DOTALL)
    assert match, f"Rust enum {enum_name} not found in enums.rs"
    body = match.group(1)
    variants = []
    for line in body.splitlines():
        line = line.split("//")[0].strip()  # strip comments
        if not line or line.startswith("#"):
            continue
        variant = line.rstrip(",").split("(")[0].strip()
        if variant and variant[0].isupper():
            variants.append(variant)
    return variants


def _pascal_to_wire(variant: str, strategy: str) -> str:
    """Convert Rust PascalCase variant to serde wire format."""
    # Insert underscore before uppercase letters (except the first)
    snake = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", variant)
    if strategy == "SCREAMING_SNAKE_CASE":
        return snake.upper()
    elif strategy == "snake_case":
        return snake.lower()
    return variant


@pytest.mark.parametrize("rust_name", ENUM_MAP.keys())
def test_enum_parity(rust_name: str):
    """Assert Python enum has all Rust variants (wire-format values)."""
    py_name, strategy = ENUM_MAP[rust_name]

    # Import Python enum dynamically
    import jeeves_infra.protocols.types as types_mod
    py_enum = getattr(types_mod, py_name)

    # Parse Rust source
    rust_source = RUST_ENUMS_FILE.read_text(encoding="utf-8")
    rust_variants = _parse_rust_enum_variants(rust_source, rust_name)
    assert rust_variants, f"No variants found for Rust enum {rust_name}"

    # Convert Rust variants to wire format
    rust_wire_values = {_pascal_to_wire(v, strategy) for v in rust_variants}

    # Get Python wire values (excluding allowed extras)
    py_wire_values = {m.value for m in py_enum} - ALLOWED_PYTHON_EXTRAS

    # Python must have ALL Rust variants
    missing_in_python = rust_wire_values - py_wire_values
    assert not missing_in_python, (
        f"Python {py_name} is missing Rust variants: {sorted(missing_in_python)}"
    )

    # Python should not have extra variants beyond allowed sentinels
    extra_in_python = py_wire_values - rust_wire_values
    assert not extra_in_python, (
        f"Python {py_name} has variants not in Rust: {sorted(extra_in_python)}"
    )
