// Package runtime provides pipeline integration tests.
//
// These tests validate end-to-end pipeline execution with mock dependencies.
// They test linear execution, cyclic routing, bounds enforcement, and routing rules.
package runtime

import (
	"context"
	"testing"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// LINEAR EXECUTION TESTS
// =============================================================================

func TestPipeline_LinearExecution(t *testing.T) {
	// Test basic A → B → C → end execution.
	cfg := testutil.NewTestPipelineConfig("linear-test", "stageA", "stageB", "stageC")
	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Test input", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
	assert.False(t, result.Terminated)
	assert.GreaterOrEqual(t, mockLLM.GetCallCount(), 3) // One call per stage
}

func TestPipeline_LinearExecutionWithTools(t *testing.T) {
	// Test linear execution with tool-using agents.
	cfg := &config.PipelineConfig{
		Name:          "linear-with-tools",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasTools: true, DefaultNext: "stageC"},
			{Name: "stageC", StageOrder: 3, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	mockTools := testutil.NewMockToolExecutor().
		WithResult("read_file", map[string]any{"content": "file content"})

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, mockTools, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Test with tools", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
}

func TestPipeline_EmptyPipeline(t *testing.T) {
	// Test pipeline with no agents goes straight to end.
	cfg := &config.PipelineConfig{
		Name:          "empty-pipeline",
		MaxIterations: 5,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelope("Empty test")

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
}

// =============================================================================
// CYCLIC ROUTING TESTS
// =============================================================================

func TestPipeline_CyclicRouting(t *testing.T) {
	// Test cyclic routing with edge limits: A → B → C → A (limited loops).
	cfg := testutil.NewTestPipelineConfigWithCycle("cyclic-test", 2, "stageA", "stageB", "stageC")

	// Mock LLM returns "loop_back" first, then "proceed"
	mockLLM := testutil.NewMockLLMProvider()
	callCount := 0
	mockLLM.DefaultResponse = `{"verdict": "loop_back", "reasoning": "Need another iteration"}`

	// Override Generate to return proceed after some calls
	mockLLM.GenerateFunc = func(ctx context.Context, model, prompt string, options map[string]any) (string, error) {
		callCount++
		// After enough iterations, return proceed
		if callCount > 6 { // 2 loops * 3 stages = 6 calls
			return `{"verdict": "proceed", "reasoning": "Done iterating"}`, nil
		}
		return mockLLM.GenerateDefault(prompt)
	}

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Cyclic test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	// Should reach end after hitting edge limit or getting proceed
	assert.Equal(t, "end", result.CurrentStage)
	// Should have iterated multiple times
	assert.Greater(t, result.Iteration, 0)
}

func TestPipeline_CyclicRoutingEdgeLimitEnforced(t *testing.T) {
	// Test that edge limits are enforced.
	cfg := &config.PipelineConfig{
		Name:          "edge-limit-test",
		MaxIterations: 10,
		MaxLLMCalls:   50,
		MaxAgentHops:  100,
		EdgeLimits: []config.EdgeLimit{
			{From: "stageC", To: "stageA", MaxCount: 2},
		},
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "stageC"},
			{
				Name:       "stageC",
				StageOrder: 3,
				HasLLM:     true,
				ModelRole:  "default",
				RoutingRules: []config.RoutingRule{
					{Condition: "verdict", Value: "loop_back", Target: "stageA"},
				},
				DefaultNext: "end",
			},
		},
	}

	// Always return loop_back - edge limit should stop us
	mockLLM := testutil.NewMockLLMProvider()
	mockLLM.DefaultResponse = `{"verdict": "loop_back", "reasoning": "Always loop"}`

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Edge limit test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	// Edge limit should force routing to "end" after 2 loops
	assert.Equal(t, "end", result.CurrentStage)
}

// =============================================================================
// ROUTING RULES TESTS
// =============================================================================

func TestPipeline_RoutingRulesVerdictProceed(t *testing.T) {
	// Test routing based on verdict = "proceed".
	cfg := &config.PipelineConfig{
		Name:          "routing-proceed",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents: []*config.AgentConfig{
			{
				Name:       "stageA",
				StageOrder: 1,
				HasLLM:     true,
				ModelRole:  "default",
				RoutingRules: []config.RoutingRule{
					{Condition: "verdict", Value: "proceed", Target: "stageB"},
					{Condition: "verdict", Value: "skip", Target: "stageC"},
				},
				DefaultNext: "stageB",
			},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{Name: "stageC", StageOrder: 3, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	mockLLM.DefaultResponse = `{"verdict": "proceed", "reasoning": "Proceeding normally"}`

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Routing test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
	// stageB should have output (was visited via proceed routing)
	assert.Contains(t, result.Outputs, "stageA")
	assert.Contains(t, result.Outputs, "stageB")
}

func TestPipeline_RoutingRulesDefaultNext(t *testing.T) {
	// Test default_next when no routing rule matches.
	cfg := &config.PipelineConfig{
		Name:          "routing-default",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents: []*config.AgentConfig{
			{
				Name:       "stageA",
				StageOrder: 1,
				HasLLM:     true,
				ModelRole:  "default",
				RoutingRules: []config.RoutingRule{
					{Condition: "verdict", Value: "never_matches", Target: "stageC"},
				},
				DefaultNext: "stageB", // This should be used
			},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
			{Name: "stageC", StageOrder: 3, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	mockLLM.DefaultResponse = `{"verdict": "something_else", "reasoning": "No match"}`

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Default routing test", []string{"stageA", "stageB", "stageC", "end"})

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
	// stageB visited (default_next), stageC not visited
	assert.Contains(t, result.Outputs, "stageB")
}

// =============================================================================
// BOUNDS ENFORCEMENT TESTS
// =============================================================================

func TestPipeline_BoundsMaxIterations(t *testing.T) {
	// Test that MaxIterations is enforced.
	cfg := &config.PipelineConfig{
		Name:          "max-iterations-test",
		MaxIterations: 2, // Low limit
		MaxLLMCalls:   50,
		MaxAgentHops:  100,
		Agents: []*config.AgentConfig{
			{
				Name:       "stageA",
				StageOrder: 1,
				HasLLM:     true,
				ModelRole:  "default",
				RoutingRules: []config.RoutingRule{
					{Condition: "verdict", Value: "loop_back", Target: "stageA"},
				},
				DefaultNext: "end",
			},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	mockLLM.DefaultResponse = `{"verdict": "loop_back", "reasoning": "Keep looping"}`

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Iteration limit test", []string{"stageA", "end"})

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	// Should have stopped due to iteration limit
	assert.LessOrEqual(t, result.Iteration, cfg.MaxIterations)
}

func TestPipeline_BoundsMaxLLMCalls(t *testing.T) {
	// Test that MaxLLMCalls is enforced.
	cfg := &config.PipelineConfig{
		Name:          "max-llm-calls-test",
		MaxIterations: 100,
		MaxLLMCalls:   3, // Low limit
		MaxAgentHops:  100,
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "stageC"},
			{Name: "stageC", StageOrder: 3, HasLLM: true, ModelRole: "default", DefaultNext: "stageD"},
			{Name: "stageD", StageOrder: 4, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("LLM limit test", []string{"stageA", "stageB", "stageC", "stageD", "end"})
	env.MaxLLMCalls = 3

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	// Should have stopped at or before MaxLLMCalls
	assert.LessOrEqual(t, result.LLMCallCount, cfg.MaxLLMCalls)
}

func TestPipeline_BoundsMaxAgentHops(t *testing.T) {
	// Test that MaxAgentHops is enforced.
	cfg := &config.PipelineConfig{
		Name:          "max-hops-test",
		MaxIterations: 100,
		MaxLLMCalls:   100,
		MaxAgentHops:  5, // Low limit
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "stageC"},
			{Name: "stageC", StageOrder: 3, HasLLM: true, ModelRole: "default", DefaultNext: "stageD"},
			{Name: "stageD", StageOrder: 4, HasLLM: true, ModelRole: "default", DefaultNext: "stageE"},
			{Name: "stageE", StageOrder: 5, HasLLM: true, ModelRole: "default", DefaultNext: "stageF"},
			{Name: "stageF", StageOrder: 6, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Hop limit test",
		[]string{"stageA", "stageB", "stageC", "stageD", "stageE", "stageF", "end"})
	env.MaxAgentHops = 5

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	// Should have stopped at or before MaxAgentHops
	assert.LessOrEqual(t, result.AgentHopCount, cfg.MaxAgentHops)
}

// =============================================================================
// TERMINAL REASON TESTS
// =============================================================================

func TestPipeline_TerminalReasonCompleted(t *testing.T) {
	// Test terminal reason when pipeline completes normally.
	cfg := testutil.NewTestPipelineConfig("completed-test", "stageA")
	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Completion test", []string{"stageA", "end"})

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
	// Terminal reason should be nil or COMPLETED for normal completion
	if result.TerminalReason_ != nil {
		assert.Equal(t, envelope.TerminalReasonCompleted, *result.TerminalReason_)
	}
}

// =============================================================================
// PERSISTENCE TESTS
// =============================================================================

func TestPipeline_PersistenceOnCompletion(t *testing.T) {
	// Test that state is persisted after execution.
	cfg := testutil.NewTestPipelineConfig("persistence-test", "stageA")
	mockLLM := testutil.NewMockLLMProvider()
	mockPersistence := testutil.NewMockPersistence()

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)
	runtime.Persistence = mockPersistence

	env := testutil.NewTestEnvelopeWithStages("Persistence test", []string{"stageA", "end"})

	_, err = runtime.Run(context.Background(), env, "thread-persist")

	require.NoError(t, err)
	assert.Greater(t, mockPersistence.SaveCount, 0)
	assert.NotNil(t, mockPersistence.GetState("thread-persist"))
}

func TestPipeline_PersistenceError(t *testing.T) {
	// Test that persistence errors don't fail execution.
	cfg := testutil.NewTestPipelineConfig("persistence-error-test", "stageA")
	mockLLM := testutil.NewMockLLMProvider()
	mockPersistence := testutil.NewMockPersistence().
		WithSaveError(assert.AnError)

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)
	runtime.Persistence = mockPersistence

	env := testutil.NewTestEnvelopeWithStages("Persistence error test", []string{"stageA", "end"})

	// Should complete despite persistence error
	result, err := runtime.Run(context.Background(), env, "thread-error")

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
}

// =============================================================================
// EVENT EMISSION TESTS
// =============================================================================

func TestPipeline_EventEmission(t *testing.T) {
	// Test that events are emitted during execution.
	cfg := testutil.NewTestPipelineConfig("event-test", "stageA", "stageB")
	mockLLM := testutil.NewMockLLMProvider()
	mockEvents := testutil.NewMockEventContext()

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)
	runtime.SetEventContext(mockEvents)

	env := testutil.NewTestEnvelopeWithStages("Event test", []string{"stageA", "stageB", "end"})

	_, err = runtime.Run(context.Background(), env, "thread-events")

	require.NoError(t, err)

	// Check events were captured
	startedAgents := mockEvents.GetStartedAgents()
	completedAgents := mockEvents.GetCompletedAgents()

	assert.Contains(t, startedAgents, "stageA")
	assert.Contains(t, startedAgents, "stageB")
	assert.Contains(t, completedAgents, "stageA")
	assert.Contains(t, completedAgents, "stageB")
}

// =============================================================================
// CONTEXT CANCELLATION TESTS
// =============================================================================

func TestPipeline_ContextCancellation(t *testing.T) {
	// Test that pipeline respects context cancellation.
	cfg := testutil.NewTestPipelineConfig("cancel-test", "stageA", "stageB", "stageC")

	mockLLM := testutil.NewMockLLMProvider().
		WithDelay(100 * time.Millisecond) // Slow responses

	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Cancel test", []string{"stageA", "stageB", "stageC", "end"})

	// Cancel after 50ms
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()

	_, err = runtime.Run(ctx, env, "thread-cancel")

	// Should error due to context cancellation
	assert.Error(t, err)
}

// =============================================================================
// STREAMING EXECUTION TESTS
// =============================================================================

func TestPipeline_StreamingExecution(t *testing.T) {
	// Test streaming execution with RunWithStream.
	cfg := testutil.NewTestPipelineConfig("stream-test", "stageA", "stageB")
	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Stream test", []string{"stageA", "stageB", "end"})

	outputChan, err := runtime.RunWithStream(context.Background(), env, "thread-stream")
	require.NoError(t, err)

	// Collect all outputs
	var outputs []StageOutput
	for output := range outputChan {
		outputs = append(outputs, output)
	}

	// Should have outputs for each stage plus end marker
	assert.NotEmpty(t, outputs)

	// Last output should be end marker
	lastOutput := outputs[len(outputs)-1]
	assert.Equal(t, "__end__", lastOutput.Stage)
}
