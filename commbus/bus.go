package commbus

import (
	"context"
	"fmt"
	"log"
	"sync"
	"sync/atomic"
	"time"
)

// BusLogger is the interface for structured logging in CommBus.
// This enables dependency injection of loggers for testability.
// It's separate from the protocols.Logger to allow simpler implementation.
type BusLogger interface {
	Debug(msg string, keysAndValues ...any)
	Info(msg string, keysAndValues ...any)
	Warn(msg string, keysAndValues ...any)
	Error(msg string, keysAndValues ...any)
}

// defaultBusLogger wraps standard log package for backward compatibility.
type defaultBusLogger struct{}

func (l *defaultBusLogger) Debug(msg string, keysAndValues ...any) {
	log.Printf("[DEBUG] %s %v", msg, keysAndValues)
}

func (l *defaultBusLogger) Info(msg string, keysAndValues ...any) {
	log.Printf("[INFO] %s %v", msg, keysAndValues)
}

func (l *defaultBusLogger) Warn(msg string, keysAndValues ...any) {
	log.Printf("[WARN] %s %v", msg, keysAndValues)
}

func (l *defaultBusLogger) Error(msg string, keysAndValues ...any) {
	log.Printf("[ERROR] %s %v", msg, keysAndValues)
}

// noopBusLogger is a no-op logger for when logging is disabled.
type noopBusLogger struct{}

func (l *noopBusLogger) Debug(msg string, keysAndValues ...any) {}
func (l *noopBusLogger) Info(msg string, keysAndValues ...any)  {}
func (l *noopBusLogger) Warn(msg string, keysAndValues ...any)  {}
func (l *noopBusLogger) Error(msg string, keysAndValues ...any) {}

// NoopBusLogger returns a logger that discards all output.
func NoopBusLogger() BusLogger {
	return &noopBusLogger{}
}

// subscriberEntry holds a subscriber with its unique ID for proper unsubscribe support.
type subscriberEntry struct {
	id      string
	handler HandlerFunc
}

// InMemoryCommBus is an in-memory implementation of CommBusProtocol.
//
// Thread-safe, async-compatible message bus for single-process deployments.
//
// Features:
//   - Event fan-out to multiple subscribers
//   - Query request-response with timeout
//   - Command fire-and-forget
//   - Middleware chain for cross-cutting concerns
//   - Handler introspection
//   - Structured logging with injectable logger
//
// Usage:
//
//	bus := NewInMemoryCommBus(30 * time.Second)
//
//	// Register handlers
//	bus.RegisterHandler("GetSettings", settingsHandler)
//	bus.Subscribe("AgentStarted", telemetryHandler)
//
//	// Use the bus
//	bus.Publish(ctx, &AgentStarted{...})
//	settings, _ := bus.QuerySync(ctx, &GetSettings{Key: "llm"})
type InMemoryCommBus struct {
	handlers     map[string]HandlerFunc
	subscribers  map[string][]subscriberEntry
	middleware   []Middleware
	queryTimeout time.Duration
	nextSubID    uint64 // Atomic counter for unique subscriber IDs
	logger       BusLogger
	mu           sync.RWMutex
}

// NewInMemoryCommBus creates a new InMemoryCommBus with default logger.
func NewInMemoryCommBus(queryTimeout time.Duration) *InMemoryCommBus {
	return NewInMemoryCommBusWithLogger(queryTimeout, &defaultBusLogger{})
}

// NewInMemoryCommBusWithLogger creates a new InMemoryCommBus with a custom logger.
func NewInMemoryCommBusWithLogger(queryTimeout time.Duration, logger BusLogger) *InMemoryCommBus {
	if logger == nil {
		logger = &defaultBusLogger{}
	}
	return &InMemoryCommBus{
		handlers:     make(map[string]HandlerFunc),
		subscribers:  make(map[string][]subscriberEntry),
		middleware:   make([]Middleware, 0),
		queryTimeout: queryTimeout,
		nextSubID:    0,
		logger:       logger,
	}
}

// SetLogger sets the logger. Use NoopBusLogger() to disable logging.
func (b *InMemoryCommBus) SetLogger(logger BusLogger) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if logger == nil {
		logger = &defaultBusLogger{}
	}
	b.logger = logger
}

// =============================================================================
// MESSAGING
// =============================================================================

// Publish publishes an event to all subscribers.
// Events are processed concurrently by all subscribers.
// Subscriber errors are logged but don't stop other subscribers.
func (b *InMemoryCommBus) Publish(ctx context.Context, event Message) error {
	eventType := GetMessageType(event)

	// Run middleware before
	processedEvent, err := b.runMiddlewareBefore(ctx, event)
	if err != nil {
		return err
	}
	if processedEvent == nil {
		b.logger.Debug("event_aborted_by_middleware", "event_type", eventType)
		return nil
	}

	// Get subscribers - copy the entries to avoid holding lock during execution
	b.mu.RLock()
	entries := b.subscribers[eventType]
	entriesCopy := make([]subscriberEntry, len(entries))
	copy(entriesCopy, entries)
	b.mu.RUnlock()

	if len(entriesCopy) == 0 {
		b.logger.Debug("no_subscribers_for_event", "event_type", eventType)
		_, _ = b.runMiddlewareAfter(ctx, event, nil, nil)
		return nil
	}

	// Fan-out to all subscribers concurrently
	var wg sync.WaitGroup
	errors := make([]error, len(entriesCopy))

	for i, entry := range entriesCopy {
		wg.Add(1)
		go func(idx int, h HandlerFunc) {
			defer wg.Done()
			_, err := h(ctx, processedEvent)
			if err != nil {
				errors[idx] = err
				b.logger.Warn("subscriber_failed", "subscriber_idx", idx, "event_type", eventType, "error", err.Error())
			}
		}(i, entry.handler)
	}

	wg.Wait()

	// Collect first error for middleware
	var firstError error
	for _, e := range errors {
		if e != nil {
			firstError = e
			break
		}
	}

	// Run middleware after
	_, _ = b.runMiddlewareAfter(ctx, event, nil, firstError)
	return nil
}

// Send sends a command to its handler.
// Commands are fire-and-forget. Handler errors are logged.
func (b *InMemoryCommBus) Send(ctx context.Context, command Message) error {
	messageType := GetMessageType(command)

	// Run middleware before
	processed, err := b.runMiddlewareBefore(ctx, command)
	if err != nil {
		return err
	}
	if processed == nil {
		b.logger.Debug("command_aborted_by_middleware", "message_type", messageType)
		return nil
	}

	// Get handler
	b.mu.RLock()
	handler, exists := b.handlers[messageType]
	b.mu.RUnlock()

	if !exists {
		b.logger.Debug("no_handler_for_command", "message_type", messageType)
		return nil
	}

	// Execute handler
	var handlerError error
	_, handlerError = handler(ctx, processed)
	if handlerError != nil {
		b.logger.Warn("command_handler_failed", "message_type", messageType, "error", handlerError.Error())
	}

	// Run middleware after
	_, _ = b.runMiddlewareAfter(ctx, command, nil, handlerError)
	return handlerError
}

// QuerySync sends a query and waits for response.
// Queries have a timeout and require a registered handler.
func (b *InMemoryCommBus) QuerySync(ctx context.Context, query Query) (any, error) {
	messageType := GetMessageType(query)

	// Run middleware before
	processed, err := b.runMiddlewareBefore(ctx, query)
	if err != nil {
		return nil, err
	}
	if processed == nil {
		return nil, NewNoHandlerError(messageType)
	}

	// Get handler
	b.mu.RLock()
	handler, exists := b.handlers[messageType]
	b.mu.RUnlock()

	if !exists {
		return nil, NewNoHandlerError(messageType)
	}

	// Create timeout context
	timeoutCtx, cancel := context.WithTimeout(ctx, b.queryTimeout)
	defer cancel()

	// Execute handler with timeout
	type result struct {
		value any
		err   error
	}
	resultCh := make(chan result, 1)

	go func() {
		v, e := handler(timeoutCtx, processed.(Message))
		resultCh <- result{value: v, err: e}
	}()

	select {
	case <-timeoutCtx.Done():
		err := NewQueryTimeoutError(messageType, b.queryTimeout.Seconds())
		_, _ = b.runMiddlewareAfter(ctx, query, nil, err)
		return nil, err
	case res := <-resultCh:
		// Run middleware after
		finalResult, middlewareErr := b.runMiddlewareAfter(ctx, query, res.value, res.err)
		// If middleware returned error, use that instead of handler error
		if middlewareErr != nil {
			return finalResult, middlewareErr
		}
		return finalResult, res.err
	}
}

// =============================================================================
// REGISTRATION
// =============================================================================

// Subscribe subscribes to an event type.
// Returns an unsubscribe function for cleanup.
// The unsubscribe function is safe to call multiple times (idempotent).
func (b *InMemoryCommBus) Subscribe(eventType string, handler HandlerFunc) func() {
	// Generate unique subscriber ID using atomic counter
	subID := fmt.Sprintf("sub_%d", atomic.AddUint64(&b.nextSubID, 1))

	b.mu.Lock()
	if _, exists := b.subscribers[eventType]; !exists {
		b.subscribers[eventType] = make([]subscriberEntry, 0)
	}
	b.subscribers[eventType] = append(b.subscribers[eventType], subscriberEntry{
		id:      subID,
		handler: handler,
	})
	b.mu.Unlock()

	b.logger.Debug("subscribed", "event_type", eventType, "sub_id", subID)

	// Return unsubscribe function that captures the unique ID
	return func() {
		b.mu.Lock()
		defer b.mu.Unlock()

		entries := b.subscribers[eventType]
		for i, entry := range entries {
			if entry.id == subID {
				// Remove by swapping with last element (order doesn't matter for fan-out)
				b.subscribers[eventType] = append(entries[:i], entries[i+1:]...)
				b.logger.Debug("unsubscribed", "event_type", eventType, "sub_id", subID)
				return
			}
		}
		// Already unsubscribed - this is safe (idempotent)
	}
}

// RegisterHandler registers a handler for a message type.
// Only one handler per message type is allowed.
func (b *InMemoryCommBus) RegisterHandler(messageType string, handler HandlerFunc) error {
	b.mu.Lock()
	defer b.mu.Unlock()

	if _, exists := b.handlers[messageType]; exists {
		return NewHandlerAlreadyRegisteredError(messageType)
	}

	b.handlers[messageType] = handler
	b.logger.Debug("handler_registered", "message_type", messageType)
	return nil
}

// AddMiddleware adds middleware to the bus.
// Middleware is executed in registration order.
func (b *InMemoryCommBus) AddMiddleware(middleware Middleware) {
	b.mu.Lock()
	defer b.mu.Unlock()

	b.middleware = append(b.middleware, middleware)
	b.logger.Debug("middleware_added")
}

// =============================================================================
// INTROSPECTION
// =============================================================================

// HasHandler checks if a handler is registered for a message type.
func (b *InMemoryCommBus) HasHandler(messageType string) bool {
	b.mu.RLock()
	defer b.mu.RUnlock()

	_, exists := b.handlers[messageType]
	return exists
}

// GetSubscribers gets all subscribers for an event type.
func (b *InMemoryCommBus) GetSubscribers(eventType string) []HandlerFunc {
	b.mu.RLock()
	defer b.mu.RUnlock()

	entries := b.subscribers[eventType]
	result := make([]HandlerFunc, len(entries))
	for i, entry := range entries {
		result[i] = entry.handler
	}
	return result
}

// GetRegisteredTypes gets all registered message types (handlers + subscriptions).
func (b *InMemoryCommBus) GetRegisteredTypes() []string {
	b.mu.RLock()
	defer b.mu.RUnlock()

	types := make(map[string]struct{})
	for t := range b.handlers {
		types[t] = struct{}{}
	}
	for t := range b.subscribers {
		types[t] = struct{}{}
	}

	result := make([]string, 0, len(types))
	for t := range types {
		result = append(result, t)
	}
	return result
}

// =============================================================================
// LIFECYCLE
// =============================================================================

// Clear clears all handlers, subscribers, and middleware.
// Useful for testing.
func (b *InMemoryCommBus) Clear() {
	b.mu.Lock()
	defer b.mu.Unlock()

	b.handlers = make(map[string]HandlerFunc)
	b.subscribers = make(map[string][]subscriberEntry)
	b.middleware = make([]Middleware, 0)
	b.logger.Debug("bus_cleared")
}

// =============================================================================
// INTERNAL HELPERS
// =============================================================================

// runMiddlewareBefore runs middleware before chain.
func (b *InMemoryCommBus) runMiddlewareBefore(ctx context.Context, message Message) (Message, error) {
	b.mu.RLock()
	middlewareCopy := make([]Middleware, len(b.middleware))
	copy(middlewareCopy, b.middleware)
	b.mu.RUnlock()

	current := message
	for _, mw := range middlewareCopy {
		result, err := mw.Before(ctx, current)
		if err != nil {
			return nil, err
		}
		if result == nil {
			return nil, nil
		}
		current = result
	}
	return current, nil
}

// runMiddlewareAfter runs middleware after chain (reverse order).
func (b *InMemoryCommBus) runMiddlewareAfter(ctx context.Context, message Message, result any, err error) (any, error) {
	b.mu.RLock()
	middlewareCopy := make([]Middleware, len(b.middleware))
	copy(middlewareCopy, b.middleware)
	b.mu.RUnlock()

	currentResult := result
	// Reverse order
	for i := len(middlewareCopy) - 1; i >= 0; i-- {
		afterResult, afterErr := middlewareCopy[i].After(ctx, message, currentResult, err)
		if afterErr != nil {
			err = afterErr
		}
		if afterResult != nil {
			currentResult = afterResult
		}
	}
	return currentResult, err
}

// Ensure InMemoryCommBus implements CommBus interface.
var _ CommBus = (*InMemoryCommBus)(nil)
