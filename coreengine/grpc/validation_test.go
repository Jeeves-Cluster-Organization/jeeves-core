// Package grpc provides tests for the validation syscall boundary.
package grpc

import (
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"google.golang.org/grpc/codes"
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
