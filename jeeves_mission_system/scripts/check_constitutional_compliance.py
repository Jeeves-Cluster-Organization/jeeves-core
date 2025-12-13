#!/usr/bin/env python3
"""
Constitutional Compliance Checker - Enforces all CONSTITUTION.md rules.

This script checks for violations of constitutional rules across all components:
- Core Engine Constitution (v2.0)
- Avionics Constitution (v2.0)
- Mission System Constitution (v2.3)
- Capability Constitution (v1.2)

CHECKS PERFORMED:

1. CORE-R5: Core Engine must not use os.getenv() directly
   - Environment variable parsing must be in avionics/mission_system

2. RETRY-BOUNDS: Retry limits must align with constitution
   - Mission System R3 says "max 2 retries per step"
   - max_attempts=3 means 1 initial + 2 retries (compliant)
   - Anything > 3 is a violation

3. INDEX-MD: Every major directory should have INDEX.md
   - Mission System R6 requires documentation

4. SECURITY: Avionics S1-S3 security rules
   - S1: No secrets in logs
   - S2: No hardcoded API keys
   - S3: No passwords in connection strings (except tests)

5. CAPABILITY-IMPORT: Apps must use mission_system.contracts
   - Capability P5 prohibits direct core_engine/avionics imports

Usage:
    python scripts/check_constitutional_compliance.py
    python scripts/check_constitutional_compliance.py --verbose
    python scripts/check_constitutional_compliance.py --ci  # Exit 1 on violations
    python scripts/check_constitutional_compliance.py --check core-r5  # Single check

Returns exit code 0 if compliant, 1 if violations found.
"""

import argparse
import ast
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import List, Optional, Set, Tuple


@dataclass
class Violation:
    """A constitutional violation."""
    file: str
    line: int
    rule: str
    evidence: str
    message: str
    severity: str = "MEDIUM"  # LOW, MEDIUM, HIGH, CRITICAL


class ConstitutionalComplianceChecker:
    """Check all constitutional compliance rules."""

    # Directories that MUST have INDEX.md (priority list from audit)
    REQUIRED_INDEX_DIRS = [
        "jeeves_core_engine/agents",
        "jeeves_core_engine/pipeline",
        "jeeves_core_engine/config",
        "jeeves_avionics/llm",
        "jeeves_avionics/memory",
        "jeeves_avionics/database",
        "jeeves_avionics/gateway",
        "jeeves_mission_system/orchestrator",
        "jeeves_mission_system/services",
        "jeeves-capability-code-analyser/agents",
        "jeeves-capability-code-analyser/tools",
        "jeeves-capability-code-analyser/orchestration",
    ]

    # Patterns that indicate secrets being logged
    SECRET_LOG_PATTERNS = [
        r'logger\.(info|debug|warn|error|warning|critical).*api.?key',
        r'logger\.(info|debug|warn|error|warning|critical).*password',
        r'logger\.(info|debug|warn|error|warning|critical).*secret',
        r'log\.(info|debug|warn|error|warning|critical).*api.?key',
        r'log\.(info|debug|warn|error|warning|critical).*password',
    ]

    # Patterns for hardcoded credentials
    HARDCODED_CREDENTIAL_PATTERNS = [
        r'api_key\s*=\s*["\'][a-zA-Z0-9]{20,}["\']',  # Looks like a real key
        r'password\s*=\s*["\'][^"\']{8,}["\']',  # Password with actual content
        r'secret\s*=\s*["\'][a-zA-Z0-9]{20,}["\']',  # Secret with real value
    ]

    # Files/paths to exclude from checks
    TEST_EXCLUSIONS = [
        "/tests/",
        "/test_",
        "_test.py",
        "conftest.py",
        "/fixtures/",
    ]

    def __init__(self, root_dir: Path, verbose: bool = False):
        self.root_dir = root_dir
        self.verbose = verbose
        self.violations: List[Violation] = []

    def check_all(self) -> List[Violation]:
        """Run all constitutional checks."""
        self.violations = []

        self._check_core_r5_no_getenv()
        self._check_retry_bounds()
        self._check_index_md_coverage()
        self._check_security_rules()
        self._check_capability_imports()

        return self.violations

    def check_specific(self, check_name: str) -> List[Violation]:
        """Run a specific check by name."""
        self.violations = []

        checks = {
            "core-r5": self._check_core_r5_no_getenv,
            "retry-bounds": self._check_retry_bounds,
            "index-md": self._check_index_md_coverage,
            "security": self._check_security_rules,
            "capability-import": self._check_capability_imports,
        }

        if check_name not in checks:
            print(f"Unknown check: {check_name}")
            print(f"Available checks: {', '.join(checks.keys())}")
            sys.exit(1)

        checks[check_name]()
        return self.violations

    def _check_core_r5_no_getenv(self) -> None:
        """CORE-R5: Core Engine must not use os.getenv() directly.

        Constitution Rule: "No environment variables directly (pass via config)"
        """
        core_dir = self.root_dir / "jeeves_core_engine"
        if not core_dir.exists():
            return

        for py_file in core_dir.rglob("*.py"):
            # Skip test files
            if self._is_test_file(py_file):
                continue

            try:
                content = py_file.read_text(encoding='utf-8')
                lines = content.split('\n')

                for i, line in enumerate(lines, 1):
                    # Check for os.getenv calls
                    if 'os.getenv' in line or 'os.environ' in line:
                        self.violations.append(Violation(
                            file=str(py_file.relative_to(self.root_dir)),
                            line=i,
                            rule="CORE-R5",
                            evidence=line.strip()[:80],
                            message="Core Engine must not use os.getenv() directly. "
                                    "Environment parsing must be in avionics/mission_system.",
                            severity="HIGH",
                        ))

            except (UnicodeDecodeError, IOError) as e:
                if self.verbose:
                    print(f"Warning: Could not read {py_file}: {e}", file=sys.stderr)

    def _check_retry_bounds(self) -> None:
        """RETRY-BOUNDS: Retry limits must align with constitution.

        Constitution Rule (Mission System R3): "Max 2 retries per step"
        Interpretation: max_attempts=3 (1 initial + 2 retries) is compliant
        """
        patterns = [
            (r'max_attempts\s*[=:]\s*(\d+)', 'max_attempts'),
            (r'MAX_RETRY_ATTEMPTS\s*=\s*(\d+)', 'MAX_RETRY_ATTEMPTS'),
            (r'max_retries\s*[=:]\s*(\d+)', 'max_retries'),
        ]

        for component in ["jeeves_core_engine", "jeeves_avionics", "jeeves_mission_system"]:
            component_dir = self.root_dir / component
            if not component_dir.exists():
                continue

            for py_file in component_dir.rglob("*.py"):
                if self._is_test_file(py_file):
                    continue

                try:
                    content = py_file.read_text(encoding='utf-8')
                    lines = content.split('\n')

                    for i, line in enumerate(lines, 1):
                        for pattern, name in patterns:
                            match = re.search(pattern, line)
                            if match:
                                value = int(match.group(1))
                                # max_attempts > 3 is a violation (1 + 2 retries = 3)
                                # Except for AGGRESSIVE_RETRY_POLICY which is documented
                                if value > 3 and 'AGGRESSIVE' not in line:
                                    self.violations.append(Violation(
                                        file=str(py_file.relative_to(self.root_dir)),
                                        line=i,
                                        rule="RETRY-BOUNDS",
                                        evidence=f"{name}={value}",
                                        message=f"Retry limit exceeds constitutional bound. "
                                                f"Max 2 retries means max_attempts=3. Found {value}.",
                                        severity="MEDIUM",
                                    ))

                except (UnicodeDecodeError, IOError) as e:
                    if self.verbose:
                        print(f"Warning: Could not read {py_file}: {e}", file=sys.stderr)

    def _check_index_md_coverage(self) -> None:
        """INDEX-MD: Every major directory should have INDEX.md.

        Constitution Rule (Mission System R6): "Every directory must have INDEX.md"
        This check focuses on priority directories.
        """
        for dir_path in self.REQUIRED_INDEX_DIRS:
            full_path = self.root_dir / dir_path
            index_path = full_path / "INDEX.md"

            if full_path.exists() and not index_path.exists():
                self.violations.append(Violation(
                    file=str(dir_path),
                    line=0,
                    rule="INDEX-MD",
                    evidence=f"Missing: {dir_path}/INDEX.md",
                    message=f"Directory {dir_path} must have INDEX.md per Mission System R6.",
                    severity="LOW",
                ))

    def _check_security_rules(self) -> None:
        """SECURITY: Avionics S1-S3 security rules.

        S1: Never Log Secrets
        S2: Environment Variables Only for API Keys
        S3: No passwords in connection strings
        """
        for component in ["jeeves_core_engine", "jeeves_avionics", "jeeves_mission_system",
                          "jeeves-capability-code-analyser"]:
            component_dir = self.root_dir / component
            if not component_dir.exists():
                continue

            for py_file in component_dir.rglob("*.py"):
                # Skip test files for credential checks (test fixtures allowed)
                is_test = self._is_test_file(py_file)

                try:
                    content = py_file.read_text(encoding='utf-8')
                    lines = content.split('\n')

                    for i, line in enumerate(lines, 1):
                        # S1: Check for secrets in logs
                        for pattern in self.SECRET_LOG_PATTERNS:
                            if re.search(pattern, line, re.IGNORECASE):
                                self.violations.append(Violation(
                                    file=str(py_file.relative_to(self.root_dir)),
                                    line=i,
                                    rule="SECURITY-S1",
                                    evidence=line.strip()[:80],
                                    message="Never log secrets (API keys, passwords).",
                                    severity="CRITICAL",
                                ))

                        # S2/S3: Check for hardcoded credentials (skip tests)
                        if not is_test:
                            for pattern in self.HARDCODED_CREDENTIAL_PATTERNS:
                                if re.search(pattern, line, re.IGNORECASE):
                                    self.violations.append(Violation(
                                        file=str(py_file.relative_to(self.root_dir)),
                                        line=i,
                                        rule="SECURITY-S2",
                                        evidence=line.strip()[:60] + "...",
                                        message="Hardcoded credentials detected. Use environment variables.",
                                        severity="CRITICAL",
                                    ))

                        # S3: Check for password in connection strings
                        if not is_test and 'postgresql://' in line.lower():
                            if re.search(r'postgresql://[^:]+:[^@]+@', line, re.IGNORECASE):
                                self.violations.append(Violation(
                                    file=str(py_file.relative_to(self.root_dir)),
                                    line=i,
                                    rule="SECURITY-S3",
                                    evidence="postgresql://user:password@...",
                                    message="Password in connection string. Use environment variable.",
                                    severity="HIGH",
                                ))

                except (UnicodeDecodeError, IOError) as e:
                    if self.verbose:
                        print(f"Warning: Could not read {py_file}: {e}", file=sys.stderr)

    def _check_capability_imports(self) -> None:
        """CAPABILITY-IMPORT: Apps must use mission_system.contracts.

        Constitution Rule (Capability P5): Apps MUST import from mission_system.contracts
        """
        capability_dir = self.root_dir / "jeeves-capability-code-analyser"
        if not capability_dir.exists():
            return

        forbidden_patterns = [
            "jeeves_core_engine.",
            "jeeves_avionics.",
        ]

        for py_file in capability_dir.rglob("*.py"):
            # Skip test files - they have documented exceptions
            if self._is_test_file(py_file):
                continue

            try:
                content = py_file.read_text(encoding='utf-8')
                tree = ast.parse(content)

                for node in ast.walk(tree):
                    if isinstance(node, ast.Import):
                        for alias in node.names:
                            for pattern in forbidden_patterns:
                                if alias.name.startswith(pattern.rstrip('.')):
                                    self.violations.append(Violation(
                                        file=str(py_file.relative_to(self.root_dir)),
                                        line=node.lineno,
                                        rule="CAPABILITY-IMPORT",
                                        evidence=f"import {alias.name}",
                                        message=f"Capability must use mission_system.contracts, "
                                                f"not import from {pattern.rstrip('.')} directly.",
                                        severity="MEDIUM",
                                    ))
                    elif isinstance(node, ast.ImportFrom):
                        if node.module:
                            for pattern in forbidden_patterns:
                                if node.module.startswith(pattern.rstrip('.')):
                                    self.violations.append(Violation(
                                        file=str(py_file.relative_to(self.root_dir)),
                                        line=node.lineno,
                                        rule="CAPABILITY-IMPORT",
                                        evidence=f"from {node.module} import ...",
                                        message=f"Capability must use mission_system.contracts, "
                                                f"not import from {pattern.rstrip('.')} directly.",
                                        severity="MEDIUM",
                                    ))

            except (SyntaxError, UnicodeDecodeError, IOError) as e:
                if self.verbose:
                    print(f"Warning: Could not parse {py_file}: {e}", file=sys.stderr)

    def _is_test_file(self, path: Path) -> bool:
        """Check if a file is a test file."""
        path_str = str(path)
        return any(excl in path_str for excl in self.TEST_EXCLUSIONS)


def main():
    parser = argparse.ArgumentParser(description="Check constitutional compliance")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    parser.add_argument("--ci", action="store_true", help="CI mode (exit 1 on violations)")
    parser.add_argument("--root", type=str, default=".", help="Root directory")
    parser.add_argument("--check", type=str, help="Run specific check only")
    parser.add_argument("--severity", type=str, default="LOW",
                        help="Minimum severity to report (LOW, MEDIUM, HIGH, CRITICAL)")
    args = parser.parse_args()

    severity_order = {"LOW": 0, "MEDIUM": 1, "HIGH": 2, "CRITICAL": 3}
    min_severity = severity_order.get(args.severity.upper(), 0)

    root_dir = Path(args.root).resolve()
    checker = ConstitutionalComplianceChecker(root_dir, verbose=args.verbose)

    print("=" * 70)
    print("Constitutional Compliance Checker")
    print("Checking against: Core Engine, Avionics, Mission System, Capability")
    print("=" * 70)
    print()

    if args.check:
        violations = checker.check_specific(args.check)
    else:
        violations = checker.check_all()

    # Filter by severity
    violations = [v for v in violations if severity_order.get(v.severity, 0) >= min_severity]

    if violations:
        # Group by severity
        by_severity = {}
        for v in violations:
            by_severity.setdefault(v.severity, []).append(v)

        print(f"FAILED: {len(violations)} constitutional violation(s) found")
        print()

        for severity in ["CRITICAL", "HIGH", "MEDIUM", "LOW"]:
            if severity in by_severity:
                print(f"--- {severity} ({len(by_severity[severity])}) ---")
                print()
                for v in by_severity[severity]:
                    print(f"  [{v.rule}] {v.file}:{v.line}")
                    print(f"    Evidence: {v.evidence}")
                    print(f"    {v.message}")
                    print()

        # Summary
        print("=" * 70)
        print("SUMMARY:")
        for severity in ["CRITICAL", "HIGH", "MEDIUM", "LOW"]:
            if severity in by_severity:
                print(f"  {severity}: {len(by_severity[severity])}")
        print("=" * 70)

        if args.ci:
            sys.exit(1)
    else:
        print("PASSED: All constitutional rules satisfied.")
        print()
        print("Checks performed:")
        print("  CORE-R5:          Core Engine must not use os.getenv() directly")
        print("  RETRY-BOUNDS:     Retry limits aligned with constitution (max 2 retries)")
        print("  INDEX-MD:         INDEX.md exists in major directories")
        print("  SECURITY:         S1-S3 security rules (no secrets logged, no hardcoded creds)")
        print("  CAPABILITY-IMPORT: Apps use mission_system.contracts")

    return 0 if not violations else 1


if __name__ == "__main__":
    sys.exit(main())
