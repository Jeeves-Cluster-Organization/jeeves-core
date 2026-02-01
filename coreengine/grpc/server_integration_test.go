// Package grpc provides gRPC server integration tests.
//
// These tests start a real gRPC server and test client-server communication.
// No external services required - uses ephemeral localhost ports.
package grpc

import (
	"context"
	"net"
	"testing"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/status"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/runtime"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

// testServer holds a running gRPC server for testing.
type testServer struct {
	server     *grpc.Server
	coreServer *EngineServer
	listener   net.Listener
	address    string
	conn       *grpc.ClientConn
	client     pb.EngineServiceClient
}

// startTestServer starts a gRPC server on an ephemeral port.
func startTestServer(t *testing.T) *testServer {
	t.Helper()

	// Create listener on ephemeral port
	lis, err := net.Listen("tcp", "localhost:0")
	require.NoError(t, err)

	// Create core server
	logger := testutil.NewMockLogger()
	coreServer := NewEngineServer(logger)

	// Create and start gRPC server
	grpcServer := grpc.NewServer()
	pb.RegisterEngineServiceServer(grpcServer, coreServer)

	// Start serving in background
	go func() {
		if err := grpcServer.Serve(lis); err != nil {
			// Expected when server is stopped
		}
	}()

	// Create client connection
	address := lis.Addr().String()
	conn, err := grpc.NewClient(address,
		grpc.WithTransportCredentials(insecure.NewCredentials()),
	)
	require.NoError(t, err)

	client := pb.NewEngineServiceClient(conn)

	return &testServer{
		server:     grpcServer,
		coreServer: coreServer,
		listener:   lis,
		address:    address,
		conn:       conn,
		client:     client,
	}
}

// stop gracefully stops the test server.
func (ts *testServer) stop() {
	if ts.conn != nil {
		ts.conn.Close()
	}
	if ts.server != nil {
		ts.server.GracefulStop()
	}
}

// =============================================================================
// SERVER LIFECYCLE TESTS
// =============================================================================

func TestGRPCIntegration_ServerStartStop(t *testing.T) {
	// Test that server starts and stops cleanly.
	ts := startTestServer(t)
	defer ts.stop()

	assert.NotEmpty(t, ts.address)

	// Verify client can connect
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()

	// Simple health check - create envelope should work
	req := &pb.CreateEnvelopeRequest{
		RawInput:  "test",
		UserId:    "user",
		SessionId: "session",
	}
	resp, err := ts.client.CreateEnvelope(ctx, req)

	require.NoError(t, err)
	assert.NotNil(t, resp)
}

func TestGRPCIntegration_MultipleSequentialRequests(t *testing.T) {
	// Test multiple sequential requests to same server.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	for i := 0; i < 5; i++ {
		req := &pb.CreateEnvelopeRequest{
			RawInput:  "test",
			UserId:    "user",
			SessionId: "session",
		}
		resp, err := ts.client.CreateEnvelope(ctx, req)

		require.NoError(t, err)
		assert.NotEmpty(t, resp.EnvelopeId)
	}
}

// =============================================================================
// CREATE ENVELOPE TESTS
// =============================================================================

func TestGRPCIntegration_CreateEnvelope(t *testing.T) {
	// Test CreateEnvelope RPC over real network.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	req := &pb.CreateEnvelopeRequest{
		RawInput:   "Hello, gRPC!",
		UserId:     "user_grpc",
		SessionId:  "session_grpc",
		RequestId:  "req_grpc_123",
		Metadata:   map[string]string{"source": "integration_test"},
		StageOrder: []string{"intake", "process", "output"},
	}

	resp, err := ts.client.CreateEnvelope(ctx, req)

	require.NoError(t, err)
	assert.Equal(t, "Hello, gRPC!", resp.RawInput)
	assert.Equal(t, "user_grpc", resp.UserId)
	assert.Equal(t, "session_grpc", resp.SessionId)
	assert.Equal(t, "req_grpc_123", resp.RequestId)
	assert.NotEmpty(t, resp.EnvelopeId)
	assert.Equal(t, []string{"intake", "process", "output"}, resp.StageOrder)
}

func TestGRPCIntegration_CreateEnvelopeGeneratesIDs(t *testing.T) {
	// Test that missing IDs are generated.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	req := &pb.CreateEnvelopeRequest{
		RawInput:  "Minimal",
		UserId:    "user",
		SessionId: "session",
		// No RequestId
	}

	resp, err := ts.client.CreateEnvelope(ctx, req)

	require.NoError(t, err)
	assert.NotEmpty(t, resp.EnvelopeId)
	assert.NotEmpty(t, resp.RequestId)
}

// =============================================================================
// UPDATE ENVELOPE TESTS
// =============================================================================

func TestGRPCIntegration_UpdateEnvelope(t *testing.T) {
	// Test UpdateEnvelope RPC over real network.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	// First create an envelope
	createReq := &pb.CreateEnvelopeRequest{
		RawInput:  "Original",
		UserId:    "user",
		SessionId: "session",
	}
	created, err := ts.client.CreateEnvelope(ctx, createReq)
	require.NoError(t, err)

	// Modify and update
	created.LlmCallCount = 5
	created.AgentHopCount = 2
	created.CurrentStage = "processing"

	updateReq := &pb.UpdateEnvelopeRequest{
		Envelope: created,
	}

	updated, err := ts.client.UpdateEnvelope(ctx, updateReq)

	require.NoError(t, err)
	assert.Equal(t, int32(5), updated.LlmCallCount)
	assert.Equal(t, int32(2), updated.AgentHopCount)
	assert.Equal(t, "processing", updated.CurrentStage)
}

// =============================================================================
// CLONE ENVELOPE TESTS
// =============================================================================

func TestGRPCIntegration_CloneEnvelope(t *testing.T) {
	// Test CloneEnvelope RPC over real network.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	// Create original envelope
	createReq := &pb.CreateEnvelopeRequest{
		RawInput:  "Clone me",
		UserId:    "user",
		SessionId: "session",
	}
	original, err := ts.client.CreateEnvelope(ctx, createReq)
	require.NoError(t, err)

	original.LlmCallCount = 3
	original.CurrentStage = "analysis"

	// Clone it
	cloneReq := &pb.CloneRequest{
		Envelope: original,
	}

	cloned, err := ts.client.CloneEnvelope(ctx, cloneReq)

	require.NoError(t, err)
	assert.Equal(t, original.EnvelopeId, cloned.EnvelopeId)
	assert.Equal(t, original.RawInput, cloned.RawInput)
	assert.Equal(t, original.LlmCallCount, cloned.LlmCallCount)
}

// =============================================================================
// BOUNDS CHECKING TESTS
// =============================================================================

func TestGRPCIntegration_CheckBoundsWithinLimits(t *testing.T) {
	// Test CheckBounds when within limits.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	env := &pb.Envelope{
		EnvelopeId:    "bounds_test",
		Iteration:     0,
		MaxIterations: 5,
		LlmCallCount:  3,
		MaxLlmCalls:   10,
		AgentHopCount: 2,
		MaxAgentHops:  21,
	}

	result, err := ts.client.CheckBounds(ctx, env)

	require.NoError(t, err)
	assert.True(t, result.CanContinue)
	assert.Equal(t, int32(7), result.LlmCallsRemaining)  // 10 - 3
	assert.Equal(t, int32(19), result.AgentHopsRemaining) // 21 - 2
	assert.Equal(t, int32(5), result.IterationsRemaining) // 5 - 0
}

func TestGRPCIntegration_CheckBoundsExceeded(t *testing.T) {
	// Test CheckBounds when bounds exceeded.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	env := &pb.Envelope{
		EnvelopeId:    "bounds_exceeded",
		Iteration:     6, // Exceeds max
		MaxIterations: 5,
		LlmCallCount:  0,
		MaxLlmCalls:   10,
		AgentHopCount: 0,
		MaxAgentHops:  21,
	}

	result, err := ts.client.CheckBounds(ctx, env)

	require.NoError(t, err)
	assert.False(t, result.CanContinue)
	assert.Equal(t, pb.TerminalReason_MAX_ITERATIONS_EXCEEDED, result.TerminalReason)
}

func TestGRPCIntegration_CheckBoundsTerminated(t *testing.T) {
	// Test CheckBounds with terminated envelope.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	env := &pb.Envelope{
		EnvelopeId:    "terminated",
		Terminated:    true,
		Iteration:     0,
		MaxIterations: 5,
	}

	result, err := ts.client.CheckBounds(ctx, env)

	require.NoError(t, err)
	assert.False(t, result.CanContinue)
}

// =============================================================================
// EXECUTE AGENT TESTS
// =============================================================================

func TestGRPCIntegration_ExecuteAgent(t *testing.T) {
	// Test ExecuteAgent RPC.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	env := &pb.Envelope{
		EnvelopeId: "exec_agent",
		RawInput:   "Execute this",
	}

	req := &pb.ExecuteAgentRequest{
		Envelope:  env,
		AgentName: "test_agent",
	}

	result, err := ts.client.ExecuteAgent(ctx, req)

	require.NoError(t, err)
	assert.True(t, result.Success)
	assert.NotNil(t, result.Envelope)
}

// =============================================================================
// PIPELINE EXECUTION TESTS
// =============================================================================

func TestGRPCIntegration_ExecutePipelineWithRuntime(t *testing.T) {
	// Test ExecutePipeline with a configured runtime.
	ts := startTestServer(t)
	defer ts.stop()

	// Configure runtime
	cfg := &config.PipelineConfig{
		Name:          "grpc-pipeline",
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
		Agents:        []*config.AgentConfig{},
	}

	mockLLM := testutil.NewMockLLMProvider()
	llmFactory := func(role string) runtime.LLMProvider { return mockLLM }

	rt, err := runtime.NewPipelineRunner(cfg, llmFactory, nil, testutil.NewMockLogger())
	require.NoError(t, err)

	ts.coreServer.SetRunner(rt)

	ctx := context.Background()

	// Create envelope
	createReq := &pb.CreateEnvelopeRequest{
		RawInput:  "Pipeline test",
		UserId:    "user",
		SessionId: "session",
	}
	env, err := ts.client.CreateEnvelope(ctx, createReq)
	require.NoError(t, err)

	// Execute pipeline
	execReq := &pb.ExecuteRequest{
		Envelope: env,
		ThreadId: "thread_grpc",
	}

	stream, err := ts.client.ExecutePipeline(ctx, execReq)
	require.NoError(t, err)

	// Collect events
	var events []*pb.ExecutionEvent
	for {
		event, err := stream.Recv()
		if err != nil {
			break
		}
		events = append(events, event)
	}

	// Should have at least start and complete events
	assert.NotEmpty(t, events)
}

// =============================================================================
// PROTO CONVERSION ROUNDTRIP TESTS
// =============================================================================

func TestGRPCIntegration_EnvelopeRoundtrip(t *testing.T) {
	// Test that envelope survives full gRPC roundtrip.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	// Create with all fields
	createReq := &pb.CreateEnvelopeRequest{
		RawInput:   "Roundtrip test",
		UserId:     "user_rt",
		SessionId:  "session_rt",
		RequestId:  "req_rt",
		Metadata:   map[string]string{"key": "value"},
		StageOrder: []string{"a", "b", "c"},
	}

	created, err := ts.client.CreateEnvelope(ctx, createReq)
	require.NoError(t, err)

	// Modify envelope
	created.Iteration = 2
	created.LlmCallCount = 5
	created.AgentHopCount = 3
	created.CurrentStage = "b"

	// Update
	updateReq := &pb.UpdateEnvelopeRequest{Envelope: created}
	updated, err := ts.client.UpdateEnvelope(ctx, updateReq)
	require.NoError(t, err)

	// Clone
	cloneReq := &pb.CloneRequest{Envelope: updated}
	cloned, err := ts.client.CloneEnvelope(ctx, cloneReq)
	require.NoError(t, err)

	// Verify all fields preserved
	assert.Equal(t, created.EnvelopeId, cloned.EnvelopeId)
	assert.Equal(t, "Roundtrip test", cloned.RawInput)
	assert.Equal(t, "user_rt", cloned.UserId)
	assert.Equal(t, int32(2), cloned.Iteration)
	assert.Equal(t, int32(5), cloned.LlmCallCount)
	assert.Equal(t, int32(3), cloned.AgentHopCount)
	assert.Equal(t, "b", cloned.CurrentStage)
	assert.Equal(t, []string{"a", "b", "c"}, cloned.StageOrder)
}

func TestGRPCIntegration_InterruptRoundtrip(t *testing.T) {
	// Test that interrupt state survives roundtrip.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	// Create envelope with interrupt
	createReq := &pb.CreateEnvelopeRequest{
		RawInput:  "Interrupt test",
		UserId:    "user",
		SessionId: "session",
	}

	created, err := ts.client.CreateEnvelope(ctx, createReq)
	require.NoError(t, err)

	// Set interrupt
	created.InterruptPending = true
	created.Interrupt = &pb.FlowInterrupt{
		Kind:        pb.InterruptKind_CLARIFICATION,
		Id: "int_123",
		Question:    "What file?",
		CreatedAtMs: time.Now().UnixMilli(),
	}

	// Update
	updateReq := &pb.UpdateEnvelopeRequest{Envelope: created}
	updated, err := ts.client.UpdateEnvelope(ctx, updateReq)
	require.NoError(t, err)

	// Verify interrupt preserved
	assert.True(t, updated.InterruptPending)
	assert.NotNil(t, updated.Interrupt)
	assert.Equal(t, pb.InterruptKind_CLARIFICATION, updated.Interrupt.Kind)
	assert.Equal(t, "int_123", updated.Interrupt.Id)
	assert.Equal(t, "What file?", updated.Interrupt.Question)
}

// =============================================================================
// CONCURRENT CLIENT TESTS
// =============================================================================

func TestGRPCIntegration_ConcurrentClients(t *testing.T) {
	// Test multiple concurrent clients.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()
	numClients := 10
	done := make(chan bool, numClients)

	for i := 0; i < numClients; i++ {
		go func(clientID int) {
			req := &pb.CreateEnvelopeRequest{
				RawInput:  "Concurrent test",
				UserId:    "user",
				SessionId: "session",
			}

			resp, err := ts.client.CreateEnvelope(ctx, req)
			if err == nil && resp != nil && resp.EnvelopeId != "" {
				done <- true
			} else {
				done <- false
			}
		}(i)
	}

	// Wait for all clients
	successCount := 0
	for i := 0; i < numClients; i++ {
		if <-done {
			successCount++
		}
	}

	assert.Equal(t, numClients, successCount)
}

// =============================================================================
// TIMEOUT TESTS
// =============================================================================

func TestGRPCIntegration_ClientTimeout(t *testing.T) {
	// Test that client respects timeout.
	ts := startTestServer(t)
	defer ts.stop()

	// Use a reasonable timeout and wait for it to expire
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Millisecond)
	defer cancel()

	// Wait for context to be cancelled (deadline exceeded)
	<-ctx.Done()

	req := &pb.CreateEnvelopeRequest{
		RawInput:  "Timeout test",
		UserId:    "user",
		SessionId: "session",
	}

	_, err := ts.client.CreateEnvelope(ctx, req)

	// Should get DeadlineExceeded error
	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok, "expected gRPC status error")
	assert.Equal(t, codes.DeadlineExceeded, st.Code(), "expected DeadlineExceeded code")
}

// =============================================================================
// TERMINAL REASON TESTS
// =============================================================================

func TestGRPCIntegration_AllTerminalReasons(t *testing.T) {
	// Test all terminal reasons are correctly transmitted.
	ts := startTestServer(t)
	defer ts.stop()

	ctx := context.Background()

	testCases := []struct {
		name   string
		setup  func(*pb.Envelope)
		reason pb.TerminalReason
	}{
		{
			name: "max_iterations",
			setup: func(e *pb.Envelope) {
				e.Iteration = 6
				e.MaxIterations = 5
			},
			reason: pb.TerminalReason_MAX_ITERATIONS_EXCEEDED,
		},
		{
			name: "max_llm_calls",
			setup: func(e *pb.Envelope) {
				e.LlmCallCount = 10
				e.MaxLlmCalls = 10
				e.MaxIterations = 100 // Ensure this doesn't trigger
			},
			reason: pb.TerminalReason_MAX_LLM_CALLS_EXCEEDED,
		},
		{
			name: "max_agent_hops",
			setup: func(e *pb.Envelope) {
				e.AgentHopCount = 21
				e.MaxAgentHops = 21
				e.MaxIterations = 100
				e.MaxLlmCalls = 100
			},
			reason: pb.TerminalReason_MAX_AGENT_HOPS_EXCEEDED,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			env := &pb.Envelope{
				EnvelopeId: tc.name,
			}
			tc.setup(env)

			result, err := ts.client.CheckBounds(ctx, env)

			require.NoError(t, err)
			assert.False(t, result.CanContinue)
			assert.Equal(t, tc.reason, result.TerminalReason)
		})
	}
}

// =============================================================================
// LOCAL TO GO ENVELOPE CONVERSION
// =============================================================================

func TestGRPCIntegration_GoEnvelopeInterop(t *testing.T) {
	// Test that Go envelope converts correctly through proto.

	// Create Go envelope directly
	goEnv := envelope.CreateEnvelope(
		"Go interop test",
		"user_go",
		"session_go",
		nil,
		map[string]any{"go": "true"},
		[]string{"a", "b"},
	)
	goEnv.Iteration = 1
	goEnv.LLMCallCount = 2
	goEnv.AgentHopCount = 1

	// Convert to proto
	protoEnv := envelopeToProto(goEnv)

	// Verify conversion
	assert.Equal(t, goEnv.EnvelopeID, protoEnv.EnvelopeId)
	assert.Equal(t, goEnv.RawInput, protoEnv.RawInput)
	assert.Equal(t, int32(goEnv.Iteration), protoEnv.Iteration)
	assert.Equal(t, int32(goEnv.LLMCallCount), protoEnv.LlmCallCount)

	// Convert back
	restored := protoToEnvelope(protoEnv)

	// Verify roundtrip
	assert.Equal(t, goEnv.EnvelopeID, restored.EnvelopeID)
	assert.Equal(t, goEnv.RawInput, restored.RawInput)
	assert.Equal(t, goEnv.UserID, restored.UserID)
	assert.Equal(t, goEnv.Iteration, restored.Iteration)
	assert.Equal(t, goEnv.LLMCallCount, restored.LLMCallCount)
}
