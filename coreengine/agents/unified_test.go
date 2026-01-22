// Package agents tests for UnifiedAgent.
package agents

import (
	"context"
	"errors"
	"testing"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

// MockLogger implements Logger for testing.
type MockLogger struct {
	infoCalls  []string
	debugCalls []string
	warnCalls  []string
	errorCalls []string
}

func (m *MockLogger) Info(msg string, fields ...any)  { m.infoCalls = append(m.infoCalls, msg) }
func (m *MockLogger) Debug(msg string, fields ...any) { m.debugCalls = append(m.debugCalls, msg) }
func (m *MockLogger) Warn(msg string, fields ...any)  { m.warnCalls = append(m.warnCalls, msg) }
func (m *MockLogger) Error(msg string, fields ...any) { m.errorCalls = append(m.errorCalls, msg) }
func (m *MockLogger) Bind(fields ...any) Logger       { return m }

// MockLLMProvider implements LLMProvider for testing.
type MockLLMProvider struct {
	response string
	err      error
}

func (m *MockLLMProvider) Generate(ctx context.Context, model string, prompt string, options map[string]any) (string, error) {
	if m.err != nil {
		return "", m.err
	}
	return m.response, nil
}

// MockToolExecutor implements ToolExecutor for testing.
type MockToolExecutor struct {
	results map[string]map[string]any
	errors  map[string]error
}

func (m *MockToolExecutor) Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error) {
	if err, exists := m.errors[toolName]; exists {
		return nil, err
	}
	if result, exists := m.results[toolName]; exists {
		return result, nil
	}
	return map[string]any{"status": "success"}, nil
}

// MockEventContext implements EventContext for testing.
type MockEventContext struct {
	startedAgents   []string
	completedAgents []string
}

func (m *MockEventContext) EmitAgentStarted(agentName string) error {
	m.startedAgents = append(m.startedAgents, agentName)
	return nil
}

func (m *MockEventContext) EmitAgentCompleted(agentName string, status string, durationMS int, err error) error {
	m.completedAgents = append(m.completedAgents, agentName)
	return nil
}

// MockPromptRegistry implements PromptRegistry for testing.
type MockPromptRegistry struct {
	prompts map[string]string
	err     error
}

func (m *MockPromptRegistry) Get(key string, context map[string]any) (string, error) {
	if m.err != nil {
		return "", m.err
	}
	if prompt, exists := m.prompts[key]; exists {
		return prompt, nil
	}
	return "", errors.New("prompt not found")
}

func createTestAgentConfig(name string) *config.AgentConfig {
	return &config.AgentConfig{
		Name:       name,
		OutputKey:  name,
		StageOrder: 1,
	}
}

// =============================================================================
// CREATION TESTS
// =============================================================================

func TestNewUnifiedAgentBasic(t *testing.T) {
	// Test creating a basic agent.
	cfg := createTestAgentConfig("test_agent")
	logger := &MockLogger{}

	agent, err := NewUnifiedAgent(cfg, logger, nil, nil)

	require.NoError(t, err)
	assert.NotNil(t, agent)
	assert.Equal(t, "test_agent", agent.Name)
	assert.Equal(t, cfg, agent.Config)
}

func TestNewUnifiedAgentWithLLM(t *testing.T) {
	// Test creating an agent with LLM.
	cfg := createTestAgentConfig("llm_agent")
	cfg.HasLLM = true
	cfg.ModelRole = "default"

	logger := &MockLogger{}
	llm := &MockLLMProvider{response: `{"result": "success"}`}

	agent, err := NewUnifiedAgent(cfg, logger, llm, nil)

	require.NoError(t, err)
	assert.NotNil(t, agent)
	assert.NotNil(t, agent.LLM)
}

func TestNewUnifiedAgentWithTools(t *testing.T) {
	// Test creating an agent with tools.
	cfg := createTestAgentConfig("tool_agent")
	cfg.HasTools = true
	cfg.ToolAccess = config.ToolAccessAll

	logger := &MockLogger{}
	tools := &MockToolExecutor{}

	agent, err := NewUnifiedAgent(cfg, logger, nil, tools)

	require.NoError(t, err)
	assert.NotNil(t, agent)
	assert.NotNil(t, agent.Tools)
}

func TestNewUnifiedAgentLLMRequiredButMissing(t *testing.T) {
	// Test that creating an agent with has_llm=true but no llm fails.
	cfg := createTestAgentConfig("llm_agent")
	cfg.HasLLM = true
	cfg.ModelRole = "default" // Need model role to pass first check

	logger := &MockLogger{}

	agent, err := NewUnifiedAgent(cfg, logger, nil, nil)

	require.Error(t, err)
	assert.Nil(t, agent)
	assert.Contains(t, err.Error(), "has_llm=true but no llm_provider")
}

func TestNewUnifiedAgentToolsRequiredButMissing(t *testing.T) {
	// Test that creating an agent with has_tools=true but no tools fails.
	cfg := createTestAgentConfig("tool_agent")
	cfg.HasTools = true

	logger := &MockLogger{}

	agent, err := NewUnifiedAgent(cfg, logger, nil, nil)

	require.Error(t, err)
	assert.Nil(t, agent)
	assert.Contains(t, err.Error(), "has_tools=true but no tool_executor")
}

// =============================================================================
// PROCESS TESTS - SERVICE AGENT
// =============================================================================

func TestProcessServiceAgent(t *testing.T) {
	// Test processing with a service agent (no LLM, no tools).
	cfg := createTestAgentConfig("service_agent")
	cfg.DefaultNext = "next_stage"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)

	env := envelope.CreateGenericEnvelope("Test input", "user", "session", nil, nil, nil)

	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	assert.NotNil(t, result)
	assert.Equal(t, "next_stage", result.CurrentStage)
	assert.True(t, result.HasOutput("service_agent"))
}

func TestProcessWithPreProcessHook(t *testing.T) {
	// Test that pre-process hook is called.
	cfg := createTestAgentConfig("hook_agent")
	cfg.DefaultNext = "end"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)

	preProcessCalled := false
	agent.PreProcess = func(env *envelope.GenericEnvelope) (*envelope.GenericEnvelope, error) {
		preProcessCalled = true
		env.SetOutput("pre_process", map[string]any{"called": true})
		return env, nil
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	assert.True(t, preProcessCalled)
	assert.True(t, result.HasOutput("pre_process"))
}

func TestProcessWithPostProcessHook(t *testing.T) {
	// Test that post-process hook is called.
	cfg := createTestAgentConfig("hook_agent")
	cfg.DefaultNext = "end"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)

	postProcessCalled := false
	agent.PostProcess = func(env *envelope.GenericEnvelope, output map[string]any) (*envelope.GenericEnvelope, error) {
		postProcessCalled = true
		env.SetOutput("post_process", map[string]any{"called": true})
		return env, nil
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	assert.True(t, postProcessCalled)
	assert.True(t, result.HasOutput("post_process"))
}

func TestProcessPreProcessHookError(t *testing.T) {
	// Test that pre-process hook error is handled.
	cfg := createTestAgentConfig("hook_agent")
	cfg.ErrorNext = "error_stage"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)

	agent.PreProcess = func(env *envelope.GenericEnvelope) (*envelope.GenericEnvelope, error) {
		return env, errors.New("pre-process failed")
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err) // Error routed to error_stage
	assert.Equal(t, "error_stage", result.CurrentStage)
	assert.Len(t, result.Errors, 1)
}

// =============================================================================
// PROCESS TESTS - MOCK HANDLER
// =============================================================================

func TestProcessWithMockHandler(t *testing.T) {
	// Test processing with mock handler.
	cfg := createTestAgentConfig("mock_agent")
	cfg.DefaultNext = "end"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)
	agent.UseMock = true
	agent.MockHandler = func(env *envelope.GenericEnvelope) (map[string]any, error) {
		return map[string]any{"mocked": true, "result": "mock success"}, nil
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	output := result.GetOutput("mock_agent")
	assert.True(t, output["mocked"].(bool))
	assert.Equal(t, "mock success", output["result"])
}

func TestProcessMockHandlerError(t *testing.T) {
	// Test mock handler error handling.
	cfg := createTestAgentConfig("mock_agent")
	cfg.ErrorNext = "error_stage"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)
	agent.UseMock = true
	agent.MockHandler = func(env *envelope.GenericEnvelope) (map[string]any, error) {
		return nil, errors.New("mock error")
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err) // Error routed
	assert.Equal(t, "error_stage", result.CurrentStage)
}

// =============================================================================
// PROCESS TESTS - LLM AGENT
// =============================================================================

func TestProcessLLMAgent(t *testing.T) {
	// Test processing with LLM agent.
	cfg := createTestAgentConfig("llm_agent")
	cfg.HasLLM = true
	cfg.ModelRole = "default"
	cfg.DefaultNext = "end"
	logger := &MockLogger{}
	llm := &MockLLMProvider{response: `{"intent": "analyze", "confidence": 0.9}`}

	agent, _ := NewUnifiedAgent(cfg, logger, llm, nil)

	env := envelope.CreateGenericEnvelope("Analyze this code", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	output := result.GetOutput("llm_agent")
	assert.Equal(t, "analyze", output["intent"])
	assert.Equal(t, 0.9, output["confidence"])
}

func TestProcessLLMAgentInvalidJSON(t *testing.T) {
	// Test LLM agent with invalid JSON response.
	cfg := createTestAgentConfig("llm_agent")
	cfg.HasLLM = true
	cfg.ModelRole = "default"
	cfg.ErrorNext = "error_stage"
	logger := &MockLogger{}
	llm := &MockLLMProvider{response: "not valid json"} // Invalid JSON

	agent, _ := NewUnifiedAgent(cfg, logger, llm, nil)

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err) // Error routed
	assert.Equal(t, "error_stage", result.CurrentStage)
}

func TestProcessLLMAgentWithPromptRegistry(t *testing.T) {
	// Test LLM agent uses prompt registry.
	cfg := createTestAgentConfig("llm_agent")
	cfg.HasLLM = true
	cfg.ModelRole = "default"
	cfg.PromptKey = "llm_prompt"
	cfg.DefaultNext = "end"
	logger := &MockLogger{}
	llm := &MockLLMProvider{response: `{"result": "from_registry"}`}
	registry := &MockPromptRegistry{
		prompts: map[string]string{"llm_prompt": "Custom prompt: {{raw_input}}"},
	}

	agent, _ := NewUnifiedAgent(cfg, logger, llm, nil)
	agent.PromptRegistry = registry

	env := envelope.CreateGenericEnvelope("Test input", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	output := result.GetOutput("llm_agent")
	assert.Equal(t, "from_registry", output["result"])
}

// =============================================================================
// PROCESS TESTS - TOOL AGENT
// =============================================================================

func TestProcessToolAgent(t *testing.T) {
	// Test processing with tool agent.
	cfg := createTestAgentConfig("executor")
	cfg.HasTools = true
	cfg.ToolAccess = config.ToolAccessAll
	cfg.DefaultNext = "end"
	logger := &MockLogger{}
	tools := &MockToolExecutor{
		results: map[string]map[string]any{
			"read_file": {"content": "file content", "status": "success"},
		},
	}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, tools)

	env := envelope.CreateGenericEnvelope("Read file", "user", "session", nil, nil, nil)
	env.SetOutput("plan", map[string]any{
		"steps": []any{
			map[string]any{"step_id": "s1", "tool": "read_file", "parameters": map[string]any{"path": "main.go"}},
		},
	})

	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	output := result.GetOutput("executor")
	assert.NotNil(t, output["results"])
	assert.True(t, output["all_succeeded"].(bool))
}

func TestProcessToolAgentNoAccessDenied(t *testing.T) {
	// Test tool access is denied when ToolAccessNone.
	cfg := createTestAgentConfig("executor")
	cfg.HasTools = true
	cfg.ToolAccess = config.ToolAccessNone
	cfg.DefaultNext = "end"
	logger := &MockLogger{}
	tools := &MockToolExecutor{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, tools)

	env := envelope.CreateGenericEnvelope("Read file", "user", "session", nil, nil, nil)
	env.SetOutput("plan", map[string]any{
		"steps": []any{
			map[string]any{"step_id": "s1", "tool": "read_file", "parameters": map[string]any{}},
		},
	})

	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	output := result.GetOutput("executor")
	results := output["results"].([]map[string]any)
	assert.Len(t, results, 1)
	assert.Equal(t, "error", results[0]["status"])
	assert.Contains(t, results[0]["error"].(string), "access denied")
}

func TestProcessToolAgentNoPlan(t *testing.T) {
	// Test tool agent with no plan returns empty results.
	cfg := createTestAgentConfig("executor")
	cfg.HasTools = true
	cfg.ToolAccess = config.ToolAccessAll
	cfg.DefaultNext = "end"
	logger := &MockLogger{}
	tools := &MockToolExecutor{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, tools)

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	// No plan output

	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	output := result.GetOutput("executor")
	results := output["results"].([]any)
	assert.Empty(t, results)
	assert.True(t, output["all_succeeded"].(bool))
}

// =============================================================================
// ROUTING TESTS
// =============================================================================

func TestRoutingRules(t *testing.T) {
	// Test routing rules evaluation.
	cfg := createTestAgentConfig("router")
	cfg.RoutingRules = []config.RoutingRule{
		{Condition: "needs_clarification", Value: true, Target: "clarification_stage"},
		{Condition: "is_complete", Value: true, Target: "end"},
	}
	cfg.DefaultNext = "default_next"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)
	agent.UseMock = true
	agent.MockHandler = func(env *envelope.GenericEnvelope) (map[string]any, error) {
		return map[string]any{"needs_clarification": true}, nil
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	assert.Equal(t, "clarification_stage", result.CurrentStage)
}

func TestRoutingDefaultNext(t *testing.T) {
	// Test default routing when no rules match.
	cfg := createTestAgentConfig("router")
	cfg.RoutingRules = []config.RoutingRule{
		{Condition: "special_case", Value: true, Target: "special"},
	}
	cfg.DefaultNext = "default_stage"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)
	agent.UseMock = true
	agent.MockHandler = func(env *envelope.GenericEnvelope) (map[string]any, error) {
		return map[string]any{"normal_output": true}, nil
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	assert.Equal(t, "default_stage", result.CurrentStage)
}

func TestRoutingNoDefaultEnds(t *testing.T) {
	// Test that no default routing ends pipeline.
	cfg := createTestAgentConfig("router")
	// No routing rules, no default next
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
}

// =============================================================================
// OUTPUT VALIDATION TESTS
// =============================================================================

func TestRequiredOutputFields(t *testing.T) {
	// Test required output field validation.
	cfg := createTestAgentConfig("validator")
	cfg.RequiredOutputFields = []string{"intent", "confidence"}
	cfg.ErrorNext = "error_stage"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)
	agent.UseMock = true
	agent.MockHandler = func(env *envelope.GenericEnvelope) (map[string]any, error) {
		return map[string]any{"intent": "analyze"}, nil // Missing confidence
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err) // Error routed
	assert.Equal(t, "error_stage", result.CurrentStage)
	assert.Len(t, result.Errors, 1)
	assert.Contains(t, result.Errors[0]["error"].(string), "confidence")
}

func TestRequiredOutputFieldsPass(t *testing.T) {
	// Test required output fields when all present.
	cfg := createTestAgentConfig("validator")
	cfg.RequiredOutputFields = []string{"intent", "confidence"}
	cfg.DefaultNext = "end"
	logger := &MockLogger{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)
	agent.UseMock = true
	agent.MockHandler = func(env *envelope.GenericEnvelope) (map[string]any, error) {
		return map[string]any{"intent": "analyze", "confidence": 0.9}, nil
	}

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	result, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	assert.Equal(t, "end", result.CurrentStage)
	assert.Empty(t, result.Errors)
}

// =============================================================================
// EVENT CONTEXT TESTS
// =============================================================================

func TestEventContextEmitsEvents(t *testing.T) {
	// Test that event context receives events.
	cfg := createTestAgentConfig("event_agent")
	cfg.DefaultNext = "end"
	logger := &MockLogger{}
	events := &MockEventContext{}

	agent, _ := NewUnifiedAgent(cfg, logger, nil, nil)
	agent.SetEventContext(events)

	env := envelope.CreateGenericEnvelope("Test", "user", "session", nil, nil, nil)
	_, err := agent.Process(context.Background(), env)

	require.NoError(t, err)
	assert.Contains(t, events.startedAgents, "event_agent")
	assert.Contains(t, events.completedAgents, "event_agent")
}

// =============================================================================
// HELPER FUNCTION TESTS
// =============================================================================

func TestTruncate(t *testing.T) {
	// Test truncate helper.
	assert.Equal(t, "hello", truncate("hello", 10))
	assert.Equal(t, "hel...", truncate("hello world", 3))
	assert.Equal(t, "", truncate("", 10))
}

func TestExtractAndParseJSON(t *testing.T) {
	// Test JSON extraction from text.

	// Direct JSON
	result, err := extractAndParseJSON(`{"key": "value"}`)
	require.NoError(t, err)
	assert.Equal(t, "value", result["key"])

	// JSON embedded in text
	result2, err2 := extractAndParseJSON(`Here is some text before {"result": 42} and after.`)
	require.NoError(t, err2)
	assert.Equal(t, float64(42), result2["result"])

	// No JSON
	_, err3 := extractAndParseJSON("no json here")
	require.Error(t, err3)
}
