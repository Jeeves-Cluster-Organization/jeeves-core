# Tools - Tool Executor

> **File Location**: `jeeves-core/coreengine/tools/executor.go`

The Tools package provides tool registration and execution for the Go runtime.

## Table of Contents

- [Overview](#overview)
- [Key Types](#key-types)
- [ToolExecutor](#toolexecutor)
- [ToolDefinition](#tooldefinition)
- [Usage Examples](#usage-examples)
- [Related Documentation](#related-documentation)

---

## Overview

The tool system provides:

- **Tool Registry**: Register tools with metadata and handlers
- **Tool Execution**: Execute tools by name with parameters
- **Risk Levels**: Classify tools by risk (low, medium, high)
- **Categories**: Organize tools by category
- **Thread Safety**: Concurrent-safe registry operations

---

## Key Types

### ToolHandler

Function type that executes a tool:

```go
type ToolHandler func(ctx context.Context, params map[string]any) (map[string]any, error)
```

### ToolDefinition

Defines a tool's metadata and handler:

```go
type ToolDefinition struct {
    Name        string       // Unique tool name
    Description string       // Human-readable description
    Category    string       // Tool category for organization
    RiskLevel   string       // "low", "medium", "high"
    Handler     ToolHandler  // Execution function
}
```

### ToolRegistry Interface

Interface for tool registration and lookup:

```go
type ToolRegistry interface {
    Register(def *ToolDefinition) error
    Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error)
    Has(toolName string) bool
    List() []string
}
```

**Note**: `ToolExecutor` implements the `ToolRegistry` interface.

---

## ToolExecutor

Thread-safe tool executor implementation.

### Structure

```go
type ToolExecutor struct {
    tools map[string]*ToolDefinition
    mu    sync.RWMutex
}
```

### Constructor

```go
func NewToolExecutor() *ToolExecutor
```

Creates a new empty tool executor.

### Methods

| Method | Signature | Description |
|--------|-----------|-------------|
| `Register` | `(def *ToolDefinition) error` | Register a tool |
| `Execute` | `(ctx, toolName, params) (map, error)` | Execute tool by name |
| `Has` | `(toolName string) bool` | Check if tool exists |
| `List` | `() []string` | List all registered tools |
| `GetDefinition` | `(toolName string) *ToolDefinition` | Get tool definition |

### Register

```go
func (e *ToolExecutor) Register(def *ToolDefinition) error
```

Registers a tool definition. Returns error if:
- Tool name is empty
- Handler is nil

### Execute

```go
func (e *ToolExecutor) Execute(
    ctx context.Context,
    toolName string,
    params map[string]any,
) (map[string]any, error)
```

Executes a tool by name. Returns error if tool not found.

### Has

```go
func (e *ToolExecutor) Has(toolName string) bool
```

Checks if a tool is registered.

### List

```go
func (e *ToolExecutor) List() []string
```

Returns all registered tool names.

### GetDefinition

```go
func (e *ToolExecutor) GetDefinition(toolName string) *ToolDefinition
```

Returns the tool definition or nil if not found.

---

## Usage Examples

### Creating an Executor

```go
executor := tools.NewToolExecutor()
```

### Registering Tools

```go
// Register a read-only tool
err := executor.Register(&tools.ToolDefinition{
    Name:        "read_file",
    Description: "Read contents of a file",
    Category:    "filesystem",
    RiskLevel:   "low",
    Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
        path := params["path"].(string)
        content, err := os.ReadFile(path)
        if err != nil {
            return nil, err
        }
        return map[string]any{
            "content": string(content),
            "path":    path,
        }, nil
    },
})

// Register a write tool
err = executor.Register(&tools.ToolDefinition{
    Name:        "write_file",
    Description: "Write contents to a file",
    Category:    "filesystem",
    RiskLevel:   "high",
    Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
        path := params["path"].(string)
        content := params["content"].(string)
        
        err := os.WriteFile(path, []byte(content), 0644)
        if err != nil {
            return nil, err
        }
        return map[string]any{
            "status": "success",
            "path":   path,
            "bytes":  len(content),
        }, nil
    },
})
```

### Executing Tools

```go
// Execute read_file
result, err := executor.Execute(ctx, "read_file", map[string]any{
    "path": "/path/to/file.txt",
})
if err != nil {
    log.Printf("Tool failed: %v", err)
    return
}

content := result["content"].(string)
fmt.Printf("File content: %s\n", content)
```

### Checking Tool Availability

```go
if executor.Has("read_file") {
    // Tool is available
}

// List all tools
tools := executor.List()
fmt.Printf("Available tools: %v\n", tools)
```

### Getting Tool Information

```go
def := executor.GetDefinition("read_file")
if def != nil {
    fmt.Printf("Tool: %s\n", def.Name)
    fmt.Printf("Description: %s\n", def.Description)
    fmt.Printf("Category: %s\n", def.Category)
    fmt.Printf("Risk Level: %s\n", def.RiskLevel)
}
```

### Using with Agent

```go
// Create executor with tools
executor := tools.NewToolExecutor()
executor.Register(&tools.ToolDefinition{
    Name:    "analyze_code",
    Handler: analyzeCodeHandler,
})

// Create agent with tool access
agent, err := agents.NewAgent(
    &config.AgentConfig{
        Name:       "executor",
        HasTools:   true,
        ToolAccess: config.ToolAccessAll,
    },
    logger,
    nil,       // No LLM
    executor,  // Tool executor
)
```

### Tool with Context Timeout

```go
executor.Register(&tools.ToolDefinition{
    Name:      "slow_operation",
    RiskLevel: "medium",
    Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
        // Respect context cancellation
        select {
        case <-ctx.Done():
            return nil, ctx.Err()
        case <-time.After(5 * time.Second):
            return map[string]any{"status": "completed"}, nil
        }
    },
})

// Execute with timeout
ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
defer cancel()

result, err := executor.Execute(ctx, "slow_operation", nil)
if err == context.DeadlineExceeded {
    log.Println("Operation timed out")
}
```

### Categorizing Tools

```go
// Register tools by category
categories := map[string][]string{
    "filesystem": {"read_file", "write_file", "list_directory"},
    "analysis":   {"parse_ast", "find_dependencies", "analyze_code"},
    "search":     {"grep_files", "find_symbol", "search_references"},
}

// Get tools by category
def := executor.GetDefinition("read_file")
if def.Category == "filesystem" {
    // Handle filesystem tool
}
```

---

## Related Documentation

- [Core Engine Overview](README.md)
- [Agent System](agents.md) - Uses ToolExecutor for tool access
- [Runtime Execution](runtime.md) - Injects ToolExecutor into Runtime
- [CommBus Protocols](../commbus/README.md) - RiskLevel and ToolCategory definitions
