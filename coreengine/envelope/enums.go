// Package envelope provides core enums - source of truth for jeeves-kernel.
//
// All Python enums are thin wrappers around these Go definitions.
// Proto definitions in engine.proto are used for gRPC transport only.
package envelope

// TerminalReason represents why processing terminated.
type TerminalReason string

const (
	TerminalReasonCompleted             TerminalReason = "completed"
	TerminalReasonMaxIterationsExceeded TerminalReason = "max_iterations_exceeded"
	TerminalReasonMaxLLMCallsExceeded   TerminalReason = "max_llm_calls_exceeded"
	TerminalReasonMaxAgentHopsExceeded  TerminalReason = "max_agent_hops_exceeded"
	TerminalReasonMaxLoopExceeded       TerminalReason = "max_loop_exceeded"
	TerminalReasonUserCancelled         TerminalReason = "user_cancelled"
	TerminalReasonToolFailedFatally     TerminalReason = "tool_failed_fatally"
	TerminalReasonLLMFailedFatally      TerminalReason = "llm_failed_fatally"
	TerminalReasonPolicyViolation       TerminalReason = "policy_violation"
)

// RiskLevel represents risk for tool execution.
type RiskLevel string

const (
	// Semantic levels
	RiskLevelReadOnly    RiskLevel = "read_only"
	RiskLevelWrite       RiskLevel = "write"
	RiskLevelDestructive RiskLevel = "destructive"
	// Severity levels
	RiskLevelLow      RiskLevel = "low"
	RiskLevelMedium   RiskLevel = "medium"
	RiskLevelHigh     RiskLevel = "high"
	RiskLevelCritical RiskLevel = "critical"
)

// RequiresConfirmation checks if a risk level requires user confirmation.
func (r RiskLevel) RequiresConfirmation() bool {
	return r == RiskLevelDestructive || r == RiskLevelHigh || r == RiskLevelCritical
}

// ToolCategory represents tool categorization.
type ToolCategory string

const (
	// Operation types
	ToolCategoryRead    ToolCategory = "read"
	ToolCategoryWrite   ToolCategory = "write"
	ToolCategoryExecute ToolCategory = "execute"
	ToolCategoryNetwork ToolCategory = "network"
	ToolCategorySystem  ToolCategory = "system"
	// Tool organization
	ToolCategoryUnified    ToolCategory = "unified"
	ToolCategoryComposite  ToolCategory = "composite"
	ToolCategoryResilient  ToolCategory = "resilient"
	ToolCategoryStandalone ToolCategory = "standalone"
	ToolCategoryInternal   ToolCategory = "internal"
)

// HealthStatus represents health check status.
type HealthStatus string

const (
	HealthStatusHealthy   HealthStatus = "healthy"
	HealthStatusDegraded  HealthStatus = "degraded"
	HealthStatusUnhealthy HealthStatus = "unhealthy"
	HealthStatusUnknown   HealthStatus = "unknown"
)

// LoopVerdict represents agent loop control verdicts.
type LoopVerdict string

const (
	LoopVerdictProceed  LoopVerdict = "proceed"
	LoopVerdictLoopBack LoopVerdict = "loop_back"
	LoopVerdictAdvance  LoopVerdict = "advance"
	LoopVerdictEscalate LoopVerdict = "escalate"
)

// RiskApproval represents risk approval status.
type RiskApproval string

const (
	RiskApprovalApproved RiskApproval = "approved"
	RiskApprovalDenied   RiskApproval = "denied"
	RiskApprovalPending  RiskApproval = "pending"
)

// ToolAccess represents tool access levels for agents.
type ToolAccess string

const (
	ToolAccessNone  ToolAccess = "none"
	ToolAccessRead  ToolAccess = "read"
	ToolAccessWrite ToolAccess = "write"
	ToolAccessAll   ToolAccess = "all"
)

// OperationStatus represents status of an operation result.
type OperationStatus string

const (
	OperationStatusSuccess           OperationStatus = "success"
	OperationStatusError             OperationStatus = "error"
	OperationStatusNotFound          OperationStatus = "not_found"
	OperationStatusTimeout           OperationStatus = "timeout"
	OperationStatusValidationError   OperationStatus = "validation_error"
	OperationStatusPartial           OperationStatus = "partial"
	OperationStatusInvalidParameters OperationStatus = "invalid_parameters"
)
