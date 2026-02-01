// Package grpc provides the OrchestrationService gRPC server.
// This exposes kernel-driven pipeline orchestration via gRPC.
package grpc

import (
	"context"
	"encoding/json"
	"strings"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/kernel"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
)

// OrchestrationServer implements the gRPC OrchestrationService.
// Thread-safe: delegates to Orchestrator which handles synchronization.
type OrchestrationServer struct {
	pb.UnimplementedOrchestrationServiceServer

	logger       Logger
	orchestrator *kernel.Orchestrator
}

// NewOrchestrationServer creates a new OrchestrationService gRPC server.
func NewOrchestrationServer(logger Logger, orch *kernel.Orchestrator) *OrchestrationServer {
	return &OrchestrationServer{
		logger:       logger,
		orchestrator: orch,
	}
}

// =============================================================================
// OrchestrationService Implementation
// =============================================================================

// InitializeSession initializes a new orchestration session.
func (s *OrchestrationServer) InitializeSession(
	ctx context.Context,
	req *pb.InitializeSessionRequest,
) (*pb.SessionState, error) {
	if err := CheckContextDeadline(ctx); err != nil {
		return nil, err
	}
	if req.ProcessId == "" {
		return nil, status.Error(codes.InvalidArgument, "process_id is required")
	}

	// Parse pipeline config
	var pipelineConfig config.PipelineConfig
	if err := json.Unmarshal(req.PipelineConfig, &pipelineConfig); err != nil {
		s.logger.Error("invalid_pipeline_config",
			"process_id", req.ProcessId,
			"error", err.Error(),
		)
		return nil, status.Errorf(codes.InvalidArgument, "invalid pipeline config: %v", err)
	}

	// Parse envelope
	var envDict map[string]any
	if err := json.Unmarshal(req.Envelope, &envDict); err != nil {
		s.logger.Error("invalid_envelope",
			"process_id", req.ProcessId,
			"error", err.Error(),
		)
		return nil, status.Errorf(codes.InvalidArgument, "invalid envelope: %v", err)
	}
	env := envelope.FromStateDict(envDict)

	// Initialize session with K8s-style duplicate handling
	state, err := s.orchestrator.InitializeSession(req.ProcessId, &pipelineConfig, env, req.GetForce())
	if err != nil {
		s.logger.Error("initialize_session_failed",
			"process_id", req.ProcessId,
			"error", err.Error(),
		)
		// Return ALREADY_EXISTS for duplicate session errors
		if strings.Contains(err.Error(), "session already exists") {
			return nil, status.Errorf(codes.AlreadyExists, "%v", err)
		}
		return nil, status.Errorf(codes.Internal, "failed to initialize session: %v", err)
	}

	s.logger.Info("orchestration_session_initialized",
		"process_id", req.ProcessId,
		"current_stage", state.CurrentStage,
	)

	return sessionStateToProto(state), nil
}

// GetNextInstruction gets the next instruction for a process.
func (s *OrchestrationServer) GetNextInstruction(
	ctx context.Context,
	req *pb.GetNextInstructionRequest,
) (*pb.Instruction, error) {
	if err := CheckContextDeadline(ctx); err != nil {
		return nil, err
	}
	if req.ProcessId == "" {
		return nil, status.Error(codes.InvalidArgument, "process_id is required")
	}

	instruction, err := s.orchestrator.GetNextInstruction(req.ProcessId)
	if err != nil {
		s.logger.Warn("get_next_instruction_failed",
			"process_id", req.ProcessId,
			"error", err.Error(),
		)
		return nil, status.Errorf(codes.NotFound, "process not found: %v", err)
	}

	s.logger.Debug("next_instruction_returned",
		"process_id", req.ProcessId,
		"kind", instruction.Kind,
		"agent", instruction.AgentName,
	)

	return instructionToProto(instruction), nil
}

// ReportAgentResult reports agent execution result and gets next instruction.
func (s *OrchestrationServer) ReportAgentResult(
	ctx context.Context,
	req *pb.ReportAgentResultRequest,
) (*pb.Instruction, error) {
	if err := CheckContextDeadline(ctx); err != nil {
		return nil, err
	}
	if req.ProcessId == "" {
		return nil, status.Error(codes.InvalidArgument, "process_id is required")
	}
	if req.AgentName == "" {
		return nil, status.Error(codes.InvalidArgument, "agent_name is required")
	}

	// Parse output
	var output map[string]any
	if len(req.Output) > 0 {
		if err := json.Unmarshal(req.Output, &output); err != nil {
			s.logger.Warn("invalid_agent_output",
				"process_id", req.ProcessId,
				"agent", req.AgentName,
				"error", err.Error(),
			)
			// Continue with nil output - not fatal
		}
	}

	// Convert metrics
	var metrics *kernel.AgentExecutionMetrics
	if req.Metrics != nil {
		metrics = &kernel.AgentExecutionMetrics{
			LLMCalls:   int(req.Metrics.LlmCalls),
			ToolCalls:  int(req.Metrics.ToolCalls),
			TokensIn:   int(req.Metrics.TokensIn),
			TokensOut:  int(req.Metrics.TokensOut),
			DurationMS: int(req.Metrics.DurationMs),
		}
	}

	instruction, err := s.orchestrator.ProcessAgentResult(
		req.ProcessId,
		req.AgentName,
		output,
		metrics,
		req.Success,
		req.Error,
	)
	if err != nil {
		s.logger.Error("process_agent_result_failed",
			"process_id", req.ProcessId,
			"agent", req.AgentName,
			"error", err.Error(),
		)
		return nil, status.Errorf(codes.Internal, "failed to process result: %v", err)
	}

	s.logger.Debug("agent_result_processed",
		"process_id", req.ProcessId,
		"agent", req.AgentName,
		"success", req.Success,
		"next_kind", instruction.Kind,
	)

	return instructionToProto(instruction), nil
}

// GetSessionState gets the current session state.
func (s *OrchestrationServer) GetSessionState(
	ctx context.Context,
	req *pb.GetSessionStateRequest,
) (*pb.SessionState, error) {
	if err := CheckContextDeadline(ctx); err != nil {
		return nil, err
	}
	if req.ProcessId == "" {
		return nil, status.Error(codes.InvalidArgument, "process_id is required")
	}

	state, err := s.orchestrator.GetSessionState(req.ProcessId)
	if err != nil {
		return nil, status.Errorf(codes.NotFound, "session not found: %v", err)
	}

	return sessionStateToProto(state), nil
}

// =============================================================================
// Conversion Functions
// =============================================================================

func instructionToProto(i *kernel.Instruction) *pb.Instruction {
	proto := &pb.Instruction{
		Kind:               instructionKindToProto(i.Kind),
		AgentName:          i.AgentName,
		TerminationMessage: i.TerminationMessage,
		InterruptPending:   i.InterruptPending,
	}

	// Agent config (JSON-encoded)
	if i.AgentConfig != nil {
		if data, err := json.Marshal(i.AgentConfig); err == nil {
			proto.AgentConfig = data
		}
	}

	// Envelope (JSON-encoded)
	if i.Envelope != nil {
		if data, err := json.Marshal(i.Envelope.ToStateDict()); err == nil {
			proto.Envelope = data
		}
	}

	// Terminal reason
	if i.TerminalReason != nil {
		proto.TerminalReason = terminalReasonToProto(*i.TerminalReason)
	}

	// Interrupt
	if i.Interrupt != nil {
		proto.Interrupt = flowInterruptToProto(i.Interrupt)
	}

	return proto
}

func instructionKindToProto(k kernel.InstructionKind) pb.InstructionKind {
	switch k {
	case kernel.InstructionKindRunAgent:
		return pb.InstructionKind_RUN_AGENT
	case kernel.InstructionKindTerminate:
		return pb.InstructionKind_TERMINATE
	case kernel.InstructionKindWaitInterrupt:
		return pb.InstructionKind_WAIT_INTERRUPT
	default:
		return pb.InstructionKind_INSTRUCTION_KIND_UNSPECIFIED
	}
}

func sessionStateToProto(s *kernel.SessionState) *pb.SessionState {
	proto := &pb.SessionState{
		ProcessId:      s.ProcessID,
		CurrentStage:   s.CurrentStage,
		StageOrder:     s.StageOrder,
		EdgeTraversals: make(map[string]int32),
		Terminated:     s.Terminated,
	}

	// Envelope (JSON-encoded)
	if s.Envelope != nil {
		if data, err := json.Marshal(s.Envelope); err == nil {
			proto.Envelope = data
		}
	}

	// Edge traversals
	for k, v := range s.EdgeTraversals {
		proto.EdgeTraversals[k] = int32(v)
	}

	// Terminal reason
	if s.TerminalReason != nil {
		proto.TerminalReason = terminalReasonToProto(*s.TerminalReason)
	}

	return proto
}
