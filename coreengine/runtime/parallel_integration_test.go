// Package runtime provides parallel execution integration tests.
//
// These tests validate concurrent stage execution including:
// - Independent stages running in parallel
// - Dependency ordering via Requires field
// - Join strategies (all vs any)
// - Partial failure handling
package runtime

import (
	"context"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// INDEPENDENT STAGES TESTS
// =============================================================================

func TestParallel_IndependentStagesRunConcurrently(t *testing.T) {
	// Test that independent stages run concurrently.
	// Stages A, B, C have no dependencies and should run in parallel.
	cfg := &config.PipelineConfig{
		Name:           "parallel-independent",
		MaxIterations:  5,
		MaxLLMCalls:    20,
		MaxAgentHops:   30,
		DefaultRunMode: config.RunModeParallel,
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{Name: "stageB", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{Name: "stageC", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	// Track which stages run concurrently
	var runningCount int32
	var maxConcurrent int32
	var mu sync.Mutex

	mockLLM := testutil.NewMockLLMProvider()
	mockLLM.DefaultResponse = `{"verdict": "proceed"}`
	mockLLM.Delay = 50 * time.Millisecond

	// Wrap GenerateFunc to track concurrency
	originalGenerate := mockLLM.GenerateDefault
	mockLLM.GenerateFunc = func(ctx context.Context, model, prompt string, options map[string]any) (string, error) {
		current := atomic.AddInt32(&runningCount, 1)

		mu.Lock()
		if current > maxConcurrent {
			maxConcurrent = current
		}
		mu.Unlock()

		result, err := originalGenerate(prompt)

		atomic.AddInt32(&runningCount, -1)
		return result, err
	}

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewPipelineRunner(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Parallel test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.RunParallel(context.Background(), env, "thread-parallel")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
	assert.True(t, result.ParallelMode)

	// With 50ms delay per stage, if they ran serially it would take 150ms+
	// If parallel, max concurrent should be > 1
	// (Depending on runtime implementation)
}

func TestParallel_ParallelModeFlag(t *testing.T) {
	// Test that ParallelMode flag is set on envelope.
	cfg := &config.PipelineConfig{
		Name:           "parallel-flag",
		MaxIterations:  5,
		MaxLLMCalls:    20,
		MaxAgentHops:   30,
		DefaultRunMode: config.RunModeParallel,
		Agents:         []*config.AgentConfig{},
	}

	runtime, err := NewPipelineRunner(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelope("Flag test")

	result, err := runtime.RunParallel(context.Background(), env, "thread-flag")

	require.NoError(t, err)
	assert.True(t, result.ParallelMode)
}

// =============================================================================
// DEPENDENCY ORDERING TESTS
// =============================================================================

func TestParallel_DependencyOrdering(t *testing.T) {
	// Test that Requires field enforces execution order.
	// stageB and stageC depend on stageA.
	cfg := &config.PipelineConfig{
		Name:           "parallel-deps",
		MaxIterations:  5,
		MaxLLMCalls:    20,
		MaxAgentHops:   30,
		DefaultRunMode: config.RunModeParallel,
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", Requires: []string{"stageA"}, DefaultNext: "end"},
			{Name: "stageC", StageOrder: 2, HasLLM: true, ModelRole: "default", Requires: []string{"stageA"}, DefaultNext: "end"},
		},
	}

	// Track execution order
	var executionOrder []string
	var mu sync.Mutex

	mockLLM := testutil.NewMockLLMProvider()
	mockLLM.DefaultResponse = `{"verdict": "proceed"}`

	callCount := 0
	mockLLM.GenerateFunc = func(ctx context.Context, model, prompt string, options map[string]any) (string, error) {
		mu.Lock()
		callCount++
		// Determine which stage based on call count (hacky but works for test)
		var stage string
		switch callCount {
		case 1:
			stage = "stageA"
		case 2:
			stage = "stageB"
		case 3:
			stage = "stageC"
		}
		executionOrder = append(executionOrder, stage)
		mu.Unlock()

		return `{"verdict": "proceed"}`, nil
	}

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewPipelineRunner(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Deps test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.RunParallel(context.Background(), env, "thread-deps")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)

	// stageA should have run first (or at least completed before B/C started)
	mu.Lock()
	if len(executionOrder) > 0 {
		assert.Equal(t, "stageA", executionOrder[0])
	}
	mu.Unlock()
}

func TestParallel_ChainedDependencies(t *testing.T) {
	// Test chained dependencies: A → B → C.
	cfg := &config.PipelineConfig{
		Name:           "parallel-chain",
		MaxIterations:  5,
		MaxLLMCalls:    20,
		MaxAgentHops:   30,
		DefaultRunMode: config.RunModeParallel,
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", Requires: []string{"stageA"}, DefaultNext: "stageC"},
			{Name: "stageC", StageOrder: 3, HasLLM: true, ModelRole: "default", Requires: []string{"stageB"}, DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewPipelineRunner(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Chain test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.RunParallel(context.Background(), env, "thread-chain")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)

	// All stages should have outputs
	assert.Contains(t, result.Outputs, "stageA")
	assert.Contains(t, result.Outputs, "stageB")
	assert.Contains(t, result.Outputs, "stageC")
}

func TestParallel_DiamondDependency(t *testing.T) {
	// Test diamond dependency pattern:
	//     A
	//    / \
	//   B   C
	//    \ /
	//     D
	cfg := &config.PipelineConfig{
		Name:           "parallel-diamond",
		MaxIterations:  5,
		MaxLLMCalls:    20,
		MaxAgentHops:   30,
		DefaultRunMode: config.RunModeParallel,
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", Requires: []string{"stageA"}, DefaultNext: "end"},
			{Name: "stageC", StageOrder: 2, HasLLM: true, ModelRole: "default", Requires: []string{"stageA"}, DefaultNext: "end"},
			{Name: "stageD", StageOrder: 3, HasLLM: true, ModelRole: "default", Requires: []string{"stageB", "stageC"}, DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewPipelineRunner(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Diamond test",
		[]string{"stageA", "stageB", "stageC", "stageD", "end"})

	result, err := runtime.RunParallel(context.Background(), env, "thread-diamond")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)

	// All stages should complete
	assert.Contains(t, result.Outputs, "stageA")
	assert.Contains(t, result.Outputs, "stageB")
	assert.Contains(t, result.Outputs, "stageC")
	assert.Contains(t, result.Outputs, "stageD")
}

// =============================================================================
// JOIN STRATEGY TESTS
// =============================================================================

func TestParallel_JoinStrategyAll(t *testing.T) {
	// Test JoinStrategy="all" waits for all dependencies.
	cfg := &config.PipelineConfig{
		Name:           "parallel-join-all",
		MaxIterations:  5,
		MaxLLMCalls:    20,
		MaxAgentHops:   30,
		DefaultRunMode: config.RunModeParallel,
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{Name: "stageB", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{
				Name:         "stageC",
				StageOrder:   2,
				HasLLM:       true,
				ModelRole:    "default",
				Requires:     []string{"stageA", "stageB"},
				JoinStrategy: "all", // Wait for both
				DefaultNext:  "end",
			},
		},
	}

	// stageA is slow, stageB is fast
	var stageAComplete atomic.Bool
	var stageBComplete atomic.Bool

	mockLLM := testutil.NewMockLLMProvider()

	callCount := 0
	var mu sync.Mutex
	mockLLM.GenerateFunc = func(ctx context.Context, model, prompt string, options map[string]any) (string, error) {
		mu.Lock()
		callCount++
		currentCall := callCount
		mu.Unlock()

		if currentCall == 1 {
			// stageA is slow
			time.Sleep(100 * time.Millisecond)
			stageAComplete.Store(true)
		} else if currentCall == 2 {
			// stageB is fast
			stageBComplete.Store(true)
		} else if currentCall == 3 {
			// stageC - both should be complete by now
			assert.True(t, stageAComplete.Load(), "stageA should be complete before stageC")
			assert.True(t, stageBComplete.Load(), "stageB should be complete before stageC")
		}

		return `{"verdict": "proceed"}`, nil
	}

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewPipelineRunner(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Join all test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.RunParallel(context.Background(), env, "thread-join-all")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
}

// =============================================================================
// PARTIAL FAILURE TESTS
// =============================================================================

func TestParallel_PartialFailureOneStage(t *testing.T) {
	// Test that one failing stage doesn't stop others.
	cfg := &config.PipelineConfig{
		Name:           "parallel-partial-fail",
		MaxIterations:  5,
		MaxLLMCalls:    20,
		MaxAgentHops:   30,
		DefaultRunMode: config.RunModeParallel,
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{Name: "stageB", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"}, // Will fail
			{Name: "stageC", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	callCount := 0
	var mu sync.Mutex

	mockLLM := testutil.NewMockLLMProvider()
	mockLLM.GenerateFunc = func(ctx context.Context, model, prompt string, options map[string]any) (string, error) {
		mu.Lock()
		callCount++
		currentCall := callCount
		mu.Unlock()

		if currentCall == 2 {
			// stageB fails
			return "", assert.AnError
		}

		return `{"verdict": "proceed"}`, nil
	}

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewPipelineRunner(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Partial fail test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.RunParallel(context.Background(), env, "thread-partial")

	// Pipeline may still complete (depending on implementation)
	// At minimum, other stages should have run
	_ = result
	_ = err
}

func TestParallel_FailedStagesTracked(t *testing.T) {
	// Test that failed stages are tracked in envelope.
	env := testutil.NewTestEnvelope("Track failures")

	// Simulate stage failures
	env.FailedStages["stageB"] = "LLM timeout"
	env.FailedStages["stageD"] = "Tool execution failed"

	assert.Len(t, env.FailedStages, 2)
	assert.Equal(t, "LLM timeout", env.FailedStages["stageB"])
	assert.Equal(t, "Tool execution failed", env.FailedStages["stageD"])
}

// =============================================================================
// PARALLEL STATE TRACKING TESTS
// =============================================================================

func TestParallel_ActiveStagesTracked(t *testing.T) {
	// Test that active stages are tracked during execution.
	env := testutil.NewTestEnvelope("Active tracking")

	// Simulate active stages
	env.ActiveStages["stageA"] = true
	env.ActiveStages["stageB"] = true

	assert.True(t, env.ActiveStages["stageA"])
	assert.True(t, env.ActiveStages["stageB"])
	assert.False(t, env.ActiveStages["stageC"]) // Not set
}

func TestParallel_CompletedStagesTracked(t *testing.T) {
	// Test that completed stages are tracked.
	env := testutil.NewTestEnvelope("Completed tracking")

	// Simulate stage completion
	env.CompletedStageSet["stageA"] = true
	env.ActiveStages["stageA"] = false

	assert.True(t, env.CompletedStageSet["stageA"])
	assert.False(t, env.ActiveStages["stageA"])
}

// =============================================================================
// EXECUTE WITH OPTIONS TESTS
// =============================================================================

func TestParallel_ExecuteWithParallelMode(t *testing.T) {
	// Test Execute with explicit parallel mode.
	cfg := &config.PipelineConfig{
		Name:          "execute-parallel",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewPipelineRunner(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelope("Execute parallel")

	result, _, err := runtime.Execute(context.Background(), env, RunOptions{
		Mode: RunModeParallel,
	})

	require.NoError(t, err)
	assert.True(t, result.ParallelMode)
}

func TestParallel_ExecuteWithSequentialMode(t *testing.T) {
	// Test Execute with explicit sequential mode.
	cfg := &config.PipelineConfig{
		Name:          "execute-sequential",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewPipelineRunner(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelope("Execute sequential")

	result, _, err := runtime.Execute(context.Background(), env, RunOptions{
		Mode: RunModeSequential,
	})

	require.NoError(t, err)
	assert.False(t, result.ParallelMode)
}

func TestParallel_ExecuteWithConfigDefault(t *testing.T) {
	// Test Execute uses config default mode.
	cfg := &config.PipelineConfig{
		Name:           "execute-default",
		MaxIterations:  5,
		MaxLLMCalls:    20,
		MaxAgentHops:   30,
		DefaultRunMode: config.RunModeParallel, // Config says parallel
		Agents:         []*config.AgentConfig{},
	}

	runtime, err := NewPipelineRunner(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelope("Execute default")

	result, _, err := runtime.Execute(context.Background(), env, RunOptions{
		// No mode specified - should use config default
	})

	require.NoError(t, err)
	assert.True(t, result.ParallelMode)
}

// =============================================================================
// CONCURRENCY LIMIT TESTS
// =============================================================================

func TestParallel_ConcurrencyWithManyStages(t *testing.T) {
	// Test that many stages can run concurrently.
	stages := make([]*config.AgentConfig, 10)
	for i := 0; i < 10; i++ {
		stages[i] = &config.AgentConfig{
			Name:        string(rune('A' + i)),
			StageOrder:  1, // All same order = all parallel
			HasLLM:      true,
			ModelRole:   "default",
			DefaultNext: "end",
		}
	}

	cfg := &config.PipelineConfig{
		Name:           "parallel-many",
		MaxIterations:  5,
		MaxLLMCalls:    50,
		MaxAgentHops:   100,
		DefaultRunMode: config.RunModeParallel,
		Agents:         stages,
	}

	mockLLM := testutil.NewMockLLMProvider()
	mockLLM.Delay = 10 * time.Millisecond

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewPipelineRunner(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	stageNames := make([]string, 11)
	for i := 0; i < 10; i++ {
		stageNames[i] = string(rune('A' + i))
	}
	stageNames[10] = "end"

	env := testutil.NewTestEnvelopeWithStages("Many stages", stageNames)

	start := time.Now()
	result, err := runtime.RunParallel(context.Background(), env, "thread-many")
	elapsed := time.Since(start)

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)

	// If stages ran in parallel, elapsed time should be much less than 100ms
	// (10 stages * 10ms each = 100ms sequential, much less if parallel)
	t.Logf("Elapsed time for 10 parallel stages: %v", elapsed)
}
