"""IPC response contract parity tests.

Parses Rust DTO struct fields (derive(Serialize)) and serde_json::json!({...})
blocks from serialization helpers and asserts Python _dict_to_* methods read
exactly the same keys.
Catches field renames/additions/removals at CI time.
"""

import re
from pathlib import Path

HANDLER_DIR = Path(__file__).resolve().parent.parent.parent.parent / "src" / "ipc" / "handlers"
KERNEL_CLIENT = Path(__file__).resolve().parent.parent.parent / "jeeves_core" / "kernel_client.py"


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


def _extract_dto_struct_fields(source: str, struct_name: str) -> set[str]:
    """Extract field names from a Rust #[derive(Serialize)] DTO struct.

    Parses struct definitions like:
        #[derive(serde::Serialize)]
        struct InstructionResponse<'a> {
            kind: InstructionKind,
            #[serde(skip_serializing_if = "Option::is_none")]
            envelope: Option<Value>,
        }
    Returns {"kind", "envelope"}.
    """
    # Find the struct definition
    struct_pattern = rf'struct\s+{re.escape(struct_name)}\b[^{{]*\{{'
    struct_match = re.search(struct_pattern, source)
    assert struct_match, f"Struct '{struct_name}' not found in Rust source"

    # Walk braces to find the closing }
    start = struct_match.end() - 1  # points at the opening {
    brace_depth = 0
    i = start
    while i < len(source):
        ch = source[i]
        if ch == '{':
            brace_depth += 1
        elif ch == '}':
            brace_depth -= 1
            if brace_depth == 0:
                break
        i += 1

    struct_body = source[start + 1:i]

    # Extract field names: lines like `    field_name: Type,` or `    pub field_name: Type,`
    # Skip lines that are attributes (#[...]) or empty
    fields = set()
    for line in struct_body.splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith('#') or stripped.startswith('//'):
            continue
        m = re.match(r'(?:pub\s+)?(\w+)\s*:', stripped)
        if m:
            fields.add(m.group(1))

    return fields


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
    """Extract field keys from a Python _dict_to_* method.

    Returns top-level keys accessed via d.get("...") or d["..."].
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

    # Extract d.get("field") and d["field"] — top-level dict access
    # Exclude usage.get, procs.get, svcs.get, etc. (nested dict access)
    fields = set()
    for m in re.finditer(r'\bd\.get\("(\w+)"', method_body):
        fields.add(m.group(1))
    for m in re.finditer(r'\bd\["(\w+)"\]', method_body):
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
    """Rust InstructionResponse DTO fields match Python _dict_to_instruction() reads."""
    rust_source = (HANDLER_DIR / "orchestration.rs").read_text(encoding="utf-8")
    py_source = KERNEL_CLIENT.read_text(encoding="utf-8")

    rust_fields = _extract_dto_struct_fields(rust_source, "InstructionResponse")

    py_fields = _extract_python_gets(py_source, "_dict_to_instruction")

    # Python reads "agent_config" which Rust doesn't send — that's intentional
    # (pipeline_worker may inject it). Only check Rust→Python direction.
    missing_in_python = rust_fields - py_fields
    assert not missing_in_python, (
        f"Rust InstructionResponse DTO sends {sorted(missing_in_python)} "
        f"but Python _dict_to_instruction() doesn't read them.\n"
        f"Rust sends: {sorted(rust_fields)}\n"
        f"Python reads: {sorted(py_fields)}"
    )


def test_session_state_fields_match():
    """Rust SessionStateResponse DTO fields match Python _dict_to_session_state() reads."""
    rust_source = (HANDLER_DIR / "orchestration.rs").read_text(encoding="utf-8")
    py_source = KERNEL_CLIENT.read_text(encoding="utf-8")

    rust_fields = _extract_dto_struct_fields(rust_source, "SessionStateResponse")

    py_fields = _extract_python_gets(py_source, "_dict_to_session_state")

    # Bidirectional: Python should read exactly what Rust sends
    missing_in_python = rust_fields - py_fields
    extra_in_python = py_fields - rust_fields

    assert not missing_in_python, (
        f"Rust SessionStateResponse DTO sends {sorted(missing_in_python)} "
        f"but Python _dict_to_session_state() doesn't read them.\n"
        f"Rust: {sorted(rust_fields)}, Python: {sorted(py_fields)}"
    )
    assert not extra_in_python, (
        f"Python _dict_to_session_state() reads {sorted(extra_in_python)} "
        f"but Rust SessionStateResponse DTO doesn't send them.\n"
        f"Rust: {sorted(rust_fields)}, Python: {sorted(py_fields)}"
    )


# =============================================================================
# Tests: kernel.rs ↔ kernel_client.py
# =============================================================================

def test_pcb_fields_match():
    """Rust ProcessInfoResponse DTO fields match Python _dict_to_process_info() reads."""
    rust_source = (HANDLER_DIR / "kernel.rs").read_text(encoding="utf-8")
    py_source = KERNEL_CLIENT.read_text(encoding="utf-8")

    rust_top_fields = _extract_dto_struct_fields(rust_source, "ProcessInfoResponse")
    rust_usage_fields = _extract_dto_struct_fields(rust_source, "UsageResponse")

    py_top_fields = _extract_python_gets(py_source, "_dict_to_process_info")
    py_usage_fields = _extract_python_nested_gets(py_source, "_dict_to_process_info", "usage")

    # Check top-level fields (Python reads "usage" as a nested dict)
    missing_top = rust_top_fields - py_top_fields
    assert not missing_top, (
        f"Rust ProcessInfoResponse DTO sends top-level {sorted(missing_top)} "
        f"but Python _dict_to_process_info() doesn't read them.\n"
        f"Rust: {sorted(rust_top_fields)}, Python: {sorted(py_top_fields)}"
    )

    # Check usage sub-fields
    missing_usage = rust_usage_fields - py_usage_fields
    assert not missing_usage, (
        f"Rust UsageResponse DTO sends {sorted(missing_usage)} "
        f"but Python doesn't read them.\n"
        f"Rust usage: {sorted(rust_usage_fields)}, Python usage: {sorted(py_usage_fields)}"
    )


# test_quota_fields_match and test_system_status_fields_match removed:
# These methods now return raw dicts (passthrough), so field parity is automatic.
