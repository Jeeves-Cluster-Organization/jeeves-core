"""IPC response contract parity tests.

Parses Rust serde_json::json!({...}) field names from serialization helpers
and asserts Python _dict_to_* methods read exactly the same keys.
Catches field renames/additions/removals at CI time.
"""

import re
from pathlib import Path

HANDLER_DIR = Path(__file__).resolve().parent.parent.parent.parent / "src" / "ipc" / "handlers"
KERNEL_CLIENT = Path(__file__).resolve().parent.parent.parent / "jeeves_airframe" / "kernel_client.py"


# =============================================================================
# Rust field extraction helpers
# =============================================================================

def _extract_json_block(source: str, fn_name: str) -> str:
    """Extract the serde_json::json!({...}) block from a named function.

    Finds `fn fn_name` then the first `serde_json::json!({` and returns
    everything up to the matching `})`.
    """
    # Find the function
    fn_pattern = rf'fn\s+{re.escape(fn_name)}\b'
    fn_match = re.search(fn_pattern, source)
    assert fn_match, f"Function '{fn_name}' not found in Rust source"

    # Find the json!({ block after the function start
    search_start = fn_match.start()
    json_start = source.find('serde_json::json!({', search_start)
    assert json_start != -1, f"No serde_json::json!({{ found in '{fn_name}'"

    # Walk forward to find the matching close: })
    brace_depth = 0
    i = json_start + len('serde_json::json!(')
    while i < len(source):
        ch = source[i]
        if ch == '{':
            brace_depth += 1
        elif ch == '}':
            brace_depth -= 1
            if brace_depth == 0:
                return source[json_start:i + 2]  # include })
        i += 1
    raise AssertionError(f"Unmatched braces in '{fn_name}' json block")


def _extract_top_level_fields(json_block: str) -> set[str]:
    """Extract top-level quoted field names from a serde_json::json! block.

    Matches lines like:  "field_name": ...
    Only top-level (depth == 1 inside the outer braces).
    """
    fields = set()
    brace_depth = 0
    for line in json_block.splitlines():
        stripped = line.strip()

        # Check for field name at the START of the line, before updating depth
        depth_at_start = brace_depth
        if depth_at_start == 1:
            m = re.match(r'"(\w+)"\s*:', stripped)
            if m:
                fields.add(m.group(1))

        # Track brace depth after extracting fields
        for ch in stripped:
            if ch == '{':
                brace_depth += 1
            elif ch == '}':
                brace_depth -= 1

    return fields


def _extract_nested_fields(json_block: str, parent_field: str) -> set[str]:
    """Extract fields nested under a parent field in a serde_json::json! block.

    For blocks like:
        "usage": {
            "llm_calls": ...,
            "tool_calls": ...,
        },
    Returns {"llm_calls", "tool_calls"} when parent_field="usage".
    """
    fields = set()
    in_parent = False
    depth = 0

    for line in json_block.splitlines():
        stripped = line.strip()

        if not in_parent:
            # Look for the parent field opening
            if re.match(rf'"{re.escape(parent_field)}"\s*:\s*\{{', stripped):
                in_parent = True
                depth = 1
                continue
        else:
            for ch in stripped:
                if ch == '{':
                    depth += 1
                elif ch == '}':
                    depth -= 1
                    if depth == 0:
                        in_parent = False
                        break

            if in_parent and depth == 1:
                m = re.match(r'"(\w+)"\s*:', stripped)
                if m:
                    fields.add(m.group(1))

    return fields


# =============================================================================
# Python field extraction helpers
# =============================================================================

def _extract_python_gets(source: str, method_name: str) -> set[str]:
    """Extract .get("field") keys from a Python _dict_to_* method.

    Returns top-level keys that are accessed via d.get("...").
    """
    # Find the method
    fn_pattern = rf'def\s+{re.escape(method_name)}\b'
    fn_match = re.search(fn_pattern, source)
    assert fn_match, f"Method '{method_name}' not found in Python source"

    # Find the end of the method (next def at same or lower indent, or EOF)
    method_start = fn_match.start()
    next_def = re.search(r'\n    def ', source[method_start + 1:])
    if next_def:
        method_end = method_start + 1 + next_def.start()
    else:
        method_end = len(source)

    method_body = source[method_start:method_end]

    # Extract d.get("field") — top-level dict access
    # Exclude usage.get, procs.get, svcs.get, etc. (nested dict access)
    fields = set()
    for m in re.finditer(r'\bd\.get\("(\w+)"', method_body):
        fields.add(m.group(1))
    return fields


def _extract_python_nested_gets(source: str, method_name: str, var_name: str) -> set[str]:
    """Extract var_name.get("field") keys from a Python method."""
    fn_pattern = rf'def\s+{re.escape(method_name)}\b'
    fn_match = re.search(fn_pattern, source)
    assert fn_match, f"Method '{method_name}' not found in Python source"

    method_start = fn_match.start()
    next_def = re.search(r'\n    def ', source[method_start + 1:])
    if next_def:
        method_end = method_start + 1 + next_def.start()
    else:
        method_end = len(source)

    method_body = source[method_start:method_end]

    fields = set()
    for m in re.finditer(rf'\b{re.escape(var_name)}\.get\("(\w+)"', method_body):
        fields.add(m.group(1))
    return fields


# =============================================================================
# Tests: orchestration.rs ↔ kernel_client.py
# =============================================================================

def test_instruction_fields_match():
    """Rust instruction_to_value() fields match Python _dict_to_instruction() reads."""
    rust_source = (HANDLER_DIR / "orchestration.rs").read_text(encoding="utf-8")
    py_source = KERNEL_CLIENT.read_text(encoding="utf-8")

    json_block = _extract_json_block(rust_source, "instruction_to_value")
    rust_fields = _extract_top_level_fields(json_block)

    py_fields = _extract_python_gets(py_source, "_dict_to_instruction")

    # Python reads "agent_config" which Rust doesn't send — that's intentional
    # (pipeline_worker may inject it). Only check Rust→Python direction.
    missing_in_python = rust_fields - py_fields
    assert not missing_in_python, (
        f"Rust instruction_to_value() sends {sorted(missing_in_python)} "
        f"but Python _dict_to_instruction() doesn't read them.\n"
        f"Rust sends: {sorted(rust_fields)}\n"
        f"Python reads: {sorted(py_fields)}"
    )


def test_session_state_fields_match():
    """Rust session_state_to_value() fields match Python _dict_to_session_state() reads."""
    rust_source = (HANDLER_DIR / "orchestration.rs").read_text(encoding="utf-8")
    py_source = KERNEL_CLIENT.read_text(encoding="utf-8")

    json_block = _extract_json_block(rust_source, "session_state_to_value")
    rust_fields = _extract_top_level_fields(json_block)

    py_fields = _extract_python_gets(py_source, "_dict_to_session_state")

    # Bidirectional: Python should read exactly what Rust sends
    missing_in_python = rust_fields - py_fields
    extra_in_python = py_fields - rust_fields

    assert not missing_in_python, (
        f"Rust session_state_to_value() sends {sorted(missing_in_python)} "
        f"but Python _dict_to_session_state() doesn't read them.\n"
        f"Rust: {sorted(rust_fields)}, Python: {sorted(py_fields)}"
    )
    assert not extra_in_python, (
        f"Python _dict_to_session_state() reads {sorted(extra_in_python)} "
        f"but Rust session_state_to_value() doesn't send them.\n"
        f"Rust: {sorted(rust_fields)}, Python: {sorted(py_fields)}"
    )


# =============================================================================
# Tests: kernel.rs ↔ kernel_client.py
# =============================================================================

def test_pcb_fields_match():
    """Rust pcb_to_value() fields match Python _dict_to_process_info() reads."""
    rust_source = (HANDLER_DIR / "kernel.rs").read_text(encoding="utf-8")
    py_source = KERNEL_CLIENT.read_text(encoding="utf-8")

    json_block = _extract_json_block(rust_source, "pcb_to_value")
    rust_top_fields = _extract_top_level_fields(json_block)
    rust_usage_fields = _extract_nested_fields(json_block, "usage")

    py_top_fields = _extract_python_gets(py_source, "_dict_to_process_info")
    py_usage_fields = _extract_python_nested_gets(py_source, "_dict_to_process_info", "usage")

    # Check top-level fields (Python reads "usage" as a nested dict)
    missing_top = rust_top_fields - py_top_fields
    assert not missing_top, (
        f"Rust pcb_to_value() sends top-level {sorted(missing_top)} "
        f"but Python _dict_to_process_info() doesn't read them.\n"
        f"Rust: {sorted(rust_top_fields)}, Python: {sorted(py_top_fields)}"
    )

    # Check usage sub-fields
    missing_usage = rust_usage_fields - py_usage_fields
    assert not missing_usage, (
        f"Rust pcb_to_value() usage sends {sorted(missing_usage)} "
        f"but Python doesn't read them.\n"
        f"Rust usage: {sorted(rust_usage_fields)}, Python usage: {sorted(py_usage_fields)}"
    )


def test_quota_fields_match():
    """Rust quota_to_value() fields match Python _dict_to_quota_defaults() reads."""
    rust_source = (HANDLER_DIR / "kernel.rs").read_text(encoding="utf-8")
    py_source = KERNEL_CLIENT.read_text(encoding="utf-8")

    json_block = _extract_json_block(rust_source, "quota_to_value")
    rust_fields = _extract_top_level_fields(json_block)

    py_fields = _extract_python_gets(py_source, "_dict_to_quota_defaults")

    # Bidirectional
    missing_in_python = rust_fields - py_fields
    extra_in_python = py_fields - rust_fields

    assert not missing_in_python, (
        f"Rust quota_to_value() sends {sorted(missing_in_python)} "
        f"but Python _dict_to_quota_defaults() doesn't read them.\n"
        f"Rust: {sorted(rust_fields)}, Python: {sorted(py_fields)}"
    )
    assert not extra_in_python, (
        f"Python _dict_to_quota_defaults() reads {sorted(extra_in_python)} "
        f"but Rust quota_to_value() doesn't send them.\n"
        f"Rust: {sorted(rust_fields)}, Python: {sorted(py_fields)}"
    )


def test_system_status_fields_match():
    """Rust GetSystemStatus response matches Python get_system_status() reads."""
    rust_source = (HANDLER_DIR / "kernel.rs").read_text(encoding="utf-8")
    py_source = KERNEL_CLIENT.read_text(encoding="utf-8")

    # Find the GetSystemStatus handler's json block
    handler_start = rust_source.find('"GetSystemStatus"')
    assert handler_start != -1, "GetSystemStatus handler not found in kernel.rs"

    # Find the serde_json::json!({ after GetSystemStatus
    json_start = rust_source.find('serde_json::json!({', handler_start)
    assert json_start != -1, "No json block in GetSystemStatus handler"

    # Extract the block (walk braces)
    brace_depth = 0
    i = json_start + len('serde_json::json!(')
    while i < len(rust_source):
        ch = rust_source[i]
        if ch == '{':
            brace_depth += 1
        elif ch == '}':
            brace_depth -= 1
            if brace_depth == 0:
                json_block = rust_source[json_start:i + 2]
                break
        i += 1

    # Top-level Rust sections: processes, services, orchestration, commbus
    rust_top = _extract_top_level_fields(json_block)
    assert rust_top == {"processes", "services", "orchestration", "commbus"}, (
        f"Unexpected top-level keys in GetSystemStatus: {sorted(rust_top)}"
    )

    # Python reads: response.get("processes"), .get("services"), etc.
    py_method_body = py_source[py_source.find("async def get_system_status"):]
    py_method_end = py_method_body.find("\n    async def ")
    if py_method_end != -1:
        py_method_body = py_method_body[:py_method_end]

    # Check Python reads all top-level sections
    py_top_keys = set(re.findall(r'response\.get\("(\w+)"', py_method_body))
    missing_top = rust_top - py_top_keys
    assert not missing_top, (
        f"Rust GetSystemStatus sends sections {sorted(missing_top)} "
        f"that Python get_system_status() doesn't read."
    )

    # Check nested fields for each section
    for section, py_var in [
        ("processes", "procs"),
        ("services", "svcs"),
        ("orchestration", "orch"),
        ("commbus", "cb"),
    ]:
        rust_nested = _extract_nested_fields(json_block, section)
        py_nested = set(re.findall(
            rf'{re.escape(py_var)}\.get\("(\w+)"', py_method_body
        ))
        missing = rust_nested - py_nested
        assert not missing, (
            f"Rust GetSystemStatus.{section} sends {sorted(missing)} "
            f"but Python doesn't read them.\n"
            f"Rust: {sorted(rust_nested)}, Python: {sorted(py_nested)}"
        )
