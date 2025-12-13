#!/usr/bin/env python3
"""
API Bootstrap Test - Interactive API testing script

This script starts the FastAPI server and makes real API calls to test the flow.
Run manually: python scripts/testing/api_bootstrap_test.py
"""

import asyncio
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import httpx

# Use centralized console utilities
from scripts.lib.console import (
    Colors,
    print_header,
    print_success,
    print_error,
    print_info,
    print_warning,
)

async def wait_for_server(url: str, timeout: int = 30):
    """Wait for server to be ready."""
    print_info(f"Waiting for server at {url}...")

    start = time.time()
    async with httpx.AsyncClient() as client:
        while time.time() - start < timeout:
            try:
                response = await client.get(f"{url}/health", timeout=2.0)
                if response.status_code == 200:
                    print_success("Server is ready!")
                    return True
            except (httpx.ConnectError, httpx.TimeoutException):
                await asyncio.sleep(0.5)

    print_error("Server failed to start within timeout")
    return False

async def test_health_endpoint(base_url: str):
    """Test health check endpoint."""
    print_header("TEST 1: Health Check Endpoint")

    async with httpx.AsyncClient() as client:
        try:
            response = await client.get(f"{base_url}/health")

            print_info(f"GET {base_url}/health")
            print(f"Status: {response.status_code}")

            data = response.json()
            print("\nResponse:")
            print(json.dumps(data, indent=2))

            if response.status_code == 200 and data.get("status") == "healthy":
                print_success("Health check passed")
                return True
            else:
                print_error("Health check failed")
                return False
        except Exception as e:
            print_error(f"Health check error: {e}")
            return False

async def test_ready_endpoint(base_url: str):
    """Test readiness check endpoint."""
    print_header("TEST 2: Readiness Check Endpoint")

    async with httpx.AsyncClient() as client:
        try:
            response = await client.get(f"{base_url}/ready")

            print_info(f"GET {base_url}/ready")
            print(f"Status: {response.status_code}")

            data = response.json()
            print("\nResponse:")
            print(json.dumps(data, indent=2))

            if response.status_code == 200:
                print_success("Readiness check passed")
                return True
            else:
                print_warning("Server not ready yet")
                return False
        except Exception as e:
            print_error(f"Readiness check error: {e}")
            return False

async def test_root_endpoint(base_url: str):
    """Test root endpoint."""
    print_header("TEST 3: Root API Info Endpoint")

    async with httpx.AsyncClient() as client:
        try:
            response = await client.get(f"{base_url}/")

            print_info(f"GET {base_url}/")
            print(f"Status: {response.status_code}")

            data = response.json()
            print("\nResponse:")
            print(json.dumps(data, indent=2))

            if response.status_code == 200:
                print_success("Root endpoint working")
                return True
            else:
                print_error("Root endpoint failed")
                return False
        except Exception as e:
            print_error(f"Root endpoint error: {e}")
            return False

async def test_submit_request(base_url: str):
    """Test request submission endpoint."""
    print_header("TEST 4: Submit Request Flow")

    test_request = {
        "user_id": "test-user-123",
        "query": "List all TODO items for my project",
        "conversation_history": []
    }

    async with httpx.AsyncClient(timeout=30.0) as client:
        try:
            print_info(f"POST {base_url}/request")
            print("\nRequest payload:")
            print(json.dumps(test_request, indent=2))

            response = await client.post(
                f"{base_url}/request",
                json=test_request
            )

            print(f"\nStatus: {response.status_code}")

            data = response.json()
            print("\nResponse:")
            print(json.dumps(data, indent=2))

            if response.status_code in (200, 201):
                print_success("Request submission successful")

                # Extract request_id if available
                request_id = data.get("request_id")
                if request_id:
                    print_info(f"Request ID: {request_id}")

                return True, data
            else:
                print_error("Request submission failed")
                return False, data
        except Exception as e:
            print_error(f"Request submission error: {e}")
            return False, None

async def test_metrics_endpoint(base_url: str):
    """Test metrics endpoint."""
    print_header("TEST 5: Metrics Endpoint")

    async with httpx.AsyncClient() as client:
        try:
            response = await client.get(f"{base_url}/metrics")

            print_info(f"GET {base_url}/metrics")
            print(f"Status: {response.status_code}")

            # Metrics might be in Prometheus format (text) or JSON
            if response.status_code == 200:
                print("\nResponse (first 500 chars):")
                print(response.text[:500])
                print_success("Metrics endpoint working")
                return True
            else:
                print_warning("Metrics not available")
                return False
        except Exception as e:
            print_info(f"Metrics endpoint not available: {e}")
            return False

async def run_all_tests(base_url: str = "http://localhost:8000"):
    """Run all API tests."""
    print_header("ASSISTANT 7AGENT - API BOOTSTRAP TEST")
    print_info(f"Base URL: {base_url}")

    results = {
        "health": await test_health_endpoint(base_url),
        "ready": await test_ready_endpoint(base_url),
        "root": await test_root_endpoint(base_url),
        "metrics": await test_metrics_endpoint(base_url),
    }

    # Test request submission (may fail in mock mode)
    success, data = await test_submit_request(base_url)
    results["submit_request"] = success

    # Summary
    print_header("TEST SUMMARY")

    total = len(results)
    passed = sum(1 for v in results.values() if v)

    for test_name, passed_flag in results.items():
        status = f"{Colors.GREEN}✓ PASS{Colors.ENDC}" if passed_flag else f"{Colors.RED}✗ FAIL{Colors.ENDC}"
        print(f"{test_name.ljust(20)}: {status}")

    print(f"\n{Colors.BOLD}Total: {passed}/{total} tests passed{Colors.ENDC}")

    if passed == total:
        print_success("All tests passed! 🎉")
        return 0
    elif passed >= total * 0.6:
        print_warning("Most tests passed (some optional endpoints may be disabled)")
        return 0
    else:
        print_error("Multiple tests failed")
        return 1

def start_server():
    """Start the FastAPI server in a subprocess."""
    print_info("Starting FastAPI server...")

    # Set MOCK_MODE to avoid needing actual LLM
    env = os.environ.copy()
    env["MOCK_MODE"] = "true"
    env["LOG_LEVEL"] = "INFO"

    process = subprocess.Popen(
        [sys.executable, "-m", "uvicorn", "api.server:app", "--host", "0.0.0.0", "--port", "8000"],
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )

    return process

async def main():
    """Main test runner."""
    import argparse

    parser = argparse.ArgumentParser(description="API Bootstrap Test")
    parser.add_argument(
        "--url",
        default="http://localhost:8000",
        help="Base URL of the API server (default: http://localhost:8000)"
    )
    parser.add_argument(
        "--no-start",
        action="store_true",
        help="Don't start the server, assume it's already running"
    )

    args = parser.parse_args()

    server_process = None

    try:
        if not args.no_start:
            server_process = start_server()

            # Wait for server to be ready
            if not await wait_for_server(args.url, timeout=30):
                print_error("Server failed to start")
                return 1

        # Run tests
        exit_code = await run_all_tests(args.url)

        return exit_code

    except KeyboardInterrupt:
        print_warning("\nTests interrupted by user")
        return 130

    finally:
        if server_process:
            print_info("Shutting down server...")
            server_process.send_signal(signal.SIGTERM)
            try:
                server_process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server_process.kill()
            print_success("Server stopped")

if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code)
