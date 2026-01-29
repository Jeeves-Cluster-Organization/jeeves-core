package grpc

import (
	"context"
	"errors"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

// TestLogger captures log calls for verification.
type TestLogger struct {
	debugCalls []map[string]any
	infoCalls  []map[string]any
	warnCalls  []map[string]any
	errorCalls []map[string]any
}

func (l *TestLogger) Debug(msg string, keysAndValues ...any) {
	l.debugCalls = append(l.debugCalls, toMap(msg, keysAndValues))
}

func (l *TestLogger) Info(msg string, keysAndValues ...any) {
	l.infoCalls = append(l.infoCalls, toMap(msg, keysAndValues))
}

func (l *TestLogger) Warn(msg string, keysAndValues ...any) {
	l.warnCalls = append(l.warnCalls, toMap(msg, keysAndValues))
}

func (l *TestLogger) Error(msg string, keysAndValues ...any) {
	l.errorCalls = append(l.errorCalls, toMap(msg, keysAndValues))
}

func toMap(msg string, keysAndValues []any) map[string]any {
	m := map[string]any{"msg": msg}
	for i := 0; i < len(keysAndValues)-1; i += 2 {
		if key, ok := keysAndValues[i].(string); ok {
			m[key] = keysAndValues[i+1]
		}
	}
	return m
}

// =============================================================================
// LOGGING INTERCEPTOR TESTS
// =============================================================================

func TestLoggingInterceptor_Success(t *testing.T) {
	logger := &TestLogger{}
	interceptor := LoggingInterceptor(logger)

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/TestMethod"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return "response", nil
	}

	resp, err := interceptor(context.Background(), "request", info, handler)

	require.NoError(t, err)
	assert.Equal(t, "response", resp)

	// Should have logged start and completion
	assert.Len(t, logger.debugCalls, 2)
	assert.Equal(t, "grpc_request_started", logger.debugCalls[0]["msg"])
	assert.Equal(t, "grpc_request_completed", logger.debugCalls[1]["msg"])
	assert.Equal(t, "/test.Service/TestMethod", logger.debugCalls[1]["method"])
}

func TestLoggingInterceptor_Error(t *testing.T) {
	logger := &TestLogger{}
	interceptor := LoggingInterceptor(logger)

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/FailMethod"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return nil, status.Error(codes.NotFound, "resource not found")
	}

	resp, err := interceptor(context.Background(), "request", info, handler)

	require.Error(t, err)
	assert.Nil(t, resp)

	// Should have logged start and error
	assert.Len(t, logger.debugCalls, 1)
	assert.Equal(t, "grpc_request_started", logger.debugCalls[0]["msg"])
	assert.Len(t, logger.errorCalls, 1)
	assert.Equal(t, "grpc_request_failed", logger.errorCalls[0]["msg"])
	assert.Equal(t, "NotFound", logger.errorCalls[0]["code"])
}

// =============================================================================
// RECOVERY INTERCEPTOR TESTS
// =============================================================================

func TestRecoveryInterceptor_NoPanic(t *testing.T) {
	logger := &TestLogger{}
	interceptor := RecoveryInterceptor(logger, nil)

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/SafeMethod"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return "safe response", nil
	}

	resp, err := interceptor(context.Background(), "request", info, handler)

	require.NoError(t, err)
	assert.Equal(t, "safe response", resp)
	assert.Empty(t, logger.errorCalls)
}

func TestRecoveryInterceptor_Panic(t *testing.T) {
	logger := &TestLogger{}
	interceptor := RecoveryInterceptor(logger, nil)

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/PanicMethod"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		panic("test panic")
	}

	resp, err := interceptor(context.Background(), "request", info, handler)

	require.Error(t, err)
	assert.Nil(t, resp)

	// Should be an Internal error
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Internal, st.Code())
	assert.Contains(t, st.Message(), "test panic")

	// Should have logged the panic
	require.Len(t, logger.errorCalls, 1)
	assert.Equal(t, "grpc_panic_recovered", logger.errorCalls[0]["msg"])
	assert.Contains(t, logger.errorCalls[0]["panic"], "test panic")
}

func TestRecoveryInterceptor_CustomHandler(t *testing.T) {
	logger := &TestLogger{}
	customHandler := func(p interface{}) error {
		return status.Errorf(codes.Aborted, "custom: %v", p)
	}
	interceptor := RecoveryInterceptor(logger, customHandler)

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/PanicMethod"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		panic("custom panic")
	}

	_, err := interceptor(context.Background(), "request", info, handler)

	require.Error(t, err)
	st, _ := status.FromError(err)
	assert.Equal(t, codes.Aborted, st.Code())
	assert.Contains(t, st.Message(), "custom: custom panic")
}

// =============================================================================
// CHAIN INTERCEPTORS TESTS
// =============================================================================

func TestChainUnaryInterceptors(t *testing.T) {
	// Track order of execution
	order := []string{}

	interceptor1 := func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
		order = append(order, "before1")
		resp, err := handler(ctx, req)
		order = append(order, "after1")
		return resp, err
	}

	interceptor2 := func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
		order = append(order, "before2")
		resp, err := handler(ctx, req)
		order = append(order, "after2")
		return resp, err
	}

	chain := ChainUnaryInterceptors(interceptor1, interceptor2)

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/ChainMethod"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		order = append(order, "handler")
		return "response", nil
	}

	resp, err := chain(context.Background(), "request", info, handler)

	require.NoError(t, err)
	assert.Equal(t, "response", resp)

	// Interceptors should wrap in order: interceptor1 -> interceptor2 -> handler
	assert.Equal(t, []string{"before1", "before2", "handler", "after2", "after1"}, order)
}

func TestChainUnaryInterceptors_Empty(t *testing.T) {
	chain := ChainUnaryInterceptors()

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/Method"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return "response", nil
	}

	resp, err := chain(context.Background(), "request", info, handler)

	require.NoError(t, err)
	assert.Equal(t, "response", resp)
}

func TestChainUnaryInterceptors_WithError(t *testing.T) {
	interceptor := func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
		return handler(ctx, req)
	}

	chain := ChainUnaryInterceptors(interceptor)

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/Method"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return nil, errors.New("handler error")
	}

	resp, err := chain(context.Background(), "request", info, handler)

	require.Error(t, err)
	assert.Nil(t, resp)
	assert.Contains(t, err.Error(), "handler error")
}

// =============================================================================
// SERVER OPTIONS TESTS
// =============================================================================

func TestServerOptions(t *testing.T) {
	logger := &TestLogger{}
	opts := ServerOptions(logger)

	// Should return at least 2 options (unary and stream interceptors)
	assert.GreaterOrEqual(t, len(opts), 2)
}

// =============================================================================
// DEFAULT RECOVERY HANDLER TESTS
// =============================================================================

func TestDefaultRecoveryHandler(t *testing.T) {
	err := DefaultRecoveryHandler("test panic value")

	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Internal, st.Code())
	assert.Contains(t, st.Message(), "test panic value")
}

// =============================================================================
// STREAM INTERCEPTOR TESTS
// =============================================================================

// mockServerStream implements grpc.ServerStream for testing
type mockServerStream struct {
	grpc.ServerStream
	ctx context.Context
}

func (m *mockServerStream) Context() context.Context {
	if m.ctx != nil {
		return m.ctx
	}
	return context.Background()
}

func TestStreamLoggingInterceptor_Success(t *testing.T) {
	logger := &TestLogger{}
	interceptor := StreamLoggingInterceptor(logger)

	info := &grpc.StreamServerInfo{
		FullMethod:     "/test.Service/StreamMethod",
		IsClientStream: true,
		IsServerStream: true,
	}

	handler := func(srv interface{}, stream grpc.ServerStream) error {
		return nil
	}

	stream := &mockServerStream{ctx: context.Background()}
	err := interceptor(nil, stream, info, handler)

	require.NoError(t, err)
	assert.GreaterOrEqual(t, len(logger.debugCalls), 1)
}

func TestStreamLoggingInterceptor_Error(t *testing.T) {
	logger := &TestLogger{}
	interceptor := StreamLoggingInterceptor(logger)

	info := &grpc.StreamServerInfo{
		FullMethod: "/test.Service/StreamMethod",
	}

	handler := func(srv interface{}, stream grpc.ServerStream) error {
		return errors.New("stream error")
	}

	stream := &mockServerStream{ctx: context.Background()}
	err := interceptor(nil, stream, info, handler)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "stream error")
}

func TestStreamRecoveryInterceptor_Success(t *testing.T) {
	logger := &TestLogger{}
	interceptor := StreamRecoveryInterceptor(logger, DefaultRecoveryHandler)

	info := &grpc.StreamServerInfo{
		FullMethod: "/test.Service/StreamMethod",
	}

	handler := func(srv interface{}, stream grpc.ServerStream) error {
		return nil
	}

	stream := &mockServerStream{ctx: context.Background()}
	err := interceptor(nil, stream, info, handler)

	require.NoError(t, err)
}

func TestStreamRecoveryInterceptor_Panic(t *testing.T) {
	logger := &TestLogger{}
	interceptor := StreamRecoveryInterceptor(logger, DefaultRecoveryHandler)

	info := &grpc.StreamServerInfo{
		FullMethod: "/test.Service/StreamMethod",
	}

	handler := func(srv interface{}, stream grpc.ServerStream) error {
		panic("stream panic")
	}

	stream := &mockServerStream{ctx: context.Background()}
	err := interceptor(nil, stream, info, handler)

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Internal, st.Code())
	assert.Contains(t, st.Message(), "stream panic")
}

func TestChainStreamInterceptors(t *testing.T) {
	var order []string

	interceptor1 := func(srv interface{}, ss grpc.ServerStream, info *grpc.StreamServerInfo, handler grpc.StreamHandler) error {
		order = append(order, "before1")
		err := handler(srv, ss)
		order = append(order, "after1")
		return err
	}

	interceptor2 := func(srv interface{}, ss grpc.ServerStream, info *grpc.StreamServerInfo, handler grpc.StreamHandler) error {
		order = append(order, "before2")
		err := handler(srv, ss)
		order = append(order, "after2")
		return err
	}

	chain := ChainStreamInterceptors(interceptor1, interceptor2)

	info := &grpc.StreamServerInfo{FullMethod: "/test.Service/StreamMethod"}
	handler := func(srv interface{}, ss grpc.ServerStream) error {
		order = append(order, "handler")
		return nil
	}

	stream := &mockServerStream{ctx: context.Background()}
	err := chain(nil, stream, info, handler)

	require.NoError(t, err)
	assert.Equal(t, []string{"before1", "before2", "handler", "after2", "after1"}, order)
}

func TestChainStreamInterceptors_Empty(t *testing.T) {
	chain := ChainStreamInterceptors()

	info := &grpc.StreamServerInfo{FullMethod: "/test.Service/StreamMethod"}
	handler := func(srv interface{}, ss grpc.ServerStream) error {
		return nil
	}

	stream := &mockServerStream{ctx: context.Background()}
	err := chain(nil, stream, info, handler)

	require.NoError(t, err)
}

// =============================================================================
// METRICS INTERCEPTOR TESTS
// =============================================================================

func TestMetricsInterceptor_Success(t *testing.T) {
	interceptor := MetricsInterceptor()

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/Method"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return "response", nil
	}

	resp, err := interceptor(context.Background(), "request", info, handler)

	require.NoError(t, err)
	assert.Equal(t, "response", resp)
}

func TestMetricsInterceptor_Error(t *testing.T) {
	interceptor := MetricsInterceptor()

	info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/Method"}
	handler := func(ctx context.Context, req interface{}) (interface{}, error) {
		return nil, status.Error(codes.Internal, "internal error")
	}

	resp, err := interceptor(context.Background(), "request", info, handler)

	require.Error(t, err)
	assert.Nil(t, resp)
}

func TestMetricsInterceptor_StatusCodes(t *testing.T) {
	interceptor := MetricsInterceptor()

	testCases := []struct {
		name string
		code codes.Code
	}{
		{"OK", codes.OK},
		{"InvalidArgument", codes.InvalidArgument},
		{"NotFound", codes.NotFound},
		{"Internal", codes.Internal},
		{"Unavailable", codes.Unavailable},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			info := &grpc.UnaryServerInfo{FullMethod: "/test.Service/Method"}
			handler := func(ctx context.Context, req interface{}) (interface{}, error) {
				if tc.code == codes.OK {
					return "ok", nil
				}
				return nil, status.Error(tc.code, "error")
			}

			_, err := interceptor(context.Background(), "request", info, handler)

			if tc.code == codes.OK {
				require.NoError(t, err)
			} else {
				require.Error(t, err)
			}
		})
	}
}

func TestStreamMetricsInterceptor_Success(t *testing.T) {
	interceptor := StreamMetricsInterceptor()

	info := &grpc.StreamServerInfo{
		FullMethod:     "/test.Service/StreamMethod",
		IsClientStream: true,
		IsServerStream: true,
	}

	handler := func(srv interface{}, stream grpc.ServerStream) error {
		return nil
	}

	stream := &mockServerStream{ctx: context.Background()}
	err := interceptor(nil, stream, info, handler)

	require.NoError(t, err)
}

func TestStreamMetricsInterceptor_Error(t *testing.T) {
	interceptor := StreamMetricsInterceptor()

	info := &grpc.StreamServerInfo{
		FullMethod: "/test.Service/StreamMethod",
	}

	handler := func(srv interface{}, stream grpc.ServerStream) error {
		return status.Error(codes.Internal, "stream error")
	}

	stream := &mockServerStream{ctx: context.Background()}
	err := interceptor(nil, stream, info, handler)

	require.Error(t, err)
	st, ok := status.FromError(err)
	require.True(t, ok)
	assert.Equal(t, codes.Internal, st.Code())
}

func TestStreamMetricsInterceptor_StatusCodes(t *testing.T) {
	interceptor := StreamMetricsInterceptor()

	testCases := []struct {
		name string
		code codes.Code
	}{
		{"OK", codes.OK},
		{"Canceled", codes.Canceled},
		{"DeadlineExceeded", codes.DeadlineExceeded},
		{"ResourceExhausted", codes.ResourceExhausted},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			info := &grpc.StreamServerInfo{FullMethod: "/test.Service/StreamMethod"}
			handler := func(srv interface{}, stream grpc.ServerStream) error {
				if tc.code == codes.OK {
					return nil
				}
				return status.Error(tc.code, "error")
			}

			stream := &mockServerStream{ctx: context.Background()}
			err := interceptor(nil, stream, info, handler)

			if tc.code == codes.OK {
				require.NoError(t, err)
			} else {
				require.Error(t, err)
			}
		})
	}
}
