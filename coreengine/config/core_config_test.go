package config

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// =============================================================================
// DEFAULT CONFIG TESTS
// =============================================================================

func TestDefaultCoreConfig(t *testing.T) {
	// Test default values are set correctly.
	config := DefaultCoreConfig()

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

func TestCoreConfigFromMapPartial(t *testing.T) {
	// Test creating config from partial map.
	configMap := map[string]any{
		"max_plan_steps": 30,
		"llm_timeout":    180,
	}

	config := CoreConfigFromMap(configMap)

	// Overridden values
	assert.Equal(t, 30, config.MaxPlanSteps)
	assert.Equal(t, 180, config.LLMTimeout)

	// Default values preserved
	assert.Equal(t, 2, config.MaxToolRetries)
	assert.Equal(t, 60, config.ExecutorTimeout)
}

func TestCoreConfigFromMapUnknownKeysIgnored(t *testing.T) {
	// Unknown keys should be ignored.
	configMap := map[string]any{
		"max_plan_steps": 25,
		"unknown_key":    "should be ignored",
	}

	config := CoreConfigFromMap(configMap)

	assert.Equal(t, 25, config.MaxPlanSteps)
}

func TestCoreConfigFromMapWithFloats(t *testing.T) {
	// Test handling float64 values (common from JSON).
	configMap := map[string]any{
		"max_plan_steps":   float64(15),
		"llm_timeout":      float64(90),
		"llm_seed":         float64(42),
	}

	config := CoreConfigFromMap(configMap)

	assert.Equal(t, 15, config.MaxPlanSteps)
	assert.Equal(t, 90, config.LLMTimeout)
	assert.NotNil(t, config.LLMSeed)
	assert.Equal(t, 42, *config.LLMSeed)
}

func TestCoreConfigFromMapBools(t *testing.T) {
	// Test boolean values.
	configMap := map[string]any{
		"enable_loop_back": false,
		"enable_arbiter":   false,
	}

	config := CoreConfigFromMap(configMap)

	assert.False(t, config.EnableLoopBack)
	assert.False(t, config.EnableArbiter)
}

func TestCoreConfigFromMapFloatThresholds(t *testing.T) {
	// Test float threshold values.
	configMap := map[string]any{
		"clarification_threshold":   0.8,
		"high_confidence_threshold": 0.95,
	}

	config := CoreConfigFromMap(configMap)

	assert.Equal(t, 0.8, config.ClarificationThreshold)
	assert.Equal(t, 0.95, config.HighConfidenceThreshold)
}

// =============================================================================
// TO MAP TESTS
// =============================================================================

func TestCoreConfigToMap(t *testing.T) {
	// Test converting config to map.
	config := DefaultCoreConfig()

	configMap := config.ToMap()

	assert.Equal(t, 20, configMap["max_plan_steps"])
	assert.Equal(t, 120, configMap["llm_timeout"])
	assert.Equal(t, true, configMap["enable_loop_back"])
	assert.Equal(t, "INFO", configMap["log_level"])
}

func TestCoreConfigToMapWithSeed(t *testing.T) {
	// Test map includes seed when set.
	config := DefaultCoreConfig()
	seed := 42
	config.LLMSeed = &seed

	configMap := config.ToMap()

	assert.Equal(t, 42, configMap["llm_seed"])
}

// =============================================================================
// GLOBAL CONFIG TESTS
// =============================================================================

func TestGetCoreConfigDefault(t *testing.T) {
	// GetCoreConfig should return defaults when not set.
	ResetCoreConfig()

	config := GetCoreConfig()

	assert.Equal(t, 20, config.MaxPlanSteps)
}

func TestSetAndGetCoreConfig(t *testing.T) {
	// Test setting and getting global config.
	defer ResetCoreConfig()

	customConfig := DefaultCoreConfig()
	customConfig.MaxPlanSteps = 50

	SetCoreConfig(customConfig)

	config := GetCoreConfig()
	assert.Equal(t, 50, config.MaxPlanSteps)
}

func TestResetCoreConfig(t *testing.T) {
	// Test resetting global config.
	customConfig := DefaultCoreConfig()
	customConfig.MaxPlanSteps = 50
	SetCoreConfig(customConfig)

	ResetCoreConfig()

	config := GetCoreConfig()
	assert.Equal(t, 20, config.MaxPlanSteps) // Back to default
}

// =============================================================================
// ROUNDTRIP TESTS
// =============================================================================

func TestConfigRoundtrip(t *testing.T) {
	// Test that config survives roundtrip through map.
	original := DefaultCoreConfig()
	original.MaxPlanSteps = 35
	original.LLMTimeout = 200
	original.EnableLoopBack = false
	seed := 123
	original.LLMSeed = &seed

	configMap := original.ToMap()
	restored := CoreConfigFromMap(configMap)

	assert.Equal(t, original.MaxPlanSteps, restored.MaxPlanSteps)
	assert.Equal(t, original.LLMTimeout, restored.LLMTimeout)
	assert.Equal(t, original.EnableLoopBack, restored.EnableLoopBack)
	assert.NotNil(t, restored.LLMSeed)
	assert.Equal(t, *original.LLMSeed, *restored.LLMSeed)
}
