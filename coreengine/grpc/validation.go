// Package grpc provides validation helpers for the gRPC syscall boundary.
//
// Microkernel Architecture:
//
//	Python (userspace) → gRPC (syscall interface) → Go Kernel
//
// This file defines the validation layer at the syscall boundary, analogous to
// Linux kernel syscall argument validation. All validation happens here before
// entering kernel code, ensuring server methods contain only business logic.
package grpc

import (
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

// =============================================================================
// SYSCALL ARGUMENT VALIDATION
// =============================================================================

// validateRequired checks if a field is non-empty.
// Returns gRPC InvalidArgument error if empty, analogous to EINVAL.
func validateRequired(field, fieldName string) error {
	if field == "" {
		return status.Errorf(codes.InvalidArgument, "%s is required", fieldName)
	}
	return nil
}

// =============================================================================
// KERNEL ERROR CODES (analogous to errno.h)
// =============================================================================
//
// These error builders provide consistent error semantics across the syscall
// boundary, similar to Unix errno values. Python clients can rely on stable
// error codes and messages.

// InvalidArgument returns a gRPC InvalidArgument error (analogous to EINVAL).
// Use for malformed or missing required fields.
func InvalidArgument(fieldName string) error {
	return status.Errorf(codes.InvalidArgument, "%s is required", fieldName)
}

// NotFound returns a gRPC NotFound error (analogous to ENOENT).
// Use when a requested resource doesn't exist.
func NotFound(resourceType, id string) error {
	return status.Errorf(codes.NotFound, "%s not found: %s", resourceType, id)
}

// Internal wraps an internal error with context (analogous to EIO).
// Use when kernel operations fail unexpectedly.
func Internal(operation string, cause error) error {
	return status.Errorf(codes.Internal, "%s failed: %v", operation, cause)
}

// FailedPrecondition returns an error for invalid state transitions (analogous to EBUSY).
// Use when an operation cannot proceed due to current state.
func FailedPrecondition(resource, currentState, attemptedAction string) error {
	return status.Errorf(codes.FailedPrecondition,
		"%s in state %s cannot %s", resource, currentState, attemptedAction)
}

// ResourceExhausted returns an error for quota/limit violations (analogous to ENOMEM/EDQUOT).
// Use when resource limits are exceeded.
func ResourceExhausted(resourceType, limit string) error {
	return status.Errorf(codes.ResourceExhausted,
		"%s limit exceeded: %s", resourceType, limit)
}

// PermissionDenied returns an error for authorization failures (analogous to EPERM).
// Use when a user lacks permission for an operation.
func PermissionDenied(operation, reason string) error {
	return status.Errorf(codes.PermissionDenied,
		"%s denied: %s", operation, reason)
}
