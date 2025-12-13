// Package commbus provides the communication bus protocols and implementation.
//
// This module defines the CANONICAL protocols for the entire Jeeves system.
// All components depend on these protocols, not implementations.
//
// Protocol Categories:
//   - CommBus Protocols: Message, Query, CommBus, Middleware
//   - App Context Protocols: Settings, FeatureFlags, AppContext
//   - Infrastructure Protocols: Persistence, LLMProvider
//   - Tool Protocols: ToolRegistry, NLIService
//
// Constitutional Reference: Core Engine R2 (Protocol-First Design)
package commbus

import (
	"context"
)

// =============================================================================
// CANONICAL ENUMS
// =============================================================================

// RiskLevel represents the canonical risk level for tool operations.
//
// Constitution Amendment VIII: Every tool must declare its risk level.
// This is used by Critic and Arbiter for validation.
type RiskLevel string

const (
	// RiskLevelReadOnly indicates no state mutation (default, safe).
	RiskLevelReadOnly RiskLevel = "read_only"
	// RiskLevelWrite indicates state mutation, may require confirmation.
	RiskLevelWrite RiskLevel = "write"
	// RiskLevelDestructive indicates irreversible operations (always requires confirmation).
	RiskLevelDestructive RiskLevel = "destructive"
)

// RequiresConfirmation checks if risk level requires user confirmation.
func (r RiskLevel) RequiresConfirmation() bool {
	return r == RiskLevelDestructive
}

// ToolCategory represents the canonical tool category for organization and access control.
type ToolCategory string

const (
	// ToolCategoryUnified represents single entry point tools (Amendment XXII).
	ToolCategoryUnified ToolCategory = "unified"
	// ToolCategoryComposite represents multi-step workflows with fallbacks.
	ToolCategoryComposite ToolCategory = "composite"
	// ToolCategoryResilient represents wrapped base tools with retry/fallback.
	ToolCategoryResilient ToolCategory = "resilient"
	// ToolCategoryStandalone represents simple tools that don't need wrapping.
	ToolCategoryStandalone ToolCategory = "standalone"
	// ToolCategoryInternal represents implementation details, not exposed to agents.
	ToolCategoryInternal ToolCategory = "internal"
)

// =============================================================================
// COMMBUS PROTOCOLS
// =============================================================================

// Message is the protocol for all commbus messages.
// All messages (events, queries, commands) must have a category.
type Message interface {
	// Category returns the message category: "event", "query", or "command".
	Category() string
}

// Query is the protocol for query messages that expect a response.
// Queries are request-response: send query, get response.
type Query interface {
	Message
	// IsQuery is a marker method to distinguish queries from other messages.
	IsQuery()
}

// Handler is the protocol for message handlers.
// Handlers process messages and optionally return responses (for queries).
type Handler interface {
	// Handle processes a message and returns a response for queries.
	Handle(ctx context.Context, message Message) (any, error)
}

// HandlerFunc is a function type that implements Handler.
type HandlerFunc func(ctx context.Context, message Message) (any, error)

// Handle implements the Handler interface.
func (f HandlerFunc) Handle(ctx context.Context, message Message) (any, error) {
	return f(ctx, message)
}

// Middleware is the protocol for commbus middleware.
// Middleware can intercept messages before/after handling.
// Used for logging, telemetry, circuit breaking, etc.
type Middleware interface {
	// Before is called before message is handled.
	// Returns modified message, or nil to abort processing.
	Before(ctx context.Context, message Message) (Message, error)

	// After is called after message is handled.
	// Returns modified result.
	After(ctx context.Context, message Message, result any, err error) (any, error)
}

// CommBus is the protocol for the communication bus.
//
// The CommBus provides three messaging patterns:
//   - Publish(event): Fire-and-forget, fan-out to all subscribers
//   - Send(command): Fire-and-forget, single handler
//   - Query(query): Request-response, returns result
//
// Constitutional Reference:
//   - Core Engine R2: Protocol-first design
//   - Core Engine R5: Dependency injection (bus is injected)
//   - Avionics R1: Adapter pattern (bus is an adapter)
type CommBus interface {
	// ==========================================================================
	// MESSAGING
	// ==========================================================================

	// Publish publishes an event to all subscribers.
	// Events are fire-and-forget with fan-out semantics.
	Publish(ctx context.Context, event Message) error

	// Send sends a command to its handler.
	// Commands are fire-and-forget with single-handler semantics.
	Send(ctx context.Context, command Message) error

	// QuerySync sends a query and waits for response.
	// Queries are request-response with single-handler semantics.
	QuerySync(ctx context.Context, query Query) (any, error)

	// ==========================================================================
	// REGISTRATION
	// ==========================================================================

	// Subscribe subscribes to an event type.
	// Returns an unsubscribe function.
	Subscribe(eventType string, handler HandlerFunc) func()

	// RegisterHandler registers a handler for a message type.
	// Only one handler per message type is allowed.
	RegisterHandler(messageType string, handler HandlerFunc) error

	// AddMiddleware adds middleware to the bus.
	// Middleware is executed in registration order.
	AddMiddleware(middleware Middleware)

	// ==========================================================================
	// INTROSPECTION
	// ==========================================================================

	// HasHandler checks if a handler is registered for a message type.
	HasHandler(messageType string) bool

	// GetSubscribers gets all subscribers for an event type.
	GetSubscribers(eventType string) []HandlerFunc

	// Clear removes all handlers, subscribers, and middleware.
	Clear()
}

// =============================================================================
// APP CONTEXT PROTOCOLS
// =============================================================================

// Settings is the canonical protocol for application settings.
// ADR-001 Decision 3: Singleton Elimination via AppContext
type Settings interface {
	// Infrastructure Settings
	LlamaServerHost() string
	PostgresHost() string
	PostgresPort() int
	PostgresDatabase() string
	LogLevel() string

	// Agent/LLM Settings
	DefaultModel() string
	DisableTemperature() bool
	PlannerTemperature() *float64
	CriticTemperature() *float64
	GetPlannerModel() string
	GetCriticModel() string
}

// FeatureFlags is the canonical protocol for feature flags.
type FeatureFlags interface {
	UseGraphEngine() bool
	EnableCheckpoints() bool
	EnableTracing() bool
	IsEnabled(flagName string) bool
}

// Logger is the canonical protocol for structured logging.
type Logger interface {
	Debug(msg string, args ...any)
	Info(msg string, args ...any)
	Warning(msg string, args ...any)
	Error(msg string, args ...any)
	Bind(args ...any) Logger
}

// Clock is the canonical protocol for time operations.
// Abstracts time to enable deterministic testing.
type Clock interface {
	Now() any
	NowUTC() any
	MonotonicMS() int64
}

// AppContext is the canonical protocol for application context.
// Single object containing all injected dependencies.
type AppContext interface {
	Settings() Settings
	FeatureFlags() FeatureFlags
	Logger() Logger
	Clock() Clock
}

// =============================================================================
// INFRASTRUCTURE PROTOCOLS
// =============================================================================

// Persistence is the canonical protocol for persistence operations.
type Persistence interface {
	Insert(ctx context.Context, table string, data map[string]any) (*string, error)
	Update(ctx context.Context, table string, data map[string]any, where map[string]any) (bool, error)
	FetchOne(ctx context.Context, query string, params ...any) (map[string]any, error)
	FetchAll(ctx context.Context, query string, params ...any) ([]map[string]any, error)
	ToJSON(data any) (string, error)
}

// LLMProvider is the canonical protocol for LLM inference.
type LLMProvider interface {
	Generate(ctx context.Context, model string, prompt string, options map[string]any) (string, error)
	HealthCheck(ctx context.Context) (bool, error)
}

// =============================================================================
// TOOL PROTOCOLS
// =============================================================================

// ToolRegistry is the canonical protocol for tool registry operations.
type ToolRegistry interface {
	ListTools() []map[string]any
	HasTool(name string) bool
	GetTool(name string) any
	GetToolsForLLM() string
	GetReadOnlyTools() []string
}

// NLIService is the canonical protocol for NLI verification service.
// Natural Language Inference service for verifying claims against evidence.
type NLIService interface {
	Verify(ctx context.Context, claims string, evidence string) (float64, error)
	IsAvailable(ctx context.Context) (bool, error)
}

// =============================================================================
// MEMORY PROTOCOLS
// =============================================================================

// SearchResult represents the canonical result from a memory search operation.
type SearchResult struct {
	ItemID   string         `json:"item_id"`
	ItemType string         `json:"item_type"`
	Content  string         `json:"content"`
	Score    float64        `json:"score"`
	Metadata map[string]any `json:"metadata,omitempty"`
}

// MemoryService is the canonical protocol for memory storage operations.
type MemoryService interface {
	Store(ctx context.Context, layer, key string, data map[string]any, userID *string, ttlSeconds *int) (bool, error)
	Load(ctx context.Context, layer, key string, userID *string) (map[string]any, error)
	Delete(ctx context.Context, layer, key string, userID *string) (bool, error)
	Search(ctx context.Context, query string, layer *string, userID *string, limit int, filters map[string]any) ([]SearchResult, error)
	AppendEvent(ctx context.Context, eventType string, payload map[string]any, userID *string) (string, error)
	GetSessionContext(ctx context.Context, sessionID string, maxTurns int) ([]map[string]any, error)
}

// SemanticSearch is the canonical protocol for semantic (embedding-based) search.
type SemanticSearch interface {
	Embed(ctx context.Context, text string) ([]float64, error)
	EmbedBatch(ctx context.Context, texts []string) ([][]float64, error)
	SearchSimilar(ctx context.Context, queryEmbedding []float64, userID *string, sourceType *string, limit int, minSimilarity float64) ([]SearchResult, error)
	StoreWithEmbedding(ctx context.Context, itemID, content string, embedding []float64, metadata map[string]any, userID *string) (bool, error)
}

// =============================================================================
// DATABASE PROTOCOLS
// =============================================================================

// DatabaseClient is the canonical protocol for database client operations.
type DatabaseClient interface {
	Backend() string
	Connect(ctx context.Context) error
	Disconnect(ctx context.Context) error
	Execute(ctx context.Context, query string, parameters map[string]any) (any, error)
	ExecuteMany(ctx context.Context, query string, parametersList []map[string]any) error
	FetchOne(ctx context.Context, query string, parameters map[string]any) (map[string]any, error)
	FetchAll(ctx context.Context, query string, parameters map[string]any) ([]map[string]any, error)
}

// VectorStorage is the canonical protocol for vector storage operations.
type VectorStorage interface {
	Dimension() int
	StoreEmbedding(ctx context.Context, itemID string, embedding []float64, metadata map[string]any) (bool, error)
	SearchSimilar(ctx context.Context, queryEmbedding []float64, limit int, minSimilarity float64, filters map[string]any) ([]map[string]any, error)
	DeleteEmbedding(ctx context.Context, itemID string) (bool, error)
	CreateIndex(ctx context.Context, indexType string) (bool, error)
}

// =============================================================================
// CHECKPOINT PROTOCOLS
// =============================================================================

// CheckpointRecord represents canonical checkpoint metadata container.
type CheckpointRecord struct {
	CheckpointID       string         `json:"checkpoint_id"`
	EnvelopeID         string         `json:"envelope_id"`
	AgentName          string         `json:"agent_name"`
	StageOrder         int            `json:"stage_order"`
	CreatedAt          any            `json:"created_at"`
	StateSizeBytes     int            `json:"state_size_bytes"`
	ParentCheckpointID *string        `json:"parent_checkpoint_id,omitempty"`
	Metadata           map[string]any `json:"metadata,omitempty"`
}

// CheckpointService is the canonical protocol for execution checkpoint management.
// Constitutional Amendment XXIII: Time-Travel Debugging Support.
type CheckpointService interface {
	SaveCheckpoint(ctx context.Context, envelopeID, checkpointID, agentName string, state map[string]any, metadata map[string]any) (*CheckpointRecord, error)
	LoadCheckpoint(ctx context.Context, checkpointID string) (map[string]any, error)
	ListCheckpoints(ctx context.Context, envelopeID string, limit int) ([]CheckpointRecord, error)
	DeleteCheckpoints(ctx context.Context, envelopeID string, beforeCheckpointID *string) (int, error)
	ForkFromCheckpoint(ctx context.Context, checkpointID, newEnvelopeID string) (string, error)
}

// =============================================================================
// DISTRIBUTED BUS PROTOCOLS
// =============================================================================

// DistributedTask represents canonical task payload for distributed execution.
type DistributedTask struct {
	TaskID        string         `json:"task_id"`
	EnvelopeState map[string]any `json:"envelope_state"`
	AgentName     string         `json:"agent_name"`
	StageOrder    int            `json:"stage_order"`
	CheckpointID  *string        `json:"checkpoint_id,omitempty"`
	CreatedAt     any            `json:"created_at,omitempty"`
	RetryCount    int            `json:"retry_count"`
	MaxRetries    int            `json:"max_retries"`
	Priority      int            `json:"priority"`
}

// QueueStats represents canonical queue statistics container.
type QueueStats struct {
	QueueName           string  `json:"queue_name"`
	PendingCount        int     `json:"pending_count"`
	InProgressCount     int     `json:"in_progress_count"`
	CompletedCount      int     `json:"completed_count"`
	FailedCount         int     `json:"failed_count"`
	AvgProcessingTimeMS float64 `json:"avg_processing_time_ms"`
	WorkersActive       int     `json:"workers_active"`
}

// DistributedBus is the canonical protocol for distributed message passing.
// Constitutional Amendment XXIV: Horizontal Scaling Support.
type DistributedBus interface {
	EnqueueTask(ctx context.Context, queueName string, task *DistributedTask) (string, error)
	DequeueTask(ctx context.Context, queueName, workerID string, timeoutSeconds int) (*DistributedTask, error)
	CompleteTask(ctx context.Context, taskID string, result map[string]any) error
	FailTask(ctx context.Context, taskID, errorMsg string, retry bool) error
	RegisterWorker(ctx context.Context, workerID string, capabilities []string) error
	DeregisterWorker(ctx context.Context, workerID string) error
	Heartbeat(ctx context.Context, workerID string) error
	GetQueueStats(ctx context.Context, queueName string) (*QueueStats, error)
	ListQueues(ctx context.Context) ([]string, error)
}
