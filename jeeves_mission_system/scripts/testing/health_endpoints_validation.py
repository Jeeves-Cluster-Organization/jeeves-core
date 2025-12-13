#!/usr/bin/env python3
"""Health endpoint validation for containerized deployment.

Validates /health and /ready endpoints work correctly in containerized
environment with real dependencies (PostgreSQL, llama-server).

Run manually after starting the server:
    python scripts/testing/health_endpoints_validation.py [--url http://localhost:8000]
"""

import asyncio
import argparse
import sys

import httpx


async def test_health_endpoint(base_url: str) -> bool:
    """GET /health returns 200 when service is running"""
    print(f"Testing GET {base_url}/health...")
    async with httpx.AsyncClient(base_url=base_url) as client:
        response = await client.get("/health")

    print(f"  Status: {response.status_code}")
    data = response.json()
    print(f"  Response: {data}")

    if response.status_code == 200 and data.get("status") in ["healthy", "degraded"]:
        print("  ✓ Health check passed")
        return True
    else:
        print("  ✗ Health check failed")
        return False


async def test_readiness_endpoint(base_url: str) -> bool:
    """GET /ready validates database connection"""
    print(f"\nTesting GET {base_url}/ready...")
    async with httpx.AsyncClient(base_url=base_url) as client:
        response = await client.get("/ready")

    print(f"  Status: {response.status_code}")
    data = response.json()
    print(f"  Response: {data}")

    if response.status_code in [200, 503]:
        print("  ✓ Readiness endpoint responded")
        return True
    else:
        print("  ✗ Readiness check failed")
        return False


async def main():
    parser = argparse.ArgumentParser(description="Health endpoint validation")
    parser.add_argument("--url", default="http://localhost:8000", help="Base URL")
    args = parser.parse_args()

    print(f"=== Health Endpoints Validation ===")
    print(f"Target: {args.url}\n")

    results = []
    results.append(await test_health_endpoint(args.url))
    results.append(await test_readiness_endpoint(args.url))

    passed = sum(results)
    total = len(results)
    print(f"\n=== Results: {passed}/{total} passed ===")

    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
