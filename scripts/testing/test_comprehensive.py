"""
Comprehensive Integration Tests for Distributed Architecture
=============================================================

Tests all components working together in both single-node and distributed modes.
"""

import asyncio
import os
import sys
from pathlib import Path

# Add project root to path
project_root = Path(__file__).parent.parent.parent
sys.path.insert(0, str(project_root))


class Colors:
    BLUE = '\033[0;34m'
    GREEN = '\033[0;32m'
    YELLOW = '\033[1;33m'
    RED = '\033[0;31m'
    NC = '\033[0m'


def log(message: str, level: str = "info"):
    """Log message with color."""
    colors = {
        "info": Colors.BLUE,
        "success": Colors.GREEN,
        "warning": Colors.YELLOW,
        "error": Colors.RED
    }
    color = colors.get(level, Colors.NC)
    print(f"{color}[{level.upper()}]{Colors.NC} {message}")


async def test_single_node_deployment():
    """Test complete single-node deployment."""
    log("Testing single-node deployment mode", "info")

    os.environ['DEPLOYMENT_MODE'] = 'single_node'
    os.environ['SKIP_CONFIG_VALIDATION'] = '1'

    try:
        from avionics.settings import settings
        from config.node_profiles import get_deployment_mode, get_active_profile_names, get_node_for_agent
        from avionics.llm.factory import LLMFactory

        # Test 1: Deployment mode detection
        mode = get_deployment_mode()
        assert mode == "single_node", f"Expected single_node, got {mode}"
        log("[OK] Deployment mode correct: single_node", "success")

        # Test 2: Active profiles
        profiles = get_active_profile_names()
        assert profiles == ["single_node"], f"Expected ['single_node'], got {profiles}"
        log(f"[OK] Active profiles: {profiles}", "success")

        # Test 3: Agent assignments
        agents = ["planner", "executor", "validator", "meta_validator"]
        for agent in agents:
            node = get_node_for_agent(agent)
            assert node == "single_node", f"Agent {agent} assigned to {node}, expected single_node"
        log(f"[OK] All {len(agents)} agents assigned to single_node", "success")

        # Test 4: LLM Factory creation
        factory = LLMFactory(settings)
        for agent in agents:
            provider = factory.get_provider_for_agent(agent)
            assert provider is not None, f"Provider not created for {agent}"
        log(f"[OK] LLM Factory created providers for {len(agents)} agents", "success")

        # Test 5: Provider caching
        provider1 = factory.get_provider_for_agent("planner")
        provider2 = factory.get_provider_for_agent("planner")
        assert provider1 is provider2, "Provider caching not working"
        log("[OK] Provider caching works", "success")

        # Test 6: Cache clearing
        factory.clear_cache()
        provider3 = factory.get_provider_for_agent("planner")
        assert provider3 is not provider1, "Cache not cleared"
        log("[OK] Cache clearing works", "success")

        return True

    except Exception as e:
        log(f"[X] Single-node deployment test failed: {e}", "error")
        import traceback
        traceback.print_exc()
        return False


async def test_distributed_mode_configuration():
    """Test distributed mode configuration (without actual nodes)."""
    log("Testing distributed mode configuration", "info")

    os.environ['DEPLOYMENT_MODE'] = 'distributed'
    os.environ['LLAMASERVER_NODE3_URL'] = 'http://node3:8080'  # Enable 3-node mode
    os.environ['SKIP_CONFIG_VALIDATION'] = '1'

    try:
        from config.node_profiles import get_deployment_mode, get_active_profile_names, get_node_for_agent, get_profile_for_agent

        # Test 1: Deployment mode
        mode = get_deployment_mode()
        assert mode == "distributed", f"Expected distributed, got {mode}"
        log("[OK] Deployment mode: distributed", "success")

        # Test 2: Active profiles (3-node setup)
        profiles = get_active_profile_names()
        assert profiles == ["node1", "node2", "node3"], f"Expected 3 nodes, got {profiles}"
        log(f"[OK] Active profiles: {profiles}", "success")

        # Test 3: Agent to node assignments
        expected_assignments = {
            "planner": "node1",
            "meta_validator": "node1",
            "validator": "node2",
            "memory": "node2",
            "executor": "node3"
        }

        for agent, expected_node in expected_assignments.items():
            actual_node = get_node_for_agent(agent)
            assert actual_node == expected_node, f"Agent {agent}: expected {expected_node}, got {actual_node}"
        log(f"[OK] All agents correctly assigned to nodes", "success")

        # Test 4: Node profiles
        for agent in expected_assignments.keys():
            profile = get_profile_for_agent(agent)
            assert profile is not None, f"Profile not found for {agent}"
            assert agent in profile.agents, f"Agent {agent} not in profile.agents"
        log("[OK] Node profiles correctly configured", "success")

        # Test 5: VRAM constraints
        from config.node_profiles import PROFILES
        for profile_name in profiles:
            profile = PROFILES[profile_name]
            assert profile.model_size_gb <= profile.vram_gb, f"Model too large for {profile_name}"
        log("[OK] VRAM constraints satisfied", "success")

        return True

    except Exception as e:
        log(f"[X] Distributed mode test failed: {e}", "error")
        import traceback
        traceback.print_exc()
        return False
    finally:
        # Clean up
        if 'LLAMASERVER_NODE3_URL' in os.environ:
            del os.environ['LLAMASERVER_NODE3_URL']


async def test_hybrid_mode():
    """Test hybrid cloud + local configuration."""
    log("Testing hybrid deployment mode", "info")

    os.environ['DEPLOYMENT_MODE'] = 'single_node'
    os.environ['PLANNER_LLM_PROVIDER'] = 'openai'
    os.environ['SKIP_CONFIG_VALIDATION'] = '1'

    try:
        from avionics.settings import settings
        from avionics.llm.factory import create_agent_provider_with_node_awareness

        # Test planner uses override
        planner_provider = create_agent_provider_with_node_awareness(settings, "planner")
        assert "OpenAI" in type(planner_provider).__name__ or "Mock" in type(planner_provider).__name__, \
            f"Expected OpenAI or Mock provider, got {type(planner_provider).__name__}"
        log("[OK] Planner uses cloud provider override", "success")

        # Test other agents use default
        validator_provider = create_agent_provider_with_node_awareness(settings, "validator")
        assert "LlamaServer" in type(validator_provider).__name__ or "Mock" in type(validator_provider).__name__, \
            f"Expected LlamaServer or Mock provider, got {type(validator_provider).__name__}"
        log("[OK] Validator uses local provider", "success")

        return True

    except Exception as e:
        log(f"[X] Hybrid mode test failed: {e}", "error")
        import traceback
        traceback.print_exc()
        return False
    finally:
        # Clean up
        if 'PLANNER_LLM_PROVIDER' in os.environ:
            del os.environ['PLANNER_LLM_PROVIDER']


async def test_mock_mode():
    """Test mock LLM mode."""
    log("Testing mock LLM mode", "info")

    os.environ['DEPLOYMENT_MODE'] = 'single_node'
    os.environ['MOCK_LLM_ENABLED'] = 'true'
    os.environ['SKIP_CONFIG_VALIDATION'] = '1'

    try:
        from avionics.settings import settings
        from avionics.llm.factory import LLMFactory

        factory = LLMFactory(settings)
        provider = factory.get_provider_for_agent("planner")

        assert "Mock" in type(provider).__name__, f"Expected MockProvider, got {type(provider).__name__}"
        log("[OK] Mock provider created", "success")

        return True

    except Exception as e:
        log(f"[X] Mock mode test failed: {e}", "error")
        import traceback
        traceback.print_exc()
        return False
    finally:
        # Clean up
        if 'MOCK_LLM_ENABLED' in os.environ:
            del os.environ['MOCK_LLM_ENABLED']


async def test_node_override():
    """Test agent node override functionality."""
    log("Testing agent node override", "info")

    os.environ['DEPLOYMENT_MODE'] = 'distributed'
    os.environ['LLAMASERVER_NODE3_URL'] = 'http://node3:8080'
    os.environ['AGENT_NODE_OVERRIDE_PLANNER'] = 'node2'
    os.environ['SKIP_CONFIG_VALIDATION'] = '1'

    try:
        from config.node_profiles import get_node_for_agent

        # Planner should be overridden to node2
        node = get_node_for_agent("planner")
        assert node == "node2", f"Expected node2, got {node}"
        log("[OK] Agent node override works", "success")

        return True

    except Exception as e:
        log(f"[X] Node override test failed: {e}", "error")
        import traceback
        traceback.print_exc()
        return False
    finally:
        # Clean up
        for key in ['LLAMASERVER_NODE3_URL', 'AGENT_NODE_OVERRIDE_PLANNER']:
            if key in os.environ:
                del os.environ[key]


async def test_error_handling():
    """Test error handling and graceful degradation."""
    log("Testing error handling", "info")

    os.environ['SKIP_CONFIG_VALIDATION'] = '1'

    try:
        from config.node_profiles import get_node_for_agent

        # Test with invalid agent name - should fallback to single_node
        node = get_node_for_agent("nonexistent_agent")
        assert node == "single_node", "Should fallback to single_node for unknown agent"
        log("[OK] Graceful fallback for unknown agent", "success")

        return True

    except Exception as e:
        log(f"[X] Error handling test failed: {e}", "error")
        import traceback
        traceback.print_exc()
        return False


async def test_settings_attributes():
    """Test that all required settings attributes exist."""
    log("Testing settings attributes", "info")

    os.environ['SKIP_CONFIG_VALIDATION'] = '1'

    try:
        from avionics.settings import settings

        required_attrs = [
            'deployment_mode',
            'database_path',
            'llm_provider',
            'llamaserver_host',
            'default_model',
            'llm_timeout',
            'api_host',
            'api_port'
        ]

        for attr in required_attrs:
            assert hasattr(settings, attr), f"Missing required attribute: {attr}"
        log(f"[OK] All {len(required_attrs)} required attributes present", "success")

        # Test attribute values
        assert settings.deployment_mode in ['single_node', 'distributed'], "Invalid deployment_mode"
        assert settings.llm_provider in ['llamaserver', 'openai', 'anthropic', 'mock'], "Invalid llm_provider"
        log("[OK] Attribute values valid", "success")

        return True

    except Exception as e:
        log(f"[X] Settings attributes test failed: {e}", "error")
        import traceback
        traceback.print_exc()
        return False


async def run_all_tests():
    """Run all comprehensive integration tests."""
    print("=" * 80)
    print("COMPREHENSIVE INTEGRATION TESTS")
    print("=" * 80)
    print()

    tests = [
        ("Single-Node Deployment", test_single_node_deployment),
        ("Distributed Mode Configuration", test_distributed_mode_configuration),
        ("Hybrid Mode", test_hybrid_mode),
        ("Mock Mode", test_mock_mode),
        ("Node Override", test_node_override),
        ("Error Handling", test_error_handling),
        ("Settings Attributes", test_settings_attributes),
    ]

    results = []
    for name, test_func in tests:
        print()
        result = await test_func()
        results.append((name, result))
        print()

    # Summary
    print("=" * 80)
    print("TEST SUMMARY")
    print("=" * 80)

    passed = sum(1 for _, r in results if r)
    failed = sum(1 for _, r in results if not r)

    for name, result in results:
        status = f"{Colors.GREEN}[OK] PASS{Colors.NC}" if result else f"{Colors.RED}[X] FAIL{Colors.NC}"
        print(f"{status} {name}")

    print()
    print(f"Total: {len(results)}")
    print(f"{Colors.GREEN}Passed: {passed}{Colors.NC}")
    print(f"{Colors.RED}Failed: {failed}{Colors.NC}")
    print()

    if failed == 0:
        print(f"{Colors.GREEN}[OK] All tests passed!{Colors.NC}")
        return 0
    else:
        print(f"{Colors.RED}[X] Some tests failed{Colors.NC}")
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(run_all_tests())
    sys.exit(exit_code)
