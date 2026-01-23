package config

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// =============================================================================
// DEFAULT CONFIG TESTS
// =============================================================================

func TestDefaultExecutionConfig(t *testing.T) {
	// Test default values are set correctly.
	config := DefaultExecutionConfig()

	// Execution Limits
	assert.Equal(t, 20, config.MaxPlanSteps)
	assert.Equal(t, 2, config.MaxToolRetries)
	assert.Equal(t, 3, config.MaxLLMRetries)
	assert.Equal(t, 10, config.MaxLoopIterations)

	// Timeouts
	assert.Equal(t, 120, config.LLMTimeout)
	assert.Equal(t, 60, config.ExecutorTimeout)
	assert.Equal(t, 30, config.ToolTimeout)
	assert.Equal(t, 300, config.AgentTimeout)

	// Confidence Thresholds
	assert.Equal(t, 0.7, config.ClarificationThreshold)
	assert.Equal(t, 0.9, config.HighConfidenceThreshold)

	// Agent Behavior
	assert.True(t, config.MetaValidationEnabled)
	assert.Equal(t, 0.5, config.MetaValidationDelay)

	// Feature Flags
	assert.True(t, config.EnableLoopBack)
	assert.True(t, config.EnableArbiter)
	assert.True(t, config.SkipArbiterForReadOnly)
	assert.True(t, config.RequireConfirmationForDestructive)

	// Loop Control
	assert.Equal(t, 3, config.MaxReplanIterations)
	assert.Equal(t, 2, config.MaxLoopBackRejections)
	assert.True(t, config.ReplanOnPartialSuccess)

	// Determinism
	assert.Nil(t, config.LLMSeed)
	assert.True(t, config.StrictTransitionValidation)

	// Idempotency
	assert.True(t, config.EnableIdempotency)
	assert.Equal(t, 3600000, config.IdempotencyTTLMs)
	assert.True(t, config.EnforceIdempotencyForTools)
	assert.False(t, config.EnforceIdempotencyForAgents)

	// Logging
	assert.Equal(t, "INFO", config.LogLevel)
}

// =============================================================================
// FROM MAP TESTS
// =============================================================================

func TestExecutionConfigFromMapPartial(t *testing.T) {
	// Test creating config from partial map.
	configMap := map[string]any{
		"max_plan_steps": 30,
		"llm_timeout":    180,
	}

	config := ExecutionConfigFromMap(configMap)

	// Overridden values
	assert.Equal(t, 30, config.MaxPlanSteps)
	assert.Equal(t, 180, config.LLMTimeout)

	// Default values preserved
	assert.Equal(t, 2, config.MaxToolRetries)
	assert.Equal(t, 60, config.ExecutorTimeout)
}

func TestExecutionConfigFromMapUnknownKeysIgnored(t *testing.T) {
	// Unknown keys should be ignored.
	configMap := map[string]any{
		"max_plan_steps": 25,
		"unknown_key":    "should be ignored",
	}

	config := ExecutionConfigFromMap(configMap)

	assert.Equal(t, 25, config.MaxPlanSteps)
}

func TestExecutionConfigFromMapWithFloats(t *testing.T) {
	// Test handling float64 values (common from JSON).
	configMap := map[string]any{
		"max_plan_steps":   float64(15),
		"llm_timeout":      float64(90),
		"llm_seed":         float64(42),
	}

	config := ExecutionConfigFromMap(configMap)

	assert.Equal(t, 15, config.MaxPlanSteps)
	assert.Equal(t, 90, config.LLMTimeout)
	assert.NotNil(t, config.LLMSeed)
	assert.Equal(t, 42, *config.LLMSeed)
}

func TestExecutionConfigFromMapBools(t *testing.T) {
	// Test boolean values.
	configMap := map[string]any{
		"enable_loop_back": false,
		"enable_arbiter":   false,
	}

	config := ExecutionConfigFromMap(configMap)

	assert.False(t, config.EnableLoopBack)
	assert.False(t, config.EnableArbiter)
}

func TestExecutionConfigFromMapFloatThresholds(t *testing.T) {
	// Test float threshold values.
	configMap := map[string]any{
		"clarification_threshold":   0.8,
		"high_confidence_threshold": 0.95,
	}

	config := ExecutionConfigFromMap(configMap)

	assert.Equal(t, 0.8, config.ClarificationThreshold)
	assert.Equal(t, 0.95, config.HighConfidenceThreshold)
}

// =============================================================================
// TO MAP TESTS
// =============================================================================

func TestExecutionConfigToMap(t *testing.T) {
	// Test converting config to map.
	config := DefaultExecutionConfig()

	configMap := config.ToMap()

	assert.Equal(t, 20, configMap["max_plan_steps"])
	assert.Equal(t, 120, configMap["llm_timeout"])
	assert.Equal(t, true, configMap["enable_loop_back"])
	assert.Equal(t, "INFO", configMap["log_level"])
}

func TestExecutionConfigToMapWithSeed(t *testing.T) {
	// Test map includes seed when set.
	config := DefaultExecutionConfig()
	seed := 42
	config.LLMSeed = &seed

	configMap := config.ToMap()

	assert.Equal(t, 42, configMap["llm_seed"])
}

// =============================================================================
// GLOBAL CONFIG TESTS
// =============================================================================

func TestGetExecutionConfigDefault(t *testing.T) {
	// GetExecutionConfig should return defaults when not set.
	ResetExecutionConfig()

	config := GetExecutionConfig()

	assert.Equal(t, 20, config.MaxPlanSteps)
}

func TestSetAndGetExecutionConfig(t *testing.T) {
	// Test setting and getting global config.
	defer ResetExecutionConfig()

	customConfig := DefaultExecutionConfig()
	customConfig.MaxPlanSteps = 50

	SetExecutionConfig(customConfig)

	config := GetExecutionConfig()
	assert.Equal(t, 50, config.MaxPlanSteps)
}

func TestResetExecutionConfig(t *testing.T) {
	// Test resetting global config.
	customConfig := DefaultExecutionConfig()
	customConfig.MaxPlanSteps = 50
	SetExecutionConfig(customConfig)

	ResetExecutionConfig()

	config := GetExecutionConfig()
	assert.Equal(t, 20, config.MaxPlanSteps) // Back to default
}

// =============================================================================
// ROUNDTRIP TESTS
// =============================================================================

func TestConfigRoundtrip(t *testing.T) {
	// Test that config survives roundtrip through map.
	original := DefaultExecutionConfig()
	original.MaxPlanSteps = 35
	original.LLMTimeout = 200
	original.EnableLoopBack = false
	seed := 123
	original.LLMSeed = &seed

	configMap := original.ToMap()
	restored := ExecutionConfigFromMap(configMap)

	assert.Equal(t, original.MaxPlanSteps, restored.MaxPlanSteps)
	assert.Equal(t, original.LLMTimeout, restored.LLMTimeout)
	assert.Equal(t, original.EnableLoopBack, restored.EnableLoopBack)
	assert.NotNil(t, restored.LLMSeed)
	assert.Equal(t, *original.LLMSeed, *restored.LLMSeed)
}

// =============================================================================
// CONFIG PROVIDER TESTS
// =============================================================================

func TestDefaultConfigProvider(t *testing.T) {
	// Test DefaultConfigProvider uses global config.
	ResetExecutionConfig()
	defer ResetExecutionConfig()

	provider := &DefaultConfigProvider{}

	// Should return defaults when no global config set
	config := provider.GetExecutionConfig()
	assert.Equal(t, 20, config.MaxPlanSteps)

	// Set global config
	custom := DefaultExecutionConfig()
	custom.MaxPlanSteps = 50
	SetExecutionConfig(custom)

	// Should return the set config
	config = provider.GetExecutionConfig()
	assert.Equal(t, 50, config.MaxPlanSteps)
}

func TestStaticConfigProvider(t *testing.T) {
	// Test StaticConfigProvider returns static config.
	custom := DefaultExecutionConfig()
	custom.MaxPlanSteps = 100
	custom.LLMTimeout = 500

	provider := NewStaticConfigProvider(custom)

	config := provider.GetExecutionConfig()
	assert.Equal(t, 100, config.MaxPlanSteps)
	assert.Equal(t, 500, config.LLMTimeout)
}

func TestStaticConfigProviderNil(t *testing.T) {
	// Test StaticConfigProvider with nil config returns defaults.
	provider := &StaticConfigProvider{Config: nil}

	config := provider.GetExecutionConfig()
	assert.Equal(t, 20, config.MaxPlanSteps) // Default value
}

func TestConfigProviderInterface(t *testing.T) {
	// Test that both providers satisfy the interface.
	var provider ConfigProvider

	// DefaultConfigProvider
	provider = &DefaultConfigProvider{}
	assert.NotNil(t, provider.GetExecutionConfig())

	// StaticConfigProvider
	provider = NewStaticConfigProvider(DefaultExecutionConfig())
	assert.NotNil(t, provider.GetExecutionConfig())
}
