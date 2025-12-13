"""Console utilities for CLI scripts.

Provides ANSI color codes and formatted print functions for consistent
script output across the codebase. Consolidates duplicate implementations
from multiple script files.

Usage:
    from scripts.lib.console import Colors, print_header, print_success, print_error

    print_header("My Script")
    print_success("Operation completed")
    print_error("Something went wrong")
"""


class Colors:
    """ANSI color codes for terminal output."""
    GREEN = '\033[0;32m'
    RED = '\033[0;31m'
    BLUE = '\033[0;34m'
    YELLOW = '\033[1;33m'
    CYAN = '\033[0;36m'
    BOLD = '\033[1m'
    NC = '\033[0m'  # No Color / Reset


def print_header(text: str, width: int = 80) -> None:
    """Print a formatted header with box borders.

    Args:
        text: Header text
        width: Box width (default 80)
    """
    print("=" * width)
    print(f"{Colors.BOLD}{text}{Colors.NC}")
    print("=" * width)
    print()


def print_success(text: str) -> None:
    """Print a success message with green checkmark.

    Args:
        text: Message to print
    """
    print(f"{Colors.GREEN}✓{Colors.NC} {text}")


def print_error(text: str) -> None:
    """Print an error message with red X.

    Args:
        text: Message to print
    """
    print(f"{Colors.RED}✗{Colors.NC} {text}")


def print_info(text: str) -> None:
    """Print an info message with blue marker.

    Args:
        text: Message to print
    """
    print(f"{Colors.BLUE}ℹ{Colors.NC} {text}")


def print_warning(text: str) -> None:
    """Print a warning message with yellow marker.

    Args:
        text: Message to print
    """
    print(f"{Colors.YELLOW}⚠{Colors.NC} {text}")


def check(name: str, condition: bool, error_msg: str = "") -> bool:
    """Check a condition and print result.

    Args:
        name: Name of the check
        condition: Boolean condition to check
        error_msg: Optional error message if check fails

    Returns:
        The condition value (for chaining)
    """
    if condition:
        print_success(name)
    else:
        print_error(name)
        if error_msg:
            print(f"  Error: {error_msg}")
    return condition


__all__ = [
    "Colors",
    "print_header",
    "print_success",
    "print_error",
    "print_info",
    "print_warning",
    "check",
]
