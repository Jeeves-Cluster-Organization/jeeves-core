"""Unit tests for the ControlTower kernel.

Tests kernel-level operations in isolation.
"""

import pytest
from unittest.mock import MagicMock, AsyncMock, patch
from datetime import datetime, timezone

from control_tower.kernel import ControlTower
from control_tower.types import (
    ResourceQuota,
    ProcessState,
    SchedulingPriority,
    ServiceDescriptor,
)
from jeeves_core.types import Envelope, TerminalReason
from protocols import RequestContext


# =============================================================================
# Fixtures
# =============================================================================

@pytest.fixture
def mock_logger():
    """Create a mock logger."""
    logger = MagicMock()
    logger.bind.return_value = logger
    return logger


@pytest.fixture
def default_quota():
    """Create a default resource quota."""
    return ResourceQuota(
        max_llm_calls=10,
        max_tool_calls=50,
        max_agent_hops=21,
        max_iterations=3,
        timeout_seconds=300,
    )


@pytest.fixture
def kernel(mock_logger, default_quota):
    """Create a ControlTower kernel instance."""
    return ControlTower(
        logger=mock_logger,
        default_quota=default_quota,
    )


@pytest.fixture
def sample_envelope():
    """Create a sample Envelope for testing."""
    request_context = RequestContext(
        request_id="req_test456",
        capability="test_capability",
        user_id="user_001",
        session_id="session_001",
    )
    return Envelope(
        request_context=request_context,
        envelope_id="env_test123",
        request_id="req_test456",
        user_id="user_001",
        session_id="session_001",
        raw_input="Test input",
        received_at=datetime.now(timezone.utc),
        created_at=datetime.now(timezone.utc),
        max_iterations=3,
        max_llm_calls=10,
        max_agent_hops=21,
    )


# =============================================================================
# Kernel Initialization Tests
# =============================================================================

class TestKernelInitialization:
    """Tests for ControlTower initialization."""

    def test_kernel_initializes_with_defaults(self, mock_logger):
        """Test that kernel initializes with default values."""
        kernel = ControlTower(logger=mock_logger)
        
        assert kernel is not None
        assert kernel.lifecycle is not None
        assert kernel.resources is not None
        assert kernel.ipc is not None
        assert kernel.events is not None
        assert kernel.interrupts is not None

    def test_kernel_initializes_with_custom_quota(self, mock_logger):
        """Test that kernel uses custom quota."""
        quota = ResourceQuota(max_llm_calls=5, max_iterations=2)
        kernel = ControlTower(logger=mock_logger, default_quota=quota)
        
        assert kernel is not None
        # Verify quota is passed to subsystems
        mock_logger.info.assert_called()

    def test_kernel_initializes_with_custom_service(self, mock_logger):
        """Test that kernel uses custom default service."""
        kernel = ControlTower(
            logger=mock_logger,
            default_service="custom_service",
        )
        
        assert kernel._default_service == "custom_service"


# =============================================================================
# Kernel Property Tests
# =============================================================================

class TestKernelProperties:
    """Tests for kernel property accessors."""

    def test_lifecycle_property(self, kernel):
        """Test lifecycle property returns lifecycle manager."""
        assert kernel.lifecycle is not None
        assert hasattr(kernel.lifecycle, 'submit')

    def test_resources_property(self, kernel):
        """Test resources property returns resource tracker."""
        assert kernel.resources is not None
        assert hasattr(kernel.resources, 'allocate')

    def test_ipc_property(self, kernel):
        """Test ipc property returns commbus coordinator."""
        assert kernel.ipc is not None
        assert hasattr(kernel.ipc, 'register_service')

    def test_events_property(self, kernel):
        """Test events property returns event aggregator."""
        assert kernel.events is not None
        assert hasattr(kernel.events, 'emit_event')

    def test_interrupts_property(self, kernel):
        """Test interrupts property returns interrupt service."""
        assert kernel.interrupts is not None
        assert hasattr(kernel.interrupts, 'create_interrupt')


# =============================================================================
# Resource Recording Tests
# =============================================================================

class TestKernelResourceRecording:
    """Tests for kernel resource recording methods."""

    def test_record_llm_call_tracks_usage(self, kernel, sample_envelope, default_quota):
        """Test that record_llm_call updates tracking."""
        # First, allocate the process
        pid = sample_envelope.request_id
        kernel.resources.allocate(pid, default_quota)
        
        # Record LLM call
        result = kernel.record_llm_call(pid, tokens_in=100, tokens_out=50)
        
        # Should return None (no quota exceeded)
        assert result is None
        
        # Verify usage was recorded
        usage = kernel.resources.get_usage(pid)
        assert usage is not None
        assert usage.llm_calls == 1
        assert usage.tokens_in == 100
        assert usage.tokens_out == 50

    def test_record_llm_call_returns_quota_exceeded(self, kernel, default_quota):
        """Test that record_llm_call returns error when quota exceeded."""
        pid = "test_process"
        kernel.resources.allocate(pid, quota=ResourceQuota(max_llm_calls=1))
        
        # First call should succeed
        result1 = kernel.record_llm_call(pid)
        assert result1 is None
        
        # Second call should exceed quota
        result2 = kernel.record_llm_call(pid)
        assert result2 is not None
        assert "llm" in result2.lower()

    def test_record_tool_call_tracks_usage(self, kernel, default_quota):
        """Test that record_tool_call updates tracking."""
        pid = "test_process"
        kernel.resources.allocate(pid, default_quota)
        
        result = kernel.record_tool_call(pid)
        
        assert result is None
        usage = kernel.resources.get_usage(pid)
        assert usage.tool_calls == 1

    def test_record_agent_hop_tracks_usage(self, kernel, default_quota):
        """Test that record_agent_hop updates tracking."""
        pid = "test_process"
        kernel.resources.allocate(pid, default_quota)
        
        result = kernel.record_agent_hop(pid)
        
        assert result is None
        usage = kernel.resources.get_usage(pid)
        assert usage.agent_hops == 1


# =============================================================================
# Service Registration Tests
# =============================================================================

class TestKernelServiceRegistration:
    """Tests for kernel service registration."""

    def test_register_service_adds_to_ipc(self, kernel):
        """Test that register_service adds service to IPC coordinator."""
        async def handler(envelope):
            return envelope

        result = kernel.register_service(
            name="test_service",
            service_type="flow",
            handler=handler,
            capabilities=["process"],
            max_concurrent=10,
        )

        assert result is True
        assert kernel.ipc.get_service("test_service") is not None

    def test_register_service_with_handler(self, kernel):
        """Test that register_service can register a handler."""
        async def handler(envelope):
            return envelope

        result = kernel.register_service(
            name="test_service",
            service_type="flow",
            handler=handler,
            capabilities=["process"],
        )

        assert result is True


# =============================================================================
# System Status Tests
# =============================================================================

class TestKernelSystemStatus:
    """Tests for kernel system status."""

    def test_get_system_status_returns_dict(self, kernel):
        """Test that get_system_status returns status dictionary."""
        status = kernel.get_system_status()
        
        assert isinstance(status, dict)
        assert "processes" in status
        assert "resources" in status
        assert "events" in status
        assert "services" in status

    def test_get_system_status_tracks_processes(self, kernel, default_quota):
        """Test that system status reflects process count."""
        # Allocate some processes
        kernel.resources.allocate("pid1", default_quota)
        kernel.resources.allocate("pid2", default_quota)
        
        status = kernel.get_system_status()
        
        assert status["resources"]["active_processes"] == 2

    def test_get_request_status_for_unknown_pid(self, kernel):
        """Test that get_request_status returns None for unknown PID."""
        result = kernel.get_request_status("unknown_pid")
        
        assert result is None


# =============================================================================
# Cancel Request Tests
# =============================================================================

class TestKernelCancelRequest:
    """Tests for kernel request cancellation."""

    @pytest.mark.anyio
    async def test_cancel_request_for_unknown_pid(self, kernel):
        """Test that cancel_request returns False for unknown PID."""
        result = await kernel.cancel_request("unknown_pid", "test cancellation")
        
        assert result is False

    @pytest.mark.anyio
    async def test_cancel_request_cleans_up_resources(self, kernel, default_quota):
        """Test that cancel_request releases resources."""
        pid = "test_process"
        kernel.resources.allocate(pid, default_quota)
        
        # Submit a process to lifecycle
        from jeeves_core.types import Envelope
        from datetime import datetime, timezone

        request_context = RequestContext(
            request_id=pid,
            capability="test_capability",
            user_id="user",
            session_id="session",
        )
        envelope = Envelope(
            request_context=request_context,
            envelope_id="env_cancel",
            request_id=pid,
            user_id="user",
            session_id="session",
            raw_input="test",
            created_at=datetime.now(timezone.utc),
        )
        
        pcb = kernel.lifecycle.submit(envelope)
        
        # Cancel it
        result = await kernel.cancel_request(pid, "user cancelled")
        
        # Should have transitioned to TERMINATED
        process = kernel.lifecycle.get_process(pid)
        if process:
            assert process.state == ProcessState.TERMINATED


# =============================================================================
# Lifecycle Integration Tests  
# =============================================================================

class TestKernelLifecycleIntegration:
    """Tests for kernel lifecycle management."""

    def test_submit_to_lifecycle_creates_pcb(self, kernel, sample_envelope):
        """Test that submitting to lifecycle creates a PCB."""
        pcb = kernel.lifecycle.submit(sample_envelope)

        assert pcb is not None
        # Lifecycle uses envelope_id as the process ID
        assert pcb.pid == sample_envelope.envelope_id
        assert pcb.state == ProcessState.NEW

    def test_submit_allocates_resources(self, kernel, sample_envelope, default_quota):
        """Test that submitting allocates resources."""
        pcb = kernel.lifecycle.submit(sample_envelope)
        kernel.resources.allocate(pcb.pid, default_quota)
        
        # Verify resources are tracked
        assert kernel.resources.is_tracked(pcb.pid)

    def test_lifecycle_state_transitions(self, kernel, sample_envelope):
        """Test that lifecycle state transitions work."""
        pcb = kernel.lifecycle.submit(sample_envelope)
        
        # Initial state
        assert pcb.state == ProcessState.NEW
        
        # Schedule it (NEW -> READY)
        kernel.lifecycle.schedule(pcb.pid)
        process = kernel.lifecycle.get_process(pcb.pid)
        assert process.state == ProcessState.READY
        
        # Transition to running
        kernel.lifecycle.transition_state(pcb.pid, ProcessState.RUNNING)
        process = kernel.lifecycle.get_process(pcb.pid)
        assert process.state == ProcessState.RUNNING


