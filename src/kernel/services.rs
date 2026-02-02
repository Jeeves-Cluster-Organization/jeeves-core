//! Service registry - IPC coordinator and service discovery.
//!
//! Features:
//!   - Service registration and discovery
//!   - Health tracking
//!   - Load balancing
//!   - Dispatch routing

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::kernel::SchedulingPriority;

// =============================================================================
// Service Status
// =============================================================================

/// Health status of a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    /// Service is healthy and accepting requests
    Healthy,
    /// Service is running but with reduced capacity
    Degraded,
    /// Service is not accepting requests
    Unhealthy,
    /// Service status is unknown
    Unknown,
}

// =============================================================================
// Service Types
// =============================================================================

/// Service type constants for classifying services.
pub const SERVICE_TYPE_FLOW: &str = "flow";
pub const SERVICE_TYPE_WORKER: &str = "worker";
pub const SERVICE_TYPE_VERTICAL: &str = "vertical";
pub const SERVICE_TYPE_INFERENCE: &str = "inference";

// =============================================================================
// Service Info
// =============================================================================

/// ServiceInfo describes a registered service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    // Identity
    pub name: String,
    pub service_type: String, // "flow", "worker", "vertical", "inference"
    pub version: String,

    // Capabilities
    pub capabilities: Vec<String>,

    // Capacity
    pub max_concurrent: i32,
    pub current_load: i32,

    // Health
    pub status: ServiceStatus,
    pub last_health_check: DateTime<Utc>,

    // Metadata
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ServiceInfo {
    /// Create a new service descriptor.
    pub fn new(name: String, service_type: String) -> Self {
        Self {
            name,
            service_type,
            version: "1.0.0".to_string(),
            capabilities: Vec::new(),
            max_concurrent: 10,
            current_load: 0,
            status: ServiceStatus::Healthy,
            last_health_check: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Check if the service can accept more load.
    pub fn can_accept(&self) -> bool {
        self.status == ServiceStatus::Healthy && self.current_load < self.max_concurrent
    }

    /// Check if the service is healthy.
    pub fn is_healthy(&self) -> bool {
        matches!(self.status, ServiceStatus::Healthy | ServiceStatus::Degraded)
    }
}

// =============================================================================
// Dispatch Target
// =============================================================================

/// DispatchTarget represents a target for request dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchTarget {
    pub service_name: String,
    pub method: String,
    pub priority: SchedulingPriority,
    pub timeout_seconds: i32,
    pub retry_count: i32,
    pub max_retries: i32,
}

impl DispatchTarget {
    /// Check if the dispatch can be retried.
    pub fn can_retry(&self) -> bool {
        self.retry_count < self.max_retries
    }

    /// Increment the retry count.
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }
}

// =============================================================================
// Dispatch Result
// =============================================================================

/// DispatchResult represents the result of a dispatch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub data: HashMap<String, serde_json::Value>,
    pub duration_ms: i64,
    pub retries: i32,
}

// =============================================================================
// Service Handler
// =============================================================================

/// ServiceHandler is a function that handles service requests.
pub type ServiceHandler = Arc<
    dyn Fn(&DispatchTarget, &HashMap<String, serde_json::Value>) -> Result<DispatchResult, String>
        + Send
        + Sync,
>;

// =============================================================================
// Service Registry
// =============================================================================

/// ServiceRegistry manages service registration, discovery, and dispatch.
///
/// Single-actor implementation for tracking registered services.
#[derive(Debug)]
pub struct ServiceRegistry {
    /// Service registry
    services: HashMap<String, ServiceInfo>,
    /// Dispatch handlers (using type erasure for simplicity)
    handlers: HashMap<String, usize>, // Placeholder - real impl would use ServiceHandler
}

impl ServiceRegistry {
    /// Create a new service registry.
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
            handlers: HashMap::new(),
        }
    }

    // =============================================================================
    // Service Registration
    // =============================================================================

    /// Register a service with the registry.
    /// Returns true if registration succeeded, false if service already exists.
    pub fn register_service(&mut self, info: ServiceInfo) -> bool {
        if self.services.contains_key(&info.name) {
            return false;
        }

        self.services.insert(info.name.clone(), info);
        true
    }

    /// Unregister a service.
    /// Returns true if unregistration succeeded.
    pub fn unregister_service(&mut self, service_name: &str) -> bool {
        if !self.services.contains_key(service_name) {
            return false;
        }

        self.services.remove(service_name);
        self.handlers.remove(service_name);
        true
    }

    /// Register a handler for a service (placeholder).
    /// Full implementation would store Arc<dyn Fn> but simplified for now.
    pub fn register_handler(&mut self, service_name: String) {
        // Placeholder - just mark that handler exists
        self.handlers.insert(service_name, 1);
    }

    // =============================================================================
    // Service Discovery
    // =============================================================================

    /// Get a service by name.
    pub fn get_service(&self, service_name: &str) -> Option<ServiceInfo> {
        self.services.get(service_name).cloned()
    }

    /// List all registered services matching criteria.
    pub fn list_services(&self, service_type: Option<&str>, healthy_only: bool) -> Vec<ServiceInfo> {
        self.services
            .values()
            .filter(|svc| {
                if let Some(stype) = service_type {
                    if svc.service_type != stype {
                        return false;
                    }
                }
                if healthy_only && !svc.is_healthy() {
                    return false;
                }
                true
            })
            .cloned()
            .collect()
    }

    /// Get all service names.
    pub fn get_service_names(&self) -> Vec<String> {
        self.services.keys().cloned().collect()
    }

    /// Check if a service is registered.
    pub fn has_service(&self, service_name: &str) -> bool {
        self.services.contains_key(service_name)
    }

    /// Check if a service has a handler registered.
    pub fn has_handler(&self, service_name: &str) -> bool {
        self.handlers.contains_key(service_name)
    }

    // =============================================================================
    // Load Management
    // =============================================================================

    /// Increment load for a service.
    pub fn increment_load(&mut self, service_name: &str) -> bool {
        if let Some(service) = self.services.get_mut(service_name) {
            service.current_load += 1;
            true
        } else {
            false
        }
    }

    /// Decrement load for a service.
    pub fn decrement_load(&mut self, service_name: &str) -> bool {
        if let Some(service) = self.services.get_mut(service_name) {
            service.current_load = (service.current_load - 1).max(0);
            true
        } else {
            false
        }
    }

    /// Get current load for a service.
    pub fn get_load(&self, service_name: &str) -> i32 {
        self.services
            .get(service_name)
            .map(|s| s.current_load)
            .unwrap_or(0)
    }

    // =============================================================================
    // Health Management
    // =============================================================================

    /// Update health status for a service.
    pub fn update_health(&mut self, service_name: &str, status: ServiceStatus) -> bool {
        if let Some(service) = self.services.get_mut(service_name) {
            service.status = status;
            service.last_health_check = Utc::now();
            true
        } else {
            false
        }
    }

    /// Get count of healthy services.
    pub fn get_healthy_count(&self) -> usize {
        self.services
            .values()
            .filter(|s| s.is_healthy())
            .count()
    }

    // =============================================================================
    // Statistics
    // =============================================================================

    /// Get statistics for a specific service.
    pub fn get_service_stats(&self, service_name: &str) -> Option<ServiceStats> {
        self.services.get(service_name).map(|svc| ServiceStats {
            name: svc.name.clone(),
            service_type: svc.service_type.clone(),
            status: svc.status,
            current_load: svc.current_load,
            max_concurrent: svc.max_concurrent,
            utilization: if svc.max_concurrent > 0 {
                (svc.current_load as f64 / svc.max_concurrent as f64) * 100.0
            } else {
                0.0
            },
        })
    }

    /// Get overall registry statistics.
    pub fn get_stats(&self) -> RegistryStats {
        let mut stats = RegistryStats {
            total_services: self.services.len(),
            healthy_services: 0,
            degraded_services: 0,
            unhealthy_services: 0,
            total_load: 0,
            total_capacity: 0,
            services_by_type: HashMap::new(),
        };

        for svc in self.services.values() {
            match svc.status {
                ServiceStatus::Healthy => stats.healthy_services += 1,
                ServiceStatus::Degraded => stats.degraded_services += 1,
                ServiceStatus::Unhealthy => stats.unhealthy_services += 1,
                ServiceStatus::Unknown => {}
            }

            stats.total_load += svc.current_load as usize;
            stats.total_capacity += svc.max_concurrent as usize;

            *stats
                .services_by_type
                .entry(svc.service_type.clone())
                .or_insert(0) += 1;
        }

        stats
    }

    // =============================================================================
    // Dispatch (Placeholder)
    // =============================================================================

    /// Dispatch a request to a service (placeholder implementation).
    /// Full implementation would invoke the registered handler.
    pub fn dispatch(
        &mut self,
        target: &DispatchTarget,
        _data: &HashMap<String, serde_json::Value>,
    ) -> Result<DispatchResult, String> {
        // Check if service exists and has handler
        if !self.has_service(&target.service_name) {
            return Err(format!("Service not found: {}", target.service_name));
        }

        if !self.has_handler(&target.service_name) {
            return Err(format!("No handler for service: {}", target.service_name));
        }

        // Check if service can accept load
        if let Some(svc) = self.services.get(&target.service_name) {
            if !svc.can_accept() {
                return Err(format!(
                    "Service {} cannot accept more load (current: {}, max: {})",
                    target.service_name, svc.current_load, svc.max_concurrent
                ));
            }
        }

        // Placeholder result
        // Full implementation would:
        // 1. Increment load
        // 2. Invoke handler
        // 3. Decrement load
        // 4. Return result
        Ok(DispatchResult {
            success: true,
            error: None,
            data: HashMap::new(),
            duration_ms: 0,
            retries: target.retry_count,
        })
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Statistics Types
// =============================================================================

/// Statistics for a specific service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStats {
    pub name: String,
    pub service_type: String,
    pub status: ServiceStatus,
    pub current_load: i32,
    pub max_concurrent: i32,
    pub utilization: f64, // Percentage
}

/// Overall registry statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    pub total_services: usize,
    pub healthy_services: usize,
    pub degraded_services: usize,
    pub unhealthy_services: usize,
    pub total_load: usize,
    pub total_capacity: usize,
    pub services_by_type: HashMap<String, usize>,
}
