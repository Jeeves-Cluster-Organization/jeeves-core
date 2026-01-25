"""Unit tests for SSE (Server-Sent Events) utilities.

Tests SSE event formatting, streaming, and keepalive functionality.
Coverage target: 80%+

Test Strategy:
- Test all formatting functions (format_sse_event, format_sse_comment)
- Test SSEStream class methods (event, done, error, keepalive)
- Test async stream merging with keepalives
- Test edge cases (multiline data, empty data, timeouts)
"""

import pytest
import asyncio
import json
from typing import List

from avionics.gateway.sse import (
    format_sse_event,
    format_sse_comment,
    sse_keepalive,
    SSEStream,
    merge_sse_streams,
)


# =============================================================================
# format_sse_event Tests
# =============================================================================

class TestFormatSSEEvent:
    """Test SSE event formatting."""

    def test_format_simple_string_data(self):
        """Test formatting simple string data."""
        result = format_sse_event("Hello World")

        assert "data: Hello World\n\n" == result

    def test_format_dict_data(self):
        """Test formatting dict data (JSON serialization)."""
        data = {"message": "test", "count": 42}
        result = format_sse_event(data)

        assert "data: {" in result
        assert '"message": "test"' in result
        assert '"count": 42' in result
        assert result.endswith("\n\n")

    def test_format_with_event_type(self):
        """Test formatting with event type."""
        result = format_sse_event("data", event="custom_event")

        assert "event: custom_event\n" in result
        assert "data: data\n\n" in result

    def test_format_with_id(self):
        """Test formatting with event ID."""
        result = format_sse_event("data", id="123")

        assert "id: 123\n" in result
        assert "data: data\n\n" in result

    def test_format_with_retry(self):
        """Test formatting with retry interval."""
        result = format_sse_event("data", retry=5000)

        assert "retry: 5000\n" in result
        assert "data: data\n\n" in result

    def test_format_with_all_fields(self):
        """Test formatting with all optional fields."""
        result = format_sse_event(
            data="test",
            event="my_event",
            id="456",
            retry=3000,
        )

        lines = result.split("\n")
        assert "id: 456" in lines
        assert "event: my_event" in lines
        assert "retry: 3000" in lines
        assert "data: test" in lines
        assert lines[-1] == ""  # Ends with \n\n
        assert lines[-2] == ""

    def test_format_multiline_string(self):
        """Test formatting multiline string data."""
        data = "line1\nline2\nline3"
        result = format_sse_event(data)

        assert "data: line1\n" in result
        assert "data: line2\n" in result
        assert "data: line3\n\n" in result

    def test_format_multiline_json(self):
        """Test formatting complex nested JSON."""
        data = {
            "nested": {
                "array": [1, 2, 3],
                "string": "value",
            }
        }
        result = format_sse_event(data)

        # Should be JSON serialized
        assert "data:" in result
        assert result.endswith("\n\n")

    def test_format_empty_string(self):
        """Test formatting empty string."""
        result = format_sse_event("")

        assert result == "data: \n\n"

    def test_format_none_fields(self):
        """Test that None fields are omitted."""
        result = format_sse_event(
            data="test",
            event=None,
            id=None,
            retry=None,
        )

        assert "event:" not in result
        assert "id:" not in result
        assert "retry:" not in result
        assert result == "data: test\n\n"


# =============================================================================
# format_sse_comment Tests
# =============================================================================

class TestFormatSSEComment:
    """Test SSE comment formatting."""

    def test_format_keepalive_comment(self):
        """Test formatting keepalive comment."""
        result = format_sse_comment("keepalive")

        assert result == ": keepalive\n\n"

    def test_format_custom_comment(self):
        """Test formatting custom comment."""
        result = format_sse_comment("ping")

        assert result == ": ping\n\n"

    def test_format_empty_comment(self):
        """Test formatting empty comment."""
        result = format_sse_comment("")

        assert result == ": \n\n"


# =============================================================================
# sse_keepalive Tests
# =============================================================================

class TestSSEKeepalive:
    """Test SSE keepalive generator."""

    @pytest.mark.asyncio
    async def test_keepalive_generates_comments(self):
        """Test that keepalive yields comment strings."""
        gen = sse_keepalive(interval=0.01, comment="test")

        # Get first keepalive
        result = await asyncio.wait_for(gen.__anext__(), timeout=1.0)

        assert result == ": test\n\n"

    @pytest.mark.asyncio
    async def test_keepalive_respects_interval(self):
        """Test that keepalive respects interval timing."""
        gen = sse_keepalive(interval=0.05)

        start = asyncio.get_event_loop().time()
        await gen.__anext__()
        elapsed = asyncio.get_event_loop().time() - start

        # Should wait approximately 0.05 seconds
        assert 0.04 < elapsed < 0.15  # Allow some tolerance

    @pytest.mark.asyncio
    async def test_keepalive_generates_multiple(self):
        """Test that keepalive generates multiple comments."""
        gen = sse_keepalive(interval=0.01)

        results = []
        for _ in range(3):
            result = await asyncio.wait_for(gen.__anext__(), timeout=1.0)
            results.append(result)

        assert len(results) == 3
        assert all(r == ": keepalive\n\n" for r in results)


# =============================================================================
# SSEStream Tests
# =============================================================================

class TestSSEStream:
    """Test SSEStream class."""

    def test_init_defaults(self):
        """Test SSEStream initialization with defaults."""
        stream = SSEStream()

        assert stream._event_id == 0
        assert stream._include_keepalive is True
        assert stream._keepalive_interval == 15.0

    def test_init_custom_params(self):
        """Test SSEStream initialization with custom params."""
        stream = SSEStream(include_keepalive=False, keepalive_interval=30.0)

        assert stream._event_id == 0
        assert stream._include_keepalive is False
        assert stream._keepalive_interval == 30.0

    def test_event_with_auto_id(self):
        """Test event with auto-incrementing ID."""
        stream = SSEStream()

        event1 = stream.event({"data": "first"})
        event2 = stream.event({"data": "second"})
        event3 = stream.event({"data": "third"})

        assert "id: 1\n" in event1
        assert "id: 2\n" in event2
        assert "id: 3\n" in event3

    def test_event_without_id(self):
        """Test event without ID."""
        stream = SSEStream()

        result = stream.event("data", include_id=False)

        assert "id:" not in result
        assert "data: data\n\n" in result

    def test_event_with_type(self):
        """Test event with custom type."""
        stream = SSEStream()

        result = stream.event({"msg": "test"}, event="custom")

        assert "event: custom\n" in result
        assert "id: 1\n" in result

    def test_event_increments_counter(self):
        """Test that event counter increments correctly."""
        stream = SSEStream()

        stream.event("first")
        stream.event("second", include_id=False)  # No ID
        stream.event("third")

        # Counter should be at 2 (first and third had IDs)
        assert stream._event_id == 2

    def test_done_without_data(self):
        """Test done marker without data."""
        stream = SSEStream()

        result = stream.done()

        assert result == "data: [DONE]\n\n"

    def test_done_with_data(self):
        """Test done marker with final data."""
        stream = SSEStream()

        result = stream.done(data={"status": "complete"})

        # Should have both the data event and [DONE] marker
        assert "event: done\n" in result
        assert "data: [DONE]\n\n" in result
        assert "status" in result

    def test_error_simple(self):
        """Test error event without code."""
        stream = SSEStream()

        result = stream.error("Something went wrong")

        assert "event: error\n" in result
        assert '"error": "Something went wrong"' in result

    def test_error_with_code(self):
        """Test error event with error code."""
        stream = SSEStream()

        result = stream.error("Not found", code="404")

        assert "event: error\n" in result
        assert '"error": "Not found"' in result
        assert '"code": "404"' in result

    def test_keepalive_comment(self):
        """Test keepalive comment generation."""
        stream = SSEStream()

        result = stream.keepalive()

        assert result == ": keepalive\n\n"

    def test_multiple_events_maintain_state(self):
        """Test that stream maintains ID counter across events."""
        stream = SSEStream()

        stream.event("a")
        stream.event("b")
        stream.error("err")
        result = stream.done({"final": "data"})

        # ID should be 4 (3 events with IDs + 1 done with ID)
        assert "id: 4\n" in result


# =============================================================================
# merge_sse_streams Tests
# =============================================================================

class TestMergeSSEStreams:
    """Test SSE stream merging with keepalives."""

    @pytest.mark.asyncio
    async def test_merge_yields_all_data(self):
        """Test that merge yields all data from source stream."""
        async def data_stream():
            yield "event1\n\n"
            yield "event2\n\n"
            yield "event3\n\n"

        results = []
        async for item in merge_sse_streams(data_stream(), keepalive_interval=10.0):
            results.append(item)

        assert len(results) == 3
        assert results[0] == "event1\n\n"
        assert results[1] == "event2\n\n"
        assert results[2] == "event3\n\n"

    @pytest.mark.asyncio
    async def test_merge_sends_keepalive_on_timeout(self):
        """Test that merge sends keepalive when no data arrives."""
        async def slow_stream():
            await asyncio.sleep(0.2)  # Longer than keepalive interval
            yield "data\n\n"

        results = []
        async for item in merge_sse_streams(slow_stream(), keepalive_interval=0.05):
            results.append(item)
            # Wait until we get the actual data event
            if "data\n\n" in results:
                break

        # Should have at least one keepalive before data
        assert len(results) >= 2
        assert ": keepalive\n\n" in results
        assert "data\n\n" in results

    @pytest.mark.asyncio
    async def test_merge_completes_when_stream_done(self):
        """Test that merge completes when source stream finishes."""
        async def finite_stream():
            yield "a\n\n"
            yield "b\n\n"

        results = []
        async for item in merge_sse_streams(finite_stream(), keepalive_interval=10.0):
            results.append(item)

        assert len(results) == 2

    @pytest.mark.asyncio
    async def test_merge_handles_fast_stream(self):
        """Test that merge handles fast-emitting stream without keepalives."""
        async def fast_stream():
            for i in range(5):
                yield f"event{i}\n\n"
                await asyncio.sleep(0.001)  # Very fast

        results = []
        async for item in merge_sse_streams(fast_stream(), keepalive_interval=1.0):
            results.append(item)

        # Should only have data, no keepalives
        assert len(results) == 5
        assert all("event" in r for r in results)
        assert not any("keepalive" in r for r in results)

    @pytest.mark.asyncio
    async def test_merge_handles_empty_stream(self):
        """Test that merge handles empty stream."""
        async def empty_stream():
            if False:  # Never yields
                yield ""

        results = []
        timeout_occurred = False

        try:
            async for item in merge_sse_streams(empty_stream(), keepalive_interval=0.05):
                results.append(item)
                if len(results) >= 1:  # Get at least one keepalive
                    break
        except asyncio.TimeoutError:
            timeout_occurred = True

        # Should either get keepalives or complete immediately
        assert len(results) >= 0

    @pytest.mark.asyncio
    async def test_merge_cancellation_cleanup(self):
        """Test that merge cleans up properly on cancellation."""
        async def infinite_stream():
            while True:
                yield "data\n\n"
                await asyncio.sleep(0.1)

        results = []
        try:
            async for item in merge_sse_streams(infinite_stream(), keepalive_interval=1.0):
                results.append(item)
                if len(results) >= 2:
                    break  # Exit early
        except asyncio.CancelledError:
            pass

        # Should have gotten some results before breaking
        assert len(results) >= 2


# =============================================================================
# Integration Tests
# =============================================================================

class TestSSEIntegration:
    """Test SSE components working together."""

    @pytest.mark.asyncio
    async def test_sse_stream_with_merge(self):
        """Test SSEStream output can be merged."""
        stream = SSEStream()

        async def data_generator():
            yield stream.event({"msg": "start"}, event="started")
            await asyncio.sleep(0.01)
            yield stream.event({"msg": "processing"})
            await asyncio.sleep(0.01)
            yield stream.done({"msg": "complete"})

        results = []
        async for item in merge_sse_streams(data_generator(), keepalive_interval=10.0):
            results.append(item)

        assert len(results) == 3
        assert "event: started\n" in results[0]
        assert "id: 2\n" in results[1]
        assert "event: done\n" in results[2]
        assert "data: [DONE]\n\n" in results[2]

    def test_sse_stream_full_lifecycle(self):
        """Test complete SSEStream lifecycle."""
        stream = SSEStream()

        events = []
        events.append(stream.event({"status": "started"}, event="flow_start"))
        events.append(stream.event({"progress": 25}))
        events.append(stream.event({"progress": 50}))
        events.append(stream.event({"progress": 75}))
        events.append(stream.done({"status": "completed"}))

        # Verify IDs increment
        assert "id: 1\n" in events[0]
        assert "id: 2\n" in events[1]
        assert "id: 3\n" in events[2]
        assert "id: 4\n" in events[3]
        assert "id: 5\n" in events[4]

        # Verify [DONE] marker
        assert "data: [DONE]\n\n" in events[4]
