package commbus

import (
	"context"
	"log"
	"sync"
	"time"
)

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
	subscribers  map[string][]HandlerFunc
	middleware   []Middleware
	queryTimeout time.Duration
	mu           sync.RWMutex
}

// NewInMemoryCommBus creates a new InMemoryCommBus.
func NewInMemoryCommBus(queryTimeout time.Duration) *InMemoryCommBus {
	return &InMemoryCommBus{
		handlers:     make(map[string]HandlerFunc),
		subscribers:  make(map[string][]HandlerFunc),
		middleware:   make([]Middleware, 0),
		queryTimeout: queryTimeout,
	}
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
		log.Printf("Event %s aborted by middleware", eventType)
		return nil
	}

	// Get subscribers
	b.mu.RLock()
	subscribers := b.subscribers[eventType]
	subscribersCopy := make([]HandlerFunc, len(subscribers))
	copy(subscribersCopy, subscribers)
	b.mu.RUnlock()

	if len(subscribersCopy) == 0 {
		log.Printf("No subscribers for event %s", eventType)
		_, _ = b.runMiddlewareAfter(ctx, event, nil, nil)
		return nil
	}

	// Fan-out to all subscribers concurrently
	var wg sync.WaitGroup
	errors := make([]error, len(subscribersCopy))

	for i, handler := range subscribersCopy {
		wg.Add(1)
		go func(idx int, h HandlerFunc) {
			defer wg.Done()
			_, err := h(ctx, processedEvent)
			if err != nil {
				errors[idx] = err
				log.Printf("Subscriber %d failed for %s: %v", idx, eventType, err)
			}
		}(i, handler)
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
		log.Printf("Command %s aborted by middleware", messageType)
		return nil
	}

	// Get handler
	b.mu.RLock()
	handler, exists := b.handlers[messageType]
	b.mu.RUnlock()

	if !exists {
		log.Printf("No handler for command %s", messageType)
		return nil
	}

	// Execute handler
	var handlerError error
	_, handlerError = handler(ctx, processed)
	if handlerError != nil {
		log.Printf("Command handler failed for %s: %v", messageType, handlerError)
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
func (b *InMemoryCommBus) Subscribe(eventType string, handler HandlerFunc) func() {
	b.mu.Lock()
	if _, exists := b.subscribers[eventType]; !exists {
		b.subscribers[eventType] = make([]HandlerFunc, 0)
	}
	b.subscribers[eventType] = append(b.subscribers[eventType], handler)
	b.mu.Unlock()

	log.Printf("Subscribed to %s", eventType)

	// Return unsubscribe function
	return func() {
		b.mu.Lock()
		defer b.mu.Unlock()

		subs := b.subscribers[eventType]
		for i, h := range subs {
			// Compare function pointers
			if &h == &handler {
				b.subscribers[eventType] = append(subs[:i], subs[i+1:]...)
				log.Printf("Unsubscribed from %s", eventType)
				break
			}
		}
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
	log.Printf("Registered handler for %s", messageType)
	return nil
}

// AddMiddleware adds middleware to the bus.
// Middleware is executed in registration order.
func (b *InMemoryCommBus) AddMiddleware(middleware Middleware) {
	b.mu.Lock()
	defer b.mu.Unlock()

	b.middleware = append(b.middleware, middleware)
	log.Printf("Added middleware")
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

	subs := b.subscribers[eventType]
	result := make([]HandlerFunc, len(subs))
	copy(result, subs)
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
	b.subscribers = make(map[string][]HandlerFunc)
	b.middleware = make([]Middleware, 0)
	log.Printf("Cleared all handlers, subscribers, and middleware")
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
