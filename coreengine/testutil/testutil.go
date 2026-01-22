// Package testutil provides shared test utilities and mocks for integration tests.
//
// All mocks in this package are designed for testing the coreengine components
// in isolation without requiring external dependencies.
package testutil

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/agents"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// =============================================================================
// MOCK LLM PROVIDER
// =============================================================================

// MockLLMProvider implements agents.LLMProvider for testing.
// Configure responses by prompt prefix or use DefaultResponse.
type MockLLMProvider struct {
	// Responses maps prompt prefixes to responses.
	// First matching prefix wins.
	Responses map[string]string

	// DefaultResponse is returned when no prefix matches.
	DefaultResponse string

	// Delay simulates LLM latency.
	Delay time.Duration

	// Error causes Generate to return this error.
	Error error

	// CallCount tracks the number of Generate calls.
	CallCount int

	// Calls records all calls for assertion.
	Calls []LLMCall

	// GenerateFunc allows custom generation logic.
	// If set, this is called instead of using Responses.
	GenerateFunc func(context.Context, string, string, map[string]any) (string, error)

	mu sync.Mutex
}

// LLMCall records a single LLM call for assertion.
type LLMCall struct {
	Model   string
	Prompt  string
	Options map[string]any
}

// NewMockLLMProvider creates a MockLLMProvider with sensible defaults.
func NewMockLLMProvider() *MockLLMProvider {
	return &MockLLMProvider{
		Responses:       make(map[string]string),
		DefaultResponse: `{"verdict": "proceed", "reasoning": "Mock response"}`,
	}
}

// Generate implements agents.LLMProvider.
func (m *MockLLMProvider) Generate(ctx context.Context, model string, prompt string, options map[string]any) (string, error) {
	m.mu.Lock()
	m.CallCount++
	m.Calls = append(m.Calls, LLMCall{Model: model, Prompt: prompt, Options: options})
	customFunc := m.GenerateFunc
	m.mu.Unlock()

	// If custom function is set, use it
	if customFunc != nil {
		return customFunc(ctx, model, prompt, options)
	}

	if m.Delay > 0 {
		select {
		case <-time.After(m.Delay):
		case <-ctx.Done():
			return "", ctx.Err()
		}
	}

	if m.Error != nil {
		return "", m.Error
	}

	// Check prefix matches
	for prefix, response := range m.Responses {
		if len(prompt) >= len(prefix) && prompt[:len(prefix)] == prefix {
			return response, nil
		}
	}

	return m.DefaultResponse, nil
}

// WithResponse adds a prefix-based response.
func (m *MockLLMProvider) WithResponse(prefix, response string) *MockLLMProvider {
	m.Responses[prefix] = response
	return m
}

// WithError configures the mock to return an error.
func (m *MockLLMProvider) WithError(err error) *MockLLMProvider {
	m.Error = err
	return m
}

// WithDelay adds latency simulation.
func (m *MockLLMProvider) WithDelay(d time.Duration) *MockLLMProvider {
	m.Delay = d
	return m
}

// GetCallCount returns the number of calls (thread-safe).
func (m *MockLLMProvider) GetCallCount() int {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.CallCount
}

// GenerateDefault provides a simple response without context/delay simulation.
// Useful for testing delegation patterns and mock setup.
func (m *MockLLMProvider) GenerateDefault(prompt string) (string, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if m.Error != nil {
		return "", m.Error
	}

	// Check prefix matches
	for prefix, response := range m.Responses {
		if len(prompt) >= len(prefix) && prompt[:len(prefix)] == prefix {
			return response, nil
		}
	}

	// Default
	if m.DefaultResponse != "" {
		return m.DefaultResponse, nil
	}

	return fmt.Sprintf(`{"verdict": "proceed", "reasoning": "mock response"}`), nil
}

// Reset clears call history.
func (m *MockLLMProvider) Reset() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.CallCount = 0
	m.Calls = nil
}

// =============================================================================
// MOCK TOOL EXECUTOR
// =============================================================================

// MockToolExecutor implements the tool executor interface for testing.
type MockToolExecutor struct {
	// Results maps tool names to their results.
	Results map[string]map[string]any

	// Errors maps tool names to errors they should return.
	Errors map[string]error

	// Delay simulates tool execution latency.
	Delay time.Duration

	// CallCount tracks the number of Execute calls.
	CallCount int

	// Calls records all calls for assertion.
	Calls []ToolCall

	mu sync.Mutex
}

// ToolCall records a single tool execution for assertion.
type ToolCall struct {
	ToolName string
	Params   map[string]any
}

// NewMockToolExecutor creates a MockToolExecutor with sensible defaults.
func NewMockToolExecutor() *MockToolExecutor {
	return &MockToolExecutor{
		Results: make(map[string]map[string]any),
		Errors:  make(map[string]error),
	}
}

// Execute implements the tool executor interface.
func (m *MockToolExecutor) Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error) {
	m.mu.Lock()
	m.CallCount++
	m.Calls = append(m.Calls, ToolCall{ToolName: toolName, Params: params})
	m.mu.Unlock()

	if m.Delay > 0 {
		select {
		case <-time.After(m.Delay):
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}

	if err, exists := m.Errors[toolName]; exists {
		return nil, err
	}

	if result, exists := m.Results[toolName]; exists {
		return result, nil
	}

	// Default success response
	return map[string]any{
		"status": "success",
		"data":   map[string]any{"tool": toolName},
	}, nil
}

// WithResult adds a tool result.
func (m *MockToolExecutor) WithResult(toolName string, result map[string]any) *MockToolExecutor {
	m.Results[toolName] = result
	return m
}

// WithError configures a tool to return an error.
func (m *MockToolExecutor) WithError(toolName string, err error) *MockToolExecutor {
	m.Errors[toolName] = err
	return m
}

// WithDelay adds latency simulation.
func (m *MockToolExecutor) WithDelay(d time.Duration) *MockToolExecutor {
	m.Delay = d
	return m
}

// GetCallCount returns the number of calls (thread-safe).
func (m *MockToolExecutor) GetCallCount() int {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.CallCount
}

// Reset clears call history.
func (m *MockToolExecutor) Reset() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.CallCount = 0
	m.Calls = nil
}

// =============================================================================
// MOCK PERSISTENCE
// =============================================================================

// MockPersistence implements state persistence for testing.
type MockPersistence struct {
	// States stores all saved states by thread ID.
	States map[string]map[string]any

	// SaveError causes SaveState to return this error.
	SaveError error

	// LoadError causes LoadState to return this error.
	LoadError error

	// SaveCount tracks number of saves.
	SaveCount int

	// LoadCount tracks number of loads.
	LoadCount int

	mu sync.RWMutex
}

// NewMockPersistence creates a MockPersistence.
func NewMockPersistence() *MockPersistence {
	return &MockPersistence{
		States: make(map[string]map[string]any),
	}
}

// SaveState saves state for a thread.
func (m *MockPersistence) SaveState(ctx context.Context, threadID string, state map[string]any) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.SaveCount++

	if m.SaveError != nil {
		return m.SaveError
	}

	// Deep copy the state
	copied := make(map[string]any)
	data, _ := json.Marshal(state)
	json.Unmarshal(data, &copied)

	m.States[threadID] = copied
	return nil
}

// LoadState loads state for a thread.
func (m *MockPersistence) LoadState(ctx context.Context, threadID string) (map[string]any, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	m.LoadCount++

	if m.LoadError != nil {
		return nil, m.LoadError
	}

	state, exists := m.States[threadID]
	if !exists {
		return nil, nil
	}

	// Return a copy
	copied := make(map[string]any)
	data, _ := json.Marshal(state)
	json.Unmarshal(data, &copied)

	return copied, nil
}

// WithSaveError configures save to fail.
func (m *MockPersistence) WithSaveError(err error) *MockPersistence {
	m.SaveError = err
	return m
}

// WithLoadError configures load to fail.
func (m *MockPersistence) WithLoadError(err error) *MockPersistence {
	m.LoadError = err
	return m
}

// GetState returns saved state for assertion (thread-safe).
func (m *MockPersistence) GetState(threadID string) map[string]any {
	m.mu.RLock()
	defer m.mu.RUnlock()
	return m.States[threadID]
}

// Clear removes all saved states.
func (m *MockPersistence) Clear() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.States = make(map[string]map[string]any)
	m.SaveCount = 0
	m.LoadCount = 0
}

// =============================================================================
// MOCK EVENT CONTEXT
// =============================================================================

// MockEventContext captures agent events for assertion.
type MockEventContext struct {
	// Events captures all emitted events.
	Events []AgentEvent

	// Error causes emit methods to return this error.
	Error error

	mu sync.Mutex
}

// AgentEvent represents a captured event.
type AgentEvent struct {
	Type       string
	AgentName  string
	Status     string
	DurationMS int
	Error      error
	Timestamp  time.Time
}

// NewMockEventContext creates a MockEventContext.
func NewMockEventContext() *MockEventContext {
	return &MockEventContext{
		Events: make([]AgentEvent, 0),
	}
}

// EmitAgentStarted records an agent started event.
func (m *MockEventContext) EmitAgentStarted(agentName string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	if m.Error != nil {
		return m.Error
	}

	m.Events = append(m.Events, AgentEvent{
		Type:      "started",
		AgentName: agentName,
		Timestamp: time.Now(),
	})
	return nil
}

// EmitAgentCompleted records an agent completed event.
func (m *MockEventContext) EmitAgentCompleted(agentName string, status string, durationMS int, err error) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	if m.Error != nil {
		return m.Error
	}

	m.Events = append(m.Events, AgentEvent{
		Type:       "completed",
		AgentName:  agentName,
		Status:     status,
		DurationMS: durationMS,
		Error:      err,
		Timestamp:  time.Now(),
	})
	return nil
}

// GetEvents returns a copy of captured events (thread-safe).
func (m *MockEventContext) GetEvents() []AgentEvent {
	m.mu.Lock()
	defer m.mu.Unlock()

	copied := make([]AgentEvent, len(m.Events))
	copy(copied, m.Events)
	return copied
}

// GetStartedAgents returns names of agents that were started.
func (m *MockEventContext) GetStartedAgents() []string {
	m.mu.Lock()
	defer m.mu.Unlock()

	var names []string
	for _, e := range m.Events {
		if e.Type == "started" {
			names = append(names, e.AgentName)
		}
	}
	return names
}

// GetCompletedAgents returns names of agents that completed.
func (m *MockEventContext) GetCompletedAgents() []string {
	m.mu.Lock()
	defer m.mu.Unlock()

	var names []string
	for _, e := range m.Events {
		if e.Type == "completed" {
			names = append(names, e.AgentName)
		}
	}
	return names
}

// Clear removes all captured events.
func (m *MockEventContext) Clear() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.Events = nil
}

// =============================================================================
// MOCK LOGGER
// =============================================================================

// MockLogger implements agents.Logger for testing.
type MockLogger struct {
	// Logs captures all log entries.
	Logs []LogEntry

	mu sync.Mutex
}

// LogEntry represents a captured log entry.
type LogEntry struct {
	Level   string
	Message string
	Fields  map[string]any
}

// NewMockLogger creates a MockLogger.
func NewMockLogger() *MockLogger {
	return &MockLogger{
		Logs: make([]LogEntry, 0),
	}
}

func (m *MockLogger) Debug(msg string, keysAndValues ...any) {
	m.log("debug", msg, keysAndValues...)
}

func (m *MockLogger) Info(msg string, keysAndValues ...any) {
	m.log("info", msg, keysAndValues...)
}

func (m *MockLogger) Warn(msg string, keysAndValues ...any) {
	m.log("warn", msg, keysAndValues...)
}

func (m *MockLogger) Error(msg string, keysAndValues ...any) {
	m.log("error", msg, keysAndValues...)
}

func (m *MockLogger) Bind(fields ...any) agents.Logger {
	return m
}

func (m *MockLogger) log(level, msg string, keysAndValues ...any) {
	m.mu.Lock()
	defer m.mu.Unlock()

	fields := make(map[string]any)
	for i := 0; i < len(keysAndValues)-1; i += 2 {
		if key, ok := keysAndValues[i].(string); ok {
			fields[key] = keysAndValues[i+1]
		}
	}

	m.Logs = append(m.Logs, LogEntry{
		Level:   level,
		Message: msg,
		Fields:  fields,
	})
}

// GetLogs returns captured logs (thread-safe).
func (m *MockLogger) GetLogs() []LogEntry {
	m.mu.Lock()
	defer m.mu.Unlock()

	copied := make([]LogEntry, len(m.Logs))
	copy(copied, m.Logs)
	return copied
}

// HasLog checks if a log message exists at the given level.
func (m *MockLogger) HasLog(level, message string) bool {
	m.mu.Lock()
	defer m.mu.Unlock()

	for _, log := range m.Logs {
		if log.Level == level && log.Message == message {
			return true
		}
	}
	return false
}

// Clear removes all captured logs.
func (m *MockLogger) Clear() {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.Logs = nil
}

// =============================================================================
// PIPELINE CONFIG HELPERS
// =============================================================================

// NewTestPipelineConfig creates a pipeline config for testing.
// Stages are created in order with linear routing (A → B → C → end).
func NewTestPipelineConfig(name string, stages ...string) *config.PipelineConfig {
	if len(stages) == 0 {
		stages = []string{"stageA", "stageB", "stageC"}
	}

	agentConfigs := make([]*config.AgentConfig, len(stages))
	for i, stage := range stages {
		var defaultNext string
		if i < len(stages)-1 {
			defaultNext = stages[i+1]
		} else {
			defaultNext = "end"
		}

		agentConfigs[i] = &config.AgentConfig{
			Name:        stage,
			StageOrder:  i + 1,
			HasLLM:      true,
			ModelRole:   "default",
			DefaultNext: defaultNext,
		}
	}

	return &config.PipelineConfig{
		Name:          name,
		Agents:        agentConfigs,
		MaxIterations: 5,
		MaxLLMCalls:   20,
		MaxAgentHops:  30,
	}
}

// NewTestPipelineConfigWithCycle creates a pipeline with cyclic routing.
// The last stage routes back to the first on "loop_back" verdict.
func NewTestPipelineConfigWithCycle(name string, maxLoops int, stages ...string) *config.PipelineConfig {
	cfg := NewTestPipelineConfig(name, stages...)

	if len(cfg.Agents) > 0 {
		lastAgent := cfg.Agents[len(cfg.Agents)-1]
		lastAgent.RoutingRules = []config.RoutingRule{
			{Condition: "verdict", Value: "proceed", Target: "end"},
			{Condition: "verdict", Value: "loop_back", Target: stages[0]},
		}
		lastAgent.DefaultNext = "end"

		// Add edge limit for the cycle
		cfg.EdgeLimits = []config.EdgeLimit{
			{From: stages[len(stages)-1], To: stages[0], MaxCount: maxLoops},
		}
	}

	return cfg
}

// NewEmptyPipelineConfig creates a pipeline config with no agents.
// Useful for testing runtime behavior with empty pipelines or as a base for custom agents.
func NewEmptyPipelineConfig(name string) *config.PipelineConfig {
	cfg := &config.PipelineConfig{
		Name:                     name,
		MaxIterations:            5,
		MaxLLMCalls:              20,
		MaxAgentHops:             30,
		Agents:                   []*config.AgentConfig{}, // Explicitly empty
		DefaultRunMode:           config.RunModeSequential,
		EdgeLimits:               []config.EdgeLimit{},
		ClarificationResumeStage: "",
		ConfirmationResumeStage:  "",
	}
	return cfg
}

// NewParallelPipelineConfig creates a pipeline config for parallel execution testing.
// All stages have StageOrder=1 (execute concurrently).
func NewParallelPipelineConfig(name string, stages ...string) *config.PipelineConfig {
	if len(stages) == 0 {
		stages = []string{"stageA", "stageB", "stageC"}
	}

	agentConfigs := make([]*config.AgentConfig, len(stages))
	for i, stage := range stages {
		agentConfigs[i] = &config.AgentConfig{
			Name:         stage,
			HasLLM:       true,
			ModelRole:    "default",
			ToolAccess:   config.ToolAccessNone,
			StageOrder:   1, // All stages same order = parallel
			DefaultNext:  "end",
			RoutingRules: []config.RoutingRule{},
			Requires:     []string{},
		}
	}

	return &config.PipelineConfig{
		Name:                     name,
		MaxIterations:            5,
		MaxLLMCalls:              20,
		MaxAgentHops:             30,
		Agents:                   agentConfigs,
		DefaultRunMode:           config.RunModeParallel,
		EdgeLimits:               []config.EdgeLimit{},
		ClarificationResumeStage: "",
		ConfirmationResumeStage:  "",
	}
}

// NewBoundedPipelineConfig creates a pipeline with custom iteration/LLM/hop bounds.
// Uses linear routing (A → B → C → end).
func NewBoundedPipelineConfig(name string, maxIterations, maxLLMCalls, maxAgentHops int, stages ...string) *config.PipelineConfig {
	cfg := NewTestPipelineConfig(name, stages...)
	cfg.MaxIterations = maxIterations
	cfg.MaxLLMCalls = maxLLMCalls
	cfg.MaxAgentHops = maxAgentHops
	return cfg
}

// NewDependencyChainConfig creates a pipeline where each stage depends on the previous one.
// Useful for testing parallel execution with sequential dependencies (Run Mode: Parallel, but stages wait).
func NewDependencyChainConfig(name string, stages ...string) *config.PipelineConfig {
	if len(stages) == 0 {
		stages = []string{"stageA", "stageB", "stageC"}
	}

	agentConfigs := make([]*config.AgentConfig, len(stages))
	for i, stage := range stages {
		requires := []string{}
		if i > 0 {
			requires = []string{stages[i-1]} // Depend on previous stage
		}

		agentConfigs[i] = &config.AgentConfig{
			Name:         stage,
			HasLLM:       true,
			ModelRole:    "default",
			ToolAccess:   config.ToolAccessNone,
			StageOrder:   1, // All same order for parallel mode
			DefaultNext:  "end",
			RoutingRules: []config.RoutingRule{},
			Requires:     requires,
		}
	}

	return &config.PipelineConfig{
		Name:                     name,
		MaxIterations:            5,
		MaxLLMCalls:              20,
		MaxAgentHops:             30,
		Agents:                   agentConfigs,
		DefaultRunMode:           config.RunModeParallel,
		EdgeLimits:               []config.EdgeLimit{},
		ClarificationResumeStage: "",
		ConfirmationResumeStage:  "",
	}
}

// =============================================================================
// ENVELOPE HELPERS
// =============================================================================

// NewTestEnvelope creates an envelope with test defaults.
func NewTestEnvelope(input string) *envelope.GenericEnvelope {
	return envelope.CreateGenericEnvelope(
		input,
		"test-user",
		"test-session",
		nil,
		nil,
		nil,
	)
}

// NewTestEnvelopeWithStages creates an envelope with specified stage order.
func NewTestEnvelopeWithStages(input string, stages []string) *envelope.GenericEnvelope {
	env := NewTestEnvelope(input)
	env.StageOrder = stages
	if len(stages) > 0 {
		env.CurrentStage = stages[0]
	}
	return env
}

// =============================================================================
// ASSERTION HELPERS
// =============================================================================

// AssertEnvelopeCompleted checks that envelope reached "end" stage.
func AssertEnvelopeCompleted(env *envelope.GenericEnvelope) error {
	if env.CurrentStage != "end" {
		return fmt.Errorf("expected current_stage='end', got '%s'", env.CurrentStage)
	}
	return nil
}

// AssertEnvelopeTerminated checks that envelope is terminated.
func AssertEnvelopeTerminated(env *envelope.GenericEnvelope) error {
	if !env.Terminated {
		return fmt.Errorf("expected terminated=true, got false")
	}
	return nil
}

// AssertEnvelopeHasOutput checks that envelope has output for an agent.
func AssertEnvelopeHasOutput(env *envelope.GenericEnvelope, agentName string) error {
	if _, exists := env.Outputs[agentName]; !exists {
		return fmt.Errorf("expected output for agent '%s', not found", agentName)
	}
	return nil
}

// AssertNoInterrupt checks that no interrupt is pending.
func AssertNoInterrupt(env *envelope.GenericEnvelope) error {
	if env.InterruptPending {
		return fmt.Errorf("expected no interrupt, but interrupt is pending")
	}
	return nil
}

// AssertInterruptPending checks that an interrupt is pending.
func AssertInterruptPending(env *envelope.GenericEnvelope, kind envelope.InterruptKind) error {
	if !env.InterruptPending {
		return fmt.Errorf("expected interrupt pending, but none found")
	}
	if env.Interrupt == nil {
		return fmt.Errorf("interrupt pending but Interrupt is nil")
	}
	if env.Interrupt.Kind != kind {
		return fmt.Errorf("expected interrupt kind '%s', got '%s'", kind, env.Interrupt.Kind)
	}
	return nil
}
