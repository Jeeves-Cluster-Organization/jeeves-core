// Package commbus provides CommBus Middleware implementations.
//
// This module provides middleware implementations for the CommBus.
// Middleware intercepts messages before/after handling for cross-cutting concerns.
//
// Available Middleware:
//   - LoggingMiddleware: Structured logging of all messages
//   - TelemetryMiddleware: Metrics and timing
//   - CircuitBreakerMiddleware: Failure protection
//
// Constitutional Reference:
//   - Core Engine P3: Bounded Efficiency (circuit breaker)
//   - Avionics R5: Defensive Error Handling
package commbus

import (
	"context"
	"log"
	"sync"
	"time"
)

// =============================================================================
// LOGGING MIDDLEWARE
// =============================================================================

// LoggingMiddleware logs all message traffic.
type LoggingMiddleware struct {
	LogLevel string
}

// NewLoggingMiddleware creates a new LoggingMiddleware.
func NewLoggingMiddleware(logLevel string) *LoggingMiddleware {
	return &LoggingMiddleware{LogLevel: logLevel}
}

// Before logs message receipt.
func (m *LoggingMiddleware) Before(ctx context.Context, message Message) (Message, error) {
	msgType := GetMessageType(message)
	category := message.Category()
	log.Printf("CommBus: %s %s", category, msgType)
	return message, nil
}

// After logs message completion.
func (m *LoggingMiddleware) After(ctx context.Context, message Message, result any, err error) (any, error) {
	msgType := GetMessageType(message)
	if err != nil {
		log.Printf("CommBus: %s failed: %v", msgType, err)
	} else {
		log.Printf("CommBus: %s completed", msgType)
	}
	return result, nil
}

// =============================================================================
// TELEMETRY MIDDLEWARE
// =============================================================================

// TelemetryMiddleware collects telemetry data.
//
// Tracks:
//   - Message counts by type
//   - Execution times
//   - Error counts
type TelemetryMiddleware struct {
	counts     map[string]int
	times      map[string]float64
	errors     map[string]int
	startTimes map[uintptr]time.Time
	mu         sync.Mutex
}

// NewTelemetryMiddleware creates a new TelemetryMiddleware.
func NewTelemetryMiddleware() *TelemetryMiddleware {
	return &TelemetryMiddleware{
		counts:     make(map[string]int),
		times:      make(map[string]float64),
		errors:     make(map[string]int),
		startTimes: make(map[uintptr]time.Time),
	}
}

// Before records start time.
func (m *TelemetryMiddleware) Before(ctx context.Context, message Message) (Message, error) {
	msgType := GetMessageType(message)

	m.mu.Lock()
	defer m.mu.Unlock()

	m.counts[msgType]++
	// Use a simple ID based on the message pointer
	msgID := uintptr(0) // placeholder - in real impl would use unique ID
	m.startTimes[msgID] = time.Now()
	return message, nil
}

// After records completion time and errors.
func (m *TelemetryMiddleware) After(ctx context.Context, message Message, result any, err error) (any, error) {
	msgType := GetMessageType(message)

	m.mu.Lock()
	defer m.mu.Unlock()

	// Record time
	msgID := uintptr(0) // placeholder
	if start, ok := m.startTimes[msgID]; ok {
		elapsed := time.Since(start).Seconds()
		m.times[msgType] += elapsed
		delete(m.startTimes, msgID)
	}

	// Record error
	if err != nil {
		m.errors[msgType]++
	}

	return result, nil
}

// GetStats returns telemetry statistics.
func (m *TelemetryMiddleware) GetStats() map[string]any {
	m.mu.Lock()
	defer m.mu.Unlock()

	countsCopy := make(map[string]int)
	for k, v := range m.counts {
		countsCopy[k] = v
	}

	timesCopy := make(map[string]float64)
	for k, v := range m.times {
		timesCopy[k] = v
	}

	errorsCopy := make(map[string]int)
	for k, v := range m.errors {
		errorsCopy[k] = v
	}

	return map[string]any{
		"counts":      countsCopy,
		"total_times": timesCopy,
		"errors":      errorsCopy,
	}
}

// Reset resets telemetry counters.
func (m *TelemetryMiddleware) Reset() {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.counts = make(map[string]int)
	m.times = make(map[string]float64)
	m.errors = make(map[string]int)
}

// =============================================================================
// CIRCUIT BREAKER MIDDLEWARE
// =============================================================================

// CircuitBreakerState represents the state for circuit breaker.
type CircuitBreakerState struct {
	Failures    int
	LastFailure time.Time
	State       string // "closed", "open", "half-open"
}

// CircuitBreakerMiddleware implements the circuit breaker pattern.
//
// Protects against cascading failures by:
//   - Opening circuit after N failures
//   - Blocking requests while open
//   - Testing with single request in half-open state
//   - Closing circuit after success
//
// Constitutional Reference: Core Engine P3 (Bounded Efficiency)
type CircuitBreakerMiddleware struct {
	failureThreshold int
	resetTimeout     time.Duration
	excludedTypes    map[string]struct{}
	states           map[string]*CircuitBreakerState
	mu               sync.Mutex
}

// NewCircuitBreakerMiddleware creates a new CircuitBreakerMiddleware.
func NewCircuitBreakerMiddleware(failureThreshold int, resetTimeout time.Duration, excludedTypes []string) *CircuitBreakerMiddleware {
	excluded := make(map[string]struct{})
	for _, t := range excludedTypes {
		excluded[t] = struct{}{}
	}

	return &CircuitBreakerMiddleware{
		failureThreshold: failureThreshold,
		resetTimeout:     resetTimeout,
		excludedTypes:    excluded,
		states:           make(map[string]*CircuitBreakerState),
	}
}

// getState gets or creates state for a message type.
func (m *CircuitBreakerMiddleware) getState(msgType string) *CircuitBreakerState {
	if _, exists := m.states[msgType]; !exists {
		m.states[msgType] = &CircuitBreakerState{State: "closed"}
	}
	return m.states[msgType]
}

// Before checks circuit breaker state.
func (m *CircuitBreakerMiddleware) Before(ctx context.Context, message Message) (Message, error) {
	msgType := GetMessageType(message)

	// Excluded types bypass breaker
	if _, excluded := m.excludedTypes[msgType]; excluded {
		return message, nil
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	state := m.getState(msgType)
	now := time.Now()

	if state.State == "open" {
		// Check if we should try half-open
		if now.Sub(state.LastFailure) >= m.resetTimeout {
			state.State = "half-open"
			log.Printf("Circuit half-open for %s", msgType)
		} else {
			log.Printf("Circuit open for %s, blocking request", msgType)
			return nil, nil // Block the request
		}
	}

	return message, nil
}

// After updates circuit breaker state based on result.
func (m *CircuitBreakerMiddleware) After(ctx context.Context, message Message, result any, err error) (any, error) {
	msgType := GetMessageType(message)

	// Excluded types bypass breaker
	if _, excluded := m.excludedTypes[msgType]; excluded {
		return result, nil
	}

	m.mu.Lock()
	defer m.mu.Unlock()

	state := m.getState(msgType)
	now := time.Now()

	if err != nil {
		// Record failure
		state.Failures++
		state.LastFailure = now

		if state.State == "half-open" {
			// Failed during half-open, reopen
			state.State = "open"
			log.Printf("Circuit reopened for %s", msgType)
		} else if state.Failures >= m.failureThreshold {
			// Threshold reached, open circuit
			state.State = "open"
			log.Printf("Circuit opened for %s after %d failures", msgType, state.Failures)
		}
	} else {
		// Success
		if state.State == "half-open" {
			// Success in half-open, close circuit
			state.State = "closed"
			state.Failures = 0
			log.Printf("Circuit closed for %s", msgType)
		}
	}

	return result, nil
}

// GetStates returns current circuit states.
func (m *CircuitBreakerMiddleware) GetStates() map[string]string {
	m.mu.Lock()
	defer m.mu.Unlock()

	result := make(map[string]string)
	for k, v := range m.states {
		result[k] = v.State
	}
	return result
}

// Reset resets circuit breaker state.
func (m *CircuitBreakerMiddleware) Reset(msgType *string) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if msgType != nil {
		delete(m.states, *msgType)
	} else {
		m.states = make(map[string]*CircuitBreakerState)
	}
}

// =============================================================================
// RETRY MIDDLEWARE
// =============================================================================

// RetryMiddleware adds retry capability for queries.
// Note: Only retries queries, not events/commands.
//
// Constitutional Reference: Core Engine P3 (Bounded Efficiency)
type RetryMiddleware struct {
	maxRetries      int // Constitutional: max 2 retries
	retryDelay      time.Duration
	retryableErrors map[string]struct{}
	retryCounts     map[uintptr]int
	mu              sync.Mutex
}

// NewRetryMiddleware creates a new RetryMiddleware.
func NewRetryMiddleware(maxRetries int, retryDelay time.Duration, retryableErrors []string) *RetryMiddleware {
	errors := make(map[string]struct{})
	for _, e := range retryableErrors {
		errors[e] = struct{}{}
	}

	return &RetryMiddleware{
		maxRetries:      maxRetries,
		retryDelay:      retryDelay,
		retryableErrors: errors,
		retryCounts:     make(map[uintptr]int),
	}
}

// Before initializes retry count.
func (m *RetryMiddleware) Before(ctx context.Context, message Message) (Message, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	msgID := uintptr(0) // placeholder
	m.retryCounts[msgID] = 0
	return message, nil
}

// After handles retries for failed queries.
func (m *RetryMiddleware) After(ctx context.Context, message Message, result any, err error) (any, error) {
	// Only retry queries
	category := message.Category()
	if category != string(MessageCategoryQuery) {
		m.mu.Lock()
		msgID := uintptr(0) // placeholder
		delete(m.retryCounts, msgID)
		m.mu.Unlock()
		return result, nil
	}

	// Note: Actual retry logic would need bus reference
	// This is a placeholder showing the pattern

	m.mu.Lock()
	msgID := uintptr(0) // placeholder
	delete(m.retryCounts, msgID)
	m.mu.Unlock()

	return result, nil
}

// Ensure all middleware types implement Middleware interface.
var (
	_ Middleware = (*LoggingMiddleware)(nil)
	_ Middleware = (*TelemetryMiddleware)(nil)
	_ Middleware = (*CircuitBreakerMiddleware)(nil)
	_ Middleware = (*RetryMiddleware)(nil)
)
