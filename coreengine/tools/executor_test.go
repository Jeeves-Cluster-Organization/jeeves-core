// Package tools tests for ToolExecutor.
package tools

import (
	"context"
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TOOL EXECUTOR TESTS
// =============================================================================

func TestNewToolExecutor(t *testing.T) {
	// Test creating a new tool executor.
	executor := NewToolExecutor()

	assert.NotNil(t, executor)
	assert.Empty(t, executor.List())
}

func TestRegisterTool(t *testing.T) {
	// Test registering a tool.
	executor := NewToolExecutor()

	def := &ToolDefinition{
		Name:        "test_tool",
		Description: "A test tool",
		Category:    "testing",
		RiskLevel:   "low",
		Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
			return map[string]any{"result": "success"}, nil
		},
	}

	err := executor.Register(def)

	require.NoError(t, err)
	assert.True(t, executor.Has("test_tool"))
	assert.Contains(t, executor.List(), "test_tool")
}

func TestRegisterToolWithoutName(t *testing.T) {
	// Test registering a tool without name fails.
	executor := NewToolExecutor()

	def := &ToolDefinition{
		Name: "", // Empty name
		Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
			return nil, nil
		},
	}

	err := executor.Register(def)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "name is required")
}

func TestRegisterToolWithoutHandler(t *testing.T) {
	// Test registering a tool without handler fails.
	executor := NewToolExecutor()

	def := &ToolDefinition{
		Name:    "broken_tool",
		Handler: nil, // No handler
	}

	err := executor.Register(def)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "handler is required")
}

func TestExecuteTool(t *testing.T) {
	// Test executing a registered tool.
	executor := NewToolExecutor()

	def := &ToolDefinition{
		Name: "echo_tool",
		Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
			return map[string]any{
				"echo":   params["input"],
				"status": "success",
			}, nil
		},
	}
	executor.Register(def)

	result, err := executor.Execute(context.Background(), "echo_tool", map[string]any{"input": "hello"})

	require.NoError(t, err)
	assert.Equal(t, "hello", result["echo"])
	assert.Equal(t, "success", result["status"])
}

func TestExecuteToolNotFound(t *testing.T) {
	// Test executing a non-existent tool fails.
	executor := NewToolExecutor()

	result, err := executor.Execute(context.Background(), "nonexistent_tool", nil)

	require.Error(t, err)
	assert.Nil(t, result)
	assert.Contains(t, err.Error(), "tool not found")
	assert.Contains(t, err.Error(), "nonexistent_tool")
}

func TestExecuteToolError(t *testing.T) {
	// Test tool that returns an error.
	executor := NewToolExecutor()

	def := &ToolDefinition{
		Name: "error_tool",
		Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
			return nil, errors.New("tool execution failed")
		},
	}
	executor.Register(def)

	result, err := executor.Execute(context.Background(), "error_tool", nil)

	require.Error(t, err)
	assert.Nil(t, result)
	assert.Contains(t, err.Error(), "tool execution failed")
}

func TestHasTool(t *testing.T) {
	// Test checking if a tool exists.
	executor := NewToolExecutor()

	assert.False(t, executor.Has("test_tool"))

	executor.Register(&ToolDefinition{
		Name: "test_tool",
		Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
			return nil, nil
		},
	})

	assert.True(t, executor.Has("test_tool"))
	assert.False(t, executor.Has("other_tool"))
}

func TestListTools(t *testing.T) {
	// Test listing all registered tools.
	executor := NewToolExecutor()

	assert.Empty(t, executor.List())

	handler := func(ctx context.Context, params map[string]any) (map[string]any, error) {
		return nil, nil
	}

	executor.Register(&ToolDefinition{Name: "tool_a", Handler: handler})
	executor.Register(&ToolDefinition{Name: "tool_b", Handler: handler})
	executor.Register(&ToolDefinition{Name: "tool_c", Handler: handler})

	tools := executor.List()

	assert.Len(t, tools, 3)
	assert.Contains(t, tools, "tool_a")
	assert.Contains(t, tools, "tool_b")
	assert.Contains(t, tools, "tool_c")
}

func TestGetDefinition(t *testing.T) {
	// Test getting a tool definition.
	executor := NewToolExecutor()

	def := &ToolDefinition{
		Name:        "my_tool",
		Description: "My tool description",
		Category:    "testing",
		RiskLevel:   "medium",
		Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
			return nil, nil
		},
	}
	executor.Register(def)

	retrieved := executor.GetDefinition("my_tool")

	require.NotNil(t, retrieved)
	assert.Equal(t, "my_tool", retrieved.Name)
	assert.Equal(t, "My tool description", retrieved.Description)
	assert.Equal(t, "testing", retrieved.Category)
	assert.Equal(t, "medium", retrieved.RiskLevel)
}

func TestGetDefinitionNotFound(t *testing.T) {
	// Test getting a non-existent tool definition.
	executor := NewToolExecutor()

	retrieved := executor.GetDefinition("nonexistent")

	assert.Nil(t, retrieved)
}

func TestExecuteWithContext(t *testing.T) {
	// Test that context is passed to handler.
	executor := NewToolExecutor()

	type ctxKey string
	key := ctxKey("test_key")

	def := &ToolDefinition{
		Name: "context_tool",
		Handler: func(ctx context.Context, params map[string]any) (map[string]any, error) {
			value := ctx.Value(key)
			if value != nil {
				return map[string]any{"context_value": value}, nil
			}
			return map[string]any{"context_value": nil}, nil
		},
	}
	executor.Register(def)

	ctx := context.WithValue(context.Background(), key, "test_value")
	result, err := executor.Execute(ctx, "context_tool", nil)

	require.NoError(t, err)
	assert.Equal(t, "test_value", result["context_value"])
}

func TestToolOverwrite(t *testing.T) {
	// Test that registering a tool with the same name overwrites.
	executor := NewToolExecutor()

	handler1 := func(ctx context.Context, params map[string]any) (map[string]any, error) {
		return map[string]any{"version": 1}, nil
	}
	handler2 := func(ctx context.Context, params map[string]any) (map[string]any, error) {
		return map[string]any{"version": 2}, nil
	}

	executor.Register(&ToolDefinition{Name: "tool", Handler: handler1})
	result1, _ := executor.Execute(context.Background(), "tool", nil)
	assert.Equal(t, 1, result1["version"])

	executor.Register(&ToolDefinition{Name: "tool", Handler: handler2})
	result2, _ := executor.Execute(context.Background(), "tool", nil)
	assert.Equal(t, 2, result2["version"])
}

// =============================================================================
// INTERFACE TESTS
// =============================================================================

func TestToolExecutorImplementsToolRegistry(t *testing.T) {
	// Test that ToolExecutor implements ToolRegistry interface.
	var registry ToolRegistry = NewToolExecutor()
	assert.NotNil(t, registry)
}
