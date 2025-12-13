#!/usr/bin/env python3
"""
Utility script to drop and recreate the PostgreSQL database with
the current schema. Useful when schema changes require a fresh database.

Usage:
    python scripts/database/reset.py
    python scripts/database/reset.py --skip-verify
"""

from __future__ import annotations

import argparse
import asyncio
import sys
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(PROJECT_ROOT))

from jeeves_mission_system.scripts.database.init import DatabaseInitializer  # noqa: E402


async def reset_database(verify: bool) -> None:
    """Drop and recreate the PostgreSQL database."""
    print("=" * 70)
    print("PostgreSQL Database Reset")
    print("=" * 70)

    initializer = DatabaseInitializer(force=True, verify=verify)
    success = await initializer.run()

    if not success:
        raise SystemExit(1)

    print("[OK] Database reset complete")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Reset the PostgreSQL database using the latest schema.",
    )
    parser.add_argument(
        "--skip-verify",
        action="store_true",
        help="Skip schema verification after re-initialization.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    verify_schema = not args.skip_verify

    try:
        asyncio.run(reset_database(verify_schema))
    except KeyboardInterrupt:
        print("\n[WARN] Database reset aborted by user")
        raise SystemExit(1)


if __name__ == "__main__":
    main()
