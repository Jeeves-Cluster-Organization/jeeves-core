package envelope

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// CREATION TESTS
// =============================================================================

func TestCreateEnvelopeBasic(t *testing.T) {
	// Create envelope with minimal parameters.
	envelope := CreateGenericEnvelope(
		"Hello",
		"user-123",
		"session-456",
		nil, nil, nil,
	)

	assert.Equal(t, "Hello", envelope.RawInput)
	assert.Equal(t, "user-123", envelope.UserID)
	assert.Equal(t, "session-456", envelope.SessionID)
	assert.Equal(t, "start", envelope.CurrentStage)
	assert.NotEmpty(t, envelope.EnvelopeID)
	assert.NotEmpty(t, envelope.RequestID)
}

func TestCreateEnvelopeWithRequestID(t *testing.T) {
	// Create envelope with custom request_id.
	requestID := "custom-req-id"
	envelope := CreateGenericEnvelope(
		"Test",
		"user-1",
		"sess-1",
		&requestID, nil, nil,
	)

	assert.Equal(t, "custom-req-id", envelope.RequestID)
}

func TestEnvelopeInitialState(t *testing.T) {
	// Verify envelope starts in correct initial state.
	envelope := CreateGenericEnvelope(
		"Test",
		"user-1",
		"sess-1",
		nil, nil, nil,
	)

	// Outputs dict should be empty initially
	assert.Empty(t, envelope.Outputs)

	// Control flow flags
	assert.False(t, envelope.InterruptPending)
	assert.Nil(t, envelope.Interrupt)
	assert.False(t, envelope.Terminated)

	// Counters
	assert.Equal(t, 0, envelope.Iteration)
	assert.Equal(t, 0, envelope.LLMCallCount)
	assert.Equal(t, 0, envelope.AgentHopCount)
}

func TestCreateEnvelopeWithStageOrder(t *testing.T) {
	// Create envelope with pipeline stage order.
	stages := []string{"perception", "intent", "planner", "executor"}
	envelope := CreateGenericEnvelope(
		"Test",
		"user-1",
		"sess-1",
		nil, nil, stages,
	)

	assert.Equal(t, stages, envelope.StageOrder)
}

// =============================================================================
// OUTPUT MANAGEMENT TESTS
// =============================================================================

func TestSetOutput(t *testing.T) {
	// Test setting agent output.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetOutput("perception", map[string]any{
		"normalized_input": "test",
		"context":          map[string]any{"key": "value"},
	})

	assert.Contains(t, envelope.Outputs, "perception")
	assert.Equal(t, "test", envelope.Outputs["perception"]["normalized_input"])
}

func TestGetOutput(t *testing.T) {
	// Test getting agent output.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.Outputs["intent"] = map[string]any{"intent": "analyze", "goals": []string{}}

	output := envelope.GetOutput("intent")
	require.NotNil(t, output)
	assert.Equal(t, "analyze", output["intent"])
}

func TestGetOutputMissing(t *testing.T) {
	// Test getting nonexistent output returns nil.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	assert.Nil(t, envelope.GetOutput("nonexistent"))
}

func TestHasOutput(t *testing.T) {
	// Test checking if output exists.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	assert.False(t, envelope.HasOutput("perception"))

	envelope.SetOutput("perception", map[string]any{"data": "value"})

	assert.True(t, envelope.HasOutput("perception"))
}

// =============================================================================
// STAGE TRANSITIONS TESTS
// =============================================================================

func TestAdvanceStage(t *testing.T) {
	// Test advancing through stages.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	assert.Equal(t, "start", envelope.CurrentStage)

	envelope.CurrentStage = "intent"
	assert.Equal(t, "intent", envelope.CurrentStage)

	envelope.CurrentStage = "planner"
	assert.Equal(t, "planner", envelope.CurrentStage)
}

func TestTerminateEnvelope(t *testing.T) {
	// Test terminating an envelope.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.Terminate("Test termination reason", nil)

	assert.True(t, envelope.Terminated)
	assert.Equal(t, "Test termination reason", *envelope.TerminationReason)
}

func TestTerminateWithTerminalReason(t *testing.T) {
	// Test terminating with explicit terminal reason.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	reason := TerminalReasonMaxLLMCallsExceeded
	envelope.Terminate("Max LLM calls exceeded", &reason)

	assert.True(t, envelope.Terminated)
	assert.Equal(t, TerminalReasonMaxLLMCallsExceeded, *envelope.TerminalReason_)
}

// =============================================================================
// PROCESSING HISTORY TESTS
// =============================================================================

func TestRecordAgentStart(t *testing.T) {
	// Test recording agent start.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.RecordAgentStart("perception", 0)

	assert.Len(t, envelope.ProcessingHistory, 1)
	entry := envelope.ProcessingHistory[0]
	assert.Equal(t, "perception", entry.Agent)
	assert.Equal(t, 0, entry.StageOrder)
	assert.Equal(t, "running", entry.Status)
	assert.Equal(t, 1, envelope.AgentHopCount)
}

func TestRecordAgentComplete(t *testing.T) {
	// Test recording agent completion.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.RecordAgentStart("intent", 1)
	envelope.RecordAgentComplete("intent", "success", nil, 1, 0)

	entry := envelope.ProcessingHistory[0]
	assert.Equal(t, "success", entry.Status)
	assert.Equal(t, 1, entry.LLMCalls)
	assert.Equal(t, 1, envelope.LLMCallCount)
}

// =============================================================================
// CONTROL FLOW TESTS
// =============================================================================

func TestCanContinueInitially(t *testing.T) {
	// Test envelope can continue initially.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	assert.True(t, envelope.CanContinue())
}

func TestCanContinueTerminated(t *testing.T) {
	// Test envelope cannot continue when terminated.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.Terminated = true
	assert.False(t, envelope.CanContinue())
}

func TestCanContinueMaxIterations(t *testing.T) {
	// Test envelope cannot continue at max iterations.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.Iteration = envelope.MaxIterations + 1
	assert.False(t, envelope.CanContinue())
	assert.Equal(t, TerminalReasonMaxIterationsExceeded, *envelope.TerminalReason_)
}

func TestCanContinueMaxLLMCalls(t *testing.T) {
	// Test envelope cannot continue at max LLM calls.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.LLMCallCount = envelope.MaxLLMCalls
	assert.False(t, envelope.CanContinue())
	assert.Equal(t, TerminalReasonMaxLLMCallsExceeded, *envelope.TerminalReason_)
}

func TestCanContinueMaxAgentHops(t *testing.T) {
	// Test envelope cannot continue at max agent hops.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.AgentHopCount = envelope.MaxAgentHops
	assert.False(t, envelope.CanContinue())
	assert.Equal(t, TerminalReasonMaxAgentHopsExceeded, *envelope.TerminalReason_)
}

func TestCanContinueInterruptPending(t *testing.T) {
	// Test envelope cannot continue when interrupt pending.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetInterrupt(InterruptKindClarification, "int-1", WithQuestion("What do you mean?"))
	assert.False(t, envelope.CanContinue())
	assert.True(t, envelope.InterruptPending)
	assert.NotNil(t, envelope.Interrupt)
	assert.Equal(t, InterruptKindClarification, envelope.Interrupt.Kind)
}

// =============================================================================
// MULTI-STAGE TESTS
// =============================================================================

func TestInitializeGoals(t *testing.T) {
	// Test initializing goal tracking from intent.
	envelope := CreateGenericEnvelope("Trace auth flow", "user-1", "sess-1", nil, nil, nil)

	goals := []string{"Find auth endpoint", "Trace middleware", "Find DB layer"}
	envelope.InitializeGoals(goals)

	assert.Equal(t, goals, envelope.AllGoals)
	assert.Equal(t, goals, envelope.RemainingGoals)
	assert.Len(t, envelope.GoalCompletionStatus, 3)
}

func TestAdvanceStageSatisfiedGoals(t *testing.T) {
	// Test advancing to next stage after goal satisfaction.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.InitializeGoals([]string{"Goal 1", "Goal 2"})
	envelope.SetOutput("plan", map[string]any{"plan_id": "plan-1"})

	canContinue := envelope.AdvanceStage(
		[]string{"Goal 1"},
		map[string]any{"results_count": 2},
	)

	assert.True(t, canContinue)
	assert.Equal(t, 2, envelope.CurrentStageNumber)
	assert.Equal(t, "satisfied", envelope.GoalCompletionStatus["Goal 1"])
	assert.NotContains(t, envelope.RemainingGoals, "Goal 1")
}

func TestAdvanceStageAllComplete(t *testing.T) {
	// Test advance_stage returns false when all goals satisfied.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.InitializeGoals([]string{"Goal 1"})
	envelope.SetOutput("plan", map[string]any{"plan_id": "plan-1"})

	canContinue := envelope.AdvanceStage(
		[]string{"Goal 1"},
		map[string]any{},
	)

	assert.False(t, canContinue)
	assert.Empty(t, envelope.RemainingGoals)
}

func TestGetStageContext(t *testing.T) {
	// Test getting accumulated context from completed stages.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.InitializeGoals([]string{"Goal 1", "Goal 2"})
	envelope.SetOutput("plan", map[string]any{"plan_id": "plan-1"})
	envelope.AdvanceStage([]string{"Goal 1"}, map[string]any{})

	context := envelope.GetStageContext()

	assert.Equal(t, 2, context["current_stage"])
	assert.Contains(t, context["remaining_goals"], "Goal 2")
	assert.Contains(t, context["satisfied_goals"], "Goal 1")
}

// =============================================================================
// RETRY MANAGEMENT TESTS
// =============================================================================

func TestIncrementIteration(t *testing.T) {
	// Test incrementing iteration counter.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	assert.Equal(t, 0, envelope.Iteration)

	envelope.IncrementIteration(nil)
	assert.Equal(t, 1, envelope.Iteration)
}

func TestIncrementIterationWithFeedback(t *testing.T) {
	// Test incrementing with loop feedback.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	feedback := "Need more context"
	envelope.IncrementIteration(&feedback)

	assert.Equal(t, 1, envelope.Iteration)
	assert.Contains(t, envelope.LoopFeedback, "Need more context")
}

func TestIncrementIterationStoresPriorPlan(t *testing.T) {
	// Test that prior plan is stored on iteration.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetOutput("plan", map[string]any{"plan_id": "plan-1", "steps": []any{}})

	envelope.IncrementIteration(nil)

	assert.Len(t, envelope.PriorPlans, 1)
	assert.Equal(t, "plan-1", envelope.PriorPlans[0]["plan_id"])
}

// =============================================================================
// RESPONSE HELPERS TESTS
// =============================================================================

func TestGetFinalResponseFromIntegration(t *testing.T) {
	// Test getting final response when integration is complete.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetOutput("integration", map[string]any{
		"final_response": "Final response text",
		"memory_updated": true,
	})

	response := envelope.GetFinalResponse()
	require.NotNil(t, response)
	assert.Equal(t, "Final response text", *response)
}

func TestGetFinalResponseClarification(t *testing.T) {
	// Test getting final response when clarification is pending.
	envelope := CreateGenericEnvelope("Do something", "user-1", "sess-1", nil, nil, nil)

	envelope.SetInterrupt(InterruptKindClarification, "int-1", WithQuestion("What would you like to do?"))

	response := envelope.GetFinalResponse()
	require.NotNil(t, response)
	assert.Equal(t, "What would you like to do?", *response)
}

func TestSetAndResolveInterrupt(t *testing.T) {
	// Test setting and resolving an interrupt.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Set clarification interrupt
	envelope.SetInterrupt(InterruptKindClarification, "int-1", WithQuestion("What file?"))
	assert.True(t, envelope.InterruptPending)
	assert.Equal(t, "What file?", envelope.Interrupt.Question)

	// Resolve with response
	text := "The main.go file"
	envelope.ResolveInterrupt(InterruptResponse{Text: &text})
	assert.False(t, envelope.InterruptPending)
	assert.NotNil(t, envelope.Interrupt.Response)
	assert.Equal(t, "The main.go file", *envelope.Interrupt.Response.Text)
}

func TestSetConfirmationInterrupt(t *testing.T) {
	// Test setting a confirmation interrupt.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetInterrupt(InterruptKindConfirmation, "conf-1", WithMessage("Delete this file?"))
	assert.True(t, envelope.InterruptPending)
	assert.Equal(t, InterruptKindConfirmation, envelope.Interrupt.Kind)
	assert.Equal(t, "Delete this file?", envelope.Interrupt.Message)

	// Resolve with approval
	approved := true
	envelope.ResolveInterrupt(InterruptResponse{Approved: &approved})
	assert.False(t, envelope.InterruptPending)
	assert.True(t, *envelope.Interrupt.Response.Approved)
}

func TestToResultDict(t *testing.T) {
	// Test converting envelope to result dict.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	result := envelope.ToResultDict()

	assert.Contains(t, result, "envelope_id")
	assert.Contains(t, result, "request_id")
	assert.Equal(t, "user-1", result["user_id"])
	assert.Equal(t, false, result["terminated"])
}

// =============================================================================
// SERIALIZATION TESTS
// =============================================================================

func TestToStateDict(t *testing.T) {
	// Test serializing envelope to state dict.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetOutput("perception", map[string]any{"normalized_input": "test"})
	envelope.CurrentStage = "intent"
	envelope.LLMCallCount = 2

	state := envelope.ToStateDict()

	assert.Equal(t, "Test", state["raw_input"])
	assert.Equal(t, "user-1", state["user_id"])
	assert.Equal(t, "intent", state["current_stage"])
	assert.Equal(t, 2, state["llm_call_count"])
	outputs := state["outputs"].(map[string]map[string]any)
	assert.Contains(t, outputs, "perception")
}

func TestFromStateDict(t *testing.T) {
	// Test deserializing envelope from state dict.
	state := map[string]any{
		"raw_input":     "Test",
		"user_id":       "user-1",
		"session_id":    "sess-1",
		"current_stage": "planner",
		"outputs": map[string]any{
			"perception": map[string]any{"normalized_input": "test"},
			"intent":     map[string]any{"intent": "analyze"},
		},
		"llm_call_count":         3,
		"all_goals":              []any{"Goal 1"},
		"remaining_goals":        []any{},
		"goal_completion_status": map[string]any{"Goal 1": "satisfied"},
	}

	envelope := FromStateDict(state)

	assert.Equal(t, "Test", envelope.RawInput)
	assert.Equal(t, "planner", envelope.CurrentStage)
	assert.Equal(t, 3, envelope.LLMCallCount)
	assert.Contains(t, envelope.Outputs, "perception")
	assert.Equal(t, "satisfied", envelope.GoalCompletionStatus["Goal 1"])
}

func TestRoundtripSerialization(t *testing.T) {
	// Test that serialization roundtrips correctly.
	original := CreateGenericEnvelope("Test query", "user-123", "session-456", nil, nil, nil)

	original.SetOutput("intent", map[string]any{"intent": "analyze", "goals": []string{"Find bugs"}})
	original.InitializeGoals([]string{"Find bugs"})
	original.LLMCallCount = 5

	state := original.ToStateDict()
	restored := FromStateDict(state)

	assert.Equal(t, original.RawInput, restored.RawInput)
	assert.Equal(t, original.UserID, restored.UserID)
	assert.Equal(t, original.LLMCallCount, restored.LLMCallCount)
	assert.Equal(t, original.AllGoals, restored.AllGoals)
}

// =============================================================================
// INTERRUPT KIND TESTS (All 7 Values)
// =============================================================================

func TestInterruptKindValues(t *testing.T) {
	// Test all 7 InterruptKind enum values.
	assert.Equal(t, InterruptKind("clarification"), InterruptKindClarification)
	assert.Equal(t, InterruptKind("confirmation"), InterruptKindConfirmation)
	assert.Equal(t, InterruptKind("agent_review"), InterruptKindAgentReview)
	assert.Equal(t, InterruptKind("checkpoint"), InterruptKindCheckpoint)
	assert.Equal(t, InterruptKind("resource_exhausted"), InterruptKindResourceExhausted)
	assert.Equal(t, InterruptKind("timeout"), InterruptKindTimeout)
	assert.Equal(t, InterruptKind("system_error"), InterruptKindSystemError)
}

func TestSetAgentReviewInterrupt(t *testing.T) {
	// Test setting an agent review interrupt.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetInterrupt(InterruptKindAgentReview, "review-1",
		WithMessage("Review this plan"),
		WithInterruptData(map[string]any{"plan_id": "plan-123"}))

	assert.True(t, envelope.InterruptPending)
	assert.Equal(t, InterruptKindAgentReview, envelope.Interrupt.Kind)
	assert.Equal(t, "Review this plan", envelope.Interrupt.Message)
	assert.Equal(t, "plan-123", envelope.Interrupt.Data["plan_id"])

	// Resolve with decision
	decision := "approve"
	envelope.ResolveInterrupt(InterruptResponse{Decision: &decision})
	assert.False(t, envelope.InterruptPending)
	assert.Equal(t, "approve", *envelope.Interrupt.Response.Decision)
}

func TestSetCheckpointInterrupt(t *testing.T) {
	// Test setting a checkpoint interrupt.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetInterrupt(InterruptKindCheckpoint, "checkpoint-1",
		WithInterruptData(map[string]any{"stage": "planner", "iteration": 2}))

	assert.True(t, envelope.InterruptPending)
	assert.Equal(t, InterruptKindCheckpoint, envelope.Interrupt.Kind)
	assert.Equal(t, "planner", envelope.Interrupt.Data["stage"])

	// Checkpoints can be cleared without response
	envelope.ClearInterrupt()
	assert.False(t, envelope.InterruptPending)
	assert.Nil(t, envelope.Interrupt)
}

func TestSetResourceExhaustedInterrupt(t *testing.T) {
	// Test setting a resource exhausted interrupt.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetInterrupt(InterruptKindResourceExhausted, "rate-limit-1",
		WithMessage("Rate limit exceeded"),
		WithInterruptData(map[string]any{"retry_after_seconds": 60.0}))

	assert.True(t, envelope.InterruptPending)
	assert.Equal(t, InterruptKindResourceExhausted, envelope.Interrupt.Kind)
	assert.Equal(t, "Rate limit exceeded", envelope.Interrupt.Message)
	assert.Equal(t, 60.0, envelope.Interrupt.Data["retry_after_seconds"])
}

func TestSetTimeoutInterrupt(t *testing.T) {
	// Test setting a timeout interrupt.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetInterrupt(InterruptKindTimeout, "timeout-1",
		WithMessage("Operation timed out"),
		WithInterruptData(map[string]any{"timeout_ms": 30000}))

	assert.True(t, envelope.InterruptPending)
	assert.Equal(t, InterruptKindTimeout, envelope.Interrupt.Kind)
	assert.Equal(t, "Operation timed out", envelope.Interrupt.Message)
}

func TestSetSystemErrorInterrupt(t *testing.T) {
	// Test setting a system error interrupt.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	envelope.SetInterrupt(InterruptKindSystemError, "error-1",
		WithMessage("Internal system error"),
		WithInterruptData(map[string]any{"error_code": "E001", "stack_trace": "..."}))

	assert.True(t, envelope.InterruptPending)
	assert.Equal(t, InterruptKindSystemError, envelope.Interrupt.Kind)
	assert.Equal(t, "Internal system error", envelope.Interrupt.Message)
	assert.Equal(t, "E001", envelope.Interrupt.Data["error_code"])
}

// =============================================================================
// PARALLEL EXECUTION SERIALIZATION TESTS
// =============================================================================

func TestParallelStateRoundtrip(t *testing.T) {
	// Test that parallel execution state fields are preserved through serialization.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Set parallel mode and state
	original.ParallelMode = true
	original.StartStage("stage1")
	original.StartStage("stage2")
	original.CompleteStage("stage1")
	original.FailStage("stage3", "connection error")

	// Serialize and deserialize
	state := original.ToStateDict()
	restored := FromStateDict(state)

	// Verify parallel execution fields preserved
	assert.Equal(t, true, restored.ParallelMode)
	assert.True(t, restored.ActiveStages["stage2"])
	assert.True(t, restored.CompletedStageSet["stage1"])
	assert.Equal(t, "connection error", restored.FailedStages["stage3"])
}

func TestToStateDictIncludesParallelFields(t *testing.T) {
	// Test that ToStateDict includes all parallel execution fields.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	envelope.ParallelMode = true
	envelope.ActiveStages["active1"] = true
	envelope.CompletedStageSet["done1"] = true
	envelope.FailedStages["fail1"] = "error"

	state := envelope.ToStateDict()

	assert.Equal(t, true, state["parallel_mode"])
	assert.NotNil(t, state["active_stages"])
	assert.NotNil(t, state["completed_stage_set"])
	assert.NotNil(t, state["failed_stages"])
}

// =============================================================================
// ENUM TESTS
// =============================================================================

func TestLoopVerdictValues(t *testing.T) {
	// Test LoopVerdict enum values.
	assert.Equal(t, LoopVerdict("proceed"), LoopVerdictProceed)
	assert.Equal(t, LoopVerdict("loop_back"), LoopVerdictLoopBack)
	assert.Equal(t, LoopVerdict("advance"), LoopVerdictAdvance)
}

func TestTerminalReasonValues(t *testing.T) {
	// Test TerminalReason enum values.
	assert.Equal(t, TerminalReason("completed_successfully"), TerminalReasonCompletedSuccessfully)
	assert.Equal(t, TerminalReason("max_iterations_exceeded"), TerminalReasonMaxIterationsExceeded)
	assert.Equal(t, TerminalReason("max_llm_calls_exceeded"), TerminalReasonMaxLLMCallsExceeded)
}

// =============================================================================
// CLONE TESTS
// =============================================================================

func TestCloneBasic(t *testing.T) {
	// Test basic clone creates independent copy.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	original.Outputs["test"] = map[string]any{"key": "value"}
	original.Iteration = 5

	clone := original.Clone()

	// Verify values copied
	assert.Equal(t, original.EnvelopeID, clone.EnvelopeID)
	assert.Equal(t, original.Iteration, clone.Iteration)
	assert.Equal(t, "value", clone.Outputs["test"]["key"])

	// Verify independence - modify clone
	clone.Outputs["test"]["key"] = "modified"
	clone.Iteration = 10

	// Original unchanged
	assert.Equal(t, "value", original.Outputs["test"]["key"])
	assert.Equal(t, 5, original.Iteration)
}

func TestCloneDeepCopyOutputs(t *testing.T) {
	// Test that nested outputs are deeply copied.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	original.Outputs["nested"] = map[string]any{
		"level1": map[string]any{
			"level2": "deep",
		},
	}

	clone := original.Clone()

	// Modify clone's nested data
	if level1, ok := clone.Outputs["nested"]["level1"].(map[string]any); ok {
		level1["level2"] = "changed"
	}

	// Original should be unchanged
	if level1, ok := original.Outputs["nested"]["level1"].(map[string]any); ok {
		assert.Equal(t, "deep", level1["level2"])
	}
}

func TestCloneWithInterrupt(t *testing.T) {
	// Test cloning envelope with interrupt.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	original.SetInterrupt(InterruptKindClarification, "int-1", WithQuestion("What file?"))

	clone := original.Clone()

	// Verify interrupt copied
	require.NotNil(t, clone.Interrupt)
	assert.Equal(t, InterruptKindClarification, clone.Interrupt.Kind)
	assert.Equal(t, "What file?", clone.Interrupt.Question)

	// Modify clone's interrupt
	clone.Interrupt.Question = "Modified?"

	// Original unchanged
	assert.Equal(t, "What file?", original.Interrupt.Question)
}

func TestCloneWithParallelState(t *testing.T) {
	// Test cloning envelope with parallel execution state.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	original.ParallelMode = true
	original.ActiveStages["stage1"] = true
	original.CompletedStageSet["stage0"] = true
	original.FailedStages["stage2"] = "error"

	clone := original.Clone()

	// Verify parallel state copied
	assert.True(t, clone.ParallelMode)
	assert.True(t, clone.ActiveStages["stage1"])
	assert.True(t, clone.CompletedStageSet["stage0"])
	assert.Equal(t, "error", clone.FailedStages["stage2"])

	// Modify clone's parallel state
	clone.ActiveStages["stage1"] = false
	clone.CompletedStageSet["new"] = true

	// Original unchanged
	assert.True(t, original.ActiveStages["stage1"])
	assert.False(t, original.CompletedStageSet["new"])
}

func TestCloneWithProcessingHistory(t *testing.T) {
	// Test cloning envelope with processing history.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	original.RecordAgentStart("agent1", 1)
	original.RecordAgentComplete("agent1", "success", nil, 2, 100)

	clone := original.Clone()

	// Verify history copied
	require.Len(t, clone.ProcessingHistory, 1)
	assert.Equal(t, "agent1", clone.ProcessingHistory[0].Agent)
	assert.Equal(t, "success", clone.ProcessingHistory[0].Status)

	// Modify clone's history
	clone.ProcessingHistory[0].Status = "modified"

	// Original unchanged
	assert.Equal(t, "success", original.ProcessingHistory[0].Status)
}

func TestCloneNilFields(t *testing.T) {
	// Test cloning envelope with nil optional fields.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	// Leave optional fields nil

	clone := original.Clone()

	// Should not panic, nil fields should remain nil
	assert.Nil(t, clone.Interrupt)
	assert.Nil(t, clone.TerminalReason_)
	assert.Nil(t, clone.TerminationReason)
	assert.Nil(t, clone.CompletedAt)
}

// =============================================================================
// WithExpiry Option Tests
// =============================================================================

func TestWithExpiry(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Set interrupt with expiry
	env.SetInterrupt(InterruptKindConfirmation, "int-exp-1", WithExpiry(time.Hour))

	// ExpiresAt should be set to approximately 1 hour from now
	assert.NotNil(t, env.Interrupt)
	assert.NotNil(t, env.Interrupt.ExpiresAt)
	// Check it's roughly an hour from now (within 1 second tolerance)
	expectedExpiry := time.Now().UTC().Add(time.Hour)
	assert.WithinDuration(t, expectedExpiry, *env.Interrupt.ExpiresAt, time.Second)
}

// =============================================================================
// Stage Query Method Tests
// =============================================================================

func TestIsStageCompleted(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Initially no stages are completed
	assert.False(t, env.IsStageCompleted("stage1"))

	// Start and complete a stage
	env.StartStage("stage1")
	assert.False(t, env.IsStageCompleted("stage1"))

	env.CompleteStage("stage1")
	assert.True(t, env.IsStageCompleted("stage1"))

	// Non-existent stage
	assert.False(t, env.IsStageCompleted("nonexistent"))
}

func TestIsStageCompletedNilMap(t *testing.T) {
	env := &GenericEnvelope{
		CompletedStageSet: nil,
	}
	// Should not panic with nil map
	assert.False(t, env.IsStageCompleted("any"))
}

func TestIsStageActive(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Initially no stages are active
	assert.False(t, env.IsStageActive("stage1"))

	// Start a stage
	env.StartStage("stage1")
	assert.True(t, env.IsStageActive("stage1"))

	// Complete it
	env.CompleteStage("stage1")
	assert.False(t, env.IsStageActive("stage1"))
}

func TestIsStageActiveNilMap(t *testing.T) {
	env := &GenericEnvelope{
		ActiveStages: nil,
	}
	// Should not panic with nil map
	assert.False(t, env.IsStageActive("any"))
}

func TestIsStageFailed(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Initially no stages are failed
	assert.False(t, env.IsStageFailed("stage1"))

	// Start and fail a stage
	env.StartStage("stage1")
	assert.False(t, env.IsStageFailed("stage1"))

	env.FailStage("stage1", "some error")
	assert.True(t, env.IsStageFailed("stage1"))

	// Non-existent stage
	assert.False(t, env.IsStageFailed("nonexistent"))
}

func TestIsStageFailedNilMap(t *testing.T) {
	env := &GenericEnvelope{
		FailedStages: nil,
	}
	// Should not panic with nil map
	assert.False(t, env.IsStageFailed("any"))
}

func TestGetActiveStageCount(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Initially zero
	assert.Equal(t, 0, env.GetActiveStageCount())

	// Start a stage
	env.StartStage("stage1")
	assert.Equal(t, 1, env.GetActiveStageCount())

	// Start another
	env.StartStage("stage2")
	assert.Equal(t, 2, env.GetActiveStageCount())

	// Complete one
	env.CompleteStage("stage1")
	assert.Equal(t, 1, env.GetActiveStageCount())
}

func TestGetActiveStageCountNilMap(t *testing.T) {
	env := &GenericEnvelope{
		ActiveStages: nil,
	}
	assert.Equal(t, 0, env.GetActiveStageCount())
}

func TestGetCompletedStageCount(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Initially zero
	assert.Equal(t, 0, env.GetCompletedStageCount())

	// Start and complete stages
	env.StartStage("stage1")
	env.CompleteStage("stage1")
	assert.Equal(t, 1, env.GetCompletedStageCount())

	env.StartStage("stage2")
	env.CompleteStage("stage2")
	assert.Equal(t, 2, env.GetCompletedStageCount())
}

func TestGetCompletedStageCountNilMap(t *testing.T) {
	env := &GenericEnvelope{
		CompletedStageSet: nil,
	}
	assert.Equal(t, 0, env.GetCompletedStageCount())
}

func TestAllStagesComplete(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Empty StageOrder returns false
	assert.False(t, env.AllStagesComplete())

	// Set stage order
	env.StageOrder = []string{"stage1", "stage2", "stage3"}

	// Not all complete
	assert.False(t, env.AllStagesComplete())

	// Complete some
	env.StartStage("stage1")
	env.CompleteStage("stage1")
	assert.False(t, env.AllStagesComplete())

	env.StartStage("stage2")
	env.CompleteStage("stage2")
	assert.False(t, env.AllStagesComplete())

	// Complete all
	env.StartStage("stage3")
	env.CompleteStage("stage3")
	assert.True(t, env.AllStagesComplete())
}

func TestHasFailures(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Initially no failures
	assert.False(t, env.HasFailures())

	// Fail a stage
	env.StartStage("stage1")
	env.FailStage("stage1", "error happened")
	assert.True(t, env.HasFailures())
}

// =============================================================================
// Interrupt Query Method Tests
// =============================================================================

func TestHasPendingInterrupt(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Initially no interrupt
	assert.False(t, env.HasPendingInterrupt())

	// Set an interrupt
	env.SetInterrupt(InterruptKindClarification, "int-pending-1", WithQuestion("What now?"))
	assert.True(t, env.HasPendingInterrupt())

	// Resolve it
	text := "My response"
	env.ResolveInterrupt(InterruptResponse{Text: &text})
	assert.False(t, env.HasPendingInterrupt())
}

func TestGetInterruptKind(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// No interrupt - returns empty string
	assert.Equal(t, InterruptKind(""), env.GetInterruptKind())

	// Set clarification interrupt
	env.SetInterrupt(InterruptKindClarification, "int-kind-1", WithQuestion("Clarify?"))
	assert.Equal(t, InterruptKindClarification, env.GetInterruptKind())

	// Clear and set confirmation
	env.ClearInterrupt()
	env.SetInterrupt(InterruptKindConfirmation, "int-kind-2", WithMessage("Confirm this?"))
	assert.Equal(t, InterruptKindConfirmation, env.GetInterruptKind())
}

// =============================================================================
// FlowInterrupt Clone Tests (all branches)
// =============================================================================

func TestFlowInterruptCloneWithExpiry(t *testing.T) {
	expiry := time.Now().Add(time.Hour)
	interrupt := &FlowInterrupt{
		Kind:      InterruptKindConfirmation,
		ID:        "int-1",
		Message:   "Confirm?",
		CreatedAt: time.Now(),
		ExpiresAt: &expiry,
	}

	clone := interrupt.Clone()

	assert.NotSame(t, interrupt.ExpiresAt, clone.ExpiresAt)
	assert.Equal(t, *interrupt.ExpiresAt, *clone.ExpiresAt)
}

func TestFlowInterruptCloneWithData(t *testing.T) {
	interrupt := &FlowInterrupt{
		Kind:      InterruptKindAgentReview,
		ID:        "int-2",
		CreatedAt: time.Now(),
		Data:      map[string]any{"key": "value", "nested": map[string]any{"a": 1}},
	}

	clone := interrupt.Clone()

	// Data should be deep copied
	assert.Equal(t, interrupt.Data, clone.Data)
	// Modify original's data
	interrupt.Data["key"] = "modified"
	// Clone should be unchanged
	assert.Equal(t, "value", clone.Data["key"])
}

func TestFlowInterruptCloneWithResponse(t *testing.T) {
	text := "response text"
	approved := true
	decision := "option_a"
	interrupt := &FlowInterrupt{
		Kind:      InterruptKindClarification,
		ID:        "int-3",
		Question:  "What?",
		CreatedAt: time.Now(),
		Response: &InterruptResponse{
			Text:       &text,
			Approved:   &approved,
			Decision:   &decision,
			Data:       map[string]any{"resp_key": "resp_value"},
			ReceivedAt: time.Now(),
		},
	}

	clone := interrupt.Clone()

	// Response should be deep copied
	assert.NotSame(t, interrupt.Response, clone.Response)
	assert.Equal(t, *interrupt.Response.Text, *clone.Response.Text)
	assert.Equal(t, *interrupt.Response.Approved, *clone.Response.Approved)
	assert.Equal(t, *interrupt.Response.Decision, *clone.Response.Decision)
	assert.Equal(t, interrupt.Response.Data, clone.Response.Data)

	// Modify original's response
	newText := "changed"
	interrupt.Response.Text = &newText
	// Clone should be unchanged
	assert.Equal(t, "response text", *clone.Response.Text)
}

// =============================================================================
// ToResultDict Tests (all branches)
// =============================================================================

func TestToResultDictBasic(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.CurrentStage = "stage1"
	env.Iteration = 3

	result := env.ToResultDict()

	assert.Equal(t, env.EnvelopeID, result["envelope_id"])
	assert.Equal(t, "user-1", result["user_id"])
	assert.Equal(t, "sess-1", result["session_id"])
	assert.Equal(t, "stage1", result["current_stage"])
	assert.Equal(t, false, result["terminated"])
	assert.Equal(t, 3, result["iteration"])
}

func TestToResultDictWithClarificationInterrupt(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.SetInterrupt(InterruptKindClarification, "int-clarify-1", WithQuestion("Please clarify?"))

	result := env.ToResultDict()

	// Check interrupt is included
	interrupt := result["interrupt"].(map[string]any)
	assert.Equal(t, string(InterruptKindClarification), interrupt["kind"])
	assert.Equal(t, "Please clarify?", interrupt["question"])

	// Check backward-compat flags for clarification
	assert.True(t, result["clarification_needed"].(bool))
	assert.Equal(t, "Please clarify?", result["clarification_question"])
}

func TestToResultDictWithConfirmationInterrupt(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.SetInterrupt(InterruptKindConfirmation, "int-confirm-1", WithMessage("Please confirm"))

	result := env.ToResultDict()

	// Check interrupt is included
	interrupt := result["interrupt"].(map[string]any)
	assert.Equal(t, string(InterruptKindConfirmation), interrupt["kind"])
	assert.Equal(t, "Please confirm", interrupt["message"])

	// Check backward-compat flags for confirmation
	assert.True(t, result["confirmation_needed"].(bool))
	assert.Equal(t, "Please confirm", result["confirmation_message"])
	assert.NotEmpty(t, result["confirmation_id"])
}

// =============================================================================
// ToStateDict Tests (comprehensive branches)
// =============================================================================

func TestToStateDictWithTerminalReason(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	completed := TerminalReasonCompleted
	env.Terminate("All done", &completed)

	state := env.ToStateDict()

	// terminal_reason is stored as *string in ToStateDict
	terminalReason := state["terminal_reason"].(*string)
	assert.Equal(t, "completed", *terminalReason)
	assert.NotNil(t, state["completed_at"])
}

func TestToStateDictWithInterrupt(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	expiry := time.Now().Add(time.Hour)
	env.SetInterrupt(InterruptKindConfirmation, "int-state-1",
		WithMessage("Confirm?"),
		WithInterruptData(map[string]any{"key": "val"}),
	)
	env.Interrupt.ExpiresAt = &expiry

	state := env.ToStateDict()

	// Check interrupt dict is included
	interruptDict := state["interrupt"].(map[string]any)
	assert.Equal(t, string(InterruptKindConfirmation), interruptDict["kind"])
	assert.Equal(t, "Confirm?", interruptDict["message"])
	assert.NotNil(t, interruptDict["expires_at"])
	assert.Equal(t, map[string]any{"key": "val"}, interruptDict["data"])
}

func TestToStateDictWithInterruptResponse(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.SetInterrupt(InterruptKindClarification, "int-resp-1", WithQuestion("What?"))
	text := "Answer"
	env.ResolveInterrupt(InterruptResponse{Text: &text})

	state := env.ToStateDict()

	interruptDict := state["interrupt"].(map[string]any)
	responseDict := interruptDict["response"].(map[string]any)
	assert.Equal(t, "Answer", responseDict["text"])
}

func TestToStateDictBasicFields(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.Iteration = 5
	env.LLMCallCount = 10
	env.AgentHopCount = 3

	state := env.ToStateDict()

	// Verify basic fields are correctly serialized
	assert.Equal(t, "user-1", state["user_id"])
	assert.Equal(t, "sess-1", state["session_id"])
	assert.Equal(t, 5, state["iteration"])
	assert.Equal(t, 10, state["llm_call_count"])
	assert.Equal(t, 3, state["agent_hop_count"])
}

// =============================================================================
// deepCopyValue Tests (all branches)
// =============================================================================

func TestDeepCopyValueTypes(t *testing.T) {
	// Test via Clone which uses deepCopyValue internally
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Add outputs with various types to trigger all branches
	env.Outputs["test"] = map[string]any{
		"string":    "hello",
		"int":       42,
		"float":     3.14,
		"bool":      true,
		"nil":       nil,
		"map":       map[string]any{"nested": "value"},
		"slice":     []any{"a", "b", "c"},
		"map_slice": []map[string]any{{"k": "v"}},
	}

	clone := env.Clone()

	// Verify deep copy
	assert.Equal(t, env.Outputs["test"], clone.Outputs["test"])

	// Modify original's nested map
	env.Outputs["test"]["map"].(map[string]any)["nested"] = "changed"
	// Clone should be unchanged
	assert.Equal(t, "value", clone.Outputs["test"]["map"].(map[string]any)["nested"])

	// Modify original's slice
	env.Outputs["test"]["slice"].([]any)[0] = "z"
	// Clone should be unchanged
	assert.Equal(t, "a", clone.Outputs["test"]["slice"].([]any)[0])
}

// =============================================================================
// FromStateDict Tests (more branches)
// =============================================================================

func TestFromStateDictWithInterruptResponse(t *testing.T) {
	state := map[string]any{
		"envelope_id":      "env-1",
		"request_id":       "req-1",
		"user_id":          "user-1",
		"session_id":       "sess-1",
		"current_goal":     "goal",
		"interrupt_pending": true,
		"interrupt": map[string]any{
			"kind":       "clarification",
			"id":         "int-1",
			"question":   "What?",
			"created_at": time.Now().Format(time.RFC3339),
			"response": map[string]any{
				"text":        "Answer",
				"received_at": time.Now().Format(time.RFC3339),
			},
		},
	}

	env := FromStateDict(state)

	assert.True(t, env.InterruptPending)
	assert.NotNil(t, env.Interrupt)
	assert.Equal(t, InterruptKindClarification, env.Interrupt.Kind)
	assert.NotNil(t, env.Interrupt.Response)
	assert.Equal(t, "Answer", *env.Interrupt.Response.Text)
}

func TestFromStateDictWithInterruptExpiry(t *testing.T) {
	expiry := time.Now().Add(time.Hour)
	state := map[string]any{
		"envelope_id":      "env-1",
		"request_id":       "req-1",
		"user_id":          "user-1",
		"session_id":       "sess-1",
		"current_goal":     "goal",
		"interrupt_pending": true,
		"interrupt": map[string]any{
			"kind":       "confirmation",
			"id":         "int-1",
			"message":    "Confirm?",
			"created_at": time.Now().Format(time.RFC3339),
			"expires_at": expiry.Format(time.RFC3339),
		},
	}

	env := FromStateDict(state)

	assert.NotNil(t, env.Interrupt.ExpiresAt)
	assert.WithinDuration(t, expiry, *env.Interrupt.ExpiresAt, time.Second)
}

func TestFromStateDictWithResponseApprovedAndDecision(t *testing.T) {
	state := map[string]any{
		"envelope_id":      "env-1",
		"request_id":       "req-1",
		"user_id":          "user-1",
		"session_id":       "sess-1",
		"current_goal":     "goal",
		"interrupt_pending": true,
		"interrupt": map[string]any{
			"kind":       "agent_review",
			"id":         "int-1",
			"created_at": time.Now().Format(time.RFC3339),
			"response": map[string]any{
				"approved":    true,
				"decision":    "option_b",
				"data":        map[string]any{"extra": "info"},
				"received_at": time.Now().Format(time.RFC3339),
			},
		},
	}

	env := FromStateDict(state)

	assert.NotNil(t, env.Interrupt.Response)
	assert.True(t, *env.Interrupt.Response.Approved)
	assert.Equal(t, "option_b", *env.Interrupt.Response.Decision)
	assert.Equal(t, "info", env.Interrupt.Response.Data["extra"])
}

// =============================================================================
// TotalProcessingTimeMS edge cases
// =============================================================================

func TestTotalProcessingTimeMSEmpty(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	// No processing history
	assert.Equal(t, 0, env.TotalProcessingTimeMS())
}

// =============================================================================
// GetFinalResponse branches
// =============================================================================

func TestGetFinalResponseFromOutputs(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.Outputs["integration"] = map[string]any{
		"final_response": "Hello from output!",
	}

	resp := env.GetFinalResponse()
	require.NotNil(t, resp)
	assert.Equal(t, "Hello from output!", *resp)
}

func TestGetFinalResponseNoResponse(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	// No outputs at all
	assert.Nil(t, env.GetFinalResponse())
}

func TestGetFinalResponseFromInterruptClarification(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.SetInterrupt(InterruptKindClarification, "int-resp-2", WithQuestion("What do you mean?"))

	resp := env.GetFinalResponse()
	require.NotNil(t, resp)
	assert.Equal(t, "What do you mean?", *resp)
}

func TestGetFinalResponseFromInterruptConfirmation(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.SetInterrupt(InterruptKindConfirmation, "int-resp-3", WithMessage("Please confirm this action"))

	resp := env.GetFinalResponse()
	require.NotNil(t, resp)
	assert.Equal(t, "Please confirm this action", *resp)
}

// =============================================================================
// AdvanceStage error branch (already tested partially)
// =============================================================================

func TestAdvanceStageMaxStagesReached(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.InitializeGoals([]string{"Goal 1", "Goal 2", "Goal 3"})
	env.SetOutput("plan", map[string]any{"plan_id": "p1"})

	// Set current stage to max
	env.CurrentStageNumber = env.MaxStages

	// AdvanceStage should return false when at max stages
	canContinue := env.AdvanceStage([]string{"Goal 1"}, nil)
	assert.False(t, canContinue)
}

func TestAdvanceStagePartialGoals(t *testing.T) {
	env := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	env.InitializeGoals([]string{"Goal 1", "Goal 2", "Goal 3"})
	env.SetOutput("plan", map[string]any{"plan_id": "p1"})

	// Complete some goals, more remain
	canContinue := env.AdvanceStage([]string{"Goal 1", "Goal 2"}, map[string]any{"result": "ok"})
	assert.True(t, canContinue)
	assert.Equal(t, 2, env.CurrentStageNumber)
	assert.Equal(t, []string{"Goal 3"}, env.RemainingGoals)
	// Plan/execution/critic outputs should be cleared
	assert.Nil(t, env.Outputs["plan"])
}