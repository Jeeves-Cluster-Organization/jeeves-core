// Package kernel provides resource tracking - cgroups equivalent.
//
// Features:
//   - Quota allocation per process
//   - Usage tracking with atomic updates
//   - Limit enforcement with quota checking
//   - System-wide metrics aggregation
package kernel

import (
	"fmt"
	"sync"
	"time"
)

// =============================================================================
// Process Resources (internal)
// =============================================================================

// processResources tracks resources for a single process.
type processResources struct {
	PID           string
	Quota         *ResourceQuota
	Usage         *ResourceUsage
	AllocatedAt   time.Time
	LastUpdatedAt time.Time
}

// newProcessResources creates a new process resource tracker.
func newProcessResources(pid string, quota *ResourceQuota) *processResources {
	now := time.Now().UTC()
	return &processResources{
		PID:           pid,
		Quota:         quota,
		Usage:         &ResourceUsage{},
		AllocatedAt:   now,
		LastUpdatedAt: now,
	}
}

// =============================================================================
// System Usage Statistics
// =============================================================================

// SystemUsage represents system-wide resource usage statistics.
type SystemUsage struct {
	TotalProcesses   int `json:"total_processes"`
	ActiveProcesses  int `json:"active_processes"`
	SystemLLMCalls   int `json:"system_llm_calls"`
	SystemToolCalls  int `json:"system_tool_calls"`
	SystemTokensIn   int `json:"system_tokens_in"`
	SystemTokensOut  int `json:"system_tokens_out"`
}

// =============================================================================
// Resource Budget
// =============================================================================

// ResourceBudget represents remaining resource budget for a process.
type ResourceBudget struct {
	LLMCalls    int     `json:"llm_calls"`
	ToolCalls   int     `json:"tool_calls"`
	AgentHops   int     `json:"agent_hops"`
	Iterations  int     `json:"iterations"`
	TimeSeconds float64 `json:"time_seconds"`
}

// =============================================================================
// Resource Tracker
// =============================================================================

// ResourceTracker tracks resource usage per process - kernel cgroups equivalent.
// Thread-safe implementation for tracking resource usage and enforcing quotas.
//
// Usage:
//
//	tracker := NewResourceTracker(nil, nil)
//
//	// Allocate quota for a process
//	tracker.Allocate(pid, quota)
//
//	// Record usage
//	tracker.RecordUsage(pid, 1, 0, 0, 100, 50)
//
//	// Check if within quota
//	if reason := tracker.CheckQuota(pid); reason != "" {
//	    // Handle quota exceeded
//	}
type ResourceTracker struct {
	defaultQuota *ResourceQuota
	logger       Logger

	// Per-process resource tracking
	resources map[string]*processResources

	// System-wide counters
	systemLLMCalls   int
	systemToolCalls  int
	systemTokensIn   int
	systemTokensOut  int
	totalProcesses   int
	activeProcesses  int

	mu sync.RWMutex
}

// Logger interface for resource tracker.
type Logger interface {
	Debug(msg string, keysAndValues ...any)
	Info(msg string, keysAndValues ...any)
	Warn(msg string, keysAndValues ...any)
	Error(msg string, keysAndValues ...any)
}

// NewResourceTracker creates a new resource tracker.
func NewResourceTracker(defaultQuota *ResourceQuota, logger Logger) *ResourceTracker {
	if defaultQuota == nil {
		defaultQuota = DefaultQuota()
	}
	return &ResourceTracker{
		defaultQuota: defaultQuota,
		logger:       logger,
		resources:    make(map[string]*processResources),
	}
}

// Allocate allocates resources to a process.
// Returns true if allocation succeeded, false if process already exists.
func (rt *ResourceTracker) Allocate(pid string, quota *ResourceQuota) bool {
	rt.mu.Lock()
	defer rt.mu.Unlock()

	if _, exists := rt.resources[pid]; exists {
		if rt.logger != nil {
			rt.logger.Warn("duplicate_allocation", "pid", pid)
		}
		return false
	}

	if quota == nil {
		quota = rt.defaultQuota
	}

	rt.resources[pid] = newProcessResources(pid, quota)
	rt.totalProcesses++
	rt.activeProcesses++

	if rt.logger != nil {
		rt.logger.Debug("resources_allocated",
			"pid", pid,
			"max_llm_calls", quota.MaxLLMCalls,
			"max_tool_calls", quota.MaxToolCalls,
			"timeout_seconds", quota.TimeoutSeconds,
		)
	}

	return true
}

// Release releases resources from a process.
// Returns true if release succeeded, false if process not found.
func (rt *ResourceTracker) Release(pid string) bool {
	rt.mu.Lock()
	defer rt.mu.Unlock()

	if _, exists := rt.resources[pid]; !exists {
		return false
	}

	delete(rt.resources, pid)
	rt.activeProcesses--

	if rt.logger != nil {
		rt.logger.Debug("resources_released", "pid", pid)
	}

	return true
}

// RecordUsage records resource usage for a process.
// Creates the process with default quota if it doesn't exist.
func (rt *ResourceTracker) RecordUsage(
	pid string,
	llmCalls, toolCalls, agentHops, tokensIn, tokensOut int,
) *ResourceUsage {
	rt.mu.Lock()
	defer rt.mu.Unlock()

	pr, exists := rt.resources[pid]
	if !exists {
		// Create with default quota if not exists
		pr = newProcessResources(pid, rt.defaultQuota)
		rt.resources[pid] = pr
		rt.totalProcesses++
		rt.activeProcesses++
	}

	// Update process usage
	pr.Usage.LLMCalls += llmCalls
	pr.Usage.ToolCalls += toolCalls
	pr.Usage.AgentHops += agentHops
	pr.Usage.TokensIn += tokensIn
	pr.Usage.TokensOut += tokensOut
	pr.LastUpdatedAt = time.Now().UTC()

	// Update elapsed time
	pr.Usage.ElapsedSeconds = pr.LastUpdatedAt.Sub(pr.AllocatedAt).Seconds()

	// Update system counters
	rt.systemLLMCalls += llmCalls
	rt.systemToolCalls += toolCalls
	rt.systemTokensIn += tokensIn
	rt.systemTokensOut += tokensOut

	// Log warnings if approaching limits
	if rt.logger != nil {
		quota := pr.Quota
		usage := pr.Usage

		// Warn at 80% of LLM call limit
		if quota.MaxLLMCalls > 0 && usage.LLMCalls >= int(float64(quota.MaxLLMCalls)*0.8) {
			rt.logger.Warn("approaching_llm_limit",
				"pid", pid,
				"usage", usage.LLMCalls,
				"quota", quota.MaxLLMCalls,
			)
		}

		// Warn at soft timeout
		if quota.SoftTimeoutSeconds > 0 && usage.ElapsedSeconds >= float64(quota.SoftTimeoutSeconds) {
			rt.logger.Warn("approaching_timeout",
				"pid", pid,
				"elapsed", usage.ElapsedSeconds,
				"soft_timeout", quota.SoftTimeoutSeconds,
				"hard_timeout", quota.TimeoutSeconds,
			)
		}
	}

	return pr.Usage.Clone()
}

// RecordLLMCall is a convenience method to record a single LLM call.
func (rt *ResourceTracker) RecordLLMCall(pid string, tokensIn, tokensOut int) *ResourceUsage {
	return rt.RecordUsage(pid, 1, 0, 0, tokensIn, tokensOut)
}

// RecordToolCall is a convenience method to record a single tool call.
func (rt *ResourceTracker) RecordToolCall(pid string) *ResourceUsage {
	return rt.RecordUsage(pid, 0, 1, 0, 0, 0)
}

// RecordAgentHop is a convenience method to record a single agent hop.
func (rt *ResourceTracker) RecordAgentHop(pid string) *ResourceUsage {
	return rt.RecordUsage(pid, 0, 0, 1, 0, 0)
}

// RecordInferenceCall records an inference call (embeddings, NLI, classification).
func (rt *ResourceTracker) RecordInferenceCall(pid string, inputChars int) *ResourceUsage {
	rt.mu.Lock()
	defer rt.mu.Unlock()

	pr, exists := rt.resources[pid]
	if !exists {
		// Auto-create with default quota if not tracked
		pr = newProcessResources(pid, rt.defaultQuota)
		rt.resources[pid] = pr
		rt.totalProcesses++
		rt.activeProcesses++
	}

	pr.Usage.InferenceRequests++
	pr.Usage.InferenceInputChars += inputChars
	pr.LastUpdatedAt = time.Now().UTC()

	if rt.logger != nil {
		rt.logger.Debug("inference_call_recorded",
			"pid", pid,
			"inference_requests", pr.Usage.InferenceRequests,
			"inference_input_chars", pr.Usage.InferenceInputChars,
		)
	}

	return pr.Usage.Clone()
}

// CheckQuota checks if process is within quota.
// Returns empty string if within quota, or reason string if exceeded.
func (rt *ResourceTracker) CheckQuota(pid string) string {
	rt.mu.RLock()
	defer rt.mu.RUnlock()

	pr, exists := rt.resources[pid]
	if !exists {
		return "" // No tracking = no limits
	}

	return pr.Usage.ExceedsQuota(pr.Quota)
}

// CheckInferenceQuota checks if a proposed inference operation would exceed quota.
// Returns empty string if within quota, or reason string if would exceed.
func (rt *ResourceTracker) CheckInferenceQuota(pid string, requests, inputChars int) string {
	rt.mu.RLock()
	defer rt.mu.RUnlock()

	pr, exists := rt.resources[pid]
	if !exists {
		return "" // No tracking = no limits (will use defaults on first record)
	}

	// Check request limit
	if pr.Usage.InferenceRequests+requests > pr.Quota.MaxInferenceRequests {
		return "max_inference_requests_exceeded"
	}

	// Check input chars limit
	if pr.Usage.InferenceInputChars+inputChars > pr.Quota.MaxInferenceInputChars {
		return "max_inference_input_chars_exceeded"
	}

	return ""
}

// GetUsage returns current usage for a process.
// Returns nil if process not found.
func (rt *ResourceTracker) GetUsage(pid string) *ResourceUsage {
	rt.mu.RLock()
	defer rt.mu.RUnlock()

	pr, exists := rt.resources[pid]
	if !exists {
		return nil
	}
	return pr.Usage.Clone()
}

// GetQuota returns quota for a process.
// Returns nil if process not found.
func (rt *ResourceTracker) GetQuota(pid string) *ResourceQuota {
	rt.mu.RLock()
	defer rt.mu.RUnlock()

	pr, exists := rt.resources[pid]
	if !exists {
		return nil
	}
	// Return a copy to prevent external modification
	q := *pr.Quota
	return &q
}

// GetSystemUsage returns system-wide resource usage.
func (rt *ResourceTracker) GetSystemUsage() *SystemUsage {
	rt.mu.RLock()
	defer rt.mu.RUnlock()

	return &SystemUsage{
		TotalProcesses:  rt.totalProcesses,
		ActiveProcesses: rt.activeProcesses,
		SystemLLMCalls:  rt.systemLLMCalls,
		SystemToolCalls: rt.systemToolCalls,
		SystemTokensIn:  rt.systemTokensIn,
		SystemTokensOut: rt.systemTokensOut,
	}
}

// UpdateElapsedTime updates and returns elapsed time for a process.
// Returns -1 if process not found.
func (rt *ResourceTracker) UpdateElapsedTime(pid string) float64 {
	rt.mu.Lock()
	defer rt.mu.Unlock()

	pr, exists := rt.resources[pid]
	if !exists {
		return -1
	}

	pr.LastUpdatedAt = time.Now().UTC()
	elapsed := pr.LastUpdatedAt.Sub(pr.AllocatedAt).Seconds()
	pr.Usage.ElapsedSeconds = elapsed

	return elapsed
}

// GetRemainingBudget returns remaining resource budget for a process.
// Returns nil if process not found.
func (rt *ResourceTracker) GetRemainingBudget(pid string) *ResourceBudget {
	rt.mu.RLock()
	defer rt.mu.RUnlock()

	pr, exists := rt.resources[pid]
	if !exists {
		return nil
	}

	quota := pr.Quota
	usage := pr.Usage

	return &ResourceBudget{
		LLMCalls:    max(0, quota.MaxLLMCalls-usage.LLMCalls),
		ToolCalls:   max(0, quota.MaxToolCalls-usage.ToolCalls),
		AgentHops:   max(0, quota.MaxAgentHops-usage.AgentHops),
		Iterations:  max(0, quota.MaxIterations-usage.Iterations),
		TimeSeconds: max(0, float64(quota.TimeoutSeconds)-usage.ElapsedSeconds),
	}
}

// AdjustQuota adjusts quota for a process.
// Returns an error if process not found.
func (rt *ResourceTracker) AdjustQuota(pid string, adjustments map[string]int) error {
	rt.mu.Lock()
	defer rt.mu.Unlock()

	pr, exists := rt.resources[pid]
	if !exists {
		return fmt.Errorf("unknown pid: %s", pid)
	}

	for key, value := range adjustments {
		switch key {
		case "max_llm_calls":
			pr.Quota.MaxLLMCalls = value
		case "max_tool_calls":
			pr.Quota.MaxToolCalls = value
		case "max_agent_hops":
			pr.Quota.MaxAgentHops = value
		case "max_iterations":
			pr.Quota.MaxIterations = value
		case "timeout_seconds":
			pr.Quota.TimeoutSeconds = value
		case "soft_timeout_seconds":
			pr.Quota.SoftTimeoutSeconds = value
		case "max_input_tokens":
			pr.Quota.MaxInputTokens = value
		case "max_output_tokens":
			pr.Quota.MaxOutputTokens = value
		case "max_context_tokens":
			pr.Quota.MaxContextTokens = value
		}
	}

	if rt.logger != nil {
		rt.logger.Info("quota_adjusted",
			"pid", pid,
			"adjustments", adjustments,
		)
	}

	return nil
}

// GetAllUsage returns usage for all tracked processes.
func (rt *ResourceTracker) GetAllUsage() map[string]*ResourceUsage {
	rt.mu.RLock()
	defer rt.mu.RUnlock()

	result := make(map[string]*ResourceUsage, len(rt.resources))
	for pid, pr := range rt.resources {
		result[pid] = pr.Usage.Clone()
	}
	return result
}

// IsTracked checks if a process is being tracked.
func (rt *ResourceTracker) IsTracked(pid string) bool {
	rt.mu.RLock()
	defer rt.mu.RUnlock()
	_, exists := rt.resources[pid]
	return exists
}

// GetProcessCount returns the number of processes being tracked.
func (rt *ResourceTracker) GetProcessCount() int {
	rt.mu.RLock()
	defer rt.mu.RUnlock()
	return len(rt.resources)
}
