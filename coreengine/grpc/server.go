// Package grpc provides the gRPC server for jeeves-core.
// This is the primary IPC mechanism between Python and Go.
package grpc

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"sync"
	"time"

	"google.golang.org/grpc"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/agents"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/runtime"
)

// Logger interface for the server.
type Logger interface {
	Debug(msg string, keysAndValues ...any)
	Info(msg string, keysAndValues ...any)
	Warn(msg string, keysAndValues ...any)
	Error(msg string, keysAndValues ...any)
}

// EngineServer implements the gRPC service.
// Thread-safe: all mutable fields are protected by runtimeMu.
type EngineServer struct {
	pb.UnimplementedEngineServiceServer

	logger    Logger
	runner    *runtime.PipelineRunner
	runtimeMu sync.RWMutex // Protects runtime field
}

// NewEngineServer creates a new gRPC server.
func NewEngineServer(logger Logger) *EngineServer {
	return &EngineServer{
		logger: logger,
	}
}

// SetRuntime sets the runtime for pipeline execution.
// Thread-safe: can be called concurrently with other methods.
func (s *EngineServer) SetRunner(r *runtime.PipelineRunner) {
	s.runtimeMu.Lock()
	defer s.runtimeMu.Unlock()
	s.runner = r
}

// getRunner returns the current pipeline runner.
// Thread-safe: protected by RWMutex.
func (s *EngineServer) getRunner() *runtime.PipelineRunner {
	s.runtimeMu.RLock()
	defer s.runtimeMu.RUnlock()
	return s.runner
}

// =============================================================================
// Envelope Operations
// =============================================================================

// CreateEnvelope creates a new Envelope.
func (s *EngineServer) CreateEnvelope(
	ctx context.Context,
	req *pb.CreateEnvelopeRequest,
) (*pb.Envelope, error) {
	var requestID *string
	if req.RequestId != "" {
		requestID = &req.RequestId
	}

	metadata := make(map[string]any)
	for k, v := range req.Metadata {
		metadata[k] = v
	}

	env := envelope.CreateEnvelope(
		req.RawInput,
		req.UserId,
		req.SessionId,
		requestID,
		metadata,
		req.StageOrder,
	)

	s.logger.Debug("envelope_created",
		"envelope_id", env.EnvelopeID,
		"user_id", req.UserId,
	)

	return envelopeToProto(env), nil
}

// UpdateEnvelope updates an envelope from proto.
func (s *EngineServer) UpdateEnvelope(
	ctx context.Context,
	req *pb.UpdateEnvelopeRequest,
) (*pb.Envelope, error) {
	env := protoToEnvelope(req.Envelope)

	s.logger.Debug("envelope_updated",
		"envelope_id", env.EnvelopeID,
	)

	return envelopeToProto(env), nil
}

// CloneEnvelope creates a deep copy of an envelope.
func (s *EngineServer) CloneEnvelope(
	ctx context.Context,
	req *pb.CloneRequest,
) (*pb.Envelope, error) {
	env := protoToEnvelope(req.Envelope)
	clone := env.Clone()

	s.logger.Debug("envelope_cloned",
		"original_id", env.EnvelopeID,
		"clone_id", clone.EnvelopeID,
	)

	return envelopeToProto(clone), nil
}

// =============================================================================
// Bounds Checking
// =============================================================================

// CheckBounds checks if execution can continue.
func (s *EngineServer) CheckBounds(
	ctx context.Context,
	protoEnv *pb.Envelope,
) (*pb.BoundsResult, error) {
	env := protoToEnvelope(protoEnv)
	canContinue := env.CanContinue()

	result := &pb.BoundsResult{
		CanContinue:         canContinue,
		LlmCallsRemaining:   int32(env.MaxLLMCalls - env.LLMCallCount),
		AgentHopsRemaining:  int32(env.MaxAgentHops - env.AgentHopCount),
		IterationsRemaining: int32(env.MaxIterations - env.Iteration),
	}

	if !canContinue && env.TerminalReason_ != nil {
		result.TerminalReason = terminalReasonToProto(*env.TerminalReason_)
	}

	s.logger.Debug("bounds_checked",
		"envelope_id", env.EnvelopeID,
		"can_continue", canContinue,
	)

	return result, nil
}

// =============================================================================
// Pipeline Execution
// =============================================================================

// ExecutePipeline executes the pipeline with streaming events.
func (s *EngineServer) ExecutePipeline(
	req *pb.ExecuteRequest,
	stream pb.EngineService_ExecutePipelineServer,
) error {
	ctx := stream.Context()
	env := protoToEnvelope(req.Envelope)

	s.logger.Info("pipeline_execution_started",
		"envelope_id", env.EnvelopeID,
		"thread_id", req.ThreadId,
	)

	// Parse pipeline config if provided
	if len(req.PipelineConfig) > 0 {
		var cfg config.PipelineConfig
		if err := json.Unmarshal(req.PipelineConfig, &cfg); err != nil {
			return fmt.Errorf("failed to parse pipeline config: %w", err)
		}
		// Create runtime with config
		rt, err := runtime.NewPipelineRunner(&cfg, nil, nil, s)
		if err != nil {
			return fmt.Errorf("failed to create runtime: %w", err)
		}
		s.SetRunner(rt)
	}

	rt := s.getRunner()
	if rt == nil {
		return fmt.Errorf("no runtime configured")
	}

	// Send started event
	if err := stream.Send(&pb.ExecutionEvent{
		Type:        pb.ExecutionEventType_STAGE_STARTED,
		Stage:       env.CurrentStage,
		TimestampMs: time.Now().UnixMilli(),
		Envelope:    envelopeToProto(env),
	}); err != nil {
		return err
	}

	// Execute pipeline using the local rt variable (thread-safe)
	result, err := rt.Run(ctx, env, req.ThreadId)
	if err != nil {
		// Send error event
		payload, _ := json.Marshal(map[string]any{"error": err.Error()})
		if sendErr := stream.Send(&pb.ExecutionEvent{
			Type:        pb.ExecutionEventType_STAGE_FAILED,
			Stage:       env.CurrentStage,
			TimestampMs: time.Now().UnixMilli(),
			Payload:     payload,
			Envelope:    envelopeToProto(result),
		}); sendErr != nil {
			return sendErr
		}
		return err
	}

	// Send completion event
	eventType := pb.ExecutionEventType_PIPELINE_COMPLETED
	if result.InterruptPending {
		eventType = pb.ExecutionEventType_INTERRUPT_RAISED
	} else if !result.CanContinue() {
		eventType = pb.ExecutionEventType_BOUNDS_EXCEEDED
	}

	if err := stream.Send(&pb.ExecutionEvent{
		Type:        eventType,
		Stage:       result.CurrentStage,
		TimestampMs: time.Now().UnixMilli(),
		Envelope:    envelopeToProto(result),
	}); err != nil {
		return err
	}

	s.logger.Info("pipeline_execution_completed",
		"envelope_id", env.EnvelopeID,
		"final_stage", result.CurrentStage,
		"terminated", result.Terminated,
	)

	return nil
}

// ExecuteAgent executes a single agent.
func (s *EngineServer) ExecuteAgent(
	ctx context.Context,
	req *pb.ExecuteAgentRequest,
) (*pb.AgentResult, error) {
	env := protoToEnvelope(req.Envelope)

	s.logger.Debug("agent_execution_started",
		"envelope_id", env.EnvelopeID,
		"agent", req.AgentName,
	)

	// For now, return a placeholder - actual agent execution
	// would go through the runtime
	return &pb.AgentResult{
		Success:    true,
		Envelope:   envelopeToProto(env),
		DurationMs: 0,
		LlmCalls:   0,
	}, nil
}

// =============================================================================
// Logger interface for grpc server (implements runtime Logger)
// =============================================================================

func (s *EngineServer) Debug(msg string, keysAndValues ...any) {
	s.logger.Debug(msg, keysAndValues...)
}

func (s *EngineServer) Info(msg string, keysAndValues ...any) {
	s.logger.Info(msg, keysAndValues...)
}

func (s *EngineServer) Warn(msg string, keysAndValues ...any) {
	s.logger.Warn(msg, keysAndValues...)
}

func (s *EngineServer) Error(msg string, keysAndValues ...any) {
	s.logger.Error(msg, keysAndValues...)
}

func (s *EngineServer) Bind(fields ...any) agents.Logger {
	return s
}

// =============================================================================
// Conversion Functions
// =============================================================================

func envelopeToProto(e *envelope.Envelope) *pb.Envelope {
	proto := &pb.Envelope{
		EnvelopeId:         e.EnvelopeID,
		RequestId:          e.RequestID,
		UserId:             e.UserID,
		SessionId:          e.SessionID,
		RawInput:           e.RawInput,
		ReceivedAtMs:       e.ReceivedAt.UnixMilli(),
		CurrentStage:       e.CurrentStage,
		StageOrder:         e.StageOrder,
		Iteration:          int32(e.Iteration),
		MaxIterations:      int32(e.MaxIterations),
		LlmCallCount:       int32(e.LLMCallCount),
		MaxLlmCalls:        int32(e.MaxLLMCalls),
		AgentHopCount:      int32(e.AgentHopCount),
		MaxAgentHops:       int32(e.MaxAgentHops),
		Terminated:         e.Terminated,
		InterruptPending:   e.InterruptPending,
		CurrentStageNumber: int32(e.CurrentStageNumber),
		MaxStages:          int32(e.MaxStages),
		AllGoals:           e.AllGoals,
		RemainingGoals:     e.RemainingGoals,
		CreatedAtMs:        e.CreatedAt.UnixMilli(),
	}

	// Termination reason
	if e.TerminationReason != nil {
		proto.TerminationReason = *e.TerminationReason
	}
	if e.TerminalReason_ != nil {
		proto.TerminalReason = terminalReasonToProto(*e.TerminalReason_)
	}
	if e.CompletedAt != nil {
		proto.CompletedAtMs = e.CompletedAt.UnixMilli()
	}

	// Outputs (JSON-encoded)
	proto.Outputs = make(map[string][]byte)
	for k, v := range e.Outputs {
		if data, err := json.Marshal(v); err == nil {
			proto.Outputs[k] = data
		}
	}

	// Parallel execution state
	proto.ActiveStages = e.ActiveStages
	proto.CompletedStageSet = e.CompletedStageSet
	proto.FailedStages = e.FailedStages

	// Goal completion status
	proto.GoalCompletionStatus = e.GoalCompletionStatus

	// Metadata
	proto.MetadataStr = make(map[string]string)
	for k, v := range e.Metadata {
		if str, ok := v.(string); ok {
			proto.MetadataStr[k] = str
		}
	}

	// Interrupt
	if e.Interrupt != nil {
		proto.Interrupt = flowInterruptToProto(e.Interrupt)
	}

	return proto
}

func protoToEnvelope(p *pb.Envelope) *envelope.Envelope {
	e := envelope.NewEnvelope()

	e.EnvelopeID = p.EnvelopeId
	e.RequestID = p.RequestId
	e.UserID = p.UserId
	e.SessionID = p.SessionId
	e.RawInput = p.RawInput
	e.ReceivedAt = time.UnixMilli(p.ReceivedAtMs)
	e.CurrentStage = p.CurrentStage
	e.StageOrder = p.StageOrder
	e.Iteration = int(p.Iteration)
	e.MaxIterations = int(p.MaxIterations)
	e.LLMCallCount = int(p.LlmCallCount)
	e.MaxLLMCalls = int(p.MaxLlmCalls)
	e.AgentHopCount = int(p.AgentHopCount)
	e.MaxAgentHops = int(p.MaxAgentHops)
	e.Terminated = p.Terminated
	e.InterruptPending = p.InterruptPending
	e.CurrentStageNumber = int(p.CurrentStageNumber)
	e.MaxStages = int(p.MaxStages)
	e.AllGoals = p.AllGoals
	e.RemainingGoals = p.RemainingGoals
	e.CreatedAt = time.UnixMilli(p.CreatedAtMs)

	// Termination
	if p.TerminationReason != "" {
		e.TerminationReason = &p.TerminationReason
	}
	if p.TerminalReason != pb.TerminalReason_TERMINAL_REASON_UNSPECIFIED {
		reason := protoToTerminalReason(p.TerminalReason)
		e.TerminalReason_ = &reason
	}
	if p.CompletedAtMs > 0 {
		t := time.UnixMilli(p.CompletedAtMs)
		e.CompletedAt = &t
	}

	// Outputs
	e.Outputs = make(map[string]map[string]any)
	for k, v := range p.Outputs {
		var output map[string]any
		if err := json.Unmarshal(v, &output); err == nil {
			e.Outputs[k] = output
		}
	}

	// Parallel execution state
	e.ActiveStages = p.ActiveStages
	e.CompletedStageSet = p.CompletedStageSet
	e.FailedStages = p.FailedStages

	// Goal completion status
	e.GoalCompletionStatus = p.GoalCompletionStatus

	// Metadata
	e.Metadata = make(map[string]any)
	for k, v := range p.MetadataStr {
		e.Metadata[k] = v
	}

	// Interrupt
	if p.Interrupt != nil {
		e.Interrupt = protoToFlowInterrupt(p.Interrupt)
	}

	return e
}

func flowInterruptToProto(i *envelope.FlowInterrupt) *pb.FlowInterrupt {
	proto := &pb.FlowInterrupt{
		Kind:        interruptKindToProto(i.Kind),
		InterruptId: i.ID,
		Question:    i.Question,
		Message:     i.Message,
		CreatedAtMs: i.CreatedAt.UnixMilli(),
	}

	if i.Data != nil {
		if data, err := json.Marshal(i.Data); err == nil {
			proto.Data = data
		}
	}

	if i.Response != nil {
		proto.Response = &pb.InterruptResponse{
			ResolvedAtMs: i.Response.ReceivedAt.UnixMilli(),
		}
		if i.Response.Text != nil {
			proto.Response.Text = *i.Response.Text
		}
		if i.Response.Approved != nil {
			proto.Response.Approved = *i.Response.Approved
		}
		if i.Response.Decision != nil {
			proto.Response.Decision = *i.Response.Decision
		}
		if i.Response.Data != nil {
			if data, err := json.Marshal(i.Response.Data); err == nil {
				proto.Response.Data = data
			}
		}
	}

	return proto
}

func protoToFlowInterrupt(p *pb.FlowInterrupt) *envelope.FlowInterrupt {
	i := &envelope.FlowInterrupt{
		Kind:      protoToInterruptKind(p.Kind),
		ID:        p.InterruptId,
		Question:  p.Question,
		Message:   p.Message,
		CreatedAt: time.UnixMilli(p.CreatedAtMs),
	}

	if len(p.Data) > 0 {
		var data map[string]any
		if err := json.Unmarshal(p.Data, &data); err == nil {
			i.Data = data
		}
	}

	if p.Response != nil {
		i.Response = &envelope.InterruptResponse{
			ReceivedAt: time.UnixMilli(p.Response.ResolvedAtMs),
		}
		if p.Response.Text != "" {
			i.Response.Text = &p.Response.Text
		}
		if p.Response.Approved {
			i.Response.Approved = &p.Response.Approved
		}
		if p.Response.Decision != "" {
			i.Response.Decision = &p.Response.Decision
		}
		if len(p.Response.Data) > 0 {
			var data map[string]any
			if err := json.Unmarshal(p.Response.Data, &data); err == nil {
				i.Response.Data = data
			}
		}
	}

	return i
}

func terminalReasonToProto(r envelope.TerminalReason) pb.TerminalReason {
	switch r {
	case envelope.TerminalReasonMaxIterationsExceeded:
		return pb.TerminalReason_MAX_ITERATIONS_EXCEEDED
	case envelope.TerminalReasonMaxLLMCallsExceeded:
		return pb.TerminalReason_MAX_LLM_CALLS_EXCEEDED
	case envelope.TerminalReasonMaxAgentHopsExceeded:
		return pb.TerminalReason_MAX_AGENT_HOPS_EXCEEDED
	case envelope.TerminalReasonUserCancelled:
		return pb.TerminalReason_USER_CANCELLED
	case envelope.TerminalReasonToolFailedFatally:
		return pb.TerminalReason_TOOL_FAILED_FATALLY
	case envelope.TerminalReasonLLMFailedFatally:
		return pb.TerminalReason_LLM_FAILED_FATALLY
	case envelope.TerminalReasonCompleted:
		return pb.TerminalReason_COMPLETED
	default:
		return pb.TerminalReason_TERMINAL_REASON_UNSPECIFIED
	}
}

func protoToTerminalReason(r pb.TerminalReason) envelope.TerminalReason {
	switch r {
	case pb.TerminalReason_MAX_ITERATIONS_EXCEEDED:
		return envelope.TerminalReasonMaxIterationsExceeded
	case pb.TerminalReason_MAX_LLM_CALLS_EXCEEDED:
		return envelope.TerminalReasonMaxLLMCallsExceeded
	case pb.TerminalReason_MAX_AGENT_HOPS_EXCEEDED:
		return envelope.TerminalReasonMaxAgentHopsExceeded
	case pb.TerminalReason_USER_CANCELLED:
		return envelope.TerminalReasonUserCancelled
	case pb.TerminalReason_TOOL_FAILED_FATALLY:
		return envelope.TerminalReasonToolFailedFatally
	case pb.TerminalReason_LLM_FAILED_FATALLY:
		return envelope.TerminalReasonLLMFailedFatally
	case pb.TerminalReason_COMPLETED:
		return envelope.TerminalReasonCompleted
	default:
		return envelope.TerminalReasonCompleted
	}
}

func interruptKindToProto(k envelope.InterruptKind) pb.InterruptKind {
	switch k {
	case envelope.InterruptKindClarification:
		return pb.InterruptKind_CLARIFICATION
	case envelope.InterruptKindConfirmation:
		return pb.InterruptKind_CONFIRMATION
	case envelope.InterruptKindCheckpoint:
		return pb.InterruptKind_CHECKPOINT
	case envelope.InterruptKindResourceExhausted:
		return pb.InterruptKind_RESOURCE_EXHAUSTED
	case envelope.InterruptKindTimeout:
		return pb.InterruptKind_TIMEOUT
	case envelope.InterruptKindSystemError:
		return pb.InterruptKind_SYSTEM_ERROR
	case envelope.InterruptKindAgentReview:
		return pb.InterruptKind_AGENT_REVIEW
	default:
		return pb.InterruptKind_INTERRUPT_KIND_UNSPECIFIED
	}
}

func protoToInterruptKind(k pb.InterruptKind) envelope.InterruptKind {
	switch k {
	case pb.InterruptKind_CLARIFICATION:
		return envelope.InterruptKindClarification
	case pb.InterruptKind_CONFIRMATION:
		return envelope.InterruptKindConfirmation
	case pb.InterruptKind_CHECKPOINT:
		return envelope.InterruptKindCheckpoint
	case pb.InterruptKind_RESOURCE_EXHAUSTED:
		return envelope.InterruptKindResourceExhausted
	case pb.InterruptKind_TIMEOUT:
		return envelope.InterruptKindTimeout
	case pb.InterruptKind_SYSTEM_ERROR:
		return envelope.InterruptKindSystemError
	case pb.InterruptKind_AGENT_REVIEW:
		return envelope.InterruptKindAgentReview
	default:
		return envelope.InterruptKindClarification
	}
}

// =============================================================================
// Server Lifecycle
// =============================================================================

// Start starts the gRPC server on the given address.
func Start(address string, server *EngineServer) error {
	lis, err := net.Listen("tcp", address)
	if err != nil {
		return fmt.Errorf("failed to listen: %w", err)
	}

	grpcServer := grpc.NewServer()
	pb.RegisterEngineServiceServer(grpcServer, server)

	server.logger.Info("grpc_server_started", "address", address)
	return grpcServer.Serve(lis)
}

// StartBackground starts the gRPC server in a goroutine.
func StartBackground(address string, server *EngineServer) (*grpc.Server, error) {
	lis, err := net.Listen("tcp", address)
	if err != nil {
		return nil, fmt.Errorf("failed to listen: %w", err)
	}

	grpcServer := grpc.NewServer()
	pb.RegisterEngineServiceServer(grpcServer, server)

	go func() {
		if err := grpcServer.Serve(lis); err != nil {
			server.logger.Error("grpc_server_error", "error", err.Error())
		}
	}()

	server.logger.Info("grpc_server_started_background", "address", address)
	return grpcServer, nil
}

// =============================================================================
// Graceful Server
// =============================================================================

// GracefulServer wraps a gRPC server with graceful shutdown support.
// It listens for context cancellation and shuts down cleanly.
type GracefulServer struct {
	grpcServer  *grpc.Server
	coreServer  *EngineServer
	address     string
	listener    net.Listener
	shutdownMu  sync.Mutex
	isShutdown  bool
}

// NewGracefulServer creates a new GracefulServer with interceptors.
func NewGracefulServer(coreServer *EngineServer, address string, opts ...grpc.ServerOption) (*GracefulServer, error) {
	// Add default interceptors if none provided
	if len(opts) == 0 {
		opts = ServerOptions(coreServer.logger)
	}

	grpcServer := grpc.NewServer(opts...)
	pb.RegisterEngineServiceServer(grpcServer, coreServer)

	return &GracefulServer{
		grpcServer: grpcServer,
		coreServer: coreServer,
		address:    address,
	}, nil
}

// Start starts the server and blocks until ctx is cancelled.
// When ctx is cancelled, it performs graceful shutdown.
func (s *GracefulServer) Start(ctx context.Context) error {
	lis, err := net.Listen("tcp", s.address)
	if err != nil {
		return fmt.Errorf("failed to listen: %w", err)
	}
	s.listener = lis

	s.coreServer.logger.Info("grpc_graceful_server_started",
		"address", s.address,
	)

	// Start serving in a goroutine
	errCh := make(chan error, 1)
	go func() {
		if err := s.grpcServer.Serve(lis); err != nil {
			errCh <- err
		}
		close(errCh)
	}()

	// Wait for context cancellation or server error
	select {
	case <-ctx.Done():
		s.coreServer.logger.Info("grpc_graceful_shutdown_initiated",
			"reason", ctx.Err().Error(),
		)
		s.GracefulStop()
		return ctx.Err()
	case err := <-errCh:
		if err != nil {
			return fmt.Errorf("server error: %w", err)
		}
		return nil
	}
}

// StartBackground starts the server in a goroutine.
// Returns a channel that receives errors.
func (s *GracefulServer) StartBackground() (<-chan error, error) {
	lis, err := net.Listen("tcp", s.address)
	if err != nil {
		return nil, fmt.Errorf("failed to listen: %w", err)
	}
	s.listener = lis

	s.coreServer.logger.Info("grpc_graceful_server_started_background",
		"address", s.address,
	)

	errCh := make(chan error, 1)
	go func() {
		if err := s.grpcServer.Serve(lis); err != nil {
			errCh <- err
		}
		close(errCh)
	}()

	return errCh, nil
}

// GracefulStop gracefully stops the server.
// It stops accepting new connections and waits for existing ones to complete.
func (s *GracefulServer) GracefulStop() {
	s.shutdownMu.Lock()
	defer s.shutdownMu.Unlock()

	if s.isShutdown {
		return
	}
	s.isShutdown = true

	s.coreServer.logger.Info("grpc_graceful_stop_started")
	s.grpcServer.GracefulStop()
	s.coreServer.logger.Info("grpc_graceful_stop_completed")
}

// Stop immediately stops the server.
// Use GracefulStop for production; this is for emergency shutdown.
func (s *GracefulServer) Stop() {
	s.shutdownMu.Lock()
	defer s.shutdownMu.Unlock()

	if s.isShutdown {
		return
	}
	s.isShutdown = true

	s.coreServer.logger.Warn("grpc_immediate_stop")
	s.grpcServer.Stop()
}

// ShutdownWithTimeout performs graceful shutdown with a timeout.
// If shutdown doesn't complete within timeout, it forces an immediate stop.
func (s *GracefulServer) ShutdownWithTimeout(timeout time.Duration) {
	done := make(chan struct{})

	go func() {
		s.GracefulStop()
		close(done)
	}()

	select {
	case <-done:
		// Graceful shutdown completed
		return
	case <-time.After(timeout):
		s.coreServer.logger.Warn("grpc_graceful_shutdown_timeout",
			"timeout_ms", timeout.Milliseconds(),
		)
		s.grpcServer.Stop()
	}
}

// GetGRPCServer returns the underlying grpc.Server.
func (s *GracefulServer) GetGRPCServer() *grpc.Server {
	return s.grpcServer
}

// Address returns the server address.
func (s *GracefulServer) Address() string {
	return s.address
}

