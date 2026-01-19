// Package envelope provides envelope enums and the GenericEnvelope implementation.
//
// Centralized Architecture (v4.0):
//   - Stage tracking is now string-based (via GenericEnvelope.CurrentStage)
//   - EnvelopeStage enum removed - stages are defined in PipelineConfig
//   - Kept: TerminalReason, LoopVerdict, RiskApproval
//
// NOTE: This package uses GENERIC terminology. Domain-specific concepts like
// "Critic" or "Intent" belong in the capability layer, not here.
package envelope

// TerminalReason represents why processing terminated - exactly one per request.
type TerminalReason string

const (
	// TerminalReasonCompletedSuccessfully indicates successful completion.
	TerminalReasonCompletedSuccessfully TerminalReason = "completed_successfully"
	// TerminalReasonCompleted is an alias for CompletedSuccessfully.
	TerminalReasonCompleted TerminalReason = "completed"
	// TerminalReasonClarificationRequired indicates user clarification is needed.
	TerminalReasonClarificationRequired TerminalReason = "clarification_required"
	// TerminalReasonConfirmationRequired indicates user confirmation is needed.
	TerminalReasonConfirmationRequired TerminalReason = "confirmation_required"
	// TerminalReasonDeniedByPolicy indicates policy denial.
	TerminalReasonDeniedByPolicy TerminalReason = "denied_by_policy"
	// TerminalReasonToolFailedRecoverably indicates a recoverable tool failure.
	TerminalReasonToolFailedRecoverably TerminalReason = "tool_failed_recoverably"
	// TerminalReasonToolFailedFatally indicates a fatal tool failure.
	TerminalReasonToolFailedFatally TerminalReason = "tool_failed_fatally"
	// TerminalReasonLLMFailedFatally indicates a fatal LLM failure.
	TerminalReasonLLMFailedFatally TerminalReason = "llm_failed_fatally"
	// TerminalReasonMaxIterationsExceeded indicates max iterations reached.
	TerminalReasonMaxIterationsExceeded TerminalReason = "max_iterations_exceeded"
	// TerminalReasonMaxLLMCallsExceeded indicates max LLM calls reached.
	TerminalReasonMaxLLMCallsExceeded TerminalReason = "max_llm_calls_exceeded"
	// TerminalReasonMaxAgentHopsExceeded indicates max agent hops reached.
	TerminalReasonMaxAgentHopsExceeded TerminalReason = "max_agent_hops_exceeded"
	// TerminalReasonMaxLoopExceeded indicates max loop iterations reached.
	TerminalReasonMaxLoopExceeded TerminalReason = "max_loop_exceeded"
	// TerminalReasonUserCancelled indicates the user cancelled the request.
	TerminalReasonUserCancelled TerminalReason = "user_cancelled"
)

// RiskApproval represents policy arbiter decisions.
type RiskApproval string

const (
	// RiskApprovalApproved indicates the request is approved.
	RiskApprovalApproved RiskApproval = "approved"
	// RiskApprovalDenied indicates the request is denied.
	RiskApprovalDenied RiskApproval = "denied"
	// RiskApprovalRequiresConfirmation indicates user confirmation is needed.
	RiskApprovalRequiresConfirmation RiskApproval = "requires_confirmation"
)

// LoopVerdict represents agent loop control verdicts.
// This is a GENERIC routing decision - capability layer defines the semantics.
//
// PROCEED: Continue to next stage - current stage goals satisfied
// LOOP_BACK: Return to an earlier stage with feedback
// ADVANCE: Skip ahead to a later stage (multi-stage execution)
type LoopVerdict string

const (
	// LoopVerdictProceed indicates continuing to the next stage.
	LoopVerdictProceed LoopVerdict = "proceed"
	// LoopVerdictLoopBack indicates returning to an earlier stage.
	LoopVerdictLoopBack LoopVerdict = "loop_back"
	// LoopVerdictAdvance indicates advancing to a later stage.
	LoopVerdictAdvance LoopVerdict = "advance"
)
