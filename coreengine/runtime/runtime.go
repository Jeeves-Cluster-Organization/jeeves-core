// Package runtime provides the PipelineRunner - pipeline orchestration engine.
package runtime

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/agents"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/observability"
)

// RunMode from config package.
type RunMode = config.RunMode

// LLMProvider type alias for convenience in tests.
type LLMProvider = agents.LLMProvider

const (
	RunModeSequential = config.RunModeSequential
	RunModeParallel   = config.RunModeParallel
)

// RunOptions configures how the pipeline runs.
type RunOptions struct {
	// Mode: sequential or parallel. Default: sequential.
	Mode RunMode

	// Stream: send stage outputs to a channel as they complete.
	Stream bool

	// ThreadID for state persistence. Empty disables persistence.
	ThreadID string
}

// LLMProviderFactory creates LLM providers by role.
type LLMProviderFactory func(role string) agents.LLMProvider

// PersistenceAdapter handles state persistence.
type PersistenceAdapter interface {
	SaveState(ctx context.Context, threadID string, state map[string]any) error
	LoadState(ctx context.Context, threadID string) (map[string]any, error)
}

// PipelineRunner executes pipelines from configuration.
type PipelineRunner struct {
	Config         *config.PipelineConfig
	LLMFactory     LLMProviderFactory
	ToolExecutor   agents.ToolExecutor
	Logger         agents.Logger
	Persistence    PersistenceAdapter
	PromptRegistry agents.PromptRegistry
	UseMock        bool

	agents   map[string]*agents.Agent
	eventCtx agents.EventContext
}

// NewPipelineRunner creates a new PipelineRunner.
func NewPipelineRunner(
	cfg *config.PipelineConfig,
	llmFactory LLMProviderFactory,
	toolExecutor agents.ToolExecutor,
	logger agents.Logger,
) (*PipelineRunner, error) {
	if err := cfg.Validate(); err != nil {
		return nil, err
	}

	runner := &PipelineRunner{
		Config:       cfg,
		LLMFactory:   llmFactory,
		ToolExecutor: toolExecutor,
		Logger:       logger.Bind("pipeline", cfg.Name),
		agents:       make(map[string]*agents.Agent),
	}

	// Build agents from config
	if err := runner.buildAgents(); err != nil {
		return nil, err
	}

	return runner, nil
}

func (r *PipelineRunner) buildAgents() error {
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

		agent, err := agents.NewAgent(agentConfig, r.Logger, llm, tools)
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
func (r *PipelineRunner) SetEventContext(ctx agents.EventContext) {
	r.eventCtx = ctx
	for _, agent := range r.agents {
		agent.SetEventContext(ctx)
	}
}

// =============================================================================
// UNIFIED EXECUTION
// =============================================================================

// Execute runs the pipeline with the given options.
func (r *PipelineRunner) Execute(ctx context.Context, env *envelope.Envelope, opts RunOptions) (*envelope.Envelope, <-chan StageOutput, error) {
	// Apply defaults from config
	if opts.Mode == "" {
		if r.Config.DefaultRunMode != "" {
			opts.Mode = RunMode(r.Config.DefaultRunMode)
		} else {
			opts.Mode = RunModeSequential
		}
	}

	// Initialize envelope
	r.initializeEnvelope(env)

	// Set parallel mode flag
	if opts.Mode == RunModeParallel {
		env.ParallelMode = true
	}

	startTime := time.Now()
	logEvent := "pipeline_started"
	if opts.Mode == RunModeParallel {
		logEvent = "pipeline_parallel_started"
	}

	r.Logger.Info(logEvent,
		"envelope_id", env.EnvelopeID,
		"request_id", env.RequestID,
		"mode", string(opts.Mode),
		"stream", opts.Stream,
		"stage_order", env.StageOrder,
	)

	// Create output channel if streaming
	var outputChan chan StageOutput
	if opts.Stream {
		outputChan = make(chan StageOutput, len(r.Config.Agents)+1)
	}

	// Run based on mode
	var resultEnv *envelope.Envelope
	var err error

	switch opts.Mode {
	case RunModeParallel:
		resultEnv, err = r.runParallelCore(ctx, env, opts, outputChan)
	default:
		resultEnv, err = r.runSequentialCore(ctx, env, opts, outputChan)
	}

	// Close output channel and send end marker
	if outputChan != nil {
		outputChan <- StageOutput{
			Stage:  "__end__",
			Output: map[string]any{"terminated": resultEnv.Terminated},
		}
		close(outputChan)
	}

	// Log completion
	durationMS := int(time.Since(startTime).Milliseconds())

	// Record metrics
	status := "success"
	if err != nil {
		status = "error"
	} else if resultEnv.Terminated {
		status = "terminated"
	}
	observability.RecordPipelineExecution(r.Config.Name, status, durationMS)

	completeEvent := "pipeline_completed"
	if opts.Mode == RunModeParallel {
		completeEvent = "pipeline_parallel_completed"
	}

	r.Logger.Info(completeEvent,
		"envelope_id", resultEnv.EnvelopeID,
		"request_id", resultEnv.RequestID,
		"final_stage", resultEnv.CurrentStage,
		"terminated", resultEnv.Terminated,
		"duration_ms", durationMS,
	)

	return resultEnv, outputChan, err
}

// initializeEnvelope sets up the envelope with pipeline configuration.
func (r *PipelineRunner) initializeEnvelope(env *envelope.Envelope) {
	env.StageOrder = r.Config.GetStageOrder()
	if len(env.StageOrder) > 0 {
		env.CurrentStage = env.StageOrder[0]
	} else {
		env.CurrentStage = "end"
	}
	env.MaxIterations = r.Config.MaxIterations
	env.MaxLLMCalls = r.Config.MaxLLMCalls
	env.MaxAgentHops = r.Config.MaxAgentHops
}

// shouldContinue checks if execution should continue (shared by all modes).
// Note: CanContinue() already checks for InterruptPending, Terminated, and bounds.
func (r *PipelineRunner) shouldContinue(env *envelope.Envelope) (bool, string) {
	if !env.CanContinue() {
		// CanContinue returns false for: Terminated, InterruptPending, or bounds exceeded
		reason := "cannot_continue"
		if env.Terminated {
			reason = "terminated"
		} else if env.InterruptPending {
			reason = "interrupt_pending"
			r.Logger.Info("pipeline_interrupt",
				"envelope_id", env.EnvelopeID,
				"interrupt_kind", env.GetInterruptKind(),
			)
		} else if env.TerminalReason_ != nil {
			reason = "bounds_exceeded"
			r.Logger.Warn("pipeline_bounds_exceeded",
				"envelope_id", env.EnvelopeID,
				"terminal_reason", env.TerminalReason_,
			)
		}
		return false, reason
	}

	return true, ""
}

// persistState saves state if persistence is configured.
func (r *PipelineRunner) persistState(ctx context.Context, env *envelope.Envelope, threadID string) {
	if r.Persistence != nil && threadID != "" {
		if persistErr := r.Persistence.SaveState(ctx, threadID, env.ToStateDict()); persistErr != nil {
			r.Logger.Warn("state_persist_error",
				"thread_id", threadID,
				"error", persistErr.Error(),
			)
		}
	}
}

// runSequentialCore runs stages one at a time following routing rules.
func (r *PipelineRunner) runSequentialCore(ctx context.Context, env *envelope.Envelope, opts RunOptions, outputChan chan StageOutput) (*envelope.Envelope, error) {
	var err error

	// Track edge traversals for edge limit enforcement
	edgeTraversals := make(map[string]int)

	for env.CurrentStage != "end" && !env.Terminated {
		// Check context cancellation early in each iteration
		select {
		case <-ctx.Done():
			r.Logger.Info("pipeline_cancelled",
				"envelope_id", env.EnvelopeID,
				"stage", env.CurrentStage,
				"reason", ctx.Err().Error(),
			)
			return env, ctx.Err()
		default:
			// Continue processing
		}

		// Check if we should continue
		if cont, _ := r.shouldContinue(env); !cont {
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

		stageName := env.CurrentStage
		fromStage := env.CurrentStage

		// Execute agent
		env, err = agent.Process(ctx, env)
		if err != nil {
			// Check if error is due to context cancellation
			if ctx.Err() != nil {
				return env, ctx.Err()
			}

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

		toStage := env.CurrentStage

		// Track edge traversal and check edge limits
		if toStage != fromStage && toStage != "end" {
			edgeKey := fromStage + "->" + toStage
			edgeTraversals[edgeKey]++

			// Detect loop-back BEFORE checking limits (to increment iteration even if limit hit)
			toIndex := -1
			fromIndex := -1
			for i, stage := range env.StageOrder {
				if stage == toStage {
					toIndex = i
				}
				if stage == fromStage {
					fromIndex = i
				}
			}

			if toIndex >= 0 && fromIndex >= 0 && toIndex < fromIndex {
				// Loop-back detected - increment iteration
				env.IncrementIteration(nil)
				r.Logger.Debug("iteration_incremented",
					"envelope_id", env.EnvelopeID,
					"iteration", env.Iteration,
					"edge", edgeKey,
				)
			}

			// Now check edge limit
			limit := r.Config.GetEdgeLimit(fromStage, toStage)
			if limit > 0 && edgeTraversals[edgeKey] > limit {
				r.Logger.Warn("edge_limit_exceeded",
					"envelope_id", env.EnvelopeID,
					"edge", edgeKey,
					"limit", limit,
					"traversals", edgeTraversals[edgeKey],
				)
				reason := envelope.TerminalReasonMaxLoopExceeded
				env.Terminate(fmt.Sprintf("Edge limit exceeded: %s", edgeKey), &reason)
				env.CurrentStage = "end"
				break
			}
		}

		// Send output to channel if streaming
		if outputChan != nil {
			output := env.GetOutput(stageName)
			outputChan <- StageOutput{Stage: stageName, Output: output}
		}

		// Persist state
		r.persistState(ctx, env, opts.ThreadID)
	}

	return env, nil
}

// runParallelCore runs independent stages concurrently.
func (r *PipelineRunner) runParallelCore(ctx context.Context, env *envelope.Envelope, opts RunOptions, outputChan chan StageOutput) (*envelope.Envelope, error) {
	// Track completed stages
	completed := make(map[string]bool)
	var mu sync.Mutex

	for !env.Terminated {
		// Check context cancellation early in each iteration
		select {
		case <-ctx.Done():
			r.Logger.Info("pipeline_parallel_cancelled",
				"envelope_id", env.EnvelopeID,
				"completed_stages", len(completed),
				"reason", ctx.Err().Error(),
			)
			return env, ctx.Err()
		default:
			// Continue processing
		}

		// Check if we should continue
		if cont, _ := r.shouldContinue(env); !cont {
			break
		}

		// Find stages ready to execute (dependencies satisfied)
		readyStages := r.Config.GetReadyStages(completed)
		if len(readyStages) == 0 {
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

			// Clone envelope while holding the lock to prevent race with SetOutput
			mu.Lock()
			stageEnv := env.Clone()
			mu.Unlock()
			stageEnv.CurrentStage = stageName

			wg.Add(1)
			go func(name string, a *agents.Agent, clonedEnv *envelope.Envelope) {
				defer wg.Done()

				// Execute agent with the cloned envelope
				resultEnv, err := a.Process(ctx, clonedEnv)
				results <- StageOutput{
					Stage:  name,
					Output: resultEnv.GetOutput(name),
					Error:  err,
				}
			}(stageName, agent, stageEnv)
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

			// Send output to channel if streaming
			if outputChan != nil {
				outputChan <- result
			}

			r.Logger.Debug("stage_completed",
				"envelope_id", env.EnvelopeID,
				"stage", result.Stage,
			)
		}

		if env.Terminated {
			break
		}

		// Persist state after each batch
		r.persistState(ctx, env, opts.ThreadID)
	}

	// Set final stage after parallel completion
	env.CurrentStage = "end"

	return env, nil
}

// =============================================================================
// CONVENIENCE METHODS
// =============================================================================

// Run runs the pipeline sequentially.
func (r *PipelineRunner) Run(ctx context.Context, env *envelope.Envelope, threadID string) (*envelope.Envelope, error) {
	result, _, err := r.Execute(ctx, env, RunOptions{
		Mode:     RunModeSequential,
		ThreadID: threadID,
	})
	return result, err
}

// RunWithStream runs the pipeline and streams stage outputs to a channel.
func (r *PipelineRunner) RunWithStream(ctx context.Context, env *envelope.Envelope, threadID string) (<-chan StageOutput, error) {
	outputChan := make(chan StageOutput, len(r.Config.Agents)+1)

	go func() {
		defer close(outputChan)

		r.initializeEnvelope(env)

		r.Logger.Info("pipeline_streaming_started",
			"envelope_id", env.EnvelopeID,
			"request_id", env.RequestID,
		)

		opts := RunOptions{
			Mode:     RunModeSequential,
			Stream:   true,
			ThreadID: threadID,
		}

		resultEnv, _ := r.runSequentialCore(ctx, env, opts, outputChan)

		outputChan <- StageOutput{
			Stage:  "__end__",
			Output: map[string]any{"terminated": resultEnv.Terminated},
		}
	}()

	return outputChan, nil
}

// RunParallel runs the pipeline with parallel stage execution.
func (r *PipelineRunner) RunParallel(ctx context.Context, env *envelope.Envelope, threadID string) (*envelope.Envelope, error) {
	result, _, err := r.Execute(ctx, env, RunOptions{
		Mode:     RunModeParallel,
		ThreadID: threadID,
	})
	return result, err
}

// Resume resumes pipeline execution after interrupt.
func (r *PipelineRunner) Resume(ctx context.Context, env *envelope.Envelope, response envelope.InterruptResponse, threadID string) (*envelope.Envelope, error) {
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
	case envelope.InterruptKindConfirmation:
		if response.Approved != nil && *response.Approved {
			if r.Config.ConfirmationResumeStage != "" {
				env.CurrentStage = r.Config.ConfirmationResumeStage
			}
		} else {
			env.Terminate("User denied confirmation", nil)
			return env, nil
		}
	case envelope.InterruptKindAgentReview:
		if r.Config.AgentReviewResumeStage != "" {
			env.CurrentStage = r.Config.AgentReviewResumeStage
		}
	default:
		// For other interrupt types, stay at current stage
	}

	r.Logger.Info("pipeline_resumed",
		"envelope_id", env.EnvelopeID,
		"interrupt_kind", kind,
		"resume_stage", env.CurrentStage,
	)

	// Resume uses the same mode as the original run
	mode := RunModeSequential
	if env.ParallelMode {
		mode = RunModeParallel
	}

	result, _, err := r.Execute(ctx, env, RunOptions{
		Mode:     mode,
		ThreadID: threadID,
	})

	// Persist state after resume execution
	if err == nil && threadID != "" {
		r.persistState(ctx, result, threadID)
	}

	return result, err
}

// GetState gets persisted state for a thread.
func (r *PipelineRunner) GetState(ctx context.Context, threadID string) (map[string]any, error) {
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

