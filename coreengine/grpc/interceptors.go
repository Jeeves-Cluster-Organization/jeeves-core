// Package grpc provides gRPC interceptors for cross-cutting concerns.
// Interceptors add logging, recovery, and observability to gRPC calls.
package grpc

import (
	"context"
	"fmt"
	"runtime/debug"
	"time"

	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

// =============================================================================
// LOGGING INTERCEPTOR
// =============================================================================

// LoggingInterceptor creates a unary server interceptor that logs requests.
// It logs the start, duration, and result of each RPC call.
func LoggingInterceptor(logger Logger) grpc.UnaryServerInterceptor {
	return func(
		ctx context.Context,
		req interface{},
		info *grpc.UnaryServerInfo,
		handler grpc.UnaryHandler,
	) (interface{}, error) {
		start := time.Now()

		logger.Debug("grpc_request_started",
			"method", info.FullMethod,
		)

		// Call the handler
		resp, err := handler(ctx, req)

		// Calculate duration
		duration := time.Since(start)

		// Log result
		if err != nil {
			st, _ := status.FromError(err)
			logger.Error("grpc_request_failed",
				"method", info.FullMethod,
				"duration_ms", duration.Milliseconds(),
				"code", st.Code().String(),
				"error", err.Error(),
			)
		} else {
			logger.Debug("grpc_request_completed",
				"method", info.FullMethod,
				"duration_ms", duration.Milliseconds(),
			)
		}

		return resp, err
	}
}

// StreamLoggingInterceptor creates a stream server interceptor that logs requests.
func StreamLoggingInterceptor(logger Logger) grpc.StreamServerInterceptor {
	return func(
		srv interface{},
		ss grpc.ServerStream,
		info *grpc.StreamServerInfo,
		handler grpc.StreamHandler,
	) error {
		start := time.Now()

		logger.Debug("grpc_stream_started",
			"method", info.FullMethod,
			"client_stream", info.IsClientStream,
			"server_stream", info.IsServerStream,
		)

		// Call the handler
		err := handler(srv, ss)

		// Calculate duration
		duration := time.Since(start)

		// Log result
		if err != nil {
			st, _ := status.FromError(err)
			logger.Error("grpc_stream_failed",
				"method", info.FullMethod,
				"duration_ms", duration.Milliseconds(),
				"code", st.Code().String(),
				"error", err.Error(),
			)
		} else {
			logger.Debug("grpc_stream_completed",
				"method", info.FullMethod,
				"duration_ms", duration.Milliseconds(),
			)
		}

		return err
	}
}

// =============================================================================
// RECOVERY INTERCEPTOR
// =============================================================================

// RecoveryHandler is called when a panic is recovered.
// It receives the panic value and should return an appropriate error.
type RecoveryHandler func(p interface{}) error

// DefaultRecoveryHandler returns an Internal error with panic details.
func DefaultRecoveryHandler(p interface{}) error {
	return status.Errorf(codes.Internal, "panic recovered: %v", p)
}

// RecoveryInterceptor creates a unary server interceptor that recovers from panics.
// If a panic occurs, it logs the stack trace and returns an Internal error.
func RecoveryInterceptor(logger Logger, handler RecoveryHandler) grpc.UnaryServerInterceptor {
	if handler == nil {
		handler = DefaultRecoveryHandler
	}

	return func(
		ctx context.Context,
		req interface{},
		info *grpc.UnaryServerInfo,
		grpcHandler grpc.UnaryHandler,
	) (resp interface{}, err error) {
		defer func() {
			if p := recover(); p != nil {
				stack := string(debug.Stack())
				logger.Error("grpc_panic_recovered",
					"method", info.FullMethod,
					"panic", fmt.Sprintf("%v", p),
					"stack", stack,
				)
				err = handler(p)
			}
		}()

		return grpcHandler(ctx, req)
	}
}

// StreamRecoveryInterceptor creates a stream server interceptor that recovers from panics.
func StreamRecoveryInterceptor(logger Logger, handler RecoveryHandler) grpc.StreamServerInterceptor {
	if handler == nil {
		handler = DefaultRecoveryHandler
	}

	return func(
		srv interface{},
		ss grpc.ServerStream,
		info *grpc.StreamServerInfo,
		grpcHandler grpc.StreamHandler,
	) (err error) {
		defer func() {
			if p := recover(); p != nil {
				stack := string(debug.Stack())
				logger.Error("grpc_stream_panic_recovered",
					"method", info.FullMethod,
					"panic", fmt.Sprintf("%v", p),
					"stack", stack,
				)
				err = handler(p)
			}
		}()

		return grpcHandler(srv, ss)
	}
}

// =============================================================================
// CHAIN INTERCEPTORS
// =============================================================================

// ChainUnaryInterceptors chains multiple unary interceptors together.
// Interceptors are executed in order: first interceptor wraps second, etc.
func ChainUnaryInterceptors(interceptors ...grpc.UnaryServerInterceptor) grpc.UnaryServerInterceptor {
	return func(
		ctx context.Context,
		req interface{},
		info *grpc.UnaryServerInfo,
		handler grpc.UnaryHandler,
	) (interface{}, error) {
		// Build the handler chain from right to left
		chain := handler
		for i := len(interceptors) - 1; i >= 0; i-- {
			interceptor := interceptors[i]
			currentHandler := chain
			chain = func(ctx context.Context, req interface{}) (interface{}, error) {
				return interceptor(ctx, req, info, currentHandler)
			}
		}
		return chain(ctx, req)
	}
}

// ChainStreamInterceptors chains multiple stream interceptors together.
func ChainStreamInterceptors(interceptors ...grpc.StreamServerInterceptor) grpc.StreamServerInterceptor {
	return func(
		srv interface{},
		ss grpc.ServerStream,
		info *grpc.StreamServerInfo,
		handler grpc.StreamHandler,
	) error {
		// Build the handler chain from right to left
		chain := handler
		for i := len(interceptors) - 1; i >= 0; i-- {
			interceptor := interceptors[i]
			currentHandler := chain
			chain = func(srv interface{}, ss grpc.ServerStream) error {
				return interceptor(srv, ss, info, currentHandler)
			}
		}
		return chain(srv, ss)
	}
}

// =============================================================================
// SERVER OPTIONS BUILDER
// =============================================================================

// ServerOptions creates gRPC server options with standard interceptors.
// This is the recommended way to configure a production gRPC server.
func ServerOptions(logger Logger) []grpc.ServerOption {
	// Create interceptor chains
	unaryInterceptor := ChainUnaryInterceptors(
		RecoveryInterceptor(logger, nil),
		LoggingInterceptor(logger),
	)

	streamInterceptor := ChainStreamInterceptors(
		StreamRecoveryInterceptor(logger, nil),
		StreamLoggingInterceptor(logger),
	)

	return []grpc.ServerOption{
		grpc.UnaryInterceptor(unaryInterceptor),
		grpc.StreamInterceptor(streamInterceptor),
	}
}
