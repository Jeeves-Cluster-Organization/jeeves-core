// Package kernel provides the Jeeves kernel - unified orchestration interface.
//
// The Kernel composes:
//   - LifecycleManager (process scheduler)
//   - ResourceTracker (cgroups)
//   - RateLimiter (sliding window rate limiting)
//   - InterruptService (unified interrupt handling)
//   - ServiceRegistry (IPC)
//
// This is the main entry point for the Go kernel layer.
package kernel

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// =============================================================================
// Kernel Configuration
// =============================================================================

// KernelConfig configures the kernel.
type KernelConfig struct {
	// Default resource quota for new processes
	DefaultQuota *ResourceQuota `json:"default_quota"`
	// Default rate limit configuration
	DefaultRateLimit *RateLimitConfig `json:"default_rate_limit"`
	// Default service for dispatch
	DefaultService string `json:"default_service"`
	// Enable telemetry/metrics
	EnableTelemetry bool `json:"enable_telemetry"`
}

// DefaultKernelConfig returns default kernel configuration.
func DefaultKernelConfig() *KernelConfig {
	return &KernelConfig{
		DefaultQuota:     DefaultQuota(),
		DefaultRateLimit: DefaultRateLimitConfig(),
		DefaultService:   "flow_service",
		EnableTelemetry:  true,
	}
}

// =============================================================================
// Kernel
// =============================================================================

// Kernel is the Jeeves kernel - central coordinator for process lifecycle,
// resource management, and service dispatch.
//
// OS Analogy:
//
//	The Kernel is like a microkernel. It doesn't execute the actual work
//	(that's done by services), but it manages the lifecycle, resources,
//	and communication.
//
// Usage:
//
//	kernel := NewKernel(logger, nil)
//
//	// Register services
//	kernel.Services().RegisterService(NewServiceInfo("flow_service", "flow"))
//	kernel.Services().RegisterHandler("flow_service", flowHandler)
//
//	// Submit a process
//	pcb, err := kernel.Submit(pid, requestID, userID, sessionID, PriorityNormal, nil)
//
//	// Record resource usage
//	kernel.RecordLLMCall(pid, tokensIn, tokensOut)
//
//	// Check quota
//	if exceeded := kernel.CheckQuota(pid); exceeded != "" {
//	    // Handle quota exceeded
//	}
type Kernel struct {
	config *KernelConfig
	logger Logger

	// Subsystems
	lifecycle    *LifecycleManager
	resources    *ResourceTracker
	rateLimiter  *RateLimiter
	interrupts   *InterruptService
	services     *ServiceRegistry
	orchestrator *Orchestrator

	// Event listeners
	eventHandlers []KernelEventHandler
	eventMu       sync.RWMutex

	// Kernel state
	startedAt time.Time
	mu        sync.RWMutex
}

// KernelEventHandler handles kernel events.
type KernelEventHandler func(*KernelEvent)

// NewKernel creates a new kernel with the given configuration.
func NewKernel(logger Logger, config *KernelConfig) *Kernel {
	if config == nil {
		config = DefaultKernelConfig()
	}

	k := &Kernel{
		config:        config,
		logger:        logger,
		lifecycle:     NewLifecycleManager(config.DefaultQuota),
		resources:     NewResourceTracker(config.DefaultQuota, logger),
		rateLimiter:   NewRateLimiter(config.DefaultRateLimit),
		interrupts:    NewInterruptService(logger, nil),
		services:      NewServiceRegistry(logger),
		eventHandlers: []KernelEventHandler{},
		startedAt:     time.Now().UTC(),
	}

	// Initialize orchestrator after kernel is created (needs reference to kernel)
	k.orchestrator = NewOrchestrator(k, logger)

	if logger != nil {
		logger.Info("kernel_initialized",
			"default_service", config.DefaultService,
			"max_llm_calls", config.DefaultQuota.MaxLLMCalls,
			"max_iterations", config.DefaultQuota.MaxIterations,
		)
	}

	return k
}

// =============================================================================
// Subsystem Access
// =============================================================================

// Lifecycle returns the lifecycle manager.
func (k *Kernel) Lifecycle() *LifecycleManager {
	return k.lifecycle
}

// Resources returns the resource tracker.
func (k *Kernel) Resources() *ResourceTracker {
	return k.resources
}

// RateLimiter returns the rate limiter.
func (k *Kernel) RateLimiter() *RateLimiter {
	return k.rateLimiter
}

// Interrupts returns the interrupt service.
func (k *Kernel) Interrupts() *InterruptService {
	return k.interrupts
}

// Services returns the service registry.
func (k *Kernel) Services() *ServiceRegistry {
	return k.services
}

// Orchestrator returns the orchestrator for kernel-driven pipeline execution.
func (k *Kernel) Orchestrator() *Orchestrator {
	return k.orchestrator
}

// =============================================================================
// Process Lifecycle
// =============================================================================

// Submit creates a new process with the given parameters.
// Returns the PCB in NEW state.
func (k *Kernel) Submit(
	pid, requestID, userID, sessionID string,
	priority SchedulingPriority,
	quota *ResourceQuota,
) (*ProcessControlBlock, error) {
	if quota == nil {
		quota = k.config.DefaultQuota
	}

	// Create PCB
	pcb, err := k.lifecycle.Submit(pid, requestID, userID, sessionID, priority, quota)
	if err != nil {
		return nil, err
	}

	// Allocate resources
	k.resources.Allocate(pid, quota)

	// Emit event
	k.emitEvent(ProcessCreatedEvent(pcb))

	if k.logger != nil {
		k.logger.Info("process_submitted",
			"pid", pid,
			"request_id", requestID,
			"user_id", userID,
			"priority", string(priority),
		)
	}

	return pcb, nil
}

// Schedule schedules a process for execution (NEW -> READY).
func (k *Kernel) Schedule(pid string) error {
	pcb := k.lifecycle.GetProcess(pid)
	if pcb == nil {
		return fmt.Errorf("unknown pid: %s", pid)
	}

	oldState := pcb.State
	err := k.lifecycle.Schedule(pid)
	if err != nil {
		return err
	}

	// Emit state change event
	k.emitEvent(ProcessStateChangedEvent(pcb, oldState))

	return nil
}

// GetNextRunnable returns the next process to run (transitions READY -> RUNNING).
func (k *Kernel) GetNextRunnable() *ProcessControlBlock {
	return k.lifecycle.GetNextRunnable()
}

// TransitionState transitions a process to a new state.
func (k *Kernel) TransitionState(pid string, newState ProcessState, reason string) error {
	pcb := k.lifecycle.GetProcess(pid)
	if pcb == nil {
		return fmt.Errorf("unknown pid: %s", pid)
	}

	oldState := pcb.State
	err := k.lifecycle.TransitionState(pid, newState, reason)
	if err != nil {
		return err
	}

	// Emit state change event
	k.emitEvent(ProcessStateChangedEvent(pcb, oldState))

	return nil
}

// Terminate terminates a process.
func (k *Kernel) Terminate(pid, reason string, force bool) error {
	pcb := k.lifecycle.GetProcess(pid)
	if pcb == nil {
		return fmt.Errorf("unknown pid: %s", pid)
	}

	oldState := pcb.State
	err := k.lifecycle.Terminate(pid, reason, force)
	if err != nil {
		return err
	}

	// Release resources
	k.resources.Release(pid)

	// Emit state change event
	k.emitEvent(ProcessStateChangedEvent(pcb, oldState))

	if k.logger != nil {
		k.logger.Info("process_terminated",
			"pid", pid,
			"reason", reason,
			"force", force,
		)
	}

	return nil
}

// GetProcess returns a process by ID.
func (k *Kernel) GetProcess(pid string) *ProcessControlBlock {
	return k.lifecycle.GetProcess(pid)
}

// ListProcesses returns processes matching criteria.
func (k *Kernel) ListProcesses(state *ProcessState, userID string) []*ProcessControlBlock {
	return k.lifecycle.ListProcesses(state, userID)
}

// =============================================================================
// Resource Management
// =============================================================================

// RecordLLMCall records an LLM call and checks quota.
// Returns quota exceeded reason if any, empty string otherwise.
func (k *Kernel) RecordLLMCall(pid string, tokensIn, tokensOut int) string {
	k.resources.RecordLLMCall(pid, tokensIn, tokensOut)
	exceeded := k.resources.CheckQuota(pid)

	if exceeded != "" {
		pcb := k.lifecycle.GetProcess(pid)
		if pcb != nil {
			usage := k.resources.GetUsage(pid)
			quota := k.resources.GetQuota(pid)
			k.emitEvent(ResourceExhaustedEvent(pcb, exceeded, usage.LLMCalls, quota.MaxLLMCalls))
		}
	}

	return exceeded
}

// RecordToolCall records a tool call and checks quota.
func (k *Kernel) RecordToolCall(pid string) string {
	k.resources.RecordToolCall(pid)
	return k.resources.CheckQuota(pid)
}

// RecordAgentHop records an agent hop and checks quota.
func (k *Kernel) RecordAgentHop(pid string) string {
	k.resources.RecordAgentHop(pid)
	return k.resources.CheckQuota(pid)
}

// CheckQuota checks if a process has exceeded its quota.
func (k *Kernel) CheckQuota(pid string) string {
	return k.resources.CheckQuota(pid)
}

// GetUsage returns resource usage for a process.
func (k *Kernel) GetUsage(pid string) *ResourceUsage {
	return k.resources.GetUsage(pid)
}

// GetRemainingBudget returns remaining resource budget for a process.
func (k *Kernel) GetRemainingBudget(pid string) *ResourceBudget {
	return k.resources.GetRemainingBudget(pid)
}

// =============================================================================
// Rate Limiting
// =============================================================================

// CheckRateLimit checks if a request is within rate limits.
func (k *Kernel) CheckRateLimit(userID, endpoint string, record bool) *RateLimitResult {
	return k.rateLimiter.CheckRateLimit(userID, endpoint, record)
}

// GetRateLimitUsage returns rate limit usage for a user/endpoint.
func (k *Kernel) GetRateLimitUsage(userID, endpoint string) map[string]map[string]any {
	return k.rateLimiter.GetUsage(userID, endpoint)
}

// =============================================================================
// Interrupt Management
// =============================================================================

// CreateInterrupt creates a new interrupt.
func (k *Kernel) CreateInterrupt(
	kind envelope.InterruptKind,
	requestID, userID, sessionID, envelopeID string,
	opts ...InterruptOption,
) *KernelInterrupt {
	interrupt := k.interrupts.CreateInterrupt(kind, requestID, userID, sessionID, envelopeID, opts...)

	// Emit event
	pcb := k.lifecycle.GetProcess(envelopeID)
	if pcb != nil {
		data := map[string]any{
			"interrupt_id": interrupt.ID,
		}
		if interrupt.Question != "" {
			data["question"] = interrupt.Question
		}
		if interrupt.Message != "" {
			data["message"] = interrupt.Message
		}
		k.emitEvent(InterruptRaisedEvent(pcb, interrupt.Kind, data))
	}

	return interrupt
}

// ResolveInterrupt resolves an interrupt.
func (k *Kernel) ResolveInterrupt(
	interruptID string,
	response *envelope.InterruptResponse,
	userID string,
) *KernelInterrupt {
	return k.interrupts.Resolve(interruptID, response, userID)
}

// GetPendingInterrupt gets the pending interrupt for a request.
func (k *Kernel) GetPendingInterrupt(requestID string) *KernelInterrupt {
	return k.interrupts.GetPendingForRequest(requestID)
}

// =============================================================================
// Service Dispatch
// =============================================================================

// RegisterService registers a service with the kernel.
func (k *Kernel) RegisterService(info *ServiceInfo) bool {
	result := k.services.RegisterService(info)
	if result {
		k.emitEvent(&KernelEvent{
			EventType: KernelEventServiceRegistered,
			Timestamp: time.Now().UTC(),
			Data: map[string]any{
				"service_name": info.Name,
				"service_type": info.ServiceType,
			},
		})
	}
	return result
}

// UnregisterService unregisters a service.
func (k *Kernel) UnregisterService(serviceName string) bool {
	result := k.services.UnregisterService(serviceName)
	if result {
		k.emitEvent(&KernelEvent{
			EventType: KernelEventServiceUnregistered,
			Timestamp: time.Now().UTC(),
			Data: map[string]any{
				"service_name": serviceName,
			},
		})
	}
	return result
}

// RegisterHandler registers a handler for a service.
func (k *Kernel) RegisterHandler(serviceName string, handler ServiceHandler) {
	k.services.RegisterHandler(serviceName, handler)
}

// Dispatch dispatches a request to a service.
func (k *Kernel) Dispatch(ctx context.Context, target *DispatchTarget, data map[string]any) *DispatchResult {
	return k.services.Dispatch(ctx, target, data)
}

// DispatchInference dispatches an inference request with quota checking.
// This is the kernel-mediated path for embeddings, NLI, and other ML inference.
func (k *Kernel) DispatchInference(ctx context.Context, pid string, target *DispatchTarget, data map[string]any) *DispatchResult {
	startTime := time.Now()

	// Check context cancellation first
	if ctx.Err() != nil {
		return &DispatchResult{
			Success:  false,
			Error:    ctx.Err().Error(),
			Duration: time.Since(startTime),
		}
	}

	// Calculate input chars from payload
	inputChars := extractInputChars(data)

	// Check quota BEFORE dispatch
	if exceeded := k.CheckInferenceQuota(pid, 1, inputChars); exceeded != "" {
		if k.logger != nil {
			k.logger.Warn("inference_quota_exceeded",
				"pid", pid,
				"reason", exceeded,
				"input_chars", inputChars,
			)
		}
		return &DispatchResult{
			Success:  false,
			Error:    fmt.Sprintf("inference quota exceeded: %s", exceeded),
			Duration: time.Since(startTime),
		}
	}

	// Dispatch to service
	result := k.services.Dispatch(ctx, target, data)

	// Record usage AFTER dispatch (even if failed, to track attempts)
	k.RecordInferenceCall(pid, inputChars)

	return result
}

// extractInputChars extracts the total character count from inference payload.
func extractInputChars(data map[string]any) int {
	totalChars := 0

	// Check for single text
	if text, ok := data["text"].(string); ok {
		totalChars += len(text)
	}

	// Check for batch texts
	if texts, ok := data["texts"].([]string); ok {
		for _, text := range texts {
			totalChars += len(text)
		}
	}

	// Also handle []interface{} case (from JSON unmarshaling)
	if texts, ok := data["texts"].([]interface{}); ok {
		for _, t := range texts {
			if text, ok := t.(string); ok {
				totalChars += len(text)
			}
		}
	}

	return totalChars
}

// CheckInferenceQuota checks if an inference operation would exceed quota.
// Returns the exceeded reason or empty string if within quota.
func (k *Kernel) CheckInferenceQuota(pid string, requests, inputChars int) string {
	return k.resources.CheckInferenceQuota(pid, requests, inputChars)
}

// RecordInferenceCall records an inference call for resource tracking.
func (k *Kernel) RecordInferenceCall(pid string, inputChars int) {
	k.resources.RecordInferenceCall(pid, inputChars)
}

// =============================================================================
// Event System
// =============================================================================

// OnEvent registers an event handler.
func (k *Kernel) OnEvent(handler KernelEventHandler) {
	k.eventMu.Lock()
	defer k.eventMu.Unlock()
	k.eventHandlers = append(k.eventHandlers, handler)
}

// emitEvent emits an event to all handlers.
func (k *Kernel) emitEvent(event *KernelEvent) {
	k.eventMu.RLock()
	handlers := make([]KernelEventHandler, len(k.eventHandlers))
	copy(handlers, k.eventHandlers)
	k.eventMu.RUnlock()

	for _, handler := range handlers {
		handler(event)
	}
}

// =============================================================================
// System Status
// =============================================================================

// GetSystemStatus returns overall system status.
func (k *Kernel) GetSystemStatus() map[string]any {
	processCounts := k.lifecycle.GetProcessCount()
	resourceUsage := k.resources.GetSystemUsage()
	interruptStats := k.interrupts.GetStats()
	serviceStats := k.services.GetStats()

	return map[string]any{
		"processes": map[string]any{
			"total":       k.lifecycle.GetTotalProcesses(),
			"queue_depth": k.lifecycle.GetQueueDepth(),
			"by_state":    processCounts,
		},
		"resources": resourceUsage,
		"interrupts": interruptStats,
		"services":   serviceStats,
		"uptime_seconds": time.Since(k.startedAt).Seconds(),
	}
}

// GetRequestStatus returns status of a specific request/process.
func (k *Kernel) GetRequestStatus(pid string) map[string]any {
	pcb := k.lifecycle.GetProcess(pid)
	if pcb == nil {
		return nil
	}

	usage := k.resources.GetUsage(pid)
	remaining := k.resources.GetRemainingBudget(pid)
	interrupt := k.interrupts.GetPendingForRequest(pcb.RequestID)

	status := map[string]any{
		"pid":           pid,
		"state":         string(pcb.State),
		"priority":      string(pcb.Priority),
		"current_stage": pcb.CurrentStage,
		"created_at":    pcb.CreatedAt.Format(time.RFC3339),
	}

	if pcb.StartedAt != nil {
		status["started_at"] = pcb.StartedAt.Format(time.RFC3339)
	}

	if usage != nil {
		status["usage"] = map[string]any{
			"llm_calls":       usage.LLMCalls,
			"tool_calls":      usage.ToolCalls,
			"agent_hops":      usage.AgentHops,
			"elapsed_seconds": usage.ElapsedSeconds,
		}
	}

	if remaining != nil {
		status["remaining"] = remaining
	}

	status["has_interrupt"] = interrupt != nil
	if interrupt != nil {
		status["interrupt_kind"] = string(interrupt.Kind)
		status["interrupt_id"] = interrupt.ID
	}

	return status
}

// =============================================================================
// Cleanup
// =============================================================================

// Cleanup performs periodic cleanup tasks.
// Should be called periodically (e.g., every minute).
func (k *Kernel) Cleanup() {
	// Expire pending interrupts
	k.interrupts.ExpirePending()

	// Cleanup old resolved interrupts (older than 24 hours)
	k.interrupts.CleanupResolved(24 * time.Hour)

	// Cleanup expired rate limit windows
	k.rateLimiter.CleanupExpired()

	if k.logger != nil {
		k.logger.Debug("kernel_cleanup_completed")
	}
}

// ShutdownError aggregates multiple errors that occurred during shutdown.
type ShutdownError struct {
	Errors []error
}

// Error returns a string representation of the shutdown errors.
func (e *ShutdownError) Error() string {
	if len(e.Errors) == 0 {
		return "shutdown completed with no errors"
	}
	if len(e.Errors) == 1 {
		return fmt.Sprintf("shutdown error: %v", e.Errors[0])
	}
	return fmt.Sprintf("shutdown completed with %d errors", len(e.Errors))
}

// Unwrap returns the first error for compatibility with errors.Is/As.
func (e *ShutdownError) Unwrap() error {
	if len(e.Errors) > 0 {
		return e.Errors[0]
	}
	return nil
}

// Shutdown gracefully shuts down the kernel.
// Returns a ShutdownError if any processes failed to terminate.
func (k *Kernel) Shutdown(ctx context.Context) error {
	if k.logger != nil {
		k.logger.Info("kernel_shutdown_initiated")
	}

	var errs []error

	// Terminate all running processes
	for _, pcb := range k.lifecycle.ListProcesses(nil, "") {
		// Check context cancellation
		select {
		case <-ctx.Done():
			errs = append(errs, fmt.Errorf("shutdown cancelled: %w", ctx.Err()))
			if k.logger != nil {
				k.logger.Warn("shutdown_cancelled", "error", ctx.Err().Error())
			}
			if len(errs) > 0 {
				return &ShutdownError{Errors: errs}
			}
			return ctx.Err()
		default:
		}

		if !pcb.IsTerminated() {
			if err := k.Terminate(pcb.PID, "kernel_shutdown", true); err != nil {
				errs = append(errs, fmt.Errorf("failed to terminate %s: %w", pcb.PID, err))
				if k.logger != nil {
					k.logger.Warn("shutdown_terminate_failed",
						"pid", pcb.PID,
						"error", err.Error(),
					)
				}
			}
		}
	}

	if k.logger != nil {
		k.logger.Info("kernel_shutdown_completed", "errors", len(errs))
	}

	if len(errs) > 0 {
		return &ShutdownError{Errors: errs}
	}
	return nil
}
