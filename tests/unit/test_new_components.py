"""Tests for newly added components.

This file tests the core changes from the architecture refactoring:
1. InMemoryCommBus - Python message bus matching Go design
2. PostgresGraphAdapter - Graph storage protocol implementation
3. UUIDGenerator - ID generation implementing IdGeneratorProtocol
"""

import pytest
import asyncio
from dataclasses import dataclass
from typing import Optional


# =============================================================================
# Test InMemoryCommBus
# =============================================================================


class TestInMemoryCommBus:
    """Tests for the InMemoryCommBus implementation."""

    def test_import(self):
        """Test that CommBus can be imported."""
        from jeeves_control_tower.ipc import InMemoryCommBus, get_commbus, reset_commbus
        assert InMemoryCommBus is not None
        assert get_commbus is not None
        assert reset_commbus is not None

    def test_create_instance(self):
        """Test creating a CommBus instance."""
        from jeeves_control_tower.ipc import InMemoryCommBus
        bus = InMemoryCommBus()
        assert bus is not None

    @pytest.mark.asyncio
    async def test_publish_subscribe(self):
        """Test event publishing and subscription."""
        from jeeves_control_tower.ipc import InMemoryCommBus

        bus = InMemoryCommBus()
        received = []

        @dataclass
        class TestEvent:
            message: str

        async def handler(event):
            received.append(event.message)

        # Subscribe (in async context, need to wait for subscription to register)
        bus.subscribe("TestEvent", handler)
        await asyncio.sleep(0.01)  # Allow async subscription to complete

        # Publish
        await bus.publish(TestEvent(message="hello"))

        # Verify
        assert len(received) == 1
        assert received[0] == "hello"

    @pytest.mark.asyncio
    async def test_query_response(self):
        """Test query-response pattern."""
        from jeeves_control_tower.ipc import InMemoryCommBus

        bus = InMemoryCommBus()

        @dataclass
        class GetValue:
            key: str

        async def handler(query: GetValue):
            return {"value": f"result_{query.key}"}

        # Register handler
        bus.register_handler("GetValue", handler)

        # Query
        result = await bus.query(GetValue(key="test"))

        # Verify
        assert result == {"value": "result_test"}

    @pytest.mark.asyncio
    async def test_send_command(self):
        """Test fire-and-forget command."""
        from jeeves_control_tower.ipc import InMemoryCommBus

        bus = InMemoryCommBus()
        executed = []

        @dataclass
        class DoSomething:
            action: str

        async def handler(cmd: DoSomething):
            executed.append(cmd.action)

        # Register handler
        bus.register_handler("DoSomething", handler)

        # Send
        await bus.send(DoSomething(action="test_action"))

        # Verify
        assert len(executed) == 1
        assert executed[0] == "test_action"


# =============================================================================
# Test InMemoryGraphStorage
# =============================================================================


class TestInMemoryGraphStorage:
    """Tests for the InMemoryGraphStorage implementation."""

    def test_import(self):
        """Test that graph storage can be imported."""
        from jeeves_memory_module.repositories import InMemoryGraphStorage, GraphNode, GraphEdge
        assert InMemoryGraphStorage is not None
        assert GraphNode is not None
        assert GraphEdge is not None

    @pytest.mark.asyncio
    async def test_add_node(self):
        """Test adding a node to the graph."""
        from jeeves_memory_module.repositories import InMemoryGraphStorage

        graph = InMemoryGraphStorage()
        result = await graph.add_node(
            node_id="test:node1",
            node_type="test",
            properties={"name": "Node 1"},
        )
        assert result is True

        # Duplicate should return False
        result = await graph.add_node(
            node_id="test:node1",
            node_type="test",
            properties={"name": "Node 1"},
        )
        assert result is False

    @pytest.mark.asyncio
    async def test_add_edge(self):
        """Test adding an edge between nodes."""
        from jeeves_memory_module.repositories import InMemoryGraphStorage

        graph = InMemoryGraphStorage()

        # Add nodes first
        await graph.add_node("file:a.py", "file", {"path": "a.py"})
        await graph.add_node("file:b.py", "file", {"path": "b.py"})

        # Add edge
        result = await graph.add_edge("file:a.py", "file:b.py", "imports")
        assert result is True

        # Duplicate should return False
        result = await graph.add_edge("file:a.py", "file:b.py", "imports")
        assert result is False

    @pytest.mark.asyncio
    async def test_get_node(self):
        """Test getting a node by ID."""
        from jeeves_memory_module.repositories import InMemoryGraphStorage

        graph = InMemoryGraphStorage()
        await graph.add_node("test:node", "test", {"key": "value"})

        node = await graph.get_node("test:node")
        assert node is not None
        assert node["node_id"] == "test:node"
        assert node["node_type"] == "test"
        assert node["properties"]["key"] == "value"

        # Non-existent
        node = await graph.get_node("test:nonexistent")
        assert node is None

    @pytest.mark.asyncio
    async def test_get_neighbors(self):
        """Test getting neighbors of a node."""
        from jeeves_memory_module.repositories import InMemoryGraphStorage

        graph = InMemoryGraphStorage()

        # Create a simple graph: A -> B -> C
        await graph.add_node("A", "node", {})
        await graph.add_node("B", "node", {})
        await graph.add_node("C", "node", {})
        await graph.add_edge("A", "B", "connects")
        await graph.add_edge("B", "C", "connects")

        # Get outgoing neighbors of A
        neighbors = await graph.get_neighbors("A", direction="out")
        assert len(neighbors) == 1
        assert neighbors[0]["node_id"] == "B"

        # Get incoming neighbors of B
        neighbors = await graph.get_neighbors("B", direction="in")
        assert len(neighbors) == 1
        assert neighbors[0]["node_id"] == "A"

    @pytest.mark.asyncio
    async def test_find_path(self):
        """Test finding a path between nodes."""
        from jeeves_memory_module.repositories import InMemoryGraphStorage

        graph = InMemoryGraphStorage()

        # Create a simple path: A -> B -> C
        await graph.add_node("A", "node", {})
        await graph.add_node("B", "node", {})
        await graph.add_node("C", "node", {})
        await graph.add_edge("A", "B", "connects")
        await graph.add_edge("B", "C", "connects")

        # Find path from A to C
        path = await graph.find_path("A", "C")
        assert path is not None
        assert len(path) == 3
        assert [p["node_id"] for p in path] == ["A", "B", "C"]

        # No path
        await graph.add_node("D", "node", {})
        path = await graph.find_path("A", "D")
        assert path is None


# =============================================================================
# Test UUIDGenerator
# =============================================================================


class TestUUIDGenerator:
    """Tests for the UUIDGenerator implementation."""

    def test_import(self):
        """Test that ID generator can be imported."""
        from jeeves_shared import UUIDGenerator, DeterministicIdGenerator, get_id_generator
        assert UUIDGenerator is not None
        assert DeterministicIdGenerator is not None
        assert get_id_generator is not None

    def test_generate(self):
        """Test generating a UUID."""
        from jeeves_shared import UUIDGenerator

        gen = UUIDGenerator()
        id1 = gen.generate()
        id2 = gen.generate()

        # Should be valid UUID format
        assert len(id1) == 36
        assert id1.count("-") == 4

        # Should be unique
        assert id1 != id2

    def test_generate_prefixed(self):
        """Test generating a prefixed ID."""
        from jeeves_shared import UUIDGenerator

        gen = UUIDGenerator()
        id1 = gen.generate_prefixed("req")

        # Should have prefix
        assert id1.startswith("req_")
        assert len(id1) > 4

    def test_deterministic_generator(self):
        """Test the deterministic generator for testing."""
        from jeeves_shared import DeterministicIdGenerator

        gen1 = DeterministicIdGenerator(seed="test")
        gen2 = DeterministicIdGenerator(seed="test")

        # Same seed should produce same IDs
        assert gen1.generate() == gen2.generate()
        assert gen1.generate() == gen2.generate()

        # Reset should restart sequence
        gen1.reset()
        gen2.reset()
        assert gen1.generate() == gen2.generate()

    def test_protocol_compliance(self):
        """Test that UUIDGenerator implements IdGeneratorProtocol."""
        from jeeves_shared import UUIDGenerator
        from jeeves_protocols import IdGeneratorProtocol

        gen = UUIDGenerator()
        assert isinstance(gen, IdGeneratorProtocol)


# =============================================================================
# Test PostgresGraphAdapter (structure only, no DB)
# =============================================================================


class TestPostgresGraphAdapterStructure:
    """Tests for PostgresGraphAdapter structure (no database required)."""

    def test_import(self):
        """Test that adapter can be imported."""
        from jeeves_avionics.database import PostgresGraphAdapter
        assert PostgresGraphAdapter is not None

    def test_protocol_compliance(self):
        """Test that adapter implements GraphStorageProtocol."""
        from jeeves_avionics.database import PostgresGraphAdapter
        from jeeves_protocols import GraphStorageProtocol

        # The adapter should implement the protocol
        # (verified at module load time)
        assert hasattr(PostgresGraphAdapter, 'add_node')
        assert hasattr(PostgresGraphAdapter, 'add_edge')
        assert hasattr(PostgresGraphAdapter, 'get_node')
        assert hasattr(PostgresGraphAdapter, 'get_neighbors')
        assert hasattr(PostgresGraphAdapter, 'find_path')
        assert hasattr(PostgresGraphAdapter, 'query_subgraph')
        assert hasattr(PostgresGraphAdapter, 'delete_node')
