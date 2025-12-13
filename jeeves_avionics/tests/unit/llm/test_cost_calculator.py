"""Unit tests for LLM cost calculator."""

import pytest
from jeeves_avionics.llm.cost_calculator import (
    CostCalculator,
    CostMetrics,
    get_cost_calculator,
    calculate_cost
)


class TestCostCalculator:
    """Test cost calculation for various providers and models."""

    def test_llamaserver_is_free(self):
        """llama-server (local) should have zero cost."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="llamaserver",
            model="qwen2.5-7b-instruct-q4_K_M",
            prompt_tokens=100,
            completion_tokens=50
        )

        assert metrics.cost_usd == 0.0
        assert metrics.total_tokens == 150
        assert metrics.provider == "llamaserver"

    def test_openai_gpt4_cost(self):
        """GPT-4 should calculate correct costs."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="openai",
            model="gpt-4",
            prompt_tokens=1000,  # 1K tokens
            completion_tokens=500  # 0.5K tokens
        )

        # GPT-4: $0.03/1K prompt, $0.06/1K completion
        # Expected: (1K * 0.03) + (0.5K * 0.06) = 0.03 + 0.03 = $0.06
        assert metrics.cost_usd == 0.06
        assert metrics.total_tokens == 1500

    def test_openai_gpt35_cost(self):
        """GPT-3.5-turbo should calculate correct costs."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="openai",
            model="gpt-3.5-turbo",
            prompt_tokens=1000,
            completion_tokens=1000
        )

        # GPT-3.5: $0.0015/1K prompt, $0.002/1K completion
        # Expected: (1K * 0.0015) + (1K * 0.002) = 0.0035
        assert metrics.cost_usd == 0.0035

    def test_anthropic_claude_sonnet_cost(self):
        """Claude Sonnet should calculate correct costs."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="anthropic",
            model="claude-3-sonnet",
            prompt_tokens=1000,
            completion_tokens=1000
        )

        # Sonnet: $0.003/1K prompt, $0.015/1K completion
        # Expected: (1K * 0.003) + (1K * 0.015) = 0.018
        assert metrics.cost_usd == 0.018

    def test_anthropic_claude_haiku_cost(self):
        """Claude Haiku should calculate correct costs."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="anthropic",
            model="claude-3-haiku",
            prompt_tokens=1000,
            completion_tokens=1000
        )

        # Haiku: $0.00025/1K prompt, $0.00125/1K completion
        # Expected: (1K * 0.00025) + (1K * 0.00125) = 0.0015
        assert metrics.cost_usd == 0.0015

    def test_model_prefix_matching(self):
        """Should match model by prefix (e.g., gpt-4-0613 → gpt-4)."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="openai",
            model="gpt-4-0613",  # Specific version
            prompt_tokens=1000,
            completion_tokens=1000
        )

        # Should match "gpt-4" pricing
        expected_cost = (1000 / 1000 * 0.03) + (1000 / 1000 * 0.06)
        assert metrics.cost_usd == expected_cost

    def test_unknown_provider_defaults_to_free(self):
        """Unknown providers should default to $0 for safety."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="unknown_provider",
            model="some_model",
            prompt_tokens=1000,
            completion_tokens=1000
        )

        assert metrics.cost_usd == 0.0

    def test_unknown_model_uses_provider_default(self):
        """Unknown models should use provider's default pricing."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="openai",
            model="unknown-model",
            prompt_tokens=1000,
            completion_tokens=1000
        )

        # Should use OpenAI default (GPT-3.5 pricing)
        expected_cost = (1000 / 1000 * 0.0015) + (1000 / 1000 * 0.002)
        assert metrics.cost_usd == expected_cost

    def test_custom_pricing_override(self):
        """Should support custom pricing override."""
        custom_pricing = {
            "custom_provider": {
                "custom_model": (0.01, 0.02)  # $0.01/1K prompt, $0.02/1K completion
            }
        }

        calc = CostCalculator(custom_pricing=custom_pricing)
        metrics = calc.calculate_cost(
            provider="custom_provider",
            model="custom_model",
            prompt_tokens=1000,
            completion_tokens=1000
        )

        expected_cost = (1000 / 1000 * 0.01) + (1000 / 1000 * 0.02)
        assert metrics.cost_usd == expected_cost

    def test_cost_metrics_properties(self):
        """Test CostMetrics properties and calculations."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="openai",
            model="gpt-4",
            prompt_tokens=1000,
            completion_tokens=500
        )

        assert metrics.prompt_tokens == 1000
        assert metrics.completion_tokens == 500
        assert metrics.total_tokens == 1500

        # Tokens per dollar
        tokens_per_dollar = metrics.tokens_per_dollar
        assert tokens_per_dollar > 0
        assert tokens_per_dollar == 1500 / metrics.cost_usd

    def test_estimate_cost(self):
        """Test cost estimation from text."""
        calc = CostCalculator()

        text = "This is a test prompt with some content."
        estimated_cost = calc.estimate_cost(
            provider="openai",
            model="gpt-4",
            text=text
        )

        # Should return a positive cost
        assert estimated_cost > 0

    def test_get_pricing(self):
        """Test retrieving pricing for provider/model."""
        calc = CostCalculator()

        # Get GPT-4 pricing
        prompt_cost, completion_cost = calc.get_pricing("openai", "gpt-4")
        assert prompt_cost == 0.03
        assert completion_cost == 0.06

        # Get default pricing for unknown model
        prompt_cost, completion_cost = calc.get_pricing("openai", "unknown")
        assert prompt_cost == 0.0015  # GPT-3.5 default
        assert completion_cost == 0.002

    def test_global_calculator(self):
        """Test global calculator instance."""
        calc = get_cost_calculator()
        assert isinstance(calc, CostCalculator)

        # Test convenience function
        metrics = calculate_cost(
            provider="llamaserver",
            model="test",
            prompt_tokens=100,
            completion_tokens=50
        )
        assert metrics.cost_usd == 0.0

    def test_case_insensitive_provider_and_model(self):
        """Provider and model names should be case-insensitive."""
        calc = CostCalculator()

        metrics1 = calc.calculate_cost(
            provider="OpenAI",
            model="GPT-4",
            prompt_tokens=1000,
            completion_tokens=1000
        )

        metrics2 = calc.calculate_cost(
            provider="openai",
            model="gpt-4",
            prompt_tokens=1000,
            completion_tokens=1000
        )

        assert metrics1.cost_usd == metrics2.cost_usd

    def test_rounding_precision(self):
        """Cost should be rounded to 6 decimal places."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="openai",
            model="gpt-3.5-turbo",
            prompt_tokens=333,  # Odd number to test rounding
            completion_tokens=666
        )

        # Should be rounded to 6 decimals
        assert len(str(metrics.cost_usd).split('.')[-1]) <= 6

    def test_zero_tokens(self):
        """Should handle zero tokens gracefully."""
        calc = CostCalculator()
        metrics = calc.calculate_cost(
            provider="openai",
            model="gpt-4",
            prompt_tokens=0,
            completion_tokens=0
        )

        assert metrics.cost_usd == 0.0
        assert metrics.total_tokens == 0
