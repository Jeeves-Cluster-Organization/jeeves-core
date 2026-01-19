package envelope

import (
	"testing"

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
// DAG SERIALIZATION TESTS
// =============================================================================

func TestDAGStateRoundtrip(t *testing.T) {
	// Test that DAG state fields are preserved through serialization.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)

	// Set DAG mode and state
	original.DAGMode = true
	original.StartStage("stage1")
	original.StartStage("stage2")
	original.CompleteStage("stage1")
	original.FailStage("stage3", "connection error")

	// Serialize and deserialize
	state := original.ToStateDict()
	restored := FromStateDict(state)

	// Verify DAG fields preserved
	assert.Equal(t, true, restored.DAGMode)
	assert.True(t, restored.ActiveStages["stage2"])
	assert.True(t, restored.CompletedStageSet["stage1"])
	assert.Equal(t, "connection error", restored.FailedStages["stage3"])
}

func TestToStateDictIncludesDAGFields(t *testing.T) {
	// Test that ToStateDict includes all DAG fields.
	envelope := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	envelope.DAGMode = true
	envelope.ActiveStages["active1"] = true
	envelope.CompletedStageSet["done1"] = true
	envelope.FailedStages["fail1"] = "error"

	state := envelope.ToStateDict()

	assert.Equal(t, true, state["dag_mode"])
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

func TestCloneWithDAGState(t *testing.T) {
	// Test cloning envelope with DAG state.
	original := CreateGenericEnvelope("Test", "user-1", "sess-1", nil, nil, nil)
	original.DAGMode = true
	original.ActiveStages["stage1"] = true
	original.CompletedStageSet["stage0"] = true
	original.FailedStages["stage2"] = "error"

	clone := original.Clone()

	// Verify DAG state copied
	assert.True(t, clone.DAGMode)
	assert.True(t, clone.ActiveStages["stage1"])
	assert.True(t, clone.CompletedStageSet["stage0"])
	assert.Equal(t, "error", clone.FailedStages["stage2"])

	// Modify clone's DAG state
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