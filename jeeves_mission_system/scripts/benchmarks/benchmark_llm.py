#!/usr/bin/env python
"""Benchmark LLM providers for performance comparison.

Compares different LLM backends (llamaserver, openai) on:
- Latency (p50, p95, p99)
- Throughput (tokens/second)
- Memory usage
- Cost per 1K tokens

Usage:
    # Compare all available providers
    python scripts/benchmarks/benchmark_llm.py --compare all

    # Compare specific providers
    python scripts/benchmarks/benchmark_llm.py --compare llamaserver openai

    # Run quick benchmark (10 requests)
    python scripts/benchmarks/benchmark_llm.py --quick

    # Full benchmark (100 requests)
    python scripts/benchmarks/benchmark_llm.py --full
"""

import argparse
import asyncio
import time
import statistics
import sys
from pathlib import Path
from typing import List, Dict, Any
from dataclasses import dataclass

# Add project root to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent))

from jeeves_avionics.settings import Settings
from jeeves_avionics.llm.cost_calculator import get_cost_calculator


@dataclass
class BenchmarkResult:
    """Results from benchmarking a provider."""

    provider: str
    requests: int
    total_time_ms: float
    latencies_ms: List[float]
    tokens_generated: List[int]
    total_cost_usd: float
    errors: int

    @property
    def p50_latency_ms(self) -> float:
        """Median latency."""
        return statistics.median(self.latencies_ms) if self.latencies_ms else 0

    @property
    def p95_latency_ms(self) -> float:
        """95th percentile latency."""
        if not self.latencies_ms:
            return 0
        sorted_lat = sorted(self.latencies_ms)
        idx = int(len(sorted_lat) * 0.95)
        return sorted_lat[idx]

    @property
    def p99_latency_ms(self) -> float:
        """99th percentile latency."""
        if not self.latencies_ms:
            return 0
        sorted_lat = sorted(self.latencies_ms)
        idx = int(len(sorted_lat) * 0.99)
        return sorted_lat[idx]

    @property
    def avg_latency_ms(self) -> float:
        """Average latency."""
        return statistics.mean(self.latencies_ms) if self.latencies_ms else 0

    @property
    def throughput_req_per_sec(self) -> float:
        """Requests per second."""
        return (self.requests / self.total_time_ms * 1000) if self.total_time_ms > 0 else 0

    @property
    def throughput_tokens_per_sec(self) -> float:
        """Tokens per second."""
        total_tokens = sum(self.tokens_generated)
        return (total_tokens / self.total_time_ms * 1000) if self.total_time_ms > 0 else 0


class LLMBenchmark:
    """Benchmark different LLM providers."""

    def __init__(self, settings: Settings):
        """Initialize benchmark.

        Args:
            settings: Application settings
        """
        self.settings = settings
        self.cost_calculator = get_cost_calculator()

        # Standard test prompts (varying lengths)
        self.test_prompts = [
            "Write a short poem about AI.",
            "Explain how a computer works in simple terms.",
            "What are the benefits of exercise? List 5 main points.",
            "Write a brief story about a robot learning to paint.",
            "Explain quantum computing to a 10-year-old.",
        ]

    async def benchmark_provider(
        self,
        provider_name: str,
        num_requests: int = 10
    ) -> BenchmarkResult:
        """Benchmark a specific provider.

        Args:
            provider_name: Provider to test (llamaserver, openai, etc.)
            num_requests: Number of requests to run

        Returns:
            BenchmarkResult with performance metrics
        """
        print(f"\n📊 Benchmarking {provider_name}...")
        print(f"   Running {num_requests} requests...")

        latencies = []
        tokens_generated = []
        total_cost = 0.0
        errors = 0

        start_time = time.time()

        for i in range(num_requests):
            # Cycle through test prompts
            prompt = self.test_prompts[i % len(self.test_prompts)]

            try:
                req_start = time.time()

                # Generate response (mock for now - replace with actual provider call)
                response = await self._call_provider(provider_name, prompt)

                latency_ms = (time.time() - req_start) * 1000
                latencies.append(latency_ms)
                tokens_generated.append(response["tokens"])

                # Calculate cost
                cost_metrics = self.cost_calculator.calculate_cost(
                    provider=provider_name,
                    model=response["model"],
                    prompt_tokens=response["prompt_tokens"],
                    completion_tokens=response["completion_tokens"]
                )
                total_cost += cost_metrics.cost_usd

                if (i + 1) % 10 == 0:
                    print(f"   Progress: {i + 1}/{num_requests}")

            except Exception as e:
                print(f"   ✗ Error on request {i + 1}: {e}")
                errors += 1

        total_time_ms = (time.time() - start_time) * 1000

        return BenchmarkResult(
            provider=provider_name,
            requests=num_requests,
            total_time_ms=total_time_ms,
            latencies_ms=latencies,
            tokens_generated=tokens_generated,
            total_cost_usd=total_cost,
            errors=errors
        )

    async def _call_provider(
        self,
        provider: str,
        prompt: str
    ) -> Dict[str, Any]:
        """Call a provider and return response metadata.

        Args:
            provider: Provider name
            prompt: Test prompt

        Returns:
            Dict with tokens, model, etc.
        """
        # Mock implementation - replace with actual provider calls
        # when integrating with gateway

        # Simulate different provider characteristics
        if provider == "llamaserver":
            await asyncio.sleep(0.2)  # C++ backend via llama.cpp
            return {
                "tokens": 100,
                "prompt_tokens": 20,
                "completion_tokens": 80,
                "model": "qwen2.5-7b-instruct-q4_K_M"
            }
        elif provider == "openai":
            await asyncio.sleep(0.3)  # API latency
            return {
                "tokens": 100,
                "prompt_tokens": 20,
                "completion_tokens": 80,
                "model": "gpt-3.5-turbo"
            }
        else:
            raise ValueError(f"Unknown provider: {provider}")

    def print_results(self, results: List[BenchmarkResult]):
        """Print benchmark results in formatted table.

        Args:
            results: List of benchmark results
        """
        print("\n" + "="*80)
        print("📈 BENCHMARK RESULTS")
        print("="*80)

        # Header
        print(f"\n{'Provider':<12} {'Requests':<10} {'p50':<10} {'p95':<10} "
              f"{'p99':<10} {'Tokens/s':<12} {'Cost':<10} {'Errors':<8}")
        print("-"*80)

        # Results
        for result in results:
            print(f"{result.provider:<12} "
                  f"{result.requests:<10} "
                  f"{result.p50_latency_ms:<10.1f} "
                  f"{result.p95_latency_ms:<10.1f} "
                  f"{result.p99_latency_ms:<10.1f} "
                  f"{result.throughput_tokens_per_sec:<12.1f} "
                  f"${result.total_cost_usd:<9.4f} "
                  f"{result.errors:<8}")

        print("-"*80)

        # Find best performer
        if results:
            fastest = min(results, key=lambda r: r.p95_latency_ms)
            cheapest = min(results, key=lambda r: r.total_cost_usd)
            highest_throughput = max(results, key=lambda r: r.throughput_tokens_per_sec)

            print(f"\n🏆 Fastest (p95):      {fastest.provider} ({fastest.p95_latency_ms:.1f}ms)")
            print(f"💰 Cheapest:           {cheapest.provider} (${cheapest.total_cost_usd:.4f})")
            print(f"🚀 Highest Throughput: {highest_throughput.provider} "
                  f"({highest_throughput.throughput_tokens_per_sec:.1f} tokens/s)")

        print("="*80 + "\n")


async def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Benchmark LLM provider performance"
    )
    parser.add_argument(
        "--compare",
        nargs="+",
        default=["llamaserver"],
        help="Providers to compare (llamaserver, openai, all)"
    )
    parser.add_argument(
        "--quick",
        action="store_true",
        help="Quick benchmark (10 requests per provider)"
    )
    parser.add_argument(
        "--full",
        action="store_true",
        help="Full benchmark (100 requests per provider)"
    )

    args = parser.parse_args()

    # Determine number of requests
    if args.full:
        num_requests = 100
    elif args.quick:
        num_requests = 10
    else:
        num_requests = 20  # Default

    # Expand "all" to all known providers
    if "all" in args.compare:
        providers = ["llamaserver", "openai"]
    else:
        providers = args.compare

    print("🔬 LLM Provider Benchmark")
    print(f"📝 Test configuration:")
    print(f"   Providers: {', '.join(providers)}")
    print(f"   Requests per provider: {num_requests}")

    # Run benchmarks
    settings = Settings()
    benchmark = LLMBenchmark(settings)

    results = []
    for provider in providers:
        try:
            result = await benchmark.benchmark_provider(provider, num_requests)
            results.append(result)
        except Exception as e:
            print(f"✗ Failed to benchmark {provider}: {e}")

    # Print results
    if results:
        benchmark.print_results(results)
    else:
        print("\n❌ No benchmark results available")


if __name__ == "__main__":
    asyncio.run(main())
