// Package commbus provides tests for message types.
package commbus

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// =============================================================================
// MESSAGE CATEGORY TESTS
// =============================================================================

// Event messages
func TestAgentStarted_Category(t *testing.T) {
	msg := &AgentStarted{}
	assert.Equal(t, "event", msg.Category())
}

func TestAgentCompleted_Category(t *testing.T) {
	msg := &AgentCompleted{}
	assert.Equal(t, "event", msg.Category())
}

func TestToolStarted_Category(t *testing.T) {
	msg := &ToolStarted{}
	assert.Equal(t, "event", msg.Category())
}

func TestToolCompleted_Category(t *testing.T) {
	msg := &ToolCompleted{}
	assert.Equal(t, "event", msg.Category())
}

func TestPipelineStarted_Category(t *testing.T) {
	msg := &PipelineStarted{}
	assert.Equal(t, "event", msg.Category())
}

func TestPipelineCompleted_Category(t *testing.T) {
	msg := &PipelineCompleted{}
	assert.Equal(t, "event", msg.Category())
}

func TestStageTransition_Category(t *testing.T) {
	msg := &StageTransition{}
	assert.Equal(t, "event", msg.Category())
}

func TestFrontendBroadcast_Category(t *testing.T) {
	msg := &FrontendBroadcast{}
	assert.Equal(t, "event", msg.Category())
}

func TestResponseChunk_Category(t *testing.T) {
	msg := &ResponseChunk{}
	assert.Equal(t, "event", msg.Category())
}

func TestInvalidateCache_Category(t *testing.T) {
	msg := &InvalidateCache{}
	assert.Equal(t, "command", msg.Category())
}

func TestGoalCompleted_Category(t *testing.T) {
	msg := &GoalCompleted{}
	assert.Equal(t, "event", msg.Category())
}

func TestClarificationRequested_Category(t *testing.T) {
	msg := &ClarificationRequested{}
	assert.Equal(t, "event", msg.Category())
}

// Query messages with IsQuery()
func TestGetAgentToolAccess_Category(t *testing.T) {
	msg := &GetAgentToolAccess{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery() // Call method for coverage
}

func TestGetInferenceEndpoint_Category(t *testing.T) {
	msg := &GetInferenceEndpoint{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery()
}

func TestGetLanguageConfig_Category(t *testing.T) {
	msg := &GetLanguageConfig{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery()
}

func TestGetSettings_Category(t *testing.T) {
	msg := &GetSettings{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery()
}

func TestHealthCheckRequest_Category(t *testing.T) {
	msg := &HealthCheckRequest{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery()
}

func TestGetToolCatalog_Category(t *testing.T) {
	msg := &GetToolCatalog{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery()
}

func TestGetToolEntry_Category(t *testing.T) {
	msg := &GetToolEntry{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery()
}

func TestGetPrompt_Category(t *testing.T) {
	msg := &GetPrompt{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery()
}

func TestListPrompts_Category(t *testing.T) {
	msg := &ListPrompts{}
	assert.Equal(t, "query", msg.Category())
	msg.IsQuery()
}

// =============================================================================
// MESSAGE TYPE HELPER TESTS
// =============================================================================

func TestGetMessageType_KnownTypes(t *testing.T) {
	tests := []struct {
		name     string
		msg      Message
		expected string
	}{
		{"AgentStarted", &AgentStarted{}, "AgentStarted"},
		{"AgentCompleted", &AgentCompleted{}, "AgentCompleted"},
		{"ToolStarted", &ToolStarted{}, "ToolStarted"},
		{"ToolCompleted", &ToolCompleted{}, "ToolCompleted"},
		{"PipelineStarted", &PipelineStarted{}, "PipelineStarted"},
		{"PipelineCompleted", &PipelineCompleted{}, "PipelineCompleted"},
		{"StageTransition", &StageTransition{}, "StageTransition"},
		{"GetAgentToolAccess", &GetAgentToolAccess{}, "GetAgentToolAccess"},
		{"HealthCheckRequest", &HealthCheckRequest{}, "HealthCheckRequest"},
		{"FrontendBroadcast", &FrontendBroadcast{}, "FrontendBroadcast"},
		{"ResponseChunk", &ResponseChunk{}, "ResponseChunk"},
		{"GetToolCatalog", &GetToolCatalog{}, "GetToolCatalog"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			msgType := GetMessageType(tt.msg)
			assert.Equal(t, tt.expected, msgType)
		})
	}
}

func TestGetMessageType_NilMessage(t *testing.T) {
	msgType := GetMessageType(nil)
	assert.Equal(t, "Unknown", msgType)
}
