// Package grpc provides the KernelService gRPC server.
// This exposes the kernel's process lifecycle and resource management via gRPC.
package grpc

import (
	"context"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/kernel"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
)

// KernelServer implements the gRPC KernelService.
// Thread-safe: delegates to kernel which handles synchronization.
type KernelServer struct {
	pb.UnimplementedKernelServiceServer

	logger Logger
	kernel *kernel.Kernel
}

// NewKernelServer creates a new KernelService gRPC server.
func NewKernelServer(logger Logger, k *kernel.Kernel) *KernelServer {
	return &KernelServer{
		logger: logger,
		kernel: k,
	}
}

// =============================================================================
// Process Lifecycle
// =============================================================================

// CreateProcess creates a new process in NEW state.
func (s *KernelServer) CreateProcess(
	ctx context.Context,
	req *pb.CreateProcessRequest,
) (*pb.ProcessControlBlock, error) {
	// Syscall validation boundary
	if err := validateRequired(req.Pid, "pid"); err != nil {
		return nil, err
	}

	// Validate request context (user identity)
	reqCtx, err := ExtractRequestContext(ctx, req)
	if err != nil {
		s.logger.Warn("invalid_request_context",
			"pid", req.Pid,
			"error", err.Error(),
		)
		return nil, err
	}

	quota := protoToResourceQuota(req.Quota)
	priority := protoToSchedulingPriority(req.Priority)

	// Enter kernel code
	pcb, err := s.kernel.Submit(
		req.Pid,
		reqCtx.RequestID,
		reqCtx.UserID,
		reqCtx.SessionID,
		priority,
		quota,
	)
	if err != nil {
		s.logger.Error("create_process_failed",
			"pid", req.Pid,
			"error", err.Error(),
		)
		return nil, Internal("create process", err)
	}

	s.logger.Debug("process_created",
		"pid", req.Pid,
		"user_id", reqCtx.UserID,
		"request_id", reqCtx.RequestID,
	)

	return pcbToProto(pcb), nil
}

// GetProcess retrieves a process by ID.
func (s *KernelServer) GetProcess(
	ctx context.Context,
	req *pb.GetProcessRequest,
) (*pb.ProcessControlBlock, error) {
	if err := validateRequired(req.Pid, "pid"); err != nil {
		return nil, err
	}

	// Extract request context for ownership validation
	reqCtx, err := ExtractRequestContext(ctx, req)
	if err != nil {
		return nil, err
	}

	pcb := s.kernel.GetProcess(req.Pid)
	if pcb == nil {
		return nil, NotFound("process", req.Pid)
	}

	// Validate ownership - agent A cannot access agent B's processes
	if err := ValidateProcessOwnership(pcb, reqCtx.UserID); err != nil {
		s.logger.Warn("ownership_violation",
			"pid", req.Pid,
			"requesting_user", reqCtx.UserID,
			"owner_user", pcb.UserID,
		)
		return nil, err
	}

	return pcbToProto(pcb), nil
}

// ScheduleProcess schedules a process for execution (NEW -> READY).
func (s *KernelServer) ScheduleProcess(
	ctx context.Context,
	req *pb.ScheduleProcessRequest,
) (*pb.ProcessControlBlock, error) {
	if err := validateRequired(req.Pid, "pid"); err != nil {
		return nil, err
	}

	// Extract request context for ownership validation
	reqCtx, err := ExtractRequestContext(ctx, req)
	if err != nil {
		return nil, err
	}

	// Get process for ownership check
	pcb := s.kernel.GetProcess(req.Pid)
	if pcb == nil {
		return nil, NotFound("process", req.Pid)
	}

	// Validate ownership
	if err := ValidateProcessOwnership(pcb, reqCtx.UserID); err != nil {
		s.logger.Warn("ownership_violation",
			"operation", "schedule",
			"pid", req.Pid,
			"requesting_user", reqCtx.UserID,
			"owner_user", pcb.UserID,
		)
		return nil, err
	}

	// Pre-flight quota check - prevent scheduling if quota exhausted
	if err := ValidateQuotaAvailable(s.kernel, req.Pid); err != nil {
		s.logger.Warn("quota_preflight_failed",
			"pid", req.Pid,
			"error", err.Error(),
		)
		return nil, err
	}

	if err := s.kernel.Schedule(req.Pid); err != nil {
		s.logger.Error("schedule_process_failed",
			"pid", req.Pid,
			"error", err.Error(),
		)
		return nil, Internal("schedule process", err)
	}

	pcb = s.kernel.GetProcess(req.Pid)
	if pcb == nil {
		return nil, NotFound("process", req.Pid)
	}

	s.logger.Debug("process_scheduled", "pid", req.Pid)

	return pcbToProto(pcb), nil
}

// GetNextRunnable returns the next process to run (READY -> RUNNING).
func (s *KernelServer) GetNextRunnable(
	ctx context.Context,
	req *pb.GetNextRunnableRequest,
) (*pb.ProcessControlBlock, error) {
	pcb := s.kernel.GetNextRunnable()
	if pcb == nil {
		return nil, status.Error(codes.NotFound, "no runnable process")
	}

	s.logger.Debug("next_runnable", "pid", pcb.PID)

	return pcbToProto(pcb), nil
}

// TransitionState transitions a process to a new state.
func (s *KernelServer) TransitionState(
	ctx context.Context,
	req *pb.TransitionStateRequest,
) (*pb.ProcessControlBlock, error) {
	if err := validateRequired(req.Pid, "pid"); err != nil {
		return nil, err
	}

	// Extract request context for ownership validation
	reqCtx, err := ExtractRequestContext(ctx, req)
	if err != nil {
		return nil, err
	}

	// Get process for ownership check
	pcb := s.kernel.GetProcess(req.Pid)
	if pcb == nil {
		return nil, NotFound("process", req.Pid)
	}

	// Validate ownership
	if err := ValidateProcessOwnership(pcb, reqCtx.UserID); err != nil {
		s.logger.Warn("ownership_violation",
			"operation", "transition_state",
			"pid", req.Pid,
			"requesting_user", reqCtx.UserID,
			"owner_user", pcb.UserID,
		)
		return nil, err
	}

	newState := protoToProcessState(req.NewState)
	if err := s.kernel.TransitionState(req.Pid, newState, req.Reason); err != nil {
		s.logger.Error("transition_state_failed",
			"pid", req.Pid,
			"new_state", newState,
			"error", err.Error(),
		)
		return nil, Internal("transition state", err)
	}

	pcb = s.kernel.GetProcess(req.Pid)
	if pcb == nil {
		return nil, NotFound("process", req.Pid)
	}

	s.logger.Debug("state_transitioned",
		"pid", req.Pid,
		"new_state", string(newState),
	)

	return pcbToProto(pcb), nil
}

// TerminateProcess terminates a process.
func (s *KernelServer) TerminateProcess(
	ctx context.Context,
	req *pb.TerminateProcessRequest,
) (*pb.ProcessControlBlock, error) {
	if err := validateRequired(req.Pid, "pid"); err != nil {
		return nil, err
	}

	// Extract request context for ownership validation
	reqCtx, err := ExtractRequestContext(ctx, req)
	if err != nil {
		return nil, err
	}

	// Get process for ownership check
	pcb := s.kernel.GetProcess(req.Pid)
	if pcb == nil {
		return nil, NotFound("process", req.Pid)
	}

	// Validate ownership
	if err := ValidateProcessOwnership(pcb, reqCtx.UserID); err != nil {
		s.logger.Warn("ownership_violation",
			"operation", "terminate",
			"pid", req.Pid,
			"requesting_user", reqCtx.UserID,
			"owner_user", pcb.UserID,
		)
		return nil, err
	}

	if err := s.kernel.Terminate(req.Pid, req.Reason, req.Force); err != nil {
		s.logger.Error("terminate_process_failed",
			"pid", req.Pid,
			"error", err.Error(),
		)
		return nil, Internal("terminate process", err)
	}

	pcb = s.kernel.GetProcess(req.Pid)
	if pcb == nil {
		return nil, NotFound("process", req.Pid)
	}

	s.logger.Debug("process_terminated",
		"pid", req.Pid,
		"reason", req.Reason,
	)

	return pcbToProto(pcb), nil
}

// =============================================================================
// Resource Management
// =============================================================================

// CheckQuota checks if a process is within its resource quota.
func (s *KernelServer) CheckQuota(
	ctx context.Context,
	req *pb.CheckQuotaRequest,
) (*pb.QuotaResult, error) {
	if err := validateRequired(req.Pid, "pid"); err != nil {
		return nil, err
	}

	exceeded := s.kernel.CheckQuota(req.Pid)
	usage := s.kernel.GetUsage(req.Pid)
	quota := s.kernel.Resources().GetQuota(req.Pid)

	result := &pb.QuotaResult{
		WithinBounds:   exceeded == "",
		ExceededReason: exceeded,
	}

	if usage != nil {
		result.Usage = resourceUsageToProto(usage)
	}
	if quota != nil {
		result.Quota = resourceQuotaToProto(quota)
	}

	return result, nil
}

// RecordUsage records resource usage for a process.
func (s *KernelServer) RecordUsage(
	ctx context.Context,
	req *pb.RecordUsageRequest,
) (*pb.ResourceUsage, error) {
	if err := validateRequired(req.Pid, "pid"); err != nil {
		return nil, err
	}

	usage := s.kernel.Resources().RecordUsage(
		req.Pid,
		int(req.LlmCalls),
		int(req.ToolCalls),
		int(req.AgentHops),
		int(req.TokensIn),
		int(req.TokensOut),
	)

	// Record inference usage if present
	if req.InferenceRequests > 0 || req.InferenceInputChars > 0 {
		usage = s.kernel.Resources().RecordInferenceCall(
			req.Pid,
			int(req.InferenceInputChars),
		)
	}

	s.logger.Debug("usage_recorded",
		"pid", req.Pid,
		"llm_calls", req.LlmCalls,
		"tool_calls", req.ToolCalls,
		"inference_requests", req.InferenceRequests,
	)

	return resourceUsageToProto(usage), nil
}

// CheckRateLimit checks rate limits for a user/endpoint.
func (s *KernelServer) CheckRateLimit(
	ctx context.Context,
	req *pb.CheckRateLimitRequest,
) (*pb.RateLimitResult, error) {
	result := s.kernel.CheckRateLimit(req.UserId, req.Endpoint, req.Record)

	return &pb.RateLimitResult{
		Allowed:           result.Allowed,
		Exceeded:          result.Exceeded,
		LimitType:         result.LimitType,
		CurrentCount:      int32(result.Current),
		Limit:             int32(result.Limit),
		RetryAfterSeconds: float32(result.RetryAfter),
		Remaining:         int32(result.Remaining),
	}, nil
}

// =============================================================================
// Queries
// =============================================================================

// ListProcesses lists processes matching criteria.
func (s *KernelServer) ListProcesses(
	ctx context.Context,
	req *pb.ListProcessesRequest,
) (*pb.ListProcessesResponse, error) {
	var stateFilter *kernel.ProcessState
	if req.State != pb.ProcessState_PROCESS_STATE_UNSPECIFIED {
		state := protoToProcessState(req.State)
		stateFilter = &state
	}

	processes := s.kernel.ListProcesses(stateFilter, req.UserId)

	response := &pb.ListProcessesResponse{
		Processes: make([]*pb.ProcessControlBlock, len(processes)),
	}

	for i, pcb := range processes {
		response.Processes[i] = pcbToProto(pcb)
	}

	return response, nil
}

// GetProcessCounts returns process counts by state.
func (s *KernelServer) GetProcessCounts(
	ctx context.Context,
	req *pb.GetProcessCountsRequest,
) (*pb.ProcessCountsResponse, error) {
	counts := s.kernel.Lifecycle().GetProcessCount()
	total := s.kernel.Lifecycle().GetTotalProcesses()
	queueDepth := s.kernel.Lifecycle().GetQueueDepth()

	countsByState := make(map[string]int32)
	for state, count := range counts {
		countsByState[string(state)] = int32(count)
	}

	return &pb.ProcessCountsResponse{
		CountsByState: countsByState,
		Total:         int32(total),
		QueueDepth:    int32(queueDepth),
	}, nil
}

// =============================================================================
// Conversion Functions
// =============================================================================

func pcbToProto(pcb *kernel.ProcessControlBlock) *pb.ProcessControlBlock {
	proto := &pb.ProcessControlBlock{
		Pid:          pcb.PID,
		RequestId:    pcb.RequestID,
		UserId:       pcb.UserID,
		SessionId:    pcb.SessionID,
		State:        processStateToProto(pcb.State),
		Priority:     schedulingPriorityToProto(pcb.Priority),
		CurrentStage: pcb.CurrentStage,
		ParentPid:    pcb.ParentPID,
		ChildPids:    pcb.ChildPIDs,
	}

	if pcb.Quota != nil {
		proto.Quota = resourceQuotaToProto(pcb.Quota)
	}

	if pcb.Usage != nil {
		proto.Usage = resourceUsageToProto(pcb.Usage)
	}

	proto.CreatedAtMs = pcb.CreatedAt.UnixMilli()
	if pcb.StartedAt != nil {
		proto.StartedAtMs = pcb.StartedAt.UnixMilli()
	}
	if pcb.CompletedAt != nil {
		proto.CompletedAtMs = pcb.CompletedAt.UnixMilli()
	}

	return proto
}

func resourceQuotaToProto(q *kernel.ResourceQuota) *pb.ResourceQuota {
	return &pb.ResourceQuota{
		MaxInputTokens:         int32(q.MaxInputTokens),
		MaxOutputTokens:        int32(q.MaxOutputTokens),
		MaxContextTokens:       int32(q.MaxContextTokens),
		MaxLlmCalls:            int32(q.MaxLLMCalls),
		MaxToolCalls:           int32(q.MaxToolCalls),
		MaxAgentHops:           int32(q.MaxAgentHops),
		MaxIterations:          int32(q.MaxIterations),
		TimeoutSeconds:         int32(q.TimeoutSeconds),
		SoftTimeoutSeconds:     int32(q.SoftTimeoutSeconds),
		RateLimitRpm:           int32(q.RateLimitRPM),
		RateLimitRph:           int32(q.RateLimitRPH),
		RateLimitBurst:         int32(q.RateLimitBurst),
		MaxInferenceRequests:   int32(q.MaxInferenceRequests),
		MaxInferenceInputChars: int32(q.MaxInferenceInputChars),
	}
}

func protoToResourceQuota(p *pb.ResourceQuota) *kernel.ResourceQuota {
	if p == nil {
		return nil
	}
	return &kernel.ResourceQuota{
		MaxInputTokens:         int(p.MaxInputTokens),
		MaxOutputTokens:        int(p.MaxOutputTokens),
		MaxContextTokens:       int(p.MaxContextTokens),
		MaxLLMCalls:            int(p.MaxLlmCalls),
		MaxToolCalls:           int(p.MaxToolCalls),
		MaxAgentHops:           int(p.MaxAgentHops),
		MaxIterations:          int(p.MaxIterations),
		TimeoutSeconds:         int(p.TimeoutSeconds),
		SoftTimeoutSeconds:     int(p.SoftTimeoutSeconds),
		RateLimitRPM:           int(p.RateLimitRpm),
		RateLimitRPH:           int(p.RateLimitRph),
		RateLimitBurst:         int(p.RateLimitBurst),
		MaxInferenceRequests:   int(p.MaxInferenceRequests),
		MaxInferenceInputChars: int(p.MaxInferenceInputChars),
	}
}

func resourceUsageToProto(u *kernel.ResourceUsage) *pb.ResourceUsage {
	return &pb.ResourceUsage{
		LlmCalls:            int32(u.LLMCalls),
		ToolCalls:           int32(u.ToolCalls),
		AgentHops:           int32(u.AgentHops),
		Iterations:          int32(u.Iterations),
		TokensIn:            int32(u.TokensIn),
		TokensOut:           int32(u.TokensOut),
		ElapsedSeconds:      u.ElapsedSeconds,
		InferenceRequests:   int32(u.InferenceRequests),
		InferenceInputChars: int32(u.InferenceInputChars),
	}
}

func processStateToProto(s kernel.ProcessState) pb.ProcessState {
	switch s {
	case kernel.ProcessStateNew:
		return pb.ProcessState_PROCESS_STATE_NEW
	case kernel.ProcessStateReady:
		return pb.ProcessState_PROCESS_STATE_READY
	case kernel.ProcessStateRunning:
		return pb.ProcessState_PROCESS_STATE_RUNNING
	case kernel.ProcessStateWaiting:
		return pb.ProcessState_PROCESS_STATE_WAITING
	case kernel.ProcessStateBlocked:
		return pb.ProcessState_PROCESS_STATE_BLOCKED
	case kernel.ProcessStateTerminated:
		return pb.ProcessState_PROCESS_STATE_TERMINATED
	case kernel.ProcessStateZombie:
		return pb.ProcessState_PROCESS_STATE_ZOMBIE
	default:
		return pb.ProcessState_PROCESS_STATE_UNSPECIFIED
	}
}

func protoToProcessState(p pb.ProcessState) kernel.ProcessState {
	switch p {
	case pb.ProcessState_PROCESS_STATE_NEW:
		return kernel.ProcessStateNew
	case pb.ProcessState_PROCESS_STATE_READY:
		return kernel.ProcessStateReady
	case pb.ProcessState_PROCESS_STATE_RUNNING:
		return kernel.ProcessStateRunning
	case pb.ProcessState_PROCESS_STATE_WAITING:
		return kernel.ProcessStateWaiting
	case pb.ProcessState_PROCESS_STATE_BLOCKED:
		return kernel.ProcessStateBlocked
	case pb.ProcessState_PROCESS_STATE_TERMINATED:
		return kernel.ProcessStateTerminated
	case pb.ProcessState_PROCESS_STATE_ZOMBIE:
		return kernel.ProcessStateZombie
	default:
		return kernel.ProcessStateNew
	}
}

func schedulingPriorityToProto(p kernel.SchedulingPriority) pb.SchedulingPriority {
	switch p {
	case kernel.PriorityRealtime:
		return pb.SchedulingPriority_SCHEDULING_PRIORITY_REALTIME
	case kernel.PriorityHigh:
		return pb.SchedulingPriority_SCHEDULING_PRIORITY_HIGH
	case kernel.PriorityNormal:
		return pb.SchedulingPriority_SCHEDULING_PRIORITY_NORMAL
	case kernel.PriorityLow:
		return pb.SchedulingPriority_SCHEDULING_PRIORITY_LOW
	case kernel.PriorityIdle:
		return pb.SchedulingPriority_SCHEDULING_PRIORITY_IDLE
	default:
		return pb.SchedulingPriority_SCHEDULING_PRIORITY_UNSPECIFIED
	}
}

func protoToSchedulingPriority(p pb.SchedulingPriority) kernel.SchedulingPriority {
	switch p {
	case pb.SchedulingPriority_SCHEDULING_PRIORITY_REALTIME:
		return kernel.PriorityRealtime
	case pb.SchedulingPriority_SCHEDULING_PRIORITY_HIGH:
		return kernel.PriorityHigh
	case pb.SchedulingPriority_SCHEDULING_PRIORITY_NORMAL:
		return kernel.PriorityNormal
	case pb.SchedulingPriority_SCHEDULING_PRIORITY_LOW:
		return kernel.PriorityLow
	case pb.SchedulingPriority_SCHEDULING_PRIORITY_IDLE:
		return kernel.PriorityIdle
	default:
		return kernel.PriorityNormal
	}
}
