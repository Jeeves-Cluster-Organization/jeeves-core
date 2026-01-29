package grpc

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/commbus"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"google.golang.org/grpc/metadata"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

// createCommBusTestServer creates a test server with a real InMemoryCommBus
func createCommBusTestServer() (*CommBusServer, *MockLogger, commbus.CommBus) {
	logger := &MockLogger{}
	bus := commbus.NewInMemoryCommBus(30 * time.Second)
	server := NewCommBusServer(logger, bus)
	return server, logger, bus
}

// mockStreamServer implements pb.CommBusService_SubscribeServer for testing
type mockStreamServer struct {
	ctx      context.Context
	sendChan chan *pb.CommBusEvent
	sendErr  error
}

func newMockStreamServer(ctx context.Context) *mockStreamServer {
	return &mockStreamServer{
		ctx:      ctx,
		sendChan: make(chan *pb.CommBusEvent, 100),
	}
}

func (m *mockStreamServer) Send(event *pb.CommBusEvent) error {
	if m.sendErr != nil {
		return m.sendErr
	}
	m.sendChan <- event
	return nil
}

func (m *mockStreamServer) Context() context.Context {
	return m.ctx
}

// gRPC ServerStream interface methods - not used in tests but required
func (m *mockStreamServer) SendHeader(md metadata.MD) error { return nil }
func (m *mockStreamServer) SetTrailer(md metadata.MD)       {}
func (m *mockStreamServer) SendMsg(msg interface{}) error   { return nil }
func (m *mockStreamServer) RecvMsg(msg interface{}) error   { return nil }
func (m *mockStreamServer) SetHeader(md metadata.MD) error  { return nil }

// =============================================================================
// GENERIC MESSAGE TESTS
// =============================================================================

func TestGenericMessage_Category(t *testing.T) {
	tests := []struct {
		name     string
		category string
	}{
		{"event category", "event"},
		{"command category", "command"},
		{"query category", "query"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			msg := &GenericMessage{
				messageType: "test",
				category:    tt.category,
				payload:     []byte(`{}`),
			}
			assert.Equal(t, tt.category, msg.Category())
		})
	}
}

func TestGenericQuery_IsQuery(t *testing.T) {
	query := &GenericQuery{
		GenericMessage: GenericMessage{
			messageType: "test",
			category:    "query",
			payload:     []byte(`{}`),
		},
	}
	// Should not panic
	query.IsQuery()
}

// =============================================================================
// PUBLISH TESTS
// =============================================================================

func TestCommBusServer_Publish_Success(t *testing.T) {
	server, logger, bus := createCommBusTestServer()
	ctx := context.Background()

	// Register a subscriber to verify event is published
	var receivedEvent commbus.Message
	var wg sync.WaitGroup
	wg.Add(1)

	bus.Subscribe("TestEvent", func(ctx context.Context, msg commbus.Message) (any, error) {
		receivedEvent = msg
		wg.Done()
		return nil, nil
	})

	req := &pb.CommBusPublishRequest{
		EventType: "TestEvent",
		Payload:   []byte(`{"key":"value"}`),
	}

	resp, err := server.Publish(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.Success)
	assert.Empty(t, resp.Error)

	// Wait for event to be received
	wg.Wait()
	assert.NotNil(t, receivedEvent)
	assert.Equal(t, "event", receivedEvent.Category())

	// Verify logger was called
	assert.Contains(t, logger.debugCalls, "commbus_publish_success")
}

func TestCommBusServer_Publish_EmptyEventType(t *testing.T) {
	server, _, _ := createCommBusTestServer()
	ctx := context.Background()

	req := &pb.CommBusPublishRequest{
		EventType: "",
		Payload:   []byte(`{}`),
	}

	resp, err := server.Publish(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "event_type is required")
}

func TestCommBusServer_Publish_WithNilPayload(t *testing.T) {
	server, logger, _ := createCommBusTestServer()
	ctx := context.Background()

	req := &pb.CommBusPublishRequest{
		EventType: "TestEvent",
		Payload:   nil,
	}

	resp, err := server.Publish(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.Success)
	assert.Contains(t, logger.debugCalls, "commbus_publish_success")
}

func TestCommBusServer_Publish_NotifiesGRPCSubscribers(t *testing.T) {
	server, _, _ := createCommBusTestServer()
	ctx := context.Background()

	// Create mock stream server
	streamCtx, cancel := context.WithCancel(context.Background())
	defer cancel()
	stream := newMockStreamServer(streamCtx)

	// Start subscription in background
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		subscribeReq := &pb.CommBusSubscribeRequest{
			EventTypes: []string{"TestEvent"},
		}
		_ = server.Subscribe(subscribeReq, stream)
	}()

	// Give subscription time to register
	time.Sleep(50 * time.Millisecond)

	// Publish event
	req := &pb.CommBusPublishRequest{
		EventType: "TestEvent",
		Payload:   []byte(`{"data":"test"}`),
	}
	resp, err := server.Publish(ctx, req)

	require.NoError(t, err)
	require.True(t, resp.Success)

	// Wait for event to arrive
	select {
	case event := <-stream.sendChan:
		assert.Equal(t, "TestEvent", event.EventType)
		assert.Equal(t, []byte(`{"data":"test"}`), event.Payload)
		assert.Greater(t, event.TimestampMs, int64(0))
	case <-time.After(1 * time.Second):
		t.Fatal("Timeout waiting for event")
	}

	// Cleanup
	cancel()
	wg.Wait()
}

// =============================================================================
// SEND TESTS
// =============================================================================

func TestCommBusServer_Send_Success(t *testing.T) {
	server, logger, bus := createCommBusTestServer()
	ctx := context.Background()

	// Register a command handler
	var receivedCommand commbus.Message
	bus.RegisterHandler("TestCommand", func(ctx context.Context, msg commbus.Message) (any, error) {
		receivedCommand = msg
		return nil, nil
	})

	req := &pb.CommBusSendRequest{
		CommandType: "TestCommand",
		Payload:     []byte(`{"action":"test"}`),
	}

	resp, err := server.Send(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.Success)
	assert.Empty(t, resp.Error)

	// Wait a bit for async delivery
	time.Sleep(50 * time.Millisecond)
	assert.NotNil(t, receivedCommand)
	assert.Equal(t, "command", receivedCommand.Category())

	// Verify logger was called
	assert.Contains(t, logger.debugCalls, "commbus_send_success")
}

func TestCommBusServer_Send_EmptyCommandType(t *testing.T) {
	server, _, _ := createCommBusTestServer()
	ctx := context.Background()

	req := &pb.CommBusSendRequest{
		CommandType: "",
		Payload:     []byte(`{}`),
	}

	resp, err := server.Send(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "command_type is required")
}

func TestCommBusServer_Send_NoHandler(t *testing.T) {
	server, _, _ := createCommBusTestServer()
	ctx := context.Background()

	req := &pb.CommBusSendRequest{
		CommandType: "NonExistentCommand",
		Payload:     []byte(`{}`),
	}

	resp, err := server.Send(ctx, req)

	// Fire-and-forget: Should succeed even without handler
	// The commbus logs "no_handler_for_command" but doesn't error
	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.Success)
}

// =============================================================================
// QUERY TESTS
// =============================================================================

func TestCommBusServer_Query_Success(t *testing.T) {
	server, logger, bus := createCommBusTestServer()
	ctx := context.Background()

	// Register a query handler
	bus.RegisterHandler("TestQuery", func(ctx context.Context, msg commbus.Message) (any, error) {
		return map[string]string{"result": "success"}, nil
	})

	req := &pb.CommBusQueryRequest{
		QueryType: "TestQuery",
		Payload:   []byte(`{"param":"value"}`),
		TimeoutMs: 1000,
	}

	resp, err := server.Query(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.Success)
	assert.Empty(t, resp.Error)
	assert.NotEmpty(t, resp.Result)

	// Verify result is valid JSON
	assert.Contains(t, string(resp.Result), "result")
	assert.Contains(t, string(resp.Result), "success")

	// Verify logger was called
	assert.Contains(t, logger.debugCalls, "commbus_query_success")
}

func TestCommBusServer_Query_EmptyQueryType(t *testing.T) {
	server, _, _ := createCommBusTestServer()
	ctx := context.Background()

	req := &pb.CommBusQueryRequest{
		QueryType: "",
		Payload:   []byte(`{}`),
	}

	resp, err := server.Query(ctx, req)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "query_type is required")
}

func TestCommBusServer_Query_NoHandler(t *testing.T) {
	server, logger, _ := createCommBusTestServer()
	ctx := context.Background()

	req := &pb.CommBusQueryRequest{
		QueryType: "NonExistentQuery",
		Payload:   []byte(`{}`),
		TimeoutMs: 1000,
	}

	resp, err := server.Query(ctx, req)

	// Should return success=false with error message
	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.False(t, resp.Success)
	assert.NotEmpty(t, resp.Error)

	// Verify error was logged
	assert.Contains(t, logger.errorCalls, "commbus_query_failed")
}

func TestCommBusServer_Query_Timeout(t *testing.T) {
	server, logger, bus := createCommBusTestServer()
	ctx := context.Background()

	// Register a slow query handler
	bus.RegisterHandler("SlowQuery", func(ctx context.Context, msg commbus.Message) (any, error) {
		time.Sleep(500 * time.Millisecond)
		return "result", nil
	})

	req := &pb.CommBusQueryRequest{
		QueryType: "SlowQuery",
		Payload:   []byte(`{}`),
		TimeoutMs: 50, // Very short timeout
	}

	resp, err := server.Query(ctx, req)

	// Should return error due to timeout
	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.False(t, resp.Success)
	assert.Contains(t, resp.Error, "timed out")

	// Verify error was logged
	assert.Contains(t, logger.errorCalls, "commbus_query_failed")
}

func TestCommBusServer_Query_HandlerError(t *testing.T) {
	server, logger, bus := createCommBusTestServer()
	ctx := context.Background()

	// Register a failing query handler
	bus.RegisterHandler("FailingQuery", func(ctx context.Context, msg commbus.Message) (any, error) {
		return nil, errors.New("handler failed")
	})

	req := &pb.CommBusQueryRequest{
		QueryType: "FailingQuery",
		Payload:   []byte(`{}`),
		TimeoutMs: 1000,
	}

	resp, err := server.Query(ctx, req)

	// Should return success=false with error message
	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.False(t, resp.Success)
	assert.Contains(t, resp.Error, "handler failed")

	// Verify error was logged
	assert.Contains(t, logger.errorCalls, "commbus_query_failed")
}

func TestCommBusServer_Query_NoTimeout(t *testing.T) {
	server, logger, bus := createCommBusTestServer()
	ctx := context.Background()

	// Register a query handler
	bus.RegisterHandler("TestQuery", func(ctx context.Context, msg commbus.Message) (any, error) {
		return "result", nil
	})

	req := &pb.CommBusQueryRequest{
		QueryType: "TestQuery",
		Payload:   []byte(`{}`),
		TimeoutMs: 0, // No timeout specified
	}

	resp, err := server.Query(ctx, req)

	require.NoError(t, err)
	require.NotNil(t, resp)
	assert.True(t, resp.Success)

	// Verify logger was called
	assert.Contains(t, logger.debugCalls, "commbus_query_success")
}

// =============================================================================
// SUBSCRIBE TESTS
// =============================================================================

func TestCommBusServer_Subscribe_Success(t *testing.T) {
	server, logger, _ := createCommBusTestServer()

	streamCtx, cancel := context.WithCancel(context.Background())
	stream := newMockStreamServer(streamCtx)

	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		req := &pb.CommBusSubscribeRequest{
			EventTypes: []string{"Event1", "Event2"},
		}
		err := server.Subscribe(req, stream)
		assert.ErrorIs(t, err, context.Canceled)
	}()

	// Wait for subscription to register
	time.Sleep(50 * time.Millisecond)

	// Verify subscription was registered
	assert.Contains(t, logger.debugCalls, "commbus_subscribe_started")

	// Cancel to end subscription
	cancel()
	wg.Wait()

	// Verify cleanup was called
	assert.Contains(t, logger.debugCalls, "commbus_subscribe_ended")
}

func TestCommBusServer_Subscribe_NoEventTypes(t *testing.T) {
	server, _, _ := createCommBusTestServer()

	streamCtx := context.Background()
	stream := newMockStreamServer(streamCtx)

	req := &pb.CommBusSubscribeRequest{
		EventTypes: []string{},
	}

	err := server.Subscribe(req, stream)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "at least one event_type is required")
}

func TestCommBusServer_Subscribe_ReceivesEvents(t *testing.T) {
	server, _, _ := createCommBusTestServer()
	ctx := context.Background()

	streamCtx, cancel := context.WithCancel(context.Background())
	defer cancel()
	stream := newMockStreamServer(streamCtx)

	// Start subscription
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		req := &pb.CommBusSubscribeRequest{
			EventTypes: []string{"TestEvent"},
		}
		_ = server.Subscribe(req, stream)
	}()

	// Wait for subscription to register
	time.Sleep(50 * time.Millisecond)

	// Publish event
	publishReq := &pb.CommBusPublishRequest{
		EventType: "TestEvent",
		Payload:   []byte(`{"test":"data"}`),
	}
	_, err := server.Publish(ctx, publishReq)
	require.NoError(t, err)

	// Wait for event to arrive
	select {
	case event := <-stream.sendChan:
		assert.Equal(t, "TestEvent", event.EventType)
		assert.Equal(t, []byte(`{"test":"data"}`), event.Payload)
		assert.Greater(t, event.TimestampMs, int64(0))
	case <-time.After(1 * time.Second):
		t.Fatal("Timeout waiting for event")
	}

	cancel()
	wg.Wait()
}

func TestCommBusServer_Subscribe_MultipleEventTypes(t *testing.T) {
	server, _, _ := createCommBusTestServer()
	ctx := context.Background()

	streamCtx, cancel := context.WithCancel(context.Background())
	defer cancel()
	stream := newMockStreamServer(streamCtx)

	// Start subscription for multiple event types
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		req := &pb.CommBusSubscribeRequest{
			EventTypes: []string{"Event1", "Event2", "Event3"},
		}
		_ = server.Subscribe(req, stream)
	}()

	// Wait for subscription to register
	time.Sleep(50 * time.Millisecond)

	// Publish events of different types
	for _, eventType := range []string{"Event1", "Event2", "Event3"} {
		publishReq := &pb.CommBusPublishRequest{
			EventType: eventType,
			Payload:   []byte(`{}`),
		}
		_, err := server.Publish(ctx, publishReq)
		require.NoError(t, err)
	}

	// Receive all three events
	received := make(map[string]bool)
	for i := 0; i < 3; i++ {
		select {
		case event := <-stream.sendChan:
			received[event.EventType] = true
		case <-time.After(1 * time.Second):
			t.Fatal("Timeout waiting for events")
		}
	}

	assert.True(t, received["Event1"])
	assert.True(t, received["Event2"])
	assert.True(t, received["Event3"])

	cancel()
	wg.Wait()
}

func TestCommBusServer_Subscribe_SendError(t *testing.T) {
	server, logger, _ := createCommBusTestServer()
	ctx := context.Background()

	streamCtx := context.Background()
	stream := newMockStreamServer(streamCtx)
	stream.sendErr = errors.New("send failed")

	// Start subscription
	var wg sync.WaitGroup
	wg.Add(1)
	var subscribeErr error
	go func() {
		defer wg.Done()
		req := &pb.CommBusSubscribeRequest{
			EventTypes: []string{"TestEvent"},
		}
		subscribeErr = server.Subscribe(req, stream)
	}()

	// Wait for subscription to register
	time.Sleep(50 * time.Millisecond)

	// Publish event
	publishReq := &pb.CommBusPublishRequest{
		EventType: "TestEvent",
		Payload:   []byte(`{}`),
	}
	_, err := server.Publish(ctx, publishReq)
	require.NoError(t, err)

	// Wait for subscription to end with error
	wg.Wait()

	require.Error(t, subscribeErr)
	assert.Contains(t, subscribeErr.Error(), "send failed")
	assert.Contains(t, logger.errorCalls, "commbus_subscribe_send_failed")
}

// =============================================================================
// NOTIFY SUBSCRIBERS TESTS
// =============================================================================

func TestCommBusServer_NotifySubscribers_NoSubscribers(t *testing.T) {
	server, _, _ := createCommBusTestServer()

	// Notify with no subscribers registered - should not panic
	server.notifySubscribers("NonExistentEvent", []byte(`{}`))

	// Smoke test passed - no panics occurred
}

// =============================================================================
// CONSTRUCTOR TESTS
// =============================================================================

func TestNewCommBusServer(t *testing.T) {
	logger := &MockLogger{}
	bus := commbus.NewInMemoryCommBus(30 * time.Second)

	server := NewCommBusServer(logger, bus)

	require.NotNil(t, server)
	assert.Equal(t, logger, server.logger)
	assert.Equal(t, bus, server.bus)
	assert.NotNil(t, server.subscriptions)
	assert.Equal(t, 0, len(server.subscriptions))
}
