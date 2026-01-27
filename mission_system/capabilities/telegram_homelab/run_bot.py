#!/usr/bin/env python3
"""Standalone script to run the Telegram Homelab bot."""

import asyncio
import logging
import os
import sys
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent.parent.parent))


def setup_logging(debug: bool = False):
    """Setup logging configuration."""
    level = logging.DEBUG if debug else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
        handlers=[
            logging.StreamHandler(),
            logging.FileHandler("telegram_bot.log"),
        ],
    )


async def main():
    """Main entry point."""
    # Parse arguments
    debug = "--debug" in sys.argv

    # Setup logging
    setup_logging(debug)
    logger = logging.getLogger(__name__)

    logger.info("=" * 60)
    logger.info("Telegram Homelab Bot")
    logger.info("=" * 60)

    # Check for .env file
    env_file = Path(__file__).parent / ".env"
    if env_file.exists():
        logger.info(f"Loading configuration from {env_file}")
        try:
            from dotenv import load_dotenv
            load_dotenv(env_file)
        except ImportError:
            logger.warning("python-dotenv not installed, using system environment variables")
    else:
        logger.warning(f"No .env file found at {env_file}")
        logger.warning("Copy .env.example to .env and configure it")
        logger.warning("Using system environment variables...")

    # Check required environment variables
    required_vars = ["TELEGRAM_BOT_TOKEN"]
    missing_vars = [var for var in required_vars if not os.getenv(var)]

    if missing_vars:
        logger.error(f"Missing required environment variables: {', '.join(missing_vars)}")
        logger.error("Please set these variables in .env or your environment")
        return 1

    try:
        # Import and run bot
        from mission_system.capabilities.telegram_homelab import run_bot

        logger.info("Starting Telegram bot...")
        logger.info("Press Ctrl+C to stop")
        logger.info("=" * 60)

        await run_bot()

        return 0

    except KeyboardInterrupt:
        logger.info("\nBot stopped by user")
        return 0
    except Exception as e:
        logger.exception(f"Fatal error: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
