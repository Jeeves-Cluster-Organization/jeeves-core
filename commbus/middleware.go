// Package commbus provides CommBus Middleware implementations.
//
// This module provides middleware implementations for the CommBus.
// Middleware intercepts messages before/after handling for cross-cutting concerns.
//
// Available Middleware:
//   - LoggingMiddleware: Structured logging of all messages
//   - CircuitBreakerMiddleware: Failure protection for message types
//
// Architectural Note (Path A - Pipeline OS):
//   Go Core owns pipeline orchestration, bounds, and CommBus circuit breakers.
//   Python App owns LLM provider retry, observability/metrics, and tool resilience.
//   See HANDOFF.md for Path B extension points if full OS mode is needed later.
//
// Constitutional Reference:
//   - Core Engine P3: Bounded Efficiency (circuit breaker)
//   - Avionics R5: Defensive Error Handling
package commbus

import (
	"context"
	"sync"
	"time"
)

// =============================================================================
// LOGGING MIDDLEWARE
// =============================================================================

// LoggingMiddleware logs all message traffic using structured logging.
type LoggingMiddleware struct {
	LogLevel string
	logger   BusLogger
}

// NewLoggingMiddleware creates a new LoggingMiddleware with default logger.
func NewLoggingMiddleware(logLevel string) *LoggingMiddleware {
	return &LoggingMiddleware{LogLevel: logLevel, logger: &defaultBusLogger{}}
}

// NewLoggingMiddlewareWithLogger creates a new LoggingMiddleware with custom logger.
func NewLoggingMiddlewareWithLogger(logLevel string, logger BusLogger) *LoggingMiddleware {
	if logger == nil {
		logger = &defaultBusLogger{}
	}
	return &LoggingMiddleware{LogLevel: logLevel, logger: logger}
}

// Before logs message receipt.
func (m *LoggingMiddleware) Before(ctx context.Context, message Message) (Message, error) {
	msgType := GetMessageType(message)
	category := message.Category()
	m.logger.Debug("commbus_message_received", "category", category, "message_type", msgType)
	return message, nil
}

// After logs message completion.
func (m *LoggingMiddleware) After(ctx context.Context, message Message, result any, err error) (any, error) {
	msgType := GetMessageType(message)
	if err != nil {
		m.logger.Warn("commbus_message_failed", "message_type", msgType, "error", err.Error())
	} else {
		m.logger.Debug("commbus_message_completed", "message_type", msgType)
	}
	return result, nil
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
	logger           BusLogger
	mu               sync.Mutex
}

// NewCircuitBreakerMiddleware creates a new CircuitBreakerMiddleware with default logger.
func NewCircuitBreakerMiddleware(failureThreshold int, resetTimeout time.Duration, excludedTypes []string) *CircuitBreakerMiddleware {
	return NewCircuitBreakerMiddlewareWithLogger(failureThreshold, resetTimeout, excludedTypes, &defaultBusLogger{})
}

// NewCircuitBreakerMiddlewareWithLogger creates a new CircuitBreakerMiddleware with custom logger.
func NewCircuitBreakerMiddlewareWithLogger(failureThreshold int, resetTimeout time.Duration, excludedTypes []string, logger BusLogger) *CircuitBreakerMiddleware {
	excluded := make(map[string]struct{})
	for _, t := range excludedTypes {
		excluded[t] = struct{}{}
	}

	if logger == nil {
		logger = &defaultBusLogger{}
	}

	return &CircuitBreakerMiddleware{
		failureThreshold: failureThreshold,
		resetTimeout:     resetTimeout,
		excludedTypes:    excluded,
		states:           make(map[string]*CircuitBreakerState),
		logger:           logger,
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
			m.logger.Info("circuit_half_open", "message_type", msgType)
		} else {
			m.logger.Warn("circuit_open_blocking_request", "message_type", msgType)
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
			m.logger.Warn("circuit_reopened", "message_type", msgType)
		} else if m.failureThreshold > 0 && state.Failures >= m.failureThreshold {
			// Threshold reached, open circuit (threshold=0 means never open)
			state.State = "open"
			m.logger.Warn("circuit_opened", "message_type", msgType, "failures", state.Failures)
		}
	} else {
		// Success
		if state.State == "half-open" {
			// Success in half-open, close circuit
			state.State = "closed"
			state.Failures = 0
			m.logger.Info("circuit_closed", "message_type", msgType)
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

// Ensure all middleware types implement Middleware interface.
var (
	_ Middleware = (*LoggingMiddleware)(nil)
	_ Middleware = (*CircuitBreakerMiddleware)(nil)
)
