package kernel

import (
	"fmt"
	"testing"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// Test Helpers
// =============================================================================

func createTestPipeline() *config.PipelineConfig {
	cfg := config.NewPipelineConfig("test_pipeline")
	cfg.MaxIterations = 3
	cfg.MaxLLMCalls = 10
	cfg.MaxAgentHops = 20

	// Add agents: understand -> think -> respond
	cfg.AddAgent(&config.AgentConfig{
		Name:        "understand",
		StageOrder:  1,
		HasLLM:      true,
		ModelRole:   "chat",
		OutputKey:   "understand",
		DefaultNext: "think",
	})
	cfg.AddAgent(&config.AgentConfig{
		Name:        "think",
		StageOrder:  2,
		HasLLM:      true,
		ModelRole:   "chat",
		OutputKey:   "think",
		DefaultNext: "respond",
	})
	cfg.AddAgent(&config.AgentConfig{
		Name:        "respond",
		StageOrder:  3,
		HasLLM:      true,
		ModelRole:   "chat",
		OutputKey:   "respond",
		DefaultNext: "end",
	})

	cfg.Validate()
	return cfg
}

func createTestPipelineWithRouting() *config.PipelineConfig {
	cfg := config.NewPipelineConfig("routing_pipeline")
	cfg.MaxIterations = 3
	cfg.MaxLLMCalls = 10
	cfg.MaxAgentHops = 20

	// Add agents with routing rules
	cfg.AddAgent(&config.AgentConfig{
		Name:       "planner",
		StageOrder: 1,
		HasLLM:     true,
		ModelRole:  "chat",
		OutputKey:  "planner",
		RoutingRules: []config.RoutingRule{
			{Condition: "needs_tools", Value: true, Target: "executor"},
		},
		DefaultNext: "responder",
	})
	cfg.AddAgent(&config.AgentConfig{
		Name:       "executor",
		StageOrder: 2,
		HasTools:   true,
		OutputKey:  "executor",
		RoutingRules: []config.RoutingRule{
			{Condition: "verdict", Value: "loop_back", Target: "planner"},
		},
		DefaultNext: "responder",
	})
	cfg.AddAgent(&config.AgentConfig{
		Name:        "responder",
		StageOrder:  3,
		HasLLM:      true,
		ModelRole:   "chat",
		OutputKey:   "responder",
		DefaultNext: "end",
	})

	// Add edge limit for executor->planner loop
	cfg.EdgeLimits = []config.EdgeLimit{
		{From: "executor", To: "planner", MaxCount: 2},
	}

	cfg.Validate()
	return cfg
}

func createTestEnvelope() *envelope.Envelope {
	env := envelope.NewEnvelope()
	env.RawInput = "test input"
	env.UserID = "user-1"
	env.SessionID = "sess-1"
	return env
}

// =============================================================================
// InitializeSession Tests
// =============================================================================

func TestOrchestrator_InitializeSession(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	state, err := orch.InitializeSession("proc-1", cfg, env, false)

	require.NoError(t, err)
	assert.Equal(t, "proc-1", state.ProcessID)
	assert.Equal(t, "understand", state.CurrentStage)
	assert.Equal(t, []string{"understand", "think", "respond"}, state.StageOrder)
	assert.False(t, state.Terminated)
	assert.Equal(t, 1, orch.GetSessionCount())
}

func TestOrchestrator_InitializeSession_InvalidConfig(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	env := createTestEnvelope()

	// Invalid config - missing name
	cfg := &config.PipelineConfig{}

	_, err := orch.InitializeSession("proc-1", cfg, env, false)

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "invalid pipeline config")
}

// =============================================================================
// GetNextInstruction Tests
// =============================================================================

func TestOrchestrator_GetNextInstruction_ReturnsRunAgent(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	instruction, err := orch.GetNextInstruction("proc-1")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
	assert.Equal(t, "understand", instruction.AgentName)
	assert.NotNil(t, instruction.AgentConfig)
	assert.NotNil(t, instruction.Envelope)
}

func TestOrchestrator_GetNextInstruction_TerminatesAtEnd(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()
	env.CurrentStage = "end"

	orch.InitializeSession("proc-1", cfg, env, false)

	instruction, err := orch.GetNextInstruction("proc-1")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
	assert.Equal(t, envelope.TerminalReasonCompleted, *instruction.TerminalReason)
}

func TestOrchestrator_GetNextInstruction_WaitsOnInterrupt(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()
	env.SetInterrupt(envelope.InterruptKindClarification, "int-1", envelope.WithQuestion("What do you mean?"))

	orch.InitializeSession("proc-1", cfg, env, false)

	instruction, err := orch.GetNextInstruction("proc-1")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindWaitInterrupt, instruction.Kind)
	assert.True(t, instruction.InterruptPending)
	assert.NotNil(t, instruction.Interrupt)
	assert.Equal(t, envelope.InterruptKindClarification, instruction.Interrupt.Kind)
}

func TestOrchestrator_GetNextInstruction_BoundsExceeded_Iterations(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()
	env.Iteration = 10 // Exceeds MaxIterations of 3
	env.MaxIterations = 3

	orch.InitializeSession("proc-1", cfg, env, false)

	instruction, err := orch.GetNextInstruction("proc-1")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
	assert.Equal(t, envelope.TerminalReasonMaxIterationsExceeded, *instruction.TerminalReason)
}

func TestOrchestrator_GetNextInstruction_BoundsExceeded_LLMCalls(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()
	env.LLMCallCount = 10 // At MaxLLMCalls
	env.MaxLLMCalls = 10

	orch.InitializeSession("proc-1", cfg, env, false)

	instruction, err := orch.GetNextInstruction("proc-1")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
	assert.Equal(t, envelope.TerminalReasonMaxLLMCallsExceeded, *instruction.TerminalReason)
}

func TestOrchestrator_GetNextInstruction_BoundsExceeded_AgentHops(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()
	env.AgentHopCount = 20 // At MaxAgentHops
	env.MaxAgentHops = 20

	orch.InitializeSession("proc-1", cfg, env, false)

	instruction, err := orch.GetNextInstruction("proc-1")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
	assert.Equal(t, envelope.TerminalReasonMaxAgentHopsExceeded, *instruction.TerminalReason)
}

func TestOrchestrator_GetNextInstruction_UnknownProcess(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	_, err := orch.GetNextInstruction("unknown-proc")

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "unknown process")
}

// =============================================================================
// ProcessAgentResult Tests
// =============================================================================

func TestOrchestrator_ProcessAgentResult_AdvancesToNextStage(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	// Complete "understand" agent
	output := map[string]any{"parsed": "test"}
	metrics := &AgentExecutionMetrics{LLMCalls: 1, DurationMS: 100}

	instruction, err := orch.ProcessAgentResult("proc-1", "understand", output, metrics, true, "")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
	assert.Equal(t, "think", instruction.AgentName)
	assert.Equal(t, "think", instruction.Envelope.CurrentStage)
}

func TestOrchestrator_ProcessAgentResult_RoutingRules(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipelineWithRouting()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	// Complete "planner" with needs_tools=true -> should route to executor
	output := map[string]any{"needs_tools": true}
	metrics := &AgentExecutionMetrics{LLMCalls: 1}

	instruction, err := orch.ProcessAgentResult("proc-1", "planner", output, metrics, true, "")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
	assert.Equal(t, "executor", instruction.AgentName)
}

func TestOrchestrator_ProcessAgentResult_RoutingRules_DefaultNext(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipelineWithRouting()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	// Complete "planner" with needs_tools=false -> should use default_next (responder)
	output := map[string]any{"needs_tools": false}
	metrics := &AgentExecutionMetrics{LLMCalls: 1}

	instruction, err := orch.ProcessAgentResult("proc-1", "planner", output, metrics, true, "")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
	assert.Equal(t, "responder", instruction.AgentName)
}

func TestOrchestrator_ProcessAgentResult_LoopBack(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipelineWithRouting()
	env := createTestEnvelope()
	env.CurrentStage = "executor"

	orch.InitializeSession("proc-1", cfg, env, false)

	// Complete "executor" with verdict=loop_back -> should route back to planner
	output := map[string]any{"verdict": "loop_back"}
	metrics := &AgentExecutionMetrics{ToolCalls: 1}

	instruction, err := orch.ProcessAgentResult("proc-1", "executor", output, metrics, true, "")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
	assert.Equal(t, "planner", instruction.AgentName)
	// Iteration should be incremented on loop-back
	assert.Equal(t, 1, instruction.Envelope.Iteration)
}

func TestOrchestrator_ProcessAgentResult_EdgeLimitExceeded(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipelineWithRouting()
	env := createTestEnvelope()
	env.CurrentStage = "executor"

	orch.InitializeSession("proc-1", cfg, env, false)

	// Simulate loop: executor -> planner -> executor -> planner -> executor -> planner (3 times)
	output := map[string]any{"verdict": "loop_back"}
	plannerOutput := map[string]any{"needs_tools": true}
	metrics := &AgentExecutionMetrics{ToolCalls: 1}

	// First loop: executor -> planner
	instruction, _ := orch.ProcessAgentResult("proc-1", "executor", output, metrics, true, "")
	assert.Equal(t, "planner", instruction.AgentName) // Should be at planner

	// planner -> executor
	instruction, _ = orch.ProcessAgentResult("proc-1", "planner", plannerOutput, metrics, true, "")
	assert.Equal(t, "executor", instruction.AgentName)

	// Second loop: executor -> planner
	instruction, _ = orch.ProcessAgentResult("proc-1", "executor", output, metrics, true, "")
	assert.Equal(t, "planner", instruction.AgentName)

	// planner -> executor
	instruction, _ = orch.ProcessAgentResult("proc-1", "planner", plannerOutput, metrics, true, "")
	assert.Equal(t, "executor", instruction.AgentName)

	// Third loop: executor -> planner - should exceed limit (edge limit is 2 for executor->planner)
	instruction, err := orch.ProcessAgentResult("proc-1", "executor", output, metrics, true, "")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
	assert.Equal(t, envelope.TerminalReasonMaxLoopExceeded, *instruction.TerminalReason)

	// Verify session state
	state, _ := orch.GetSessionState("proc-1")
	assert.True(t, state.Terminated)
}

func TestOrchestrator_ProcessAgentResult_ErrorHandling(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	// Agent fails with error
	instruction, err := orch.ProcessAgentResult("proc-1", "understand", nil, nil, false, "LLM failed")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
	assert.Equal(t, envelope.TerminalReasonToolFailedFatally, *instruction.TerminalReason)
}

func TestOrchestrator_ProcessAgentResult_ErrorRouting(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	// Add error_next to understand agent
	cfg.GetAgent("understand").ErrorNext = "respond"
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	// Agent fails with error -> should route to error_next
	instruction, err := orch.ProcessAgentResult("proc-1", "understand", nil, nil, false, "LLM failed")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
	assert.Equal(t, "respond", instruction.AgentName)
}

func TestOrchestrator_ProcessAgentResult_CompletePipeline(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)
	metrics := &AgentExecutionMetrics{LLMCalls: 1}

	// Get first instruction (this registers the first agent start)
	instruction, err := orch.GetNextInstruction("proc-1")
	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
	assert.Equal(t, "understand", instruction.AgentName)

	// Complete all agents
	orch.ProcessAgentResult("proc-1", "understand", map[string]any{"ok": true}, metrics, true, "")
	orch.ProcessAgentResult("proc-1", "think", map[string]any{"ok": true}, metrics, true, "")
	instruction, err = orch.ProcessAgentResult("proc-1", "respond", map[string]any{"ok": true}, metrics, true, "")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
	assert.Equal(t, envelope.TerminalReasonCompleted, *instruction.TerminalReason)

	// Verify envelope state
	assert.Equal(t, 3, instruction.Envelope.LLMCallCount)
	assert.Equal(t, 3, instruction.Envelope.AgentHopCount)
}

// =============================================================================
// GetSessionState Tests
// =============================================================================

func TestOrchestrator_GetSessionState(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)
	metrics := &AgentExecutionMetrics{LLMCalls: 1}
	orch.ProcessAgentResult("proc-1", "understand", map[string]any{"ok": true}, metrics, true, "")

	state, err := orch.GetSessionState("proc-1")

	require.NoError(t, err)
	assert.Equal(t, "proc-1", state.ProcessID)
	assert.Equal(t, "think", state.CurrentStage)
	assert.False(t, state.Terminated)
}

func TestOrchestrator_GetSessionState_UnknownProcess(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	_, err := orch.GetSessionState("unknown-proc")

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "unknown process")
}

// =============================================================================
// CleanupSession Tests
// =============================================================================

func TestOrchestrator_CleanupSession(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)
	assert.Equal(t, 1, orch.GetSessionCount())

	orch.CleanupSession("proc-1")

	assert.Equal(t, 0, orch.GetSessionCount())
	_, err := orch.GetSessionState("proc-1")
	assert.Error(t, err)
}

// =============================================================================
// Routing Rule Value Matching Tests
// =============================================================================

func TestOrchestrator_ValuesMatch_Strings(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	assert.True(t, orch.valuesMatch("proceed", "proceed"))
	assert.False(t, orch.valuesMatch("proceed", "loop_back"))
}

func TestOrchestrator_ValuesMatch_Booleans(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	assert.True(t, orch.valuesMatch(true, true))
	assert.True(t, orch.valuesMatch(false, false))
	assert.False(t, orch.valuesMatch(true, false))
}

func TestOrchestrator_ValuesMatch_Numbers(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	assert.True(t, orch.valuesMatch(1, 1))
	assert.True(t, orch.valuesMatch(1.5, 1.5))
	assert.True(t, orch.valuesMatch(int64(1), int32(1)))
	assert.False(t, orch.valuesMatch(1, 2))
}

func TestOrchestrator_ValuesMatch_JSON_Fallback(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	// Test complex objects fall back to JSON comparison
	obj1 := map[string]any{"key": "value"}
	obj2 := map[string]any{"key": "value"}
	obj3 := map[string]any{"key": "different"}

	assert.True(t, orch.valuesMatch(obj1, obj2))
	assert.False(t, orch.valuesMatch(obj1, obj3))
}

func TestOrchestrator_ValuesMatch_MixedTypes(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	// String vs int should not match
	assert.False(t, orch.valuesMatch("1", 1))
	// Bool vs string should not match
	assert.False(t, orch.valuesMatch(true, "true"))
}

// =============================================================================
// checkBounds Direct Tests
// =============================================================================

func TestOrchestrator_checkBounds_AllWithinLimits(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.Envelope.Iteration = 1
	session.Envelope.LLMCallCount = 5
	session.Envelope.AgentHopCount = 10

	canContinue, reason := orch.checkBounds(session)

	assert.True(t, canContinue)
	assert.Empty(t, reason)
}

func TestOrchestrator_checkBounds_IterationsExceeded(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.Envelope.Iteration = 10
	session.Envelope.MaxIterations = 3

	canContinue, reason := orch.checkBounds(session)

	assert.False(t, canContinue)
	assert.Equal(t, envelope.TerminalReasonMaxIterationsExceeded, reason)
}

func TestOrchestrator_checkBounds_LLMCallsExactlyAtLimit(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.Envelope.LLMCallCount = 10
	session.Envelope.MaxLLMCalls = 10 // At limit - should fail

	canContinue, reason := orch.checkBounds(session)

	assert.False(t, canContinue)
	assert.Equal(t, envelope.TerminalReasonMaxLLMCallsExceeded, reason)
}

func TestOrchestrator_checkBounds_AgentHopsExactlyAtLimit(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.Envelope.AgentHopCount = 20
	session.Envelope.MaxAgentHops = 20 // At limit - should fail

	canContinue, reason := orch.checkBounds(session)

	assert.False(t, canContinue)
	assert.Equal(t, envelope.TerminalReasonMaxAgentHopsExceeded, reason)
}

func TestOrchestrator_checkBounds_ZeroLimits(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	cfg.MaxIterations = 0
	cfg.MaxLLMCalls = 0
	cfg.MaxAgentHops = 0
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.Envelope.Iteration = 0
	session.Envelope.LLMCallCount = 0
	session.Envelope.AgentHopCount = 0

	canContinue, reason := orch.checkBounds(session)

	// With zero limits and zero counts, iterations check (0 >= 0) triggers first
	// because bounds are checked in order: iterations, LLM calls, agent hops
	assert.False(t, canContinue)
	assert.Equal(t, envelope.TerminalReasonMaxIterationsExceeded, reason)
}

// =============================================================================
// checkEdgeLimits Direct Tests
// =============================================================================

func TestOrchestrator_checkEdgeLimits_NoLimitConfigured(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline() // No edge limits
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.EdgeTraversals["understand->think"] = 100 // Many traversals

	// Should return true because no limit is configured
	result := orch.checkEdgeLimits(session, "understand", "think")

	assert.True(t, result)
}

func TestOrchestrator_checkEdgeLimits_WithinLimit(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipelineWithRouting() // Has edge limit executor->planner = 2
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.EdgeTraversals["executor->planner"] = 1 // Under limit

	result := orch.checkEdgeLimits(session, "executor", "planner")

	assert.True(t, result)
}

func TestOrchestrator_checkEdgeLimits_AtLimit(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipelineWithRouting() // Has edge limit executor->planner = 2
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.EdgeTraversals["executor->planner"] = 2 // At limit

	result := orch.checkEdgeLimits(session, "executor", "planner")

	assert.True(t, result) // At limit should still be OK (count <= limit)
}

func TestOrchestrator_checkEdgeLimits_ExceedsLimit(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipelineWithRouting() // Has edge limit executor->planner = 2
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]
	session.EdgeTraversals["executor->planner"] = 3 // Exceeds limit

	result := orch.checkEdgeLimits(session, "executor", "planner")

	assert.False(t, result)
}

// =============================================================================
// isLoopBack Direct Tests
// =============================================================================

func TestOrchestrator_isLoopBack_ForwardTransition(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]

	// understand (0) -> think (1) is forward
	result := orch.isLoopBack(session, "understand", "think")
	assert.False(t, result)

	// think (1) -> respond (2) is forward
	result = orch.isLoopBack(session, "think", "respond")
	assert.False(t, result)
}

func TestOrchestrator_isLoopBack_BackwardTransition(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]

	// think (1) -> understand (0) is backward
	result := orch.isLoopBack(session, "think", "understand")
	assert.True(t, result)

	// respond (2) -> understand (0) is backward
	result = orch.isLoopBack(session, "respond", "understand")
	assert.True(t, result)
}

func TestOrchestrator_isLoopBack_SameStage(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]

	// Same stage is not a loop-back (toIdx < fromIdx is false when equal)
	result := orch.isLoopBack(session, "think", "think")
	assert.False(t, result)
}

func TestOrchestrator_isLoopBack_UnknownStages(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	session := orch.sessions["proc-1"]

	// Unknown stages should return false
	result := orch.isLoopBack(session, "unknown1", "unknown2")
	assert.False(t, result)

	result = orch.isLoopBack(session, "understand", "unknown")
	assert.False(t, result)
}

// =============================================================================
// Concurrent Access Tests
// =============================================================================

func TestOrchestrator_ConcurrentSessionCreation(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()

	const numSessions = 100
	done := make(chan bool, numSessions)

	for i := 0; i < numSessions; i++ {
		go func(id int) {
			env := createTestEnvelope()
			processID := fmt.Sprintf("proc-%d", id)
			_, err := orch.InitializeSession(processID, cfg, env, false)
			assert.NoError(t, err)
			done <- true
		}(i)
	}

	// Wait for all goroutines
	for i := 0; i < numSessions; i++ {
		<-done
	}

	assert.Equal(t, numSessions, orch.GetSessionCount())
}

func TestOrchestrator_ConcurrentGetNextInstruction(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	const numReads = 100
	done := make(chan bool, numReads)

	for i := 0; i < numReads; i++ {
		go func() {
			instruction, err := orch.GetNextInstruction("proc-1")
			assert.NoError(t, err)
			assert.NotNil(t, instruction)
			done <- true
		}()
	}

	// Wait for all goroutines
	for i := 0; i < numReads; i++ {
		<-done
	}
}

func TestOrchestrator_ConcurrentProcessAgentResult(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	const numSessions = 50
	done := make(chan bool, numSessions)

	// Create multiple sessions
	for i := 0; i < numSessions; i++ {
		cfg := createTestPipeline()
		env := createTestEnvelope()
		processID := fmt.Sprintf("proc-%d", i)
		orch.InitializeSession(processID, cfg, env, false)
	}

	// Process agent results concurrently on different sessions
	for i := 0; i < numSessions; i++ {
		go func(id int) {
			processID := fmt.Sprintf("proc-%d", id)
			metrics := &AgentExecutionMetrics{LLMCalls: 1}
			instruction, err := orch.ProcessAgentResult(
				processID, "understand",
				map[string]any{"ok": true},
				metrics, true, "",
			)
			assert.NoError(t, err)
			assert.NotNil(t, instruction)
			done <- true
		}(i)
	}

	// Wait for all goroutines
	for i := 0; i < numSessions; i++ {
		<-done
	}
}

// =============================================================================
// Edge Case Tests
// =============================================================================

func TestOrchestrator_ProcessAgentResult_NilMetrics(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	// Process with nil metrics - should not panic
	instruction, err := orch.ProcessAgentResult(
		"proc-1", "understand",
		map[string]any{"ok": true},
		nil, // nil metrics
		true, "",
	)

	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
}

func TestOrchestrator_ProcessAgentResult_NilOutput(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	// Process with nil output - should not panic
	instruction, err := orch.ProcessAgentResult(
		"proc-1", "understand",
		nil, // nil output
		&AgentExecutionMetrics{LLMCalls: 1},
		true, "",
	)

	require.NoError(t, err)
	assert.Equal(t, InstructionKindRunAgent, instruction.Kind)
}

func TestOrchestrator_ProcessAgentResult_UnknownProcess(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	_, err := orch.ProcessAgentResult(
		"unknown-proc", "understand",
		map[string]any{"ok": true},
		nil, true, "",
	)

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "unknown process")
}

func TestOrchestrator_ProcessAgentResult_OnTerminatedSession(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()
	env.CurrentStage = "end"

	orch.InitializeSession("proc-1", cfg, env, false)

	// Get instruction to trigger termination
	instruction, _ := orch.GetNextInstruction("proc-1")
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)

	// Try to process result on terminated session
	instruction, err := orch.ProcessAgentResult(
		"proc-1", "respond",
		map[string]any{"response": "test"},
		nil, true, "",
	)

	// Should still return terminate instruction
	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
}

func TestOrchestrator_InitializeSession_DuplicateProcessID(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env1 := createTestEnvelope()
	env2 := createTestEnvelope()

	// First session - should succeed
	_, err := orch.InitializeSession("proc-1", cfg, env1, false)
	require.NoError(t, err)

	// Second session with same ID without force - should fail (K8s-style)
	_, err = orch.InitializeSession("proc-1", cfg, env2, false)
	require.Error(t, err)
	assert.Contains(t, err.Error(), "session already exists")

	// Count should still be 1 (second call failed)
	assert.Equal(t, 1, orch.GetSessionCount())
}

func TestOrchestrator_InitializeSession_DuplicateProcessID_WithForce(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env1 := createTestEnvelope()
	env2 := createTestEnvelope()

	// First session - should succeed
	_, err := orch.InitializeSession("proc-1", cfg, env1, false)
	require.NoError(t, err)

	// Second session with same ID WITH force - should succeed and replace
	_, err = orch.InitializeSession("proc-1", cfg, env2, true)
	require.NoError(t, err)

	// Count should still be 1 (replaced, not added)
	assert.Equal(t, 1, orch.GetSessionCount())
}

func TestOrchestrator_GetNextInstruction_UnknownStage(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)

	// Manually set an unknown stage after initialization
	// (InitializeSession auto-corrects invalid stages, so we bypass that)
	orch.mu.Lock()
	orch.sessions["proc-1"].Envelope.CurrentStage = "nonexistent_stage"
	orch.mu.Unlock()

	instruction, err := orch.GetNextInstruction("proc-1")

	require.NoError(t, err)
	assert.Equal(t, InstructionKindTerminate, instruction.Kind)
	assert.Equal(t, envelope.TerminalReasonToolFailedFatally, *instruction.TerminalReason)
}

func TestOrchestrator_CleanupSession_NonExistent(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	// Should not panic
	orch.CleanupSession("nonexistent")

	assert.Equal(t, 0, orch.GetSessionCount())
}

func TestOrchestrator_CleanupSession_Idempotent(t *testing.T) {
	orch := NewOrchestrator(nil, nil)
	cfg := createTestPipeline()
	env := createTestEnvelope()

	orch.InitializeSession("proc-1", cfg, env, false)
	assert.Equal(t, 1, orch.GetSessionCount())

	// First cleanup
	orch.CleanupSession("proc-1")
	assert.Equal(t, 0, orch.GetSessionCount())

	// Second cleanup - should not panic
	orch.CleanupSession("proc-1")
	assert.Equal(t, 0, orch.GetSessionCount())
}

// =============================================================================
// toFloat64 Tests
// =============================================================================

func TestToFloat64_Int(t *testing.T) {
	val, ok := toFloat64(42)
	assert.True(t, ok)
	assert.Equal(t, float64(42), val)
}

func TestToFloat64_Int32(t *testing.T) {
	val, ok := toFloat64(int32(42))
	assert.True(t, ok)
	assert.Equal(t, float64(42), val)
}

func TestToFloat64_Int64(t *testing.T) {
	val, ok := toFloat64(int64(42))
	assert.True(t, ok)
	assert.Equal(t, float64(42), val)
}

func TestToFloat64_Float32(t *testing.T) {
	val, ok := toFloat64(float32(3.14))
	assert.True(t, ok)
	assert.InDelta(t, 3.14, val, 0.001)
}

func TestToFloat64_Float64(t *testing.T) {
	val, ok := toFloat64(3.14159)
	assert.True(t, ok)
	assert.Equal(t, 3.14159, val)
}

func TestToFloat64_String(t *testing.T) {
	_, ok := toFloat64("42")
	assert.False(t, ok)
}

func TestToFloat64_Bool(t *testing.T) {
	_, ok := toFloat64(true)
	assert.False(t, ok)
}

func TestToFloat64_Nil(t *testing.T) {
	_, ok := toFloat64(nil)
	assert.False(t, ok)
}
