package agents

import (
	"errors"
	"testing"

	"github.com/jeeves-cluster-organization/codeanalysis/commbus"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TOOL STATUS TESTS
// =============================================================================

func TestValidStatusSuccess(t *testing.T) {
	// SUCCESS status should be valid.
	assert.Equal(t, "success", string(ToolStatusSuccess))

	status, err := ToolStatusFromString("success")
	require.NoError(t, err)
	assert.Equal(t, ToolStatusSuccess, status)
}

func TestValidStatusError(t *testing.T) {
	// ERROR status should be valid.
	assert.Equal(t, "error", string(ToolStatusError))

	status, err := ToolStatusFromString("error")
	require.NoError(t, err)
	assert.Equal(t, ToolStatusError, status)
}

func TestCaseInsensitiveParsing(t *testing.T) {
	// Status parsing should be case-insensitive.
	status1, err1 := ToolStatusFromString("SUCCESS")
	require.NoError(t, err1)
	assert.Equal(t, ToolStatusSuccess, status1)

	status2, err2 := ToolStatusFromString("Error")
	require.NoError(t, err2)
	assert.Equal(t, ToolStatusError, status2)

	status3, err3 := ToolStatusFromString("  success  ")
	require.NoError(t, err3)
	assert.Equal(t, ToolStatusSuccess, status3)
}

func TestInvalidStatusRaises(t *testing.T) {
	// Invalid status should raise error with helpful message.
	_, err := ToolStatusFromString("ok")
	require.Error(t, err)
	assert.Contains(t, err.Error(), "invalid tool status 'ok'")
	assert.Contains(t, err.Error(), "success")
	assert.Contains(t, err.Error(), "error")
}

// =============================================================================
// AGENT OUTCOME TESTS
// =============================================================================

func TestAgentOutcomeIsTerminal(t *testing.T) {
	assert.True(t, AgentOutcomeError.IsTerminal())
	assert.True(t, AgentOutcomeClarify.IsTerminal())
	assert.True(t, AgentOutcomeConfirm.IsTerminal())
	assert.True(t, AgentOutcomeTerminate.IsTerminal())

	assert.False(t, AgentOutcomeSuccess.IsTerminal())
	assert.False(t, AgentOutcomeReplan.IsTerminal())
}

func TestAgentOutcomeIsSuccess(t *testing.T) {
	assert.True(t, AgentOutcomeSuccess.IsSuccess())
	assert.True(t, AgentOutcomePartial.IsSuccess())
	assert.True(t, AgentOutcomeSkip.IsSuccess())

	assert.False(t, AgentOutcomeError.IsSuccess())
	assert.False(t, AgentOutcomeReplan.IsSuccess())
}

func TestAgentOutcomeRequiresLoop(t *testing.T) {
	assert.True(t, AgentOutcomeReplan.RequiresLoop())
	assert.True(t, AgentOutcomeLoopBack.RequiresLoop())

	assert.False(t, AgentOutcomeSuccess.RequiresLoop())
	assert.False(t, AgentOutcomeError.RequiresLoop())
}

// =============================================================================
// RISK LEVEL TESTS
// =============================================================================

func TestRiskLevelsExist(t *testing.T) {
	// All risk levels should be defined.
	assert.Equal(t, "read_only", string(commbus.RiskLevelReadOnly))
	assert.Equal(t, "write", string(commbus.RiskLevelWrite))
	assert.Equal(t, "destructive", string(commbus.RiskLevelDestructive))
}

func TestDestructiveRequiresConfirmation(t *testing.T) {
	// Only DESTRUCTIVE risk level requires confirmation.
	assert.True(t, commbus.RiskLevelDestructive.RequiresConfirmation())
	assert.False(t, commbus.RiskLevelWrite.RequiresConfirmation())
	assert.False(t, commbus.RiskLevelReadOnly.RequiresConfirmation())
}

// =============================================================================
// IDEMPOTENCY CLASS TESTS
// =============================================================================

func TestIdempotencyClassFromRiskLevel(t *testing.T) {
	assert.Equal(t, IdempotencyClassSafe, IdempotencyClassFromRiskLevel(commbus.RiskLevelReadOnly))
	assert.Equal(t, IdempotencyClassNonIdempotent, IdempotencyClassFromRiskLevel(commbus.RiskLevelWrite))
	assert.Equal(t, IdempotencyClassIdempotent, IdempotencyClassFromRiskLevel(commbus.RiskLevelDestructive))
}

// =============================================================================
// ERROR DETAILS TESTS
// =============================================================================

func TestBasicErrorCreation(t *testing.T) {
	// Basic error should have required fields.
	error := NewToolErrorDetails("ValueError", "Task not found", true)

	assert.Equal(t, "ValueError", error.ErrorType)
	assert.Equal(t, "Task not found", error.Message)
	assert.True(t, error.Recoverable)
	assert.Nil(t, error.Details)
}

func TestFromException(t *testing.T) {
	// Should create error from Go error.
	err := errors.New("test error")
	errorDetails := ToolErrorDetailsFromError(err, true)

	assert.Equal(t, "*errors.errorString", errorDetails.ErrorType)
	assert.Equal(t, "test error", errorDetails.Message)
	assert.Contains(t, errorDetails.Details, "traceback")
	assert.True(t, errorDetails.Recoverable)
}

func TestNotFoundFactory(t *testing.T) {
	// NotFound factory should create proper error.
	error := ToolErrorDetailsNotFound("Task", "task-123")

	assert.Equal(t, "NotFoundError", error.ErrorType)
	assert.Contains(t, error.Message, "task-123")
	assert.False(t, error.Recoverable)
}

// =============================================================================
// RESULT TESTS
// =============================================================================

func TestSuccessResult(t *testing.T) {
	// Success result should have data, no error.
	msg := "Task created"
	result := NewStandardToolResultSuccess(
		map[string]any{"task_id": "123", "title": "Test"},
		&msg,
	)

	assert.Equal(t, ToolStatusSuccess, result.Status)
	assert.Equal(t, "123", result.Data["task_id"])
	assert.Nil(t, result.Error)
	assert.Equal(t, "Task created", *result.Message)
}

func TestFailureResult(t *testing.T) {
	// Failure result should have error, no data.
	errorDetails := ToolErrorDetailsNotFound("Task", "456")
	result := NewStandardToolResultFailure(errorDetails, nil)

	assert.Equal(t, ToolStatusError, result.Status)
	assert.Equal(t, "NotFoundError", result.Error.ErrorType)
	assert.Nil(t, result.Data)
}

func TestErrorStatusRequiresErrorField(t *testing.T) {
	// ERROR status without error field should fail validation.
	result := &StandardToolResult{
		Status: ToolStatusError,
		Data:   map[string]any{"something": "wrong"},
	}
	err := result.Validate()
	require.Error(t, err)
	assert.Contains(t, err.Error(), "error field is required")
}

func TestSuccessStatusRejectsErrorField(t *testing.T) {
	// SUCCESS status with error field should fail validation.
	result := &StandardToolResult{
		Status: ToolStatusSuccess,
		Data:   map[string]any{"task": "created"},
		Error:  &ToolErrorDetails{ErrorType: "Bug", Message: "This shouldn't be here"},
	}
	err := result.Validate()
	require.Error(t, err)
	assert.Contains(t, err.Error(), "error field must be None")
}

// =============================================================================
// NORMALIZE TESTS
// =============================================================================

func TestPassthroughStandardResult(t *testing.T) {
	// StandardToolResult should pass through unchanged.
	original := NewStandardToolResultSuccess(map[string]any{"x": 1}, nil)
	normalized, err := NormalizeToolResult(original)

	require.NoError(t, err)
	assert.Equal(t, original, normalized)
}

func TestNormalizeSuccessDict(t *testing.T) {
	// Legacy success dict should normalize.
	legacy := map[string]any{
		"status":   "success",
		"task_id":  "123",
		"title":    "Buy milk",
		"message":  "Task added",
	}
	result, err := NormalizeToolResult(legacy)

	require.NoError(t, err)
	assert.Equal(t, ToolStatusSuccess, result.Status)
	assert.Equal(t, "123", result.Data["task_id"])
	assert.Equal(t, "Task added", *result.Message)
}

func TestNormalizeErrorDict(t *testing.T) {
	// Legacy error dict should normalize.
	legacy := map[string]any{
		"status":  "error",
		"error":   "Task not found",
		"message": "Could not find task",
	}
	result, err := NormalizeToolResult(legacy)

	require.NoError(t, err)
	assert.Equal(t, ToolStatusError, result.Status)
	require.NotNil(t, result.Error)
	assert.Contains(t, result.Error.Message, "not found")
}

func TestNormalizeHeuristicErrorDetection(t *testing.T) {
	// Should detect errors even without explicit status.
	legacy := map[string]any{
		"error": "Database connection failed",
	}
	result, err := NormalizeToolResult(legacy)

	require.NoError(t, err)
	assert.Equal(t, ToolStatusError, result.Status)
	assert.Contains(t, result.Error.Message, "Database")
}

func TestNormalizeCompletedStatus(t *testing.T) {
	// 'completed' status should normalize to SUCCESS.
	legacy := map[string]any{"status": "completed", "result": "done"}
	result, err := NormalizeToolResult(legacy)

	require.NoError(t, err)
	assert.Equal(t, ToolStatusSuccess, result.Status)
}

func TestNormalizeStructuredError(t *testing.T) {
	// Structured error dict should preserve details.
	legacy := map[string]any{
		"status": "error",
		"error": map[string]any{
			"type":    "DatabaseError",
			"message": "Connection timeout",
			"code":    500,
		},
	}
	result, err := NormalizeToolResult(legacy)

	require.NoError(t, err)
	assert.Equal(t, ToolStatusError, result.Status)
	assert.Equal(t, "DatabaseError", result.Error.ErrorType)
	assert.Contains(t, result.Error.Message, "timeout")
}

func TestInvalidTypeRaises(t *testing.T) {
	// Non-map, non-StandardToolResult should raise.
	_, err1 := NormalizeToolResult("just a string")
	require.Error(t, err1)

	_, err2 := NormalizeToolResult(123)
	require.Error(t, err2)
}

// =============================================================================
// DATA STRUCTURES TESTS
// =============================================================================

func TestToolResultDataFromDict(t *testing.T) {
	data := map[string]any{
		"items_explored": []any{"file1.go", "file2.go"},
		"items_pending":  []any{"file3.go"},
		"discoveries": []any{
			map[string]any{"identifier": "func1", "item_type": "function"},
		},
		"evidence": []any{
			map[string]any{"location": "file1.go:10", "content": "code"},
		},
	}

	result := NewToolResultData(data)

	assert.Equal(t, []string{"file1.go", "file2.go"}, result.ItemsExplored)
	assert.Equal(t, []string{"file3.go"}, result.ItemsPending)
	assert.Len(t, result.Discoveries, 1)
	assert.Equal(t, "func1", result.Discoveries[0].Identifier)
	assert.Len(t, result.Evidence, 1)
	assert.Equal(t, "file1.go:10", result.Evidence[0].Location)
}
