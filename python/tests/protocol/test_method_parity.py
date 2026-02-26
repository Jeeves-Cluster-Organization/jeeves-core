"""Cross-language IPC method name parity tests.

Parses Rust dispatch match arms from src/ipc/handlers/*.rs and asserts
Python kernel_client.py uses exactly the same method strings.
Catches method name drift at CI time.
"""

import re
from pathlib import Path

HANDLER_DIR = Path(__file__).resolve().parent.parent.parent.parent / "src" / "ipc" / "handlers"
KERNEL_CLIENT = Path(__file__).resolve().parent.parent.parent / "jeeves_infra" / "kernel_client.py"

# service name -> Rust handler file
SERVICE_FILES = {
    "kernel": HANDLER_DIR / "kernel.rs",
    "orchestration": HANDLER_DIR / "orchestration.rs",
    "commbus": HANDLER_DIR / "commbus.rs",
    "interrupt": HANDLER_DIR / "interrupt.rs",
    "tools": HANDLER_DIR / "tools.rs",
}


def _extract_rust_methods(rust_file: Path) -> set[str]:
    """Extract method names from Rust match arms like '"CreateProcess" =>'."""
    source = rust_file.read_text(encoding="utf-8")
    # Match quoted PascalCase identifiers followed by =>
    return set(re.findall(r'"([A-Z][a-zA-Z]+)"\s*=>', source))


def _extract_python_methods(client_file: Path) -> dict[str, set[str]]:
    """Extract (service, method) pairs from transport.request() calls."""
    source = client_file.read_text(encoding="utf-8")
    # Match self._transport.request("service", "Method", ...) and request_stream variant
    pairs = re.findall(
        r'self\._transport\.request(?:_stream)?\(\s*"(\w+)"\s*,\s*"([A-Z]\w+)"',
        source,
    )
    result: dict[str, set[str]] = {}
    for service, method in pairs:
        result.setdefault(service, set()).add(method)
    return result


def test_all_python_methods_have_rust_handlers():
    """Every Python kernel_client method call must have a matching Rust handler."""
    py_methods = _extract_python_methods(KERNEL_CLIENT)

    for service, rust_file in SERVICE_FILES.items():
        assert rust_file.exists(), f"Rust handler file not found: {rust_file}"
        rust_methods = _extract_rust_methods(rust_file)

        py_service_methods = py_methods.get(service, set())
        missing_in_rust = py_service_methods - rust_methods
        assert not missing_in_rust, (
            f"Python calls {service}.{sorted(missing_in_rust)} "
            f"but Rust has no handler. Rust has: {sorted(rust_methods)}"
        )


def test_rust_handler_files_exist():
    """All expected Rust handler files exist."""
    for service, path in SERVICE_FILES.items():
        assert path.exists(), f"Missing Rust handler for service '{service}': {path}"


def test_python_uses_known_services_only():
    """Python kernel_client should only call known services."""
    py_methods = _extract_python_methods(KERNEL_CLIENT)
    unknown = set(py_methods.keys()) - set(SERVICE_FILES.keys())
    assert not unknown, f"Python calls unknown services: {sorted(unknown)}"
