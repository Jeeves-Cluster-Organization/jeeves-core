package grpc

import (
	"context"
	"testing"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/kernel"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

func createKernelTestServer() (*KernelServer, *MockLogger, *kernel.Kernel) {
	// Use testutil helper but keep local name for compatibility
	return CreateTestKernelServer()
}

// =============================================================================
// CONSTRUCTOR TESTS
// =============================================================================

func TestNewKernelServer(t *testing.T) {
	logger := &MockLogger{}
	k := kernel.NewKernel(logger, nil)

	server := NewKernelServer(logger, k)

	require.NotNil(t, server)
	assert.Equal(t, logger, server.logger)
	assert.Equal(t, k, server.kernel)
}

// =============================================================================
// CREATE PROCESS TESTS
// =============================================================================

func TestKernelServer_CreateProcess_Success(t *testing.T) {
	server, logger, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.CreateProcessRequest{
		Pid:       "test-process-1",
		RequestId: "req-1",
		UserId:    "user-1",
		SessionId: "session-1",
		Priority:  pb.SchedulingPriority_SCHEDULING_PRIORITY_NORMAL,
		Quota: &pb.ResourceQuota{
			MaxLlmCalls:    10,
			MaxToolCalls:   20,
			TimeoutSeconds: 3600,
		},
	}

	resp, err := server.CreateProcess(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "test-process-1", resp.Pid)
	assert.Equal(t, "req-1", resp.RequestId)
	assert.Equal(t, "user-1", resp.UserId)
	assert.Equal(t, "session-1", resp.SessionId)
	// Process is created in NEW state, then auto-scheduled to READY
	assert.NotEqual(t, pb.ProcessState_PROCESS_STATE_UNSPECIFIED, resp.State)
	assert.Contains(t, logger.debugCalls, "process_created")
}

func TestKernelServer_CreateProcess_EmptyPid(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.CreateProcessRequest{
		Pid:       "",
		RequestId: "req-1",
		UserId:    "user-1",
		SessionId: "session-1",
	}

	resp, err := server.CreateProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "pid is required")
}

func TestKernelServer_CreateProcess_WithNilQuota(t *testing.T) {
	server, logger, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.CreateProcessRequest{
		Pid:       "test-process-2",
		RequestId: "req-2",
		UserId:    "user-2",
		SessionId: "session-2",
		Quota:     nil, // Nil quota should use defaults
	}

	resp, err := server.CreateProcess(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "test-process-2", resp.Pid)
	assert.Contains(t, logger.debugCalls, "process_created")
}

// =============================================================================
// GET PROCESS TESTS
// =============================================================================

func TestKernelServer_GetProcess_Success(t *testing.T) {
	server, _, k := createKernelTestServer()

	// Create a process first
	_, err := k.Submit("test-pid", "req-1", "user-1", "session-1", kernel.PriorityNormal, nil)
	require.NoError(t, err)

	// Use context with metadata matching the process owner
	ctx := ContextWithUserMetadata("user-1", "session-1", "req-2")

	req := &pb.GetProcessRequest{
		Pid: "test-pid",
	}

	resp, err := server.GetProcess(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "test-pid", resp.Pid)
}

func TestKernelServer_GetProcess_EmptyPid(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.GetProcessRequest{
		Pid: "",
	}

	resp, err := server.GetProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "pid is required")
}

func TestKernelServer_GetProcess_NotFound(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := ContextWithUserMetadata("user-1", "session-1", "req-1")

	req := &pb.GetProcessRequest{
		Pid: "non-existent-pid",
	}

	resp, err := server.GetProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "process not found")
}

// =============================================================================
// SCHEDULE PROCESS TESTS
// =============================================================================

func TestKernelServer_ScheduleProcess_Success(t *testing.T) {
	server, _, k := createKernelTestServer()

	// Create process in NEW state
	_, err := k.Submit("test-pid", "req-1", "user-1", "session-1", kernel.PriorityNormal, nil)
	require.NoError(t, err)

	// Use context with metadata matching the process owner
	ctx := ContextWithUserMetadata("user-1", "session-1", "req-2")

	req := &pb.ScheduleProcessRequest{
		Pid: "test-pid",
	}

	resp, err := server.ScheduleProcess(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "test-pid", resp.Pid)
}

func TestKernelServer_ScheduleProcess_EmptyPid(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.ScheduleProcessRequest{
		Pid: "",
	}

	resp, err := server.ScheduleProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "pid is required")
}

// =============================================================================
// GET NEXT RUNNABLE TESTS
// =============================================================================

func TestKernelServer_GetNextRunnable_Success(t *testing.T) {
	server, _, k := createKernelTestServer()
	ctx := context.Background()

	// Create and schedule a high-priority process
	_, err := k.Submit("high-priority-pid", "req-1", "user-1", "session-1", kernel.PriorityHigh, nil)
	require.NoError(t, err)
	err = k.Schedule("high-priority-pid")
	require.NoError(t, err)

	req := &pb.GetNextRunnableRequest{}

	resp, err := server.GetNextRunnable(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "high-priority-pid", resp.Pid)
}

func TestKernelServer_GetNextRunnable_EmptyQueue(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.GetNextRunnableRequest{}

	resp, err := server.GetNextRunnable(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "no runnable process")
}

// =============================================================================
// TRANSITION STATE TESTS
// =============================================================================

func TestKernelServer_TransitionState_Success(t *testing.T) {
	server, _, k := createKernelTestServer()

	// Create and schedule process
	_, err := k.Submit("test-pid", "req-1", "user-1", "session-1", kernel.PriorityNormal, nil)
	require.NoError(t, err)
	err = k.Schedule("test-pid")
	require.NoError(t, err)

	// Use context with metadata matching the process owner
	ctx := ContextWithUserMetadata("user-1", "session-1", "req-2")

	req := &pb.TransitionStateRequest{
		Pid:      "test-pid",
		NewState: pb.ProcessState_PROCESS_STATE_RUNNING,
	}

	resp, err := server.TransitionState(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "test-pid", resp.Pid)
	assert.Equal(t, pb.ProcessState_PROCESS_STATE_RUNNING, resp.State)
}

func TestKernelServer_TransitionState_EmptyPid(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.TransitionStateRequest{
		Pid:      "",
		NewState: pb.ProcessState_PROCESS_STATE_RUNNING,
	}

	resp, err := server.TransitionState(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "pid is required")
}

// =============================================================================
// TERMINATE PROCESS TESTS
// =============================================================================

func TestKernelServer_TerminateProcess_Success(t *testing.T) {
	server, _, k := createKernelTestServer()

	// Create process
	_, err := k.Submit("test-pid", "req-1", "user-1", "session-1", kernel.PriorityNormal, nil)
	require.NoError(t, err)

	// Use context with metadata matching the process owner
	ctx := ContextWithUserMetadata("user-1", "session-1", "req-2")

	req := &pb.TerminateProcessRequest{
		Pid:    "test-pid",
		Reason: "success",
	}

	resp, err := server.TerminateProcess(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, "test-pid", resp.Pid)
}

func TestKernelServer_TerminateProcess_EmptyPid(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.TerminateProcessRequest{
		Pid:    "",
		Reason: "success",
	}

	resp, err := server.TerminateProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "pid is required")
}

// =============================================================================
// CHECK QUOTA TESTS
// =============================================================================

func TestKernelServer_CheckQuota_Success(t *testing.T) {
	server, _, k := createKernelTestServer()
	ctx := context.Background()

	// Create process with quota
	quota := &kernel.ResourceQuota{
		MaxLLMCalls:    2,
		TimeoutSeconds: 3600,
	}
	_, err := k.Submit("test-pid", "req-1", "user-1", "session-1", kernel.PriorityNormal, quota)
	require.NoError(t, err)

	req := &pb.CheckQuotaRequest{
		Pid: "test-pid",
	}

	resp, err := server.CheckQuota(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.WithinBounds) // Within quota
	assert.Empty(t, resp.ExceededReason)
}

func TestKernelServer_CheckQuota_EmptyPid(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.CheckQuotaRequest{
		Pid: "",
	}

	resp, err := server.CheckQuota(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "pid is required")
}

// =============================================================================
// RECORD USAGE TESTS
// =============================================================================

func TestKernelServer_RecordUsage_Success(t *testing.T) {
	server, _, k := createKernelTestServer()
	ctx := context.Background()

	// Create process
	_, err := k.Submit("test-pid", "req-1", "user-1", "session-1", kernel.PriorityNormal, nil)
	require.NoError(t, err)

	req := &pb.RecordUsageRequest{
		Pid:       "test-pid",
		LlmCalls:  1,
		ToolCalls: 2,
		TokensIn:  100,
		TokensOut: 50,
	}

	resp, err := server.RecordUsage(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.Equal(t, int32(1), resp.LlmCalls)
	assert.Equal(t, int32(2), resp.ToolCalls)
	assert.Equal(t, int32(100), resp.TokensIn)
	assert.Equal(t, int32(50), resp.TokensOut)
}

func TestKernelServer_RecordUsage_EmptyPid(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.RecordUsageRequest{
		Pid:       "",
		LlmCalls:  1,
		ToolCalls: 2,
	}

	resp, err := server.RecordUsage(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "pid is required")
}

// =============================================================================
// CHECK RATE LIMIT TESTS
// =============================================================================

func TestKernelServer_CheckRateLimit_Success(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.CheckRateLimitRequest{
		UserId: "user-1",
	}

	resp, err := server.CheckRateLimit(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.Allowed) // Should be allowed initially
	assert.False(t, resp.Exceeded) // Should not be exceeded
}

func TestKernelServer_CheckRateLimit_SpecificUser(t *testing.T) {
	server, _, _ := createKernelTestServer()
	ctx := context.Background()

	req := &pb.CheckRateLimitRequest{
		UserId: "specific-user",
	}

	resp, err := server.CheckRateLimit(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.Allowed)
}

// =============================================================================
// LIST PROCESSES TESTS
// =============================================================================

func TestKernelServer_ListProcesses_Success(t *testing.T) {
	server, _, k := createKernelTestServer()
	ctx := context.Background()

	// Create multiple processes
	_, _ = k.Submit("pid-1", "req-1", "user-1", "session-1", kernel.PriorityNormal, nil)
	_, _ = k.Submit("pid-2", "req-2", "user-1", "session-1", kernel.PriorityHigh, nil)

	req := &pb.ListProcessesRequest{
		UserId: "user-1",
	}

	resp, err := server.ListProcesses(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.GreaterOrEqual(t, len(resp.Processes), 2)
}

func TestKernelServer_ListProcesses_ByState(t *testing.T) {
	server, _, k := createKernelTestServer()
	ctx := context.Background()

	// Create processes
	_, _ = k.Submit("pid-1", "req-1", "user-1", "session-1", kernel.PriorityNormal, nil)
	err := k.Schedule("pid-1")
	require.NoError(t, err)

	req := &pb.ListProcessesRequest{
		State: pb.ProcessState_PROCESS_STATE_READY,
	}

	resp, err := server.ListProcesses(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	// Should only return READY processes
	for _, pcb := range resp.Processes {
		assert.Equal(t, pb.ProcessState_PROCESS_STATE_READY, pcb.State)
	}
}

// =============================================================================
// GET PROCESS COUNTS TESTS
// =============================================================================

func TestKernelServer_GetProcessCounts_Success(t *testing.T) {
	server, _, k := createKernelTestServer()
	ctx := context.Background()

	// Create some processes
	_, _ = k.Submit("pid-1", "req-1", "user-1", "session-1", kernel.PriorityNormal, nil)
	_, _ = k.Submit("pid-2", "req-2", "user-2", "session-2", kernel.PriorityHigh, nil)

	req := &pb.GetProcessCountsRequest{}

	resp, err := server.GetProcessCounts(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.GreaterOrEqual(t, resp.Total, int32(2))
	assert.NotNil(t, resp.CountsByState)
}

// =============================================================================
// PROTO CONVERSION TESTS
// =============================================================================

func TestResourceQuotaConversion(t *testing.T) {
	tests := []struct {
		name       string
		protoQuota *pb.ResourceQuota
	}{
		{
			name: "full quota",
			protoQuota: &pb.ResourceQuota{
				MaxLlmCalls:      10,
				MaxToolCalls:     20,
				MaxAgentHops:     5,
				MaxIterations:    100,
				TimeoutSeconds:   3600,
				MaxInputTokens:   1000,
				MaxOutputTokens:  500,
				MaxContextTokens: 2000,
			},
		},
		{
			name:       "nil quota",
			protoQuota: nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Convert proto -> kernel
			kernelQuota := protoToResourceQuota(tt.protoQuota)

			if tt.protoQuota == nil {
				assert.Nil(t, kernelQuota)
			} else {
				// Convert kernel -> proto
				protoQuota := resourceQuotaToProto(kernelQuota)
				require.NotNil(t, kernelQuota)
				require.NotNil(t, protoQuota)
				assert.Equal(t, tt.protoQuota.MaxLlmCalls, protoQuota.MaxLlmCalls)
				assert.Equal(t, tt.protoQuota.MaxToolCalls, protoQuota.MaxToolCalls)
			}
		})
	}
}

func TestSchedulingPriorityConversion(t *testing.T) {
	tests := []struct {
		name          string
		protoPriority pb.SchedulingPriority
		kernelPriority kernel.SchedulingPriority
	}{
		{"low", pb.SchedulingPriority_SCHEDULING_PRIORITY_LOW, kernel.PriorityLow},
		{"normal", pb.SchedulingPriority_SCHEDULING_PRIORITY_NORMAL, kernel.PriorityNormal},
		{"high", pb.SchedulingPriority_SCHEDULING_PRIORITY_HIGH, kernel.PriorityHigh},
		{"realtime", pb.SchedulingPriority_SCHEDULING_PRIORITY_REALTIME, kernel.PriorityRealtime},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Proto -> Kernel
			kernelPri := protoToSchedulingPriority(tt.protoPriority)
			assert.Equal(t, tt.kernelPriority, kernelPri)

			// Kernel -> Proto
			protoPri := schedulingPriorityToProto(tt.kernelPriority)
			assert.Equal(t, tt.protoPriority, protoPri)
		})
	}
}

func TestProcessStateConversion(t *testing.T) {
	tests := []struct {
		name        string
		protoState  pb.ProcessState
		kernelState kernel.ProcessState
	}{
		{"new", pb.ProcessState_PROCESS_STATE_NEW, kernel.ProcessStateNew},
		{"ready", pb.ProcessState_PROCESS_STATE_READY, kernel.ProcessStateReady},
		{"running", pb.ProcessState_PROCESS_STATE_RUNNING, kernel.ProcessStateRunning},
		{"blocked", pb.ProcessState_PROCESS_STATE_BLOCKED, kernel.ProcessStateBlocked},
		{"terminated", pb.ProcessState_PROCESS_STATE_TERMINATED, kernel.ProcessStateTerminated},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Proto -> Kernel
			kernelState := protoToProcessState(tt.protoState)
			assert.Equal(t, tt.kernelState, kernelState)

			// Kernel -> Proto
			protoState := processStateToProto(tt.kernelState)
			assert.Equal(t, tt.protoState, protoState)
		})
	}
}

// =============================================================================
// SECURITY TESTS (Agentic Validation)
// =============================================================================

func TestKernelServer_GetProcess_OwnershipViolation(t *testing.T) {
	server, logger, k := createKernelTestServer()

	// Agent A creates a process
	_, err := k.Submit("proc-1", "req-1", "agent-a", "sess-a", kernel.PriorityNormal, nil)
	require.NoError(t, err)

	// Agent B tries to access agent A's process
	ctx := ContextWithUserMetadata("agent-b", "sess-b", "req-2")

	req := &pb.GetProcessRequest{
		Pid: "proc-1",
	}

	resp, err := server.GetProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "denied")
	assert.Contains(t, err.Error(), "agent-a") // Owner
	assert.Contains(t, err.Error(), "agent-b") // Requester
	assert.Contains(t, logger.warnCalls, "ownership_violation")
}

func TestKernelServer_GetProcess_MissingAuthentication(t *testing.T) {
	server, _, k := createKernelTestServer()

	// Create a process
	_, err := k.Submit("proc-1", "req-1", "agent-a", "sess-a", kernel.PriorityNormal, nil)
	require.NoError(t, err)

	// Try to access without metadata (no authentication)
	ctx := context.Background()

	req := &pb.GetProcessRequest{
		Pid: "proc-1",
	}

	resp, err := server.GetProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "missing request metadata")
}

func TestKernelServer_ScheduleProcess_OwnershipViolation(t *testing.T) {
	server, logger, k := createKernelTestServer()

	// Agent A creates a process
	_, err := k.Submit("proc-1", "req-1", "agent-a", "sess-a", kernel.PriorityNormal, nil)
	require.NoError(t, err)

	// Agent B tries to schedule agent A's process
	ctx := ContextWithUserMetadata("agent-b", "sess-b", "req-2")

	req := &pb.ScheduleProcessRequest{
		Pid: "proc-1",
	}

	resp, err := server.ScheduleProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "denied")
	assert.Contains(t, logger.warnCalls, "ownership_violation")
}

func TestKernelServer_ScheduleProcess_QuotaExhausted(t *testing.T) {
	server, logger, k := createKernelTestServer()

	// Create process with tight quota
	quota := &kernel.ResourceQuota{
		MaxLLMCalls: 1,
	}
	_, err := k.Submit("proc-1", "req-1", "agent-a", "sess-a", kernel.PriorityNormal, quota)
	require.NoError(t, err)

	// Exhaust quota
	k.Resources().RecordUsage("proc-1", 2, 0, 0, 0, 0)

	// Try to schedule process with exhausted quota
	ctx := ContextWithUserMetadata("agent-a", "sess-a", "req-2")

	req := &pb.ScheduleProcessRequest{
		Pid: "proc-1",
	}

	resp, err := server.ScheduleProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "quota")
	assert.Contains(t, logger.warnCalls, "quota_preflight_failed")
}

func TestKernelServer_TransitionState_OwnershipViolation(t *testing.T) {
	server, logger, k := createKernelTestServer()

	// Agent A creates and schedules a process
	_, err := k.Submit("proc-1", "req-1", "agent-a", "sess-a", kernel.PriorityNormal, nil)
	require.NoError(t, err)
	err = k.Schedule("proc-1")
	require.NoError(t, err)

	// Agent B tries to transition agent A's process
	ctx := ContextWithUserMetadata("agent-b", "sess-b", "req-2")

	req := &pb.TransitionStateRequest{
		Pid:      "proc-1",
		NewState: pb.ProcessState_PROCESS_STATE_RUNNING,
	}

	resp, err := server.TransitionState(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "denied")
	assert.Contains(t, logger.warnCalls, "ownership_violation")
}

func TestKernelServer_TerminateProcess_OwnershipViolation(t *testing.T) {
	server, logger, k := createKernelTestServer()

	// Agent A creates a process
	_, err := k.Submit("proc-1", "req-1", "agent-a", "sess-a", kernel.PriorityNormal, nil)
	require.NoError(t, err)

	// Agent B tries to terminate agent A's process
	ctx := ContextWithUserMetadata("agent-b", "sess-b", "req-2")

	req := &pb.TerminateProcessRequest{
		Pid:    "proc-1",
		Reason: "unauthorized termination attempt",
	}

	resp, err := server.TerminateProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "denied")
	assert.Contains(t, logger.warnCalls, "ownership_violation")
}

func TestKernelServer_CreateProcess_InvalidContext(t *testing.T) {
	server, logger, _ := createKernelTestServer()

	// Empty user_id in request
	ctx := context.Background()

	req := &pb.CreateProcessRequest{
		Pid:       "proc-1",
		RequestId: "req-1",
		UserId:    "", // Empty!
		SessionId: "sess-1",
		Priority:  pb.SchedulingPriority_SCHEDULING_PRIORITY_NORMAL,
	}

	resp, err := server.CreateProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "user_id")
	assert.Contains(t, logger.warnCalls, "invalid_request_context")
}

func TestKernelServer_ScheduleProcess_ProcessNotFound(t *testing.T) {
	server, _, _ := createKernelTestServer()

	// Try to schedule non-existent process
	ctx := ContextWithUserMetadata("agent-a", "sess-a", "req-1")

	req := &pb.ScheduleProcessRequest{
		Pid: "non-existent",
	}

	resp, err := server.ScheduleProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "process not found")
}

func TestKernelServer_TransitionState_ProcessNotFound(t *testing.T) {
	server, _, _ := createKernelTestServer()

	// Try to transition non-existent process
	ctx := ContextWithUserMetadata("agent-a", "sess-a", "req-1")

	req := &pb.TransitionStateRequest{
		Pid:      "non-existent",
		NewState: pb.ProcessState_PROCESS_STATE_RUNNING,
	}

	resp, err := server.TransitionState(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "process not found")
}

func TestKernelServer_TerminateProcess_ProcessNotFound(t *testing.T) {
	server, _, _ := createKernelTestServer()

	// Try to terminate non-existent process
	ctx := ContextWithUserMetadata("agent-a", "sess-a", "req-1")

	req := &pb.TerminateProcessRequest{
		Pid:    "non-existent",
		Reason: "test",
	}

	resp, err := server.TerminateProcess(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "process not found")
}
