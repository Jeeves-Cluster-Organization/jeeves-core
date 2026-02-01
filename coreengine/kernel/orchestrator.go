// Package kernel provides the Orchestrator - kernel-side pipeline execution control.
//
// The Orchestrator:
//   - Owns the orchestration loop (moved from Python)
//   - Evaluates routing rules
//   - Tracks edge traversals
//   - Enforces bounds (iterations, LLM calls, agent hops)
//   - Returns Instructions to Python workers
//
// Python workers:
//   - Execute agents (LLM calls, tool execution)
//   - Report results back
//   - Have NO control over what runs next
package kernel

import (
	"encoding/json"
	"fmt"
	"sync"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// =============================================================================
// Instruction Types
// =============================================================================

// InstructionKind indicates what the Python worker should do next.
type InstructionKind string

const (
	InstructionKindRunAgent      InstructionKind = "run_agent"
	InstructionKindTerminate     InstructionKind = "terminate"
	InstructionKindWaitInterrupt InstructionKind = "wait_interrupt"
)

// Instruction tells Python worker what to do next.
type Instruction struct {
	Kind               InstructionKind          `json:"kind"`
	AgentName          string                   `json:"agent_name,omitempty"`
	AgentConfig        *config.AgentConfig      `json:"agent_config,omitempty"`
	Envelope           *envelope.Envelope       `json:"envelope,omitempty"`
	TerminalReason     *envelope.TerminalReason `json:"terminal_reason,omitempty"`
	TerminationMessage string                   `json:"termination_message,omitempty"`
	InterruptPending   bool                     `json:"interrupt_pending,omitempty"`
	Interrupt          *envelope.FlowInterrupt  `json:"interrupt,omitempty"`
}

// AgentExecutionMetrics contains metrics from agent execution.
type AgentExecutionMetrics struct {
	LLMCalls   int `json:"llm_calls"`
	ToolCalls  int `json:"tool_calls"`
	TokensIn   int `json:"tokens_in"`
	TokensOut  int `json:"tokens_out"`
	DurationMS int `json:"duration_ms"`
}

// =============================================================================
// Orchestration Session
// =============================================================================

// OrchestrationSession represents an active pipeline execution session.
type OrchestrationSession struct {
	ProcessID      string                 `json:"process_id"`
	PipelineConfig *config.PipelineConfig `json:"pipeline_config"`
	Envelope       *envelope.Envelope     `json:"envelope"`
	EdgeTraversals map[string]int         `json:"edge_traversals"` // "from->to" -> count
	Terminated     bool                   `json:"terminated"`
	TerminalReason *envelope.TerminalReason `json:"terminal_reason,omitempty"`
	CreatedAt      time.Time              `json:"created_at"`
	LastActivityAt time.Time              `json:"last_activity_at"`
}

// SessionState is the external representation of session state.
type SessionState struct {
	ProcessID      string                   `json:"process_id"`
	CurrentStage   string                   `json:"current_stage"`
	StageOrder     []string                 `json:"stage_order"`
	Envelope       map[string]any           `json:"envelope"`
	EdgeTraversals map[string]int           `json:"edge_traversals"`
	Terminated     bool                     `json:"terminated"`
	TerminalReason *envelope.TerminalReason `json:"terminal_reason,omitempty"`
}

// =============================================================================
// Orchestrator
// =============================================================================

// Orchestrator manages kernel-side pipeline execution.
type Orchestrator struct {
	kernel   *Kernel
	logger   Logger
	sessions map[string]*OrchestrationSession
	mu       sync.RWMutex
}

// NewOrchestrator creates a new Orchestrator.
func NewOrchestrator(kernel *Kernel, logger Logger) *Orchestrator {
	return &Orchestrator{
		kernel:   kernel,
		logger:   logger,
		sessions: make(map[string]*OrchestrationSession),
	}
}

// InitializeSession creates a new orchestration session.
// If force is false and a session already exists for processID, returns an error.
// If force is true, replaces any existing session (K8s-style create/replace pattern).
func (o *Orchestrator) InitializeSession(
	processID string,
	pipelineConfig *config.PipelineConfig,
	env *envelope.Envelope,
	force bool,
) (*SessionState, error) {
	o.mu.Lock()
	defer o.mu.Unlock()

	// Check for duplicate processID (K8s-style: error by default, allow force replace)
	if existing, exists := o.sessions[processID]; exists {
		if !force {
			return nil, fmt.Errorf("session already exists for process: %s (use force=true to replace)", processID)
		}
		if o.logger != nil {
			o.logger.Warn("session_force_replaced",
				"process_id", processID,
				"previous_stage", existing.Envelope.CurrentStage,
				"previous_iteration", existing.Envelope.Iteration,
			)
		}
	}

	// Validate pipeline config
	if err := pipelineConfig.Validate(); err != nil {
		return nil, fmt.Errorf("invalid pipeline config: %w", err)
	}

	// Initialize envelope with pipeline bounds
	env.MaxIterations = pipelineConfig.MaxIterations
	env.MaxLLMCalls = pipelineConfig.MaxLLMCalls
	env.MaxAgentHops = pipelineConfig.MaxAgentHops
	env.StageOrder = pipelineConfig.GetStageOrder()

	// Set initial stage - always use first stage from pipeline unless explicitly set to a valid stage
	// Special stages (end, clarification, confirmation) are always valid
	specialStages := map[string]bool{"end": true, "clarification": true, "confirmation": true}
	if len(env.StageOrder) > 0 && !specialStages[env.CurrentStage] {
		// Check if current stage is a valid pipeline stage
		isValidStage := false
		for _, stage := range env.StageOrder {
			if stage == env.CurrentStage {
				isValidStage = true
				break
			}
		}
		if !isValidStage {
			env.CurrentStage = env.StageOrder[0]
		}
	}

	now := time.Now().UTC()
	session := &OrchestrationSession{
		ProcessID:      processID,
		PipelineConfig: pipelineConfig,
		Envelope:       env,
		EdgeTraversals: make(map[string]int),
		Terminated:     false,
		CreatedAt:      now,
		LastActivityAt: now,
	}

	o.sessions[processID] = session

	if o.logger != nil {
		o.logger.Info("orchestration_session_initialized",
			"process_id", processID,
			"pipeline", pipelineConfig.Name,
			"current_stage", env.CurrentStage,
			"stage_count", len(env.StageOrder),
		)
	}

	return o.buildSessionState(session), nil
}

// GetNextInstruction determines what Python worker should do next.
// Uses write lock because buildInstruction may modify session state.
func (o *Orchestrator) GetNextInstruction(processID string) (*Instruction, error) {
	o.mu.Lock()
	defer o.mu.Unlock()

	session, exists := o.sessions[processID]
	if !exists {
		return nil, fmt.Errorf("unknown process: %s", processID)
	}

	return o.buildInstruction(session), nil
}

// ProcessAgentResult handles agent completion and determines next step.
func (o *Orchestrator) ProcessAgentResult(
	processID string,
	agentName string,
	output map[string]any,
	metrics *AgentExecutionMetrics,
	success bool,
	errorMsg string,
) (*Instruction, error) {
	o.mu.Lock()
	defer o.mu.Unlock()

	session, exists := o.sessions[processID]
	if !exists {
		return nil, fmt.Errorf("unknown process: %s", processID)
	}

	// Early return for already terminated sessions
	if session.Terminated {
		return o.buildInstruction(session), nil
	}

	env := session.Envelope
	fromStage := env.CurrentStage

	// Update envelope with agent output
	if output != nil {
		env.SetOutput(agentName, output)
	}

	// Record usage in kernel resource tracker if available
	if metrics != nil && o.kernel != nil {
		o.kernel.Resources().RecordUsage(
			processID,
			metrics.LLMCalls,
			metrics.ToolCalls,
			1, // agent hop
			metrics.TokensIn,
			metrics.TokensOut,
		)
	}

	// Record agent completion in processing history
	llmCalls := 0
	durationMS := 0
	if metrics != nil {
		llmCalls = metrics.LLMCalls
		durationMS = metrics.DurationMS
	}
	env.RecordAgentComplete(agentName, "success", nil, llmCalls, durationMS)

	session.LastActivityAt = time.Now().UTC()

	// Handle error
	if !success {
		agentConfig := session.PipelineConfig.GetAgent(agentName)
		if agentConfig != nil && agentConfig.ErrorNext != "" {
			env.CurrentStage = agentConfig.ErrorNext
			if o.logger != nil {
				o.logger.Info("agent_error_routing",
					"process_id", processID,
					"agent", agentName,
					"error", errorMsg,
					"next_stage", agentConfig.ErrorNext,
				)
			}
		} else {
			// Terminate on unhandled error
			reason := envelope.TerminalReasonToolFailedFatally
			session.Terminated = true
			session.TerminalReason = &reason
			env.Terminate(errorMsg, &reason)

			if o.logger != nil {
				o.logger.Warn("agent_error_terminated",
					"process_id", processID,
					"agent", agentName,
					"error", errorMsg,
				)
			}

			return o.buildInstruction(session), nil
		}
	} else {
		// Evaluate routing rules to determine next stage
		toStage := o.evaluateRouting(session, agentName, output)

		// Track edge traversal
		if fromStage != toStage && toStage != "end" {
			edgeKey := fmt.Sprintf("%s->%s", fromStage, toStage)
			session.EdgeTraversals[edgeKey]++

			// Detect loop-back (going to earlier stage)
			if o.isLoopBack(session, fromStage, toStage) {
				env.IncrementIteration(nil)
				if o.logger != nil {
					o.logger.Info("loop_detected",
						"process_id", processID,
						"from", fromStage,
						"to", toStage,
						"iteration", env.Iteration,
					)
				}
			}

			// Check edge limit
			if !o.checkEdgeLimits(session, fromStage, toStage) {
				reason := envelope.TerminalReasonMaxLoopExceeded
				session.Terminated = true
				session.TerminalReason = &reason
				env.Terminate(fmt.Sprintf("edge limit exceeded: %s", edgeKey), &reason)

				if o.logger != nil {
					o.logger.Warn("edge_limit_exceeded",
						"process_id", processID,
						"edge", edgeKey,
						"count", session.EdgeTraversals[edgeKey],
					)
				}

				return o.buildInstruction(session), nil
			}
		}

		env.CurrentStage = toStage

		if o.logger != nil {
			o.logger.Info("agent_completed_routing",
				"process_id", processID,
				"agent", agentName,
				"from_stage", fromStage,
				"to_stage", toStage,
			)
		}
	}

	return o.buildInstruction(session), nil
}

// GetSessionState returns the current session state.
func (o *Orchestrator) GetSessionState(processID string) (*SessionState, error) {
	o.mu.RLock()
	defer o.mu.RUnlock()

	session, exists := o.sessions[processID]
	if !exists {
		return nil, fmt.Errorf("unknown process: %s", processID)
	}

	return o.buildSessionState(session), nil
}

// CleanupSession removes a session.
func (o *Orchestrator) CleanupSession(processID string) {
	o.mu.Lock()
	defer o.mu.Unlock()
	delete(o.sessions, processID)
}

// =============================================================================
// Internal Methods
// =============================================================================

// buildInstruction creates an Instruction based on current session state.
func (o *Orchestrator) buildInstruction(session *OrchestrationSession) *Instruction {
	env := session.Envelope

	// Check if already terminated
	if session.Terminated {
		return &Instruction{
			Kind:               InstructionKindTerminate,
			TerminalReason:     session.TerminalReason,
			TerminationMessage: "Pipeline terminated",
			Envelope:           env,
		}
	}

	// Check if at end stage
	if env.CurrentStage == "end" {
		reason := envelope.TerminalReasonCompleted
		session.Terminated = true
		session.TerminalReason = &reason
		env.Terminate("Pipeline completed", &reason)

		return &Instruction{
			Kind:               InstructionKindTerminate,
			TerminalReason:     &reason,
			TerminationMessage: "Pipeline completed successfully",
			Envelope:           env,
		}
	}

	// Check for interrupt
	if env.InterruptPending {
		return &Instruction{
			Kind:             InstructionKindWaitInterrupt,
			InterruptPending: true,
			Interrupt:        env.Interrupt,
			Envelope:         env,
		}
	}

	// Check bounds
	canContinue, termReason := o.checkBounds(session)
	if !canContinue {
		session.Terminated = true
		session.TerminalReason = &termReason
		env.Terminate(string(termReason), &termReason)

		return &Instruction{
			Kind:               InstructionKindTerminate,
			TerminalReason:     &termReason,
			TerminationMessage: string(termReason),
			Envelope:           env,
		}
	}

	// Get agent config for current stage
	agentConfig := session.PipelineConfig.GetAgent(env.CurrentStage)
	if agentConfig == nil {
		reason := envelope.TerminalReasonToolFailedFatally
		session.Terminated = true
		session.TerminalReason = &reason
		msg := fmt.Sprintf("unknown stage: %s", env.CurrentStage)
		env.Terminate(msg, &reason)

		return &Instruction{
			Kind:               InstructionKindTerminate,
			TerminalReason:     &reason,
			TerminationMessage: msg,
			Envelope:           env,
		}
	}

	// Record agent start
	env.RecordAgentStart(env.CurrentStage, agentConfig.StageOrder)

	// Return instruction to run agent
	return &Instruction{
		Kind:        InstructionKindRunAgent,
		AgentName:   env.CurrentStage,
		AgentConfig: agentConfig,
		Envelope:    env,
	}
}

// evaluateRouting evaluates routing rules to determine next stage.
func (o *Orchestrator) evaluateRouting(session *OrchestrationSession, agentName string, output map[string]any) string {
	agentConfig := session.PipelineConfig.GetAgent(agentName)
	if agentConfig == nil {
		return "end"
	}

	// Evaluate routing rules in order
	for _, rule := range agentConfig.RoutingRules {
		value, exists := output[rule.Condition]
		if !exists {
			continue
		}

		// Compare values (handle different types)
		if o.valuesMatch(value, rule.Value) {
			return rule.Target
		}
	}

	// Use default next
	if agentConfig.DefaultNext != "" {
		return agentConfig.DefaultNext
	}

	return "end"
}

// valuesMatch compares two values for routing rule matching.
func (o *Orchestrator) valuesMatch(actual, expected any) bool {
	// Handle string comparison
	if actualStr, ok := actual.(string); ok {
		if expectedStr, ok := expected.(string); ok {
			return actualStr == expectedStr
		}
	}

	// Handle bool comparison
	if actualBool, ok := actual.(bool); ok {
		if expectedBool, ok := expected.(bool); ok {
			return actualBool == expectedBool
		}
	}

	// Handle numeric comparison
	if actualNum, ok := toFloat64(actual); ok {
		if expectedNum, ok := toFloat64(expected); ok {
			return actualNum == expectedNum
		}
	}

	// Fallback to JSON comparison
	actualJSON, _ := json.Marshal(actual)
	expectedJSON, _ := json.Marshal(expected)
	return string(actualJSON) == string(expectedJSON)
}

// toFloat64 converts a value to float64 if possible.
func toFloat64(v any) (float64, bool) {
	switch n := v.(type) {
	case int:
		return float64(n), true
	case int32:
		return float64(n), true
	case int64:
		return float64(n), true
	case float32:
		return float64(n), true
	case float64:
		return n, true
	default:
		return 0, false
	}
}

// checkBounds checks if session can continue based on resource limits.
func (o *Orchestrator) checkBounds(session *OrchestrationSession) (bool, envelope.TerminalReason) {
	env := session.Envelope

	if env.Iteration >= env.MaxIterations {
		return false, envelope.TerminalReasonMaxIterationsExceeded
	}
	if env.LLMCallCount >= env.MaxLLMCalls {
		return false, envelope.TerminalReasonMaxLLMCallsExceeded
	}
	if env.AgentHopCount >= env.MaxAgentHops {
		return false, envelope.TerminalReasonMaxAgentHopsExceeded
	}

	return true, ""
}

// checkEdgeLimits checks if edge traversal is within limits.
func (o *Orchestrator) checkEdgeLimits(session *OrchestrationSession, from, to string) bool {
	edgeKey := fmt.Sprintf("%s->%s", from, to)
	limit := session.PipelineConfig.GetEdgeLimit(from, to)

	if limit <= 0 {
		// No limit configured
		return true
	}

	count := session.EdgeTraversals[edgeKey]
	return count <= limit
}

// isLoopBack detects if routing goes back to an earlier stage.
func (o *Orchestrator) isLoopBack(session *OrchestrationSession, from, to string) bool {
	stageOrder := session.Envelope.StageOrder

	fromIdx := -1
	toIdx := -1
	for i, stage := range stageOrder {
		if stage == from {
			fromIdx = i
		}
		if stage == to {
			toIdx = i
		}
	}

	// Loop-back if going to an earlier stage in the order
	return fromIdx >= 0 && toIdx >= 0 && toIdx < fromIdx
}

// buildSessionState creates external SessionState from internal session.
func (o *Orchestrator) buildSessionState(session *OrchestrationSession) *SessionState {
	return &SessionState{
		ProcessID:      session.ProcessID,
		CurrentStage:   session.Envelope.CurrentStage,
		StageOrder:     session.Envelope.StageOrder,
		Envelope:       session.Envelope.ToStateDict(),
		EdgeTraversals: session.EdgeTraversals,
		Terminated:     session.Terminated,
		TerminalReason: session.TerminalReason,
	}
}

// GetSessionCount returns the number of active sessions.
func (o *Orchestrator) GetSessionCount() int {
	o.mu.RLock()
	defer o.mu.RUnlock()
	return len(o.sessions)
}

// CleanupStaleSessions removes sessions that are terminated or inactive beyond staleDuration.
// Returns the number of sessions cleaned up.
func (o *Orchestrator) CleanupStaleSessions(staleDuration time.Duration) int {
	o.mu.Lock()
	defer o.mu.Unlock()

	cutoff := time.Now().UTC().Add(-staleDuration)
	cleaned := 0

	for pid, session := range o.sessions {
		// Clean up terminated sessions or sessions inactive beyond cutoff
		if session.Terminated || session.LastActivityAt.Before(cutoff) {
			delete(o.sessions, pid)
			cleaned++

			if o.logger != nil {
				o.logger.Debug("session_cleaned_up",
					"process_id", pid,
					"terminated", session.Terminated,
					"last_activity", session.LastActivityAt.Format(time.RFC3339),
				)
			}
		}
	}

	return cleaned
}
