// Package kernel provides service registry - IPC coordinator equivalent.
//
// Features:
//   - Service registration and discovery
//   - Health tracking
//   - Load balancing
//   - Dispatch routing
package kernel

import (
	"context"
	"fmt"
	"sync"
	"time"
)

// =============================================================================
// Service Status
// =============================================================================

// ServiceStatus represents the health status of a service.
type ServiceStatus string

const (
	// ServiceStatusHealthy indicates the service is healthy and accepting requests.
	ServiceStatusHealthy ServiceStatus = "healthy"
	// ServiceStatusDegraded indicates the service is running but with reduced capacity.
	ServiceStatusDegraded ServiceStatus = "degraded"
	// ServiceStatusUnhealthy indicates the service is not accepting requests.
	ServiceStatusUnhealthy ServiceStatus = "unhealthy"
	// ServiceStatusUnknown indicates the service status is unknown.
	ServiceStatusUnknown ServiceStatus = "unknown"
)

// =============================================================================
// Service Types
// =============================================================================

// ServiceType constants for classifying services.
const (
	// ServiceTypeFlow is for flow orchestration services.
	ServiceTypeFlow = "flow"
	// ServiceTypeWorker is for worker/execution services.
	ServiceTypeWorker = "worker"
	// ServiceTypeVertical is for vertical/domain-specific services.
	ServiceTypeVertical = "vertical"
	// ServiceTypeInference is for ML inference services (embeddings, NLI, etc).
	ServiceTypeInference = "inference"
)

// =============================================================================
// Service Descriptor
// =============================================================================

// ServiceInfo describes a registered service.
type ServiceInfo struct {
	// Identity
	Name        string `json:"name"`
	ServiceType string `json:"service_type"` // "flow", "worker", "vertical"
	Version     string `json:"version"`

	// Capabilities
	Capabilities []string `json:"capabilities"`

	// Capacity
	MaxConcurrent int `json:"max_concurrent"`
	CurrentLoad   int `json:"current_load"`

	// Health
	Status          ServiceStatus `json:"status"`
	LastHealthCheck time.Time     `json:"last_health_check"`

	// Metadata
	Metadata map[string]any `json:"metadata,omitempty"`
}

// NewServiceInfo creates a new service descriptor.
func NewServiceInfo(name, serviceType string) *ServiceInfo {
	return &ServiceInfo{
		Name:            name,
		ServiceType:     serviceType,
		Version:         "1.0.0",
		Capabilities:    []string{},
		MaxConcurrent:   10,
		CurrentLoad:     0,
		Status:          ServiceStatusHealthy,
		LastHealthCheck: time.Now().UTC(),
	}
}

// CanAccept checks if the service can accept more load.
func (s *ServiceInfo) CanAccept() bool {
	return s.Status == ServiceStatusHealthy && s.CurrentLoad < s.MaxConcurrent
}

// IsHealthy checks if the service is healthy.
func (s *ServiceInfo) IsHealthy() bool {
	return s.Status == ServiceStatusHealthy || s.Status == ServiceStatusDegraded
}

// Clone creates a copy of the service info.
func (s *ServiceInfo) Clone() *ServiceInfo {
	clone := &ServiceInfo{
		Name:            s.Name,
		ServiceType:     s.ServiceType,
		Version:         s.Version,
		MaxConcurrent:   s.MaxConcurrent,
		CurrentLoad:     s.CurrentLoad,
		Status:          s.Status,
		LastHealthCheck: s.LastHealthCheck,
	}
	if s.Capabilities != nil {
		clone.Capabilities = make([]string, len(s.Capabilities))
		copy(clone.Capabilities, s.Capabilities)
	}
	if s.Metadata != nil {
		clone.Metadata = make(map[string]any, len(s.Metadata))
		for k, v := range s.Metadata {
			clone.Metadata[k] = v
		}
	}
	return clone
}

// =============================================================================
// Dispatch Target
// =============================================================================

// DispatchTarget represents a target for request dispatch.
type DispatchTarget struct {
	ServiceName    string             `json:"service_name"`
	Method         string             `json:"method"`
	Priority       SchedulingPriority `json:"priority"`
	TimeoutSeconds int                `json:"timeout_seconds"`
	RetryCount     int                `json:"retry_count"`
	MaxRetries     int                `json:"max_retries"`
}

// CanRetry checks if the dispatch can be retried.
func (d *DispatchTarget) CanRetry() bool {
	return d.RetryCount < d.MaxRetries
}

// IncrementRetry increments the retry count.
func (d *DispatchTarget) IncrementRetry() {
	d.RetryCount++
}

// =============================================================================
// Dispatch Result
// =============================================================================

// DispatchResult represents the result of a dispatch operation.
type DispatchResult struct {
	Success  bool           `json:"success"`
	Error    string         `json:"error,omitempty"`
	Data     map[string]any `json:"data,omitempty"`
	Duration time.Duration  `json:"duration"`
	Retries  int            `json:"retries"`
}

// =============================================================================
// Service Handler
// =============================================================================

// ServiceHandler is a function that handles service requests.
type ServiceHandler func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error)

// =============================================================================
// Service Registry
// =============================================================================

// ServiceRegistry manages service registration, discovery, and dispatch.
// Thread-safe implementation for tracking registered services.
//
// Usage:
//
//	registry := NewServiceRegistry(nil)
//
//	// Register a service
//	registry.RegisterService(NewServiceInfo("flow_service", "flow"))
//
//	// Register a handler
//	registry.RegisterHandler("flow_service", myHandler)
//
//	// Dispatch a request
//	result := registry.Dispatch(ctx, target, data)
type ServiceRegistry struct {
	logger Logger

	// Service registry
	services map[string]*ServiceInfo

	// Dispatch handlers
	handlers map[string]ServiceHandler

	mu sync.RWMutex
}

// NewServiceRegistry creates a new service registry.
func NewServiceRegistry(logger Logger) *ServiceRegistry {
	return &ServiceRegistry{
		logger:   logger,
		services: make(map[string]*ServiceInfo),
		handlers: make(map[string]ServiceHandler),
	}
}

// =============================================================================
// Service Registration
// =============================================================================

// RegisterService registers a service with the registry.
// Returns true if registration succeeded, false if service already exists.
func (sr *ServiceRegistry) RegisterService(info *ServiceInfo) bool {
	sr.mu.Lock()
	defer sr.mu.Unlock()

	if _, exists := sr.services[info.Name]; exists {
		if sr.logger != nil {
			sr.logger.Warn("service_already_registered", "service_name", info.Name)
		}
		return false
	}

	sr.services[info.Name] = info

	if sr.logger != nil {
		sr.logger.Info("service_registered",
			"service_name", info.Name,
			"service_type", info.ServiceType,
			"capabilities", info.Capabilities,
		)
	}

	return true
}

// UnregisterService unregisters a service.
// Returns true if unregistration succeeded.
func (sr *ServiceRegistry) UnregisterService(serviceName string) bool {
	sr.mu.Lock()
	defer sr.mu.Unlock()

	if _, exists := sr.services[serviceName]; !exists {
		return false
	}

	delete(sr.services, serviceName)
	delete(sr.handlers, serviceName)

	if sr.logger != nil {
		sr.logger.Info("service_unregistered", "service_name", serviceName)
	}

	return true
}

// RegisterHandler registers a handler for a service.
func (sr *ServiceRegistry) RegisterHandler(serviceName string, handler ServiceHandler) {
	sr.mu.Lock()
	defer sr.mu.Unlock()

	sr.handlers[serviceName] = handler

	if sr.logger != nil {
		sr.logger.Debug("handler_registered", "service_name", serviceName)
	}
}

// =============================================================================
// Service Discovery
// =============================================================================

// GetService gets a service by name.
func (sr *ServiceRegistry) GetService(serviceName string) *ServiceInfo {
	sr.mu.RLock()
	defer sr.mu.RUnlock()

	if svc, exists := sr.services[serviceName]; exists {
		return svc.Clone()
	}
	return nil
}

// ListServices lists all registered services matching criteria.
func (sr *ServiceRegistry) ListServices(serviceType string, healthyOnly bool) []*ServiceInfo {
	sr.mu.RLock()
	defer sr.mu.RUnlock()

	var result []*ServiceInfo
	for _, svc := range sr.services {
		if serviceType != "" && svc.ServiceType != serviceType {
			continue
		}
		if healthyOnly && !svc.IsHealthy() {
			continue
		}
		result = append(result, svc.Clone())
	}
	return result
}

// GetServiceNames returns all registered service names.
func (sr *ServiceRegistry) GetServiceNames() []string {
	sr.mu.RLock()
	defer sr.mu.RUnlock()

	names := make([]string, 0, len(sr.services))
	for name := range sr.services {
		names = append(names, name)
	}
	return names
}

// HasService checks if a service is registered.
func (sr *ServiceRegistry) HasService(serviceName string) bool {
	sr.mu.RLock()
	defer sr.mu.RUnlock()
	_, exists := sr.services[serviceName]
	return exists
}

// HasHandler checks if a handler is registered for a service.
func (sr *ServiceRegistry) HasHandler(serviceName string) bool {
	sr.mu.RLock()
	defer sr.mu.RUnlock()
	_, exists := sr.handlers[serviceName]
	return exists
}

// =============================================================================
// Health Management
// =============================================================================

// UpdateHealth updates the health status of a service.
func (sr *ServiceRegistry) UpdateHealth(serviceName string, status ServiceStatus) bool {
	sr.mu.Lock()
	defer sr.mu.Unlock()

	svc, exists := sr.services[serviceName]
	if !exists {
		return false
	}

	svc.Status = status
	svc.LastHealthCheck = time.Now().UTC()

	if sr.logger != nil {
		sr.logger.Debug("service_health_updated",
			"service_name", serviceName,
			"status", string(status),
		)
	}

	return true
}

// GetHealthyCount returns the count of healthy services.
func (sr *ServiceRegistry) GetHealthyCount() int {
	sr.mu.RLock()
	defer sr.mu.RUnlock()

	count := 0
	for _, svc := range sr.services {
		if svc.IsHealthy() {
			count++
		}
	}
	return count
}

// =============================================================================
// Load Management
// =============================================================================

// IncrementLoad increments the current load for a service.
// Returns false if service not found or at capacity.
func (sr *ServiceRegistry) IncrementLoad(serviceName string) bool {
	sr.mu.Lock()
	defer sr.mu.Unlock()

	svc, exists := sr.services[serviceName]
	if !exists || !svc.CanAccept() {
		return false
	}

	svc.CurrentLoad++
	return true
}

// DecrementLoad decrements the current load for a service.
func (sr *ServiceRegistry) DecrementLoad(serviceName string) {
	sr.mu.Lock()
	defer sr.mu.Unlock()

	svc, exists := sr.services[serviceName]
	if !exists {
		return
	}

	if svc.CurrentLoad > 0 {
		svc.CurrentLoad--
	}
}

// GetLoad returns the current load for a service.
func (sr *ServiceRegistry) GetLoad(serviceName string) int {
	sr.mu.RLock()
	defer sr.mu.RUnlock()

	svc, exists := sr.services[serviceName]
	if !exists {
		return -1
	}
	return svc.CurrentLoad
}

// =============================================================================
// Dispatch
// =============================================================================

// Dispatch dispatches a request to a service.
func (sr *ServiceRegistry) Dispatch(
	ctx context.Context,
	target *DispatchTarget,
	data map[string]any,
) *DispatchResult {
	startTime := time.Now()

	// Check if service exists
	sr.mu.RLock()
	svc, exists := sr.services[target.ServiceName]
	handler := sr.handlers[target.ServiceName]
	sr.mu.RUnlock()

	if !exists {
		if sr.logger != nil {
			sr.logger.Error("dispatch_unknown_service", "service_name", target.ServiceName)
		}
		return &DispatchResult{
			Success:  false,
			Error:    fmt.Sprintf("unknown service: %s", target.ServiceName),
			Duration: time.Since(startTime),
			Retries:  target.RetryCount,
		}
	}

	// Check if service is healthy
	if !svc.IsHealthy() {
		if sr.logger != nil {
			sr.logger.Error("dispatch_unhealthy_service", "service_name", target.ServiceName)
		}
		return &DispatchResult{
			Success:  false,
			Error:    fmt.Sprintf("service unhealthy: %s", target.ServiceName),
			Duration: time.Since(startTime),
			Retries:  target.RetryCount,
		}
	}

	// Check if service can accept
	if !svc.CanAccept() {
		if sr.logger != nil {
			sr.logger.Warn("dispatch_service_at_capacity", "service_name", target.ServiceName)
		}
		return &DispatchResult{
			Success:  false,
			Error:    fmt.Sprintf("service at capacity: %s", target.ServiceName),
			Duration: time.Since(startTime),
			Retries:  target.RetryCount,
		}
	}

	// Check if handler exists
	if handler == nil {
		if sr.logger != nil {
			sr.logger.Error("dispatch_no_handler", "service_name", target.ServiceName)
		}
		return &DispatchResult{
			Success:  false,
			Error:    fmt.Sprintf("no handler for service: %s", target.ServiceName),
			Duration: time.Since(startTime),
			Retries:  target.RetryCount,
		}
	}

	// Update load
	if !sr.IncrementLoad(target.ServiceName) {
		return &DispatchResult{
			Success:  false,
			Error:    fmt.Sprintf("failed to acquire load slot: %s", target.ServiceName),
			Duration: time.Since(startTime),
			Retries:  target.RetryCount,
		}
	}
	defer sr.DecrementLoad(target.ServiceName)

	// Apply timeout
	if target.TimeoutSeconds > 0 {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, time.Duration(target.TimeoutSeconds)*time.Second)
		defer cancel()
	}

	if sr.logger != nil {
		sr.logger.Debug("dispatch_started",
			"service_name", target.ServiceName,
			"method", target.Method,
		)
	}

	// Execute handler
	result, err := handler(ctx, target, data)
	if err != nil {
		// Check if context timed out
		if ctx.Err() == context.DeadlineExceeded {
			if sr.logger != nil {
				sr.logger.Error("dispatch_timeout",
					"service_name", target.ServiceName,
					"timeout_seconds", target.TimeoutSeconds,
				)
			}
			return &DispatchResult{
				Success:  false,
				Error:    "dispatch timeout",
				Duration: time.Since(startTime),
				Retries:  target.RetryCount,
			}
		}

		// Check if we should retry
		if target.CanRetry() {
			target.IncrementRetry()
			if sr.logger != nil {
				sr.logger.Info("dispatch_retry",
					"service_name", target.ServiceName,
					"retry", target.RetryCount,
					"max_retries", target.MaxRetries,
				)
			}
			return sr.Dispatch(ctx, target, data)
		}

		if sr.logger != nil {
			sr.logger.Error("dispatch_error",
				"service_name", target.ServiceName,
				"error", err.Error(),
			)
		}
		return &DispatchResult{
			Success:  false,
			Error:    err.Error(),
			Duration: time.Since(startTime),
			Retries:  target.RetryCount,
		}
	}

	if result == nil {
		result = &DispatchResult{Success: true}
	}
	result.Duration = time.Since(startTime)
	result.Retries = target.RetryCount

	return result
}

// =============================================================================
// Statistics
// =============================================================================

// GetStats returns statistics about the service registry.
func (sr *ServiceRegistry) GetStats() map[string]any {
	sr.mu.RLock()
	defer sr.mu.RUnlock()

	totalLoad := 0
	totalCapacity := 0
	healthyCount := 0
	unhealthyCount := 0

	for _, svc := range sr.services {
		totalLoad += svc.CurrentLoad
		totalCapacity += svc.MaxConcurrent
		if svc.IsHealthy() {
			healthyCount++
		} else {
			unhealthyCount++
		}
	}

	return map[string]any{
		"total_services":    len(sr.services),
		"healthy_services":  healthyCount,
		"unhealthy_services": unhealthyCount,
		"total_handlers":    len(sr.handlers),
		"total_load":        totalLoad,
		"total_capacity":    totalCapacity,
	}
}

// GetServiceStats returns per-service statistics.
func (sr *ServiceRegistry) GetServiceStats() map[string]map[string]any {
	sr.mu.RLock()
	defer sr.mu.RUnlock()

	stats := make(map[string]map[string]any, len(sr.services))
	for name, svc := range sr.services {
		_, hasHandler := sr.handlers[name]
		stats[name] = map[string]any{
			"type":            svc.ServiceType,
			"status":          string(svc.Status),
			"current_load":    svc.CurrentLoad,
			"max_concurrent":  svc.MaxConcurrent,
			"has_handler":     hasHandler,
			"last_health_check": svc.LastHealthCheck.Format(time.RFC3339),
		}
	}
	return stats
}
