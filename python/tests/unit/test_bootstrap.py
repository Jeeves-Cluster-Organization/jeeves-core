"""Unit tests for the bootstrap module.

Tests:
- Request PID context management (set, get, clear, context manager)
- create_app_context composition root (dependency wiring, injection overrides)
- create_tool_executor_with_access factory
"""

import os
import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from jeeves_core.bootstrap import (
    create_app_context,
    set_request_pid,
    get_request_pid,
    clear_request_pid,
    request_pid_context,
)


# =============================================================================
# Request PID Context
# =============================================================================


class TestRequestPidContext:
    def setup_method(self):
        """Ensure clean state before each test."""
        clear_request_pid()

    def test_set_and_get(self):
        set_request_pid("pid-123")
        assert get_request_pid() == "pid-123"

    def test_clear(self):
        set_request_pid("pid-456")
        clear_request_pid()
        assert get_request_pid() is None

    def test_default_is_none(self):
        assert get_request_pid() is None

    def test_overwrite(self):
        set_request_pid("pid-1")
        set_request_pid("pid-2")
        assert get_request_pid() == "pid-2"

    def test_context_manager_sets_and_restores(self):
        assert get_request_pid() is None

        with request_pid_context("pid-789") as pid:
            assert pid == "pid-789"
            assert get_request_pid() == "pid-789"

        # After exiting context, should be reset
        assert get_request_pid() is None

    def test_nested_context_managers(self):
        with request_pid_context("outer"):
            assert get_request_pid() == "outer"

            with request_pid_context("inner"):
                assert get_request_pid() == "inner"

            # After inner context exits, outer is restored
            assert get_request_pid() == "outer"

        assert get_request_pid() is None

    def test_context_manager_returns_pid(self):
        with request_pid_context("test-pid") as pid:
            assert pid == "test-pid"


# =============================================================================
# create_app_context (composition root)
# =============================================================================


class TestCreateAppContext:
    """Test the composition root via its public injection interface.

    Instead of deeply patching all local imports inside create_app_context
    (which is fragile), we test the injection override paths where
    pre-configured objects are passed in, and test the env-var-driven
    config builders separately above.
    """

    def test_configures_logging_with_settings(self):
        """Verify that configure_logging is called with settings-derived args."""
        mock_logger = MagicMock()
        mock_logger.bind = MagicMock(return_value=mock_logger)
        mock_logger.info = MagicMock()

        mock_settings = MagicMock()
        mock_settings.log_level = "DEBUG"
        mock_settings.airframe_llm_adapter = "mock"

        mock_ff = MagicMock()
        mock_ff.enable_tracing = True
        mock_ff.use_redis_state = False
        mock_ff.enable_distributed_mode = False

        mock_registry = MagicMock()
        mock_registry.get_default_service.return_value = "test"

        with patch("jeeves_core.bootstrap.get_settings", return_value=mock_settings), \
             patch("jeeves_core.bootstrap.get_feature_flags", return_value=mock_ff), \
             patch("jeeves_core.bootstrap.configure_logging") as mock_configure_logging, \
             patch("jeeves_core.logging.create_logger", return_value=mock_logger), \
             patch("jeeves_core.bootstrap.get_capability_resource_registry", return_value=mock_registry), \
             patch("jeeves_core.llm.factory.create_llm_provider_factory", return_value=MagicMock()), \
             patch("jeeves_core.ipc.IpcTransport", return_value=MagicMock()), \
             patch("jeeves_core.bootstrap.KernelClient", return_value=MagicMock()), \
             patch("jeeves_core.config.registry.ConfigRegistry", return_value=MagicMock()), \
             patch("jeeves_core.redis.connection_manager.InMemoryStateBackend", return_value=MagicMock()), \
             patch("jeeves_core.observability.otel_adapter.init_global_otel"), \
             patch("jeeves_core.observability.otel_adapter.get_global_otel_adapter", return_value=MagicMock(enabled=True)):

            create_app_context()

        mock_configure_logging.assert_called_once_with(
            level="DEBUG",
            json_output=True,
            enable_otel=True,
        )

    def test_returns_app_context_with_all_fields(self):
        """Verify the returned AppContext has all expected fields populated."""
        from jeeves_core.context import AppContext

        mock_logger = MagicMock()
        mock_logger.bind = MagicMock(return_value=mock_logger)
        mock_logger.info = MagicMock()

        mock_settings = MagicMock()
        mock_settings.log_level = "INFO"
        mock_settings.airframe_llm_adapter = "mock"

        mock_ff = MagicMock()
        mock_ff.enable_tracing = False
        mock_ff.use_redis_state = False
        mock_ff.enable_distributed_mode = False

        mock_registry = MagicMock()
        mock_registry.get_default_service.return_value = "test"

        with patch("jeeves_core.bootstrap.get_settings", return_value=mock_settings), \
             patch("jeeves_core.bootstrap.get_feature_flags", return_value=mock_ff), \
             patch("jeeves_core.bootstrap.configure_logging"), \
             patch("jeeves_core.logging.create_logger", return_value=mock_logger), \
             patch("jeeves_core.bootstrap.get_capability_resource_registry", return_value=mock_registry), \
             patch("jeeves_core.llm.factory.create_llm_provider_factory", return_value=MagicMock()), \
             patch("jeeves_core.ipc.IpcTransport", return_value=MagicMock()), \
             patch("jeeves_core.bootstrap.KernelClient", return_value=MagicMock()), \
             patch("jeeves_core.config.registry.ConfigRegistry", return_value=MagicMock()), \
             patch("jeeves_core.redis.connection_manager.InMemoryStateBackend", return_value=MagicMock()):

            ctx = create_app_context()

        assert isinstance(ctx, AppContext)
        assert ctx.settings is mock_settings
        assert ctx.feature_flags is mock_ff
        assert ctx.kernel_client is not None
        assert ctx.state_backend is not None



# =============================================================================
# create_tool_executor_with_access
# =============================================================================


class TestCreateToolExecutorWithAccess:
    def test_creates_tool_executor(self):
        from jeeves_core.bootstrap import create_tool_executor_with_access

        mock_registry = MagicMock()
        mock_app_context = MagicMock()
        mock_app_context.get_bound_logger = MagicMock(return_value=MagicMock())

        mock_tool_executor = MagicMock()

        with patch("jeeves_core.wiring.ToolExecutor", return_value=mock_tool_executor) as mock_cls:
            result = create_tool_executor_with_access(
                tool_registry=mock_registry,
                app_context=mock_app_context,
            )

        assert result is mock_tool_executor
        mock_cls.assert_called_once()
        call_kwargs = mock_cls.call_args[1]
        assert call_kwargs["registry"] is mock_registry
