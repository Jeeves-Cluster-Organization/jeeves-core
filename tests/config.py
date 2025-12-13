"""Test configuration for integration tests.

This module provides configuration constants and utilities for testing.
"""

import os

# CI Detection
IS_CI = os.environ.get("CI", "").lower() in ("true", "1", "yes")
IS_GITHUB_ACTIONS = os.environ.get("GITHUB_ACTIONS", "").lower() == "true"

# Logging configuration
SILENCE_LIBRARY_LOGS = True

# Test database defaults
DEFAULT_TEST_DATABASE = "test_jeeves"
DEFAULT_TEST_USER = "postgres"
DEFAULT_TEST_PASSWORD = "postgres"
DEFAULT_TEST_HOST = "localhost"
DEFAULT_TEST_PORT = 5432

# Timeouts
DEFAULT_TEST_TIMEOUT = 30  # seconds
ASYNC_TEST_TIMEOUT = 60  # seconds for async operations
