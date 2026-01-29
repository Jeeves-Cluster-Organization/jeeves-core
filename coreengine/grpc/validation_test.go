// Package grpc provides tests for the validation syscall boundary.
package grpc

import (
	"context"
	"errors"
	"testing"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/kernel"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/metadata"
	"google.golang.org/grpc/status"
)

// =============================================================================
// VALIDATE REQUIRED TESTS
// =============================================================================

func TestValidateRequired_Success(t *testing.T) {
	err := validateRequired("valid-value", "field_name")
	assert.NoError(t, err)
}

func TestValidateRequired_EmptyString(t *testing.T) {
	err := validateRequired("", "field_name")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Contains(t, st.Message(), "field_name is required")
}

func TestValidateRequired_VariousFields(t *testing.T) {
	tests := []struct {
		name      string
		value     string
		fieldName string
		wantErr   bool
	}{
		{"valid pid", "proc-123", "pid", false},
		{"valid user_id", "user-456", "user_id", false},
		{"empty pid", "", "pid", true},
		{"empty event_type", "", "event_type", true},
		{"whitespace not empty", "  ", "field", false}, // Whitespace is considered non-empty
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := validateRequired(tt.value, tt.fieldName)
			if tt.wantErr {
				require.Error(t, err)
				st, ok := status.FromError(err)
				require.True(t, ok)
				assert.Equal(t, codes.InvalidArgument, st.Code())
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

// =============================================================================
// ERROR BUILDER TESTS
// =============================================================================

func TestInvalidArgument(t *testing.T) {
	err := InvalidArgument("pid")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())
	assert.Equal(t, "pid is required", st.Message())
}

func TestNotFound(t *testing.T) {
	err := NotFound("process", "proc-123")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.NotFound, st.Code())
	assert.Contains(t, st.Message(), "process not found")
	assert.Contains(t, st.Message(), "proc-123")
}

func TestInternal(t *testing.T) {
	cause := errors.New("database connection failed")
	err := Internal("save process", cause)

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Internal, st.Code())
	assert.Contains(t, st.Message(), "save process failed")
	assert.Contains(t, st.Message(), "database connection failed")
}

func TestFailedPrecondition(t *testing.T) {
	err := FailedPrecondition("process", "RUNNING", "be scheduled")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.FailedPrecondition, st.Code())
	assert.Contains(t, st.Message(), "process in state RUNNING cannot be scheduled")
}

func TestResourceExhausted(t *testing.T) {
	err := ResourceExhausted("LLM calls", "10/hour")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.ResourceExhausted, st.Code())
	assert.Contains(t, st.Message(), "LLM calls limit exceeded")
	assert.Contains(t, st.Message(), "10/hour")
}

func TestPermissionDenied(t *testing.T) {
	err := PermissionDenied("terminate process", "insufficient privileges")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.PermissionDenied, st.Code())
	assert.Contains(t, st.Message(), "terminate process denied")
	assert.Contains(t, st.Message(), "insufficient privileges")
}

// =============================================================================
// ERROR CODE CONSISTENCY TESTS
// =============================================================================

func TestErrorCodes_Consistency(t *testing.T) {
	// Test that all error builders return consistent gRPC status codes
	tests := []struct {
		name     string
		errFunc  func() error
		wantCode codes.Code
	}{
		{
			name:     "InvalidArgument",
			errFunc:  func() error { return InvalidArgument("test") },
			wantCode: codes.InvalidArgument,
		},
		{
			name:     "NotFound",
			errFunc:  func() error { return NotFound("resource", "id") },
			wantCode: codes.NotFound,
		},
		{
			name:     "Internal",
			errFunc:  func() error { return Internal("operation", errors.New("cause")) },
			wantCode: codes.Internal,
		},
		{
			name:     "FailedPrecondition",
			errFunc:  func() error { return FailedPrecondition("res", "state", "action") },
			wantCode: codes.FailedPrecondition,
		},
		{
			name:     "ResourceExhausted",
			errFunc:  func() error { return ResourceExhausted("resource", "limit") },
			wantCode: codes.ResourceExhausted,
		},
		{
			name:     "PermissionDenied",
			errFunc:  func() error { return PermissionDenied("operation", "reason") },
			wantCode: codes.PermissionDenied,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := tt.errFunc()
			st, ok := status.FromError(err)
			require.True(t, ok, "Error should be a gRPC status")
			assert.Equal(t, tt.wantCode, st.Code())
		})
	}
}

// =============================================================================
// REQUEST CONTEXT VALIDATION TESTS (Agentic Security)
// =============================================================================

func TestExtractRequestContext_FromCreateProcessRequest(t *testing.T) {
	req := &pb.CreateProcessRequest{
		Pid:       "test-pid",
		UserId:    "agent-1",
		SessionId: "sess-1",
		RequestId: "req-1",
	}

	ctx := context.Background()
	reqCtx, err := ExtractRequestContext(ctx, req)

	require.NoError(t, err)
	assert.Equal(t, "agent-1", reqCtx.UserID)
	assert.Equal(t, "sess-1", reqCtx.SessionID)
	assert.Equal(t, "req-1", reqCtx.RequestID)
}

func TestExtractRequestContext_FromMetadata(t *testing.T) {
	// Simulate gRPC metadata for GetProcess or other operations
	md := metadata.Pairs(
		"user_id", "agent-2",
		"session_id", "sess-2",
		"request_id", "req-2",
	)
	ctx := metadata.NewIncomingContext(context.Background(), md)

	req := &pb.GetProcessRequest{Pid: "test-pid"}
	reqCtx, err := ExtractRequestContext(ctx, req)

	require.NoError(t, err)
	assert.Equal(t, "agent-2", reqCtx.UserID)
	assert.Equal(t, "sess-2", reqCtx.SessionID)
	assert.Equal(t, "req-2", reqCtx.RequestID)
}

func TestExtractRequestContext_MissingUserID(t *testing.T) {
	md := metadata.Pairs(
		"session_id", "sess-1",
		"request_id", "req-1",
		// missing user_id
	)
	ctx := metadata.NewIncomingContext(context.Background(), md)

	req := &pb.GetProcessRequest{Pid: "test-pid"}
	reqCtx, err := ExtractRequestContext(ctx, req)

	require.Error(t, err)
	assert.Nil(t, reqCtx)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Unauthenticated, st.Code())
	assert.Contains(t, st.Message(), "user_id")
}

func TestExtractRequestContext_MissingSessionID(t *testing.T) {
	md := metadata.Pairs(
		"user_id", "agent-1",
		"request_id", "req-1",
		// missing session_id
	)
	ctx := metadata.NewIncomingContext(context.Background(), md)

	req := &pb.GetProcessRequest{Pid: "test-pid"}
	reqCtx, err := ExtractRequestContext(ctx, req)

	require.Error(t, err)
	assert.Nil(t, reqCtx)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Unauthenticated, st.Code())
	assert.Contains(t, st.Message(), "session_id")
}

func TestExtractRequestContext_NoMetadata(t *testing.T) {
	// No metadata attached to context
	ctx := context.Background()
	req := &pb.GetProcessRequest{Pid: "test-pid"}

	reqCtx, err := ExtractRequestContext(ctx, req)

	require.Error(t, err)
	assert.Nil(t, reqCtx)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Unauthenticated, st.Code())
}

// =============================================================================
// QUOTA PRE-FLIGHT VALIDATION TESTS (Cost Control)
// =============================================================================

func TestValidateQuotaAvailable_Success(t *testing.T) {
	logger := &MockLogger{}
	k := kernel.NewKernel(logger, nil)

	// Create process with quota
	quota := &kernel.ResourceQuota{
		MaxLLMCalls:  10,
		MaxToolCalls: 20,
	}
	k.Submit("test-pid", "req-1", "user-1", "sess-1", kernel.PriorityNormal, quota)

	// Should succeed - no usage yet
	err := ValidateQuotaAvailable(k, "test-pid")
	assert.NoError(t, err)
}

func TestValidateQuotaAvailable_LLMCallsExceeded(t *testing.T) {
	logger := &MockLogger{}
	k := kernel.NewKernel(logger, nil)

	// Create process with tight quota
	quota := &kernel.ResourceQuota{
		MaxLLMCalls: 1,
	}
	k.Submit("test-pid", "req-1", "user-1", "sess-1", kernel.PriorityNormal, quota)

	// Record usage that exceeds quota
	k.Resources().RecordUsage("test-pid", 2, 0, 0, 0, 0)

	err := ValidateQuotaAvailable(k, "test-pid")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.ResourceExhausted, st.Code())
	assert.Contains(t, st.Message(), "quota")
	assert.Contains(t, st.Message(), "llm") // lowercase to match actual error message
}

func TestValidateQuotaAvailable_ToolCallsExceeded(t *testing.T) {
	logger := &MockLogger{}
	k := kernel.NewKernel(logger, nil)

	quota := &kernel.ResourceQuota{
		MaxToolCalls: 5,
	}
	k.Submit("test-pid", "req-1", "user-1", "sess-1", kernel.PriorityNormal, quota)

	// Record tool call usage that exceeds quota
	k.Resources().RecordUsage("test-pid", 0, 10, 0, 0, 0) // 10 tool calls > 5 max

	err := ValidateQuotaAvailable(k, "test-pid")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.ResourceExhausted, st.Code())
	assert.Contains(t, st.Message(), "quota")
}

func TestValidateQuotaAvailable_ProcessNotFound(t *testing.T) {
	logger := &MockLogger{}
	k := kernel.NewKernel(logger, nil)

	// Process doesn't exist
	err := ValidateQuotaAvailable(k, "nonexistent-pid")

	// Should return empty string (no quota exceeded)
	// Quota check returns "" for non-existent processes
	assert.NoError(t, err)
}

// =============================================================================
// PROCESS OWNERSHIP VALIDATION TESTS (Multi-Tenant Isolation)
// =============================================================================

func TestValidateProcessOwnership_Success(t *testing.T) {
	pcb := &kernel.ProcessControlBlock{
		PID:    "test-pid",
		UserID: "agent-1",
	}

	err := ValidateProcessOwnership(pcb, "agent-1")
	assert.NoError(t, err)
}

func TestValidateProcessOwnership_WrongUser(t *testing.T) {
	pcb := &kernel.ProcessControlBlock{
		PID:    "test-pid",
		UserID: "agent-1",
	}

	err := ValidateProcessOwnership(pcb, "agent-2")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.PermissionDenied, st.Code())
	assert.Contains(t, st.Message(), "agent-1")
	assert.Contains(t, st.Message(), "agent-2")
	assert.Contains(t, st.Message(), "denied")
}

func TestValidateProcessOwnership_NilPCB(t *testing.T) {
	err := ValidateProcessOwnership(nil, "agent-1")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.NotFound, st.Code())
}

func TestValidateProcessOwnership_EmptyUserID(t *testing.T) {
	pcb := &kernel.ProcessControlBlock{
		PID:    "test-pid",
		UserID: "agent-1",
	}

	// Empty user ID should fail ownership check
	err := ValidateProcessOwnership(pcb, "")

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.PermissionDenied, st.Code())
}

// =============================================================================
// INTEGRATION TESTS
// =============================================================================

func TestValidation_ErrorPropagation(t *testing.T) {
	// Test that validation errors can be properly returned from RPC handlers
	err := validateRequired("", "pid")
	require.Error(t, err)

	// Should be usable directly as RPC error return
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.InvalidArgument, st.Code())

	// Can be checked by client code
	assert.Contains(t, err.Error(), "pid is required")
}

func TestValidation_MultipleFields(t *testing.T) {
	// Test validating multiple fields in sequence (short-circuit pattern)
	fields := []struct{ value, name string }{
		{"", "pid"},         // Should fail here
		{"", "request_id"},  // Won't be checked
	}

	for _, f := range fields {
		if err := validateRequired(f.value, f.name); err != nil {
			// First error stops validation
			assert.Contains(t, err.Error(), "pid is required")
			return
		}
	}

	t.Fatal("Should have failed validation")
}

func TestValidation_AgenticWorkflow(t *testing.T) {
	// Integration test: full agentic validation workflow
	logger := &MockLogger{}
	k := kernel.NewKernel(logger, nil)

	// 1. Agent creates process
	quota := &kernel.ResourceQuota{MaxLLMCalls: 5}
	k.Submit("proc-1", "req-1", "agent-a", "sess-a", kernel.PriorityNormal, quota)

	// 2. Check quota is available
	err := ValidateQuotaAvailable(k, "proc-1")
	assert.NoError(t, err)

	// 3. Verify agent-a can access their process
	pcb := k.GetProcess("proc-1")
	err = ValidateProcessOwnership(pcb, "agent-a")
	assert.NoError(t, err)

	// 4. Verify agent-b CANNOT access agent-a's process
	err = ValidateProcessOwnership(pcb, "agent-b")
	require.Error(t, err)
	assert.Equal(t, codes.PermissionDenied, status.Code(err))

	// 5. Record usage and verify quota check
	k.Resources().RecordUsage("proc-1", 6, 0, 0, 0, 0) // Exceed quota
	err = ValidateQuotaAvailable(k, "proc-1")
	require.Error(t, err)
	assert.Equal(t, codes.ResourceExhausted, status.Code(err))
}
