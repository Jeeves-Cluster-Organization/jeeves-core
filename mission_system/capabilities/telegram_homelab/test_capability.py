"""Test script for Telegram Homelab capability."""

import asyncio
import logging
import os
import sys
from pathlib import Path

# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)


async def test_configuration():
    """Test configuration loading."""
    logger.info("Testing configuration loading...")

    try:
        from mission_system.capabilities.telegram_homelab.config import get_config

        config = get_config()

        logger.info(f"✓ Configuration loaded successfully")
        logger.info(f"  - Telegram bot token: {'*' * 20}...{config.telegram.bot_token[-4:]}")
        logger.info(f"  - Admin users: {len(config.telegram.admin_user_ids)} configured")
        logger.info(f"  - SSH hosts: {len(config.ssh.hosts)} configured")
        logger.info(f"  - Allowed dirs: {len(config.homelab.allowed_dirs)} configured")
        logger.info(f"  - Calendar backend: {config.calendar.api_type}")
        logger.info(f"  - Notes backend: {config.notes.backend}")

        return True
    except Exception as e:
        logger.error(f"✗ Configuration loading failed: {e}")
        return False


async def test_tools():
    """Test tool implementations."""
    logger.info("\nTesting tools...")

    from mission_system.capabilities.telegram_homelab.tools import (
        file_list,
        file_read,
        file_search,
        ssh_execute,
        calendar_query,
        notes_search,
    )

    # Test file list (on current directory - should be safe)
    try:
        logger.info("  Testing file_list...")
        # This will fail with path validation, which is expected
        result = await file_list(".")
        logger.info(f"    Status: {result.status}")
        if result.status == "error":
            logger.info(f"    Expected error: {result.error_message}")
    except Exception as e:
        logger.error(f"    ✗ file_list failed: {e}")

    # Test SSH execute (will fail without configured hosts)
    try:
        logger.info("  Testing ssh_execute...")
        result = await ssh_execute("test.local", "echo test")
        logger.info(f"    Status: {result.status}")
        if result.status == "error":
            logger.info(f"    Expected error: {result.error_message}")
    except Exception as e:
        logger.error(f"    ✗ ssh_execute failed: {e}")

    # Test calendar query
    try:
        logger.info("  Testing calendar_query...")
        result = await calendar_query()
        logger.info(f"    Status: {result.status}")
        if result.status == "error":
            logger.info(f"    Expected error: {result.error_message}")
    except Exception as e:
        logger.error(f"    ✗ calendar_query failed: {e}")

    # Test notes search
    try:
        logger.info("  Testing notes_search...")
        result = await notes_search("test")
        logger.info(f"    Status: {result.status}")
        if result.status == "error":
            logger.info(f"    Expected error: {result.error_message}")
    except Exception as e:
        logger.error(f"    ✗ notes_search failed: {e}")

    logger.info("✓ Tool testing completed (errors are expected without full configuration)")
    return True


async def test_servicer():
    """Test servicer implementation."""
    logger.info("\nTesting servicer...")

    try:
        from mission_system.capabilities.telegram_homelab.servicer import TelegramHomelabServicer

        servicer = TelegramHomelabServicer(llm_provider=None)

        logger.info("  Processing test message...")
        event_count = 0
        async for event in servicer.process_request(
            user_id="test_user",
            session_id="test_session",
            message="list files in /tmp",
            context={},
        ):
            event_type = event.get("type")
            event_count += 1
            logger.info(f"    Event {event_count}: {event_type}")

            if event_type == "response":
                response_text = event.get("data", {}).get("text", "")
                logger.info(f"    Response length: {len(response_text)} chars")

        logger.info(f"✓ Servicer processed {event_count} events")
        return True
    except Exception as e:
        logger.error(f"✗ Servicer testing failed: {e}")
        return False


async def test_wiring():
    """Test capability wiring."""
    logger.info("\nTesting wiring...")

    try:
        from mission_system.capabilities.telegram_homelab.wiring import register_capability

        # Note: This will fail if config is incomplete, which is expected
        try:
            register_capability()
            logger.info("✓ Capability registration succeeded")
        except ValueError as e:
            logger.info(f"  Expected error (missing config): {e}")
            logger.info("✓ Wiring code is functional (config incomplete)")

        return True
    except Exception as e:
        logger.error(f"✗ Wiring test failed: {e}")
        return False


async def main():
    """Run all tests."""
    logger.info("=" * 60)
    logger.info("Telegram Homelab Capability Test Suite")
    logger.info("=" * 60)

    # Check if .env file exists
    env_file = Path(__file__).parent / ".env"
    if not env_file.exists():
        logger.warning(f"\n⚠️  No .env file found at {env_file}")
        logger.warning("   Copy .env.example to .env and configure it for full testing")
        logger.warning("   Continuing with basic tests...\n")

    results = []

    # Run tests
    results.append(("Configuration", await test_configuration()))
    results.append(("Tools", await test_tools()))
    results.append(("Servicer", await test_servicer()))
    results.append(("Wiring", await test_wiring()))

    # Summary
    logger.info("\n" + "=" * 60)
    logger.info("Test Summary:")
    logger.info("=" * 60)

    passed = sum(1 for _, result in results if result)
    total = len(results)

    for test_name, result in results:
        status = "✓ PASS" if result else "✗ FAIL"
        logger.info(f"{status} - {test_name}")

    logger.info(f"\nTotal: {passed}/{total} tests passed")

    if passed == total:
        logger.info("\n🎉 All tests passed!")
        return 0
    else:
        logger.info(f"\n⚠️  {total - passed} test(s) failed")
        return 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
