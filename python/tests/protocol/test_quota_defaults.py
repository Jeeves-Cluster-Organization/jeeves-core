"""Quota default value parity tests.

Asserts Python QuotaDefaults (kernel_client.py) match Rust parse_quota()
defaults in src/ipc/handlers/kernel.rs.
"""

import re
from pathlib import Path

RUST_KERNEL_HANDLER = (
    Path(__file__).resolve().parent.parent.parent.parent
    / "src" / "ipc" / "handlers" / "kernel.rs"
)


def _extract_rust_quota_defaults(source: str) -> dict[str, int]:
    """Extract field("name", default) pairs from Rust parse_quota()."""
    # Match: field("field_name", 123)? or field("field_name", 100_000)?
    pairs = re.findall(r'field\(\s*"(\w+)"\s*,\s*(\d[\d_]*)\s*\)', source)
    return {name: int(val.replace("_", "")) for name, val in pairs}


def test_quota_defaults_match():
    """Python QuotaDefaults field values match Rust parse_quota() defaults."""
    from jeeves_infra.kernel_client import QuotaDefaults

    rust_source = RUST_KERNEL_HANDLER.read_text(encoding="utf-8")
    rust_defaults = _extract_rust_quota_defaults(rust_source)
    assert rust_defaults, "No quota defaults found in Rust source"

    py_defaults = QuotaDefaults()

    for field_name, rust_val in rust_defaults.items():
        py_val = getattr(py_defaults, field_name, None)
        assert py_val is not None, (
            f"Python QuotaDefaults missing field: {field_name}"
        )
        assert py_val == rust_val, (
            f"QuotaDefaults.{field_name}: Python={py_val}, Rust={rust_val}"
        )


def test_python_has_no_extra_quota_fields():
    """Python QuotaDefaults should not have fields absent from Rust."""
    from jeeves_infra.kernel_client import QuotaDefaults

    rust_source = RUST_KERNEL_HANDLER.read_text(encoding="utf-8")
    rust_defaults = _extract_rust_quota_defaults(rust_source)
    rust_fields = set(rust_defaults.keys())

    py_defaults = QuotaDefaults()
    py_fields = set(py_defaults.__dataclass_fields__.keys())

    extra = py_fields - rust_fields
    assert not extra, (
        f"Python QuotaDefaults has fields not in Rust: {sorted(extra)}"
    )
