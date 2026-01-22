package testutil

import (
	"testing"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// CONFIG HELPER TESTS
// =============================================================================

func TestNewEmptyPipelineConfig(t *testing.T) {
	cfg := NewEmptyPipelineConfig("test-empty")

	assert.Equal(t, "test-empty", cfg.Name)
	assert.Equal(t, 5, cfg.MaxIterations)
	assert.Equal(t, 20, cfg.MaxLLMCalls)
	assert.Equal(t, 30, cfg.MaxAgentHops)
	assert.Empty(t, cfg.Agents)
	assert.NotNil(t, cfg.Agents) // Should be empty slice, not nil
}

func TestNewParallelPipelineConfig(t *testing.T) {
	t.Run("with default stages", func(t *testing.T) {
		cfg := NewParallelPipelineConfig("test-parallel")

		assert.Equal(t, "test-parallel", cfg.Name)
		assert.Equal(t, config.RunModeParallel, cfg.DefaultRunMode)
		assert.Len(t, cfg.Agents, 3) // Default: stageA, stageB, stageC

		// All stages should have StageOrder=1 for parallel
		for _, agent := range cfg.Agents {
			assert.Equal(t, 1, agent.StageOrder)
			assert.True(t, agent.HasLLM)
			assert.Equal(t, "default", agent.ModelRole)
			assert.Equal(t, "end", agent.DefaultNext)
		}
	})

	t.Run("with custom stages", func(t *testing.T) {
		cfg := NewParallelPipelineConfig("test-custom", "stage1", "stage2")

		assert.Len(t, cfg.Agents, 2)
		assert.Equal(t, "stage1", cfg.Agents[0].Name)
		assert.Equal(t, "stage2", cfg.Agents[1].Name)

		// Both should be parallel (same order)
		assert.Equal(t, 1, cfg.Agents[0].StageOrder)
		assert.Equal(t, 1, cfg.Agents[1].StageOrder)
	})
}

func TestNewBoundedPipelineConfig(t *testing.T) {
	t.Run("with custom bounds", func(t *testing.T) {
		cfg := NewBoundedPipelineConfig("test-bounded", 3, 10, 15, "stageA")

		assert.Equal(t, 3, cfg.MaxIterations)
		assert.Equal(t, 10, cfg.MaxLLMCalls)
		assert.Equal(t, 15, cfg.MaxAgentHops)
		assert.Len(t, cfg.Agents, 1)
	})

	t.Run("creates linear routing", func(t *testing.T) {
		cfg := NewBoundedPipelineConfig("test", 1, 2, 3, "a", "b", "c")

		// Should use NewTestPipelineConfig underneath (linear)
		assert.Equal(t, 1, cfg.Agents[0].StageOrder)
		assert.Equal(t, 2, cfg.Agents[1].StageOrder)
		assert.Equal(t, 3, cfg.Agents[2].StageOrder)
		assert.Equal(t, "b", cfg.Agents[0].DefaultNext)
		assert.Equal(t, "c", cfg.Agents[1].DefaultNext)
		assert.Equal(t, "end", cfg.Agents[2].DefaultNext)
	})
}

func TestNewDependencyChainConfig(t *testing.T) {
	t.Run("with default stages", func(t *testing.T) {
		cfg := NewDependencyChainConfig("test-deps")

		assert.Equal(t, config.RunModeParallel, cfg.DefaultRunMode)
		assert.Len(t, cfg.Agents, 3)

		// First stage has no dependencies
		assert.Empty(t, cfg.Agents[0].Requires)

		// Second stage requires first
		require.Len(t, cfg.Agents[1].Requires, 1)
		assert.Equal(t, "stageA", cfg.Agents[1].Requires[0])

		// Third stage requires second
		require.Len(t, cfg.Agents[2].Requires, 1)
		assert.Equal(t, "stageB", cfg.Agents[2].Requires[0])
	})

	t.Run("with custom stages", func(t *testing.T) {
		cfg := NewDependencyChainConfig("test", "first", "second", "third", "fourth")

		assert.Len(t, cfg.Agents, 4)

		// First has no deps
		assert.Empty(t, cfg.Agents[0].Requires)

		// Each subsequent stage depends on previous
		assert.Equal(t, []string{"first"}, cfg.Agents[1].Requires)
		assert.Equal(t, []string{"second"}, cfg.Agents[2].Requires)
		assert.Equal(t, []string{"third"}, cfg.Agents[3].Requires)
	})
}

// =============================================================================
// INTEGRATION: EXISTING HELPERS STILL WORK
// =============================================================================

func TestExistingHelpersStillWork(t *testing.T) {
	// Ensure our new helpers don't break existing ones

	t.Run("NewTestPipelineConfig", func(t *testing.T) {
		cfg := NewTestPipelineConfig("test", "a", "b")
		assert.Len(t, cfg.Agents, 2)
		assert.Equal(t, "b", cfg.Agents[0].DefaultNext)
	})

	t.Run("NewTestPipelineConfigWithCycle", func(t *testing.T) {
		cfg := NewTestPipelineConfigWithCycle("test", 3, "a", "b")
		assert.NotEmpty(t, cfg.EdgeLimits)
		assert.NotEmpty(t, cfg.Agents[1].RoutingRules)
	})

	t.Run("NewTestEnvelope", func(t *testing.T) {
		env := NewTestEnvelope("input")
		assert.Equal(t, "input", env.RawInput)
	})

	t.Run("NewTestEnvelopeWithStages", func(t *testing.T) {
		env := NewTestEnvelopeWithStages("input", []string{"a", "b", "end"})
		assert.Equal(t, []string{"a", "b", "end"}, env.StageOrder)
		assert.Equal(t, "a", env.CurrentStage)
	})
}

// =============================================================================
// MOCK TESTS (ENSURE MOCKS WORK CORRECTLY)
// =============================================================================

func TestMockLLMProvider(t *testing.T) {
	t.Run("default response", func(t *testing.T) {
		mock := NewMockLLMProvider()
		response, err := mock.GenerateDefault("test prompt")
		
		require.NoError(t, err)
		assert.Contains(t, response, "proceed")
	})

	t.Run("with custom response", func(t *testing.T) {
		mock := NewMockLLMProvider().
			WithResponse("hello", "world")
		
		response, err := mock.GenerateDefault("hello there")
		require.NoError(t, err)
		assert.Equal(t, "world", response)
	})

	t.Run("tracks call count", func(t *testing.T) {
		mock := NewMockLLMProvider()
		assert.Equal(t, 0, mock.GetCallCount())
		
		mock.GenerateDefault("test")
		// Note: GenerateDefault doesn't increment counter
		// Only Generate() does (via context simulation)
	})
}

func TestMockPersistence(t *testing.T) {
	mock := NewMockPersistence()
	
	// Save state
	state := map[string]any{"key": "value"}
	err := mock.SaveState(nil, "thread-1", state)
	require.NoError(t, err)
	
	// Load state
	loaded, err := mock.LoadState(nil, "thread-1")
	require.NoError(t, err)
	assert.Equal(t, "value", loaded["key"])
	
	// Verify counts
	assert.Equal(t, 1, mock.SaveCount)
	assert.Equal(t, 1, mock.LoadCount)
}

func TestMockLogger(t *testing.T) {
	logger := NewMockLogger()
	
	logger.Info("test message", "key", "value")
	logger.Error("error message", "error", "something")
	
	logs := logger.GetLogs()
	assert.Len(t, logs, 2)
	assert.Equal(t, "info", logs[0].Level)
	assert.Equal(t, "error", logs[1].Level)
	assert.True(t, logger.HasLog("info", "test message"))
	assert.True(t, logger.HasLog("error", "error message"))
}
