"""Tests for IPC transport layer.

Uses in-memory asyncio streams to simulate the kernel TCP connection
without requiring a real kernel process.
"""

import asyncio
import struct

import msgpack
import pytest

from jeeves_infra.ipc.protocol import (
    MSG_REQUEST,
    MSG_RESPONSE,
    MSG_STREAM_CHUNK,
    MSG_STREAM_END,
    MSG_ERROR,
    MAX_FRAME_SIZE,
    IpcError,
    decode_frame,
    encode_frame,
)
from jeeves_infra.ipc.transport import IpcTransport


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_response_frame(request_id: str, body: dict, ok: bool = True) -> bytes:
    """Build a MSG_RESPONSE frame that the transport's _read_loop can parse."""
    payload = {"id": request_id, "ok": ok, "body": body}
    if not ok:
        payload["error"] = body  # body is the error dict in error case
        payload.pop("body", None)
    return encode_frame(MSG_RESPONSE, payload)


def _make_error_frame(request_id: str, code: str, message: str) -> bytes:
    """Build a MSG_ERROR frame."""
    payload = {
        "id": request_id,
        "ok": False,
        "error": {"code": code, "message": message},
    }
    return encode_frame(MSG_ERROR, payload)


def _make_stream_chunk_frame(request_id: str, body: dict) -> bytes:
    return encode_frame(MSG_STREAM_CHUNK, {"id": request_id, "body": body})


def _make_stream_end_frame(request_id: str) -> bytes:
    return encode_frame(MSG_STREAM_END, {"id": request_id})


def _decode_first_outbound_frame(raw_bytes: bytes) -> tuple[int, dict]:
    """Decode the first outbound frame captured by the mock writer."""
    frame_len = struct.unpack(">I", raw_bytes[:4])[0]
    frame_data = raw_bytes[4:4 + frame_len]
    return decode_frame(frame_data)


class FakeKernel:
    """Simulates the kernel side of a TCP connection for testing.

    Writes pre-built response frames into the reader that the transport
    consumes. Captures frames sent by the transport for assertion.
    """

    def __init__(self):
        # Transport reads from this:
        self._to_transport = asyncio.StreamReader()
        # Transport writes to this (we capture via a mock writer):
        self._from_transport: list[bytes] = []

    def feed_frame(self, frame: bytes) -> None:
        """Feed a raw frame (with length prefix) to the transport's reader."""
        self._to_transport.feed_data(frame)

    def feed_eof(self) -> None:
        self._to_transport.feed_eof()

    def get_reader(self) -> asyncio.StreamReader:
        return self._to_transport


async def _create_connected_transport(fake: FakeKernel) -> IpcTransport:
    """Create a transport wired to a FakeKernel (bypass real TCP)."""
    transport = IpcTransport("127.0.0.1", 50051)

    # Monkey-patch: inject fake reader + capture writer
    transport._reader = fake.get_reader()

    # Create a mock writer that captures writes
    class MockWriter:
        def __init__(self):
            self.data = bytearray()
            self.closed = False

        def write(self, data: bytes):
            self.data.extend(data)

        async def drain(self):
            pass

        def close(self):
            self.closed = True

        async def wait_closed(self):
            pass

    transport._writer = MockWriter()
    transport._closed = False
    transport._reader_task = asyncio.create_task(transport._read_loop())
    return transport


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestRequestHappyPath:
    async def test_request_returns_body(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)

        async def respond():
            # Wait a tick for the request to be registered
            await asyncio.sleep(0.01)
            # Find the request_id from pending
            assert len(transport._pending) == 1
            req_id = list(transport._pending.keys())[0]
            fake.feed_frame(_make_response_frame(req_id, {"pid": "p-1", "state": "NEW"}))

        task = asyncio.create_task(respond())
        result = await transport.request("kernel", "CreateProcess", {"user_id": "u1"})
        await task
        assert result == {"pid": "p-1", "state": "NEW"}
        msg_type, payload = _decode_first_outbound_frame(bytes(transport._writer.data))
        assert msg_type == MSG_REQUEST
        assert "protocol_version" not in payload
        await transport.close()


class TestRequestErrors:
    async def test_error_response_raises_ipc_error(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)

        async def respond():
            await asyncio.sleep(0.01)
            req_id = list(transport._pending.keys())[0]
            fake.feed_frame(_make_error_frame(req_id, "NOT_FOUND", "Process not found"))

        task = asyncio.create_task(respond())
        with pytest.raises(IpcError) as exc_info:
            await transport.request("kernel", "GetProcess", {"pid": "bad"})
        await task
        assert exc_info.value.code == "NOT_FOUND"
        assert "Process not found" in exc_info.value.message
        await transport.close()

    async def test_non_ok_response_raises_ipc_error(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)

        async def respond():
            await asyncio.sleep(0.01)
            req_id = list(transport._pending.keys())[0]
            payload = {
                "id": req_id,
                "ok": False,
                "error": {"code": "INVALID_ARGUMENT", "message": "bad field"},
            }
            fake.feed_frame(encode_frame(MSG_RESPONSE, payload))

        task = asyncio.create_task(respond())
        with pytest.raises(IpcError) as exc_info:
            await transport.request("kernel", "CreateProcess", {})
        await task
        assert exc_info.value.code == "INVALID_ARGUMENT"
        await transport.close()

    async def test_timeout_raises_ipc_error(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)
        # Don't feed any response — let it timeout
        with pytest.raises(IpcError) as exc_info:
            await transport.request("kernel", "GetProcess", {"pid": "p-1"}, timeout=0.05)
        assert exc_info.value.code == "TIMEOUT"
        # Pending should be cleaned up
        assert len(transport._pending) == 0
        await transport.close()


class TestConnectionLost:
    async def test_eof_fails_pending_requests(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)

        async def disconnect():
            await asyncio.sleep(0.01)
            fake.feed_eof()

        task = asyncio.create_task(disconnect())
        with pytest.raises(IpcError) as exc_info:
            await transport.request("kernel", "GetProcess", {"pid": "p-1"}, timeout=1.0)
        await task
        assert exc_info.value.code == "UNAVAILABLE"
        await transport.close()


class TestStreaming:
    async def test_stream_yields_chunks_and_ends(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)

        async def respond():
            await asyncio.sleep(0.01)
            req_id = list(transport._stream_queues.keys())[0]
            fake.feed_frame(_make_stream_chunk_frame(req_id, {"event": "msg1"}))
            fake.feed_frame(_make_stream_chunk_frame(req_id, {"event": "msg2"}))
            fake.feed_frame(_make_stream_end_frame(req_id))

        task = asyncio.create_task(respond())
        chunks = []
        async for chunk in transport.request_stream("commbus", "Subscribe", {"topic": "t"}):
            chunks.append(chunk)
        await task
        assert len(chunks) == 2
        assert chunks[0] == {"event": "msg1"}
        assert chunks[1] == {"event": "msg2"}
        msg_type, payload = _decode_first_outbound_frame(bytes(transport._writer.data))
        assert msg_type == MSG_REQUEST
        assert "protocol_version" not in payload
        await transport.close()


class TestClose:
    async def test_close_is_idempotent(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)
        await transport.close()
        await transport.close()  # Should not raise
        assert transport._closed

    async def test_send_after_close_raises(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)
        await transport.close()
        with pytest.raises(IpcError) as exc_info:
            await transport.request("kernel", "GetProcess", {"pid": "p-1"})
        assert exc_info.value.code == "UNAVAILABLE"


class TestOversizedFrame:
    async def test_oversized_frame_disconnects(self):
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)

        # Feed a frame with a length prefix exceeding MAX_FRAME_SIZE
        huge_len = MAX_FRAME_SIZE + 1
        fake.feed_frame(struct.pack(">I", huge_len) + b"\x00" * 10)
        # Wait for the read loop to process and disconnect
        await asyncio.sleep(0.05)
        # The oversized frame breaks the read loop — the frame we sent
        # is raw bytes including the length prefix, but _read_loop reads
        # the length prefix itself. Let's feed it correctly.
        # Actually the _create_connected_transport starts _read_loop which
        # reads LENGTH_PREFIX (4 bytes) then checks frame_len.
        # We need to feed just the 4 bytes + some data.
        assert transport._closed

    async def test_oversized_frame_via_raw_reader(self):
        """Feed an oversized length prefix directly to the reader."""
        fake = FakeKernel()
        transport = await _create_connected_transport(fake)

        # Feed a 4-byte length prefix that exceeds MAX_FRAME_SIZE
        oversized_len = MAX_FRAME_SIZE + 100
        fake._to_transport.feed_data(struct.pack(">I", oversized_len))
        # Feed enough data so readexactly doesn't block forever
        fake._to_transport.feed_data(b"\x00" * 100)

        await asyncio.sleep(0.05)
        assert transport._closed
        await transport.close()
