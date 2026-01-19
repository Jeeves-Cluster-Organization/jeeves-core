#!/usr/bin/env python3
"""
Code Indexing Script - Index repository code for RAG-based semantic search.

This script indexes all code files in a repository, generating embeddings
and storing them in the code_index table for semantic search.

Usage:
    python scripts/database/index_codebase.py [OPTIONS]

Options:
    --repo-path PATH    Path to repository (default: REPO_PATH env var)
    --force             Force re-index all files (ignore content hashes)
    --clean             Remove stale entries for deleted files
    --stats             Show current index statistics and exit
    --quiet             Suppress progress output

Examples:
    # Index the repository specified by REPO_PATH
    python scripts/database/index_codebase.py

    # Index a specific repository
    python scripts/database/index_codebase.py --repo-path /path/to/repo

    # Force re-index everything
    python scripts/database/index_codebase.py --force

    # Clean up and then index
    python scripts/database/index_codebase.py --clean

    # Just show stats
    python scripts/database/index_codebase.py --stats
"""

import argparse
import asyncio
import os
import sys
from pathlib import Path

# Add project root to path
project_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(project_root))


async def main():
    """Main entry point for code indexing."""
    parser = argparse.ArgumentParser(
        description="Index repository code for RAG-based semantic search"
    )
    parser.add_argument(
        "--repo-path",
        type=str,
        default=os.environ.get("REPO_PATH"),
        help="Path to repository (default: REPO_PATH env var)",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Force re-index all files (ignore content hashes)",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="Remove stale entries for deleted files before indexing",
    )
    parser.add_argument(
        "--stats",
        action="store_true",
        help="Show current index statistics and exit",
    )
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="Suppress progress output",
    )

    args = parser.parse_args()

    # Validate repo path
    if not args.repo_path:
        print("Error: Repository path required. Set REPO_PATH env var or use --repo-path")
        sys.exit(1)

    repo_path = Path(args.repo_path).resolve()
    if not repo_path.exists():
        print(f"Error: Repository path does not exist: {repo_path}")
        sys.exit(1)

    if not args.quiet:
        print(f"Repository: {repo_path}")
        print("-" * 60)

    # Initialize components
    try:
        from jeeves_avionics.settings import get_settings
        from jeeves_avionics.database.factory import create_database_client
        from jeeves_memory_module.services.embedding_service import EmbeddingService
        from jeeves_memory_module.services.code_indexer import CodeIndexer

        settings = get_settings()

        if not args.quiet:
            print("Connecting to database...")

        db = await create_database_client(settings)

        if not args.quiet:
            print("Initializing embedding service...")

        embedding_service = EmbeddingService()

        if not args.quiet:
            print("Initializing code indexer...")

        indexer = CodeIndexer(
            postgres_client=db,
            embedding_service=embedding_service,
        )

    except Exception as e:
        print(f"Error initializing components: {e}")
        sys.exit(1)

    # Stats only mode
    if args.stats:
        if not args.quiet:
            print("\nIndex Statistics:")
            print("-" * 40)

        stats = await indexer.get_stats()

        if "error" in stats:
            print(f"Error: {stats['error']}")
            sys.exit(1)

        print(f"Total files indexed: {stats['total_files']}")
        print(f"Files with embeddings: {stats['embedded_files']}")
        print(f"Total lines of code: {stats['total_lines']:,}")
        print(f"Total bytes: {stats['total_bytes']:,}")
        print(f"Last indexed: {stats['last_indexed'] or 'Never'}")
        print(f"\nBy language:")
        for lang, count in sorted(stats.get('by_language', {}).items(), key=lambda x: -x[1]):
            print(f"  {lang}: {count} files")

        return

    # Clean stale entries
    if args.clean:
        if not args.quiet:
            print("\nCleaning stale entries...")

        removed = await indexer.remove_stale_entries(str(repo_path))

        if not args.quiet:
            print(f"Removed {removed} stale entries")

    # Index repository
    if not args.quiet:
        print("\nIndexing repository...")
        print("-" * 40)

    def progress_callback(current, total, file_path):
        if not args.quiet:
            pct = (current / total) * 100
            # Clear line and print progress
            sys.stdout.write(f"\r[{pct:5.1f}%] {current}/{total} - {file_path[:60]:<60}")
            sys.stdout.flush()

    try:
        result = await indexer.index_repository(
            repo_path=str(repo_path),
            force=args.force,
            progress_callback=progress_callback,
        )

        if not args.quiet:
            print("\n" + "-" * 40)
            print("\nIndexing Complete!")
            print(f"  Total files scanned: {result['total_files']}")
            print(f"  Files indexed: {result['indexed']}")
            print(f"  Files skipped (unchanged): {result['skipped']}")
            print(f"  Files failed: {result['failed']}")

        if result['failed'] > 0:
            print(f"\nWarning: {result['failed']} files failed to index")

    except Exception as e:
        print(f"\nError during indexing: {e}")
        sys.exit(1)

    # Show final stats
    if not args.quiet:
        print("\nFinal Index Statistics:")
        print("-" * 40)

        stats = await indexer.get_stats()
        print(f"Total files indexed: {stats.get('total_files', 0)}")
        print(f"Files with embeddings: {stats.get('embedded_files', 0)}")


if __name__ == "__main__":
    asyncio.run(main())
