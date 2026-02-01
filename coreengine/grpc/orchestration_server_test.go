package grpc

import (
	"context"
	"encoding/json"
	"testing"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/kernel"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

// CreateTestOrchestrationServer creates an OrchestrationServer with mock logger for testing.
func CreateTestOrchestrationServer() (*OrchestrationServer, *MockLogger, *kernel.Orchestrator) {
	logger := &MockLogger{}
	orch := kernel.NewOrchestrator(nil, logger)
	server := NewOrchestrationServer(logger, orch)
	return server, logger, orch
}

// createTestPipelineConfigJSON creates a valid pipeline config JSON for testing.
func createTestPipelineConfigJSON() []byte {
	cfg := config.PipelineConfig{
		Name: "test-pipeline",
		Agents: []*config.AgentConfig{
			{
				Name:        "start",
				StageOrder:  0,
				DefaultNext: "end",
			},
			{
				Name:       "end",
				StageOrder: 1,
			},
		},
	}
	data, _ := json.Marshal(cfg)
	return data
}

// createTestEnvelopeJSON creates a valid envelope JSON for testing.
func createTestEnvelopeJSON() []byte {
	env := map[string]any{
		"envelope_id":    "test-env-1",
		"current_stage":  "start",
		"iteration":      0,
		"max_iterations": 10,
		"max_llm_calls":  100,
		"max_agent_hops": 50,
		"outputs":        map[string]any{},
	}
	data, _ := json.Marshal(env)
	return data
}

// =============================================================================
// CONSTRUCTOR TESTS
// =============================================================================

func TestNewOrchestrationServer(t *testing.T) {
	logger := &MockLogger{}
	orch := kernel.NewOrchestrator(nil, logger)

	server := NewOrchestrationServer(logger, orch)

	require.NotNil(t, server)
	assert.Equal(t, logger, server.logger)
	assert.Equal(t, orch, server.orchestrator)
}

// =============================================================================
// INITIALIZE SESSION TESTS
// =============================================================================

func TestOrchestrationServer_InitializeSession_Success(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.InitializeSessionRequest{
		ProcessId:      "proc-1",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}

	resp, err := server.InitializeSession(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "proc-1", resp.ProcessId)
	assert.Equal(t, "start", resp.CurrentStage)
	assert.False(t, resp.Terminated)
	assert.Contains(t, logger.infoCalls, "orchestration_session_initialized")
}

func TestOrchestrationServer_InitializeSession_EmptyProcessId(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.InitializeSessionRequest{
		ProcessId:      "",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}

	resp, err := server.InitializeSession(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Contains(t, st.Message(), "process_id is required")
}

func TestOrchestrationServer_InitializeSession_InvalidPipelineConfig(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.InitializeSessionRequest{
		ProcessId:      "proc-1",
		PipelineConfig: []byte("invalid json"),
		Envelope:       createTestEnvelopeJSON(),
	}

	resp, err := server.InitializeSession(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Contains(t, st.Message(), "invalid pipeline config")
	assert.Contains(t, logger.errorCalls, "invalid_pipeline_config")
}

func TestOrchestrationServer_InitializeSession_InvalidEnvelope(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.InitializeSessionRequest{
		ProcessId:      "proc-1",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       []byte("invalid json"),
	}

	resp, err := server.InitializeSession(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Contains(t, st.Message(), "invalid envelope")
	assert.Contains(t, logger.errorCalls, "invalid_envelope")
}

func TestOrchestrationServer_InitializeSession_DuplicateProcessId(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.InitializeSessionRequest{
		ProcessId:      "proc-dup",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}

	// First call - should succeed
	resp1, err1 := server.InitializeSession(ctx, req)
	require.NoError(t, err1)
	require.NotNil(t, resp1)

	// Second call with same ID - should fail with ALREADY_EXISTS
	resp2, err2 := server.InitializeSession(ctx, req)

	require.Error(t, err2)
	assert.Nil(t, resp2)

	st, ok := status.FromError(err2)
	require.True(t, ok)
	assert.Equal(t, codes.AlreadyExists, st.Code())
	assert.Contains(t, st.Message(), "session already exists")
	assert.Contains(t, logger.errorCalls, "initialize_session_failed")
}

func TestOrchestrationServer_InitializeSession_DuplicateWithForce(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.InitializeSessionRequest{
		ProcessId:      "proc-force",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}

	// First call - should succeed
	resp1, err1 := server.InitializeSession(ctx, req)
	require.NoError(t, err1)
	require.NotNil(t, resp1)

	// Second call with force=true - should succeed and replace
	req.Force = true
	resp2, err2 := server.InitializeSession(ctx, req)

	require.NoError(t, err2)
	require.NotNil(t, resp2)
	assert.Equal(t, "proc-force", resp2.ProcessId)

	// Should have logged session initialization at least twice (force replace logs too)
	initCount := 0
	for _, msg := range logger.infoCalls {
		if msg == "orchestration_session_initialized" {
			initCount++
		}
	}
	assert.GreaterOrEqual(t, initCount, 2)
}

// =============================================================================
// GET NEXT INSTRUCTION TESTS
// =============================================================================

func TestOrchestrationServer_GetNextInstruction_Success(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// First initialize a session
	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-instr",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Now get next instruction
	req := &pb.GetNextInstructionRequest{
		ProcessId: "proc-instr",
	}

	resp, err := server.GetNextInstruction(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	// Instruction should be valid (either RUN_AGENT or TERMINATE)
	assert.True(t, resp.Kind == pb.InstructionKind_RUN_AGENT || resp.Kind == pb.InstructionKind_TERMINATE,
		"expected RUN_AGENT or TERMINATE, got %v", resp.Kind)
	assert.Contains(t, logger.debugCalls, "next_instruction_returned")
}

func TestOrchestrationServer_GetNextInstruction_EmptyProcessId(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.GetNextInstructionRequest{
		ProcessId: "",
	}

	resp, err := server.GetNextInstruction(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Contains(t, st.Message(), "process_id is required")
}

func TestOrchestrationServer_GetNextInstruction_NotFound(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.GetNextInstructionRequest{
		ProcessId: "non-existent",
	}

	resp, err := server.GetNextInstruction(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.NotFound, st.Code())
	assert.Contains(t, st.Message(), "process not found")
	assert.Contains(t, logger.warnCalls, "get_next_instruction_failed")
}

// =============================================================================
// REPORT AGENT RESULT TESTS
// =============================================================================

func TestOrchestrationServer_ReportAgentResult_Success(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Initialize session first
	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-result",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Report agent result
	output, _ := json.Marshal(map[string]any{"result": "success"})
	req := &pb.ReportAgentResultRequest{
		ProcessId: "proc-result",
		AgentName: "start",
		Output:    output,
		Success:   true,
	}

	resp, err := server.ReportAgentResult(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	// After start completes, should get end agent or terminate
	assert.Contains(t, logger.debugCalls, "agent_result_processed")
}

func TestOrchestrationServer_ReportAgentResult_EmptyProcessId(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.ReportAgentResultRequest{
		ProcessId: "",
		AgentName: "start",
		Success:   true,
	}

	resp, err := server.ReportAgentResult(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Contains(t, st.Message(), "process_id is required")
}

func TestOrchestrationServer_ReportAgentResult_EmptyAgentName(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.ReportAgentResultRequest{
		ProcessId: "proc-1",
		AgentName: "",
		Success:   true,
	}

	resp, err := server.ReportAgentResult(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Contains(t, st.Message(), "agent_name is required")
}

func TestOrchestrationServer_ReportAgentResult_WithMetrics(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Initialize session first
	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-metrics",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Report with metrics
	req := &pb.ReportAgentResultRequest{
		ProcessId: "proc-metrics",
		AgentName: "start",
		Success:   true,
		Metrics: &pb.AgentExecutionMetrics{
			LlmCalls:   5,
			ToolCalls:  3,
			TokensIn:   1000,
			TokensOut:  500,
			DurationMs: 2000,
		},
	}

	resp, err := server.ReportAgentResult(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
}

func TestOrchestrationServer_ReportAgentResult_InvalidOutput(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Initialize session first
	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-invalid-output",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Report with invalid JSON output - should continue with nil output
	req := &pb.ReportAgentResultRequest{
		ProcessId: "proc-invalid-output",
		AgentName: "start",
		Output:    []byte("invalid json"),
		Success:   true,
	}

	resp, err := server.ReportAgentResult(ctx, req)

	// Should still succeed - invalid output is not fatal
	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Contains(t, logger.warnCalls, "invalid_agent_output")
}

func TestOrchestrationServer_ReportAgentResult_ProcessNotFound(t *testing.T) {
	server, logger, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.ReportAgentResultRequest{
		ProcessId: "non-existent",
		AgentName: "start",
		Success:   true,
	}

	resp, err := server.ReportAgentResult(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Internal, st.Code())
	assert.Contains(t, st.Message(), "failed to process result")
	assert.Contains(t, logger.errorCalls, "process_agent_result_failed")
}

func TestOrchestrationServer_ReportAgentResult_WithError(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Initialize session first
	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-error",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Report error result
	req := &pb.ReportAgentResultRequest{
		ProcessId: "proc-error",
		AgentName: "start",
		Success:   false,
		Error:     "agent failed",
	}

	resp, err := server.ReportAgentResult(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
}

// =============================================================================
// GET SESSION STATE TESTS
// =============================================================================

func TestOrchestrationServer_GetSessionState_Success(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Initialize session first
	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-state",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Get session state
	req := &pb.GetSessionStateRequest{
		ProcessId: "proc-state",
	}

	resp, err := server.GetSessionState(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "proc-state", resp.ProcessId)
	assert.Equal(t, "start", resp.CurrentStage)
	assert.False(t, resp.Terminated)
	assert.NotNil(t, resp.Envelope)
}

func TestOrchestrationServer_GetSessionState_EmptyProcessId(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.GetSessionStateRequest{
		ProcessId: "",
	}

	resp, err := server.GetSessionState(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Contains(t, st.Message(), "process_id is required")
}

func TestOrchestrationServer_GetSessionState_NotFound(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	req := &pb.GetSessionStateRequest{
		ProcessId: "non-existent",
	}

	resp, err := server.GetSessionState(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.NotFound, st.Code())
	assert.Contains(t, st.Message(), "session not found")
}

// =============================================================================
// CONVERSION FUNCTION TESTS (via RPC)
// =============================================================================

func TestOrchestrationServer_InstructionToProto_AllKinds(t *testing.T) {
	// Test that all instruction kinds are properly converted
	tests := []struct {
		name     string
		kind     kernel.InstructionKind
		expected pb.InstructionKind
	}{
		{"RUN_AGENT", kernel.InstructionKindRunAgent, pb.InstructionKind_RUN_AGENT},
		{"TERMINATE", kernel.InstructionKindTerminate, pb.InstructionKind_TERMINATE},
		{"WAIT_INTERRUPT", kernel.InstructionKindWaitInterrupt, pb.InstructionKind_WAIT_INTERRUPT},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := instructionKindToProto(tt.kind)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestOrchestrationServer_InstructionToProto_UnknownKind(t *testing.T) {
	// Unknown kind should return UNSPECIFIED
	result := instructionKindToProto(kernel.InstructionKind("unknown"))
	assert.Equal(t, pb.InstructionKind_INSTRUCTION_KIND_UNSPECIFIED, result)
}

func TestOrchestrationServer_SessionStateToProto_WithEdgeTraversals(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Create pipeline with routing that creates edge traversals
	cfg := config.PipelineConfig{
		Name: "test-pipeline",
		Agents: []*config.AgentConfig{
			{
				Name:        "agent1",
				StageOrder:  0,
				DefaultNext: "agent2",
			},
			{
				Name:        "agent2",
				StageOrder:  1,
				DefaultNext: "end",
			},
			{
				Name:       "end",
				StageOrder: 2,
			},
		},
	}
	cfgJSON, _ := json.Marshal(cfg)

	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-edges",
		PipelineConfig: cfgJSON,
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Report result to transition through agents
	resultReq := &pb.ReportAgentResultRequest{
		ProcessId: "proc-edges",
		AgentName: "agent1",
		Success:   true,
	}
	_, err = server.ReportAgentResult(ctx, resultReq)
	require.NoError(t, err)

	// Get state and check edge traversals
	stateReq := &pb.GetSessionStateRequest{ProcessId: "proc-edges"}
	state, err := server.GetSessionState(ctx, stateReq)
	require.NoError(t, err)

	// Should have edge traversal recorded
	assert.NotNil(t, state.EdgeTraversals)
}

func TestOrchestrationServer_InstructionToProto_WithTerminalReason(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Create simple pipeline that completes
	cfg := config.PipelineConfig{
		Name: "test-pipeline",
		Agents: []*config.AgentConfig{
			{
				Name:       "only",
				StageOrder: 0,
				// No default_next means it should complete
			},
		},
	}
	cfgJSON, _ := json.Marshal(cfg)

	env := map[string]any{
		"envelope_id":    "test-env",
		"current_stage":  "only",
		"iteration":      0,
		"max_iterations": 10,
		"max_llm_calls":  100,
		"max_agent_hops": 50,
		"outputs":        map[string]any{},
	}
	envJSON, _ := json.Marshal(env)

	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-terminal",
		PipelineConfig: cfgJSON,
		Envelope:       envJSON,
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Report result - should get terminal instruction
	resultReq := &pb.ReportAgentResultRequest{
		ProcessId: "proc-terminal",
		AgentName: "only",
		Success:   true,
	}
	instr, err := server.ReportAgentResult(ctx, resultReq)
	require.NoError(t, err)

	assert.Equal(t, pb.InstructionKind_TERMINATE, instr.Kind)
	assert.Equal(t, pb.TerminalReason_COMPLETED, instr.TerminalReason)
}

// =============================================================================
// FULL PIPELINE FLOW TEST
// =============================================================================

func TestOrchestrationServer_FullPipelineFlow(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Initialize session
	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-flow",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	initResp, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)
	assert.NotEmpty(t, initResp.CurrentStage)

	// Get first instruction - may be RUN_AGENT or TERMINATE depending on pipeline
	instrReq := &pb.GetNextInstructionRequest{ProcessId: "proc-flow"}
	instr1, err := server.GetNextInstruction(ctx, instrReq)
	require.NoError(t, err)
	require.NotNil(t, instr1)

	// If we got RUN_AGENT, report result and continue
	if instr1.Kind == pb.InstructionKind_RUN_AGENT && instr1.AgentName != "" {
		resultReq := &pb.ReportAgentResultRequest{
			ProcessId: "proc-flow",
			AgentName: instr1.AgentName,
			Success:   true,
		}
		instr2, err := server.ReportAgentResult(ctx, resultReq)
		require.NoError(t, err)
		require.NotNil(t, instr2)

		// Continue until we hit TERMINATE
		for instr2.Kind == pb.InstructionKind_RUN_AGENT && instr2.AgentName != "" {
			resultReq.AgentName = instr2.AgentName
			instr2, err = server.ReportAgentResult(ctx, resultReq)
			require.NoError(t, err)
		}
	}

	// Eventually session should be queryable
	stateReq := &pb.GetSessionStateRequest{ProcessId: "proc-flow"}
	state, err := server.GetSessionState(ctx, stateReq)
	require.NoError(t, err)
	assert.NotNil(t, state)
}

// =============================================================================
// CONCURRENT ACCESS TESTS
// =============================================================================

func TestOrchestrationServer_ConcurrentSessionCreation(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	// Create multiple sessions concurrently
	done := make(chan bool, 10)
	for i := 0; i < 10; i++ {
		go func(idx int) {
			env := map[string]any{
				"envelope_id":    "env-" + string(rune('0'+idx)),
				"current_stage":  "start",
				"iteration":      0,
				"max_iterations": 10,
				"max_llm_calls":  100,
				"max_agent_hops": 50,
				"outputs":        map[string]any{},
			}
			envJSON, _ := json.Marshal(env)

			req := &pb.InitializeSessionRequest{
				ProcessId:      "proc-concurrent-" + string(rune('0'+idx)),
				PipelineConfig: createTestPipelineConfigJSON(),
				Envelope:       envJSON,
			}
			_, err := server.InitializeSession(ctx, req)
			done <- err == nil
		}(i)
	}

	// Wait for all
	successCount := 0
	for i := 0; i < 10; i++ {
		if <-done {
			successCount++
		}
	}

	assert.Equal(t, 10, successCount)
}

// =============================================================================
// INSTRUCTION WITH ENVELOPE TEST
// =============================================================================

func TestOrchestrationServer_InstructionContainsEnvelope(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-env-check",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Get instruction - should have envelope
	instrReq := &pb.GetNextInstructionRequest{ProcessId: "proc-env-check"}
	instr, err := server.GetNextInstruction(ctx, instrReq)
	require.NoError(t, err)

	assert.NotEmpty(t, instr.Envelope, "instruction should contain envelope")

	// Verify envelope can be unmarshaled
	var envMap map[string]any
	err = json.Unmarshal(instr.Envelope, &envMap)
	require.NoError(t, err)
	assert.Equal(t, "test-env-1", envMap["envelope_id"])
}

// =============================================================================
// AGENT CONFIG IN INSTRUCTION TEST
// =============================================================================

func TestOrchestrationServer_InstructionContainsAgentConfig(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()
	ctx := context.Background()

	initReq := &pb.InitializeSessionRequest{
		ProcessId:      "proc-config-check",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}
	_, err := server.InitializeSession(ctx, initReq)
	require.NoError(t, err)

	// Get instruction
	instrReq := &pb.GetNextInstructionRequest{ProcessId: "proc-config-check"}
	instr, err := server.GetNextInstruction(ctx, instrReq)
	require.NoError(t, err)

	// Only RUN_AGENT instructions have agent config
	if instr.Kind == pb.InstructionKind_RUN_AGENT {
		assert.NotEmpty(t, instr.AgentConfig, "RUN_AGENT instruction should contain agent config")

		// Verify agent config can be unmarshaled
		var cfgMap map[string]any
		err = json.Unmarshal(instr.AgentConfig, &cfgMap)
		require.NoError(t, err)
		assert.NotEmpty(t, cfgMap["name"])
	} else {
		// TERMINATE or other instructions may not have agent config
		t.Log("Got non-RUN_AGENT instruction, skipping agent config check")
	}
}

// =============================================================================
// CONTEXT DEADLINE TESTS
// =============================================================================

func TestOrchestrationServer_InitializeSession_DeadlineExceeded(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()

	ctx, cancel := context.WithTimeout(context.Background(), time.Millisecond)
	defer cancel()
	time.Sleep(5 * time.Millisecond) // Wait for deadline to expire

	req := &pb.InitializeSessionRequest{
		ProcessId:      "proc-deadline",
		PipelineConfig: createTestPipelineConfigJSON(),
		Envelope:       createTestEnvelopeJSON(),
	}

	resp, err := server.InitializeSession(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.DeadlineExceeded, st.Code())
}

func TestOrchestrationServer_GetNextInstruction_DeadlineExceeded(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()

	ctx, cancel := context.WithTimeout(context.Background(), time.Millisecond)
	defer cancel()
	time.Sleep(5 * time.Millisecond)

	req := &pb.GetNextInstructionRequest{ProcessId: "proc-1"}
	resp, err := server.GetNextInstruction(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.DeadlineExceeded, st.Code())
}

func TestOrchestrationServer_ReportAgentResult_DeadlineExceeded(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()

	ctx, cancel := context.WithTimeout(context.Background(), time.Millisecond)
	defer cancel()
	time.Sleep(5 * time.Millisecond)

	req := &pb.ReportAgentResultRequest{
		ProcessId: "proc-1",
		AgentName: "start",
		Success:   true,
	}
	resp, err := server.ReportAgentResult(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.DeadlineExceeded, st.Code())
}

func TestOrchestrationServer_GetSessionState_DeadlineExceeded(t *testing.T) {
	server, _, _ := CreateTestOrchestrationServer()

	ctx, cancel := context.WithTimeout(context.Background(), time.Millisecond)
	defer cancel()
	time.Sleep(5 * time.Millisecond)

	req := &pb.GetSessionStateRequest{ProcessId: "proc-1"}
	resp, err := server.GetSessionState(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.DeadlineExceeded, st.Code())
}
