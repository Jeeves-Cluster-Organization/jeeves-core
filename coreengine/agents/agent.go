// Package agents provides the Agent - single agent class driven by configuration.
package agents

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/observability"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/codes"
)

// LLMProvider is the interface for LLM providers.
type LLMProvider interface {
	Generate(ctx context.Context, model string, prompt string, options map[string]any) (string, error)
}

// ToolExecutor is the interface for tool execution.
type ToolExecutor interface {
	Execute(ctx context.Context, toolName string, params map[string]any) (map[string]any, error)
}

// Logger is the interface for logging.
type Logger interface {
	Info(msg string, fields ...any)
	Debug(msg string, fields ...any)
	Warn(msg string, fields ...any)
	Error(msg string, fields ...any)
	Bind(fields ...any) Logger
}

// EventContext is the interface for event emission.
type EventContext interface {
	EmitAgentStarted(agentName string) error
	EmitAgentCompleted(agentName string, status string, durationMS int, err error) error
}

// PromptRegistry is the interface for prompt lookup.
type PromptRegistry interface {
	Get(key string, context map[string]any) (string, error)
}

// ProcessHook is a function called during processing.
type ProcessHook func(env *envelope.Envelope) (*envelope.Envelope, error)

// OutputHook is a function called with output.
type OutputHook func(env *envelope.Envelope, output map[string]any) (*envelope.Envelope, error)

// MockHandler generates mock output for testing.
type MockHandler func(env *envelope.Envelope) (map[string]any, error)

var tracer = otel.Tracer("jeeves-core/agents")

// Agent is the single agent class that handles all agent types via configuration.
type Agent struct {
	Config         *config.AgentConfig
	Name           string
	Logger         Logger
	LLM            LLMProvider
	Tools          ToolExecutor
	EventCtx       EventContext
	PromptRegistry PromptRegistry
	UseMock        bool

	// Hooks (set by capability layer)
	PreProcess  ProcessHook
	PostProcess OutputHook
	MockHandler MockHandler
}

// NewAgent creates a new Agent.
func NewAgent(
	cfg *config.AgentConfig,
	logger Logger,
	llm LLMProvider,
	tools ToolExecutor,
) (*Agent, error) {
	if err := cfg.Validate(); err != nil {
		return nil, err
	}

	if cfg.HasLLM && llm == nil {
		return nil, fmt.Errorf("agent '%s' has_llm=true but no llm_provider", cfg.Name)
	}
	if cfg.HasTools && tools == nil {
		return nil, fmt.Errorf("agent '%s' has_tools=true but no tool_executor", cfg.Name)
	}

	return &Agent{
		Config: cfg,
		Name:   cfg.Name,
		Logger: logger.Bind("agent", cfg.Name),
		LLM:    llm,
		Tools:  tools,
	}, nil
}

// SetEventContext sets the event context for this agent.
func (a *Agent) SetEventContext(ctx EventContext) {
	a.EventCtx = ctx
}

// Process processes an envelope through this agent.
func (a *Agent) Process(ctx context.Context, env *envelope.Envelope) (*envelope.Envelope, error) {
	// Create tracing span
	ctx, span := tracer.Start(ctx, "agent.process",
		attribute.String("jeeves.agent.name", a.Name),
		attribute.String("jeeves.request.id", env.RequestID),
	)
	defer span.End()

	startTime := time.Now()
	llmCalls := 0

	// Record start
	env.RecordAgentStart(a.Name, a.Config.StageOrder)
	a.emitStarted()
	a.Logger.Info(fmt.Sprintf("%s_started", a.Name))

	var output map[string]any
	var err error

	defer func() {
		durationMS := int(time.Since(startTime).Milliseconds())

		// Set span attributes
		span.SetAttributes(
			attribute.Int("jeeves.llm.calls", llmCalls),
			attribute.Int("duration_ms", durationMS),
		)

		if err != nil {
			observability.RecordAgentExecution(a.Name, "error", durationMS)
			span.RecordError(err)
			span.SetStatus(codes.Error, err.Error())
			a.Logger.Error(fmt.Sprintf("%s_error", a.Name), "error", err.Error(), "duration_ms", durationMS)
			errStr := err.Error()
			env.RecordAgentComplete(a.Name, "error", &errStr, llmCalls, durationMS)
			a.emitCompleted("error", durationMS, err)
		} else {
			observability.RecordAgentExecution(a.Name, "success", durationMS)
			span.SetStatus(codes.Ok, "success")
			a.Logger.Info(fmt.Sprintf("%s_completed", a.Name), "duration_ms", durationMS, "next_stage", env.CurrentStage)
			env.RecordAgentComplete(a.Name, "success", nil, llmCalls, durationMS)
			a.emitCompleted("success", durationMS, nil)
		}
	}()

	// Pre-process hook
	if a.PreProcess != nil {
		env, err = a.PreProcess(env)
		if err != nil {
			return a.handleError(env, err)
		}
	}

	// Main processing based on capabilities
	if a.UseMock && a.MockHandler != nil {
		output, err = a.MockHandler(env)
	} else if a.Config.HasLLM {
		output, err = a.llmProcess(ctx, env)
		llmCalls = 1
	} else if a.Config.HasTools {
		output, err = a.toolProcess(ctx, env)
	} else {
		output, err = a.serviceProcess(ctx, env)
	}

	if err != nil {
		return a.handleError(env, err)
	}

	// Validate required fields
	if err = a.validateOutput(output); err != nil {
		return a.handleError(env, err)
	}

	// Store output in envelope
	env.SetOutput(a.Config.OutputKey, output)

	// Post-process hook
	if a.PostProcess != nil {
		env, err = a.PostProcess(env, output)
		if err != nil {
			return a.handleError(env, err)
		}
	}

	// Determine next stage from routing rules
	nextStage := a.evaluateRouting(output)
	env.CurrentStage = nextStage

	return env, nil
}

func (a *Agent) llmProcess(ctx context.Context, env *envelope.Envelope) (map[string]any, error) {
	// Build prompt
	prompt := a.buildPrompt(env)

	// Build options
	options := map[string]any{
		"num_predict": 2000,
		"num_ctx":     16384,
	}
	if a.Config.MaxTokens != nil {
		options["num_predict"] = *a.Config.MaxTokens
	}
	if a.Config.Temperature != nil {
		options["temperature"] = *a.Config.Temperature
	}

	// Call LLM
	model := a.Config.ModelRole
	if model == "" {
		model = "default"
	}

	responseText, err := a.LLM.Generate(ctx, model, prompt, options)
	if err != nil {
		return nil, fmt.Errorf("llm generation failed: %w", err)
	}

	a.Logger.Debug(fmt.Sprintf("%s_llm_response", a.Name),
		"response_length", len(responseText),
		"response_preview", truncate(responseText, 200),
	)

	// Parse JSON response
	output, err := extractAndParseJSON(responseText)
	if err != nil {
		return nil, fmt.Errorf("json parsing failed: %w", err)
	}

	return output, nil
}

func (a *Agent) toolProcess(ctx context.Context, env *envelope.Envelope) (map[string]any, error) {
	plan := env.GetOutput("plan")
	if plan == nil {
		return map[string]any{
			"results":       []any{},
			"total_time_ms": 0,
			"all_succeeded": true,
		}, nil
	}

	steps, _ := plan["steps"].([]any)
	results := make([]map[string]any, 0, len(steps))
	totalTimeMS := 0

	for i, stepAny := range steps {
		step, ok := stepAny.(map[string]any)
		if !ok {
			continue
		}

		toolName, _ := step["tool"].(string)
		params, _ := step["parameters"].(map[string]any)
		if params == nil {
			params = make(map[string]any)
		}
		stepID, ok := step["step_id"].(string)
		if !ok {
			stepID = fmt.Sprintf("step_%d", i)
		}

		// Check tool access
		if !a.canAccessTool(toolName) {
			results = append(results, map[string]any{
				"step_id": stepID,
				"tool":    toolName,
				"status":  "error",
				"error":   fmt.Sprintf("Tool access denied: %s", toolName),
			})
			continue
		}

		start := time.Now()
		result, err := a.Tools.Execute(ctx, toolName, params)
		execTime := int(time.Since(start).Milliseconds())
		totalTimeMS += execTime

		if err != nil {
			results = append(results, map[string]any{
				"step_id":           stepID,
				"tool":              toolName,
				"parameters":        params,
				"status":            "error",
				"error":             map[string]any{"message": err.Error(), "type": "ExecutionError"},
				"execution_time_ms": execTime,
			})
		} else {
			data := result
			if d, ok := result["data"]; ok {
				data = d.(map[string]any)
			}
			results = append(results, map[string]any{
				"step_id":           stepID,
				"tool":              toolName,
				"parameters":        params,
				"status":            "success",
				"data":              data,
				"execution_time_ms": execTime,
			})
		}
	}

	allSucceeded := true
	for _, r := range results {
		if r["status"] != "success" {
			allSucceeded = false
			break
		}
	}

	return map[string]any{
		"results":       results,
		"total_time_ms": totalTimeMS,
		"all_succeeded": allSucceeded,
	}, nil
}

func (a *Agent) serviceProcess(ctx context.Context, env *envelope.Envelope) (map[string]any, error) {
	// Default service processing returns empty output
	// Capability layer hooks add actual logic
	return map[string]any{}, nil
}

func (a *Agent) buildPrompt(env *envelope.Envelope) string {
	// Registry lookup
	if a.PromptRegistry != nil && a.Config.PromptKey != "" {
		context := a.extractPromptContext(env)
		prompt, err := a.PromptRegistry.Get(a.Config.PromptKey, context)
		if err == nil {
			return prompt
		}
		a.Logger.Warn(fmt.Sprintf("%s_prompt_registry_error", a.Name), "error", err.Error(), "key", a.Config.PromptKey)
	}

	// Fallback: minimal prompt
	return fmt.Sprintf("Process this request: %s", env.RawInput)
}

func (a *Agent) extractPromptContext(env *envelope.Envelope) map[string]any {
	context := map[string]any{
		"raw_input":  env.RawInput,
		"user_id":    env.UserID,
		"session_id": env.SessionID,
	}

	// Add outputs from previous stages
	for key, value := range env.Outputs {
		context[key] = value
	}

	return context
}

func (a *Agent) canAccessTool(toolName string) bool {
	if a.Config.ToolAccess == config.ToolAccessNone {
		return false
	}
	if a.Config.ToolAccess == config.ToolAccessAll {
		return true
	}
	if a.Config.AllowedTools != nil {
		return a.Config.AllowedTools[toolName]
	}
	return true
}

func (a *Agent) validateOutput(output map[string]any) error {
	for _, field := range a.Config.RequiredOutputFields {
		if _, exists := output[field]; !exists {
			return fmt.Errorf("agent '%s' output missing required field: %s", a.Name, field)
		}
	}
	return nil
}

func (a *Agent) evaluateRouting(output map[string]any) string {
	// Evaluate rules in order
	for _, rule := range a.Config.RoutingRules {
		value, exists := output[rule.Condition]
		if exists && value == rule.Value {
			a.Logger.Debug(fmt.Sprintf("%s_routing", a.Name),
				"condition", rule.Condition,
				"value", value,
				"target", rule.Target,
			)
			return rule.Target
		}
	}

	// Default next stage
	if a.Config.DefaultNext != "" {
		return a.Config.DefaultNext
	}
	return "end"
}

func (a *Agent) handleError(env *envelope.Envelope, err error) (*envelope.Envelope, error) {
	// Record error
	env.Errors = append(env.Errors, map[string]any{
		"agent":      a.Name,
		"error":      err.Error(),
		"error_type": "ProcessingError",
		"timestamp":  time.Now().UTC().Format(time.RFC3339),
	})

	// Route to error stage if configured
	if a.Config.ErrorNext != "" {
		env.CurrentStage = a.Config.ErrorNext
		return env, nil
	}

	return env, err
}

func (a *Agent) emitStarted() {
	if a.EventCtx != nil {
		_ = a.EventCtx.EmitAgentStarted(a.Name)
	}
}

func (a *Agent) emitCompleted(status string, durationMS int, err error) {
	if a.EventCtx != nil {
		_ = a.EventCtx.EmitAgentCompleted(a.Name, status, durationMS, err)
	}
}

// Helper functions

func truncate(s string, maxLen int) string {
	if len(s) <= maxLen {
		return s
	}
	return s[:maxLen] + "..."
}

func extractAndParseJSON(text string) (map[string]any, error) {
	// Try direct parse first
	var result map[string]any
	if err := json.Unmarshal([]byte(text), &result); err == nil {
		return result, nil
	}

	// Try to find JSON object in text
	start := -1
	braceCount := 0
	for i, c := range text {
		if c == '{' {
			if start == -1 {
				start = i
			}
			braceCount++
		} else if c == '}' {
			braceCount--
			if braceCount == 0 && start != -1 {
				jsonStr := text[start : i+1]
				if err := json.Unmarshal([]byte(jsonStr), &result); err == nil {
					return result, nil
				}
			}
		}
	}

	return nil, fmt.Errorf("no valid JSON object found in response")
}
