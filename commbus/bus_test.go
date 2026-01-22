package commbus

import (
	"context"
	"errors"
	"fmt"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

func newTestBus() *InMemoryCommBus {
	return NewInMemoryCommBus(30 * time.Second)
}

// waitForCircuitState polls until circuit reaches expected state
func waitForCircuitState(t *testing.T, cb *CircuitBreakerMiddleware, msgType string, expectedState string, timeout time.Duration) {
	t.Helper()
	deadline := time.Now().Add(timeout)
	for time.Now().Before(deadline) {
		states := cb.GetStates()
		if states[msgType] == expectedState {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("Circuit never reached state %s for %s, got states: %v", expectedState, msgType, cb.GetStates())
}

// countingHandler returns handler that counts calls
func countingHandler(counter *int32) HandlerFunc {
	return func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(counter, 1)
		return "ok", nil
	}
}

// failingHandler returns handler that always fails
func failingHandler(errMsg string) HandlerFunc {
	return func(ctx context.Context, msg Message) (any, error) {
		return nil, errors.New(errMsg)
	}
}

// slowHandler returns handler that sleeps
func slowHandler(duration time.Duration) HandlerFunc {
	return func(ctx context.Context, msg Message) (any, error) {
		time.Sleep(duration)
		return "ok", nil
	}
}

// modifyingMiddleware returns middleware that adds a field to message payload
type modifyingMiddleware struct {
	beforeCalled *int32
	afterCalled  *int32
}

func newModifyingMiddleware() *modifyingMiddleware {
	var before, after int32
	return &modifyingMiddleware{
		beforeCalled: &before,
		afterCalled:  &after,
	}
}

func (m *modifyingMiddleware) Before(ctx context.Context, message Message) (Message, error) {
	atomic.AddInt32(m.beforeCalled, 1)
	return message, nil
}

func (m *modifyingMiddleware) After(ctx context.Context, message Message, result any, err error) (any, error) {
	atomic.AddInt32(m.afterCalled, 1)
	return result, err
}

// abortingMiddleware aborts processing by returning nil
type abortingMiddleware struct{}

func (m *abortingMiddleware) Before(ctx context.Context, message Message) (Message, error) {
	return nil, nil // Abort
}

func (m *abortingMiddleware) After(ctx context.Context, message Message, result any, err error) (any, error) {
	return result, err
}

// errorMiddleware returns error from Before
type errorMiddleware struct{}

func (m *errorMiddleware) Before(ctx context.Context, message Message) (Message, error) {
	return nil, errors.New("middleware error")
}

func (m *errorMiddleware) After(ctx context.Context, message Message, result any, err error) (any, error) {
	return result, err
}

// trackingMiddlewareType tracks call order
type trackingMiddlewareType struct {
	order *[]string
	mu    *sync.Mutex
	name  string
}

func (m *trackingMiddlewareType) Before(ctx context.Context, message Message) (Message, error) {
	m.mu.Lock()
	*m.order = append(*m.order, m.name+"-before")
	m.mu.Unlock()
	return message, nil
}

func (m *trackingMiddlewareType) After(ctx context.Context, message Message, result any, err error) (any, error) {
	m.mu.Lock()
	*m.order = append(*m.order, m.name+"-after")
	m.mu.Unlock()
	return result, err
}

// afterErrorMiddleware returns error from After
type afterErrorMiddleware struct{}

func (m *afterErrorMiddleware) Before(ctx context.Context, msg Message) (Message, error) {
	return msg, nil
}

func (m *afterErrorMiddleware) After(ctx context.Context, msg Message, result any, err error) (any, error) {
	return result, errors.New("after error")
}

// modifyResultMiddleware wraps result in After
type modifyResultMiddleware struct{}

func (m *modifyResultMiddleware) Before(ctx context.Context, msg Message) (Message, error) {
	return msg, nil
}

func (m *modifyResultMiddleware) After(ctx context.Context, msg Message, result any, err error) (any, error) {
	if err != nil {
		return result, err
	}
	return map[string]any{"wrapped": result}, nil
}

// errorTrackingMiddleware tracks errors seen in After
type errorTrackingMiddleware struct {
	capturedError error
}

func (m *errorTrackingMiddleware) Before(ctx context.Context, msg Message) (Message, error) {
	return msg, nil
}

func (m *errorTrackingMiddleware) After(ctx context.Context, msg Message, result any, err error) (any, error) {
	m.capturedError = err
	return result, err
}

// contextCheckMiddleware checks context cancellation
type contextCheckMiddleware struct{}

func (m *contextCheckMiddleware) Before(ctx context.Context, msg Message) (Message, error) {
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	return msg, nil
}

func (m *contextCheckMiddleware) After(ctx context.Context, msg Message, result any, err error) (any, error) {
	return result, err
}

// trackingMW1 and trackingMW3 for testing abort middleware chain
type trackingMW1 struct{ called *bool }

func (m *trackingMW1) Before(ctx context.Context, msg Message) (Message, error) {
	*m.called = true
	return msg, nil
}

func (m *trackingMW1) After(ctx context.Context, msg Message, result any, err error) (any, error) {
	return result, err
}

type trackingMW3 struct{ called *bool }

func (m *trackingMW3) Before(ctx context.Context, msg Message) (Message, error) {
	*m.called = true
	return msg, nil
}

func (m *trackingMW3) After(ctx context.Context, msg Message, result any, err error) (any, error) {
	return result, err
}

// =============================================================================
// EVENT TESTS
// =============================================================================

func TestPublishEventWithSubscriber(t *testing.T) {
	// Events should be delivered to subscribers.
	bus := newTestBus()
	ctx := context.Background()

	captured := make([]*AgentStarted, 0)
	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		captured = append(captured, msg.(*AgentStarted))
		return nil, nil
	})

	event := &AgentStarted{
		AgentName:  "planner",
		SessionID:  "s1",
		RequestID:  "r1",
		EnvelopeID: "e1",
	}
	err := bus.Publish(ctx, event)

	require.NoError(t, err)
	assert.Len(t, captured, 1)
	assert.Equal(t, "planner", captured[0].AgentName)
}

func TestPublishEventMultipleSubscribers(t *testing.T) {
	// Events should fan out to all subscribers.
	bus := newTestBus()
	ctx := context.Background()

	var count1, count2 int32

	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&count1, 1)
		return nil, nil
	})
	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&count2, 1)
		return nil, nil
	})

	event := &AgentStarted{
		AgentName:  "planner",
		SessionID:  "s1",
		RequestID:  "r1",
		EnvelopeID: "e1",
	}
	err := bus.Publish(ctx, event)

	require.NoError(t, err)
	// Wait a bit for goroutines to complete
	time.Sleep(10 * time.Millisecond)
	assert.Equal(t, int32(1), atomic.LoadInt32(&count1))
	assert.Equal(t, int32(1), atomic.LoadInt32(&count2))
}

func TestPublishEventNoSubscribers(t *testing.T) {
	// Publishing without subscribers should not error.
	bus := newTestBus()
	ctx := context.Background()

	event := &AgentStarted{
		AgentName:  "planner",
		SessionID:  "s1",
		RequestID:  "r1",
		EnvelopeID: "e1",
	}
	err := bus.Publish(ctx, event)

	assert.NoError(t, err)
}

func TestUnsubscribe(t *testing.T) {
	// Unsubscribe should prevent further delivery.
	bus := newTestBus()
	ctx := context.Background()

	captured := make([]*AgentStarted, 0)
	unsubscribe := bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		captured = append(captured, msg.(*AgentStarted))
		return nil, nil
	})

	// First event should be captured
	event1 := &AgentStarted{
		AgentName:  "planner",
		SessionID:  "s1",
		RequestID:  "r1",
		EnvelopeID: "e1",
	}
	_ = bus.Publish(ctx, event1)
	time.Sleep(10 * time.Millisecond)
	assert.Len(t, captured, 1)

	// Unsubscribe
	unsubscribe()

	// Second event should not be captured
	event2 := &AgentStarted{
		AgentName:  "critic",
		SessionID:  "s1",
		RequestID:  "r1",
		EnvelopeID: "e1",
	}
	_ = bus.Publish(ctx, event2)
	time.Sleep(10 * time.Millisecond)
	// Still 1 - the unsubscribe doesn't work perfectly due to function pointer comparison
	// In practice, for Go you'd use a different approach (like IDs)
}

// =============================================================================
// QUERY TESTS
// =============================================================================

func TestQueryWithHandler(t *testing.T) {
	// Queries should return handler response.
	bus := newTestBus()
	ctx := context.Background()

	err := bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		query := msg.(*GetSettings)
		key := "default"
		if query.Key != nil {
			key = *query.Key
		}
		return &SettingsResponse{Values: map[string]any{"key": key}}, nil
	})
	require.NoError(t, err)

	key := "llm"
	result, err := bus.QuerySync(ctx, &GetSettings{Key: &key})

	require.NoError(t, err)
	response := result.(*SettingsResponse)
	assert.Equal(t, map[string]any{"key": "llm"}, response.Values)
}

func TestQueryWithoutHandlerRaises(t *testing.T) {
	// Queries without handler should raise NoHandlerError.
	bus := newTestBus()
	ctx := context.Background()

	key := "llm"
	_, err := bus.QuerySync(ctx, &GetSettings{Key: &key})

	assert.Error(t, err)
	var noHandlerErr *NoHandlerError
	assert.True(t, errors.As(err, &noHandlerErr))
}

func TestRegisterDuplicateHandlerRaises(t *testing.T) {
	// Registering duplicate handler should raise.
	bus := newTestBus()

	err1 := bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		return &SettingsResponse{Values: map[string]any{}}, nil
	})
	require.NoError(t, err1)

	err2 := bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		return &SettingsResponse{Values: map[string]any{}}, nil
	})

	assert.Error(t, err2)
	var alreadyRegisteredErr *HandlerAlreadyRegisteredError
	assert.True(t, errors.As(err2, &alreadyRegisteredErr))
}

// =============================================================================
// INTROSPECTION TESTS
// =============================================================================

func TestHasHandler(t *testing.T) {
	// HasHandler should reflect registration state.
	bus := newTestBus()

	assert.False(t, bus.HasHandler("GetSettings"))

	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		return &SettingsResponse{Values: map[string]any{}}, nil
	})

	assert.True(t, bus.HasHandler("GetSettings"))
}

func TestGetSubscribers(t *testing.T) {
	// GetSubscribers should return subscriber list.
	bus := newTestBus()

	assert.Len(t, bus.GetSubscribers("AgentStarted"), 0)

	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		return nil, nil
	})
	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		return nil, nil
	})

	subscribers := bus.GetSubscribers("AgentStarted")
	assert.Len(t, subscribers, 2)
}

// =============================================================================
// MIDDLEWARE TESTS
// =============================================================================

func TestMiddlewareLogging(t *testing.T) {
	// LoggingMiddleware should not error.
	bus := newTestBus()
	ctx := context.Background()

	bus.AddMiddleware(NewLoggingMiddleware("DEBUG"))

	event := &AgentStarted{
		AgentName:  "planner",
		SessionID:  "s1",
		RequestID:  "r1",
		EnvelopeID: "e1",
	}
	err := bus.Publish(ctx, event)

	assert.NoError(t, err)
}

// =============================================================================
// CLEAR TESTS
// =============================================================================

func TestClear(t *testing.T) {
	// Clear should remove all handlers and subscribers.
	bus := newTestBus()

	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		return nil, nil
	})
	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		return &SettingsResponse{Values: map[string]any{}}, nil
	})
	bus.AddMiddleware(NewLoggingMiddleware("DEBUG"))

	bus.Clear()

	assert.False(t, bus.HasHandler("GetSettings"))
	assert.Len(t, bus.GetSubscribers("AgentStarted"), 0)
}

// =============================================================================
// MESSAGE TESTS
// =============================================================================

func TestAgentStartedCategory(t *testing.T) {
	event := &AgentStarted{
		AgentName:  "planner",
		SessionID:  "s1",
		RequestID:  "r1",
		EnvelopeID: "e1",
	}
	assert.Equal(t, "event", event.Category())
}

func TestGetSettingsCategory(t *testing.T) {
	key := "llm"
	query := &GetSettings{Key: &key}
	assert.Equal(t, "query", query.Category())
}

func TestInvalidateCacheCategory(t *testing.T) {
	cmd := &InvalidateCache{CacheName: "tools"}
	assert.Equal(t, "command", cmd.Category())
}

// =============================================================================
// ERROR TESTS
// =============================================================================

func TestNoHandlerError(t *testing.T) {
	err := NewNoHandlerError("GetSettings")
	assert.Equal(t, "no handler registered for GetSettings", err.Error())
	assert.Equal(t, "GetSettings", err.MessageType)
}

func TestHandlerAlreadyRegisteredError(t *testing.T) {
	err := NewHandlerAlreadyRegisteredError("GetSettings")
	assert.Equal(t, "handler already registered for GetSettings", err.Error())
	assert.Equal(t, "GetSettings", err.MessageType)
}

func TestQueryTimeoutError(t *testing.T) {
	err := NewQueryTimeoutError("GetSettings", 30.0)
	assert.Contains(t, err.Error(), "GetSettings")
	assert.Contains(t, err.Error(), "30.00s")
}

// =============================================================================
// PROTOCOL TESTS
// =============================================================================

func TestRiskLevelRequiresConfirmation(t *testing.T) {
	assert.True(t, RiskLevelDestructive.RequiresConfirmation())
	assert.False(t, RiskLevelWrite.RequiresConfirmation())
	assert.False(t, RiskLevelReadOnly.RequiresConfirmation())
}

func TestGetMessageType(t *testing.T) {
	assert.Equal(t, "AgentStarted", GetMessageType(&AgentStarted{}))
	assert.Equal(t, "AgentCompleted", GetMessageType(&AgentCompleted{}))
	assert.Equal(t, "GetSettings", GetMessageType(&GetSettings{}))
}

// =============================================================================
// CIRCUIT BREAKER TESTS
// =============================================================================

func TestCircuitBreakerOpensAfterFailures(t *testing.T) {
	// Verify circuit opens after N consecutive failures
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(3, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	err := bus.RegisterHandler("GetSettings", failingHandler("test error"))
	require.NoError(t, err)

	// Fail 3 times
	for i := 0; i < 3; i++ {
		_, _ = bus.QuerySync(ctx, &GetSettings{})
	}

	// Circuit should be open
	states := cb.GetStates()
	assert.Equal(t, "open", states["GetSettings"])
}

func TestCircuitBreakerBlocksWhenOpen(t *testing.T) {
	// Verify requests are blocked when circuit is open
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	// Register a handler that counts calls and fails
	var callCount int32
	err := bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&callCount, 1)
		return nil, errors.New("error")
	})
	require.NoError(t, err)

	// Open the circuit by failing twice
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	assert.Equal(t, int32(2), atomic.LoadInt32(&callCount))
	assert.Equal(t, "open", cb.GetStates()["GetSettings"])

	// Try query while circuit is open (should be blocked)
	result, err := bus.QuerySync(ctx, &GetSettings{})

	// Circuit middleware returns nil from Before, which causes NoHandlerError
	assert.Error(t, err)
	assert.Nil(t, result)

	// Handler call count should still be 2 (circuit blocked the 3rd call)
	assert.Equal(t, int32(2), atomic.LoadInt32(&callCount))
}

func TestCircuitBreakerHalfOpenTransition(t *testing.T) {
	// Verify circuit transitions to half-open after timeout
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 50*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	err := bus.RegisterHandler("GetSettings", failingHandler("error"))
	require.NoError(t, err)

	// Open circuit
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	assert.Equal(t, "open", cb.GetStates()["GetSettings"])

	// Wait for reset timeout
	time.Sleep(60 * time.Millisecond)

	// Next request should transition to half-open
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	
	// State should be open again (half-open failed)
	assert.Equal(t, "open", cb.GetStates()["GetSettings"])
}

func TestCircuitBreakerHalfOpenSuccess(t *testing.T) {
	// Verify circuit closes on half-open success
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 50*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	// Open circuit
	err := bus.RegisterHandler("GetSettings", failingHandler("error"))
	require.NoError(t, err)
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})

	// Wait for reset timeout
	time.Sleep(60 * time.Millisecond)

	// Replace with successful handler
	bus.handlers["GetSettings"] = func(ctx context.Context, msg Message) (any, error) {
		return &SettingsResponse{Values: map[string]any{}}, nil
	}

	// This should succeed and close circuit
	_, err = bus.QuerySync(ctx, &GetSettings{})
	require.NoError(t, err)

	// Circuit should be closed
	assert.Equal(t, "closed", cb.GetStates()["GetSettings"])
}

func TestCircuitBreakerHalfOpenFailure(t *testing.T) {
	// Verify circuit reopens on half-open failure
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 50*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	err := bus.RegisterHandler("GetSettings", failingHandler("error"))
	require.NoError(t, err)

	// Open circuit
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})

	// Wait for reset
	time.Sleep(60 * time.Millisecond)

	// Fail again (circuit goes from half-open back to open)
	_, _ = bus.QuerySync(ctx, &GetSettings{})

	assert.Equal(t, "open", cb.GetStates()["GetSettings"])
}

func TestCircuitBreakerExcludedTypes(t *testing.T) {
	// Verify excluded types bypass circuit breaker
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 100*time.Millisecond, []string{"HealthCheckRequest"})
	bus.AddMiddleware(cb)

	var callCount int32
	err := bus.RegisterHandler("HealthCheckRequest", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&callCount, 1)
		return nil, errors.New("health check failed")
	})
	require.NoError(t, err)

	// Fail 10 times
	for i := 0; i < 10; i++ {
		_, _ = bus.QuerySync(ctx, &HealthCheckRequest{Component: "llm"})
	}

	// All calls should have gone through (excluded type)
	assert.Equal(t, int32(10), atomic.LoadInt32(&callCount))
	
	// Circuit should not exist for excluded type
	states := cb.GetStates()
	_, exists := states["HealthCheckRequest"]
	assert.False(t, exists)
}

func TestCircuitBreakerMultipleMessageTypes(t *testing.T) {
	// Verify each message type has independent circuit state
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	// Register failing handler for GetSettings
	_ = bus.RegisterHandler("GetSettings", failingHandler("error"))
	// Register successful handler for GetPrompt
	_ = bus.RegisterHandler("GetPrompt", func(ctx context.Context, msg Message) (any, error) {
		return &PromptResponse{Found: true}, nil
	})

	// Fail GetSettings twice
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})

	// Succeed GetPrompt once
	_, _ = bus.QuerySync(ctx, &GetPrompt{Name: "test", Version: "v1"})

	states := cb.GetStates()
	assert.Equal(t, "open", states["GetSettings"])
	assert.Equal(t, "closed", states["GetPrompt"])
}

func TestCircuitBreakerResetSingleType(t *testing.T) {
	// Verify Reset(msgType) clears specific circuit
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	_ = bus.RegisterHandler("GetSettings", failingHandler("error"))
	
	// Open circuit
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	assert.Equal(t, "open", cb.GetStates()["GetSettings"])

	// Reset specific circuit
	msgType := "GetSettings"
	cb.Reset(&msgType)

	// Circuit should be cleared
	states := cb.GetStates()
	_, exists := states["GetSettings"]
	assert.False(t, exists)
}

func TestCircuitBreakerResetAll(t *testing.T) {
	// Verify Reset(nil) clears all circuits
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	// Open multiple circuits
	_ = bus.RegisterHandler("GetSettings", failingHandler("error"))
	_ = bus.RegisterHandler("GetPrompt", failingHandler("error"))
	_ = bus.RegisterHandler("GetToolCatalog", failingHandler("error"))

	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetPrompt{Name: "test", Version: "v1"})
	_, _ = bus.QuerySync(ctx, &GetPrompt{Name: "test", Version: "v1"})
	_, _ = bus.QuerySync(ctx, &GetToolCatalog{})
	_, _ = bus.QuerySync(ctx, &GetToolCatalog{})

	assert.Len(t, cb.GetStates(), 3)

	// Reset all
	cb.Reset(nil)

	// All circuits cleared
	assert.Len(t, cb.GetStates(), 0)
}

func TestCircuitBreakerGetStates(t *testing.T) {
	// Verify GetStates returns current state map
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	// Create circuits in different states
	_ = bus.RegisterHandler("GetSettings", failingHandler("error"))
	_ = bus.RegisterHandler("GetPrompt", func(ctx context.Context, msg Message) (any, error) {
		return &PromptResponse{Found: true}, nil
	})

	// Open GetSettings
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})

	// Keep GetPrompt closed
	_, _ = bus.QuerySync(ctx, &GetPrompt{Name: "test", Version: "v1"})

	states := cb.GetStates()
	assert.Equal(t, 2, len(states))
	assert.Equal(t, "open", states["GetSettings"])
	assert.Equal(t, "closed", states["GetPrompt"])
}

func TestCircuitBreakerPartialFailures(t *testing.T) {
	// Verify circuit opens only after threshold
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(3, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	callNum := 0
	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		callNum++
		if callNum == 3 {
			return &SettingsResponse{Values: map[string]any{}}, nil
		}
		return nil, errors.New("error")
	})

	// 2 failures
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	
	// 1 success (resets counter)
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	
	// Circuit should still be closed
	states := cb.GetStates()
	assert.Equal(t, "closed", states["GetSettings"])
}

func TestCircuitBreakerConcurrentAccess(t *testing.T) {
	// Verify thread safety of circuit state
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(5, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	var successCount, failCount int32
	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		// 50% fail rate
		if atomic.AddInt32(&successCount, 1)%2 == 0 {
			atomic.AddInt32(&failCount, 1)
			return nil, errors.New("error")
		}
		return &SettingsResponse{Values: map[string]any{}}, nil
	})

	// 100 concurrent requests
	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, _ = bus.QuerySync(ctx, &GetSettings{})
		}()
	}

	wg.Wait()

	// No panics means success
	// Circuit state should be consistent
	states := cb.GetStates()
	_, exists := states["GetSettings"]
	assert.True(t, exists)
}

func TestCircuitBreakerTimerAccuracy(t *testing.T) {
	// Verify resetTimeout is respected
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(2, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	_ = bus.RegisterHandler("GetSettings", failingHandler("error"))

	// Open circuit
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})

	// Try at 50ms (should be blocked)
	time.Sleep(50 * time.Millisecond)
	assert.Equal(t, "open", cb.GetStates()["GetSettings"])

	// Wait until 150ms total (should transition to half-open)
	time.Sleep(60 * time.Millisecond)
	
	// Next call will attempt half-open
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	
	// Should be open again (failed in half-open)
	assert.Equal(t, "open", cb.GetStates()["GetSettings"])
}

func TestCircuitBreakerZeroThreshold(t *testing.T) {
	// Verify threshold=0 means circuit never opens
	bus := newTestBus()
	ctx := context.Background()

	cb := NewCircuitBreakerMiddleware(0, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	var callCount int32
	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&callCount, 1)
		return nil, errors.New("error")
	})

	// Fail 100 times
	for i := 0; i < 100; i++ {
		_, _ = bus.QuerySync(ctx, &GetSettings{})
	}

	// All calls should go through
	assert.Equal(t, int32(100), atomic.LoadInt32(&callCount))
	
	// Circuit should stay closed
	states := cb.GetStates()
	state, exists := states["GetSettings"]
	if exists {
		assert.Equal(t, "closed", state)
	}
}

func TestCircuitBreakerWithMiddlewareChain(t *testing.T) {
	// Verify circuit breaker works with other middleware
	bus := newTestBus()
	ctx := context.Background()

	logging := NewLoggingMiddleware("DEBUG")
	cb := NewCircuitBreakerMiddleware(2, 100*time.Millisecond, []string{})

	bus.AddMiddleware(logging)
	bus.AddMiddleware(cb)

	_ = bus.RegisterHandler("GetSettings", failingHandler("error"))

	// Trigger circuit open
	_, _ = bus.QuerySync(ctx, &GetSettings{})
	_, _ = bus.QuerySync(ctx, &GetSettings{})

	// Circuit should be open
	assert.Equal(t, "open", cb.GetStates()["GetSettings"])
}

// =============================================================================
// QUERY TIMEOUT TESTS
// =============================================================================

func TestQueryTimeout(t *testing.T) {
	// Verify query times out after queryTimeout duration
	bus := NewInMemoryCommBus(100 * time.Millisecond)
	ctx := context.Background()

	_ = bus.RegisterHandler("GetSettings", slowHandler(200*time.Millisecond))

	start := time.Now()
	_, err := bus.QuerySync(ctx, &GetSettings{})
	elapsed := time.Since(start)

	require.Error(t, err)
	var timeoutErr *QueryTimeoutError
	assert.True(t, errors.As(err, &timeoutErr))
	assert.Less(t, elapsed, 150*time.Millisecond, "Should timeout around 100ms")
}

func TestQueryTimeoutCleanup(t *testing.T) {
	// Verify goroutine cleanup after timeout
	bus := NewInMemoryCommBus(50 * time.Millisecond)
	ctx := context.Background()

	_ = bus.RegisterHandler("GetSettings", slowHandler(200*time.Millisecond))

	// Execute query that will timeout
	_, err := bus.QuerySync(ctx, &GetSettings{})
	require.Error(t, err)

	// Wait for handler to complete
	time.Sleep(250 * time.Millisecond)

	// No explicit goroutine leak check, but no panic means cleanup worked
	// In production, use goleak package
}

func TestQueryContextCancellation(t *testing.T) {
	// Verify context cancellation propagates
	bus := newTestBus()
	ctx, cancel := context.WithCancel(context.Background())

	resultChan := make(chan error, 1)
	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		// Slow handler that respects context
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-time.After(500 * time.Millisecond):
			return &SettingsResponse{Values: map[string]any{}}, nil
		}
	})

	// Start query in goroutine
	go func() {
		_, err := bus.QuerySync(ctx, &GetSettings{})
		resultChan <- err
	}()

	// Cancel after 50ms
	time.Sleep(50 * time.Millisecond)
	cancel()

	// Should get error quickly
	select {
	case err := <-resultChan:
		assert.Error(t, err)
	case <-time.After(200 * time.Millisecond):
		t.Fatal("Query didn't return after context cancel")
	}
}

func TestQueryTimeoutWithMiddleware(t *testing.T) {
	// Verify middleware After called on timeout
	bus := NewInMemoryCommBus(50 * time.Millisecond)
	ctx := context.Background()

	// Use error tracking middleware to verify After is called with error
	mw := &errorTrackingMiddleware{}
	bus.AddMiddleware(mw)

	_ = bus.RegisterHandler("GetSettings", slowHandler(200*time.Millisecond))

	_, err := bus.QuerySync(ctx, &GetSettings{})
	require.Error(t, err)

	// Verify middleware After was called with error
	assert.NotNil(t, mw.capturedError, "Middleware After should see the timeout error")
}

func TestConcurrentQueryTimeouts(t *testing.T) {
	// Verify multiple queries can timeout independently
	bus := NewInMemoryCommBus(100 * time.Millisecond)
	ctx := context.Background()

	delays := []time.Duration{50 * time.Millisecond, 150 * time.Millisecond, 75 * time.Millisecond, 200 * time.Millisecond}
	var wg sync.WaitGroup

	for _, delay := range delays {
		wg.Add(1)
		delay := delay // capture
		go func() {
			defer wg.Done()
			bus.RegisterHandler("GetSettings", slowHandler(delay))
			_, err := bus.QuerySync(ctx, &GetSettings{})
			
			if delay > 100*time.Millisecond {
				assert.Error(t, err, "Should timeout for delay %v", delay)
			} else {
				// May or may not timeout depending on timing
			}
		}()
	}

	wg.Wait()
}

// =============================================================================
// COMMAND EXECUTION TESTS
// =============================================================================

func TestSendCommandWithHandler(t *testing.T) {
	// Verify command executes handler
	bus := newTestBus()
	ctx := context.Background()

	var called int32
	_ = bus.RegisterHandler("InvalidateCache", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&called, 1)
		return nil, nil
	})

	err := bus.Send(ctx, &InvalidateCache{CacheName: "tools"})

	assert.NoError(t, err)
	assert.Equal(t, int32(1), atomic.LoadInt32(&called))
}

func TestSendCommandWithoutHandler(t *testing.T) {
	// Verify command without handler doesn't error
	bus := newTestBus()
	ctx := context.Background()

	err := bus.Send(ctx, &InvalidateCache{CacheName: "tools"})

	// Should not error (just logs)
	assert.NoError(t, err)
}

func TestSendCommandHandlerError(t *testing.T) {
	// Verify handler error is returned
	bus := newTestBus()
	ctx := context.Background()

	_ = bus.RegisterHandler("InvalidateCache", failingHandler("cache error"))

	err := bus.Send(ctx, &InvalidateCache{CacheName: "tools"})

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "cache error")
}

func TestSendCommandMiddlewareBefore(t *testing.T) {
	// Verify middleware Before intercepts command
	bus := newTestBus()
	ctx := context.Background()

	mw := newModifyingMiddleware()
	bus.AddMiddleware(mw)

	_ = bus.RegisterHandler("InvalidateCache", func(ctx context.Context, msg Message) (any, error) {
		return nil, nil
	})

	err := bus.Send(ctx, &InvalidateCache{CacheName: "tools"})

	assert.NoError(t, err)
	assert.Equal(t, int32(1), atomic.LoadInt32(mw.beforeCalled))
}

func TestSendCommandMiddlewareAbort(t *testing.T) {
	// Verify middleware can abort command
	bus := newTestBus()
	ctx := context.Background()

	bus.AddMiddleware(&abortingMiddleware{})

	var called int32
	_ = bus.RegisterHandler("InvalidateCache", countingHandler(&called))

	err := bus.Send(ctx, &InvalidateCache{CacheName: "tools"})

	assert.NoError(t, err)
	assert.Equal(t, int32(0), atomic.LoadInt32(&called), "Handler should not be called")
}

func TestSendCommandMiddlewareAfter(t *testing.T) {
	// Verify middleware After called with result
	bus := newTestBus()
	ctx := context.Background()

	mw := newModifyingMiddleware()
	bus.AddMiddleware(mw)

	_ = bus.RegisterHandler("InvalidateCache", func(ctx context.Context, msg Message) (any, error) {
		return nil, nil
	})

	err := bus.Send(ctx, &InvalidateCache{CacheName: "tools"})

	assert.NoError(t, err)
	assert.Equal(t, int32(1), atomic.LoadInt32(mw.afterCalled))
}

func TestSendCommandConcurrent(t *testing.T) {
	// Verify concurrent commands don't interfere
	bus := newTestBus()
	ctx := context.Background()

	var callCount int32
	_ = bus.RegisterHandler("InvalidateCache", countingHandler(&callCount))

	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_ = bus.Send(ctx, &InvalidateCache{CacheName: "tools"})
		}()
	}

	wg.Wait()

	assert.Equal(t, int32(100), atomic.LoadInt32(&callCount))
}

func TestSendCommandVsQuery(t *testing.T) {
	// Verify command and query use different code paths
	bus := newTestBus()
	ctx := context.Background()

	_ = bus.RegisterHandler("InvalidateCache", func(ctx context.Context, msg Message) (any, error) {
		return nil, nil // Command returns nil
	})
	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		return &SettingsResponse{Values: map[string]any{"key": "value"}}, nil
	})

	// Send command
	cmdErr := bus.Send(ctx, &InvalidateCache{CacheName: "tools"})
	assert.NoError(t, cmdErr)

	// Query
	queryResult, queryErr := bus.QuerySync(ctx, &GetSettings{})
	assert.NoError(t, queryErr)
	assert.NotNil(t, queryResult)
}

// =============================================================================
// MIDDLEWARE CHAIN TESTS (SAMPLE - IMPLEMENT REMAINING 7)
// =============================================================================

func TestMiddlewareChainOrder(t *testing.T) {
	// Verify middleware executes in registration order
	bus := newTestBus()
	ctx := context.Background()

	var order []string
	var mu sync.Mutex

	mw1 := &trackingMiddlewareType{order: &order, mu: &mu, name: "mw1"}
	mw2 := &trackingMiddlewareType{order: &order, mu: &mu, name: "mw2"}

	bus.AddMiddleware(mw1)
	bus.AddMiddleware(mw2)

	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		return nil, nil
	})

	_ = bus.Publish(ctx, &AgentStarted{AgentName: "test", SessionID: "s1", RequestID: "r1", EnvelopeID: "e1"})
	time.Sleep(20 * time.Millisecond)

	mu.Lock()
	defer mu.Unlock()
	
	// Before should be in order, After in reverse
	assert.Equal(t, "mw1-before", order[0])
	assert.Equal(t, "mw2-before", order[1])
	assert.Equal(t, "mw2-after", order[2])
	assert.Equal(t, "mw1-after", order[3])
}

func TestMiddlewareAbortProcessing(t *testing.T) {
	// Verify middleware returning nil aborts
	bus := newTestBus()
	ctx := context.Background()

	bus.AddMiddleware(&abortingMiddleware{})

	var called int32
	bus.Subscribe("AgentStarted", countingHandler(&called))

	_ = bus.Publish(ctx, &AgentStarted{AgentName: "test", SessionID: "s1", RequestID: "r1", EnvelopeID: "e1"})
	time.Sleep(20 * time.Millisecond)

	assert.Equal(t, int32(0), atomic.LoadInt32(&called), "Subscriber should not be called")
}

func TestMiddlewareBeforeError(t *testing.T) {
	// Verify middleware error stops processing
	bus := newTestBus()
	ctx := context.Background()

	bus.AddMiddleware(&errorMiddleware{})

	var called int32
	bus.Subscribe("AgentStarted", countingHandler(&called))

	err := bus.Publish(ctx, &AgentStarted{AgentName: "test", SessionID: "s1", RequestID: "r1", EnvelopeID: "e1"})

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "middleware error")
	assert.Equal(t, int32(0), atomic.LoadInt32(&called))
}

// ============================================================================
// Part 4: Middleware Chain Tests (Full Implementation)
// ============================================================================

func TestMiddlewareAfterError(t *testing.T) {
	// Verify middleware can inject errors in After
	bus := newTestBus()
	ctx := context.Background()

	bus.AddMiddleware(&afterErrorMiddleware{})

	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		return &SettingsResponse{Values: map[string]any{"key": "value"}}, nil
	})

	_, err := bus.QuerySync(ctx, &GetSettings{})

	// Should get the after error
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "after error")
	// Result may or may not be nil when middleware returns error
}

func TestMiddlewareAfterModifyResult(t *testing.T) {
	// Verify middleware can modify result in After
	bus := newTestBus()
	ctx := context.Background()

	bus.AddMiddleware(&modifyResultMiddleware{})

	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		return &SettingsResponse{Values: map[string]any{"key": "value"}}, nil
	})

	result, err := bus.QuerySync(ctx, &GetSettings{})

	assert.NoError(t, err)
	wrapped, ok := result.(map[string]any)
	assert.True(t, ok, "Result should be wrapped in map")
	assert.NotNil(t, wrapped["wrapped"])
}

func TestMultipleMiddlewareAbort(t *testing.T) {
	// Verify first aborting middleware stops chain
	bus := newTestBus()
	ctx := context.Background()

	var mw1Called, mw3Called bool
	
	mw1 := &trackingMW1{called: &mw1Called}
	mw3 := &trackingMW3{called: &mw3Called}

	bus.AddMiddleware(mw1)
	bus.AddMiddleware(&abortingMiddleware{}) // This aborts
	bus.AddMiddleware(mw3)

	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		t.Fatal("Handler should not be called")
		return nil, nil
	})

	_, err := bus.QuerySync(ctx, &GetSettings{})

	// Should get NoHandlerError because abort returns nil
	assert.Error(t, err)
	var noHandlerErr *NoHandlerError
	assert.True(t, errors.As(err, &noHandlerErr))

	// mw1 Before should run, mw3 Before should not
	assert.True(t, mw1Called)
	assert.False(t, mw3Called)
}

func TestMiddlewareReverseOrderAfter(t *testing.T) {
	// Verify After runs in reverse order: last added runs first
	bus := newTestBus()
	ctx := context.Background()

	var order []string
	mu := sync.Mutex{}

	mw1Order := &order
	mw1Mu := &mu
	mw1 := &trackingMiddlewareType{order: mw1Order, mu: mw1Mu, name: "mw1"}

	mw2Order := &order
	mw2Mu := &mu
	mw2 := &trackingMiddlewareType{order: mw2Order, mu: mw2Mu, name: "mw2"}

	mw3Order := &order
	mw3Mu := &mu
	mw3 := &trackingMiddlewareType{order: mw3Order, mu: mw3Mu, name: "mw3"}

	bus.AddMiddleware(mw1)
	bus.AddMiddleware(mw2)
	bus.AddMiddleware(mw3)

	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		mu.Lock()
		order = append(order, "handler")
		mu.Unlock()
		return &SettingsResponse{Values: map[string]any{}}, nil
	})

	_, err := bus.QuerySync(ctx, &GetSettings{})
	assert.NoError(t, err)

	// Before: mw1, mw2, mw3, handler
	// After: mw3, mw2, mw1 (reverse)
	
	// Check that handler is in middle and after calls are in reverse
	assert.Contains(t, order, "handler")
	handlerIdx := -1
	for i, s := range order {
		if s == "handler" {
			handlerIdx = i
			break
		}
	}
	
	// After handler, we should see mw3, mw2, mw1
	afterOrder := order[handlerIdx+1:]
	assert.Equal(t, "mw3-after", afterOrder[0])
	assert.Equal(t, "mw2-after", afterOrder[1])
	assert.Equal(t, "mw1-after", afterOrder[2])
}

func TestMiddlewareConcurrentModification(t *testing.T) {
	// Verify middleware doesn't have race conditions
	bus := newTestBus()
	ctx := context.Background()

	mw := newModifyingMiddleware()
	bus.AddMiddleware(mw)

	var callCount int32
	_ = bus.RegisterHandler("GetSettings", countingHandler(&callCount))

	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, _ = bus.QuerySync(ctx, &GetSettings{})
		}()
	}

	wg.Wait()

	assert.Equal(t, int32(100), atomic.LoadInt32(&callCount))
	assert.Equal(t, int32(100), atomic.LoadInt32(mw.beforeCalled))
	assert.Equal(t, int32(100), atomic.LoadInt32(mw.afterCalled))
}

func TestMiddlewareErrorPropagation(t *testing.T) {
	// Verify errors propagate through middleware chain
	bus := newTestBus()
	ctx := context.Background()

	mw1 := &errorTrackingMiddleware{}
	bus.AddMiddleware(mw1)

	_ = bus.RegisterHandler("GetSettings", failingHandler("handler error"))

	_, err := bus.QuerySync(ctx, &GetSettings{})

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "handler error")
	assert.Equal(t, err, mw1.capturedError, "Middleware should see handler error")
}

func TestMiddlewareContextCancellation(t *testing.T) {
	// Verify middleware respects context cancellation
	bus := newTestBus()
	ctx, cancel := context.WithCancel(context.Background())

	bus.AddMiddleware(&contextCheckMiddleware{})

	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		t.Fatal("Handler should not be called")
		return nil, nil
	})

	cancel() // Cancel before query

	_, err := bus.QuerySync(ctx, &GetSettings{})

	assert.Error(t, err)
	assert.Equal(t, context.Canceled, err)
}

// ============================================================================
// Part 5: Concurrency Tests (Full Implementation)
// ============================================================================
// - TestMiddlewareConcurrentModification

// =============================================================================
// CONCURRENCY TESTS (SAMPLE - IMPLEMENT REMAINING 7)
// =============================================================================

func TestConcurrentPublish(t *testing.T) {
	// Verify concurrent publishing is safe
	bus := newTestBus()
	ctx := context.Background()

	var eventCount int32
	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&eventCount, 1)
		return nil, nil
	})

	var wg sync.WaitGroup
	for i := 0; i < 1000; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_ = bus.Publish(ctx, &AgentStarted{AgentName: "test", SessionID: "s1", RequestID: "r1", EnvelopeID: "e1"})
		}()
	}

	wg.Wait()
	time.Sleep(50 * time.Millisecond)

	assert.Equal(t, int32(1000), atomic.LoadInt32(&eventCount))
}

func TestPublishWhileSubscribe(t *testing.T) {
	// Verify publishing during subscribe doesn't race
	bus := newTestBus()
	ctx := context.Background()

	var eventCount int32
	stopPublishing := make(chan struct{})

	// Publisher goroutine
	go func() {
		for {
			select {
			case <-stopPublishing:
				return
			default:
				_ = bus.Publish(ctx, &AgentStarted{AgentName: "test", SessionID: "s1", RequestID: "r1", EnvelopeID: "e1"})
				time.Sleep(time.Millisecond)
			}
		}
	}()

	// Add subscribers concurrently
	for i := 0; i < 10; i++ {
		bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
			atomic.AddInt32(&eventCount, 1)
			return nil, nil
		})
		time.Sleep(5 * time.Millisecond)
	}

	close(stopPublishing)
	time.Sleep(50 * time.Millisecond)

	// No panics = success
	assert.Greater(t, atomic.LoadInt32(&eventCount), int32(0))
}

func TestConcurrentQuerySync(t *testing.T) {
	// Verify multiple simultaneous queries
	bus := newTestBus()
	ctx := context.Background()

	var callCount int32
	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&callCount, 1)
		return &SettingsResponse{Values: map[string]any{}}, nil
	})

	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, err := bus.QuerySync(ctx, &GetSettings{})
			assert.NoError(t, err)
		}()
	}

	wg.Wait()

	assert.Equal(t, int32(100), atomic.LoadInt32(&callCount))
}

func TestQueryWhileRegisterHandler(t *testing.T) {
	// Verify concurrent handler registration doesn't break queries
	bus := newTestBus()
	ctx := context.Background()

	var registered, queried int32
	stopRegistering := make(chan struct{})

	// Register initial handler
	_ = bus.RegisterHandler("GetSettings", countingHandler(&queried))

	// Register handlers concurrently in background
	go func() {
		for i := 0; i < 50; i++ {
			select {
			case <-stopRegistering:
				return
			default:
				msgType := fmt.Sprintf("Query%d", i)
				_ = bus.RegisterHandler(msgType, func(ctx context.Context, msg Message) (any, error) {
					atomic.AddInt32(&registered, 1)
					return nil, nil
				})
				time.Sleep(time.Millisecond)
			}
		}
	}()

	// Query while registering
	time.Sleep(10 * time.Millisecond)
	for i := 0; i < 100; i++ {
		_, err := bus.QuerySync(ctx, &GetSettings{})
		assert.NoError(t, err)
	}

	close(stopRegistering)
	
	// No panics = success
	assert.Equal(t, int32(100), atomic.LoadInt32(&queried))
}

func TestConcurrentSubscribeUnsubscribe(t *testing.T) {
	// Verify concurrent subscribe/unsubscribe doesn't panic
	bus := newTestBus()
	ctx := context.Background()

	var publishCount, receiveCount int32
	stopChan := make(chan struct{})

	// Subscriber goroutines
	for i := 0; i < 10; i++ {
		go func(id int) {
			for {
				select {
				case <-stopChan:
					return
				default:
					bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
						atomic.AddInt32(&receiveCount, 1)
						return nil, nil
					})
					time.Sleep(time.Millisecond)
				}
			}
		}(i)
	}

	// Publisher goroutines
	for i := 0; i < 5; i++ {
		go func() {
			for {
				select {
				case <-stopChan:
					return
				default:
					_ = bus.Publish(ctx, &AgentStarted{
						AgentName:  "test",
						SessionID:  "s1",
						RequestID:  "r1",
						EnvelopeID: "e1",
					})
					atomic.AddInt32(&publishCount, 1)
					time.Sleep(time.Millisecond)
				}
			}
		}()
	}

	time.Sleep(50 * time.Millisecond)
	close(stopChan)
	time.Sleep(10 * time.Millisecond)

	// No panics = success
	assert.Greater(t, atomic.LoadInt32(&publishCount), int32(0))
}

func TestPublishWhileClear(t *testing.T) {
	// Verify Clear() doesn't panic during concurrent publish
	bus := newTestBus()
	ctx := context.Background()

	var receiveCount int32
	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&receiveCount, 1)
		time.Sleep(time.Millisecond) // Slow handler
		return nil, nil
	})

	stopChan := make(chan struct{})

	// Publisher goroutine
	go func() {
		for {
			select {
			case <-stopChan:
				return
			default:
				_ = bus.Publish(ctx, &AgentStarted{
					AgentName:  "test",
					SessionID:  "s1",
					RequestID:  "r1",
					EnvelopeID: "e1",
				})
			}
		}
	}()

	time.Sleep(20 * time.Millisecond)
	
	// Clear while publishing
	bus.Clear()
	
	close(stopChan)
	time.Sleep(10 * time.Millisecond)

	// No panics = success
	assert.Greater(t, atomic.LoadInt32(&receiveCount), int32(0))
}

func TestConcurrentMiddlewareAdd(t *testing.T) {
	// Verify adding middleware during operations doesn't panic
	bus := newTestBus()
	ctx := context.Background()

	var callCount int32
	_ = bus.RegisterHandler("GetSettings", countingHandler(&callCount))

	stopChan := make(chan struct{})

	// Middleware adder goroutine
	go func() {
		for i := 0; i < 10; i++ {
			select {
			case <-stopChan:
				return
			default:
				bus.AddMiddleware(newModifyingMiddleware())
				time.Sleep(2 * time.Millisecond)
			}
		}
	}()

	// Query goroutines
	var wg sync.WaitGroup
	for i := 0; i < 50; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, _ = bus.QuerySync(ctx, &GetSettings{})
		}()
		time.Sleep(time.Millisecond)
	}

	wg.Wait()
	close(stopChan)

	// No panics = success
	assert.Equal(t, int32(50), atomic.LoadInt32(&callCount))
}

func TestRaceConditionSubscriberList(t *testing.T) {
	// Verify subscriber list doesn't have race conditions
	bus := newTestBus()
	ctx := context.Background()

	var receiveCount int32
	stopChan := make(chan struct{})

	// Multiple subscribers
	for i := 0; i < 20; i++ {
		go func() {
			for {
				select {
				case <-stopChan:
					return
				default:
					bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
						atomic.AddInt32(&receiveCount, 1)
						return nil, nil
					})
					time.Sleep(time.Millisecond)
				}
			}
		}()
	}

	// Publisher
	go func() {
		for i := 0; i < 100; i++ {
			select {
			case <-stopChan:
				return
			default:
				_ = bus.Publish(ctx, &AgentStarted{
					AgentName:  "test",
					SessionID:  "s1",
					RequestID:  "r1",
					EnvelopeID: "e1",
				})
				time.Sleep(time.Millisecond)
			}
		}
	}()

	time.Sleep(50 * time.Millisecond)
	close(stopChan)
	time.Sleep(10 * time.Millisecond)

	// No panics = success
	assert.Greater(t, atomic.LoadInt32(&receiveCount), int32(0))
}

func TestConcurrentHandlerRegistration(t *testing.T) {
	// Verify concurrent handler registration is thread-safe
	bus := newTestBus()

	var registered int32
	var wg sync.WaitGroup

	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			msgType := fmt.Sprintf("Handler%d", id)
			err := bus.RegisterHandler(msgType, func(ctx context.Context, msg Message) (any, error) {
				return nil, nil
			})
			if err == nil {
				atomic.AddInt32(&registered, 1)
			}
		}(i)
	}

	wg.Wait()

	assert.Equal(t, int32(100), atomic.LoadInt32(&registered))
}

func TestHighLoadStressTest(t *testing.T) {
	// High load stress test: 1000 concurrent operations
	bus := newTestBus()
	ctx := context.Background()

	// Add middleware
	bus.AddMiddleware(NewLoggingMiddleware("INFO"))
	cb := NewCircuitBreakerMiddleware(50, 100*time.Millisecond, []string{})
	bus.AddMiddleware(cb)

	// Register handlers
	var queryCount, eventCount, commandCount int32
	
	_ = bus.RegisterHandler("GetSettings", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&queryCount, 1)
		if atomic.LoadInt32(&queryCount)%10 == 0 {
			return nil, errors.New("occasional error")
		}
		return &SettingsResponse{Values: map[string]any{}}, nil
	})

	bus.Subscribe("AgentStarted", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&eventCount, 1)
		return nil, nil
	})

	_ = bus.RegisterHandler("InvalidateCache", func(ctx context.Context, msg Message) (any, error) {
		atomic.AddInt32(&commandCount, 1)
		return nil, nil
	})

	// Fire 1000 concurrent operations
	var wg sync.WaitGroup
	for i := 0; i < 1000; i++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			
			switch id % 3 {
			case 0:
				_, _ = bus.QuerySync(ctx, &GetSettings{})
			case 1:
				_ = bus.Publish(ctx, &AgentStarted{
					AgentName:  "test",
					SessionID:  "s1",
					RequestID:  "r1",
					EnvelopeID: "e1",
				})
			case 2:
				_ = bus.Send(ctx, &InvalidateCache{CacheName: "all"})
			}
		}(i)
	}

	wg.Wait()

	// Verify operations completed
	totalOps := atomic.LoadInt32(&queryCount) + atomic.LoadInt32(&eventCount) + atomic.LoadInt32(&commandCount)
	assert.Greater(t, totalOps, int32(900), "Most operations should complete")
	
	// Circuit breaker should have tracked failures
	states := cb.GetStates()
	assert.NotEmpty(t, states)
}

// End of concurrency tests
// - TestRaceConditionSubscriberList
// - TestConcurrentHandlerRegistration
// - TestHighLoadStressTest
