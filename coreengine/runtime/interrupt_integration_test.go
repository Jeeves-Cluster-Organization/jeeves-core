// Package runtime provides interrupt handling integration tests.
//
// These tests validate the full interrupt/resume cycle including:
// - Clarification requests
// - Confirmation dialogs
// - Agent review checkpoints
// - Checkpoint persistence and recovery
package runtime

import (
	"context"
	"testing"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// CLARIFICATION INTERRUPT TESTS
// =============================================================================

func TestInterrupt_ClarificationFlow(t *testing.T) {
	// Test full clarification flow: execute → interrupt → resume → complete.
	cfg := &config.PipelineConfig{
		Name:                     "clarification-flow",
		MaxIterations:            5,
		MaxLLMCalls:              20,
		MaxAgentHops:             30,
		ClarificationResumeStage: "stageA",
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	// Create envelope and set clarification interrupt
	env := testutil.NewTestEnvelopeWithStages("Clarify me", []string{"stageA", "stageB", "end"})
	env.CurrentStage = "stageA"
	env.SetInterrupt(envelope.InterruptKindClarification, "clar-1",
		envelope.WithQuestion("Which file should I analyze?"))

	// Verify interrupt blocks execution
	assert.False(t, env.CanContinue())
	assert.True(t, env.InterruptPending)

	// Resume with clarification response
	text := "main.go"
	response := envelope.InterruptResponse{Text: &text}

	result, err := runtime.Resume(context.Background(), env, response, "thread-clar")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	assert.Equal(t, "main.go", *result.Interrupt.Response.Text)
}

func TestInterrupt_ClarificationWithResumeStage(t *testing.T) {
	// Test that clarification resumes at configured stage.
	cfg := &config.PipelineConfig{
		Name:                     "clarification-resume-stage",
		MaxIterations:            5,
		MaxLLMCalls:              20,
		MaxAgentHops:             30,
		ClarificationResumeStage: "stageB", // Resume at stageB, not current
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "stageC"},
			{Name: "stageC", StageOrder: 3, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Resume stage test", []string{"stageA", "stageB", "stageC", "end"})
	env.CurrentStage = "stageC" // We're at stageC but should resume at stageB
	env.SetInterrupt(envelope.InterruptKindClarification, "clar-2",
		envelope.WithQuestion("What now?"))

	text := "continue"
	response := envelope.InterruptResponse{Text: &text}

	result, err := runtime.Resume(context.Background(), env, response, "thread-resume")

	require.NoError(t, err)
	// After resume, pipeline should run from resume stage to end
	assert.Equal(t, "end", result.CurrentStage)
}

func TestInterrupt_MultipleSequentialClarifications(t *testing.T) {
	// Test handling multiple clarifications in sequence.
	cfg := &config.PipelineConfig{
		Name:          "multi-clarification",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Multi clarify", []string{"end"})

	// First clarification
	env.SetInterrupt(envelope.InterruptKindClarification, "clar-1",
		envelope.WithQuestion("Question 1?"))

	text1 := "Answer 1"
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Text: &text1}, "thread-multi")
	require.NoError(t, err)
	assert.False(t, result.InterruptPending)

	// Second clarification
	result.SetInterrupt(envelope.InterruptKindClarification, "clar-2",
		envelope.WithQuestion("Question 2?"))

	text2 := "Answer 2"
	result, err = runtime.Resume(context.Background(), result, envelope.InterruptResponse{Text: &text2}, "thread-multi")
	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
}

// =============================================================================
// CONFIRMATION INTERRUPT TESTS
// =============================================================================

func TestInterrupt_ConfirmationApproved(t *testing.T) {
	// Test confirmation flow when user approves.
	cfg := &config.PipelineConfig{
		Name:                    "confirmation-approved",
		MaxIterations:           5,
		MaxLLMCalls:             20,
		MaxAgentHops:            30,
		ConfirmationResumeStage: "stageB",
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Confirm test", []string{"stageA", "stageB", "end"})
	env.CurrentStage = "stageA"
	env.SetInterrupt(envelope.InterruptKindConfirmation, "conf-1",
		envelope.WithMessage("Delete file main.go?"))

	// Approve the action
	approved := true
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Approved: &approved}, "thread-approve")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	assert.True(t, *result.Interrupt.Response.Approved)
	assert.False(t, result.Terminated) // Approved, so not terminated
}

func TestInterrupt_ConfirmationDenied(t *testing.T) {
	// Test confirmation flow when user denies.
	cfg := &config.PipelineConfig{
		Name:          "confirmation-denied",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Deny test", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindConfirmation, "conf-deny",
		envelope.WithMessage("Proceed with destructive action?"))

	// Deny the action
	approved := false
	result, err := runtime.Resume(context.Background(), env, envelope.InterruptResponse{Approved: &approved}, "thread-deny")

	require.NoError(t, err)
	assert.True(t, result.Terminated) // Denied confirmation terminates
	assert.Contains(t, *result.TerminationReason, "denied")
}

func TestInterrupt_ConfirmationWithData(t *testing.T) {
	// Test confirmation with additional data in response.
	cfg := &config.PipelineConfig{
		Name:          "confirmation-with-data",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Data confirm", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindConfirmation, "conf-data",
		envelope.WithMessage("Apply changes?"),
		envelope.WithInterruptData(map[string]any{"changes": []string{"a.go", "b.go"}}))

	approved := true
	responseData := map[string]any{"selected": []string{"a.go"}}
	result, err := runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{Approved: &approved, Data: responseData}, "thread-data")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	assert.NotNil(t, result.Interrupt.Response.Data)
}

// =============================================================================
// AGENT REVIEW INTERRUPT TESTS
// =============================================================================

func TestInterrupt_AgentReviewApprove(t *testing.T) {
	// Test agent review interrupt with approve decision.
	cfg := &config.PipelineConfig{
		Name:                   "agent-review-approve",
		MaxIterations:          5,
		MaxLLMCalls:            20,
		MaxAgentHops:           30,
		AgentReviewResumeStage: "stageB",
		Agents: []*config.AgentConfig{
			{Name: "stageA", StageOrder: 1, HasLLM: true, ModelRole: "default", DefaultNext: "stageB"},
			{Name: "stageB", StageOrder: 2, HasLLM: true, ModelRole: "default", DefaultNext: "end"},
		},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) LLMProvider { return mockLLM }

	runtime, err := NewRuntime(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Agent review", []string{"stageA", "stageB", "end"})
	env.CurrentStage = "stageA"
	env.SetInterrupt(envelope.InterruptKindAgentReview, "review-1",
		envelope.WithMessage("Review the proposed plan"),
		envelope.WithInterruptData(map[string]any{
			"plan": "Step 1: Read files\nStep 2: Analyze\nStep 3: Report",
		}))

	decision := "approve"
	result, err := runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{Decision: &decision}, "thread-review")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	assert.Equal(t, "approve", *result.Interrupt.Response.Decision)
}

func TestInterrupt_AgentReviewReject(t *testing.T) {
	// Test agent review interrupt with reject decision.
	cfg := &config.PipelineConfig{
		Name:          "agent-review-reject",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Agent reject", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindAgentReview, "review-reject",
		envelope.WithMessage("Review this output"))

	decision := "reject"
	result, err := runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{Decision: &decision}, "thread-reject")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	assert.Equal(t, "reject", *result.Interrupt.Response.Decision)
}

func TestInterrupt_AgentReviewModify(t *testing.T) {
	// Test agent review with modify decision and corrections.
	cfg := &config.PipelineConfig{
		Name:          "agent-review-modify",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Agent modify", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindAgentReview, "review-modify",
		envelope.WithMessage("Review analysis results"))

	decision := "modify"
	text := "Please also consider error handling"
	result, err := runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{Decision: &decision, Text: &text}, "thread-modify")

	require.NoError(t, err)
	assert.Equal(t, "modify", *result.Interrupt.Response.Decision)
	assert.Equal(t, "Please also consider error handling", *result.Interrupt.Response.Text)
}

// =============================================================================
// CHECKPOINT INTERRUPT TESTS
// =============================================================================

func TestInterrupt_CheckpointCreation(t *testing.T) {
	// Test checkpoint interrupt creation and resolution.
	cfg := &config.PipelineConfig{
		Name:          "checkpoint-create",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	mockPersistence := testutil.NewMockPersistence()
	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)
	runtime.Persistence = mockPersistence

	env := testutil.NewTestEnvelopeWithStages("Checkpoint test", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindCheckpoint, "cp-1",
		envelope.WithInterruptData(map[string]any{
			"stage":    "stageB",
			"progress": 0.5,
		}))

	// Resume from checkpoint
	result, err := runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{}, "thread-checkpoint")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
	assert.NotNil(t, result.Interrupt.Response)
}

func TestInterrupt_CheckpointWithPersistence(t *testing.T) {
	// Test that checkpoint state is persisted.
	cfg := &config.PipelineConfig{
		Name:          "checkpoint-persist",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	mockPersistence := testutil.NewMockPersistence()
	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)
	runtime.Persistence = mockPersistence

	env := testutil.NewTestEnvelopeWithStages("Persist checkpoint", []string{"end"})
	env.CurrentStage = "stageB"
	env.Outputs["stageA"] = map[string]any{"result": "completed"}
	env.SetInterrupt(envelope.InterruptKindCheckpoint, "cp-persist",
		envelope.WithInterruptData(map[string]any{"reason": "long_operation"}))

	_, err = runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{}, "thread-persist-cp")

	require.NoError(t, err)

	// Verify state was persisted
	savedState := mockPersistence.GetState("thread-persist-cp")
	assert.NotNil(t, savedState)
}

// =============================================================================
// RESOURCE EXHAUSTED INTERRUPT TESTS
// =============================================================================

func TestInterrupt_ResourceExhausted(t *testing.T) {
	// Test resource exhausted interrupt (e.g., rate limiting).
	cfg := &config.PipelineConfig{
		Name:          "resource-exhausted",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Rate limited", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindResourceExhausted, "rate-1",
		envelope.WithMessage("Rate limit exceeded"),
		envelope.WithInterruptData(map[string]any{
			"retry_after_seconds": 60,
			"resource":            "llm_api",
		}))

	// Resume after waiting
	result, err := runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{}, "thread-rate")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
}

// =============================================================================
// TIMEOUT INTERRUPT TESTS
// =============================================================================

func TestInterrupt_Timeout(t *testing.T) {
	// Test timeout interrupt.
	cfg := &config.PipelineConfig{
		Name:          "timeout-test",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("Timed out", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindTimeout, "timeout-1",
		envelope.WithMessage("Operation timed out after 30s"))

	// Resume after timeout
	result, err := runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{}, "thread-timeout")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
}

// =============================================================================
// SYSTEM ERROR INTERRUPT TESTS
// =============================================================================

func TestInterrupt_SystemError(t *testing.T) {
	// Test system error interrupt.
	cfg := &config.PipelineConfig{
		Name:          "system-error",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("System error", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindSystemError, "error-1",
		envelope.WithMessage("Database connection lost"),
		envelope.WithInterruptData(map[string]any{
			"error_code": "DB_CONNECTION_FAILED",
			"retryable":  true,
		}))

	// Resume after recovery
	result, err := runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{}, "thread-error")

	require.NoError(t, err)
	assert.False(t, result.InterruptPending)
}

// =============================================================================
// ERROR HANDLING TESTS
// =============================================================================

func TestInterrupt_ResumeWithoutPendingInterrupt(t *testing.T) {
	// Test that resume fails when no interrupt is pending.
	cfg := &config.PipelineConfig{
		Name:   "no-interrupt",
		Agents: []*config.AgentConfig{},
	}

	runtime, err := NewRuntime(cfg, nil, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	env := testutil.NewTestEnvelopeWithStages("No interrupt", []string{"end"})
	// No interrupt set

	text := "Some response"
	_, err = runtime.Resume(context.Background(), env,
		envelope.InterruptResponse{Text: &text}, "thread-no-int")

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "no pending interrupt")
}

func TestInterrupt_ClearWithoutResponse(t *testing.T) {
	// Test clearing interrupt without providing a response.
	env := testutil.NewTestEnvelopeWithStages("Clear test", []string{"end"})
	env.SetInterrupt(envelope.InterruptKindCheckpoint, "cp-clear",
		envelope.WithInterruptData(map[string]any{"stage": "test"}))

	assert.True(t, env.InterruptPending)

	// Clear without response
	env.ClearInterrupt()

	assert.False(t, env.InterruptPending)
	assert.Nil(t, env.Interrupt)
	assert.True(t, env.CanContinue())
}

// =============================================================================
// INTERRUPT STATE PRESERVATION TESTS
// =============================================================================

func TestInterrupt_StatePreservedDuringInterrupt(t *testing.T) {
	// Test that envelope state is preserved when interrupt is set.
	env := testutil.NewTestEnvelopeWithStages("State test", []string{"stageA", "stageB", "end"})
	env.CurrentStage = "stageB"
	env.Iteration = 2
	env.LLMCallCount = 5
	env.AgentHopCount = 3
	env.Outputs["stageA"] = map[string]any{"result": "data"}

	// Set interrupt
	env.SetInterrupt(envelope.InterruptKindClarification, "clar-state",
		envelope.WithQuestion("Need more info"))

	// Verify state is preserved
	assert.Equal(t, "stageB", env.CurrentStage)
	assert.Equal(t, 2, env.Iteration)
	assert.Equal(t, 5, env.LLMCallCount)
	assert.Equal(t, 3, env.AgentHopCount)
	assert.NotNil(t, env.Outputs["stageA"])

	// Resolve interrupt
	text := "Here's more info"
	env.ResolveInterrupt(envelope.InterruptResponse{Text: &text})

	// Verify state still preserved
	assert.Equal(t, "stageB", env.CurrentStage)
	assert.Equal(t, 2, env.Iteration)
	assert.Equal(t, 5, env.LLMCallCount)
}

func TestInterrupt_InterruptDataPreserved(t *testing.T) {
	// Test that interrupt data survives serialization.
	env := testutil.NewTestEnvelope("Data preservation")

	originalData := map[string]any{
		"files":    []string{"a.go", "b.go"},
		"priority": "high",
		"count":    42,
	}

	env.SetInterrupt(envelope.InterruptKindAgentReview, "review-data",
		envelope.WithMessage("Review these files"),
		envelope.WithInterruptData(originalData))

	// Verify data is accessible
	assert.NotNil(t, env.Interrupt)
	assert.NotNil(t, env.Interrupt.Data)
	assert.Equal(t, "high", env.Interrupt.Data["priority"])
	assert.Equal(t, float64(42), env.Interrupt.Data["count"]) // JSON converts int to float64
}
