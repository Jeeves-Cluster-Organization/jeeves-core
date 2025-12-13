"""
Common error handling utilities.

Provides reusable error formatting, wrapping, and context extraction utilities.

Location: jeeves_avionics/utils/error_utils.py
Constitutional Layer: Infrastructure (Avionics)
"""

from typing import Any, Dict, Optional, Union

from jeeves_avionics.logging import create_logger


logger = create_logger(__name__)


class ErrorFormatter:
    """Utilities for formatting and normalizing error messages."""

    @staticmethod
    def normalize_error(error: Union[str, Dict[str, Any], Exception]) -> Dict[str, Any]:
        """
        Normalize various error formats to a consistent dictionary structure.

        Args:
            error: Error in various formats (string, dict, Exception)

        Returns:
            Normalized error dictionary with 'message' key
        """
        if isinstance(error, dict):
            # Already a dictionary - ensure it has a 'message' key
            if "message" not in error and "detail" not in error:
                error["message"] = str(error)
            return error

        if isinstance(error, Exception):
            return {
                "message": str(error),
                "type": type(error).__name__
            }

        # String or other type
        return {"message": str(error)}

    @staticmethod
    def extract_error_message(error: Union[str, Dict[str, Any], Exception]) -> str:
        """
        Extract a clean error message from various error formats.

        Args:
            error: Error in various formats

        Returns:
            Clean error message string
        """
        if isinstance(error, dict):
            return error.get("message", "") or error.get("detail", "") or str(error)

        if isinstance(error, Exception):
            return str(error)

        return str(error)

    @staticmethod
    def is_error_type(error_msg: str, error_type: str) -> bool:
        """
        Check if an error message matches a specific error type.

        Args:
            error_msg: Error message to check
            error_type: Error type keyword (e.g., 'not found', 'timeout', 'permission')

        Returns:
            True if error matches the type
        """
        error_msg_lower = error_msg.lower()
        error_type_lower = error_type.lower()
        return error_type_lower in error_msg_lower

    @staticmethod
    def truncate_error(error_msg: str, max_length: int = 200) -> str:
        """
        Truncate long error messages for logging.

        Args:
            error_msg: Error message to truncate
            max_length: Maximum length (default: 200)

        Returns:
            Truncated error message
        """
        if len(error_msg) <= max_length:
            return error_msg

        return error_msg[:max_length] + "..."


class SafeExecutor:
    """Utilities for safe execution with error handling and logging."""

    @staticmethod
    async def try_execute_async(
        func,
        *args,
        default_result: Any = None,
        error_message: str = "Operation failed",
        log_errors: bool = True,
        **kwargs
    ) -> tuple[Any, Optional[Exception]]:
        """
        Safely execute an async function with error handling.

        Args:
            func: Async function to execute
            *args: Positional arguments for the function
            default_result: Default value to return on error
            error_message: Custom error message for logging
            log_errors: Whether to log errors
            **kwargs: Keyword arguments for the function

        Returns:
            Tuple of (result, error). If successful, error is None.
            If failed, returns (default_result, exception)
        """
        try:
            result = await func(*args, **kwargs)
            return result, None
        except Exception as e:
            if log_errors:
                logger.error(
                    error_message,
                    error=str(e),
                    error_type=type(e).__name__
                )
            return default_result, e

    @staticmethod
    def try_execute(
        func,
        *args,
        default_result: Any = None,
        error_message: str = "Operation failed",
        log_errors: bool = True,
        **kwargs
    ) -> tuple[Any, Optional[Exception]]:
        """
        Safely execute a synchronous function with error handling.

        Args:
            func: Function to execute
            *args: Positional arguments for the function
            default_result: Default value to return on error
            error_message: Custom error message for logging
            log_errors: Whether to log errors
            **kwargs: Keyword arguments for the function

        Returns:
            Tuple of (result, error). If successful, error is None.
            If failed, returns (default_result, exception)
        """
        try:
            result = func(*args, **kwargs)
            return result, None
        except Exception as e:
            if log_errors:
                logger.error(
                    error_message,
                    error=str(e),
                    error_type=type(e).__name__
                )
            return default_result, e


def create_error_response(
    message: str,
    details: Optional[Dict[str, Any]] = None,
    user_friendly: Optional[str] = None
) -> Dict[str, Any]:
    """
    Create a standardized error response dictionary.

    Args:
        message: Technical error message
        details: Optional additional error details
        user_friendly: Optional user-friendly error message

    Returns:
        Standardized error dictionary
    """
    error_dict = {"message": message}

    if details:
        error_dict.update(details)

    if user_friendly:
        error_dict["user_friendly_message"] = user_friendly

    return error_dict


def enrich_error_with_suggestions(
    error_dict: Dict[str, Any],
    suggestions: list[str]
) -> Dict[str, Any]:
    """
    Add suggestions to an error dictionary.

    Args:
        error_dict: Error dictionary to enrich
        suggestions: List of suggestion strings

    Returns:
        Enriched error dictionary (modifies in place and returns)
    """
    error_dict["suggestions"] = suggestions
    return error_dict
