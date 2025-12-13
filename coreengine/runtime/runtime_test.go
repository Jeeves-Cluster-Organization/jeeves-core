// Package runtime tests for UnifiedRuntime
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
func createTestRuntime(t *testing.T) *UnifiedRuntime {
	cfg := &config.PipelineConfig{
		Name:          "test-pipeline",
		MaxIterations: 3,
		MaxLLMCalls:   10,
		MaxAgentHops:  21,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewUnifiedRuntime(cfg, nil, nil, &MockLogger{})
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
	env.CurrentStage = "planner"
	env.StageOrder = []string{"intent", "planner", "executor", "end"}

	// Set clarification interrupt
	env.SetInterrupt(envelope.InterruptKindClarification, "clar-1",
		envelope.WithQuestion("Which file?"))

	// Resume with text response
	text := "main.go"
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Text: &text}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Clarification resets to intent, but since no agents configured, pipeline ends
	assert.Equal(t, "main.go", *result.Interrupt.Response.Text)
}

func TestResumeConfirmationApproved(t *testing.T) {
	// Test Resume with confirmation interrupt approved.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Delete file", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "planner"
	env.StageOrder = []string{"intent", "planner", "executor", "end"}

	// Set confirmation interrupt
	env.SetInterrupt(envelope.InterruptKindConfirmation, "conf-1",
		envelope.WithMessage("Delete main.go?"))

	// Resume with approval
	approved := true
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Approved: &approved}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Approved confirmation goes to executor, but since no agents configured, pipeline ends
	assert.True(t, *result.Interrupt.Response.Approved)
}

func TestResumeConfirmationDenied(t *testing.T) {
	// Test Resume with confirmation interrupt denied.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Delete file", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "planner"
	env.StageOrder = []string{"intent", "planner", "executor", "end"}

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

func TestResumeCriticReviewInterrupt(t *testing.T) {
	// Test Resume with critic review interrupt.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "critic"
	env.StageOrder = []string{"intent", "planner", "critic", "executor", "end"}

	// Set critic review interrupt
	env.SetInterrupt(envelope.InterruptKindCriticReview, "critic-1",
		envelope.WithMessage("Review plan"),
		envelope.WithInterruptData(map[string]any{"plan_id": "plan-1"}))

	// Resume with decision
	decision := "approve"
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Decision: &decision}, "thread-1")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	// Critic review resets to intent, but since no agents are configured, pipeline ends
	assert.Equal(t, "approve", *result.Interrupt.Response.Decision)
}

func TestResumeCheckpointInterrupt(t *testing.T) {
	// Test Resume with checkpoint interrupt.
	runtime := createTestRuntime(t)
	env := envelope.CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "planner"
	env.StageOrder = []string{"intent", "planner", "executor", "end"}

	// Set checkpoint interrupt
	env.SetInterrupt(envelope.InterruptKindCheckpoint, "checkpoint-1",
		envelope.WithInterruptData(map[string]any{"stage": "planner"}))

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
	env.CurrentStage = "executor"
	env.StageOrder = []string{"intent", "planner", "executor", "end"}

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
	env.CurrentStage = "executor"
	env.StageOrder = []string{"intent", "planner", "executor", "end"}

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
	env.CurrentStage = "planner"
	env.StageOrder = []string{"intent", "planner", "executor", "end"}

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
