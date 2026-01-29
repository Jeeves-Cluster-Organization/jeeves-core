// Package commbus provides CommBus Message Definitions.
//
// This module defines all message types for the Jeeves Communication Bus.
// Messages are organized by domain.
//
// Categories:
//   - EVENT: Fire-and-forget, fan-out to subscribers
//   - QUERY: Request-response, single handler
//   - COMMAND: Fire-and-forget, single handler
//
// Constitutional Reference:
//   - Core Engine R3: Immutable State Transitions
//   - Mission System R1: Evidence Chain (events trace to source)
package commbus

// =============================================================================
// MESSAGE CATEGORIES
// =============================================================================

// MessageCategory represents message routing categories.
type MessageCategory string

const (
	// MessageCategoryEvent represents fire-and-forget, fan-out to all subscribers.
	MessageCategoryEvent MessageCategory = "event"
	// MessageCategoryQuery represents request-response, single handler.
	MessageCategoryQuery MessageCategory = "query"
	// MessageCategoryCommand represents fire-and-forget, single handler.
	MessageCategoryCommand MessageCategory = "command"
)

// HealthStatus represents canonical health status values.
type HealthStatus string

const (
	HealthStatusHealthy   HealthStatus = "healthy"
	HealthStatusDegraded  HealthStatus = "degraded"
	HealthStatusUnhealthy HealthStatus = "unhealthy"
	HealthStatusUnknown   HealthStatus = "unknown"
)

// =============================================================================
// AGENT LIFECYCLE EVENTS
// =============================================================================

// AgentStarted is emitted when an agent begins processing.
// Subscribers: telemetry, WebSocket broadcast, trace logging.
type AgentStarted struct {
	AgentName  string         `json:"agent_name"`
	SessionID  string         `json:"session_id"`
	RequestID  string         `json:"request_id"`
	EnvelopeID string         `json:"envelope_id"`
	Payload    map[string]any `json:"payload,omitempty"`
}

// Category implements the Message interface.
func (m *AgentStarted) Category() string { return string(MessageCategoryEvent) }

// AgentCompleted is emitted when an agent finishes processing.
// Subscribers: telemetry, WebSocket broadcast, trace logging.
type AgentCompleted struct {
	AgentName  string         `json:"agent_name"`
	SessionID  string         `json:"session_id"`
	RequestID  string         `json:"request_id"`
	EnvelopeID string         `json:"envelope_id"`
	Status     string         `json:"status"` // "success", "error", "skipped"
	DurationMS int            `json:"duration_ms"`
	Error      *string        `json:"error,omitempty"`
	Payload    map[string]any `json:"payload,omitempty"`
}

// Category implements the Message interface.
func (m *AgentCompleted) Category() string { return string(MessageCategoryEvent) }

// =============================================================================
// TOOL EXECUTION EVENTS
// =============================================================================

// ToolStarted is emitted when a tool execution begins.
// Subscribers: telemetry, WebSocket progress, governance.
type ToolStarted struct {
	ToolName      string            `json:"tool_name"`
	SessionID     string            `json:"session_id"`
	RequestID     string            `json:"request_id"`
	StepNumber    int               `json:"step_number"`
	TotalSteps    int               `json:"total_steps"`
	ParamsPreview map[string]string `json:"params_preview,omitempty"`
}

// Category implements the Message interface.
func (m *ToolStarted) Category() string { return string(MessageCategoryEvent) }

// ToolCompleted is emitted when tool execution finishes.
// Subscribers: telemetry, WebSocket progress, governance.
type ToolCompleted struct {
	ToolName        string  `json:"tool_name"`
	SessionID       string  `json:"session_id"`
	RequestID       string  `json:"request_id"`
	Status          string  `json:"status"` // "success", "error", "timeout"
	ExecutionTimeMS int     `json:"execution_time_ms"`
	Error           *string `json:"error,omitempty"`
	ErrorType       *string `json:"error_type,omitempty"`
}

// Category implements the Message interface.
func (m *ToolCompleted) Category() string { return string(MessageCategoryEvent) }

// =============================================================================
// PIPELINE LIFECYCLE EVENTS
// =============================================================================

// PipelineStarted is emitted when a new pipeline run starts.
type PipelineStarted struct {
	SessionID  string `json:"session_id"`
	RequestID  string `json:"request_id"`
	EnvelopeID string `json:"envelope_id"`
	Query      string `json:"query"`
	UserID     string `json:"user_id"`
}

// Category implements the Message interface.
func (m *PipelineStarted) Category() string { return string(MessageCategoryEvent) }

// PipelineCompleted is emitted when pipeline completes (success or failure).
type PipelineCompleted struct {
	SessionID       string  `json:"session_id"`
	RequestID       string  `json:"request_id"`
	EnvelopeID      string  `json:"envelope_id"`
	Status          string  `json:"status"` // "completed", "error", "cancelled"
	DurationMS      int     `json:"duration_ms"`
	StagesCompleted int     `json:"stages_completed"`
	Error           *string `json:"error,omitempty"`
}

// Category implements the Message interface.
func (m *PipelineCompleted) Category() string { return string(MessageCategoryEvent) }

// StageTransition is emitted when pipeline moves to a new stage.
type StageTransition struct {
	SessionID   string `json:"session_id"`
	RequestID   string `json:"request_id"`
	EnvelopeID  string `json:"envelope_id"`
	FromStage   string `json:"from_stage"`
	ToStage     string `json:"to_stage"`
	StageNumber int    `json:"stage_number"`
}

// Category implements the Message interface.
func (m *StageTransition) Category() string { return string(MessageCategoryEvent) }

// =============================================================================
// CONFIG QUERIES
// =============================================================================

// GetAgentToolAccess queries which tools an agent can access.
type GetAgentToolAccess struct {
	AgentName string `json:"agent_name"`
}

// Category implements the Message interface.
func (m *GetAgentToolAccess) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *GetAgentToolAccess) IsQuery() {}

// AgentToolAccessResponse is the response for GetAgentToolAccess query.
type AgentToolAccessResponse struct {
	AllowedTools []string `json:"allowed_tools"`
	DeniedTools  []string `json:"denied_tools,omitempty"`
}

// GetInferenceEndpoint queries LLM configuration for an agent.
type GetInferenceEndpoint struct {
	AgentName string `json:"agent_name"`
}

// Category implements the Message interface.
func (m *GetInferenceEndpoint) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *GetInferenceEndpoint) IsQuery() {}

// InferenceEndpointResponse is the response for GetInferenceEndpoint query.
type InferenceEndpointResponse struct {
	Model        string   `json:"model"`
	Temperature  *float64 `json:"temperature,omitempty"`
	MaxTokens    *int     `json:"max_tokens,omitempty"`
	SystemPrompt *string  `json:"system_prompt,omitempty"`
}

// GetLanguageConfig queries language configuration.
type GetLanguageConfig struct {
	Language string `json:"language"`
}

// Category implements the Message interface.
func (m *GetLanguageConfig) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *GetLanguageConfig) IsQuery() {}

// LanguageConfigResponse is the response for GetLanguageConfig query.
type LanguageConfigResponse struct {
	Language        string            `json:"language"`
	Extensions      []string          `json:"extensions"`
	CommentPatterns map[string]string `json:"comment_patterns"`
	ImportPatterns  []string          `json:"import_patterns"`
}

// GetSettings queries application settings.
type GetSettings struct {
	Key *string `json:"key,omitempty"` // nil = get all settings
}

// Category implements the Message interface.
func (m *GetSettings) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *GetSettings) IsQuery() {}

// SettingsResponse is the response for GetSettings query.
type SettingsResponse struct {
	Values map[string]any `json:"values"`
}

// =============================================================================
// HEALTH CHECK EVENTS
// =============================================================================

// HealthCheckRequest requests health check from a component.
type HealthCheckRequest struct {
	Component string `json:"component"` // "llm", "database", "tools", "memory"
}

// Category implements the Message interface.
func (m *HealthCheckRequest) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *HealthCheckRequest) IsQuery() {}

// HealthCheckResponse is the response for HealthCheckRequest.
type HealthCheckResponse struct {
	Component string         `json:"component"`
	Status    string         `json:"status"` // "healthy", "degraded", "unhealthy"
	Details   map[string]any `json:"details,omitempty"`
	LatencyMS *int           `json:"latency_ms,omitempty"`
}

// =============================================================================
// WEBSOCKET BROADCAST EVENTS
// =============================================================================

// FrontendBroadcast is a broadcast message to frontend clients.
// Unified WebSocket broadcast for all frontend updates.
type FrontendBroadcast struct {
	SessionID string         `json:"session_id"`
	EventType string         `json:"event_type"` // "agent_update", "tool_progress", "response_chunk"
	Payload   map[string]any `json:"payload,omitempty"`
}

// Category implements the Message interface.
func (m *FrontendBroadcast) Category() string { return string(MessageCategoryEvent) }

// ResponseChunk is a streaming response chunk.
type ResponseChunk struct {
	SessionID string `json:"session_id"`
	RequestID string `json:"request_id"`
	Content   string `json:"content"`
	IsFinal   bool   `json:"is_final"`
}

// Category implements the Message interface.
func (m *ResponseChunk) Category() string { return string(MessageCategoryEvent) }

// =============================================================================
// CACHE COMMANDS
// =============================================================================

// InvalidateCache is a command to invalidate cache entries.
type InvalidateCache struct {
	CacheName string  `json:"cache_name"`
	Key       *string `json:"key,omitempty"` // nil = invalidate all
}

// Category implements the Message interface.
func (m *InvalidateCache) Category() string { return string(MessageCategoryCommand) }

// =============================================================================
// TOOL CATALOG QUERIES
// =============================================================================

// GetToolCatalog queries tool catalog information.
type GetToolCatalog struct {
	ToolIDs         []string `json:"tool_ids,omitempty"` // nil = get all exposed tools
	IncludeInternal bool     `json:"include_internal"`
}

// Category implements the Message interface.
func (m *GetToolCatalog) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *GetToolCatalog) IsQuery() {}

// ToolCatalogResponse is the response for GetToolCatalog query.
type ToolCatalogResponse struct {
	Tools         []map[string]any `json:"tools"`
	PlannerPrompt string           `json:"planner_prompt"`
}

// GetToolEntry queries a single tool entry.
type GetToolEntry struct {
	ToolID string `json:"tool_id"`
}

// Category implements the Message interface.
func (m *GetToolEntry) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *GetToolEntry) IsQuery() {}

// ToolEntryResponse is the response for GetToolEntry query.
type ToolEntryResponse struct {
	Found bool            `json:"found"`
	Entry map[string]any `json:"entry,omitempty"`
}

// =============================================================================
// PROMPT REGISTRY QUERIES
// =============================================================================

// GetPrompt queries a prompt template.
type GetPrompt struct {
	Name    string         `json:"name"`
	Version string         `json:"version"`
	Context map[string]any `json:"context,omitempty"`
}

// Category implements the Message interface.
func (m *GetPrompt) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *GetPrompt) IsQuery() {}

// PromptResponse is the response for GetPrompt query.
type PromptResponse struct {
	Name     string `json:"name"`
	Version  string `json:"version"`
	Template string `json:"template"`
	Found    bool   `json:"found"`
}

// ListPrompts queries available prompts.
type ListPrompts struct{}

// Category implements the Message interface.
func (m *ListPrompts) Category() string { return string(MessageCategoryQuery) }

// IsQuery implements the Query interface.
func (m *ListPrompts) IsQuery() {}

// ListPromptsResponse is the response for ListPrompts query.
type ListPromptsResponse struct {
	Prompts map[string][]string `json:"prompts"` // name -> list of versions
}

// =============================================================================
// DOMAIN EVENTS (Code Analysis Specific)
// =============================================================================

// GoalCompleted is emitted when a goal was completed during analysis.
type GoalCompleted struct {
	SessionID string   `json:"session_id"`
	RequestID string   `json:"request_id"`
	GoalID    string   `json:"goal_id"`
	GoalText  string   `json:"goal_text"`
	Evidence  []string `json:"evidence,omitempty"`
}

// Category implements the Message interface.
func (m *GoalCompleted) Category() string { return string(MessageCategoryEvent) }

// ClarificationRequested is emitted when clarification is needed from user.
type ClarificationRequested struct {
	SessionID string   `json:"session_id"`
	RequestID string   `json:"request_id"`
	Question  string   `json:"question"`
	Options   []string `json:"options,omitempty"`
}

// Category implements the Message interface.
func (m *ClarificationRequested) Category() string { return string(MessageCategoryEvent) }

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

// TypedMessage is an optional interface for messages that can provide their own type name.
// This is useful for dynamically-typed messages like those from gRPC.
type TypedMessage interface {
	Message
	MessageType() string
}

// GetMessageType returns the type name of a message for routing.
func GetMessageType(msg Message) string {
	// First check if the message can provide its own type
	if typed, ok := msg.(TypedMessage); ok {
		return typed.MessageType()
	}

	// Otherwise use the static type switch
	switch msg.(type) {
	case *AgentStarted:
		return "AgentStarted"
	case *AgentCompleted:
		return "AgentCompleted"
	case *ToolStarted:
		return "ToolStarted"
	case *ToolCompleted:
		return "ToolCompleted"
	case *PipelineStarted:
		return "PipelineStarted"
	case *PipelineCompleted:
		return "PipelineCompleted"
	case *StageTransition:
		return "StageTransition"
	case *GetAgentToolAccess:
		return "GetAgentToolAccess"
	case *GetInferenceEndpoint:
		return "GetInferenceEndpoint"
	case *GetLanguageConfig:
		return "GetLanguageConfig"
	case *GetSettings:
		return "GetSettings"
	case *HealthCheckRequest:
		return "HealthCheckRequest"
	case *FrontendBroadcast:
		return "FrontendBroadcast"
	case *ResponseChunk:
		return "ResponseChunk"
	case *InvalidateCache:
		return "InvalidateCache"
	case *GetToolCatalog:
		return "GetToolCatalog"
	case *GetToolEntry:
		return "GetToolEntry"
	case *GetPrompt:
		return "GetPrompt"
	case *ListPrompts:
		return "ListPrompts"
	case *GoalCompleted:
		return "GoalCompleted"
	case *ClarificationRequested:
		return "ClarificationRequested"
	default:
		return "Unknown"
	}
}
