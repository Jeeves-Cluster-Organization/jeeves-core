// Package envelope provides the GenericEnvelope - Dynamic output slots for centralized agent architecture.
//
// This replaces the hardcoded per-agent output fields in CoreEnvelope with
// a single dynamic `Outputs` map that any agent can write to.
//
// Design:
//   - Outputs: map[string]map[string]any where key = agent output_key
//   - Stage tracking uses string names, not enum
//   - Unified interrupt handling via FlowInterrupt
package envelope

import (
	"time"

	"github.com/google/uuid"
)

// =============================================================================
// Unified Flow Interrupt System
// =============================================================================

// InterruptKind represents the type of flow interrupt.
// These are GENERIC interrupt types - capability layer defines specific usage.
type InterruptKind string

const (
	// InterruptKindClarification is for user clarification requests.
	InterruptKindClarification InterruptKind = "clarification"
	// InterruptKindConfirmation is for user confirmation requests.
	InterruptKindConfirmation InterruptKind = "confirmation"
	// InterruptKindAgentReview is for agent review requests (e.g., human-in-the-loop).
	InterruptKindAgentReview InterruptKind = "agent_review"
	// InterruptKindCheckpoint is for checkpoint pauses.
	InterruptKindCheckpoint InterruptKind = "checkpoint"
	// InterruptKindResourceExhausted is for rate limit/resource exhaustion.
	InterruptKindResourceExhausted InterruptKind = "resource_exhausted"
	// InterruptKindTimeout is for timeout interrupts.
	InterruptKindTimeout InterruptKind = "timeout"
	// InterruptKindSystemError is for system error interrupts.
	InterruptKindSystemError InterruptKind = "system_error"
)

// InterruptResponse represents a response to an interrupt.
type InterruptResponse struct {
	Text       *string        `json:"text,omitempty"`       // For clarification
	Approved   *bool          `json:"approved,omitempty"`   // For confirmation
	Decision   *string        `json:"decision,omitempty"`   // For critic (approve/reject/modify)
	Data       map[string]any `json:"data,omitempty"`       // Extensible
	ReceivedAt time.Time      `json:"received_at"`
}

// FlowInterrupt represents a unified flow interrupt.
type FlowInterrupt struct {
	Kind       InterruptKind     `json:"kind"`
	ID         string            `json:"id"`
	Question   string            `json:"question,omitempty"`   // For clarification
	Message    string            `json:"message,omitempty"`    // For confirmation
	Data       map[string]any    `json:"data,omitempty"`       // Extensible payload
	Response   *InterruptResponse `json:"response,omitempty"`
	CreatedAt  time.Time         `json:"created_at"`
	ExpiresAt  *time.Time        `json:"expires_at,omitempty"`
}

// InterruptOption is a functional option for configuring interrupts.
type InterruptOption func(*FlowInterrupt)

// WithQuestion sets the question for a clarification interrupt.
func WithQuestion(q string) InterruptOption {
	return func(i *FlowInterrupt) { i.Question = q }
}

// WithMessage sets the message for a confirmation interrupt.
func WithMessage(m string) InterruptOption {
	return func(i *FlowInterrupt) { i.Message = m }
}

// WithExpiry sets the expiry duration for an interrupt.
func WithExpiry(d time.Duration) InterruptOption {
	return func(i *FlowInterrupt) {
		t := time.Now().UTC().Add(d)
		i.ExpiresAt = &t
	}
}

// WithInterruptData sets additional data for an interrupt.
func WithInterruptData(d map[string]any) InterruptOption {
	return func(i *FlowInterrupt) { i.Data = d }
}

// ProcessingRecord represents a record of a single agent processing step.
type ProcessingRecord struct {
	Agent       string     `json:"agent"`
	StageOrder  int        `json:"stage_order"`
	StartedAt   time.Time  `json:"started_at"`
	CompletedAt *time.Time `json:"completed_at,omitempty"`
	DurationMS  int        `json:"duration_ms"`
	Status      string     `json:"status"` // "running", "success", "error", "skipped"
	Error       *string    `json:"error,omitempty"`
	LLMCalls    int        `json:"llm_calls"`
}

// GenericEnvelope is the envelope with dynamic output slots for centralized agent architecture.
//
// Unlike CoreEnvelope which has hardcoded fields (perception, intent, plan, etc.),
// GenericEnvelope uses a single `Outputs` map where any agent can write results.
//
// Example:
//
//	envelope.Outputs["perception"] = map[string]any{"normalized_input": "...", "context": {...}}
//	envelope.Outputs["intent"] = map[string]any{"intent": "trace_flow", "goals": [...]}
//	envelope.Outputs["my_custom_agent"] = map[string]any{"custom_field": "value"}
//
// This enables:
//   - Variable number of agents via configuration
//   - Custom agents without schema changes
//   - Any routing flow
type GenericEnvelope struct {
	// Identification
	EnvelopeID string `json:"envelope_id"`
	RequestID  string `json:"request_id"`
	UserID     string `json:"user_id"`
	SessionID  string `json:"session_id"`

	// Original Input
	RawInput   string    `json:"raw_input"`
	ReceivedAt time.Time `json:"received_at"`

	// Dynamic Agent Outputs
	Outputs map[string]map[string]any `json:"outputs"`

	// Pipeline State (String-based, not enum)
	CurrentStage  string   `json:"current_stage"`
	StageOrder    []string `json:"stage_order"`
	Iteration     int      `json:"iteration"`
	MaxIterations int      `json:"max_iterations"`

	// DAG Execution State (for parallel execution)
	// ActiveStages: stages currently executing (may be multiple in parallel)
	ActiveStages map[string]bool `json:"active_stages,omitempty"`
	// CompletedStageSet: stages that have completed successfully
	CompletedStageSet map[string]bool `json:"completed_stage_set,omitempty"`
	// FailedStages: stages that failed (with error messages)
	FailedStages map[string]string `json:"failed_stages,omitempty"`
	// DAGMode: whether running in DAG parallel mode
	DAGMode bool `json:"dag_mode,omitempty"`

	// Bounds Tracking
	LLMCallCount    int             `json:"llm_call_count"`
	MaxLLMCalls     int             `json:"max_llm_calls"`
	AgentHopCount   int             `json:"agent_hop_count"`
	MaxAgentHops    int             `json:"max_agent_hops"`
	TerminalReason_ *TerminalReason `json:"terminal_reason,omitempty"`

	// Control Flow
	Terminated        bool    `json:"terminated"`
	TerminationReason *string `json:"termination_reason,omitempty"`

	// Unified Interrupt Handling
	InterruptPending bool          `json:"interrupt_pending"`
	Interrupt        *FlowInterrupt `json:"interrupt,omitempty"`

	// Multi-Stage Execution
	CompletedStages      []map[string]any  `json:"completed_stages"`
	CurrentStageNumber   int               `json:"current_stage_number"`
	MaxStages            int               `json:"max_stages"`
	AllGoals             []string          `json:"all_goals"`
	RemainingGoals       []string          `json:"remaining_goals"`
	GoalCompletionStatus map[string]string `json:"goal_completion_status"`

	// Retry Context
	PriorPlans     []map[string]any `json:"prior_plans"`
	CriticFeedback []string         `json:"critic_feedback"`

	// Audit Trail
	ProcessingHistory []ProcessingRecord `json:"processing_history"`
	Errors            []map[string]any   `json:"errors"`

	// Timing
	CreatedAt   time.Time  `json:"created_at"`
	CompletedAt *time.Time `json:"completed_at,omitempty"`

	// Metadata
	Metadata map[string]any `json:"metadata"`
}

// NewGenericEnvelope creates a new GenericEnvelope with default values.
func NewGenericEnvelope() *GenericEnvelope {
	now := time.Now().UTC()
	return &GenericEnvelope{
		EnvelopeID:           "env_" + uuid.New().String()[:16],
		RequestID:            "req_" + uuid.New().String()[:16],
		UserID:               "anonymous",
		SessionID:            "sess_" + uuid.New().String()[:16],
		RawInput:             "",
		ReceivedAt:           now,
		Outputs:              make(map[string]map[string]any),
		CurrentStage:         "start",
		StageOrder:           []string{},
		Iteration:            0,
		MaxIterations:        3,
		ActiveStages:         make(map[string]bool),
		CompletedStageSet:    make(map[string]bool),
		FailedStages:         make(map[string]string),
		DAGMode:              false,
		LLMCallCount:         0,
		MaxLLMCalls:          10,
		AgentHopCount:        0,
		MaxAgentHops:     21,
		Terminated:       false,
		InterruptPending: false,
		Interrupt:        nil,
		CompletedStages:  []map[string]any{},
		CurrentStageNumber:   1,
		MaxStages:            5,
		AllGoals:             []string{},
		RemainingGoals:       []string{},
		GoalCompletionStatus: make(map[string]string),
		PriorPlans:           []map[string]any{},
		CriticFeedback:       []string{},
		ProcessingHistory:    []ProcessingRecord{},
		Errors:               []map[string]any{},
		CreatedAt:            now,
		Metadata:             make(map[string]any),
	}
}

// =============================================================================
// DAG Execution Helpers
// =============================================================================

// StartStage marks a stage as actively executing.
func (e *GenericEnvelope) StartStage(stageName string) {
	if e.ActiveStages == nil {
		e.ActiveStages = make(map[string]bool)
	}
	e.ActiveStages[stageName] = true
}

// CompleteStage marks a stage as successfully completed.
func (e *GenericEnvelope) CompleteStage(stageName string) {
	if e.CompletedStageSet == nil {
		e.CompletedStageSet = make(map[string]bool)
	}
	e.CompletedStageSet[stageName] = true
	delete(e.ActiveStages, stageName)
}

// FailStage marks a stage as failed with an error message.
func (e *GenericEnvelope) FailStage(stageName string, errorMsg string) {
	if e.FailedStages == nil {
		e.FailedStages = make(map[string]string)
	}
	e.FailedStages[stageName] = errorMsg
	delete(e.ActiveStages, stageName)
}

// IsStageCompleted returns true if the stage has completed successfully.
func (e *GenericEnvelope) IsStageCompleted(stageName string) bool {
	if e.CompletedStageSet == nil {
		return false
	}
	return e.CompletedStageSet[stageName]
}

// IsStageActive returns true if the stage is currently executing.
func (e *GenericEnvelope) IsStageActive(stageName string) bool {
	if e.ActiveStages == nil {
		return false
	}
	return e.ActiveStages[stageName]
}

// IsStageFailed returns true if the stage has failed.
func (e *GenericEnvelope) IsStageFailed(stageName string) bool {
	if e.FailedStages == nil {
		return false
	}
	_, failed := e.FailedStages[stageName]
	return failed
}

// GetActiveStageCount returns the number of currently active stages.
func (e *GenericEnvelope) GetActiveStageCount() int {
	if e.ActiveStages == nil {
		return 0
	}
	return len(e.ActiveStages)
}

// GetCompletedStageCount returns the number of completed stages.
func (e *GenericEnvelope) GetCompletedStageCount() int {
	if e.CompletedStageSet == nil {
		return 0
	}
	return len(e.CompletedStageSet)
}

// AllStagesComplete returns true if all stages in StageOrder are completed.
func (e *GenericEnvelope) AllStagesComplete() bool {
	if len(e.StageOrder) == 0 {
		return false
	}
	for _, stage := range e.StageOrder {
		if !e.IsStageCompleted(stage) {
			return false
		}
	}
	return true
}

// HasFailures returns true if any stages have failed.
func (e *GenericEnvelope) HasFailures() bool {
	return len(e.FailedStages) > 0
}

// =============================================================================
// Output Access Helpers
// =============================================================================

// GetOutput gets agent output by key.
func (e *GenericEnvelope) GetOutput(key string) map[string]any {
	return e.Outputs[key]
}

// SetOutput sets agent output by key.
func (e *GenericEnvelope) SetOutput(key string, value map[string]any) {
	e.Outputs[key] = value
}

// HasOutput checks if output exists for key.
func (e *GenericEnvelope) HasOutput(key string) bool {
	_, exists := e.Outputs[key]
	return exists
}

// =============================================================================
// Processing History
// =============================================================================

// RecordAgentStart records start of agent processing.
func (e *GenericEnvelope) RecordAgentStart(agentName string, stageOrder int) {
	entry := ProcessingRecord{
		Agent:      agentName,
		StageOrder: stageOrder,
		StartedAt:  time.Now().UTC(),
		Status:     "running",
	}
	e.ProcessingHistory = append(e.ProcessingHistory, entry)
	e.AgentHopCount++
}

// RecordAgentComplete records completion of agent processing.
func (e *GenericEnvelope) RecordAgentComplete(agentName string, status string, errorMsg *string, llmCalls int, durationMS int) {
	// Find the last entry for this agent
	for i := len(e.ProcessingHistory) - 1; i >= 0; i-- {
		entry := &e.ProcessingHistory[i]
		if entry.Agent == agentName && entry.Status == "running" {
			now := time.Now().UTC()
			entry.CompletedAt = &now
			entry.Status = status
			entry.Error = errorMsg
			entry.LLMCalls = llmCalls
			if durationMS > 0 {
				entry.DurationMS = durationMS
			} else if entry.CompletedAt != nil {
				entry.DurationMS = int(entry.CompletedAt.Sub(entry.StartedAt).Milliseconds())
			}
			break
		}
	}
	e.LLMCallCount += llmCalls
}

// =============================================================================
// Control Flow
// =============================================================================

// CanContinue checks if processing can continue.
func (e *GenericEnvelope) CanContinue() bool {
	if e.Terminated {
		return false
	}
	if e.InterruptPending {
		return false
	}
	if e.Iteration > e.MaxIterations {
		reason := TerminalReasonMaxIterationsExceeded
		e.TerminalReason_ = &reason
		return false
	}
	if e.LLMCallCount >= e.MaxLLMCalls {
		reason := TerminalReasonMaxLLMCallsExceeded
		e.TerminalReason_ = &reason
		return false
	}
	if e.AgentHopCount >= e.MaxAgentHops {
		reason := TerminalReasonMaxAgentHopsExceeded
		e.TerminalReason_ = &reason
		return false
	}
	return true
}

// SetInterrupt sets a flow interrupt with the given kind and options.
func (e *GenericEnvelope) SetInterrupt(kind InterruptKind, id string, opts ...InterruptOption) {
	e.InterruptPending = true
	e.Interrupt = &FlowInterrupt{
		Kind:      kind,
		ID:        id,
		CreatedAt: time.Now().UTC(),
	}
	for _, opt := range opts {
		opt(e.Interrupt)
	}
}

// ResolveInterrupt marks the interrupt as resolved with the given response.
func (e *GenericEnvelope) ResolveInterrupt(response InterruptResponse) {
	if e.Interrupt != nil {
		response.ReceivedAt = time.Now().UTC()
		e.Interrupt.Response = &response
	}
	e.InterruptPending = false
}

// ClearInterrupt clears the interrupt state without a response.
func (e *GenericEnvelope) ClearInterrupt() {
	e.InterruptPending = false
	e.Interrupt = nil
}

// HasPendingInterrupt returns true if there is a pending interrupt.
func (e *GenericEnvelope) HasPendingInterrupt() bool {
	return e.InterruptPending && e.Interrupt != nil
}

// GetInterruptKind returns the kind of pending interrupt, or empty string if none.
func (e *GenericEnvelope) GetInterruptKind() InterruptKind {
	if e.Interrupt != nil {
		return e.Interrupt.Kind
	}
	return ""
}

// Terminate marks envelope as terminated.
func (e *GenericEnvelope) Terminate(reason string, terminalReason *TerminalReason) {
	e.Terminated = true
	e.TerminationReason = &reason
	if terminalReason != nil {
		e.TerminalReason_ = terminalReason
	}
	now := time.Now().UTC()
	e.CompletedAt = &now
}

// =============================================================================
// Clone - Deep Copy for State Rollback
// =============================================================================

// Clone creates a deep copy of the envelope for checkpointing.
func (e *GenericEnvelope) Clone() *GenericEnvelope {
	clone := &GenericEnvelope{
		// Identification (copy by value)
		EnvelopeID: e.EnvelopeID,
		RequestID:  e.RequestID,
		UserID:     e.UserID,
		SessionID:  e.SessionID,

		// Input
		RawInput:   e.RawInput,
		ReceivedAt: e.ReceivedAt,

		// Pipeline state
		CurrentStage:  e.CurrentStage,
		Iteration:     e.Iteration,
		MaxIterations: e.MaxIterations,

		// Bounds
		LLMCallCount:  e.LLMCallCount,
		MaxLLMCalls:   e.MaxLLMCalls,
		AgentHopCount: e.AgentHopCount,
		MaxAgentHops:  e.MaxAgentHops,

		// Control flow
		Terminated:       e.Terminated,
		InterruptPending: e.InterruptPending,
		DAGMode:          e.DAGMode,

		// Multi-stage
		CurrentStageNumber: e.CurrentStageNumber,
		MaxStages:          e.MaxStages,

		// Timing
		CreatedAt: e.CreatedAt,
	}

	// Deep copy slices
	clone.StageOrder = copyStringSlice(e.StageOrder)
	clone.AllGoals = copyStringSlice(e.AllGoals)
	clone.RemainingGoals = copyStringSlice(e.RemainingGoals)
	clone.CriticFeedback = copyStringSlice(e.CriticFeedback)
	clone.CompletedStages = deepCopyMapSlice(e.CompletedStages)
	clone.PriorPlans = deepCopyMapSlice(e.PriorPlans)
	clone.Errors = deepCopyMapSlice(e.Errors)
	clone.ProcessingHistory = copyProcessingHistory(e.ProcessingHistory)

	// Deep copy maps
	clone.Outputs = deepCopyOutputs(e.Outputs)
	clone.ActiveStages = copyStringBoolMap(e.ActiveStages)
	clone.CompletedStageSet = copyStringBoolMap(e.CompletedStageSet)
	clone.FailedStages = copyStringStringMap(e.FailedStages)
	clone.GoalCompletionStatus = copyStringStringMap(e.GoalCompletionStatus)
	clone.Metadata = deepCopyAnyMap(e.Metadata)

	// Deep copy pointers
	if e.TerminalReason_ != nil {
		reason := *e.TerminalReason_
		clone.TerminalReason_ = &reason
	}
	if e.TerminationReason != nil {
		reason := *e.TerminationReason
		clone.TerminationReason = &reason
	}
	if e.Interrupt != nil {
		clone.Interrupt = e.Interrupt.Clone()
	}
	if e.CompletedAt != nil {
		t := *e.CompletedAt
		clone.CompletedAt = &t
	}

	return clone
}

// Clone creates a deep copy of the FlowInterrupt.
func (i *FlowInterrupt) Clone() *FlowInterrupt {
	clone := &FlowInterrupt{
		Kind:      i.Kind,
		ID:        i.ID,
		Question:  i.Question,
		Message:   i.Message,
		CreatedAt: i.CreatedAt,
	}
	if i.Data != nil {
		clone.Data = deepCopyAnyMap(i.Data)
	}
	if i.ExpiresAt != nil {
		t := *i.ExpiresAt
		clone.ExpiresAt = &t
	}
	if i.Response != nil {
		clone.Response = &InterruptResponse{
			ReceivedAt: i.Response.ReceivedAt,
		}
		if i.Response.Text != nil {
			text := *i.Response.Text
			clone.Response.Text = &text
		}
		if i.Response.Approved != nil {
			approved := *i.Response.Approved
			clone.Response.Approved = &approved
		}
		if i.Response.Decision != nil {
			decision := *i.Response.Decision
			clone.Response.Decision = &decision
		}
		if i.Response.Data != nil {
			clone.Response.Data = deepCopyAnyMap(i.Response.Data)
		}
	}
	return clone
}

// Helper functions for deep copying
func copyStringSlice(s []string) []string {
	if s == nil {
		return nil
	}
	result := make([]string, len(s))
	copy(result, s)
	return result
}

func copyStringBoolMap(m map[string]bool) map[string]bool {
	if m == nil {
		return nil
	}
	result := make(map[string]bool, len(m))
	for k, v := range m {
		result[k] = v
	}
	return result
}

func copyStringStringMap(m map[string]string) map[string]string {
	if m == nil {
		return nil
	}
	result := make(map[string]string, len(m))
	for k, v := range m {
		result[k] = v
	}
	return result
}

func deepCopyOutputs(m map[string]map[string]any) map[string]map[string]any {
	if m == nil {
		return nil
	}
	result := make(map[string]map[string]any, len(m))
	for k, v := range m {
		result[k] = deepCopyAnyMap(v)
	}
	return result
}

func deepCopyAnyMap(m map[string]any) map[string]any {
	if m == nil {
		return nil
	}
	result := make(map[string]any, len(m))
	for k, v := range m {
		result[k] = deepCopyValue(v)
	}
	return result
}

func deepCopyValue(v any) any {
	switch val := v.(type) {
	case map[string]any:
		return deepCopyAnyMap(val)
	case []any:
		result := make([]any, len(val))
		for i, item := range val {
			result[i] = deepCopyValue(item)
		}
		return result
	case []string:
		return copyStringSlice(val)
	default:
		return v // Primitives are copied by value
	}
}

func deepCopyMapSlice(s []map[string]any) []map[string]any {
	if s == nil {
		return nil
	}
	result := make([]map[string]any, len(s))
	for i, m := range s {
		result[i] = deepCopyAnyMap(m)
	}
	return result
}

func copyProcessingHistory(h []ProcessingRecord) []ProcessingRecord {
	if h == nil {
		return nil
	}
	result := make([]ProcessingRecord, len(h))
	for i, r := range h {
		result[i] = ProcessingRecord{
			Agent:      r.Agent,
			StageOrder: r.StageOrder,
			StartedAt:  r.StartedAt,
			DurationMS: r.DurationMS,
			Status:     r.Status,
			LLMCalls:   r.LLMCalls,
		}
		if r.CompletedAt != nil {
			t := *r.CompletedAt
			result[i].CompletedAt = &t
		}
		if r.Error != nil {
			err := *r.Error
			result[i].Error = &err
		}
	}
	return result
}

// =============================================================================
// Goal Tracking
// =============================================================================

// InitializeGoals initializes goal tracking from intent output.
func (e *GenericEnvelope) InitializeGoals(goals []string) {
	e.AllGoals = make([]string, len(goals))
	copy(e.AllGoals, goals)
	e.RemainingGoals = make([]string, len(goals))
	copy(e.RemainingGoals, goals)
	e.GoalCompletionStatus = make(map[string]string)
	for _, goal := range goals {
		e.GoalCompletionStatus[goal] = "pending"
	}
}

// AdvanceStage advances to next stage after critic assessment.
// Returns true if there are more goals to process.
func (e *GenericEnvelope) AdvanceStage(satisfiedGoals []string, stageSummary map[string]any) bool {
	for _, goal := range satisfiedGoals {
		e.GoalCompletionStatus[goal] = "satisfied"
		// Remove from remaining goals
		newRemaining := make([]string, 0, len(e.RemainingGoals))
		for _, g := range e.RemainingGoals {
			if g != goal {
				newRemaining = append(newRemaining, g)
			}
		}
		e.RemainingGoals = newRemaining
	}

	planOutput := e.Outputs["plan"]
	var planID any
	if planOutput != nil {
		planID = planOutput["plan_id"]
	}

	e.CompletedStages = append(e.CompletedStages, map[string]any{
		"stage_number":    e.CurrentStageNumber,
		"satisfied_goals": satisfiedGoals,
		"summary":         stageSummary,
		"plan_id":         planID,
	})

	if len(e.RemainingGoals) == 0 {
		return false
	}

	if e.CurrentStageNumber >= e.MaxStages {
		return false
	}

	e.CurrentStageNumber++

	// Reset stage-specific outputs
	for _, key := range []string{"plan", "execution", "critic"} {
		delete(e.Outputs, key)
	}

	return true
}

// GetStageContext gets accumulated context from completed stages.
func (e *GenericEnvelope) GetStageContext() map[string]any {
	satisfiedGoals := make([]string, 0)
	for goal, status := range e.GoalCompletionStatus {
		if status == "satisfied" {
			satisfiedGoals = append(satisfiedGoals, goal)
		}
	}

	return map[string]any{
		"current_stage":          e.CurrentStageNumber,
		"completed_stages":       e.CompletedStages,
		"remaining_goals":        e.RemainingGoals,
		"goal_completion_status": e.GoalCompletionStatus,
		"satisfied_goals":        satisfiedGoals,
	}
}

// =============================================================================
// Retry Management
// =============================================================================

// IncrementIteration increments iteration for retry.
func (e *GenericEnvelope) IncrementIteration(feedback *string) {
	e.Iteration++
	if feedback != nil {
		e.CriticFeedback = append(e.CriticFeedback, *feedback)
	}
	// Store current plan
	if plan, exists := e.Outputs["plan"]; exists {
		planCopy := make(map[string]any)
		for k, v := range plan {
			planCopy[k] = v
		}
		e.PriorPlans = append(e.PriorPlans, planCopy)
	}
	// Reset downstream outputs
	for _, key := range []string{"plan", "arbiter", "execution", "synthesizer", "critic"} {
		delete(e.Outputs, key)
	}
}

// =============================================================================
// Response Helpers
// =============================================================================

// GetFinalResponse gets the final response text.
func (e *GenericEnvelope) GetFinalResponse() *string {
	// If there's a pending interrupt with a question/message, return that
	if e.InterruptPending && e.Interrupt != nil {
		if e.Interrupt.Kind == InterruptKindClarification && e.Interrupt.Question != "" {
			return &e.Interrupt.Question
		}
		if e.Interrupt.Kind == InterruptKindConfirmation && e.Interrupt.Message != "" {
			return &e.Interrupt.Message
		}
	}
	if integration, exists := e.Outputs["integration"]; exists {
		if response, ok := integration["final_response"].(string); ok {
			return &response
		}
	}
	return nil
}

// ToResultDict converts to result dictionary for API responses.
func (e *GenericEnvelope) ToResultDict() map[string]any {
	result := map[string]any{
		"envelope_id":        e.EnvelopeID,
		"request_id":         e.RequestID,
		"user_id":            e.UserID,
		"session_id":         e.SessionID,
		"current_stage":      e.CurrentStage,
		"terminated":         e.Terminated,
		"termination_reason": e.TerminationReason,
		"response":           e.GetFinalResponse(),
		"interrupt_pending":  e.InterruptPending,
		"iteration":          e.Iteration,
		"llm_call_count":     e.LLMCallCount,
		"processing_time_ms": e.TotalProcessingTimeMS(),
		"errors":             e.Errors,
	}

	// Add interrupt details if present
	if e.Interrupt != nil {
		result["interrupt"] = map[string]any{
			"kind":       string(e.Interrupt.Kind),
			"id":         e.Interrupt.ID,
			"question":   e.Interrupt.Question,
			"message":    e.Interrupt.Message,
			"created_at": e.Interrupt.CreatedAt.Format(time.RFC3339),
		}
		// Set convenience flags for backward compatibility in API responses
		if e.Interrupt.Kind == InterruptKindClarification {
			result["clarification_needed"] = e.InterruptPending
			result["clarification_question"] = e.Interrupt.Question
		}
		if e.Interrupt.Kind == InterruptKindConfirmation {
			result["confirmation_needed"] = e.InterruptPending
			result["confirmation_message"] = e.Interrupt.Message
			result["confirmation_id"] = e.Interrupt.ID
		}
	}

	return result
}

// TotalProcessingTimeMS calculates total processing time from history.
func (e *GenericEnvelope) TotalProcessingTimeMS() int {
	total := 0
	for _, entry := range e.ProcessingHistory {
		total += entry.DurationMS
	}
	return total
}

// =============================================================================
// Serialization
// =============================================================================

// ToStateDict converts to state dict for persistence/runtime.
func (e *GenericEnvelope) ToStateDict() map[string]any {
	var terminalReasonStr *string
	if e.TerminalReason_ != nil {
		s := string(*e.TerminalReason_)
		terminalReasonStr = &s
	}

	var completedAtStr *string
	if e.CompletedAt != nil {
		s := e.CompletedAt.Format(time.RFC3339)
		completedAtStr = &s
	}

	// Serialize interrupt if present
	var interruptDict map[string]any
	if e.Interrupt != nil {
		interruptDict = map[string]any{
			"kind":       string(e.Interrupt.Kind),
			"id":         e.Interrupt.ID,
			"question":   e.Interrupt.Question,
			"message":    e.Interrupt.Message,
			"data":       e.Interrupt.Data,
			"created_at": e.Interrupt.CreatedAt.Format(time.RFC3339),
		}
		if e.Interrupt.ExpiresAt != nil {
			interruptDict["expires_at"] = e.Interrupt.ExpiresAt.Format(time.RFC3339)
		}
		if e.Interrupt.Response != nil {
			respDict := map[string]any{
				"received_at": e.Interrupt.Response.ReceivedAt.Format(time.RFC3339),
			}
			if e.Interrupt.Response.Text != nil {
				respDict["text"] = *e.Interrupt.Response.Text
			}
			if e.Interrupt.Response.Approved != nil {
				respDict["approved"] = *e.Interrupt.Response.Approved
			}
			if e.Interrupt.Response.Decision != nil {
				respDict["decision"] = *e.Interrupt.Response.Decision
			}
			if e.Interrupt.Response.Data != nil {
				respDict["data"] = e.Interrupt.Response.Data
			}
			interruptDict["response"] = respDict
		}
	}

	return map[string]any{
		"envelope_id":            e.EnvelopeID,
		"request_id":             e.RequestID,
		"user_id":                e.UserID,
		"session_id":             e.SessionID,
		"raw_input":              e.RawInput,
		"received_at":            e.ReceivedAt.Format(time.RFC3339),
		"outputs":                e.Outputs,
		"current_stage":          e.CurrentStage,
		"stage_order":            e.StageOrder,
		"iteration":              e.Iteration,
		"llm_call_count":         e.LLMCallCount,
		"agent_hop_count":        e.AgentHopCount,
		// DAG execution state
		"active_stages":         e.ActiveStages,
		"completed_stage_set":   e.CompletedStageSet,
		"failed_stages":         e.FailedStages,
		"dag_mode":              e.DAGMode,
		"terminated":             e.Terminated,
		"termination_reason":     e.TerminationReason,
		"terminal_reason":        terminalReasonStr,
		"interrupt_pending":      e.InterruptPending,
		"interrupt":              interruptDict,
		"completed_stages":       e.CompletedStages,
		"current_stage_number":   e.CurrentStageNumber,
		"all_goals":              e.AllGoals,
		"remaining_goals":        e.RemainingGoals,
		"goal_completion_status": e.GoalCompletionStatus,
		"prior_plans":            e.PriorPlans,
		"critic_feedback":        e.CriticFeedback,
		"errors":                 e.Errors,
		"completed_at":           completedAtStr,
		"metadata":               e.Metadata,
	}
}

// FromStateDict creates envelope from state dict.
func FromStateDict(state map[string]any) *GenericEnvelope {
	e := NewGenericEnvelope()

	if v, ok := state["envelope_id"].(string); ok {
		e.EnvelopeID = v
	}
	if v, ok := state["request_id"].(string); ok {
		e.RequestID = v
	}
	if v, ok := state["user_id"].(string); ok {
		e.UserID = v
	}
	if v, ok := state["session_id"].(string); ok {
		e.SessionID = v
	}
	if v, ok := state["raw_input"].(string); ok {
		e.RawInput = v
	}
	if v, ok := state["received_at"].(string); ok {
		if t, err := time.Parse(time.RFC3339, v); err == nil {
			e.ReceivedAt = t
		}
	}
	if v, ok := state["outputs"].(map[string]map[string]any); ok {
		e.Outputs = v
	} else if v, ok := state["outputs"].(map[string]any); ok {
		// Handle case where outputs is unmarshaled as map[string]any
		e.Outputs = make(map[string]map[string]any)
		for k, val := range v {
			if m, ok := val.(map[string]any); ok {
				e.Outputs[k] = m
			}
		}
	}
	if v, ok := state["current_stage"].(string); ok {
		e.CurrentStage = v
	}
	if v, ok := state["stage_order"].([]string); ok {
		e.StageOrder = v
	} else if v, ok := state["stage_order"].([]any); ok {
		e.StageOrder = make([]string, len(v))
		for i, s := range v {
			if str, ok := s.(string); ok {
				e.StageOrder[i] = str
			}
		}
	}
	if v, ok := state["iteration"].(int); ok {
		e.Iteration = v
	} else if v, ok := state["iteration"].(float64); ok {
		e.Iteration = int(v)
	}
	if v, ok := state["llm_call_count"].(int); ok {
		e.LLMCallCount = v
	} else if v, ok := state["llm_call_count"].(float64); ok {
		e.LLMCallCount = int(v)
	}
	if v, ok := state["agent_hop_count"].(int); ok {
		e.AgentHopCount = v
	} else if v, ok := state["agent_hop_count"].(float64); ok {
		e.AgentHopCount = int(v)
	}
	// DAG execution state
	if v, ok := state["active_stages"].(map[string]bool); ok {
		e.ActiveStages = v
	} else if v, ok := state["active_stages"].(map[string]any); ok {
		e.ActiveStages = make(map[string]bool)
		for k, val := range v {
			if b, ok := val.(bool); ok {
				e.ActiveStages[k] = b
			}
		}
	}
	if v, ok := state["completed_stage_set"].(map[string]bool); ok {
		e.CompletedStageSet = v
	} else if v, ok := state["completed_stage_set"].(map[string]any); ok {
		e.CompletedStageSet = make(map[string]bool)
		for k, val := range v {
			if b, ok := val.(bool); ok {
				e.CompletedStageSet[k] = b
			}
		}
	}
	if v, ok := state["failed_stages"].(map[string]string); ok {
		e.FailedStages = v
	} else if v, ok := state["failed_stages"].(map[string]any); ok {
		e.FailedStages = make(map[string]string)
		for k, val := range v {
			if s, ok := val.(string); ok {
				e.FailedStages[k] = s
			}
		}
	}
	if v, ok := state["dag_mode"].(bool); ok {
		e.DAGMode = v
	}
	if v, ok := state["terminated"].(bool); ok {
		e.Terminated = v
	}
	if v, ok := state["termination_reason"].(string); ok {
		e.TerminationReason = &v
	}
	if v, ok := state["terminal_reason"].(string); ok {
		reason := TerminalReason(v)
		e.TerminalReason_ = &reason
	}
	if v, ok := state["interrupt_pending"].(bool); ok {
		e.InterruptPending = v
	}
	if v, ok := state["interrupt"].(map[string]any); ok {
		e.Interrupt = &FlowInterrupt{}
		if kind, ok := v["kind"].(string); ok {
			e.Interrupt.Kind = InterruptKind(kind)
		}
		if id, ok := v["id"].(string); ok {
			e.Interrupt.ID = id
		}
		if q, ok := v["question"].(string); ok {
			e.Interrupt.Question = q
		}
		if m, ok := v["message"].(string); ok {
			e.Interrupt.Message = m
		}
		if d, ok := v["data"].(map[string]any); ok {
			e.Interrupt.Data = d
		}
		if createdAt, ok := v["created_at"].(string); ok {
			if t, err := time.Parse(time.RFC3339, createdAt); err == nil {
				e.Interrupt.CreatedAt = t
			}
		}
		if expiresAt, ok := v["expires_at"].(string); ok {
			if t, err := time.Parse(time.RFC3339, expiresAt); err == nil {
				e.Interrupt.ExpiresAt = &t
			}
		}
		if resp, ok := v["response"].(map[string]any); ok {
			e.Interrupt.Response = &InterruptResponse{}
			if text, ok := resp["text"].(string); ok {
				e.Interrupt.Response.Text = &text
			}
			if approved, ok := resp["approved"].(bool); ok {
				e.Interrupt.Response.Approved = &approved
			}
			if decision, ok := resp["decision"].(string); ok {
				e.Interrupt.Response.Decision = &decision
			}
			if data, ok := resp["data"].(map[string]any); ok {
				e.Interrupt.Response.Data = data
			}
			if receivedAt, ok := resp["received_at"].(string); ok {
				if t, err := time.Parse(time.RFC3339, receivedAt); err == nil {
					e.Interrupt.Response.ReceivedAt = t
				}
			}
		}
	}
	if v, ok := state["completed_stages"].([]map[string]any); ok {
		e.CompletedStages = v
	} else if v, ok := state["completed_stages"].([]any); ok {
		e.CompletedStages = make([]map[string]any, len(v))
		for i, s := range v {
			if m, ok := s.(map[string]any); ok {
				e.CompletedStages[i] = m
			}
		}
	}
	if v, ok := state["current_stage_number"].(int); ok {
		e.CurrentStageNumber = v
	} else if v, ok := state["current_stage_number"].(float64); ok {
		e.CurrentStageNumber = int(v)
	}
	if v, ok := state["all_goals"].([]string); ok {
		e.AllGoals = v
	} else if v, ok := state["all_goals"].([]any); ok {
		e.AllGoals = make([]string, len(v))
		for i, s := range v {
			if str, ok := s.(string); ok {
				e.AllGoals[i] = str
			}
		}
	}
	if v, ok := state["remaining_goals"].([]string); ok {
		e.RemainingGoals = v
	} else if v, ok := state["remaining_goals"].([]any); ok {
		e.RemainingGoals = make([]string, len(v))
		for i, s := range v {
			if str, ok := s.(string); ok {
				e.RemainingGoals[i] = str
			}
		}
	}
	if v, ok := state["goal_completion_status"].(map[string]string); ok {
		e.GoalCompletionStatus = v
	} else if v, ok := state["goal_completion_status"].(map[string]any); ok {
		e.GoalCompletionStatus = make(map[string]string)
		for k, val := range v {
			if str, ok := val.(string); ok {
				e.GoalCompletionStatus[k] = str
			}
		}
	}
	if v, ok := state["prior_plans"].([]map[string]any); ok {
		e.PriorPlans = v
	} else if v, ok := state["prior_plans"].([]any); ok {
		e.PriorPlans = make([]map[string]any, len(v))
		for i, s := range v {
			if m, ok := s.(map[string]any); ok {
				e.PriorPlans[i] = m
			}
		}
	}
	if v, ok := state["critic_feedback"].([]string); ok {
		e.CriticFeedback = v
	} else if v, ok := state["critic_feedback"].([]any); ok {
		e.CriticFeedback = make([]string, len(v))
		for i, s := range v {
			if str, ok := s.(string); ok {
				e.CriticFeedback[i] = str
			}
		}
	}
	if v, ok := state["errors"].([]map[string]any); ok {
		e.Errors = v
	} else if v, ok := state["errors"].([]any); ok {
		e.Errors = make([]map[string]any, len(v))
		for i, s := range v {
			if m, ok := s.(map[string]any); ok {
				e.Errors[i] = m
			}
		}
	}
	if v, ok := state["completed_at"].(string); ok {
		if t, err := time.Parse(time.RFC3339, v); err == nil {
			e.CompletedAt = &t
		}
	}
	if v, ok := state["metadata"].(map[string]any); ok {
		e.Metadata = v
	}

	return e
}

// CreateGenericEnvelope creates a new GenericEnvelope from user input.
func CreateGenericEnvelope(userMessage, userID, sessionID string, requestID *string, metadata map[string]any, stageOrder []string) *GenericEnvelope {
	e := NewGenericEnvelope()
	e.RawInput = userMessage
	e.UserID = userID
	e.SessionID = sessionID
	if requestID != nil {
		e.RequestID = *requestID
	}
	if metadata != nil {
		e.Metadata = metadata
	}
	if stageOrder != nil {
		e.StageOrder = stageOrder
	}
	return e
}
