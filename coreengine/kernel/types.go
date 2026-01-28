// Package kernel implements OS-level abstractions for the Jeeves kernel.
//
// This package provides process lifecycle management, resource quotas,
// and scheduling primitives similar to an operating system kernel.
//
// Key concepts:
//   - ProcessState: Process lifecycle states (NEW -> RUNNING -> TERMINATED)
//   - ResourceQuota: cgroups-style resource limits
//   - ProcessControlBlock: Kernel's view of a running request
package kernel

import (
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// =============================================================================
// Process States (mirrors OS process lifecycle)
// =============================================================================

// ProcessState represents the lifecycle state of a process.
// State transitions:
//
//	NEW -> READY -> RUNNING -> (WAITING | BLOCKED | TERMINATED)
//	WAITING -> READY (on event)
//	BLOCKED -> READY (on resource available)
type ProcessState string

const (
	// ProcessStateNew indicates a newly created process, not yet scheduled.
	ProcessStateNew ProcessState = "new"
	// ProcessStateReady indicates the process is ready to run, waiting for CPU.
	ProcessStateReady ProcessState = "ready"
	// ProcessStateRunning indicates the process is currently executing.
	ProcessStateRunning ProcessState = "running"
	// ProcessStateWaiting indicates the process is waiting for I/O or event.
	ProcessStateWaiting ProcessState = "waiting"
	// ProcessStateBlocked indicates the process is blocked on a resource.
	ProcessStateBlocked ProcessState = "blocked"
	// ProcessStateTerminated indicates the process has finished execution.
	ProcessStateTerminated ProcessState = "terminated"
	// ProcessStateZombie indicates the process terminated but not yet cleaned up.
	ProcessStateZombie ProcessState = "zombie"
)

// IsTerminal returns true if this is a terminal state.
func (s ProcessState) IsTerminal() bool {
	return s == ProcessStateTerminated || s == ProcessStateZombie
}

// CanSchedule returns true if the process can be scheduled.
func (s ProcessState) CanSchedule() bool {
	return s == ProcessStateNew || s == ProcessStateReady
}

// IsRunnable returns true if the process is ready to run.
func (s ProcessState) IsRunnable() bool {
	return s == ProcessStateReady
}

// =============================================================================
// Scheduling Priority
// =============================================================================

// SchedulingPriority represents the scheduling priority level.
type SchedulingPriority string

const (
	// PriorityRealtime is the highest priority (system critical).
	PriorityRealtime SchedulingPriority = "realtime"
	// PriorityHigh is for user-interactive operations.
	PriorityHigh SchedulingPriority = "high"
	// PriorityNormal is the default priority.
	PriorityNormal SchedulingPriority = "normal"
	// PriorityLow is for background tasks.
	PriorityLow SchedulingPriority = "low"
	// PriorityIdle is only scheduled when nothing else to do.
	PriorityIdle SchedulingPriority = "idle"
)

// Weight returns the scheduling weight (higher = more priority).
func (p SchedulingPriority) Weight() int {
	switch p {
	case PriorityRealtime:
		return 100
	case PriorityHigh:
		return 75
	case PriorityNormal:
		return 50
	case PriorityLow:
		return 25
	case PriorityIdle:
		return 1
	default:
		return 50
	}
}

// =============================================================================
// Resource Quotas (mirrors cgroups)
// =============================================================================

// ResourceQuota defines cgroups-style resource limits.
// Enforces resource constraints at the kernel level:
//   - Token limits (memory equivalent)
//   - Call limits (CPU time equivalent)
//   - Time limits (wall clock)
type ResourceQuota struct {
	// Token limits (like memory limits)
	MaxInputTokens   int `json:"max_input_tokens"`
	MaxOutputTokens  int `json:"max_output_tokens"`
	MaxContextTokens int `json:"max_context_tokens"`

	// Call limits (like CPU time)
	MaxLLMCalls   int `json:"max_llm_calls"`
	MaxToolCalls  int `json:"max_tool_calls"`
	MaxAgentHops  int `json:"max_agent_hops"`
	MaxIterations int `json:"max_iterations"`

	// Time limits
	TimeoutSeconds     int `json:"timeout_seconds"`
	SoftTimeoutSeconds int `json:"soft_timeout_seconds"` // Warn before hard timeout

	// Rate limits (per-user) - uses existing RateLimitConfig
	RateLimitRPM   int `json:"rate_limit_rpm,omitempty"`   // Requests per minute
	RateLimitRPH   int `json:"rate_limit_rph,omitempty"`   // Requests per hour
	RateLimitBurst int `json:"rate_limit_burst,omitempty"` // Burst allowance
}

// DefaultQuota returns sensible default resource limits.
func DefaultQuota() *ResourceQuota {
	return &ResourceQuota{
		MaxInputTokens:     4096,
		MaxOutputTokens:    2048,
		MaxContextTokens:   16384,
		MaxLLMCalls:        10,
		MaxToolCalls:       50,
		MaxAgentHops:       21,
		MaxIterations:      3,
		TimeoutSeconds:     300,
		SoftTimeoutSeconds: 240,
	}
}

// IsWithinBounds checks if the given usage is within quota.
func (q *ResourceQuota) IsWithinBounds(llmCalls, toolCalls, agentHops, iterations int) bool {
	return llmCalls <= q.MaxLLMCalls &&
		toolCalls <= q.MaxToolCalls &&
		agentHops <= q.MaxAgentHops &&
		iterations <= q.MaxIterations
}

// =============================================================================
// Resource Usage
// =============================================================================

// ResourceUsage tracks current resource consumption for a process.
type ResourceUsage struct {
	LLMCalls       int     `json:"llm_calls"`
	ToolCalls      int     `json:"tool_calls"`
	AgentHops      int     `json:"agent_hops"`
	Iterations     int     `json:"iterations"`
	TokensIn       int     `json:"tokens_in"`
	TokensOut      int     `json:"tokens_out"`
	ElapsedSeconds float64 `json:"elapsed_seconds"`
}

// ExceedsQuota checks if usage exceeds quota. Returns the reason or empty string.
func (u *ResourceUsage) ExceedsQuota(q *ResourceQuota) string {
	if u.LLMCalls > q.MaxLLMCalls {
		return "max_llm_calls_exceeded"
	}
	if u.ToolCalls > q.MaxToolCalls {
		return "max_tool_calls_exceeded"
	}
	if u.AgentHops > q.MaxAgentHops {
		return "max_agent_hops_exceeded"
	}
	if u.Iterations > q.MaxIterations {
		return "max_iterations_exceeded"
	}
	if u.ElapsedSeconds > float64(q.TimeoutSeconds) {
		return "timeout_exceeded"
	}
	return ""
}

// Clone returns a copy of the usage.
func (u *ResourceUsage) Clone() *ResourceUsage {
	return &ResourceUsage{
		LLMCalls:       u.LLMCalls,
		ToolCalls:      u.ToolCalls,
		AgentHops:      u.AgentHops,
		Iterations:     u.Iterations,
		TokensIn:       u.TokensIn,
		TokensOut:      u.TokensOut,
		ElapsedSeconds: u.ElapsedSeconds,
	}
}

// =============================================================================
// Process Control Block (PCB)
// =============================================================================

// ProcessControlBlock is the kernel's metadata about a running "process" (request).
// The actual request state is in Envelope; this tracks:
//   - Scheduling state
//   - Resource accounting
//   - Interrupt status
type ProcessControlBlock struct {
	// Identity
	PID       string `json:"pid"`        // Process ID (envelope_id)
	RequestID string `json:"request_id"` // Original request ID
	UserID    string `json:"user_id"`
	SessionID string `json:"session_id"`

	// State
	State    ProcessState       `json:"state"`
	Priority SchedulingPriority `json:"priority"`

	// Resource tracking
	Quota *ResourceQuota `json:"quota"`
	Usage *ResourceUsage `json:"usage"`

	// Scheduling timestamps
	CreatedAt       time.Time  `json:"created_at"`
	StartedAt       *time.Time `json:"started_at,omitempty"`
	CompletedAt     *time.Time `json:"completed_at,omitempty"`
	LastScheduledAt *time.Time `json:"last_scheduled_at,omitempty"`

	// Current execution
	CurrentStage   string `json:"current_stage,omitempty"`
	CurrentService string `json:"current_service,omitempty"`

	// Interrupt handling
	PendingInterrupt *envelope.InterruptKind `json:"pending_interrupt,omitempty"`
	InterruptData    map[string]any          `json:"interrupt_data,omitempty"`

	// Parent/child relationships (for sub-requests)
	ParentPID string   `json:"parent_pid,omitempty"`
	ChildPIDs []string `json:"child_pids,omitempty"`
}

// NewProcessControlBlock creates a new PCB with default values.
func NewProcessControlBlock(pid, requestID, userID, sessionID string) *ProcessControlBlock {
	now := time.Now().UTC()
	return &ProcessControlBlock{
		PID:       pid,
		RequestID: requestID,
		UserID:    userID,
		SessionID: sessionID,
		State:     ProcessStateNew,
		Priority:  PriorityNormal,
		Quota:     DefaultQuota(),
		Usage:     &ResourceUsage{},
		CreatedAt: now,
		ChildPIDs: []string{},
	}
}

// CanSchedule checks if process can be scheduled.
func (pcb *ProcessControlBlock) CanSchedule() bool {
	return pcb.State.CanSchedule()
}

// IsRunnable checks if process is runnable.
func (pcb *ProcessControlBlock) IsRunnable() bool {
	return pcb.State.IsRunnable()
}

// IsTerminated checks if process has terminated.
func (pcb *ProcessControlBlock) IsTerminated() bool {
	return pcb.State.IsTerminal()
}

// Start transitions process to RUNNING state.
func (pcb *ProcessControlBlock) Start() {
	now := time.Now().UTC()
	pcb.State = ProcessStateRunning
	pcb.StartedAt = &now
	pcb.LastScheduledAt = &now
}

// Complete transitions process to TERMINATED state.
func (pcb *ProcessControlBlock) Complete() {
	now := time.Now().UTC()
	pcb.State = ProcessStateTerminated
	pcb.CompletedAt = &now
	if pcb.StartedAt != nil {
		pcb.Usage.ElapsedSeconds = now.Sub(*pcb.StartedAt).Seconds()
	}
}

// Block transitions process to BLOCKED state.
func (pcb *ProcessControlBlock) Block(reason string) {
	pcb.State = ProcessStateBlocked
	if pcb.InterruptData == nil {
		pcb.InterruptData = make(map[string]any)
	}
	pcb.InterruptData["block_reason"] = reason
}

// Wait transitions process to WAITING state.
func (pcb *ProcessControlBlock) Wait(interruptKind envelope.InterruptKind) {
	pcb.State = ProcessStateWaiting
	pcb.PendingInterrupt = &interruptKind
}

// Resume transitions process from WAITING/BLOCKED to READY.
func (pcb *ProcessControlBlock) Resume() {
	pcb.State = ProcessStateReady
	pcb.PendingInterrupt = nil
	delete(pcb.InterruptData, "block_reason")
}

// RecordLLMCall records an LLM call.
func (pcb *ProcessControlBlock) RecordLLMCall(tokensIn, tokensOut int) {
	pcb.Usage.LLMCalls++
	pcb.Usage.TokensIn += tokensIn
	pcb.Usage.TokensOut += tokensOut
}

// RecordToolCall records a tool call.
func (pcb *ProcessControlBlock) RecordToolCall() {
	pcb.Usage.ToolCalls++
}

// RecordAgentHop records an agent hop.
func (pcb *ProcessControlBlock) RecordAgentHop() {
	pcb.Usage.AgentHops++
}

// CheckQuota checks if usage exceeds quota.
func (pcb *ProcessControlBlock) CheckQuota() string {
	return pcb.Usage.ExceedsQuota(pcb.Quota)
}

// =============================================================================
// Service Descriptor
// =============================================================================

// ServiceDescriptor describes a registered service (Mission System service).
// Services are like OS daemons that the kernel dispatches to.
type ServiceDescriptor struct {
	Name          string         `json:"name"`
	ServiceType   string         `json:"service_type"` // "flow", "worker", "vertical"
	Version       string         `json:"version"`
	Capabilities  []string       `json:"capabilities"`
	MaxConcurrent int            `json:"max_concurrent"`
	CurrentLoad   int            `json:"current_load"`
	Healthy       bool           `json:"healthy"`
	Metadata      map[string]any `json:"metadata,omitempty"`
}

// NewServiceDescriptor creates a new service descriptor.
func NewServiceDescriptor(name, serviceType string) *ServiceDescriptor {
	return &ServiceDescriptor{
		Name:          name,
		ServiceType:   serviceType,
		Version:       "1.0.0",
		Capabilities:  []string{},
		MaxConcurrent: 10,
		CurrentLoad:   0,
		Healthy:       true,
	}
}

// CanAccept checks if the service can accept more load.
func (s *ServiceDescriptor) CanAccept() bool {
	return s.Healthy && s.CurrentLoad < s.MaxConcurrent
}

// =============================================================================
// Kernel Events
// =============================================================================

// KernelEventType represents types of kernel events.
type KernelEventType string

const (
	KernelEventProcessCreated      KernelEventType = "process.created"
	KernelEventProcessStateChanged KernelEventType = "process.state_changed"
	KernelEventInterruptRaised     KernelEventType = "interrupt.raised"
	KernelEventResourceExhausted   KernelEventType = "resource.exhausted"
	KernelEventServiceRegistered   KernelEventType = "service.registered"
	KernelEventServiceUnregistered KernelEventType = "service.unregistered"
)

// KernelEvent represents an event emitted by the kernel.
// These are OS-level events, not application events.
type KernelEvent struct {
	EventType KernelEventType `json:"event_type"`
	Timestamp time.Time       `json:"timestamp"`
	RequestID string          `json:"request_id"`
	UserID    string          `json:"user_id"`
	SessionID string          `json:"session_id"`
	PID       string          `json:"pid,omitempty"`
	Data      map[string]any  `json:"data,omitempty"`
}

// NewKernelEvent creates a new kernel event.
func NewKernelEvent(eventType KernelEventType, pid, requestID, userID, sessionID string) *KernelEvent {
	return &KernelEvent{
		EventType: eventType,
		Timestamp: time.Now().UTC(),
		RequestID: requestID,
		UserID:    userID,
		SessionID: sessionID,
		PID:       pid,
	}
}

// ProcessCreatedEvent creates a process.created event.
func ProcessCreatedEvent(pcb *ProcessControlBlock) *KernelEvent {
	evt := NewKernelEvent(KernelEventProcessCreated, pcb.PID, pcb.RequestID, pcb.UserID, pcb.SessionID)
	evt.Data = map[string]any{
		"priority": string(pcb.Priority),
	}
	return evt
}

// ProcessStateChangedEvent creates a process.state_changed event.
func ProcessStateChangedEvent(pcb *ProcessControlBlock, oldState ProcessState) *KernelEvent {
	evt := NewKernelEvent(KernelEventProcessStateChanged, pcb.PID, pcb.RequestID, pcb.UserID, pcb.SessionID)
	evt.Data = map[string]any{
		"old_state": string(oldState),
		"new_state": string(pcb.State),
	}
	return evt
}

// InterruptRaisedEvent creates an interrupt.raised event.
func InterruptRaisedEvent(pcb *ProcessControlBlock, interruptKind envelope.InterruptKind, data map[string]any) *KernelEvent {
	evt := NewKernelEvent(KernelEventInterruptRaised, pcb.PID, pcb.RequestID, pcb.UserID, pcb.SessionID)
	evt.Data = map[string]any{
		"interrupt_type": string(interruptKind),
	}
	for k, v := range data {
		evt.Data[k] = v
	}
	return evt
}

// ResourceExhaustedEvent creates a resource.exhausted event.
func ResourceExhaustedEvent(pcb *ProcessControlBlock, resource string, usage, quota int) *KernelEvent {
	evt := NewKernelEvent(KernelEventResourceExhausted, pcb.PID, pcb.RequestID, pcb.UserID, pcb.SessionID)
	evt.Data = map[string]any{
		"resource": resource,
		"usage":    usage,
		"quota":    quota,
	}
	return evt
}
