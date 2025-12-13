// Package runtime provides the DAG executor for parallel pipeline execution.
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

// StageResult represents the result of a stage execution.
type StageResult struct {
	StageName string
	Success   bool
	Error     error
	Output    map[string]any
	Duration  time.Duration
	LLMCalls  int
}

// DAGExecutor executes pipeline stages in parallel based on dependencies.
// Implements hybrid systemd + CSP pattern:
// - systemd-style: Declarative dependencies (Requires, After)
// - CSP-style: Channel-based coordination between goroutines
type DAGExecutor struct {
	Config       *config.PipelineConfig
	Agents       map[string]*agents.UnifiedAgent
	Logger       agents.Logger
	Persistence  PersistenceAdapter

	// Channels for coordination (CSP pattern)
	completedChan chan StageResult // Signals when a stage completes
	errorChan     chan StageResult // Signals when a stage fails

	// Synchronization
	mu            sync.RWMutex
	wg            sync.WaitGroup
	activeCount   int
	maxParallel   int // Max concurrent stages (0 = unlimited)
}

// NewDAGExecutor creates a new DAG executor.
func NewDAGExecutor(
	cfg *config.PipelineConfig,
	agentsMap map[string]*agents.UnifiedAgent,
	logger agents.Logger,
) *DAGExecutor {
	return &DAGExecutor{
		Config:        cfg,
		Agents:        agentsMap,
		Logger:        logger.Bind("executor", "dag"),
		completedChan: make(chan StageResult, len(cfg.Agents)),
		errorChan:     make(chan StageResult, len(cfg.Agents)),
		maxParallel:   0, // Unlimited by default
	}
}

// SetMaxParallel sets the maximum number of concurrent stage executions.
func (d *DAGExecutor) SetMaxParallel(max int) {
	d.maxParallel = max
}

// Execute runs the pipeline using DAG-based parallel execution.
func (d *DAGExecutor) Execute(ctx context.Context, env *envelope.GenericEnvelope, threadID string) (*envelope.GenericEnvelope, error) {
	startTime := time.Now()

	// Initialize DAG mode
	env.DAGMode = true
	env.StageOrder = d.Config.GetStageOrder()
	env.ActiveStages = make(map[string]bool)
	env.CompletedStageSet = make(map[string]bool)
	env.FailedStages = make(map[string]string)

	// Apply bounds from config
	env.MaxIterations = d.Config.MaxIterations
	env.MaxLLMCalls = d.Config.MaxLLMCalls
	env.MaxAgentHops = d.Config.MaxAgentHops

	d.Logger.Info("dag_execution_started",
		"envelope_id", env.EnvelopeID,
		"request_id", env.RequestID,
		"stage_count", len(d.Config.Agents),
		"topological_order", d.Config.GetTopologicalOrder(),
	)

	// Create cancellable context for graceful shutdown
	execCtx, cancel := context.WithCancel(ctx)
	defer cancel()

	// Start coordinator goroutine
	done := make(chan struct{})
	var execErr error

	go func() {
		defer close(done)
		execErr = d.coordinate(execCtx, env, threadID)
	}()

	// Wait for completion or context cancellation
	select {
	case <-done:
		// Normal completion
	case <-ctx.Done():
		cancel()
		<-done // Wait for coordinator to finish
		if execErr == nil {
			execErr = ctx.Err()
		}
	}

	// Mark completion
	durationMS := int(time.Since(startTime).Milliseconds())
	d.Logger.Info("dag_execution_completed",
		"envelope_id", env.EnvelopeID,
		"duration_ms", durationMS,
		"completed_stages", env.GetCompletedStageCount(),
		"failed_stages", len(env.FailedStages),
		"terminated", env.Terminated,
	)

	return env, execErr
}

// coordinate is the main coordination loop (runs in its own goroutine).
func (d *DAGExecutor) coordinate(ctx context.Context, env *envelope.GenericEnvelope, threadID string) error {
	// Track pending stages (not yet started)
	pending := make(map[string]bool)
	for _, agent := range d.Config.Agents {
		pending[agent.Name] = true
	}

	// Main coordination loop
	for {
		// Check termination conditions
		if env.Terminated {
			d.Logger.Info("dag_terminated", "reason", env.TerminationReason)
			return nil
		}

		if !env.CanContinue() {
			d.Logger.Warn("dag_bounds_exceeded",
				"terminal_reason", env.TerminalReason_,
			)
			return nil
		}

		// Check if all stages are complete
		if env.AllStagesComplete() {
			env.CurrentStage = "end"
			d.Logger.Info("dag_all_stages_complete")
			return nil
		}

		// Check for failures (fail-fast)
		if env.HasFailures() {
			reason := fmt.Sprintf("Stage failures: %v", env.FailedStages)
			env.Terminate(reason, nil)
			return fmt.Errorf(reason)
		}

		// Find ready stages
		readyStages := d.Config.GetReadyStages(env.CompletedStageSet)

		// Filter out already active or completed stages
		toStart := make([]string, 0)
		for _, stage := range readyStages {
			if !env.IsStageActive(stage) && !env.IsStageCompleted(stage) && pending[stage] {
				toStart = append(toStart, stage)
			}
		}

		// Respect max parallel limit
		if d.maxParallel > 0 {
			available := d.maxParallel - env.GetActiveStageCount()
			if available < len(toStart) {
				toStart = toStart[:available]
			}
		}

		// Start ready stages
		for _, stageName := range toStart {
			delete(pending, stageName)
			d.startStage(ctx, env, stageName, threadID)
		}

		// Wait for at least one stage to complete (or context cancellation)
		select {
		case result := <-d.completedChan:
			d.handleCompletion(env, result, threadID)

		case result := <-d.errorChan:
			d.handleError(env, result)

		case <-ctx.Done():
			return ctx.Err()

		case <-time.After(100 * time.Millisecond):
			// Prevent busy-waiting; check again
			continue
		}
	}
}

// startStage launches a stage in a new goroutine.
func (d *DAGExecutor) startStage(ctx context.Context, env *envelope.GenericEnvelope, stageName string, threadID string) {
	agent, exists := d.Agents[stageName]
	if !exists {
		d.errorChan <- StageResult{
			StageName: stageName,
			Success:   false,
			Error:     fmt.Errorf("agent not found: %s", stageName),
		}
		return
	}

	// Mark as active
	d.mu.Lock()
	env.StartStage(stageName)
	env.RecordAgentStart(stageName, agent.Config.StageOrder)
	d.activeCount++
	d.mu.Unlock()

	d.Logger.Info("dag_stage_started",
		"stage", stageName,
		"active_count", d.activeCount,
	)

	// Launch goroutine for execution
	d.wg.Add(1)
	go func(name string, agent *agents.UnifiedAgent) {
		defer d.wg.Done()

		startTime := time.Now()

		// Execute the agent
		// NOTE: We need to be careful about concurrent writes to envelope
		// Each agent writes to its own output slot, which should be safe
		_, err := agent.Process(ctx, env)

		duration := time.Since(startTime)
		llmCalls := 0 // TODO: Track from agent

		result := StageResult{
			StageName: name,
			Success:   err == nil,
			Error:     err,
			Output:    env.GetOutput(name),
			Duration:  duration,
			LLMCalls:  llmCalls,
		}

		if err != nil {
			d.errorChan <- result
		} else {
			d.completedChan <- result
		}
	}(stageName, agent)
}

// handleCompletion processes a successful stage completion.
func (d *DAGExecutor) handleCompletion(env *envelope.GenericEnvelope, result StageResult, threadID string) {
	d.mu.Lock()
	defer d.mu.Unlock()

	env.CompleteStage(result.StageName)
	env.RecordAgentComplete(result.StageName, "success", nil, result.LLMCalls, int(result.Duration.Milliseconds()))
	d.activeCount--

	d.Logger.Info("dag_stage_completed",
		"stage", result.StageName,
		"duration_ms", result.Duration.Milliseconds(),
		"active_count", d.activeCount,
		"completed_count", env.GetCompletedStageCount(),
	)

	// Persist state if configured
	if d.Persistence != nil && threadID != "" {
		if err := d.Persistence.SaveState(context.Background(), threadID, env.ToStateDict()); err != nil {
			d.Logger.Warn("dag_state_persist_error",
				"thread_id", threadID,
				"error", err.Error(),
			)
		}
	}
}

// handleError processes a stage failure.
func (d *DAGExecutor) handleError(env *envelope.GenericEnvelope, result StageResult) {
	d.mu.Lock()
	defer d.mu.Unlock()

	errMsg := ""
	if result.Error != nil {
		errMsg = result.Error.Error()
	}

	env.FailStage(result.StageName, errMsg)
	env.RecordAgentComplete(result.StageName, "error", &errMsg, result.LLMCalls, int(result.Duration.Milliseconds()))
	d.activeCount--

	d.Logger.Error("dag_stage_failed",
		"stage", result.StageName,
		"error", errMsg,
		"active_count", d.activeCount,
	)
}

// ExecuteStreaming runs the pipeline with streaming updates via channel.
func (d *DAGExecutor) ExecuteStreaming(ctx context.Context, env *envelope.GenericEnvelope, threadID string) (<-chan StageOutput, error) {
	outputChan := make(chan StageOutput, len(d.Config.Agents)+1)

	go func() {
		defer close(outputChan)

		// Create a wrapper that streams completions
		streamingCompletedChan := make(chan StageResult, len(d.Config.Agents))
		originalCompletedChan := d.completedChan
		d.completedChan = streamingCompletedChan

		// Start a goroutine to forward completions and stream outputs
		go func() {
			for result := range streamingCompletedChan {
				originalCompletedChan <- result
				outputChan <- StageOutput{
					Stage:  result.StageName,
					Output: result.Output,
				}
			}
		}()

		// Execute the DAG
		_, err := d.Execute(ctx, env, threadID)

		// Restore original channel
		d.completedChan = originalCompletedChan

		// Send final output
		outputChan <- StageOutput{
			Stage: "__end__",
			Output: map[string]any{
				"terminated":       env.Terminated,
				"completed_stages": env.GetCompletedStageCount(),
				"error":            err,
			},
		}
	}()

	return outputChan, nil
}

// Wait waits for all active stages to complete.
func (d *DAGExecutor) Wait() {
	d.wg.Wait()
}
