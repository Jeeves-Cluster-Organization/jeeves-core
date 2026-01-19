"""Dependency vulnerability scanning with pip-audit.

This tool scans Python dependencies for known security vulnerabilities
using pip-audit and the OSV database.

Usage:
    python scripts/maintenance/dependency_scan.py
    python scripts/maintenance/dependency_scan.py --fix
    python scripts/maintenance/dependency_scan.py --output report.json

Exit codes:
    0: No vulnerabilities found
    1: Vulnerabilities found (in strict mode)
    2: Scan error
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import List, Optional

from jeeves_avionics.logging import get_current_logger


@dataclass
class Vulnerability:
    """A single vulnerability finding."""

    package: str
    version: str
    vulnerability_id: str
    description: str
    severity: Optional[str] = None
    fixed_version: Optional[str] = None


@dataclass
class ScanResult:
    """Results from dependency scan."""

    total_vulnerabilities: int
    critical: int
    high: int
    medium: int
    low: int
    unknown: int
    vulnerabilities: List[Vulnerability]
    scan_succeeded: bool
    error_message: Optional[str] = None


def parse_pip_audit_json(output: str) -> ScanResult:
    """Parse pip-audit JSON output into ScanResult."""
    try:
        data = json.loads(output)
    except json.JSONDecodeError as exc:
        return ScanResult(
            total_vulnerabilities=0,
            critical=0,
            high=0,
            medium=0,
            low=0,
            unknown=0,
            vulnerabilities=[],
            scan_succeeded=False,
            error_message=f"Failed to parse pip-audit output: {exc}",
        )

    vulnerabilities: List[Vulnerability] = []
    severity_counts = {"critical": 0, "high": 0, "medium": 0, "low": 0, "unknown": 0}

    # pip-audit JSON format: {"dependencies": [...]}
    for dep in data.get("dependencies", []):
        package = dep.get("name", "unknown")
        version = dep.get("version", "unknown")

        for vuln in dep.get("vulns", []):
            vuln_id = vuln.get("id", "unknown")
            description = vuln.get("description", "No description")
            severity = vuln.get("severity", "unknown").lower()
            fixed_version = vuln.get("fix_versions", [None])[0]

            vulnerabilities.append(
                Vulnerability(
                    package=package,
                    version=version,
                    vulnerability_id=vuln_id,
                    description=description,
                    severity=severity,
                    fixed_version=fixed_version,
                )
            )

            # Count by severity
            if severity in severity_counts:
                severity_counts[severity] += 1
            else:
                severity_counts["unknown"] += 1

    return ScanResult(
        total_vulnerabilities=len(vulnerabilities),
        critical=severity_counts["critical"],
        high=severity_counts["high"],
        medium=severity_counts["medium"],
        low=severity_counts["low"],
        unknown=severity_counts["unknown"],
        vulnerabilities=vulnerabilities,
        scan_succeeded=True,
    )


def run_pip_audit(
    *,
    requirements_file: Optional[Path] = None,
    fix: bool = False,
    ignore_vulns: Optional[List[str]] = None,
) -> ScanResult:
    """Run pip-audit and return results."""
    _logger = get_current_logger()
    cmd = ["pip-audit", "--format=json"]

    if requirements_file:
        cmd.extend(["--requirement", str(requirements_file)])

    if fix:
        cmd.append("--fix")

    if ignore_vulns:
        for vuln_id in ignore_vulns:
            cmd.extend(["--ignore-vuln", vuln_id])

    _logger.info("running_pip_audit", command=" ".join(cmd))

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=300,  # 5 minutes timeout
        )

        # pip-audit returns 0 on success, 1 if vulnerabilities found
        if result.returncode in (0, 1):
            return parse_pip_audit_json(result.stdout)
        else:
            return ScanResult(
                total_vulnerabilities=0,
                critical=0,
                high=0,
                medium=0,
                low=0,
                unknown=0,
                vulnerabilities=[],
                scan_succeeded=False,
                error_message=f"pip-audit failed: {result.stderr}",
            )

    except FileNotFoundError:
        return ScanResult(
            total_vulnerabilities=0,
            critical=0,
            high=0,
            medium=0,
            low=0,
            unknown=0,
            vulnerabilities=[],
            scan_succeeded=False,
            error_message="pip-audit not installed. Run: pip install pip-audit",
        )
    except subprocess.TimeoutExpired:
        return ScanResult(
            total_vulnerabilities=0,
            critical=0,
            high=0,
            medium=0,
            low=0,
            unknown=0,
            vulnerabilities=[],
            scan_succeeded=False,
            error_message="pip-audit timed out after 5 minutes",
        )
    except Exception as exc:
        return ScanResult(
            total_vulnerabilities=0,
            critical=0,
            high=0,
            medium=0,
            low=0,
            unknown=0,
            vulnerabilities=[],
            scan_succeeded=False,
            error_message=f"Unexpected error: {exc}",
        )


def print_results(result: ScanResult, *, verbose: bool = False) -> None:
    """Print scan results in human-readable format."""
    print("\n" + "=" * 60)
    print("DEPENDENCY VULNERABILITY SCAN RESULTS")
    print("=" * 60)

    if not result.scan_succeeded:
        print(f"\n❌ Scan failed: {result.error_message}")
        print("=" * 60)
        return

    print(f"\nTotal vulnerabilities: {result.total_vulnerabilities}")
    print()
    print("By severity:")
    print(f"  Critical: {result.critical}")
    print(f"  High:     {result.high}")
    print(f"  Medium:   {result.medium}")
    print(f"  Low:      {result.low}")
    print(f"  Unknown:  {result.unknown}")
    print()

    if result.total_vulnerabilities == 0:
        print("[OK] No vulnerabilities found!")
    else:
        print(f"[WARN]️  {result.total_vulnerabilities} vulnerabilities detected")

        if verbose and result.vulnerabilities:
            print("\nDetailed findings:")
            print("-" * 60)
            for i, vuln in enumerate(result.vulnerabilities, 1):
                print(f"\n{i}. {vuln.package} {vuln.version}")
                print(f"   ID: {vuln.vulnerability_id}")
                print(f"   Severity: {vuln.severity or 'unknown'}")
                if vuln.fixed_version:
                    print(f"   Fixed in: {vuln.fixed_version}")
                print(f"   Description: {vuln.description[:100]}...")

    print("=" * 60)


def main() -> int:
    """Run dependency scan from command line."""
    parser = argparse.ArgumentParser(
        description="Scan dependencies for security vulnerabilities"
    )
    parser.add_argument(
        "--requirements",
        type=Path,
        default=Path("requirements.txt"),
        help="Path to requirements.txt (default: requirements.txt)",
    )
    parser.add_argument(
        "--fix",
        action="store_true",
        help="Attempt to automatically fix vulnerabilities",
    )
    parser.add_argument(
        "--ignore",
        action="append",
        dest="ignore_vulns",
        help="Vulnerability IDs to ignore (can be specified multiple times)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Save results to JSON file",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Print detailed vulnerability information",
    )
    parser.add_argument(
        "--strict",
        action="store_true",
        help="Exit with code 1 if any vulnerabilities found",
    )

    args = parser.parse_args()

    # Run scan
    result = run_pip_audit(
        requirements_file=args.requirements if args.requirements.exists() else None,
        fix=args.fix,
        ignore_vulns=args.ignore_vulns,
    )

    # Print results
    print_results(result, verbose=args.verbose)

    # Save to file if requested
    if args.output:
        output_data = {
            "scan_result": asdict(result),
            "timestamp": "2025-11-08",
        }
        args.output.write_text(json.dumps(output_data, indent=2))
        print(f"\nResults saved to: {args.output}")

    # Determine exit code
    _logger = get_current_logger()
    if not result.scan_succeeded:
        _logger.error("dependency_scan_failed", error=result.error_message)
        return 2

    if args.strict and result.total_vulnerabilities > 0:
        _logger.warning(
            "dependency_scan_vulnerabilities_found",
            total=result.total_vulnerabilities,
            critical=result.critical,
            high=result.high,
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
