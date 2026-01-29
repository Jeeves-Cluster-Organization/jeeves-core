// Package grpc provides the CommBusService gRPC server.
// This exposes the kernel's CommBus via gRPC for Python access.
//
// Agentic OS Pattern:
//
//	Python (jeeves_infra) → kernel_client → gRPC → CommBusServer → InMemoryCommBus
//
// This follows the "syscall" pattern where Python calls the Go kernel via gRPC.
package grpc

import (
	"context"
	"encoding/json"
	"sync"
	"time"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/jeeves-cluster-organization/codeanalysis/commbus"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
)

// GenericMessage is a message wrapper for gRPC-originated messages.
// It wraps JSON payload and message type for CommBus routing.
type GenericMessage struct {
	messageType string
	category    string // "event", "command", or "query"
	payload     []byte
}

// Category implements commbus.Message interface.
func (m *GenericMessage) Category() string {
	return m.category
}

// GenericQuery wraps a query message for gRPC.
type GenericQuery struct {
	GenericMessage
}

// IsQuery implements commbus.Query interface marker method.
func (q *GenericQuery) IsQuery() {}

// CommBusServer implements the gRPC CommBusService.
// Thread-safe: delegates to CommBus which handles synchronization.
type CommBusServer struct {
	pb.UnimplementedCommBusServiceServer

	logger Logger
	bus    commbus.CommBus

	// Subscription management
	subscriptions map[string][]chan *pb.CommBusEvent
	subMu         sync.RWMutex
}

// NewCommBusServer creates a new CommBusService gRPC server.
func NewCommBusServer(logger Logger, bus commbus.CommBus) *CommBusServer {
	return &CommBusServer{
		logger:        logger,
		bus:           bus,
		subscriptions: make(map[string][]chan *pb.CommBusEvent),
	}
}

// =============================================================================
// Publish - Fire-and-forget event to all subscribers
// =============================================================================

// Publish publishes an event to all subscribers.
func (s *CommBusServer) Publish(
	ctx context.Context,
	req *pb.CommBusPublishRequest,
) (*pb.CommBusPublishResponse, error) {
	if req.EventType == "" {
		return nil, status.Error(codes.InvalidArgument, "event_type is required")
	}

	event := &GenericMessage{
		messageType: req.EventType,
		category:    "event",
		payload:     req.Payload,
	}

	err := s.bus.Publish(ctx, event)
	if err != nil {
		s.logger.Error("commbus_publish_failed",
			"event_type", req.EventType,
			"error", err.Error(),
		)
		return &pb.CommBusPublishResponse{
			Success: false,
			Error:   err.Error(),
		}, nil
	}

	s.logger.Debug("commbus_publish_success",
		"event_type", req.EventType,
	)

	// Also notify gRPC subscribers
	s.notifySubscribers(req.EventType, req.Payload)

	return &pb.CommBusPublishResponse{
		Success: true,
	}, nil
}

// =============================================================================
// Send - Fire-and-forget command to single handler
// =============================================================================

// Send sends a command to its handler.
func (s *CommBusServer) Send(
	ctx context.Context,
	req *pb.CommBusSendRequest,
) (*pb.CommBusSendResponse, error) {
	if req.CommandType == "" {
		return nil, status.Error(codes.InvalidArgument, "command_type is required")
	}

	command := &GenericMessage{
		messageType: req.CommandType,
		category:    "command",
		payload:     req.Payload,
	}

	err := s.bus.Send(ctx, command)
	if err != nil {
		s.logger.Error("commbus_send_failed",
			"command_type", req.CommandType,
			"error", err.Error(),
		)
		return &pb.CommBusSendResponse{
			Success: false,
			Error:   err.Error(),
		}, nil
	}

	s.logger.Debug("commbus_send_success",
		"command_type", req.CommandType,
	)

	return &pb.CommBusSendResponse{
		Success: true,
	}, nil
}

// =============================================================================
// Query - Request-response with timeout
// =============================================================================

// Query sends a query and waits for response.
func (s *CommBusServer) Query(
	ctx context.Context,
	req *pb.CommBusQueryRequest,
) (*pb.CommBusQueryResponse, error) {
	if req.QueryType == "" {
		return nil, status.Error(codes.InvalidArgument, "query_type is required")
	}

	query := &GenericQuery{
		GenericMessage: GenericMessage{
			messageType: req.QueryType,
			category:    "query",
			payload:     req.Payload,
		},
	}

	// Apply timeout if specified
	queryCtx := ctx
	if req.TimeoutMs > 0 {
		var cancel context.CancelFunc
		queryCtx, cancel = context.WithTimeout(ctx, time.Duration(req.TimeoutMs)*time.Millisecond)
		defer cancel()
	}

	result, err := s.bus.QuerySync(queryCtx, query)
	if err != nil {
		s.logger.Error("commbus_query_failed",
			"query_type", req.QueryType,
			"error", err.Error(),
		)
		return &pb.CommBusQueryResponse{
			Success: false,
			Error:   err.Error(),
		}, nil
	}

	// Serialize result to JSON
	resultBytes, err := json.Marshal(result)
	if err != nil {
		s.logger.Error("commbus_query_marshal_failed",
			"query_type", req.QueryType,
			"error", err.Error(),
		)
		return &pb.CommBusQueryResponse{
			Success: false,
			Error:   "failed to marshal result: " + err.Error(),
		}, nil
	}

	s.logger.Debug("commbus_query_success",
		"query_type", req.QueryType,
	)

	return &pb.CommBusQueryResponse{
		Success: true,
		Result:  resultBytes,
	}, nil
}

// =============================================================================
// Subscribe - Server streaming for events
// =============================================================================

// Subscribe subscribes to events via server streaming.
func (s *CommBusServer) Subscribe(
	req *pb.CommBusSubscribeRequest,
	stream pb.CommBusService_SubscribeServer,
) error {
	if len(req.EventTypes) == 0 {
		return status.Error(codes.InvalidArgument, "at least one event_type is required")
	}

	// Create channel for this subscriber
	eventChan := make(chan *pb.CommBusEvent, 100)

	// Register for each event type
	s.subMu.Lock()
	for _, eventType := range req.EventTypes {
		s.subscriptions[eventType] = append(s.subscriptions[eventType], eventChan)
	}
	s.subMu.Unlock()

	s.logger.Debug("commbus_subscribe_started",
		"event_types", req.EventTypes,
	)

	// Cleanup on exit
	defer func() {
		s.subMu.Lock()
		for _, eventType := range req.EventTypes {
			channels := s.subscriptions[eventType]
			for i, ch := range channels {
				if ch == eventChan {
					s.subscriptions[eventType] = append(channels[:i], channels[i+1:]...)
					break
				}
			}
		}
		s.subMu.Unlock()
		close(eventChan)
	}()

	// Stream events to client
	for {
		select {
		case <-stream.Context().Done():
			s.logger.Debug("commbus_subscribe_ended",
				"event_types", req.EventTypes,
			)
			return stream.Context().Err()
		case event := <-eventChan:
			if err := stream.Send(event); err != nil {
				s.logger.Error("commbus_subscribe_send_failed",
					"error", err.Error(),
				)
				return err
			}
		}
	}
}

// notifySubscribers sends event to all gRPC subscribers.
func (s *CommBusServer) notifySubscribers(eventType string, payload []byte) {
	s.subMu.RLock()
	channels := s.subscriptions[eventType]
	s.subMu.RUnlock()

	event := &pb.CommBusEvent{
		EventType:   eventType,
		Payload:     payload,
		TimestampMs: time.Now().UnixMilli(),
	}

	for _, ch := range channels {
		// Non-blocking send
		select {
		case ch <- event:
		default:
			s.logger.Warn("commbus_subscriber_channel_full",
				"event_type", eventType,
			)
		}
	}
}
