#!/usr/bin/env python3
"""
Test LLM provider connections via LiteLLM.

This script tests connectivity to LLM providers using the unified LiteLLM interface.

Usage:
    python scripts/diagnostics/test_llm_connection.py [--model MODEL]

Examples:
    # Test local llama-server
    LITELLM_API_BASE=http://localhost:8080/v1 python scripts/diagnostics/test_llm_connection.py --model openai/llama

    # Test OpenAI
    OPENAI_API_KEY=sk-... python scripts/diagnostics/test_llm_connection.py --model gpt-4-turbo

    # Test Anthropic
    ANTHROPIC_API_KEY=sk-ant-... python scripts/diagnostics/test_llm_connection.py --model claude-3-sonnet-20240229
"""

from __future__ import annotations

import argparse
import asyncio
import os
import sys
from pathlib import Path

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


async def check_litellm_provider(model: str, api_base: str = None, api_key: str = None) -> bool:
    """Check LLM connection via LiteLLM (diagnostic script, not pytest test)."""
    print_header(f"Testing LLM Connection: {model}")

    try:
        from jeeves_infra.llm.providers import LiteLLMProvider
    except ImportError as e:
        print_error(f"Failed to import LiteLLMProvider: {e}")
        print_info("Install with: pip install litellm")
        return False

    # Get config from environment
    api_base = api_base or os.getenv("LITELLM_API_BASE")
    api_key = api_key or os.getenv("LITELLM_API_KEY")

    print_info(f"Model: {model}")
    print_info(f"API Base: {api_base or '(default)'}")
    print_info(f"API Key: {'***' + api_key[-4:] if api_key else '(from env)'}")

    try:
        print("\n[Initializing provider...]")
        provider = LiteLLMProvider(
            model=model,
            api_base=api_base,
            api_key=api_key,
            timeout=30.0,
            max_retries=1,
        )
        print_success("Provider initialized")

        # Test health check
        print("\n[Test 1] Health check...")
        healthy = await provider.health_check()
        if healthy:
            print_success("Health check passed")
        else:
            print_error("Health check failed")
            return False

        # Test simple generation
        print("\n[Test 2] Testing generation...")
        response = await provider.generate(
            model=model,
            prompt="Say 'Hello' in one word",
            options={"temperature": 0.0, "max_tokens": 10}
        )
        print_success("Generation successful!")
        print_info(f"Response: {response[:100]}")

        # Test streaming
        print("\n[Test 3] Testing streaming...")
        chunks = []
        async for chunk in provider.generate_stream(
            model=model,
            prompt="Count from 1 to 3",
            options={"temperature": 0.0, "max_tokens": 20}
        ):
            chunks.append(chunk)
        full_response = "".join(c.text for c in chunks)
        print_success(f"Streaming successful! Got {len(chunks)} chunks")
        print_info(f"Response: {full_response[:100]}")

        print_header("✓ All Tests Passed")
        return True

    except Exception as e:
        print_error(f"Test failed: {e}")
        print_info("Check your configuration and network connection")
        import traceback
        traceback.print_exc()
        return False


def main():
    parser = argparse.ArgumentParser(description="Test LLM connections via LiteLLM")
    parser.add_argument(
        "--model",
        default=os.getenv("LITELLM_MODEL", "openai/llama"),
        help="LiteLLM model string (default: $LITELLM_MODEL or 'openai/llama')"
    )
    parser.add_argument(
        "--api-base",
        default=None,
        help="API base URL (default: $LITELLM_API_BASE)"
    )
    parser.add_argument(
        "--api-key",
        default=None,
        help="API key (default: $LITELLM_API_KEY or provider-specific env var)"
    )

    args = parser.parse_args()

    print_header("LLM Connection Test (via LiteLLM)")
    print_info(f"Model: {args.model}")
    print_info(f"API Base: {args.api_base or os.getenv('LITELLM_API_BASE', '(not set)')}")

    success = asyncio.run(check_litellm_provider(
        model=args.model,
        api_base=args.api_base,
        api_key=args.api_key,
    ))

    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
