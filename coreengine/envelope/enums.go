// Package envelope provides envelope enums and the GenericEnvelope implementation.
//
// Centralized Architecture (v4.0):
//   - Stage tracking is now string-based (via GenericEnvelope.CurrentStage)
//   - EnvelopeStage enum removed - stages are defined in PipelineConfig
//   - Kept: TerminalReason, CriticVerdict, RiskApproval (still useful)
package envelope

// TerminalReason represents why processing terminated - exactly one per request.
type TerminalReason string

const (
	// TerminalReasonCompletedSuccessfully indicates successful completion.
	TerminalReasonCompletedSuccessfully TerminalReason = "completed_successfully"
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
	// TerminalReasonMaxIterationsExceeded indicates max iterations reached.
	TerminalReasonMaxIterationsExceeded TerminalReason = "max_iterations_exceeded"
	// TerminalReasonMaxLLMCallsExceeded indicates max LLM calls reached.
	TerminalReasonMaxLLMCallsExceeded TerminalReason = "max_llm_calls_exceeded"
	// TerminalReasonMaxAgentHopsExceeded indicates max agent hops reached.
	TerminalReasonMaxAgentHopsExceeded TerminalReason = "max_agent_hops_exceeded"
	// TerminalReasonMaxCriticFiresExceeded indicates max critic fires reached.
	TerminalReasonMaxCriticFiresExceeded TerminalReason = "max_critic_fires_exceeded"
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

// CriticVerdict represents critic agent verdicts.
//
// APPROVED: Proceed to Integration - goals satisfied
// REINTENT: Go back to Intent with feedback - Critic proposes hints, Intent decides
// NEXT_STAGE: Advance to next goal set in multi-stage execution
type CriticVerdict string

const (
	// CriticVerdictApproved indicates proceeding to Integration.
	CriticVerdictApproved CriticVerdict = "approved"
	// CriticVerdictReintent indicates going back to Intent.
	CriticVerdictReintent CriticVerdict = "reintent"
	// CriticVerdictNextStage indicates advancing to next stage.
	CriticVerdictNextStage CriticVerdict = "next_stage"
)
