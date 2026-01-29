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
	"context"
	"fmt"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/kernel"
	pb "github.com/jeeves-cluster-organization/codeanalysis/coreengine/proto"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/metadata"
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
// REQUEST CONTEXT VALIDATION (Agentic Security)
// =============================================================================

// RequestContext contains validated agent/user identity from syscall.
// Analogous to Linux task_struct - represents the calling process context.
// Required for multi-tenant agentic kernel operations.
type RequestContext struct {
	UserID    string
	SessionID string
	RequestID string
}

// ExtractRequestContext extracts and validates agent identity from gRPC context.
// For CreateProcess: extracts from request fields (entry point).
// For other operations: extracts from gRPC metadata (standard auth pattern).
//
// Returns PermissionDenied if context is missing or invalid.
func ExtractRequestContext(ctx context.Context, req interface{}) (*RequestContext, error) {
	var reqCtx RequestContext

	// Try extracting from request fields first (CreateProcess has them)
	switch r := req.(type) {
	case *pb.CreateProcessRequest:
		reqCtx.UserID = r.UserId
		reqCtx.SessionID = r.SessionId
		reqCtx.RequestID = r.RequestId
	default:
		// For other requests, extract from gRPC metadata
		md, ok := metadata.FromIncomingContext(ctx)
		if !ok {
			return nil, status.Error(codes.Unauthenticated, "missing request metadata")
		}

		// Extract user_id from metadata
		if vals := md.Get("user_id"); len(vals) > 0 {
			reqCtx.UserID = vals[0]
		}
		// Extract session_id from metadata
		if vals := md.Get("session_id"); len(vals) > 0 {
			reqCtx.SessionID = vals[0]
		}
		// Extract request_id from metadata
		if vals := md.Get("request_id"); len(vals) > 0 {
			reqCtx.RequestID = vals[0]
		}
	}

	// Validate required fields
	if err := validateRequired(reqCtx.UserID, "user_id"); err != nil {
		return nil, status.Error(codes.Unauthenticated, "user_id required in request or metadata")
	}
	if err := validateRequired(reqCtx.SessionID, "session_id"); err != nil {
		return nil, status.Error(codes.Unauthenticated, "session_id required in request or metadata")
	}

	return &reqCtx, nil
}

// =============================================================================
// QUOTA PRE-FLIGHT VALIDATION (Cost Control)
// =============================================================================

// ValidateQuotaAvailable checks if process has quota remaining for operation.
// Returns ResourceExhausted if any quota limit is exceeded.
// This prevents wasted work - check BEFORE expensive operations (LLM calls, etc.).
func ValidateQuotaAvailable(k *kernel.Kernel, pid string) error {
	exceeded := k.CheckQuota(pid)
	if exceeded != "" {
		return ResourceExhausted("quota", exceeded)
	}
	return nil
}

// =============================================================================
// PROCESS OWNERSHIP VALIDATION (Multi-Tenant Isolation)
// =============================================================================

// ValidateProcessOwnership checks if user owns the process.
// Returns PermissionDenied if process is owned by different user.
// Critical for multi-agent security - agent A cannot access agent B's processes.
func ValidateProcessOwnership(pcb *kernel.ProcessControlBlock, userID string) error {
	if pcb == nil {
		return NotFound("process", "nil")
	}

	if pcb.UserID != userID {
		return PermissionDenied("access process",
			fmt.Sprintf("process owned by %s, requested by %s", pcb.UserID, userID))
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
