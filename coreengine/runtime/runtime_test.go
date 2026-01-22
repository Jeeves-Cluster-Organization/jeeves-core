// Package runtime tests for Runtime
package runtime

import (
	"context"
	"testing"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/agents"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

// MockLogger implements agents.Logger for testing.
type MockLogger struct{}

func (m *MockLogger) Debug(msg string, keysAndValues ...any) {}
func (m *MockLogger) Info(msg string, keysAndValues ...any)  {}
func (m *MockLogger) Warn(msg string, keysAndValues ...any)  {}
func (m *MockLogger) Error(msg string, keysAndValues ...any) {}
func (m *MockLogger) Bind(fields ...any) agents.Logger {
	return m
}

// createTestRuntime creates a minimal runtime for testing.
func createTestRuntime(t *testing.T) *Runtime {
	cfg := &config.PipelineConfig{
		Name:          "test-pipeline",
		MaxIterations: 3,
		MaxLLMCalls:   10,
		MaxAgentHops:  21,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, &MockLogger{})
	require.NoError(t, err)
	return runtime
}

// =============================================================================
// RESUME TESTS - ALL INTERRUPT KINDS
// =============================================================================

func TestResumeNoInterruptPending(t *testing.T) {
	// Test Resume with no pending interrupt returns error.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	text := "response text"
	_, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Text: &text}, "thread-1")

	require.Error(t, err)
	assert.Contains(t, err.Error(), "no pending interrupt")
}

func TestResumeClarificationInterrupt(t *testing.T) {
	// Test Resume with clarification interrupt.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Analyze code", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageB"
	env.StageOrder = []string{"stageA", "stageB", "stageC", "end"}

	// Set clarification interrupt
	env.SetInterrupt(envelope.InterruptKindClarification, "clar-1",
		envelope.WithQuestion("Which file?"))

	// Resume with text response
	text := "main.go"
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Text: &text}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Clarification stays at current stage (no resume stage configured), pipeline ends with no agents
	assert.Equal(t, "main.go", *result.Interrupt.Response.Text)
}

func TestResumeConfirmationApproved(t *testing.T) {
	// Test Resume with confirmation interrupt approved.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Delete file", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageB"
	env.StageOrder = []string{"stageA", "stageB", "stageC", "end"}

	// Set confirmation interrupt
	env.SetInterrupt(envelope.InterruptKindConfirmation, "conf-1",
		envelope.WithMessage("Delete main.go?"))

	// Resume with approval
	approved := true
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Approved: &approved}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Approved confirmation stays at current stage (no resume stage configured), pipeline ends with no agents
	assert.True(t, *result.Interrupt.Response.Approved)
}

func TestResumeConfirmationDenied(t *testing.T) {
	// Test Resume with confirmation interrupt denied.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Delete file", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageB"
	env.StageOrder = []string{"stageA", "stageB", "stageC", "end"}

	// Set confirmation interrupt
	env.SetInterrupt(envelope.InterruptKindConfirmation, "conf-1",
		envelope.WithMessage("Delete main.go?"))

	// Resume with denial
	approved := false
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Approved: &approved}, "thread-1")

	require.NoError(t, err)
	assert.True(t, result.Terminated) // Denied confirmation terminates
	assert.Contains(t, *result.TerminationReason, "denied")
}

func TestResumeAgentReviewInterrupt(t *testing.T) {
	// Test Resume with agent review interrupt.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageC"
	env.StageOrder = []string{"stageA", "stageB", "stageC", "stageD", "end"}

	// Set agent review interrupt
	env.SetInterrupt(envelope.InterruptKindAgentReview, "review-1",
		envelope.WithMessage("Review plan"),
		envelope.WithInterruptData(map[string]any{"plan_id": "plan-1"}))

	// Resume with decision
	decision := "approve"
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Decision: &decision}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Agent review stays at current stage (no resume stage configured), pipeline ends with no agents
	assert.Equal(t, "approve", *result.Interrupt.Response.Decision)
}

func TestResumeCheckpointInterrupt(t *testing.T) {
	// Test Resume with checkpoint interrupt.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageB"
	env.StageOrder = []string{"stageA", "stageB", "stageC", "end"}

	// Set checkpoint interrupt
	env.SetInterrupt(envelope.InterruptKindCheckpoint, "checkpoint-1",
		envelope.WithInterruptData(map[string]any{"stage": "stageB"}))

	// Resume checkpoint (no specific response needed)
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Checkpoint stays at current stage, but since no agents configured, pipeline ends
	// Just verify interrupt was resolved
	assert.NotNil(t, result.Interrupt.Response)
}

func TestResumeResourceExhaustedInterrupt(t *testing.T) {
	// Test Resume with resource exhausted interrupt.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageC"
	env.StageOrder = []string{"stageA", "stageB", "stageC", "end"}

	// Set resource exhausted interrupt
	env.SetInterrupt(envelope.InterruptKindResourceExhausted, "rate-1",
		envelope.WithMessage("Rate limit exceeded"),
		envelope.WithInterruptData(map[string]any{"retry_after": 60}))

	// Resume after waiting
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Resource exhausted stays at current stage for retry, but no agents configured
	assert.NotNil(t, result.Interrupt.Response)
}

func TestResumeTimeoutInterrupt(t *testing.T) {
	// Test Resume with timeout interrupt.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageC"
	env.StageOrder = []string{"stageA", "stageB", "stageC", "end"}

	// Set timeout interrupt
	env.SetInterrupt(envelope.InterruptKindTimeout, "timeout-1",
		envelope.WithMessage("Operation timed out"))

	// Resume after timeout
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Timeout stays at current stage for retry, but no agents configured
	assert.NotNil(t, result.Interrupt.Response)
}

func TestResumeSystemErrorInterrupt(t *testing.T) {
	// Test Resume with system error interrupt.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageB"
	env.StageOrder = []string{"stageA", "stageB", "stageC", "end"}

	// Set system error interrupt
	env.SetInterrupt(envelope.InterruptKindSystemError, "error-1",
		envelope.WithMessage("Database connection lost"),
		envelope.WithInterruptData(map[string]any{"error_code": "DB001"}))

	// Resume after recovery
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// System error stays at current stage for retry, but no agents configured
	assert.NotNil(t, result.Interrupt.Response)
}

// =============================================================================
// INTERRUPT STATE TESTS
// =============================================================================

func TestInterruptBlocksExecution(t *testing.T) {
	// Test that pending interrupts block execution.
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Initially can continue
	assert.True(t, env.CanContinue())

	// Set interrupt
	env.SetInterrupt(envelope.InterruptKindClarification, "int-1",
		envelope.WithQuestion("Which file?"))

	// Now cannot continue
	assert.False(t, env.CanContinue())
	assert.True(t, env.InterruptPending)
}

func TestResolveInterruptEnablesExecution(t *testing.T) {
	// Test that resolving interrupt enables execution.
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Set and resolve interrupt
	env.SetInterrupt(envelope.InterruptKindClarification, "int-1",
		envelope.WithQuestion("Which file?"))
	assert.False(t, env.CanContinue())

	text := "main.go"
	env.ResolveInterrupt(envelope.InterruptResponse{Text: &text})

	// Now can continue again
	assert.True(t, env.CanContinue())
	assert.False(t, env.InterruptPending)
}

func TestClearInterruptWithoutResponse(t *testing.T) {
	// Test that interrupt can be cleared without response.
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	env.SetInterrupt(envelope.InterruptKindCheckpoint, "cp-1",
		envelope.WithInterruptData(map[string]any{"stage": "test"}))

	// Clear without response
	env.ClearInterrupt()

	assert.False(t, env.InterruptPending)
	assert.Nil(t, env.Interrupt)
	assert.True(t, env.CanContinue())
}

// =============================================================================
// EVENT CONTEXT TESTS
// =============================================================================

type MockEventContext struct {
	started   []string
	completed []string
}

func (m *MockEventContext) EmitAgentStarted(agentName string) error {
	m.started = append(m.started, agentName)
	return nil
}

func (m *MockEventContext) EmitAgentCompleted(agentName string, status string, durationMS int, err error) error {
	m.completed = append(m.completed, agentName)
	return nil
}

func TestSetEventContext(t *testing.T) {
	runtime := createTestRuntime(t)
	eventCtx := &MockEventContext{}

	runtime.SetEventContext(eventCtx)

	// Verify event context is set on runtime
	assert.Equal(t, eventCtx, runtime.eventCtx)
}

// =============================================================================
// SHOULD CONTINUE TESTS
// =============================================================================

func TestShouldContinueNormal(t *testing.T) {
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	canContinue, reason := runtime.shouldContinue(env)
	assert.True(t, canContinue)
	assert.Empty(t, reason)
}

func TestShouldContinueBoundsExceeded(t *testing.T) {
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Exceed max iterations
	env.MaxIterations = 3
	env.Iteration = 5

	canContinue, reason := runtime.shouldContinue(env)
	assert.False(t, canContinue)
	assert.Equal(t, "bounds_exceeded", reason)
}

func TestShouldContinueInterruptPending(t *testing.T) {
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Set all limits high
	env.MaxIterations = 100
	env.Iteration = 1
	env.MaxLLMCalls = 100
	env.LLMCallCount = 1
	env.MaxAgentHops = 100
	env.AgentHopCount = 1

	env.SetInterrupt(envelope.InterruptKindClarification, "int-1",
		envelope.WithQuestion("What?"))

	canContinue, reason := runtime.shouldContinue(env)
	assert.False(t, canContinue)
	assert.Equal(t, "interrupt_pending", reason)
}

// =============================================================================
// PERSIST STATE TESTS
// =============================================================================

type MockPersistence struct {
	savedStates map[string]map[string]any
	loadError   error
	saveError   error
}

func (m *MockPersistence) SaveState(ctx context.Context, threadID string, state map[string]any) error {
	if m.saveError != nil {
		return m.saveError
	}
	if m.savedStates == nil {
		m.savedStates = make(map[string]map[string]any)
	}
	m.savedStates[threadID] = state
	return nil
}

func (m *MockPersistence) LoadState(ctx context.Context, threadID string) (map[string]any, error) {
	if m.loadError != nil {
		return nil, m.loadError
	}
	return m.savedStates[threadID], nil
}

func TestPersistStateWithPersistence(t *testing.T) {
	runtime := createTestRuntime(t)
	persistence := &MockPersistence{}
	runtime.Persistence = persistence

	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	runtime.persistState(context.Background(), env, "thread-123")

	// Verify state was saved
	savedState := persistence.savedStates["thread-123"]
	assert.NotNil(t, savedState)
	assert.Equal(t, "user-1", savedState["user_id"])
}

func TestPersistStateWithoutPersistence(t *testing.T) {
	runtime := createTestRuntime(t)
	// No persistence adapter set

	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Should not panic
	runtime.persistState(context.Background(), env, "thread-123")
}

func TestPersistStateEmptyThreadID(t *testing.T) {
	runtime := createTestRuntime(t)
	persistence := &MockPersistence{}
	runtime.Persistence = persistence

	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Empty thread ID skips persistence
	runtime.persistState(context.Background(), env, "")

	assert.Empty(t, persistence.savedStates)
}

func TestPersistStateError(t *testing.T) {
	runtime := createTestRuntime(t)
	persistence := &MockPersistence{saveError: assert.AnError}
	runtime.Persistence = persistence

	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Should log error but not panic
	runtime.persistState(context.Background(), env, "thread-123")
}

// =============================================================================
// RUN TESTS
// =============================================================================

func TestRun(t *testing.T) {
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	result, err := runtime.Run(context.Background(), env, "thread-1")

	require.NoError(t, err)
	assert.NotNil(t, result)
	// With no agents, pipeline ends immediately
	assert.Equal(t, "end", result.CurrentStage)
}

func TestRunParallel(t *testing.T) {
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	result, err := runtime.RunParallel(context.Background(), env, "thread-1")

	require.NoError(t, err)
	assert.NotNil(t, result)
	// ParallelMode flag should be set
	assert.True(t, result.ParallelMode)
}

func TestRunWithStream(t *testing.T) {
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	outputChan, err := runtime.RunWithStream(context.Background(), env, "thread-1")

	require.NoError(t, err)
	require.NotNil(t, outputChan)

	// Collect outputs
	var outputs []StageOutput
	for output := range outputChan {
		outputs = append(outputs, output)
	}

	// Should have at least an end marker
	assert.NotEmpty(t, outputs)
	lastOutput := outputs[len(outputs)-1]
	assert.Equal(t, "__end__", lastOutput.Stage)
}

// =============================================================================
// GET STATE TESTS
// =============================================================================

func TestGetStateWithPersistence(t *testing.T) {
	runtime := createTestRuntime(t)
	persistence := &MockPersistence{
		savedStates: map[string]map[string]any{
			"thread-1": {"user_id": "test-user"},
		},
	}
	runtime.Persistence = persistence

	state, err := runtime.GetState(context.Background(), "thread-1")

	require.NoError(t, err)
	assert.Equal(t, "test-user", state["user_id"])
}

func TestGetStateWithoutPersistence(t *testing.T) {
	runtime := createTestRuntime(t)
	// No persistence adapter

	state, err := runtime.GetState(context.Background(), "thread-1")

	require.NoError(t, err)
	assert.Nil(t, state)
}

// =============================================================================
// EXECUTE TESTS
// =============================================================================

func TestExecuteWithDefaultMode(t *testing.T) {
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	result, _, err := runtime.Execute(context.Background(), env, RunOptions{})

	require.NoError(t, err)
	assert.NotNil(t, result)
}

func TestExecuteWithConfigDefaultMode(t *testing.T) {
	cfg := &config.PipelineConfig{
		Name:           "test",
		DefaultRunMode: config.RunModeParallel,
		Agents:         []*config.AgentConfig{},
	}
	runtime, err := NewRuntime(cfg, nil, nil, &MockLogger{})
	require.NoError(t, err)

	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	result, _, err := runtime.Execute(context.Background(), env, RunOptions{})

	require.NoError(t, err)
	assert.True(t, result.ParallelMode)
}

func TestExecuteWithStreaming(t *testing.T) {
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	result, outputChan, err := runtime.Execute(context.Background(), env, RunOptions{Stream: true})

	require.NoError(t, err)
	assert.NotNil(t, result)
	assert.NotNil(t, outputChan)

	// Drain channel
	for range outputChan {
	}
}

// =============================================================================
// RESUME WITH CONFIG RESUME STAGES
// =============================================================================

func TestResumeWithConfiguredClarificationStage(t *testing.T) {
	cfg := &config.PipelineConfig{
		Name:                     "test",
		ClarificationResumeStage: "stageA",
		Agents:                   []*config.AgentConfig{},
	}
	runtime, err := NewRuntime(cfg, nil, nil, &MockLogger{})
	require.NoError(t, err)

	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageC"
	env.SetInterrupt(envelope.InterruptKindClarification, "clar-1",
		envelope.WithQuestion("What?"))

	text := "answer"
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Text: &text}, "")

	require.NoError(t, err)
	// Should resume at configured stage
	// Note: since no agents match, it will end at "end"
	assert.NotNil(t, result)
}

func TestResumeWithConfiguredConfirmationStage(t *testing.T) {
	cfg := &config.PipelineConfig{
		Name:                    "test",
		ConfirmationResumeStage: "stageB",
		Agents:                  []*config.AgentConfig{},
	}
	runtime, err := NewRuntime(cfg, nil, nil, &MockLogger{})
	require.NoError(t, err)

	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageC"
	env.SetInterrupt(envelope.InterruptKindConfirmation, "conf-1",
		envelope.WithMessage("Confirm?"))

	approved := true
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Approved: &approved}, "")

	require.NoError(t, err)
	assert.NotNil(t, result)
}

func TestResumeWithConfiguredAgentReviewStage(t *testing.T) {
	cfg := &config.PipelineConfig{
		Name:                   "test",
		AgentReviewResumeStage: "stageD",
		Agents:                 []*config.AgentConfig{},
	}
	runtime, err := NewRuntime(cfg, nil, nil, &MockLogger{})
	require.NoError(t, err)

	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stageC"
	env.SetInterrupt(envelope.InterruptKindAgentReview, "review-1",
		envelope.WithMessage("Review"))

	decision := "approve"
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Decision: &decision}, "")

	require.NoError(t, err)
	assert.NotNil(t, result)
}

// =============================================================================
// BUILD AGENTS TESTS
// =============================================================================

func TestBuildAgentsWithLLM(t *testing.T) {
	mockLLM := &MockLLMProvider{}
	llmFactory := func(role string) agents.LLMProvider {
		return mockLLM
	}

	cfg := &config.PipelineConfig{
		Name: "test",
		Agents: []*config.AgentConfig{
			{Name: "llm-agent", HasLLM: true, ModelRole: "default"},
		},
	}

	runtime, err := NewRuntime(cfg, llmFactory, nil, &MockLogger{})

	require.NoError(t, err)
	assert.Len(t, runtime.agents, 1)
}

func TestBuildAgentsWithTools(t *testing.T) {
	toolExecutor := &MockToolExecutor{}

	cfg := &config.PipelineConfig{
		Name: "test",
		Agents: []*config.AgentConfig{
			{Name: "tool-agent", HasTools: true},
		},
	}

	runtime, err := NewRuntime(cfg, nil, toolExecutor, &MockLogger{})

	require.NoError(t, err)
	assert.Len(t, runtime.agents, 1)
}

func TestBuildAgentsError(t *testing.T) {
	// Try to create an agent with invalid config
	cfg := &config.PipelineConfig{
		Name: "test",
		Agents: []*config.AgentConfig{
			{Name: "invalid", HasLLM: true}, // Missing model_role
		},
	}

	_, err := NewRuntime(cfg, nil, nil, &MockLogger{})

	require.Error(t, err)
	// Error comes from agent validation during pipeline config validation
	assert.Contains(t, err.Error(), "has_llm=true but no model_role")
}

// =============================================================================
// MOCK IMPLEMENTATIONS
// =============================================================================

type MockLLMProvider struct{}

func (m *MockLLMProvider) Generate(ctx context.Context, model string, prompt string, options map[string]any) (string, error) {
	return "mock response", nil
}

type MockToolExecutor struct{}

func (m *MockToolExecutor) Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error) {
	return map[string]any{"result": "mock"}, nil
}