// Package kernel provides interrupt handling - unified interrupt service.
//
// Features:
//   - Create and resolve interrupts
//   - Pending interrupt queue per process
//   - Interrupt lifecycle management
//   - Config-driven behavior per interrupt kind
package kernel

import (
	"fmt"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// =============================================================================
// Interrupt Status
// =============================================================================

// InterruptStatus represents the status of an interrupt.
type InterruptStatus string

const (
	// InterruptStatusPending indicates the interrupt is awaiting response.
	InterruptStatusPending InterruptStatus = "pending"
	// InterruptStatusResolved indicates the interrupt has been resolved.
	InterruptStatusResolved InterruptStatus = "resolved"
	// InterruptStatusExpired indicates the interrupt has expired.
	InterruptStatusExpired InterruptStatus = "expired"
	// InterruptStatusCancelled indicates the interrupt was cancelled.
	InterruptStatusCancelled InterruptStatus = "cancelled"
)

// =============================================================================
// Interrupt Config
// =============================================================================

// InterruptConfig configures behavior for a specific interrupt kind.
type InterruptConfig struct {
	// DefaultTTL is the default time-to-live for interrupts of this kind.
	// Zero means no expiry.
	DefaultTTL time.Duration `json:"default_ttl"`
	// AutoExpire determines if pending interrupts should auto-expire.
	AutoExpire bool `json:"auto_expire"`
	// RequireResponse determines if a response is required to resolve.
	RequireResponse bool `json:"require_response"`
}

// DefaultInterruptConfigs provides default configurations per interrupt kind.
var DefaultInterruptConfigs = map[envelope.InterruptKind]*InterruptConfig{
	envelope.InterruptKindClarification: {
		DefaultTTL:      24 * time.Hour,
		AutoExpire:      true,
		RequireResponse: true,
	},
	envelope.InterruptKindConfirmation: {
		DefaultTTL:      1 * time.Hour,
		AutoExpire:      true,
		RequireResponse: true,
	},
	envelope.InterruptKindAgentReview: {
		DefaultTTL:      30 * time.Minute,
		AutoExpire:      true,
		RequireResponse: true,
	},
	envelope.InterruptKindCheckpoint: {
		DefaultTTL:      0, // No expiry
		AutoExpire:      false,
		RequireResponse: false,
	},
	envelope.InterruptKindResourceExhausted: {
		DefaultTTL:      5 * time.Minute,
		AutoExpire:      true,
		RequireResponse: false,
	},
	envelope.InterruptKindTimeout: {
		DefaultTTL:      5 * time.Minute,
		AutoExpire:      true,
		RequireResponse: false,
	},
	envelope.InterruptKindSystemError: {
		DefaultTTL:      1 * time.Hour,
		AutoExpire:      true,
		RequireResponse: false,
	},
}

// =============================================================================
// Kernel Interrupt (extended FlowInterrupt)
// =============================================================================

// KernelInterrupt extends FlowInterrupt with kernel-level tracking.
type KernelInterrupt struct {
	*envelope.FlowInterrupt

	// Kernel tracking
	Status     InterruptStatus `json:"status"`
	RequestID  string          `json:"request_id"`
	UserID     string          `json:"user_id"`
	SessionID  string          `json:"session_id"`
	EnvelopeID string          `json:"envelope_id"`
	ResolvedAt *time.Time      `json:"resolved_at,omitempty"`
	TraceID    string          `json:"trace_id,omitempty"`
	SpanID     string          `json:"span_id,omitempty"`
}

// NewKernelInterrupt creates a new kernel interrupt.
func NewKernelInterrupt(
	kind envelope.InterruptKind,
	requestID, userID, sessionID, envelopeID string,
	ttl time.Duration,
) *KernelInterrupt {
	now := time.Now().UTC()
	interrupt := &KernelInterrupt{
		FlowInterrupt: &envelope.FlowInterrupt{
			Kind:      kind,
			ID:        "int_" + uuid.New().String()[:16],
			CreatedAt: now,
		},
		Status:     InterruptStatusPending,
		RequestID:  requestID,
		UserID:     userID,
		SessionID:  sessionID,
		EnvelopeID: envelopeID,
	}

	if ttl > 0 {
		expiresAt := now.Add(ttl)
		interrupt.ExpiresAt = &expiresAt
	}

	return interrupt
}

// IsExpired checks if the interrupt has expired.
func (ki *KernelInterrupt) IsExpired() bool {
	if ki.ExpiresAt == nil {
		return false
	}
	return time.Now().UTC().After(*ki.ExpiresAt)
}

// IsPending checks if the interrupt is pending.
func (ki *KernelInterrupt) IsPending() bool {
	return ki.Status == InterruptStatusPending
}

// =============================================================================
// Interrupt Service
// =============================================================================

// InterruptService manages interrupt lifecycle.
// Thread-safe implementation for creating, querying, and resolving interrupts.
//
// Usage:
//
//	service := NewInterruptService(nil, nil)
//
//	// Create an interrupt
//	interrupt := service.CreateInterrupt(
//	    envelope.InterruptKindClarification,
//	    requestID, userID, sessionID, envelopeID,
//	    WithQuestion("What file?"),
//	)
//
//	// Resolve the interrupt
//	resolved := service.Resolve(interrupt.ID, response, userID)
type InterruptService struct {
	logger  Logger
	configs map[envelope.InterruptKind]*InterruptConfig

	// In-memory store keyed by interrupt ID
	store map[string]*KernelInterrupt
	// Index by request ID for fast lookup
	byRequest map[string][]*KernelInterrupt
	// Index by session ID
	bySession map[string][]*KernelInterrupt

	mu sync.RWMutex
}

// NewInterruptService creates a new interrupt service.
func NewInterruptService(logger Logger, configs map[envelope.InterruptKind]*InterruptConfig) *InterruptService {
	mergedConfigs := make(map[envelope.InterruptKind]*InterruptConfig)
	// Copy defaults
	for k, v := range DefaultInterruptConfigs {
		mergedConfigs[k] = v
	}
	// Override with custom configs
	for k, v := range configs {
		mergedConfigs[k] = v
	}

	return &InterruptService{
		logger:    logger,
		configs:   mergedConfigs,
		store:     make(map[string]*KernelInterrupt),
		byRequest: make(map[string][]*KernelInterrupt),
		bySession: make(map[string][]*KernelInterrupt),
	}
}

// GetConfig returns the configuration for an interrupt kind.
func (is *InterruptService) GetConfig(kind envelope.InterruptKind) *InterruptConfig {
	if cfg, ok := is.configs[kind]; ok {
		return cfg
	}
	// Return a sensible default
	return &InterruptConfig{
		DefaultTTL:      1 * time.Hour,
		AutoExpire:      true,
		RequireResponse: true,
	}
}

// =============================================================================
// Create Interrupts
// =============================================================================

// InterruptOption is a functional option for configuring interrupts.
type InterruptOption func(*KernelInterrupt)

// WithInterruptQuestion sets the question for a clarification interrupt.
func WithInterruptQuestion(q string) InterruptOption {
	return func(ki *KernelInterrupt) { ki.Question = q }
}

// WithInterruptMessage sets the message for a confirmation interrupt.
func WithInterruptMessage(m string) InterruptOption {
	return func(ki *KernelInterrupt) { ki.Message = m }
}

// WithInterruptData sets additional data for the interrupt.
func WithInterruptData(d map[string]any) InterruptOption {
	return func(ki *KernelInterrupt) { ki.Data = d }
}

// WithInterruptTTL overrides the default TTL.
func WithInterruptTTL(ttl time.Duration) InterruptOption {
	return func(ki *KernelInterrupt) {
		if ttl > 0 {
			expiresAt := time.Now().UTC().Add(ttl)
			ki.ExpiresAt = &expiresAt
		}
	}
}

// WithTraceContext sets trace context for the interrupt.
func WithTraceContext(traceID, spanID string) InterruptOption {
	return func(ki *KernelInterrupt) {
		ki.TraceID = traceID
		ki.SpanID = spanID
	}
}

// CreateInterrupt creates a new interrupt.
func (is *InterruptService) CreateInterrupt(
	kind envelope.InterruptKind,
	requestID, userID, sessionID, envelopeID string,
	opts ...InterruptOption,
) *KernelInterrupt {
	config := is.GetConfig(kind)
	interrupt := NewKernelInterrupt(kind, requestID, userID, sessionID, envelopeID, config.DefaultTTL)

	// Apply options
	for _, opt := range opts {
		opt(interrupt)
	}

	is.mu.Lock()
	defer is.mu.Unlock()

	// Store interrupt
	is.store[interrupt.ID] = interrupt

	// Index by request
	is.byRequest[requestID] = append(is.byRequest[requestID], interrupt)

	// Index by session
	is.bySession[sessionID] = append(is.bySession[sessionID], interrupt)

	if is.logger != nil {
		is.logger.Info("interrupt_created",
			"interrupt_id", interrupt.ID,
			"kind", string(kind),
			"request_id", requestID,
			"user_id", userID,
			"session_id", sessionID,
		)
	}

	return interrupt
}

// CreateClarification is a convenience method to create a clarification interrupt.
func (is *InterruptService) CreateClarification(
	requestID, userID, sessionID, envelopeID, question string,
	context map[string]any,
) *KernelInterrupt {
	opts := []InterruptOption{
		WithInterruptQuestion(question),
	}
	if context != nil {
		opts = append(opts, WithInterruptData(context))
	}
	return is.CreateInterrupt(
		envelope.InterruptKindClarification,
		requestID, userID, sessionID, envelopeID,
		opts...,
	)
}

// CreateConfirmation is a convenience method to create a confirmation interrupt.
func (is *InterruptService) CreateConfirmation(
	requestID, userID, sessionID, envelopeID, message string,
	actionData map[string]any,
) *KernelInterrupt {
	opts := []InterruptOption{
		WithInterruptMessage(message),
	}
	if actionData != nil {
		opts = append(opts, WithInterruptData(actionData))
	}
	return is.CreateInterrupt(
		envelope.InterruptKindConfirmation,
		requestID, userID, sessionID, envelopeID,
		opts...,
	)
}

// CreateResourceExhausted is a convenience method to create a resource exhausted interrupt.
func (is *InterruptService) CreateResourceExhausted(
	requestID, userID, sessionID, envelopeID string,
	resourceType string, retryAfterSeconds float64,
) *KernelInterrupt {
	return is.CreateInterrupt(
		envelope.InterruptKindResourceExhausted,
		requestID, userID, sessionID, envelopeID,
		WithInterruptMessage(fmt.Sprintf("Resource exhausted: %s", resourceType)),
		WithInterruptData(map[string]any{
			"resource_type":       resourceType,
			"retry_after_seconds": retryAfterSeconds,
		}),
	)
}

// =============================================================================
// Query Interrupts
// =============================================================================

// GetInterrupt gets an interrupt by ID.
func (is *InterruptService) GetInterrupt(interruptID string) *KernelInterrupt {
	is.mu.RLock()
	defer is.mu.RUnlock()
	return is.store[interruptID]
}

// GetPendingForRequest gets the most recent pending interrupt for a request.
func (is *InterruptService) GetPendingForRequest(requestID string) *KernelInterrupt {
	is.mu.RLock()
	defer is.mu.RUnlock()

	interrupts := is.byRequest[requestID]
	// Return most recent pending
	for i := len(interrupts) - 1; i >= 0; i-- {
		if interrupts[i].IsPending() && !interrupts[i].IsExpired() {
			return interrupts[i]
		}
	}
	return nil
}

// GetPendingForSession gets all pending interrupts for a session.
func (is *InterruptService) GetPendingForSession(sessionID string, kinds []envelope.InterruptKind) []*KernelInterrupt {
	is.mu.RLock()
	defer is.mu.RUnlock()

	kindSet := make(map[envelope.InterruptKind]bool)
	for _, k := range kinds {
		kindSet[k] = true
	}

	var result []*KernelInterrupt
	for _, interrupt := range is.bySession[sessionID] {
		if !interrupt.IsPending() || interrupt.IsExpired() {
			continue
		}
		if len(kinds) > 0 && !kindSet[interrupt.Kind] {
			continue
		}
		result = append(result, interrupt)
	}

	return result
}

// =============================================================================
// Resolve Interrupts
// =============================================================================

// Resolve resolves an interrupt with a response.
// Returns the resolved interrupt, or nil if not found/invalid.
func (is *InterruptService) Resolve(
	interruptID string,
	response *envelope.InterruptResponse,
	userID string,
) *KernelInterrupt {
	is.mu.Lock()
	defer is.mu.Unlock()

	interrupt, exists := is.store[interruptID]
	if !exists {
		if is.logger != nil {
			is.logger.Warn("interrupt_not_found", "interrupt_id", interruptID)
		}
		return nil
	}

	if interrupt.Status != InterruptStatusPending {
		if is.logger != nil {
			is.logger.Warn("interrupt_not_pending",
				"interrupt_id", interruptID,
				"status", string(interrupt.Status),
			)
		}
		return nil
	}

	// Validate user if provided
	if userID != "" && interrupt.UserID != userID {
		if is.logger != nil {
			is.logger.Warn("interrupt_user_mismatch",
				"interrupt_id", interruptID,
				"expected_user", interrupt.UserID,
				"actual_user", userID,
			)
		}
		return nil
	}

	// Update interrupt
	now := time.Now().UTC()
	if response != nil {
		response.ReceivedAt = now
		interrupt.Response = response
	}
	interrupt.Status = InterruptStatusResolved
	interrupt.ResolvedAt = &now

	if is.logger != nil {
		is.logger.Info("interrupt_resolved",
			"interrupt_id", interruptID,
			"kind", string(interrupt.Kind),
			"request_id", interrupt.RequestID,
		)
	}

	return interrupt
}

// Cancel cancels an interrupt.
func (is *InterruptService) Cancel(interruptID string, reason string) *KernelInterrupt {
	is.mu.Lock()
	defer is.mu.Unlock()

	interrupt, exists := is.store[interruptID]
	if !exists || interrupt.Status != InterruptStatusPending {
		return nil
	}

	interrupt.Status = InterruptStatusCancelled
	if interrupt.Data == nil {
		interrupt.Data = make(map[string]any)
	}
	interrupt.Data["cancel_reason"] = reason

	if is.logger != nil {
		is.logger.Info("interrupt_cancelled",
			"interrupt_id", interruptID,
			"reason", reason,
		)
	}

	return interrupt
}

// ExpirePending expires all pending interrupts that have passed their expiry time.
// Returns the number of interrupts expired.
func (is *InterruptService) ExpirePending() int {
	is.mu.Lock()
	defer is.mu.Unlock()

	count := 0
	for _, interrupt := range is.store {
		if interrupt.IsPending() && interrupt.IsExpired() {
			interrupt.Status = InterruptStatusExpired
			count++
		}
	}

	if is.logger != nil && count > 0 {
		is.logger.Info("interrupts_expired", "count", count)
	}

	return count
}

// =============================================================================
// Cleanup
// =============================================================================

// CleanupResolved removes resolved/expired/cancelled interrupts older than the given duration.
// Returns the number of interrupts cleaned up.
func (is *InterruptService) CleanupResolved(olderThan time.Duration) int {
	is.mu.Lock()
	defer is.mu.Unlock()

	cutoff := time.Now().UTC().Add(-olderThan)
	count := 0

	toDelete := make([]string, 0)
	for id, interrupt := range is.store {
		if interrupt.Status != InterruptStatusPending && interrupt.CreatedAt.Before(cutoff) {
			toDelete = append(toDelete, id)
		}
	}

	for _, id := range toDelete {
		interrupt := is.store[id]

		// Remove from indices
		is.removeFromIndex(is.byRequest, interrupt.RequestID, id)
		is.removeFromIndex(is.bySession, interrupt.SessionID, id)

		// Remove from store
		delete(is.store, id)
		count++
	}

	if is.logger != nil && count > 0 {
		is.logger.Info("interrupts_cleaned_up", "count", count)
	}

	return count
}

// removeFromIndex removes an interrupt from an index slice.
func (is *InterruptService) removeFromIndex(index map[string][]*KernelInterrupt, key, interruptID string) {
	interrupts := index[key]
	for i, interrupt := range interrupts {
		if interrupt.ID == interruptID {
			index[key] = append(interrupts[:i], interrupts[i+1:]...)
			break
		}
	}
}

// =============================================================================
// Statistics
// =============================================================================

// GetStats returns statistics about interrupts.
func (is *InterruptService) GetStats() map[string]int {
	is.mu.RLock()
	defer is.mu.RUnlock()

	stats := map[string]int{
		"total":     len(is.store),
		"pending":   0,
		"resolved":  0,
		"expired":   0,
		"cancelled": 0,
	}

	for _, interrupt := range is.store {
		switch interrupt.Status {
		case InterruptStatusPending:
			stats["pending"]++
		case InterruptStatusResolved:
			stats["resolved"]++
		case InterruptStatusExpired:
			stats["expired"]++
		case InterruptStatusCancelled:
			stats["cancelled"]++
		}
	}

	return stats
}

// GetPendingCount returns the number of pending interrupts.
func (is *InterruptService) GetPendingCount() int {
	is.mu.RLock()
	defer is.mu.RUnlock()

	count := 0
	for _, interrupt := range is.store {
		if interrupt.IsPending() {
			count++
		}
	}
	return count
}
