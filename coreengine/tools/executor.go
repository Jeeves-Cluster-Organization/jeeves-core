// Package tools provides tool execution for the Go runtime.
package tools

import (
	"context"
	"fmt"
	"sync"
)

// ToolHandler is a function that executes a tool.
type ToolHandler func(ctx context.Context, params map[string]any) (map[string]any, error)

// ToolDefinition defines a tool's metadata and handler.
type ToolDefinition struct {
	Name        string
	Description string
	Category    string
	RiskLevel   string // "low", "medium", "high"
	Handler     ToolHandler
}

// ToolExecutor executes tools by name.
type ToolExecutor struct {
	tools map[string]*ToolDefinition
	mu    sync.RWMutex
}

// NewToolExecutor creates a new ToolExecutor.
func NewToolExecutor() *ToolExecutor {
	return &ToolExecutor{
		tools: make(map[string]*ToolDefinition),
	}
}

// Register registers a tool.
func (e *ToolExecutor) Register(def *ToolDefinition) error {
	if def.Name == "" {
		return fmt.Errorf("tool name is required")
	}
	if def.Handler == nil {
		return fmt.Errorf("tool handler is required for '%s'", def.Name)
	}

	e.mu.Lock()
	defer e.mu.Unlock()

	e.tools[def.Name] = def
	return nil
}

// Execute executes a tool by name.
func (e *ToolExecutor) Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error) {
	e.mu.RLock()
	def, exists := e.tools[toolName]
	e.mu.RUnlock()

	if !exists {
		return nil, fmt.Errorf("tool not found: %s", toolName)
	}

	return def.Handler(ctx, params)
}

// Has checks if a tool is registered.
func (e *ToolExecutor) Has(toolName string) bool {
	e.mu.RLock()
	defer e.mu.RUnlock()
	_, exists := e.tools[toolName]
	return exists
}

// List returns all registered tool names.
func (e *ToolExecutor) List() []string {
	e.mu.RLock()
	defer e.mu.RUnlock()

	names := make([]string, 0, len(e.tools))
	for name := range e.tools {
		names = append(names, name)
	}
	return names
}

// GetDefinition gets a tool definition by name.
func (e *ToolExecutor) GetDefinition(toolName string) *ToolDefinition {
	e.mu.RLock()
	defer e.mu.RUnlock()
	return e.tools[toolName]
}

// ToolRegistry is an interface for tool registration and lookup.
type ToolRegistry interface {
	Register(def *ToolDefinition) error
	Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error)
	Has(toolName string) bool
	List() []string
}

// Ensure ToolExecutor implements ToolRegistry
var _ ToolRegistry = (*ToolExecutor)(nil)
