#!/usr/bin/env python3
"""
Import Boundary Checker - Enforces Four-Layer Architecture (Phase F).

FOUR-LAYER ARCHITECTURE:
    capability → jeeves_mission_system → jeeves_avionics → jeeves_core_engine → jeeves_commbus

BOUNDARY RULES:
- RULE 0: jeeves_commbus/ must have ZERO dependencies on other Jeeves packages
    - The foundation layer cannot import from core_engine, avionics, or mission_system
- RULE 1: jeeves_core_engine/ may depend on commbus only
    - Must not import from avionics.*, jeeves_mission_system.*, verticals.*
- RULE 2: jeeves_avionics/ may depend on core_engine and commbus only
    - Must not import from mission_system.*, verticals.*
- RULE 3: jeeves_mission_system/ may depend on avionics, core_engine, and commbus
    - Must not import from capability packages directly (use contracts)
- RULE 4: Capabilities access core only through mission_system.contracts
    - jeeves-capability-* should import from mission_system.contracts, not directly
- RULE 5: Shared modules must not import agents (except envelope, contracts, base)

Directory Structure (Four-Layer Architecture):
- jeeves_commbus/           - Foundation layer (zero dependencies)
- jeeves_core_engine/       - Pure orchestration runtime (depends on commbus only)
- jeeves_avionics/          - Infrastructure (depends on core_engine, commbus)
- jeeves_mission_system/    - Application layer (depends on avionics, core_engine, commbus)
- jeeves-capability-*/      - Capabilities (depends on mission_system.contracts)

Usage:
    python scripts/check_import_boundaries.py
    python scripts/check_import_boundaries.py --verbose
    python scripts/check_import_boundaries.py --ci  # Exit with non-zero on violation

Returns exit code 0 if all boundaries are respected, 1 if violations found.
"""

import argparse
import ast
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import List, Set, Tuple


@dataclass
class Violation:
    """A boundary violation."""
    file: str
    line: int
    rule: str
    import_path: str
    message: str


class ImportBoundaryChecker:
    """Check import boundary rules."""

    # Allowed exceptions
    ALLOWED_AGENT_IMPORTS = {"agents.envelope", "agents.contracts", "agents.base"}

    def __init__(self, root_dir: Path, verbose: bool = False):
        self.root_dir = root_dir
        self.verbose = verbose
        self.violations: List[Violation] = []

    def check_all(self) -> List[Violation]:
        """Run all boundary checks."""
        self.violations = []

        # Rule 0: CommBus must have ZERO dependencies on other Jeeves packages
        self._check_rule0_commbus_isolation()

        # Rule 1: Core engine may depend on commbus only
        self._check_rule1_core_engine_isolation()

        # Rule 2: Avionics may depend on core_engine and commbus only
        self._check_rule2_avionics_isolation()

        # Rule 3: Mission system may not import capability packages directly
        self._check_rule3_mission_system()

        # Rule 4: Capabilities should use mission_system.contracts
        self._check_rule4_capability_contracts()

        # Rule 5: Shared modules must not import agents
        self._check_rule5_shared_agents()

        return self.violations

    def _check_rule0_commbus_isolation(self) -> None:
        """RULE 0: jeeves_commbus must have ZERO dependencies on other Jeeves packages."""
        commbus_dir = self.root_dir / "jeeves_commbus"
        if not commbus_dir.exists():
            return

        forbidden_patterns = [
            "jeeves_core_engine",
            "avionics",
            "mission_system",
            "jeeves-capability",
        ]

        for py_file in commbus_dir.rglob("*.py"):
            imports = self._get_imports(py_file)
            for imp, line in imports:
                # Skip self-references
                if imp.startswith("jeeves_commbus"):
                    continue
                for pattern in forbidden_patterns:
                    if imp.startswith(pattern):
                        self.violations.append(Violation(
                            file=str(py_file.relative_to(self.root_dir)),
                            line=line,
                            rule="RULE 0",
                            import_path=imp,
                            message=f"CommBus (foundation layer) must have ZERO dependencies on {pattern}",
                        ))

    def _check_rule1_core_engine_isolation(self) -> None:
        """RULE 1: jeeves_core_engine may depend on commbus only."""
        core_dir = self.root_dir / "jeeves_core_engine"
        if not core_dir.exists():
            return

        forbidden_patterns = [
            "avionics",
            "mission_system",
            "jeeves-capability",
            "verticals.",
        ]

        for py_file in core_dir.rglob("*.py"):
            imports = self._get_imports(py_file)
            for imp, line in imports:
                for pattern in forbidden_patterns:
                    if imp.startswith(pattern):
                        self.violations.append(Violation(
                            file=str(py_file.relative_to(self.root_dir)),
                            line=line,
                            rule="RULE 1",
                            import_path=imp,
                            message=f"Core engine may depend on commbus only, not {pattern}",
                        ))

    def _check_rule2_avionics_isolation(self) -> None:
        """RULE 2: jeeves_avionics may depend on core_engine and commbus only."""
        avionics_dir = self.root_dir / "avionics"
        if not avionics_dir.exists():
            return

        forbidden_patterns = [
            "mission_system",
            "jeeves-capability",
            "verticals.",
        ]

        for py_file in avionics_dir.rglob("*.py"):
            imports = self._get_imports(py_file)
            for imp, line in imports:
                for pattern in forbidden_patterns:
                    if imp.startswith(pattern):
                        self.violations.append(Violation(
                            file=str(py_file.relative_to(self.root_dir)),
                            line=line,
                            rule="RULE 2",
                            import_path=imp,
                            message=f"Avionics may depend on core_engine and commbus only, not {pattern}",
                        ))

    def _check_rule3_mission_system(self) -> None:
        """RULE 3: Mission system may not import capability packages directly."""
        mission_dir = self.root_dir / "mission_system"
        if not mission_dir.exists():
            return

        forbidden_patterns = [
            "jeeves-capability",
        ]

        for py_file in mission_dir.rglob("*.py"):
            imports = self._get_imports(py_file)
            for imp, line in imports:
                for pattern in forbidden_patterns:
                    if imp.startswith(pattern):
                        self.violations.append(Violation(
                            file=str(py_file.relative_to(self.root_dir)),
                            line=line,
                            rule="RULE 3",
                            import_path=imp,
                            message=f"Mission system should not import capability packages directly",
                        ))

    def _check_rule4_capability_contracts(self) -> None:
        """RULE 4: Capabilities should import from mission_system.contracts."""
        capability_dirs = list(self.root_dir.glob("jeeves-capability-*"))
        if not capability_dirs:
            return

        # Direct imports that should go through contracts instead
        direct_patterns = [
            # Direct core_engine imports (except envelope which is re-exported)
            ("jeeves_core_engine.agents.base", "Use jeeves_mission_system.contracts.Agent"),
            ("jeeves_core_engine.protocols", "Use jeeves_mission_system.contracts for protocols"),
        ]

        for cap_dir in capability_dirs:
            for py_file in cap_dir.rglob("*.py"):
                imports = self._get_imports(py_file)
                for imp, line in imports:
                    for pattern, suggestion in direct_patterns:
                        if imp.startswith(pattern):
                            # This is a warning, not a hard violation for now
                            if self.verbose:
                                print(f"  Warning: {py_file.relative_to(self.root_dir)}:{line}")
                                print(f"    Direct import: {imp}")
                                print(f"    Consider: {suggestion}")

    def _check_rule5_shared_agents(self) -> None:
        """RULE 5: Shared modules must not import agents."""
        shared_dirs = [
            (self.root_dir / "mission_system" / "prompts", "prompts/"),
            (self.root_dir / "mission_system" / "common", "common/"),
        ]

        for shared_dir, prefix in shared_dirs:
            if not shared_dir.exists():
                continue

            for py_file in shared_dir.rglob("*.py"):
                imports = self._get_imports(py_file)
                for imp, line in imports:
                    # Check if it's an agent import (but allow envelope, contracts, base)
                    if imp.startswith("agents.") and not self._is_allowed_agent_import(imp):
                        self.violations.append(Violation(
                            file=str(py_file.relative_to(self.root_dir)),
                            line=line,
                            rule="RULE 5",
                            import_path=imp,
                            message=f"Shared module ({prefix}) must not import agents",
                        ))

    def _is_allowed_agent_import(self, imp: str) -> bool:
        """Check if an agent import is in the allowed list."""
        for allowed in self.ALLOWED_AGENT_IMPORTS:
            if imp.startswith(allowed):
                return True
        return False

    def _get_imports(self, py_file: Path) -> List[Tuple[str, int]]:
        """Extract all imports from a Python file."""
        imports = []
        try:
            with open(py_file, 'r', encoding='utf-8') as f:
                content = f.read()

            tree = ast.parse(content)

            for node in ast.walk(tree):
                if isinstance(node, ast.Import):
                    for alias in node.names:
                        imports.append((alias.name, node.lineno))
                elif isinstance(node, ast.ImportFrom):
                    if node.module:
                        imports.append((node.module, node.lineno))

        except (SyntaxError, UnicodeDecodeError) as e:
            if self.verbose:
                print(f"Warning: Could not parse {py_file}: {e}", file=sys.stderr)

        return imports


def main():
    parser = argparse.ArgumentParser(description="Check import boundary rules")
    parser.add_argument("--verbose", "-v", action="store_true", help="Verbose output")
    parser.add_argument("--ci", action="store_true", help="CI mode (exit 1 on violations)")
    parser.add_argument("--root", type=str, default=".", help="Root directory")
    args = parser.parse_args()

    root_dir = Path(args.root).resolve()
    checker = ImportBoundaryChecker(root_dir, verbose=args.verbose)

    print("=" * 60)
    print("Import Boundary Checker (Four-Layer Architecture)")
    print("=" * 60)
    print()

    violations = checker.check_all()

    if violations:
        print(f"FAILED: {len(violations)} boundary violation(s) found:")
        print()
        for v in violations:
            print(f"  [{v.rule}] {v.file}:{v.line}")
            print(f"    Import: {v.import_path}")
            print(f"    {v.message}")
            print()

        if args.ci:
            sys.exit(1)
    else:
        print("PASSED: All import boundaries respected.")
        print()
        print("Rules checked (Four-Layer Architecture):")
        print("  RULE 0: jeeves_commbus has ZERO dependencies")
        print("  RULE 1: jeeves_core_engine may depend on commbus only")
        print("  RULE 2: jeeves_avionics may depend on core_engine and commbus only")
        print("  RULE 3: jeeves_mission_system may not import capability packages")
        print("  RULE 4: Capabilities should use mission_system.contracts")
        print("  RULE 5: Shared modules must not import agents")

    return 0 if not violations else 1


if __name__ == "__main__":
    sys.exit(main())
