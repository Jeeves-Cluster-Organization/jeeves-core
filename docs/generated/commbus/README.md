# CommBus - Communication Bus

> **File Location**: `jeeves-core/commbus/`

The CommBus package provides a thread-safe, in-memory message bus implementation for the Jeeves system. It enables decoupled communication between components through events, queries, and commands.

## Table of Contents

- [Overview](#overview)
- [Message Categories](#message-categories)
- [Core Types](#core-types)
- [Message Types](#message-types)
- [Middleware System](#middleware-system)
- [Error Handling](#error-handling)
- [Usage Examples](#usage-examples)
- [Related Documentation](#related-documentation)

---

## Overview

CommBus implements a publish-subscribe and request-response messaging pattern. It serves as the backbone for inter-component communication within the Jeeves system.

### Key Features

- **Event Fan-out**: Events are delivered to all subscribers concurrently
- **Query Request-Response**: Synchronous queries with timeout support
- **Command Fire-and-Forget**: Single-handler command processing
- **Middleware Chain**: Extensible middleware for cross-cutting concerns
- **Handler Introspection**: Ability to inspect registered handlers
- **Structured Logging**: Injectable logger for observability

### Constitutional References

- **Core Engine R2**: Protocol-First Design
- **Core Engine R3**: Immutable State Transitions
- **Core Engine R5**: Dependency Injection
- **Avionics R1**: Adapter Pattern

---

## Message Categories

The bus supports three message routing categories:

| Category | Description | Semantics |
|----------|-------------|-----------|
| `event` | Fire-and-forget | Fan-out to all subscribers |
| `query` | Request-response | Single handler, returns result |
| `command` | Fire-and-forget | Single handler |

```go
type MessageCategory string

const (
    MessageCategoryEvent   MessageCategory = "event"
    MessageCategoryQuery   MessageCategory = "query"
    MessageCategoryCommand MessageCategory = "command"
)
```

---

## Core Types

### CommBus Interface

**File**: `protocols.go`

The primary interface for the communication bus:

```go
type CommBus interface {
    // Messaging
    Publish(ctx context.Context, event Message) error
    Send(ctx context.Context, command Message) error
    QuerySync(ctx context.Context, query Query) (any, error)
    
    // Registration
    Subscribe(eventType string, handler HandlerFunc) func()
    RegisterHandler(messageType string, handler HandlerFunc) error
    AddMiddleware(middleware Middleware)
    
    // Introspection
    HasHandler(messageType string) bool
    GetSubscribers(eventType string) []HandlerFunc
    Clear()
}
```

### Message Interface

**File**: `protocols.go`

All messages must implement this interface:

```go
type Message interface {
    Category() string
}
```

### Query Interface

**File**: `protocols.go`

Queries expect a response:

```go
type Query interface {
    Message
    IsQuery()
}
```

### Handler Interface

**File**: `protocols.go`

Interface for message handlers:

```go
type Handler interface {
    Handle(ctx context.Context, message Message) (any, error)
}
```

### HandlerFunc

**File**: `protocols.go`

Function type that implements Handler:

```go
type HandlerFunc func(ctx context.Context, message Message) (any, error)

// Handle implements the Handler interface.
func (f HandlerFunc) Handle(ctx context.Context, message Message) (any, error)
```

### InMemoryCommBus

**File**: `bus.go`

Thread-safe, async-compatible message bus implementation:

```go
type InMemoryCommBus struct {
    handlers     map[string]HandlerFunc
    subscribers  map[string][]subscriberEntry
    middleware   []Middleware
    queryTimeout time.Duration
    nextSubID    uint64
    logger       BusLogger
    mu           sync.RWMutex
}
```

**Constructor Functions**:

| Function | Description |
|----------|-------------|
| `NewInMemoryCommBus(queryTimeout time.Duration)` | Creates bus with default logger |
| `NewInMemoryCommBusWithLogger(queryTimeout time.Duration, logger BusLogger)` | Creates bus with custom logger |

**Additional Methods**:

| Method | Description |
|--------|-------------|
| `SetLogger(logger BusLogger)` | Sets the logger; use `NoopBusLogger()` to disable |
| `GetRegisteredTypes() []string` | Gets all registered message types (handlers + subscriptions) |

**Logger Utilities**:

| Function | Description |
|----------|-------------|
| `NoopBusLogger() BusLogger` | Returns a logger that discards all output |

---

## Message Types

### Agent Lifecycle Events

**File**: `messages.go`

| Type | Category | Description |
|------|----------|-------------|
| `AgentStarted` | event | Emitted when agent begins processing |
| `AgentCompleted` | event | Emitted when agent finishes processing |

```go
type AgentStarted struct {
    AgentName  string
    SessionID  string
    RequestID  string
    EnvelopeID string
    Payload    map[string]any
}

type AgentCompleted struct {
    AgentName  string
    SessionID  string
    RequestID  string
    EnvelopeID string
    Status     string  // "success", "error", "skipped"
    DurationMS int
    Error      *string
    Payload    map[string]any
}
```

### Tool Execution Events

| Type | Category | Description |
|------|----------|-------------|
| `ToolStarted` | event | Emitted when tool execution begins |
| `ToolCompleted` | event | Emitted when tool execution finishes |

```go
type ToolStarted struct {
    ToolName      string
    SessionID     string
    RequestID     string
    StepNumber    int
    TotalSteps    int
    ParamsPreview map[string]string
}

type ToolCompleted struct {
    ToolName        string
    SessionID       string
    RequestID       string
    Status          string  // "success", "error", "timeout"
    ExecutionTimeMS int
    Error           *string
    ErrorType       *string
}
```

### Pipeline Lifecycle Events

| Type | Category | Description |
|------|----------|-------------|
| `PipelineStarted` | event | Pipeline run started |
| `PipelineCompleted` | event | Pipeline completed |
| `StageTransition` | event | Pipeline moved to new stage |

### Config Queries

| Type | Category | Description |
|------|----------|-------------|
| `GetAgentToolAccess` | query | Query tool access for agent |
| `GetNodeProfile` | query | Query LLM config for agent |
| `GetLanguageConfig` | query | Query language configuration |
| `GetSettings` | query | Query application settings |

### Health Check

| Type | Category | Response Type |
|------|----------|---------------|
| `HealthCheckRequest` | query | `HealthCheckResponse` |

```go
type HealthStatus string

const (
    HealthStatusHealthy   HealthStatus = "healthy"
    HealthStatusDegraded  HealthStatus = "degraded"
    HealthStatusUnhealthy HealthStatus = "unhealthy"
    HealthStatusUnknown   HealthStatus = "unknown"
)
```

### WebSocket Broadcast Events

| Type | Category | Description |
|------|----------|-------------|
| `FrontendBroadcast` | event | Unified broadcast to frontend |
| `ResponseChunk` | event | Streaming response chunk |

### Tool Catalog Queries

| Type | Category | Description |
|------|----------|-------------|
| `GetToolCatalog` | query | Query tool catalog |
| `GetToolEntry` | query | Query single tool entry |

### Prompt Registry Queries

| Type | Category | Description |
|------|----------|-------------|
| `GetPrompt` | query | Query prompt template |
| `ListPrompts` | query | List available prompts |

### Domain Events (Code Analysis)

| Type | Category | Description |
|------|----------|-------------|
| `GoalCompleted` | event | Goal completed during analysis |
| `ClarificationRequested` | event | Clarification needed from user |

---

## Middleware System

**File**: `middleware.go`

Middleware intercepts messages before and after handling.

### Middleware Interface

```go
type Middleware interface {
    Before(ctx context.Context, message Message) (Message, error)
    After(ctx context.Context, message Message, result any, err error) (any, error)
}
```

### LoggingMiddleware

Logs all message traffic using structured logging:

```go
func NewLoggingMiddleware(logLevel string) *LoggingMiddleware
func NewLoggingMiddlewareWithLogger(logLevel string, logger BusLogger) *LoggingMiddleware
```

### CircuitBreakerMiddleware

Implements the circuit breaker pattern to protect against cascading failures:

```go
func NewCircuitBreakerMiddleware(
    failureThreshold int,
    resetTimeout time.Duration,
    excludedTypes []string,
) *CircuitBreakerMiddleware

func NewCircuitBreakerMiddlewareWithLogger(
    failureThreshold int,
    resetTimeout time.Duration,
    excludedTypes []string,
    logger BusLogger,
) *CircuitBreakerMiddleware
```

**Circuit States**:

| State | Description |
|-------|-------------|
| `closed` | Normal operation, requests pass through |
| `open` | Requests are blocked |
| `half-open` | Testing with single request |

**Methods**:

| Method | Description |
|--------|-------------|
| `GetStates()` | Returns current circuit states |
| `Reset(msgType *string)` | Resets circuit state |

---

## Error Handling

**File**: `errors.go`

### Error Types

| Error Type | Description |
|------------|-------------|
| `CommBusError` | Base error type |
| `NoHandlerError` | No handler registered for message type |
| `HandlerAlreadyRegisteredError` | Duplicate handler registration |
| `QueryTimeoutError` | Query timed out |

### Error Constructors

```go
func NewNoHandlerError(messageType string) *NoHandlerError
func NewHandlerAlreadyRegisteredError(messageType string) *HandlerAlreadyRegisteredError
func NewQueryTimeoutError(messageType string, timeout float64) *QueryTimeoutError
```

---

## Risk Levels

**File**: `protocols.go`

Tools declare their risk level (Constitutional Amendment VIII):

```go
type RiskLevel string

const (
    RiskLevelReadOnly    RiskLevel = "read_only"    // No state mutation
    RiskLevelWrite       RiskLevel = "write"        // State mutation
    RiskLevelDestructive RiskLevel = "destructive"  // Irreversible operations
)

// RequiresConfirmation checks if risk level needs user confirmation
func (r RiskLevel) RequiresConfirmation() bool
```

---

## Tool Categories

**File**: `protocols.go`

Tools are organized by category for access control:

```go
type ToolCategory string

const (
    ToolCategoryUnified    ToolCategory = "unified"    // Single entry point tools (Amendment XXII)
    ToolCategoryComposite  ToolCategory = "composite"  // Multi-step workflows with fallbacks
    ToolCategoryResilient  ToolCategory = "resilient"  // Wrapped base tools with retry/fallback
    ToolCategoryStandalone ToolCategory = "standalone" // Simple tools that don't need wrapping
    ToolCategoryInternal   ToolCategory = "internal"   // Implementation details, not exposed to agents
)
```

---

## Usage Examples

### Creating a Bus

```go
bus := NewInMemoryCommBus(30 * time.Second)

// Or with custom logger
bus := NewInMemoryCommBusWithLogger(30 * time.Second, myLogger)
```

### Registering Handlers

```go
// Register a query handler
err := bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
    query := msg.(*GetSettings)
    return &SettingsResponse{Values: settings}, nil
})

// Subscribe to events
unsubscribe := bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
    event := msg.(*AgentStarted)
    log.Printf("Agent %s started", event.AgentName)
    return nil, nil
})
defer unsubscribe()
```

### Publishing Events

```go
err := bus.Publish(ctx, &AgentStarted{
    AgentName:  "planner",
    SessionID:  "sess_123",
    RequestID:  "req_456",
    EnvelopeID: "env_789",
})
```

### Sending Commands

```go
err := bus.Send(ctx, &InvalidateCache{
    CacheName: "prompts",
    Key:       nil, // Invalidate all
})
```

### Querying

```go
result, err := bus.QuerySync(ctx, &GetSettings{Key: ptr("llm_model")})
if err != nil {
    return err
}
settings := result.(*SettingsResponse)
```

### Adding Middleware

```go
// Add logging
bus.AddMiddleware(NewLoggingMiddleware("DEBUG"))

// Add circuit breaker
bus.AddMiddleware(NewCircuitBreakerMiddleware(
    5,                    // Failure threshold
    30 * time.Second,     // Reset timeout
    []string{"HealthCheck"}, // Excluded types
))
```

---

## Related Documentation

- [Core Engine Overview](../coreengine/README.md)
- [Runtime Execution](../coreengine/runtime.md)
- [Envelope System](../coreengine/envelope.md)
