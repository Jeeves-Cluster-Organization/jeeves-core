"""Wire protocol constant parity tests.

Asserts Python wire constants (ipc/protocol.py) match Rust constants
(src/ipc/codec.rs). Both define MSG_REQUEST, MSG_RESPONSE, etc.
"""

import re
from pathlib import Path

RUST_CODEC = Path(__file__).resolve().parent.parent.parent.parent / "src" / "ipc" / "codec.rs"

# Canonical wire constants — both Rust and Python must match these
EXPECTED = {
    "MSG_REQUEST": 0x01,
    "MSG_RESPONSE": 0x02,
    "MSG_STREAM_CHUNK": 0x03,
    "MSG_STREAM_END": 0x04,
    "MSG_ERROR": 0xFF,
}


def test_python_wire_constants():
    """Python ipc/protocol.py constants match expected values."""
    from jeeves_airframe.ipc.protocol import (
        MSG_REQUEST,
        MSG_RESPONSE,
        MSG_STREAM_CHUNK,
        MSG_STREAM_END,
        MSG_ERROR,
    )
    actual = {
        "MSG_REQUEST": MSG_REQUEST,
        "MSG_RESPONSE": MSG_RESPONSE,
        "MSG_STREAM_CHUNK": MSG_STREAM_CHUNK,
        "MSG_STREAM_END": MSG_STREAM_END,
        "MSG_ERROR": MSG_ERROR,
    }
    for name, expected_val in EXPECTED.items():
        assert actual[name] == expected_val, (
            f"Python {name} = {hex(actual[name])}, expected {hex(expected_val)}"
        )


def test_rust_wire_constants():
    """Rust ipc/codec.rs constants match expected values."""
    source = RUST_CODEC.read_text(encoding="utf-8")
    for name, expected_val in EXPECTED.items():
        pattern = rf"pub const {name}: u8 = (0x[0-9A-Fa-f]+);"
        match = re.search(pattern, source)
        assert match, f"Rust constant {name} not found in codec.rs"
        rust_val = int(match.group(1), 16)
        assert rust_val == expected_val, (
            f"Rust {name} = {hex(rust_val)}, expected {hex(expected_val)}"
        )
