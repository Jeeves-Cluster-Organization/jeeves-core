#!/usr/bin/env python3
"""
Test LLM provider connections.

This script tests connectivity to various LLM providers (llama-server, OpenAI, Anthropic, etc.)
and validates their configuration.

Usage:
    python scripts/diagnostics/test_llm_connection.py [--provider PROVIDER]

Examples:
    # Test llama-server (default)
    python scripts/diagnostics/test_llm_connection.py

    # Test specific provider
    python scripts/diagnostics/test_llm_connection.py --provider openai
    python scripts/diagnostics/test_llm_connection.py --provider anthropic

    # Test all configured providers
    python scripts/diagnostics/test_llm_connection.py --all
"""

from __future__ import annotations

import argparse
import asyncio
import sys
from pathlib import Path
from typing import Dict, Any

# Add project root to path
PROJECT_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(PROJECT_ROOT))


def print_header(text: str):
    """Print formatted header."""
    print("\n" + "=" * 60)
    print(text)
    print("=" * 60 + "\n")


def print_success(text: str):
    """Print success message."""
    print(f"✓ {text}")


def print_error(text: str):
    """Print error message."""
    print(f"✗ {text}")


def print_info(text: str):
    """Print info message."""
    print(f"  {text}")


def test_llamaserver(host: str = "http://localhost:8080") -> bool:
    """Test llama-server connection."""
    print_header("Testing llama-server Connection")
    print_info(f"Host: {host}")

    try:
        import httpx
    except ImportError:
        print_error("httpx package not installed")
        print_info("Install with: pip install httpx")
        return False

    try:
        # Test 1: Health check
        print("\n[Test 1] Checking health endpoint...")
        with httpx.Client(timeout=10.0) as client:
            response = client.get(f"{host}/health")
            if response.status_code == 200:
                print_success("llama-server is healthy")
            else:
                print_error(f"Health check failed with status {response.status_code}")
                return False

        # Test 2: Simple generation (using /completion to avoid chat templates)
        print("\n[Test 2] Testing generation...")
        with httpx.Client(timeout=30.0) as client:
            response = client.post(
                f"{host}/completion",
                json={
                    "prompt": "Say 'Hello' in one word:",
                    "temperature": 0.0,
                    "n_predict": 10
                }
            )
            if response.status_code == 200:
                result = response.json()
                content = result.get("content", "")
                print_success("Generation successful!")
                print_info(f"Response: {content[:100]}")
            else:
                print_error(f"Generation failed with status {response.status_code}")
                return False

        print_header("✓ llama-server Tests Passed")
        return True

    except Exception as e:
        print_error(f"Connection failed: {e}")
        print_info("Make sure llama-server is running:")
        print_info("  docker compose up -d llama-server")
        return False


async def test_openai(api_key: str = None) -> bool:
    """Test OpenAI connection."""
    print_header("Testing OpenAI Connection")

    import os
    api_key = api_key or os.getenv('OPENAI_API_KEY')

    if not api_key:
        print_error("OPENAI_API_KEY not set")
        print_info("Set with: export OPENAI_API_KEY=sk-your-key-here")
        return False

    try:
        from jeeves_avionics.llm.providers import OpenAIProvider
    except ImportError:
        print_error("OpenAI dependencies not installed")
        print_info("Install with: pip install openai")
        return False

    try:
        print_info("Initializing OpenAI provider...")
        provider = OpenAIProvider(api_key=api_key)

        # Test health check
        print("\n[Test 1] Health check...")
        healthy = await provider.health_check()
        if healthy:
            print_success("Provider initialized successfully")
        else:
            print_error("Health check failed")
            return False

        # Test simple generation
        print("\n[Test 2] Testing generation...")
        response = await provider.generate(
            model="gpt-3.5-turbo",
            prompt="Say 'Hello' in one word",
            options={"temperature": 0.0, "max_tokens": 10}
        )
        print_success("Generation successful!")
        print_info(f"Response: {response[:100]}")

        print_header("✓ OpenAI Tests Passed")
        return True

    except Exception as e:
        print_error(f"Connection failed: {e}")
        print_info("Check your API key and network connection")
        return False


async def test_anthropic(api_key: str = None) -> bool:
    """Test Anthropic connection."""
    print_header("Testing Anthropic Connection")

    import os
    api_key = api_key or os.getenv('ANTHROPIC_API_KEY')

    if not api_key:
        print_error("ANTHROPIC_API_KEY not set")
        print_info("Set with: export ANTHROPIC_API_KEY=sk-ant-your-key-here")
        return False

    try:
        from jeeves_avionics.llm.providers import AnthropicProvider
    except ImportError:
        print_error("Anthropic dependencies not installed")
        print_info("Install with: pip install anthropic")
        return False

    try:
        print_info("Initializing Anthropic provider...")
        provider = AnthropicProvider(api_key=api_key)

        # Test health check
        print("\n[Test 1] Health check...")
        healthy = await provider.health_check()
        if healthy:
            print_success("Provider initialized successfully")
        else:
            print_error("Health check failed")
            return False

        # Test simple generation
        print("\n[Test 2] Testing generation...")
        response = await provider.generate(
            model="claude-3-haiku-20240307",
            prompt="Say 'Hello' in one word",
            options={"temperature": 0.0, "max_tokens": 10}
        )
        print_success("Generation successful!")
        print_info(f"Response: {response[:100]}")

        print_header("✓ Anthropic Tests Passed")
        return True

    except Exception as e:
        print_error(f"Connection failed: {e}")
        print_info("Check your API key and network connection")
        return False


def test_mock() -> bool:
    """Test Mock provider."""
    print_header("Testing Mock Provider")

    try:
        from jeeves_avionics.llm.providers import MockProvider
    except ImportError:
        print_error("Cannot import MockProvider")
        return False

    try:
        print_info("Initializing Mock provider...")
        provider = MockProvider()

        # Test generation
        print("\n[Test 1] Testing generation...")
        import asyncio
        response = asyncio.run(provider.generate(
            model='test',
            prompt='Generate a test response',
            options={}
        ))
        print_success("Generation successful!")
        print_info(f"Response length: {len(response)} characters")

        print_header("✓ Mock Provider Tests Passed")
        return True

    except Exception as e:
        print_error(f"Test failed: {e}")
        return False


async def test_all_providers() -> Dict[str, bool]:
    """Test all configured providers."""
    print_header("Testing All LLM Providers")

    results = {}

    # Test Mock (always available)
    results['mock'] = test_mock()

    # Test llama-server
    results['llamaserver'] = test_llamaserver()

    # Test OpenAI (if configured)
    import os
    if os.getenv('OPENAI_API_KEY'):
        results['openai'] = await test_openai()
    else:
        print("\nSkipping OpenAI (OPENAI_API_KEY not set)")

    # Test Anthropic (if configured)
    if os.getenv('ANTHROPIC_API_KEY'):
        results['anthropic'] = await test_anthropic()
    else:
        print("\nSkipping Anthropic (ANTHROPIC_API_KEY not set)")

    # Print summary
    print_header("Summary")
    for provider, passed in results.items():
        status = "✓ PASS" if passed else "✗ FAIL"
        print(f"  {status}: {provider}")

    all_passed = all(results.values())
    return results


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Test LLM provider connections",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    parser.add_argument(
        '--provider',
        choices=['llamaserver', 'openai', 'anthropic', 'mock'],
        default='llamaserver',
        help='LLM provider to test (default: llamaserver)'
    )

    parser.add_argument(
        '--all',
        action='store_true',
        help='Test all configured providers'
    )

    parser.add_argument(
        '--host',
        default='http://localhost:8080',
        help='llama-server host URL (default: http://localhost:8080)'
    )

    args = parser.parse_args()

    if args.all:
        results = asyncio.run(test_all_providers())
        success = all(results.values())
    elif args.provider == 'llamaserver':
        success = test_llamaserver(host=args.host)
    elif args.provider == 'openai':
        success = asyncio.run(test_openai())
    elif args.provider == 'anthropic':
        success = asyncio.run(test_anthropic())
    elif args.provider == 'mock':
        success = test_mock()
    else:
        print(f"Unknown provider: {args.provider}")
        success = False

    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
