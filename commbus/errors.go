package commbus

import (
	"fmt"
)

// =============================================================================
// EXCEPTIONS
// =============================================================================

// CommBusError is the base error type for commbus errors.
type CommBusError struct {
	Message string
	Cause   error
}

func (e *CommBusError) Error() string {
	if e.Cause != nil {
		return fmt.Sprintf("%s: %v", e.Message, e.Cause)
	}
	return e.Message
}

func (e *CommBusError) Unwrap() error {
	return e.Cause
}

// NoHandlerError is raised when no handler is registered for a message type.
type NoHandlerError struct {
	MessageType string
}

func (e *NoHandlerError) Error() string {
	return fmt.Sprintf("no handler registered for %s", e.MessageType)
}

// NewNoHandlerError creates a new NoHandlerError.
func NewNoHandlerError(messageType string) *NoHandlerError {
	return &NoHandlerError{MessageType: messageType}
}

// HandlerAlreadyRegisteredError is raised when trying to register a duplicate handler.
type HandlerAlreadyRegisteredError struct {
	MessageType string
}

func (e *HandlerAlreadyRegisteredError) Error() string {
	return fmt.Sprintf("handler already registered for %s", e.MessageType)
}

// NewHandlerAlreadyRegisteredError creates a new HandlerAlreadyRegisteredError.
func NewHandlerAlreadyRegisteredError(messageType string) *HandlerAlreadyRegisteredError {
	return &HandlerAlreadyRegisteredError{MessageType: messageType}
}

// QueryTimeoutError is raised when a query times out.
type QueryTimeoutError struct {
	MessageType string
	Timeout     float64
}

func (e *QueryTimeoutError) Error() string {
	return fmt.Sprintf("query %s timed out after %.2fs", e.MessageType, e.Timeout)
}

// NewQueryTimeoutError creates a new QueryTimeoutError.
func NewQueryTimeoutError(messageType string, timeout float64) *QueryTimeoutError {
	return &QueryTimeoutError{MessageType: messageType, Timeout: timeout}
}
