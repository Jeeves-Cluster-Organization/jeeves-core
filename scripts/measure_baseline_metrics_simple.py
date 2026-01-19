#!/usr/bin/env python
"""Simplified baseline metrics collection (Windows-compatible).

This is a lightweight version that skips potentially hanging operations.
"""

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path


def collect_metrics(output_file: str = "baseline_metrics.json"):
    """Collect basic metrics without running full test suite."""
    print("=" * 60)
    print("7-Agent Framework: Baseline Metrics Collection (Simple)")
    print("=" * 60)
    print()

    project_root = Path(__file__).parent.parent
    metrics = {
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "version": "phase_0_baseline",
    }

    # Code metrics
    print("📏 Collecting code metrics...")
    python_files = list(project_root.rglob("*.py"))
    python_files = [
        f for f in python_files
        if not any(part.startswith('.') for part in f.parts)
        and 'venv' not in f.parts
        and '__pycache__' not in f.parts
    ]

    total_loc = 0
    files_by_module = {}

    for file in python_files:
        try:
            total_loc += len(file.read_text(encoding='utf-8', errors='ignore').splitlines())
            if len(file.parts) > 0:
                module = file.parts[0]
                files_by_module[module] = files_by_module.get(module, 0) + 1
        except (OSError, UnicodeDecodeError):
            # Skip files that can't be read
            pass

    metrics["code_metrics"] = {
        "total_python_files": len(python_files),
        "total_lines_of_code": total_loc,
        "files_by_module": files_by_module,
    }
    print(f"  ✓ Python files: {len(python_files)}")
    print(f"  ✓ Lines of code: {total_loc:,}")

    # Database size
    print("💾 Checking database...")
    db_path = project_root / "assistant.db"
    db_size_mb = 0
    if db_path.exists():
        try:
            db_size_mb = round(db_path.stat().st_size / (1024 * 1024), 2)
        except OSError:
            # Couldn't stat file
            pass

    metrics["system"] = {
        "database_size_mb": db_size_mb,
    }
    print(f"  ✓ Database size: {db_size_mb} MB")

    # Performance baselines (mock values)
    print("⚡ Setting performance baselines...")
    metrics["performance"] = {
        "note": "Baseline values - measure actual after implementation",
        "p95_latency_ms": 1500,
        "throughput_requests_per_sec": 0.67,
    }
    print(f"  ✓ Target p95 latency: 1500ms")

    # Save metrics
    output_path = project_root / output_file
    with open(output_path, 'w') as f:
        json.dump(metrics, f, indent=2)

    print()
    print("=" * 60)
    print("✅ Baseline metrics collected successfully")
    print(f"📊 Results saved to: {output_path}")
    print("=" * 60)
    print()
    print("Note: Run full test suite separately:")
    print("  pytest -v --cov=. --cov-report=html")
    print()

    return metrics


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Collect baseline metrics (simplified version)"
    )
    parser.add_argument(
        "--output",
        "-o",
        default="baseline_metrics.json",
        help="Output file (default: baseline_metrics.json)"
    )
    args = parser.parse_args()

    collect_metrics(args.output)
