//! Service registry - IPC coordinator and service discovery.
//!
//! Features:
//!   - Service registration and discovery
//!   - Health tracking
//!   - Load balancing

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
pub const SERVICE_TYPE_INFERENCE: &str = "inference";

// =============================================================================
// Service Info
// =============================================================================

/// ServiceInfo describes a registered service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    // Identity
    pub name: String,
    pub service_type: String, // "flow", "worker", "inference"
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
        matches!(
            self.status,
            ServiceStatus::Healthy | ServiceStatus::Degraded
        )
    }
}

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
}

impl ServiceRegistry {
    /// Create a new service registry.
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
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
        true
    }

    // =============================================================================
    // Service Discovery
    // =============================================================================

    /// Get a service by name.
    pub fn get_service(&self, service_name: &str) -> Option<ServiceInfo> {
        self.services.get(service_name).cloned()
    }

    /// List all registered services matching criteria.
    pub fn list_services(
        &self,
        service_type: Option<&str>,
        healthy_only: bool,
    ) -> Vec<ServiceInfo> {
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
        self.services.values().filter(|s| s.is_healthy()).count()
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_service() {
        let mut registry = ServiceRegistry::new();

        let service = ServiceInfo::new("flow_service".to_string(), SERVICE_TYPE_FLOW.to_string());
        let success = registry.register_service(service.clone());
        assert!(success);

        // Verify service is registered
        let retrieved = registry.get_service("flow_service");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "flow_service");

        // Try to register again - should fail
        let duplicate = registry.register_service(service);
        assert!(!duplicate);
    }

    #[test]
    fn test_unregister_service() {
        let mut registry = ServiceRegistry::new();

        let service = ServiceInfo::new("flow_service".to_string(), SERVICE_TYPE_FLOW.to_string());
        registry.register_service(service);

        // Verify service exists
        assert!(registry.has_service("flow_service"));

        // Unregister
        let success = registry.unregister_service("flow_service");
        assert!(success);

        // Verify service is gone
        assert!(!registry.has_service("flow_service"));

        // Try to unregister again - should fail
        let not_found = registry.unregister_service("flow_service");
        assert!(!not_found);
    }

    #[test]
    fn test_get_service() {
        let mut registry = ServiceRegistry::new();

        let service = ServiceInfo::new("worker1".to_string(), SERVICE_TYPE_WORKER.to_string());
        registry.register_service(service);

        // Get existing service
        let retrieved = registry.get_service("worker1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().service_type, SERVICE_TYPE_WORKER);

        // Get non-existent service
        let not_found = registry.get_service("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_list_services_filtered() {
        let mut registry = ServiceRegistry::new();

        // Register multiple services of different types
        let flow1 = ServiceInfo::new("flow1".to_string(), SERVICE_TYPE_FLOW.to_string());
        let flow2 = ServiceInfo::new("flow2".to_string(), SERVICE_TYPE_FLOW.to_string());
        let worker1 = ServiceInfo::new("worker1".to_string(), SERVICE_TYPE_WORKER.to_string());

        registry.register_service(flow1);
        registry.register_service(flow2);
        registry.register_service(worker1);

        // Mark one as unhealthy
        registry.update_health("flow2", ServiceStatus::Unhealthy);

        // List all services
        let all = registry.list_services(None, false);
        assert_eq!(all.len(), 3);

        // List only flow services
        let flows = registry.list_services(Some(SERVICE_TYPE_FLOW), false);
        assert_eq!(flows.len(), 2);

        // List only healthy services
        let healthy = registry.list_services(None, true);
        assert_eq!(healthy.len(), 2);

        // List healthy flow services
        let healthy_flows = registry.list_services(Some(SERVICE_TYPE_FLOW), true);
        assert_eq!(healthy_flows.len(), 1);
        assert_eq!(healthy_flows[0].name, "flow1");
    }

    #[test]
    fn test_increment_decrement_load() {
        let mut registry = ServiceRegistry::new();

        let service = ServiceInfo::new("worker1".to_string(), SERVICE_TYPE_WORKER.to_string());
        registry.register_service(service);

        // Initial load is 0
        assert_eq!(registry.get_load("worker1"), 0);

        // Increment load
        let success = registry.increment_load("worker1");
        assert!(success);
        assert_eq!(registry.get_load("worker1"), 1);

        // Increment again
        registry.increment_load("worker1");
        assert_eq!(registry.get_load("worker1"), 2);

        // Decrement load
        let success = registry.decrement_load("worker1");
        assert!(success);
        assert_eq!(registry.get_load("worker1"), 1);

        // Decrement to zero
        registry.decrement_load("worker1");
        assert_eq!(registry.get_load("worker1"), 0);

        // Decrement below zero (should stay at 0)
        registry.decrement_load("worker1");
        assert_eq!(registry.get_load("worker1"), 0);

        // Try to increment non-existent service
        let not_found = registry.increment_load("nonexistent");
        assert!(!not_found);
    }

    #[test]
    fn test_update_health() {
        let mut registry = ServiceRegistry::new();

        let service = ServiceInfo::new("worker1".to_string(), SERVICE_TYPE_WORKER.to_string());
        registry.register_service(service);

        // Initial status is healthy
        let svc = registry.get_service("worker1").unwrap();
        assert_eq!(svc.status, ServiceStatus::Healthy);

        // Update to degraded
        let success = registry.update_health("worker1", ServiceStatus::Degraded);
        assert!(success);
        assert_eq!(
            registry.get_service("worker1").unwrap().status,
            ServiceStatus::Degraded
        );

        // Update to unhealthy
        registry.update_health("worker1", ServiceStatus::Unhealthy);
        assert_eq!(
            registry.get_service("worker1").unwrap().status,
            ServiceStatus::Unhealthy
        );

        // Try to update non-existent service
        let not_found = registry.update_health("nonexistent", ServiceStatus::Healthy);
        assert!(!not_found);
    }

    #[test]
    fn test_get_stats() {
        let mut registry = ServiceRegistry::new();

        // Register multiple services
        let flow1 = ServiceInfo::new("flow1".to_string(), SERVICE_TYPE_FLOW.to_string());
        let mut flow2 = ServiceInfo::new("flow2".to_string(), SERVICE_TYPE_FLOW.to_string());
        flow2.current_load = 3;
        flow2.max_concurrent = 5;

        let mut worker1 = ServiceInfo::new("worker1".to_string(), SERVICE_TYPE_WORKER.to_string());
        worker1.current_load = 2;
        worker1.max_concurrent = 10;

        registry.register_service(flow1);
        registry.register_service(flow2);
        registry.register_service(worker1);

        // Update health statuses
        registry.update_health("flow2", ServiceStatus::Degraded);

        // Get overall stats
        let stats = registry.get_stats();
        assert_eq!(stats.total_services, 3);
        assert_eq!(stats.healthy_services, 2);
        assert_eq!(stats.degraded_services, 1);
        assert_eq!(stats.unhealthy_services, 0);
        assert_eq!(stats.total_load, 5); // 0 + 3 + 2
        assert_eq!(stats.total_capacity, 25); // 10 + 5 + 10
        assert_eq!(stats.services_by_type.get(SERVICE_TYPE_FLOW), Some(&2));
        assert_eq!(stats.services_by_type.get(SERVICE_TYPE_WORKER), Some(&1));

        // Get service-specific stats
        let flow2_stats = registry.get_service_stats("flow2").unwrap();
        assert_eq!(flow2_stats.name, "flow2");
        assert_eq!(flow2_stats.current_load, 3);
        assert_eq!(flow2_stats.max_concurrent, 5);
        assert_eq!(flow2_stats.utilization, 60.0); // 3/5 * 100
    }

    #[test]
    fn test_service_can_accept() {
        let mut service = ServiceInfo::new("test".to_string(), SERVICE_TYPE_FLOW.to_string());
        service.max_concurrent = 5;
        service.current_load = 3;

        // Healthy and below capacity
        assert!(service.can_accept());

        // At capacity
        service.current_load = 5;
        assert!(!service.can_accept());

        // Unhealthy
        service.current_load = 3;
        service.status = ServiceStatus::Unhealthy;
        assert!(!service.can_accept());

        // Degraded but below capacity (should accept)
        service.status = ServiceStatus::Degraded;
        assert!(service.is_healthy()); // Degraded is considered healthy for is_healthy()
    }
}
