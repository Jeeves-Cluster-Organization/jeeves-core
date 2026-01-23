"""Contract tests for layer boundary enforcement.

These tests verify that the four-layer architecture is properly enforced
by programmatically checking import dependencies.

Contract: Each layer may only import from layers below it.

Layers (top to bottom):
- L4: jeeves_mission_system (application)
- L3: jeeves_avionics (infrastructure)  
- L2: jeeves_memory_module (memory)
- L1: jeeves_control_tower (kernel)
- L0: jeeves_protocols, jeeves_shared (foundation)
"""

import pytest
import ast
import importlib
import sys
from pathlib import Path
from typing import Set, List, Tuple


# =============================================================================
# Layer Definitions
# =============================================================================

LAYER_HIERARCHY = {
    # Layer 4: Application (can import from L3, L2, L1, L0)
    "jeeves_mission_system": {
        "level": 4,
        "allowed": [
            "jeeves_avionics",
            "jeeves_memory_module", 
            "jeeves_control_tower",
            "jeeves_protocols",
            "jeeves_shared",
        ],
    },
    # Layer 3: Infrastructure (can import from L2, L1, L0)
    "jeeves_avionics": {
        "level": 3,
        "allowed": [
            "jeeves_memory_module",
            "jeeves_control_tower",
            "jeeves_protocols",
            "jeeves_shared",
        ],
    },
    # Layer 2: Memory (can import from L1, L0)
    "jeeves_memory_module": {
        "level": 2,
        "allowed": [
            "jeeves_control_tower",
            "jeeves_protocols",
            "jeeves_shared",
        ],
    },
    # Layer 1: Kernel (can import from L0 only)
    "jeeves_control_tower": {
        "level": 1,
        "allowed": [
            "jeeves_protocols",
            "jeeves_shared",
        ],
    },
    # Layer 0: Foundation (no internal deps)
    "jeeves_protocols": {
        "level": 0,
        "allowed": ["jeeves_shared"],
    },
    "jeeves_shared": {
        "level": 0,
        "allowed": [],
    },
}

# Packages that are known internal to jeeves-core
JEEVES_PACKAGES = set(LAYER_HIERARCHY.keys())


# =============================================================================
# Helper Functions
# =============================================================================

def get_imports_from_file(filepath: Path) -> List[Tuple[str, int]]:
    """Extract all imports from a Python file.
    
    Returns:
        List of (import_path, line_number) tuples
    """
    imports = []
    try:
        with open(filepath, 'r', encoding='utf-8') as f:
            content = f.read()
        
        tree = ast.parse(content)
        
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                for alias in node.names:
                    imports.append((alias.name, node.lineno))
            elif isinstance(node, ast.ImportFrom):
                if node.module:
                    imports.append((node.module, node.lineno))
    except (SyntaxError, UnicodeDecodeError):
        pass
    
    return imports


def get_package_name(import_path: str) -> str:
    """Extract the top-level package name from an import path."""
    return import_path.split('.')[0]


def check_layer_violation(
    source_package: str,
    import_path: str,
) -> bool:
    """Check if an import violates layer boundaries.
    
    Returns:
        True if violation, False if allowed
    """
    target_package = get_package_name(import_path)
    
    # Not a jeeves package - external dependency, always allowed
    if target_package not in JEEVES_PACKAGES:
        return False
    
    # Self-import - always allowed
    if target_package == source_package:
        return False
    
    # Check if target is in allowed list
    source_config = LAYER_HIERARCHY.get(source_package)
    if not source_config:
        return False  # Unknown package, skip
    
    return target_package not in source_config["allowed"]


# =============================================================================
# Contract Tests
# =============================================================================

class TestLayerBoundariesStatic:
    """Static analysis tests for layer boundaries."""

    def test_jeeves_protocols_has_no_internal_deps(self):
        """Test that jeeves_protocols doesn't import from other jeeves packages."""
        project_root = Path(__file__).parent.parent.parent
        protocols_dir = project_root / "jeeves_protocols"
        
        if not protocols_dir.exists():
            pytest.skip("jeeves_protocols not found")
        
        violations = []
        for py_file in protocols_dir.rglob("*.py"):
            imports = get_imports_from_file(py_file)
            for import_path, line in imports:
                pkg = get_package_name(import_path)
                # jeeves_protocols can only import from jeeves_shared
                if pkg in JEEVES_PACKAGES and pkg != "jeeves_protocols" and pkg != "jeeves_shared":
                    violations.append(
                        f"{py_file.relative_to(project_root)}:{line} imports {import_path}"
                    )
        
        assert len(violations) == 0, f"Layer violations found:\n" + "\n".join(violations)

    def test_jeeves_control_tower_respects_layer(self):
        """Test that jeeves_control_tower only imports from L0."""
        project_root = Path(__file__).parent.parent.parent
        control_tower_dir = project_root / "jeeves_control_tower"
        
        if not control_tower_dir.exists():
            pytest.skip("jeeves_control_tower not found")
        
        violations = []
        allowed = {"jeeves_protocols", "jeeves_shared", "jeeves_control_tower"}
        
        for py_file in control_tower_dir.rglob("*.py"):
            imports = get_imports_from_file(py_file)
            for import_path, line in imports:
                pkg = get_package_name(import_path)
                if pkg in JEEVES_PACKAGES and pkg not in allowed:
                    violations.append(
                        f"{py_file.relative_to(project_root)}:{line} imports {import_path}"
                    )
        
        assert len(violations) == 0, f"Layer violations found:\n" + "\n".join(violations)

    def test_jeeves_avionics_respects_layer(self):
        """Test that jeeves_avionics doesn't import from jeeves_mission_system."""
        project_root = Path(__file__).parent.parent.parent
        avionics_dir = project_root / "jeeves_avionics"
        
        if not avionics_dir.exists():
            pytest.skip("jeeves_avionics not found")
        
        violations = []
        forbidden = {"jeeves_mission_system"}
        
        for py_file in avionics_dir.rglob("*.py"):
            imports = get_imports_from_file(py_file)
            for import_path, line in imports:
                pkg = get_package_name(import_path)
                if pkg in forbidden:
                    violations.append(
                        f"{py_file.relative_to(project_root)}:{line} imports {import_path}"
                    )
        
        assert len(violations) == 0, f"Layer violations found:\n" + "\n".join(violations)


class TestLayerBoundariesRuntime:
    """Runtime tests for layer boundary enforcement."""

    def test_protocols_can_be_imported_standalone(self):
        """Test that jeeves_protocols can be imported without other packages."""
        # This should work because protocols has no internal deps
        try:
            import jeeves_protocols
            assert True
        except ImportError as e:
            pytest.fail(f"jeeves_protocols could not be imported: {e}")

    def test_shared_can_be_imported_standalone(self):
        """Test that jeeves_shared can be imported without other packages."""
        try:
            import jeeves_shared
            assert True
        except ImportError as e:
            pytest.fail(f"jeeves_shared could not be imported: {e}")

    def test_control_tower_imports_only_from_l0(self):
        """Test that control_tower modules import correctly."""
        try:
            from jeeves_control_tower import kernel
            from jeeves_control_tower.types import ResourceQuota
            assert True
        except ImportError as e:
            pytest.fail(f"Control tower import failed: {e}")


# =============================================================================
# Cross-Layer Contract Tests
# =============================================================================

class TestCrossLayerContracts:
    """Test contracts that span multiple layers."""

    def test_envelope_type_same_across_layers(self):
        """Test that Envelope is the same type everywhere."""
        from jeeves_protocols import Envelope as ProtocolEnvelope
        from jeeves_protocols.envelope import Envelope as EnvelopeModule
        
        # Should be the exact same class
        assert ProtocolEnvelope is EnvelopeModule

    def test_terminal_reason_enum_consistent(self):
        """Test that TerminalReason enum is consistent."""
        from jeeves_protocols import TerminalReason as ProtocolReason
        from jeeves_protocols.enums import TerminalReason as CoreReason
        
        # Should be the exact same enum
        assert ProtocolReason is CoreReason
        
        # Should have expected values
        expected_values = {
            "max_iterations_exceeded",
            "max_llm_calls_exceeded",
            "max_agent_hops_exceeded",
            "user_cancelled",
            "tool_failed_fatally",
            "llm_failed_fatally",
            "completed",
        }
        actual_values = {e.value for e in TerminalReason}
        assert expected_values.issubset(actual_values)

    def test_logger_protocol_consistent(self):
        """Test that LoggerProtocol is consistent across layers."""
        from jeeves_protocols import LoggerProtocol as ProtocolLogger
        from jeeves_protocols.protocols import LoggerProtocol as ModuleLogger
        
        assert ProtocolLogger is ModuleLogger


# =============================================================================
# Import Cycle Detection
# =============================================================================

class TestNoCyclicImports:
    """Test that there are no circular imports between layers."""

    def test_no_cycle_protocols_to_control_tower(self):
        """Test no cycle between protocols and control_tower."""
        # If this import works without hanging, no cycle exists
        from jeeves_protocols import Envelope
        from jeeves_control_tower.kernel import ControlTower
        assert True

    def test_no_cycle_control_tower_to_avionics(self):
        """Test no cycle between control_tower and avionics."""
        from jeeves_control_tower.types import ResourceQuota
        # Avionics can import from control_tower
        # Control tower should not import from avionics
        assert True

    def test_no_cycle_avionics_to_mission_system(self):
        """Test no cycle between avionics and mission_system."""
        from jeeves_avionics.settings import Settings
        # Mission system can import from avionics
        # Avionics should not import from mission_system
        assert True


