// Package agents provides agent contracts for tool results and outcomes.
//
// Centralized Architecture (v4.0):
//   - AgentStage enum removed - stages defined in PipelineConfig
//   - AGENT_TOOL_ACCESS removed - tool access defined in AgentConfig.ToolAccess
//   - Kept: AgentOutcome, ToolStatus, StandardToolResult, ToolErrorDetails, ToolResultData
//
// Tool result standardization remains important for consistent error handling.
package agents

import (
	"fmt"
	"runtime/debug"
	"strings"

	"github.com/jeeves-cluster-organization/codeanalysis/commbus"
)

// =============================================================================
// ENUMS
// =============================================================================

// ToolStatus represents the status of a tool execution.
type ToolStatus string

const (
	// ToolStatusSuccess indicates successful execution.
	ToolStatusSuccess ToolStatus = "success"
	// ToolStatusError indicates execution failed.
	ToolStatusError ToolStatus = "error"
)

// FromString parses a status string.
func ToolStatusFromString(value string) (ToolStatus, error) {
	normalized := strings.ToLower(strings.TrimSpace(value))
	switch normalized {
	case "success":
		return ToolStatusSuccess, nil
	case "error":
		return ToolStatusError, nil
	default:
		return "", fmt.Errorf("invalid tool status '%s'. Must be one of: success, error", value)
	}
}

// AgentOutcome represents explicit outcome types for agent execution.
//
// Outcomes:
//
//	SUCCESS: Agent completed successfully, continue to next stage
//	ERROR: Agent encountered an error
//	CLARIFY: Agent needs user clarification
//	CONFIRM: Agent needs user confirmation
//	REPLAN: Agent recommends replanning
//	REINTENT: Agent recommends re-evaluating intent
//	TERMINATE: Agent recommends terminating
//	SKIP: Agent was skipped
//	PARTIAL: Agent partially completed
type AgentOutcome string

const (
	AgentOutcomeSuccess   AgentOutcome = "success"
	AgentOutcomeError     AgentOutcome = "error"
	AgentOutcomeClarify   AgentOutcome = "clarify"
	AgentOutcomeConfirm   AgentOutcome = "confirm"
	AgentOutcomeReplan    AgentOutcome = "replan"
	AgentOutcomeReintent  AgentOutcome = "reintent"
	AgentOutcomeTerminate AgentOutcome = "terminate"
	AgentOutcomeSkip      AgentOutcome = "skip"
	AgentOutcomePartial   AgentOutcome = "partial"
)

// FromString parses an outcome string.
func AgentOutcomeFromString(value string) (AgentOutcome, error) {
	normalized := strings.ToLower(strings.TrimSpace(value))
	switch normalized {
	case "success":
		return AgentOutcomeSuccess, nil
	case "error":
		return AgentOutcomeError, nil
	case "clarify":
		return AgentOutcomeClarify, nil
	case "confirm":
		return AgentOutcomeConfirm, nil
	case "replan":
		return AgentOutcomeReplan, nil
	case "reintent":
		return AgentOutcomeReintent, nil
	case "terminate":
		return AgentOutcomeTerminate, nil
	case "skip":
		return AgentOutcomeSkip, nil
	case "partial":
		return AgentOutcomePartial, nil
	default:
		return "", fmt.Errorf("invalid agent outcome '%s'. Must be one of: success, error, clarify, confirm, replan, reintent, terminate, skip, partial", value)
	}
}

// IsTerminal checks if this outcome terminates the pipeline.
func (o AgentOutcome) IsTerminal() bool {
	return o == AgentOutcomeError ||
		o == AgentOutcomeClarify ||
		o == AgentOutcomeConfirm ||
		o == AgentOutcomeTerminate
}

// IsSuccess checks if this outcome indicates successful completion.
func (o AgentOutcome) IsSuccess() bool {
	return o == AgentOutcomeSuccess ||
		o == AgentOutcomePartial ||
		o == AgentOutcomeSkip
}

// RequiresLoop checks if this outcome requires looping back.
func (o AgentOutcome) RequiresLoop() bool {
	return o == AgentOutcomeReplan || o == AgentOutcomeReintent
}

// IdempotencyClass represents idempotency classification for tools.
type IdempotencyClass string

const (
	IdempotencyClassSafe          IdempotencyClass = "safe"
	IdempotencyClassIdempotent    IdempotencyClass = "idempotent"
	IdempotencyClassNonIdempotent IdempotencyClass = "non_idempotent"
)

// FromRiskLevel infers idempotency class from risk level.
func IdempotencyClassFromRiskLevel(riskLevel commbus.RiskLevel) IdempotencyClass {
	switch riskLevel {
	case commbus.RiskLevelReadOnly:
		return IdempotencyClassSafe
	case commbus.RiskLevelWrite:
		return IdempotencyClassNonIdempotent
	case commbus.RiskLevelDestructive:
		return IdempotencyClassIdempotent
	default:
		return IdempotencyClassNonIdempotent
	}
}

// ValidationSeverity represents severity levels for validation issues.
type ValidationSeverity string

const (
	ValidationSeverityCritical ValidationSeverity = "critical"
	ValidationSeverityError    ValidationSeverity = "error"
	ValidationSeverityWarning  ValidationSeverity = "warning"
)

// =============================================================================
// TOOL ERROR DETAILS
// =============================================================================

// ToolErrorDetails represents standardized error structure for tool failures.
type ToolErrorDetails struct {
	ErrorType   string         `json:"error_type"`
	Message     string         `json:"message"`
	Details     map[string]any `json:"details,omitempty"`
	Recoverable bool           `json:"recoverable"`
}

// NewToolErrorDetails creates a new ToolErrorDetails.
func NewToolErrorDetails(errorType, message string, recoverable bool) *ToolErrorDetails {
	return &ToolErrorDetails{
		ErrorType:   errorType,
		Message:     message,
		Recoverable: recoverable,
	}
}

// FromError creates error details from a Go error.
func ToolErrorDetailsFromError(err error, recoverable bool) *ToolErrorDetails {
	return &ToolErrorDetails{
		ErrorType:   fmt.Sprintf("%T", err),
		Message:     err.Error(),
		Details:     map[string]any{"traceback": string(debug.Stack())},
		Recoverable: recoverable,
	}
}

// NotFound is a factory for "not found" errors.
func ToolErrorDetailsNotFound(entity, identifier string) *ToolErrorDetails {
	return &ToolErrorDetails{
		ErrorType:   "NotFoundError",
		Message:     fmt.Sprintf("%s '%s' not found", entity, identifier),
		Recoverable: false,
	}
}

// =============================================================================
// STANDARD TOOL RESULT
// =============================================================================

// StandardToolResult represents a standardized tool result structure.
type StandardToolResult struct {
	Status  ToolStatus        `json:"status"`
	Data    map[string]any    `json:"data,omitempty"`
	Error   *ToolErrorDetails `json:"error,omitempty"`
	Message *string           `json:"message,omitempty"`
}

// NewStandardToolResultSuccess creates a successful result.
func NewStandardToolResultSuccess(data map[string]any, message *string) *StandardToolResult {
	return &StandardToolResult{
		Status:  ToolStatusSuccess,
		Data:    data,
		Message: message,
	}
}

// NewStandardToolResultFailure creates a failed result.
func NewStandardToolResultFailure(err *ToolErrorDetails, message *string) *StandardToolResult {
	msg := message
	if msg == nil {
		msg = &err.Message
	}
	return &StandardToolResult{
		Status:  ToolStatusError,
		Error:   err,
		Message: msg,
	}
}

// Validate validates cross-field constraints.
func (r *StandardToolResult) Validate() error {
	if r.Status == ToolStatusError && r.Error == nil {
		return fmt.Errorf("error field is required when status is 'error'")
	}
	if r.Status == ToolStatusSuccess && r.Error != nil {
		return fmt.Errorf("error field must be None when status is 'success'")
	}
	return nil
}

// =============================================================================
// NORMALIZE TOOL RESULT
// =============================================================================

// NormalizeToolResult converts legacy tool result dict to StandardToolResult.
func NormalizeToolResult(result any) (*StandardToolResult, error) {
	// If already StandardToolResult, return as-is
	if str, ok := result.(*StandardToolResult); ok {
		return str, nil
	}

	// Must be a map
	resultMap, ok := result.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("tool result must be map or StandardToolResult, got %T", result)
	}

	// Determine status
	var status ToolStatus
	if statusRaw, exists := resultMap["status"]; exists {
		statusStr := fmt.Sprintf("%v", statusRaw)
		switch strings.ToLower(statusStr) {
		case "success", "completed", "ok":
			status = ToolStatusSuccess
		case "error", "failed", "failure":
			status = ToolStatusError
		default:
			if _, hasError := resultMap["error"]; hasError {
				status = ToolStatusError
			} else {
				status = ToolStatusSuccess
			}
		}
	} else {
		if _, hasError := resultMap["error"]; hasError {
			status = ToolStatusError
		} else {
			status = ToolStatusSuccess
		}
	}

	var message *string
	if msg, ok := resultMap["message"].(string); ok {
		message = &msg
	}

	if status == ToolStatusSuccess {
		return &StandardToolResult{
			Status:  status,
			Data:    resultMap,
			Message: message,
		}, nil
	}

	// Build error details
	errorMsg := ""
	if e, ok := resultMap["error"].(string); ok {
		errorMsg = e
	} else if e, ok := resultMap["message"].(string); ok {
		errorMsg = e
	} else if e, ok := resultMap["error"]; ok {
		errorMsg = fmt.Sprintf("%v", e)
	} else {
		errorMsg = "Unknown error"
	}

	var errorDetails *ToolErrorDetails
	if errorMap, ok := resultMap["error"].(map[string]any); ok {
		errorType := "ToolError"
		if t, ok := errorMap["type"].(string); ok {
			errorType = t
		}
		msg := errorMsg
		if m, ok := errorMap["message"].(string); ok {
			msg = m
		}
		errorDetails = &ToolErrorDetails{
			ErrorType: errorType,
			Message:   msg,
			Details:   errorMap,
		}
	} else {
		errorType := "ToolError"
		if t, ok := resultMap["error_type"].(string); ok {
			errorType = t
		}
		errorDetails = &ToolErrorDetails{
			ErrorType: errorType,
			Message:   errorMsg,
		}
	}

	return &StandardToolResult{
		Status:  status,
		Error:   errorDetails,
		Message: message,
	}, nil
}

// =============================================================================
// DATA STRUCTURES
// =============================================================================

// DiscoveredItem represents a discovered item from tool execution.
type DiscoveredItem struct {
	Identifier string         `json:"identifier"`
	ItemType   string         `json:"item_type"`
	Location   *string        `json:"location,omitempty"`
	Metadata   map[string]any `json:"metadata,omitempty"`
}

// Evidence represents evidence for citation.
type Evidence struct {
	Location   string  `json:"location"`
	Content    string  `json:"content"`
	Relevance  string  `json:"relevance"`
	Confidence float64 `json:"confidence"`
}

// NewEvidence creates a new Evidence with defaults.
func NewEvidence(location, content string) *Evidence {
	return &Evidence{
		Location:   location,
		Content:    content,
		Relevance:  "",
		Confidence: 0.9,
	}
}

// ToolResultData represents standardized tool output structure.
type ToolResultData struct {
	ItemsExplored []string         `json:"items_explored"`
	ItemsPending  []string         `json:"items_pending"`
	Discoveries   []DiscoveredItem `json:"discoveries"`
	Evidence      []Evidence       `json:"evidence"`
	Raw           map[string]any   `json:"raw"`
}

// NewToolResultData creates a new ToolResultData from a map.
func NewToolResultData(data map[string]any) *ToolResultData {
	result := &ToolResultData{
		ItemsExplored: []string{},
		ItemsPending:  []string{},
		Discoveries:   []DiscoveredItem{},
		Evidence:      []Evidence{},
		Raw:           data,
	}

	if items, ok := data["items_explored"].([]string); ok {
		result.ItemsExplored = items
	} else if items, ok := data["items_explored"].([]any); ok {
		for _, item := range items {
			if s, ok := item.(string); ok {
				result.ItemsExplored = append(result.ItemsExplored, s)
			}
		}
	}

	if items, ok := data["items_pending"].([]string); ok {
		result.ItemsPending = items
	} else if items, ok := data["items_pending"].([]any); ok {
		for _, item := range items {
			if s, ok := item.(string); ok {
				result.ItemsPending = append(result.ItemsPending, s)
			}
		}
	}

	if discoveries, ok := data["discoveries"].([]any); ok {
		for _, d := range discoveries {
			if dm, ok := d.(map[string]any); ok {
				disc := DiscoveredItem{
					Identifier: fmt.Sprintf("%v", dm["identifier"]),
					ItemType:   "item",
					Metadata:   make(map[string]any),
				}
				if t, ok := dm["item_type"].(string); ok {
					disc.ItemType = t
				}
				if l, ok := dm["location"].(string); ok {
					disc.Location = &l
				}
				if m, ok := dm["metadata"].(map[string]any); ok {
					disc.Metadata = m
				}
				result.Discoveries = append(result.Discoveries, disc)
			} else if s, ok := d.(string); ok {
				result.Discoveries = append(result.Discoveries, DiscoveredItem{
					Identifier: s,
					ItemType:   "item",
				})
			}
		}
	}

	if evidence, ok := data["evidence"].([]any); ok {
		for _, e := range evidence {
			if em, ok := e.(map[string]any); ok {
				ev := Evidence{
					Location:   fmt.Sprintf("%v", em["location"]),
					Content:    fmt.Sprintf("%v", em["content"]),
					Relevance:  "",
					Confidence: 0.9,
				}
				if r, ok := em["relevance"].(string); ok {
					ev.Relevance = r
				}
				if c, ok := em["confidence"].(float64); ok {
					ev.Confidence = c
				}
				result.Evidence = append(result.Evidence, ev)
			} else if s, ok := e.(string); ok {
				result.Evidence = append(result.Evidence, Evidence{
					Location:   "unknown",
					Content:    s,
					Confidence: 0.9,
				})
			}
		}
	}

	return result
}
