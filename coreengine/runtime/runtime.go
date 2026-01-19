// Package runtime provides the UnifiedRuntime - pipeline orchestration engine.
package runtime

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/agents"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// LLMProviderFactory creates LLM providers by role.
type LLMProviderFactory func(role string) agents.LLMProvider

// PersistenceAdapter handles state persistence.
type PersistenceAdapter interface {
	SaveState(ctx context.Context, threadID string, state map[string]any) error
	LoadState(ctx context.Context, threadID string) (map[string]any, error)
}

// UnifiedRuntime executes pipelines from configuration.
type UnifiedRuntime struct {
	Config         *config.PipelineConfig
	LLMFactory     LLMProviderFactory
	ToolExecutor   agents.ToolExecutor
	Logger         agents.Logger
	Persistence    PersistenceAdapter
	PromptRegistry agents.PromptRegistry
	UseMock        bool

	agents   map[string]*agents.UnifiedAgent
	eventCtx agents.EventContext
}

// NewUnifiedRuntime creates a new UnifiedRuntime.
func NewUnifiedRuntime(
	cfg *config.PipelineConfig,
	llmFactory LLMProviderFactory,
	toolExecutor agents.ToolExecutor,
	logger agents.Logger,
) (*UnifiedRuntime, error) {
	if err := cfg.Validate(); err != nil {
		return nil, err
	}

	runtime := &UnifiedRuntime{
		Config:       cfg,
		LLMFactory:   llmFactory,
		ToolExecutor: toolExecutor,
		Logger:       logger.Bind("pipeline", cfg.Name),
		agents:       make(map[string]*agents.UnifiedAgent),
	}

	// Build agents from config
	if err := runtime.buildAgents(); err != nil {
		return nil, err
	}

	return runtime, nil
}

func (r *UnifiedRuntime) buildAgents() error {
	for _, agentConfig := range r.Config.Agents {
		// Get LLM provider if needed
		var llm agents.LLMProvider
		if agentConfig.HasLLM && agentConfig.ModelRole != "" && r.LLMFactory != nil {
			llm = r.LLMFactory(agentConfig.ModelRole)
		}

		// Get tool executor if needed
		var tools agents.ToolExecutor
		if agentConfig.HasTools {
			tools = r.ToolExecutor
		}

		agent, err := agents.NewUnifiedAgent(agentConfig, r.Logger, llm, tools)
		if err != nil {
			return fmt.Errorf("failed to create agent '%s': %w", agentConfig.Name, err)
		}

		agent.PromptRegistry = r.PromptRegistry
		agent.UseMock = r.UseMock

		r.agents[agentConfig.Name] = agent
	}

	r.Logger.Info("runtime_agents_built",
		"agent_count", len(r.agents),
		"agents", r.Config.GetStageOrder(),
	)

	return nil
}

// SetEventContext sets the event context for all agents.
func (r *UnifiedRuntime) SetEventContext(ctx agents.EventContext) {
	r.eventCtx = ctx
	for _, agent := range r.agents {
		agent.SetEventContext(ctx)
	}
}

// Run executes the pipeline on an envelope.
func (r *UnifiedRuntime) Run(ctx context.Context, env *envelope.GenericEnvelope, threadID string) (*envelope.GenericEnvelope, error) {
	startTime := time.Now()

	// Set stage order on envelope
	env.StageOrder = r.Config.GetStageOrder()
	if len(env.StageOrder) > 0 {
		env.CurrentStage = env.StageOrder[0]
	} else {
		env.CurrentStage = "end"
	}

	// Apply bounds from config
	env.MaxIterations = r.Config.MaxIterations
	env.MaxLLMCalls = r.Config.MaxLLMCalls
	env.MaxAgentHops = r.Config.MaxAgentHops

	r.Logger.Info("pipeline_started",
		"envelope_id", env.EnvelopeID,
		"request_id", env.RequestID,
		"stage_order", env.StageOrder,
	)

	var err error

	// Execute pipeline loop
	for env.CurrentStage != "end" && !env.Terminated {
		// Check bounds
		if !env.CanContinue() {
			r.Logger.Warn("pipeline_bounds_exceeded",
				"envelope_id", env.EnvelopeID,
				"terminal_reason", env.TerminalReason_,
			)
			break
		}

		// Handle pending interrupt
		if env.InterruptPending {
			r.Logger.Info("pipeline_interrupt",
				"envelope_id", env.EnvelopeID,
				"interrupt_kind", env.GetInterruptKind(),
			)
			break
		}

		// Get agent for current stage
		agent, exists := r.agents[env.CurrentStage]
		if !exists {
			r.Logger.Error("pipeline_unknown_stage",
				"envelope_id", env.EnvelopeID,
				"stage", env.CurrentStage,
			)
			reason := envelope.TerminalReasonToolFailedFatally
			env.Terminate(fmt.Sprintf("Unknown stage: %s", env.CurrentStage), &reason)
			break
		}

		// Execute agent
		env, err = agent.Process(ctx, env)
		if err != nil {
			r.Logger.Error("pipeline_agent_error",
				"envelope_id", env.EnvelopeID,
				"agent", agent.Name,
				"error", err.Error(),
			)
			// Agent may have set error_next routing
			if env.CurrentStage == "end" || env.Terminated {
				break
			}
			// If no error routing, propagate error
			reason := envelope.TerminalReasonToolFailedFatally
			env.Terminate(err.Error(), &reason)
			break
		}

		// Persist state if configured
		if r.Persistence != nil && threadID != "" {
			if persistErr := r.Persistence.SaveState(ctx, threadID, env.ToStateDict()); persistErr != nil {
				r.Logger.Warn("state_persist_error",
					"thread_id", threadID,
					"error", persistErr.Error(),
				)
			}
		}
	}

	// Mark complete
	durationMS := int(time.Since(startTime).Milliseconds())

	r.Logger.Info("pipeline_completed",
		"envelope_id", env.EnvelopeID,
		"request_id", env.RequestID,
		"final_stage", env.CurrentStage,
		"terminated", env.Terminated,
		"duration_ms", durationMS,
	)

	return env, nil
}

// RunStreaming executes pipeline with streaming updates via channel.
func (r *UnifiedRuntime) RunStreaming(ctx context.Context, env *envelope.GenericEnvelope, threadID string) (<-chan StageOutput, error) {
	outputChan := make(chan StageOutput, len(r.Config.Agents)+1)

	go func() {
		defer close(outputChan)

		env.StageOrder = r.Config.GetStageOrder()
		if len(env.StageOrder) > 0 {
			env.CurrentStage = env.StageOrder[0]
		} else {
			env.CurrentStage = "end"
		}

		env.MaxIterations = r.Config.MaxIterations
		env.MaxLLMCalls = r.Config.MaxLLMCalls
		env.MaxAgentHops = r.Config.MaxAgentHops

		r.Logger.Info("pipeline_streaming_started",
			"envelope_id", env.EnvelopeID,
			"request_id", env.RequestID,
		)

		var err error

		for env.CurrentStage != "end" && !env.Terminated {
			if !env.CanContinue() {
				break
			}

			if env.InterruptPending {
				break
			}

			agent, exists := r.agents[env.CurrentStage]
			if !exists {
				reason := envelope.TerminalReasonToolFailedFatally
				env.Terminate(fmt.Sprintf("Unknown stage: %s", env.CurrentStage), &reason)
				break
			}

			stageName := env.CurrentStage

			env, err = agent.Process(ctx, env)
			if err != nil {
				reason := envelope.TerminalReasonToolFailedFatally
				env.Terminate(err.Error(), &reason)
				break
			}

			output := env.GetOutput(stageName)
			outputChan <- StageOutput{Stage: stageName, Output: output}

			if r.Persistence != nil && threadID != "" {
				_ = r.Persistence.SaveState(ctx, threadID, env.ToStateDict())
			}
		}

		outputChan <- StageOutput{
			Stage:  "__end__",
			Output: map[string]any{"terminated": env.Terminated},
		}
	}()

	return outputChan, nil
}

// Resume resumes pipeline execution after interrupt.
func (r *UnifiedRuntime) Resume(ctx context.Context, env *envelope.GenericEnvelope, response envelope.InterruptResponse, threadID string) (*envelope.GenericEnvelope, error) {
	if !env.InterruptPending || env.Interrupt == nil {
		return env, fmt.Errorf("no pending interrupt to resume")
	}

	// Store the interrupt kind before resolving
	kind := env.Interrupt.Kind

	// Resolve the interrupt with the response
	env.ResolveInterrupt(response)

	// Determine resume stage based on interrupt kind
	// Use configurable stages from PipelineConfig, or stay at current stage
	switch kind {
	case envelope.InterruptKindClarification:
		if r.Config.ClarificationResumeStage != "" {
			env.CurrentStage = r.Config.ClarificationResumeStage
		}
		// If not configured, stay at current stage
	case envelope.InterruptKindConfirmation:
		if response.Approved != nil && *response.Approved {
			if r.Config.ConfirmationResumeStage != "" {
				env.CurrentStage = r.Config.ConfirmationResumeStage
			}
			// If not configured, stay at current stage
		} else {
			env.Terminate("User denied confirmation", nil)
			return env, nil
		}
	case envelope.InterruptKindAgentReview:
		if r.Config.AgentReviewResumeStage != "" {
			env.CurrentStage = r.Config.AgentReviewResumeStage
		}
		// If not configured, stay at current stage
	default:
		// For other interrupt types, stay at current stage
	}

	r.Logger.Info("pipeline_resumed",
		"envelope_id", env.EnvelopeID,
		"interrupt_kind", kind,
		"resume_stage", env.CurrentStage,
	)

	return r.Run(ctx, env, threadID)
}

// GetState gets persisted state for a thread.
func (r *UnifiedRuntime) GetState(ctx context.Context, threadID string) (map[string]any, error) {
	if r.Persistence == nil {
		return nil, nil
	}
	return r.Persistence.LoadState(ctx, threadID)
}

// StageOutput represents output from a pipeline stage.
type StageOutput struct {
	Stage  string
	Output map[string]any
	Error  error
}

// RunParallel executes pipeline with parallel stage execution.
// Independent stages (no dependencies between them) run concurrently.
// Sets ParallelMode=true on the envelope during execution.
func (r *UnifiedRuntime) RunParallel(ctx context.Context, env *envelope.GenericEnvelope, threadID string) (*envelope.GenericEnvelope, error) {
	startTime := time.Now()

	// Initialize envelope
	env.StageOrder = r.Config.GetStageOrder()
	env.MaxIterations = r.Config.MaxIterations
	env.MaxLLMCalls = r.Config.MaxLLMCalls
	env.MaxAgentHops = r.Config.MaxAgentHops

	r.Logger.Info("pipeline_parallel_started",
		"envelope_id", env.EnvelopeID,
		"request_id", env.RequestID,
		"stage_order", env.StageOrder,
	)

	// Track completed stages
	completed := make(map[string]bool)
	var mu sync.Mutex

	// Execute until all stages done or terminated
	for !env.Terminated {
		// Check bounds
		if !env.CanContinue() {
			r.Logger.Warn("pipeline_bounds_exceeded",
				"envelope_id", env.EnvelopeID,
				"terminal_reason", env.TerminalReason_,
			)
			break
		}

		// Handle pending interrupt
		if env.InterruptPending {
			r.Logger.Info("pipeline_interrupt",
				"envelope_id", env.EnvelopeID,
				"interrupt_kind", env.GetInterruptKind(),
			)
			break
		}

		// Find stages ready to execute
		readyStages := r.Config.GetReadyStages(completed)
		if len(readyStages) == 0 {
			// No more stages to execute
			break
		}

		r.Logger.Debug("parallel_batch",
			"envelope_id", env.EnvelopeID,
			"ready_stages", readyStages,
			"completed", len(completed),
		)

		// Execute ready stages in parallel
		results := make(chan StageOutput, len(readyStages))
		var wg sync.WaitGroup

		for _, stageName := range readyStages {
			agent, exists := r.agents[stageName]
			if !exists {
				r.Logger.Error("pipeline_unknown_stage",
					"envelope_id", env.EnvelopeID,
					"stage", stageName,
				)
				reason := envelope.TerminalReasonToolFailedFatally
				env.Terminate(fmt.Sprintf("Unknown stage: %s", stageName), &reason)
				break
			}

			wg.Add(1)
			go func(name string, a *agents.UnifiedAgent) {
				defer wg.Done()

				// Clone envelope for this stage (thread safety)
				stageEnv := env.Clone()
				stageEnv.CurrentStage = name

				// Execute agent
				resultEnv, err := a.Process(ctx, stageEnv)
				results <- StageOutput{
					Stage:  name,
					Output: resultEnv.GetOutput(name),
					Error:  err,
				}
			}(stageName, agent)
		}

		// Wait for all parallel stages to complete
		go func() {
			wg.Wait()
			close(results)
		}()

		// Collect results
		for result := range results {
			if result.Error != nil {
				r.Logger.Error("pipeline_agent_error",
					"envelope_id", env.EnvelopeID,
					"agent", result.Stage,
					"error", result.Error.Error(),
				)
				reason := envelope.TerminalReasonToolFailedFatally
				env.Terminate(result.Error.Error(), &reason)
				break
			}

			// Merge output into main envelope
			mu.Lock()
			env.SetOutput(result.Stage, result.Output)
			completed[result.Stage] = true
			mu.Unlock()

			r.Logger.Debug("stage_completed",
				"envelope_id", env.EnvelopeID,
				"stage", result.Stage,
			)
		}

		if env.Terminated {
			break
		}

		// Persist state after each batch
		if r.Persistence != nil && threadID != "" {
			if persistErr := r.Persistence.SaveState(ctx, threadID, env.ToStateDict()); persistErr != nil {
				r.Logger.Warn("state_persist_error",
					"thread_id", threadID,
					"error", persistErr.Error(),
				)
			}
		}
	}

	durationMS := int(time.Since(startTime).Milliseconds())

	r.Logger.Info("pipeline_parallel_completed",
		"envelope_id", env.EnvelopeID,
		"request_id", env.RequestID,
		"stages_completed", len(completed),
		"terminated", env.Terminated,
		"duration_ms", durationMS,
	)

	return env, nil
}

// CreateRuntimeFromConfig is a factory function to create UnifiedRuntime.
func CreateRuntimeFromConfig(
	cfg *config.PipelineConfig,
	llmFactory LLMProviderFactory,
	toolExecutor agents.ToolExecutor,
	logger agents.Logger,
) (*UnifiedRuntime, error) {
	return NewUnifiedRuntime(cfg, llmFactory, toolExecutor, logger)
}
