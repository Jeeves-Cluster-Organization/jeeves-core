// Package grpc provides tests for the gRPC server.
//
// Constitutional Compliance:
// - Contract 11 (CONTRACTS.md): Envelope round-trip must be lossless
// - Contract 12 (CONTRACTS.md): Go is authoritative for bounds checking
package grpc

import (
	"context"
	"sync"
	"testing"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

// MockLogger implements Logger for testing.
type MockLogger struct {
	debugCalls []string
	infoCalls  []string
	warnCalls  []string
	errorCalls []string
}

func (m *MockLogger) Debug(msg string, keysAndValues ...any) {
	m.debugCalls = append(m.debugCalls, msg)
}

func (m *MockLogger) Info(msg string, keysAndValues ...any) {
	m.infoCalls = append(m.infoCalls, msg)
}

func (m *MockLogger) Warn(msg string, keysAndValues ...any) {
	m.warnCalls = append(m.warnCalls, msg)
}

func (m *MockLogger) Error(msg string, keysAndValues ...any) {
	m.errorCalls = append(m.errorCalls, msg)
}

func createTestServer() (*JeevesCoreServer, *MockLogger) {
	logger := &MockLogger{}
	server := NewJeevesCoreServer(logger)
	return server, logger
}

// =============================================================================
// ENVELOPE OPERATIONS TESTS
// =============================================================================

func TestCreateEnvelope(t *testing.T) {
	// Test CreateEnvelope RPC.
	server, logger := createTestServer()
	ctx := context.Background()

	req := &pb.CreateEnvelopeRequest{
		RawInput:   "Hello, world!",
		UserId:     "user_123",
		SessionId:  "session_456",
		RequestId:  "req_789",
		Metadata:   map[string]string{"key": "value"},
		StageOrder: []string{"stage1", "stage2"},
	}

	resp, err := server.CreateEnvelope(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)

	assert.Equal(t, "Hello, world!", resp.RawInput)
	assert.Equal(t, "user_123", resp.UserId)
	assert.Equal(t, "session_456", resp.SessionId)
	assert.Equal(t, "req_789", resp.RequestId)
	assert.Equal(t, []string{"stage1", "stage2"}, resp.StageOrder)
	assert.NotEmpty(t, resp.EnvelopeId)
	assert.Contains(t, logger.debugCalls, "envelope_created")
}

func TestCreateEnvelopeWithoutRequestId(t *testing.T) {
	// Test CreateEnvelope generates request ID when not provided.
	server, _ := createTestServer()
	ctx := context.Background()

	req := &pb.CreateEnvelopeRequest{
		RawInput:  "Test",
		UserId:    "user",
		SessionId: "session",
		// No RequestId
	}

	resp, err := server.CreateEnvelope(ctx, req)

	require.NoError(t, err)
	assert.NotEmpty(t, resp.RequestId)
	assert.True(t, len(resp.RequestId) > 0)
}

func TestUpdateEnvelope(t *testing.T) {
	// Test UpdateEnvelope RPC.
	server, logger := createTestServer()
	ctx := context.Background()

	// Create a proto envelope
	protoEnv := &pb.Envelope{
		EnvelopeId:    "env_test",
		RequestId:     "req_test",
		UserId:        "user_123",
		SessionId:     "session_456",
		RawInput:      "Updated input",
		CurrentStage:  "stage2",
		LlmCallCount:  5,
		AgentHopCount: 2,
	}

	req := &pb.UpdateEnvelopeRequest{
		Envelope: protoEnv,
	}

	resp, err := server.UpdateEnvelope(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)

	assert.Equal(t, "env_test", resp.EnvelopeId)
	assert.Equal(t, "Updated input", resp.RawInput)
	assert.Equal(t, int32(5), resp.LlmCallCount)
	assert.Equal(t, int32(2), resp.AgentHopCount)
	assert.Contains(t, logger.debugCalls, "envelope_updated")
}

func TestCloneEnvelope(t *testing.T) {
	// Test CloneEnvelope RPC.
	server, logger := createTestServer()
	ctx := context.Background()

	protoEnv := &pb.Envelope{
		EnvelopeId:   "env_original",
		RequestId:    "req_test",
		UserId:       "user_123",
		SessionId:    "session_456",
		RawInput:     "Clone me",
		CurrentStage: "stage1",
	}

	req := &pb.CloneRequest{
		Envelope: protoEnv,
	}

	resp, err := server.CloneEnvelope(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)

	// Clone should have same values
	assert.Equal(t, "env_original", resp.EnvelopeId)
	assert.Equal(t, "Clone me", resp.RawInput)
	assert.Contains(t, logger.debugCalls, "envelope_cloned")
}

// =============================================================================
// BOUNDS CHECKING TESTS (Contract 12: Go is authoritative)
// =============================================================================

func TestCheckBoundsWithinLimits(t *testing.T) {
	// Test CheckBounds when within all limits.
	server, logger := createTestServer()
	ctx := context.Background()

	protoEnv := &pb.Envelope{
		EnvelopeId:    "env_test",
		Iteration:     0,
		MaxIterations: 3,
		LlmCallCount:  0,
		MaxLlmCalls:   10,
		AgentHopCount: 0,
		MaxAgentHops:  21,
		Terminated:    false,
	}

	result, err := server.CheckBounds(ctx, protoEnv)

	require.NoError(t, err)
	require.NotNil(t, result)

	assert.True(t, result.CanContinue)
	assert.Equal(t, pb.TerminalReason_TERMINAL_REASON_UNSPECIFIED, result.TerminalReason)
	assert.Equal(t, int32(10), result.LlmCallsRemaining)
	assert.Equal(t, int32(21), result.AgentHopsRemaining)
	assert.Equal(t, int32(3), result.IterationsRemaining)
	assert.Contains(t, logger.debugCalls, "bounds_checked")
}

func TestCheckBoundsMaxIterationsExceeded(t *testing.T) {
	// Test CheckBounds when max iterations exceeded.
	server, _ := createTestServer()
	ctx := context.Background()

	protoEnv := &pb.Envelope{
		EnvelopeId:    "env_test",
		Iteration:     4, // Exceeds max
		MaxIterations: 3,
		LlmCallCount:  0,
		MaxLlmCalls:   10,
		AgentHopCount: 0,
		MaxAgentHops:  21,
	}

	result, err := server.CheckBounds(ctx, protoEnv)

	require.NoError(t, err)
	assert.False(t, result.CanContinue)
	assert.Equal(t, pb.TerminalReason_MAX_ITERATIONS_EXCEEDED, result.TerminalReason)
	assert.Equal(t, int32(-1), result.IterationsRemaining)
}

func TestCheckBoundsMaxLLMCallsExceeded(t *testing.T) {
	// Test CheckBounds when max LLM calls exceeded.
	server, _ := createTestServer()
	ctx := context.Background()

	protoEnv := &pb.Envelope{
		EnvelopeId:    "env_test",
		Iteration:     0,
		MaxIterations: 3,
		LlmCallCount:  10, // At max
		MaxLlmCalls:   10,
		AgentHopCount: 0,
		MaxAgentHops:  21,
	}

	result, err := server.CheckBounds(ctx, protoEnv)

	require.NoError(t, err)
	assert.False(t, result.CanContinue)
	assert.Equal(t, pb.TerminalReason_MAX_LLM_CALLS_EXCEEDED, result.TerminalReason)
	assert.Equal(t, int32(0), result.LlmCallsRemaining)
}

func TestCheckBoundsMaxAgentHopsExceeded(t *testing.T) {
	// Test CheckBounds when max agent hops exceeded.
	server, _ := createTestServer()
	ctx := context.Background()

	protoEnv := &pb.Envelope{
		EnvelopeId:    "env_test",
		Iteration:     0,
		MaxIterations: 3,
		LlmCallCount:  0,
		MaxLlmCalls:   10,
		AgentHopCount: 21, // At max
		MaxAgentHops:  21,
	}

	result, err := server.CheckBounds(ctx, protoEnv)

	require.NoError(t, err)
	assert.False(t, result.CanContinue)
	assert.Equal(t, pb.TerminalReason_MAX_AGENT_HOPS_EXCEEDED, result.TerminalReason)
	assert.Equal(t, int32(0), result.AgentHopsRemaining)
}

func TestCheckBoundsTerminated(t *testing.T) {
	// Test CheckBounds when envelope is terminated.
	server, _ := createTestServer()
	ctx := context.Background()

	protoEnv := &pb.Envelope{
		EnvelopeId:    "env_test",
		Terminated:    true, // Already terminated
		Iteration:     0,
		MaxIterations: 3,
		LlmCallCount:  0,
		MaxLlmCalls:   10,
		AgentHopCount: 0,
		MaxAgentHops:  21,
	}

	result, err := server.CheckBounds(ctx, protoEnv)

	require.NoError(t, err)
	assert.False(t, result.CanContinue)
}

func TestCheckBoundsInterruptPending(t *testing.T) {
	// Test CheckBounds when interrupt is pending.
	server, _ := createTestServer()
	ctx := context.Background()

	protoEnv := &pb.Envelope{
		EnvelopeId:       "env_test",
		InterruptPending: true, // Interrupt pending
		Iteration:        0,
		MaxIterations:    3,
		LlmCallCount:     0,
		MaxLlmCalls:      10,
		AgentHopCount:    0,
		MaxAgentHops:     21,
	}

	result, err := server.CheckBounds(ctx, protoEnv)

	require.NoError(t, err)
	assert.False(t, result.CanContinue)
}

// =============================================================================
// EXECUTE AGENT TESTS
// =============================================================================

func TestExecuteAgent(t *testing.T) {
	// Test ExecuteAgent RPC (placeholder implementation).
	server, logger := createTestServer()
	ctx := context.Background()

	protoEnv := &pb.Envelope{
		EnvelopeId: "env_test",
		RawInput:   "Test input",
	}

	req := &pb.ExecuteAgentRequest{
		Envelope:  protoEnv,
		AgentName: "test_agent",
	}

	result, err := server.ExecuteAgent(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, result)

	assert.True(t, result.Success)
	assert.NotNil(t, result.Envelope)
	assert.Contains(t, logger.debugCalls, "agent_execution_started")
}

// =============================================================================
// PROTO CONVERSION TESTS (Contract 11: Lossless round-trip)
// =============================================================================

func TestEnvelopeToProtoBasic(t *testing.T) {
	// Test basic envelope to proto conversion.
	env := envelope.CreateGenericEnvelope(
		"Hello, world!",
		"user_123",
		"session_456",
		nil, nil,
		[]string{"stage1", "stage2"},
	)
	env.Iteration = 1
	env.LLMCallCount = 3
	env.AgentHopCount = 2

	proto := envelopeToProto(env)

	assert.Equal(t, env.EnvelopeID, proto.EnvelopeId)
	assert.Equal(t, env.RequestID, proto.RequestId)
	assert.Equal(t, env.UserID, proto.UserId)
	assert.Equal(t, env.SessionID, proto.SessionId)
	assert.Equal(t, env.RawInput, proto.RawInput)
	assert.Equal(t, int32(env.Iteration), proto.Iteration)
	assert.Equal(t, int32(env.LLMCallCount), proto.LlmCallCount)
	assert.Equal(t, int32(env.AgentHopCount), proto.AgentHopCount)
	assert.Equal(t, env.StageOrder, proto.StageOrder)
}

func TestProtoToEnvelopeBasic(t *testing.T) {
	// Test basic proto to envelope conversion.
	proto := &pb.Envelope{
		EnvelopeId:    "env_test",
		RequestId:     "req_test",
		UserId:        "user_123",
		SessionId:     "session_456",
		RawInput:      "Hello, world!",
		CurrentStage:  "stage1",
		StageOrder:    []string{"stage1", "stage2"},
		Iteration:     1,
		MaxIterations: 3,
		LlmCallCount:  3,
		MaxLlmCalls:   10,
		AgentHopCount: 2,
		MaxAgentHops:  21,
		ReceivedAtMs:  time.Now().UnixMilli(),
	}

	env := protoToEnvelope(proto)

	assert.Equal(t, proto.EnvelopeId, env.EnvelopeID)
	assert.Equal(t, proto.RequestId, env.RequestID)
	assert.Equal(t, proto.UserId, env.UserID)
	assert.Equal(t, proto.SessionId, env.SessionID)
	assert.Equal(t, proto.RawInput, env.RawInput)
	assert.Equal(t, proto.CurrentStage, env.CurrentStage)
	assert.Equal(t, int(proto.Iteration), env.Iteration)
	assert.Equal(t, int(proto.LlmCallCount), env.LLMCallCount)
	assert.Equal(t, int(proto.AgentHopCount), env.AgentHopCount)
}

func TestEnvelopeRoundtrip(t *testing.T) {
	// Test envelope survives round-trip through proto (Contract 11).
	original := envelope.CreateGenericEnvelope(
		"Round-trip test",
		"user_123",
		"session_456",
		nil,
		map[string]any{"key": "value"},
		[]string{"stage1", "stage2", "stage3"},
	)
	original.Iteration = 2
	original.LLMCallCount = 5
	original.AgentHopCount = 3
	original.CurrentStage = "stage2"
	original.MaxIterations = 5
	original.MaxLLMCalls = 20
	original.MaxAgentHops = 30
	original.Outputs["test_agent"] = map[string]any{"result": "success"}

	// Convert to proto and back
	proto := envelopeToProto(original)
	restored := protoToEnvelope(proto)

	// Verify key fields preserved
	assert.Equal(t, original.EnvelopeID, restored.EnvelopeID)
	assert.Equal(t, original.RequestID, restored.RequestID)
	assert.Equal(t, original.UserID, restored.UserID)
	assert.Equal(t, original.SessionID, restored.SessionID)
	assert.Equal(t, original.RawInput, restored.RawInput)
	assert.Equal(t, original.CurrentStage, restored.CurrentStage)
	assert.Equal(t, original.Iteration, restored.Iteration)
	assert.Equal(t, original.LLMCallCount, restored.LLMCallCount)
	assert.Equal(t, original.AgentHopCount, restored.AgentHopCount)
	assert.Equal(t, original.MaxIterations, restored.MaxIterations)
	assert.Equal(t, original.MaxLLMCalls, restored.MaxLLMCalls)
	assert.Equal(t, original.MaxAgentHops, restored.MaxAgentHops)
	assert.Equal(t, original.StageOrder, restored.StageOrder)
}

func TestEnvelopeRoundtripWithInterrupt(t *testing.T) {
	// Test envelope with interrupt survives round-trip.
	original := envelope.CreateGenericEnvelope(
		"Interrupt test",
		"user_123",
		"session_456",
		nil, nil, nil,
	)
	original.SetInterrupt(envelope.InterruptKindClarification, "int_1",
		envelope.WithQuestion("What file?"),
		envelope.WithInterruptData(map[string]any{"context": "important"}))

	// Convert to proto and back
	proto := envelopeToProto(original)
	restored := protoToEnvelope(proto)

	// Verify interrupt preserved
	assert.True(t, restored.InterruptPending)
	require.NotNil(t, restored.Interrupt)
	assert.Equal(t, envelope.InterruptKindClarification, restored.Interrupt.Kind)
	assert.Equal(t, "int_1", restored.Interrupt.ID)
	assert.Equal(t, "What file?", restored.Interrupt.Question)
}

func TestEnvelopeRoundtripWithParallelState(t *testing.T) {
	// Test envelope with parallel execution state survives round-trip.
	original := envelope.CreateGenericEnvelope(
		"Parallel test",
		"user_123",
		"session_456",
		nil, nil, nil,
	)
	original.ActiveStages["stage1"] = true
	original.ActiveStages["stage2"] = true
	original.CompletedStageSet["stage0"] = true
	original.FailedStages["stageFail"] = "connection error"

	// Convert to proto and back
	proto := envelopeToProto(original)
	restored := protoToEnvelope(proto)

	// Verify parallel state preserved
	assert.True(t, restored.ActiveStages["stage1"])
	assert.True(t, restored.ActiveStages["stage2"])
	assert.True(t, restored.CompletedStageSet["stage0"])
	assert.Equal(t, "connection error", restored.FailedStages["stageFail"])
}

// =============================================================================
// TERMINAL REASON CONVERSION TESTS
// =============================================================================

func TestTerminalReasonToProto(t *testing.T) {
	// Test all terminal reason conversions to proto.
	testCases := []struct {
		goReason    envelope.TerminalReason
		protoReason pb.TerminalReason
	}{
		{envelope.TerminalReasonMaxIterationsExceeded, pb.TerminalReason_MAX_ITERATIONS_EXCEEDED},
		{envelope.TerminalReasonMaxLLMCallsExceeded, pb.TerminalReason_MAX_LLM_CALLS_EXCEEDED},
		{envelope.TerminalReasonMaxAgentHopsExceeded, pb.TerminalReason_MAX_AGENT_HOPS_EXCEEDED},
		{envelope.TerminalReasonUserCancelled, pb.TerminalReason_USER_CANCELLED},
		{envelope.TerminalReasonToolFailedFatally, pb.TerminalReason_TOOL_FAILED_FATALLY},
		{envelope.TerminalReasonLLMFailedFatally, pb.TerminalReason_LLM_FAILED_FATALLY},
		{envelope.TerminalReasonCompleted, pb.TerminalReason_COMPLETED},
	}

	for _, tc := range testCases {
		result := terminalReasonToProto(tc.goReason)
		assert.Equal(t, tc.protoReason, result, "Failed for %v", tc.goReason)
	}
}

func TestProtoToTerminalReason(t *testing.T) {
	// Test all proto terminal reason conversions to Go.
	testCases := []struct {
		protoReason pb.TerminalReason
		goReason    envelope.TerminalReason
	}{
		{pb.TerminalReason_MAX_ITERATIONS_EXCEEDED, envelope.TerminalReasonMaxIterationsExceeded},
		{pb.TerminalReason_MAX_LLM_CALLS_EXCEEDED, envelope.TerminalReasonMaxLLMCallsExceeded},
		{pb.TerminalReason_MAX_AGENT_HOPS_EXCEEDED, envelope.TerminalReasonMaxAgentHopsExceeded},
		{pb.TerminalReason_USER_CANCELLED, envelope.TerminalReasonUserCancelled},
		{pb.TerminalReason_TOOL_FAILED_FATALLY, envelope.TerminalReasonToolFailedFatally},
		{pb.TerminalReason_LLM_FAILED_FATALLY, envelope.TerminalReasonLLMFailedFatally},
		{pb.TerminalReason_COMPLETED, envelope.TerminalReasonCompleted},
	}

	for _, tc := range testCases {
		result := protoToTerminalReason(tc.protoReason)
		assert.Equal(t, tc.goReason, result, "Failed for %v", tc.protoReason)
	}
}

func TestTerminalReasonRoundtrip(t *testing.T) {
	// Test terminal reason survives round-trip.
	reasons := []envelope.TerminalReason{
		envelope.TerminalReasonMaxIterationsExceeded,
		envelope.TerminalReasonMaxLLMCallsExceeded,
		envelope.TerminalReasonMaxAgentHopsExceeded,
		envelope.TerminalReasonUserCancelled,
		envelope.TerminalReasonToolFailedFatally,
		envelope.TerminalReasonLLMFailedFatally,
		envelope.TerminalReasonCompleted,
	}

	for _, reason := range reasons {
		protoVal := terminalReasonToProto(reason)
		restored := protoToTerminalReason(protoVal)
		assert.Equal(t, reason, restored, "Roundtrip failed for %v", reason)
	}
}

// =============================================================================
// INTERRUPT KIND CONVERSION TESTS
// =============================================================================

func TestInterruptKindToProto(t *testing.T) {
	// Test all interrupt kind conversions to proto.
	testCases := []struct {
		goKind    envelope.InterruptKind
		protoKind pb.InterruptKind
	}{
		{envelope.InterruptKindClarification, pb.InterruptKind_CLARIFICATION},
		{envelope.InterruptKindConfirmation, pb.InterruptKind_CONFIRMATION},
		{envelope.InterruptKindCheckpoint, pb.InterruptKind_CHECKPOINT},
		{envelope.InterruptKindResourceExhausted, pb.InterruptKind_RESOURCE_EXHAUSTED},
		{envelope.InterruptKindTimeout, pb.InterruptKind_TIMEOUT},
		{envelope.InterruptKindSystemError, pb.InterruptKind_SYSTEM_ERROR},
		{envelope.InterruptKindAgentReview, pb.InterruptKind_AGENT_REVIEW},
	}

	for _, tc := range testCases {
		result := interruptKindToProto(tc.goKind)
		assert.Equal(t, tc.protoKind, result, "Failed for %v", tc.goKind)
	}
}

func TestProtoToInterruptKind(t *testing.T) {
	// Test all proto interrupt kind conversions to Go.
	testCases := []struct {
		protoKind pb.InterruptKind
		goKind    envelope.InterruptKind
	}{
		{pb.InterruptKind_CLARIFICATION, envelope.InterruptKindClarification},
		{pb.InterruptKind_CONFIRMATION, envelope.InterruptKindConfirmation},
		{pb.InterruptKind_CHECKPOINT, envelope.InterruptKindCheckpoint},
		{pb.InterruptKind_RESOURCE_EXHAUSTED, envelope.InterruptKindResourceExhausted},
		{pb.InterruptKind_TIMEOUT, envelope.InterruptKindTimeout},
		{pb.InterruptKind_SYSTEM_ERROR, envelope.InterruptKindSystemError},
		{pb.InterruptKind_AGENT_REVIEW, envelope.InterruptKindAgentReview},
	}

	for _, tc := range testCases {
		result := protoToInterruptKind(tc.protoKind)
		assert.Equal(t, tc.goKind, result, "Failed for %v", tc.protoKind)
	}
}

func TestInterruptKindRoundtrip(t *testing.T) {
	// Test interrupt kind survives round-trip.
	kinds := []envelope.InterruptKind{
		envelope.InterruptKindClarification,
		envelope.InterruptKindConfirmation,
		envelope.InterruptKindCheckpoint,
		envelope.InterruptKindResourceExhausted,
		envelope.InterruptKindTimeout,
		envelope.InterruptKindSystemError,
		envelope.InterruptKindAgentReview,
	}

	for _, kind := range kinds {
		protoVal := interruptKindToProto(kind)
		restored := protoToInterruptKind(protoVal)
		assert.Equal(t, kind, restored, "Roundtrip failed for %v", kind)
	}
}

// =============================================================================
// FLOW INTERRUPT CONVERSION TESTS
// =============================================================================

func TestFlowInterruptToProto(t *testing.T) {
	// Test flow interrupt conversion to proto.
	interrupt := &envelope.FlowInterrupt{
		Kind:      envelope.InterruptKindClarification,
		ID:        "int_123",
		Question:  "What file?",
		Message:   "",
		Data:      map[string]any{"context": "important"},
		CreatedAt: time.Now().UTC(),
	}

	proto := flowInterruptToProto(interrupt)

	assert.Equal(t, pb.InterruptKind_CLARIFICATION, proto.Kind)
	assert.Equal(t, "int_123", proto.InterruptId)
	assert.Equal(t, "What file?", proto.Question)
	assert.NotEmpty(t, proto.Data)
}

func TestFlowInterruptToProtoWithResponse(t *testing.T) {
	// Test flow interrupt with response conversion to proto.
	text := "main.go"
	approved := true
	interrupt := &envelope.FlowInterrupt{
		Kind:      envelope.InterruptKindClarification,
		ID:        "int_123",
		Question:  "What file?",
		CreatedAt: time.Now().UTC(),
		Response: &envelope.InterruptResponse{
			Text:       &text,
			Approved:   &approved,
			ReceivedAt: time.Now().UTC(),
		},
	}

	proto := flowInterruptToProto(interrupt)

	require.NotNil(t, proto.Response)
	assert.Equal(t, "main.go", proto.Response.Text)
	assert.True(t, proto.Response.Approved)
}

func TestProtoToFlowInterrupt(t *testing.T) {
	// Test proto to flow interrupt conversion.
	proto := &pb.FlowInterrupt{
		Kind:        pb.InterruptKind_CONFIRMATION,
		InterruptId: "conf_456",
		Question:    "",
		Message:     "Delete this file?",
		CreatedAtMs: time.Now().UnixMilli(),
	}

	interrupt := protoToFlowInterrupt(proto)

	assert.Equal(t, envelope.InterruptKindConfirmation, interrupt.Kind)
	assert.Equal(t, "conf_456", interrupt.ID)
	assert.Equal(t, "Delete this file?", interrupt.Message)
}

// =============================================================================
// SERVER LIFECYCLE TESTS
// =============================================================================

func TestNewJeevesCoreServer(t *testing.T) {
	// Test server creation.
	logger := &MockLogger{}
	server := NewJeevesCoreServer(logger)

	assert.NotNil(t, server)
	assert.Nil(t, server.getRuntime()) // Runtime not set yet
}

func TestSetRuntime(t *testing.T) {
	// Test setting runtime on server.
	server, _ := createTestServer()
	assert.Nil(t, server.getRuntime())

	// We can't easily create a real runtime in tests, but we can verify the method exists
	// and doesn't panic with nil
	server.SetRuntime(nil)
	assert.Nil(t, server.getRuntime())
}

// =============================================================================
// LOGGER INTERFACE TESTS
// =============================================================================

func TestServerImplementsLoggerInterface(t *testing.T) {
	// Test that server implements the logger interface for runtime.
	server, _ := createTestServer()

	// These should not panic
	server.Debug("test debug")
	server.Info("test info")
	server.Warn("test warn")
	server.Error("test error")

	logger := server.Bind("key", "value")
	assert.Equal(t, server, logger)
}

// =============================================================================
// THREAD SAFETY TESTS
// =============================================================================

func TestSetRuntimeThreadSafe(t *testing.T) {
	// Test that SetRuntime is thread-safe.
	server, _ := createTestServer()

	// Concurrent SetRuntime calls should not race
	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			server.SetRuntime(nil)
		}()
	}
	wg.Wait()
	// No panic = success
}

func TestGetRuntimeThreadSafe(t *testing.T) {
	// Test that getRuntime is thread-safe.
	server, _ := createTestServer()

	// Concurrent SetRuntime and getRuntime calls should not race
	var wg sync.WaitGroup
	for i := 0; i < 50; i++ {
		wg.Add(2)
		go func() {
			defer wg.Done()
			server.SetRuntime(nil)
		}()
		go func() {
			defer wg.Done()
			_ = server.getRuntime()
		}()
	}
	wg.Wait()
	// No panic = success
}
