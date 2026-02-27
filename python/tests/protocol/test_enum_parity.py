"""Cross-language enum parity tests.

Parses Rust enum definitions from source files and asserts Python enums
(auto-generated in _generated.py) match the Rust serde output exactly.
Catches enum drift at CI time.
"""

import re
from pathlib import Path

import pytest

JEEVES_CORE_ROOT = Path(__file__).resolve().parent.parent.parent.parent

# (Rust source file, enum name, Python enum name, serde rename strategy)
ENUM_SOURCES = [
    ("src/envelope/enums.rs", "TerminalReason", "TerminalReason", "SCREAMING_SNAKE_CASE"),
    ("src/envelope/enums.rs", "InterruptKind", "InterruptKind", "snake_case"),
    ("src/envelope/enums.rs", "RiskSemantic", "RiskSemantic", "snake_case"),
    ("src/envelope/enums.rs", "RiskSeverity", "RiskSeverity", "snake_case"),
    ("src/envelope/enums.rs", "ToolCategory", "ToolCategory", "snake_case"),
    ("src/envelope/enums.rs", "HealthStatus", "HealthStatus", "snake_case"),
    ("src/envelope/enums.rs", "LoopVerdict", "LoopVerdict", "snake_case"),
    ("src/envelope/enums.rs", "RiskApproval", "RiskApproval", "snake_case"),
    ("src/envelope/enums.rs", "ToolAccess", "ToolAccess", "snake_case"),
    ("src/envelope/enums.rs", "OperationStatus", "OperationStatus", "snake_case"),
    ("src/kernel/interrupts.rs", "InterruptStatus", "InterruptStatus", "lowercase"),
]

# No extra Python variants allowed — generated enums must match Rust exactly
ALLOWED_PYTHON_EXTRAS: set[str] = set()


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
    elif strategy == "lowercase":
        return variant.lower()
    return variant


@pytest.mark.parametrize(
    "rust_file,rust_name,py_name,strategy",
    ENUM_SOURCES,
    ids=[e[1] for e in ENUM_SOURCES],
)
def test_enum_parity(rust_file: str, rust_name: str, py_name: str, strategy: str):
    """Assert Python enum has all Rust variants (wire-format values)."""
    # Import Python enum dynamically
    import jeeves_core.protocols.types as types_mod
    py_enum = getattr(types_mod, py_name)

    # Parse Rust source
    rust_path = JEEVES_CORE_ROOT / rust_file
    rust_source = rust_path.read_text(encoding="utf-8")
    rust_variants = _parse_rust_enum_variants(rust_source, rust_name)
    assert rust_variants, f"No variants found for Rust enum {rust_name} in {rust_file}"

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
