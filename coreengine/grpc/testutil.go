// Package grpc provides shared test utilities.
//
// Go Testing Best Practice:
// Shared test helpers belong in the same package, not a parallel test directory.
// This follows stdlib patterns (net/http/httptest, testing/iotest).
package grpc

import (
	"context"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/commbus"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/kernel"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"google.golang.org/grpc"
	"google.golang.org/grpc/metadata"
)

// =============================================================================
// LOGGER MOCKS
// =============================================================================

// TestLogger is a flexible mock logger for testing.
// Captures both simple message strings and structured key-value pairs.
// Unified implementation supporting both simple and detailed verification.
type TestLogger struct {
	DebugCalls []LogCall
	InfoCalls  []LogCall
	WarnCalls  []LogCall
	ErrorCalls []LogCall
}

// LogCall represents a single log call with message and structured fields.
type LogCall struct {
	Message string
	Fields  map[string]any
}

func (l *TestLogger) Debug(msg string, keysAndValues ...any) {
	l.DebugCalls = append(l.DebugCalls, LogCall{
		Message: msg,
		Fields:  toMap(msg, keysAndValues),
	})
}

func (l *TestLogger) Info(msg string, keysAndValues ...any) {
	l.InfoCalls = append(l.InfoCalls, LogCall{
		Message: msg,
		Fields:  toMap(msg, keysAndValues),
	})
}

func (l *TestLogger) Warn(msg string, keysAndValues ...any) {
	l.WarnCalls = append(l.WarnCalls, LogCall{
		Message: msg,
		Fields:  toMap(msg, keysAndValues),
	})
}

func (l *TestLogger) Error(msg string, keysAndValues ...any) {
	l.ErrorCalls = append(l.ErrorCalls, LogCall{
		Message: msg,
		Fields:  toMap(msg, keysAndValues),
	})
}

// toMap converts key-value pairs to a map for structured assertions.
func toMap(msg string, keysAndValues []any) map[string]any {
	m := map[string]any{"msg": msg}
	for i := 0; i < len(keysAndValues)-1; i += 2 {
		if key, ok := keysAndValues[i].(string); ok {
			m[key] = keysAndValues[i+1]
		}
	}
	return m
}

// HasDebugMessage checks if any debug call contains the given message.
func (l *TestLogger) HasDebugMessage(msg string) bool {
	for _, call := range l.DebugCalls {
		if call.Message == msg {
			return true
		}
	}
	return false
}

// HasErrorMessage checks if any error call contains the given message.
func (l *TestLogger) HasErrorMessage(msg string) bool {
	for _, call := range l.ErrorCalls {
		if call.Message == msg {
			return true
		}
	}
	return false
}

// MockLogger is a simple mock logger for basic testing needs.
// Used by server_test.go, kernel_server_test.go, commbus_server_test.go.
// For compatibility with existing tests that check simple message strings.
type MockLogger struct {
	debugCalls []string
	infoCalls  []string
	warnCalls  []string
	errorCalls []string
}

func (m *MockLogger) Debug(msg string, keysAndValues ...any) {
	m.debugCalls = append(m.debugCalls, msg)
}

func (m *MockLogger) Info(msg string, keysAndValues ...any) {
	m.infoCalls = append(m.infoCalls, msg)
}

func (m *MockLogger) Warn(msg string, keysAndValues ...any) {
	m.warnCalls = append(m.warnCalls, msg)
}

func (m *MockLogger) Error(msg string, keysAndValues ...any) {
	m.errorCalls = append(m.errorCalls, msg)
}

// =============================================================================
// TEST SERVER FACTORIES
// =============================================================================

// CreateTestEngineServer creates an EngineServer with simple mock logger for testing.
func CreateTestEngineServer() (*EngineServer, *MockLogger) {
	logger := &MockLogger{}
	server := NewEngineServer(logger)
	return server, logger
}

// CreateTestKernelServer creates a KernelServer with mock kernel and logger for testing.
func CreateTestKernelServer() (*KernelServer, *MockLogger, *kernel.Kernel) {
	logger := &MockLogger{}
	k := kernel.NewKernel(logger, nil)
	server := NewKernelServer(logger, k)
	return server, logger, k
}

// CreateTestCommBusServer creates a CommBusServer with real InMemoryCommBus for testing.
func CreateTestCommBusServer() (*CommBusServer, *MockLogger, commbus.CommBus) {
	logger := &MockLogger{}
	bus := commbus.NewInMemoryCommBus(30 * time.Second)
	server := NewCommBusServer(logger, bus)
	return server, logger, bus
}

// =============================================================================
// MOCK GRPC STREAMS
// =============================================================================

// MockCommBusStream implements pb.CommBusService_SubscribeServer for testing.
// Used for testing CommBus subscription streams.
type MockCommBusStream struct {
	ctx      context.Context
	sendChan chan *pb.CommBusEvent
	sendErr  error
}

// NewMockCommBusStream creates a new mock CommBus subscription stream.
func NewMockCommBusStream(ctx context.Context) *MockCommBusStream {
	return &MockCommBusStream{
		ctx:      ctx,
		sendChan: make(chan *pb.CommBusEvent, 100),
	}
}

func (m *MockCommBusStream) Send(event *pb.CommBusEvent) error {
	if m.sendErr != nil {
		return m.sendErr
	}
	m.sendChan <- event
	return nil
}

func (m *MockCommBusStream) Context() context.Context {
	return m.ctx
}

// SetSendError configures the stream to return an error on Send.
func (m *MockCommBusStream) SetSendError(err error) {
	m.sendErr = err
}

// RecvChan returns the channel for receiving sent events.
func (m *MockCommBusStream) RecvChan() <-chan *pb.CommBusEvent {
	return m.sendChan
}

// gRPC ServerStream interface methods - required for interface compliance
func (m *MockCommBusStream) SendHeader(md metadata.MD) error { return nil }
func (m *MockCommBusStream) SetTrailer(md metadata.MD)       {}
func (m *MockCommBusStream) SendMsg(msg interface{}) error   { return nil }
func (m *MockCommBusStream) RecvMsg(msg interface{}) error   { return nil }
func (m *MockCommBusStream) SetHeader(md metadata.MD) error  { return nil }

// MockServerStream implements grpc.ServerStream for testing interceptors.
// Used for testing gRPC stream interceptors.
type MockServerStream struct {
	grpc.ServerStream // Embed for default implementations
	ctx               context.Context
}

// NewMockServerStream creates a new mock server stream.
func NewMockServerStream(ctx context.Context) *MockServerStream {
	return &MockServerStream{ctx: ctx}
}

func (m *MockServerStream) Context() context.Context {
	if m.ctx != nil {
		return m.ctx
	}
	return context.Background()
}

// =============================================================================
// TEST CONTEXT HELPERS (Agentic Security Testing)
// =============================================================================

// ContextWithUserMetadata creates a context with user/session/request metadata.
// Used for testing RPC methods that require request context validation.
func ContextWithUserMetadata(userID, sessionID, requestID string) context.Context {
	md := metadata.Pairs(
		"user_id", userID,
		"session_id", sessionID,
		"request_id", requestID,
	)
	return metadata.NewIncomingContext(context.Background(), md)
}
