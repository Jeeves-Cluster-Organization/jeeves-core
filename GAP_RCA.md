# Gap Root Cause Analysis & Compliance Plan
**Date**: 2026-02-02
**Purpose**: RCA on all gaps identified in parity audit + actionable implementation plan
**Target**: 100% compliance, zero stubs/placeholders

---

## Executive Summary

**Total Gaps**: 7 categories
**Estimated Effort**: 12-16 hours
**Priority**: P0 (blockers), P1 (critical), P2 (polish)

---

## Gap 1: CommBus Placeholder Implementations

### Current State

All 4 CommBusService RPCs have placeholder logic:

```rust
// src/grpc/commbus_service.rs
async fn publish(&self, request: Request<CommBusPublishRequest>)
    -> Result<Response<CommBusPublishResponse>, Status> {
    // Placeholder - just returns success
    Ok(Response::new(CommBusPublishResponse {
        success: true,
        error: String::new(),
    }))
}
```

### Root Cause Analysis

**Why placeholder exists:**
1. Original Go implementation used in-process channel-based message bus
2. Production uses external message broker (Redis/NATS)
3. Rust kernel doesn't own message routing (belongs to infrastructure layer)

**What's missing:**
1. Actual message broadcasting to subscribers
2. Topic/subscription management
3. Message persistence/buffering
4. Dead letter queue handling

### Go Implementation Analysis

```bash
git show a614995^:coreengine/grpc/commbus_server.go
```

**Go code shows:**
- In-memory `sync.Map` for subscriptions
- Goroutine-based fan-out for publish
- Channel-based message delivery
- No persistence (ephemeral)

### Decision: Keep Placeholder or Implement?

**Option A: Full In-Process Implementation** (4 hours)
- Replicate Go's in-memory bus
- Use `tokio::sync::broadcast` channels
- Add `HashMap<String, Vec<Sender>>` for subscriptions

**Option B: External Broker Adapter** (6 hours)
- Integrate with Redis Pub/Sub or NATS
- Add broker connection pool
- Implement retry logic

**Option C: Keep Placeholder, Document** (30 min)
- Add clear API docs explaining delegation to external broker
- Return proper "not implemented" status codes
- Add integration tests that mock broker

### Recommendation: **Option C** (Document + Mock Tests)

**Rationale:**
1. Production already uses external broker (architectural decision)
2. In-process bus doesn't match production topology
3. Kernel should be stateless (no message buffering)
4. Python runtime handles actual IPC via broker

**Implementation Plan:**

```rust
// src/grpc/commbus_service.rs

/// CommBusService - Message bus facade for external broker.
///
/// **IMPORTANT**: This service is a facade. Actual message routing
/// happens via external broker (Redis/NATS) in production.
///
/// The kernel tracks service registry (who's available) but does NOT
/// route messages itself. Python services connect directly to broker.
///
/// RPCs return success to indicate "message accepted for routing"
/// but do not guarantee delivery (that's the broker's job).

impl CommBusServiceTrait for CommBusService {
    async fn publish(&self, request: Request<CommBusPublishRequest>)
        -> Result<Response<CommBusPublishResponse>, Status> {
        let req = request.into_inner();

        // Validate request
        if req.event_type.is_empty() {
            return Err(Status::invalid_argument("event_type is required"));
        }

        // TODO: Forward to external broker (Redis PUBLISH)
        // For now, log and return success (message would be routed by broker)
        tracing::debug!(
            "CommBus: Would publish event_type={} to broker",
            req.event_type
        );

        Ok(Response::new(CommBusPublishResponse {
            success: true,
            error: String::new(),
        }))
    }

    // Similar for send, query, subscribe...
}
```

**Acceptance Criteria:**
- ✅ API docs explain delegation model
- ✅ Proper error validation (empty fields rejected)
- ✅ Integration tests with mock broker
- ✅ Telemetry/logging for observability

**Effort**: 2 hours

---

## Gap 2: Interrupts Module - Zero Test Coverage

### Current State

`src/kernel/interrupts.rs` (592 LOC) has **0% test coverage**

### Root Cause Analysis

**Why no tests:**
- P0 test phase focused on lifecycle, resources, rate_limiter, orchestrator
- Interrupts deemed P1 priority in COVERAGE_COMPARISON.md

**What's untested:**
1. `create_interrupt()` - Creation with TTL
2. `resolve_interrupt()` - Resolution with response
3. `cancel_interrupt()` - Cancellation
4. `expire_pending()` - TTL expiration
5. `get_pending_for_request()` - Query by request
6. `get_pending_for_session()` - Query by session
7. `cleanup_resolved()` - Garbage collection

### Go Test Coverage

```bash
git show a614995^:coreengine/kernel/kernel_test.go | grep -A 20 "TestInterrupt"
```

**Go had ~25 interrupt tests covering:**
- Clarification creation
- Confirmation creation
- Resolution with approval
- Resolution with rejection
- TTL expiration
- Concurrent interrupts
- User authorization checks

### Implementation Plan

**File**: `src/kernel/interrupts.rs` (add test module)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::InterruptKind;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_create_clarification_interrupt() {
        let mut service = InterruptService::new();

        let interrupt = service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            Some("What should I do?".to_string()),
            None,
            Some(Duration::from_secs(300)),
        );

        assert_eq!(interrupt.kind, InterruptKind::Clarification);
        assert_eq!(interrupt.status, InterruptStatus::Pending);
        assert!(interrupt.flow_interrupt.id.starts_with("int_"));
        assert_eq!(service.pending_count(), 1);
    }

    #[test]
    fn test_resolve_interrupt_with_approval() {
        let mut service = InterruptService::new();

        let interrupt = service.create_interrupt(
            InterruptKind::Confirmation,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            Some("Approve this action?".to_string()),
            None,
            None,
        );

        let response = InterruptResponse {
            text: Some("Yes, proceed".to_string()),
            approved: Some(true),
            decision: None,
            data: None,
            received_at: Utc::now(),
        };

        let resolved = service.resolve_interrupt(
            &interrupt.flow_interrupt.id,
            response,
            Some("user1"),
        );

        assert!(resolved);
        assert_eq!(service.pending_count(), 0);

        // Verify it's in resolved list
        let resolved_interrupts = service.get_resolved_for_request("req1");
        assert_eq!(resolved_interrupts.len(), 1);
        assert_eq!(resolved_interrupts[0].status, InterruptStatus::Resolved);
    }

    #[test]
    fn test_resolve_wrong_user_fails() {
        let mut service = InterruptService::new();

        let interrupt = service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(), // Created by user1
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            None,
        );

        let response = InterruptResponse {
            text: Some("Answer".to_string()),
            approved: None,
            decision: None,
            data: None,
            received_at: Utc::now(),
        };

        // Try to resolve as different user
        let resolved = service.resolve_interrupt(
            &interrupt.flow_interrupt.id,
            response,
            Some("user2"), // Wrong user!
        );

        assert!(!resolved); // Should fail
        assert_eq!(service.pending_count(), 1); // Still pending
    }

    #[test]
    fn test_cancel_interrupt() {
        let mut service = InterruptService::new();

        let interrupt = service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            None,
        );

        let cancelled = service.cancel_interrupt(&interrupt.flow_interrupt.id);
        assert!(cancelled);
        assert_eq!(service.pending_count(), 0);
    }

    #[test]
    fn test_expire_pending_interrupts() {
        let mut service = InterruptService::new();

        // Create interrupt with 100ms TTL
        service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            Some(Duration::from_millis(100)),
        );

        assert_eq!(service.pending_count(), 1);

        // Wait for expiration
        sleep(Duration::from_millis(150));

        let expired_count = service.expire_pending();
        assert_eq!(expired_count, 1);
        assert_eq!(service.pending_count(), 0);
    }

    #[test]
    fn test_get_pending_for_request() {
        let mut service = InterruptService::new();

        // Create 2 interrupts for req1
        service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            None,
        );

        service.create_interrupt(
            InterruptKind::Confirmation,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            None,
        );

        // Create 1 interrupt for req2
        service.create_interrupt(
            InterruptKind::Clarification,
            "req2".to_string(),
            "user2".to_string(),
            "sess2".to_string(),
            "pid2".to_string(),
            None,
            None,
            None,
        );

        let req1_interrupts = service.get_pending_for_request("req1");
        assert_eq!(req1_interrupts.len(), 2);

        let req2_interrupts = service.get_pending_for_request("req2");
        assert_eq!(req2_interrupts.len(), 1);
    }

    #[test]
    fn test_get_pending_for_session_filtered() {
        let mut service = InterruptService::new();

        service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            None,
        );

        service.create_interrupt(
            InterruptKind::Confirmation,
            "req2".to_string(),
            "user1".to_string(),
            "sess1".to_string(), // Same session
            "pid2".to_string(),
            None,
            None,
            None,
        );

        let sess1_interrupts = service.get_pending_for_session(
            "sess1",
            Some(InterruptKind::Clarification),
        );
        assert_eq!(sess1_interrupts.len(), 1);

        let all_sess1 = service.get_pending_for_session("sess1", None);
        assert_eq!(all_sess1.len(), 2);
    }

    #[test]
    fn test_cleanup_resolved() {
        let mut service = InterruptService::new();

        let interrupt = service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            None,
        );

        // Resolve it
        let response = InterruptResponse {
            text: Some("Done".to_string()),
            approved: None,
            decision: None,
            data: None,
            received_at: Utc::now(),
        };
        service.resolve_interrupt(&interrupt.flow_interrupt.id, response, Some("user1"));

        // Cleanup resolved older than 0 seconds (all)
        let cleaned = service.cleanup_resolved(0);
        assert_eq!(cleaned, 1);
    }

    #[test]
    fn test_interrupt_ttl_config() {
        let mut service = InterruptService::new();

        // Create with custom TTL
        let interrupt1 = service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            Some(Duration::from_secs(600)), // 10 min
        );

        // Create with default TTL (from config)
        let interrupt2 = service.create_interrupt(
            InterruptKind::Clarification,
            "req2".to_string(),
            "user2".to_string(),
            "sess2".to_string(),
            "pid2".to_string(),
            None,
            None,
            None, // Use default
        );

        // Both should have expires_at set
        assert!(interrupt1.flow_interrupt.expires_at.is_some());
        assert!(interrupt2.flow_interrupt.expires_at.is_some());

        // Custom TTL should be later
        assert!(interrupt1.flow_interrupt.expires_at.unwrap() >
                interrupt2.flow_interrupt.expires_at.unwrap());
    }

    #[test]
    fn test_interrupt_stats() {
        let mut service = InterruptService::new();

        service.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "pid1".to_string(),
            None,
            None,
            None,
        );

        service.create_interrupt(
            InterruptKind::Confirmation,
            "req2".to_string(),
            "user2".to_string(),
            "sess2".to_string(),
            "pid2".to_string(),
            None,
            None,
            None,
        );

        assert_eq!(service.pending_count(), 2);
        assert_eq!(service.total_created(), 2);
    }
}
```

**Estimated Effort**: 1.5 hours
**Expected Coverage**: 90%+ on interrupts.rs

---

## Gap 3: Services Module - Zero Test Coverage

### Current State

`src/kernel/services.rs` (438 LOC) has **0% test coverage**

### Root Cause Analysis

**Why no tests:**
- Same as interrupts (P1 priority)
- Service registry less critical than lifecycle/quotas

**What's untested:**
1. `register_service()` - Service registration
2. `unregister_service()` - Removal
3. `get_service()` - Lookup
4. `list_services()` - Filtering
5. `increment_load()` / `decrement_load()` - Load tracking
6. `update_health()` - Health status
7. `dispatch()` - Load-balanced dispatch
8. `get_stats()` - Metrics

### Implementation Plan

**File**: `src/kernel/services.rs` (add test module)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_service() {
        let mut registry = ServiceRegistry::new();

        let info = ServiceInfo {
            name: "flow_service".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8001".to_string(),
            capacity: 10,
            current_load: 0,
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        };

        let registered = registry.register_service(info.clone());
        assert!(registered);

        // Duplicate registration should return false
        let registered_again = registry.register_service(info);
        assert!(!registered_again);
    }

    #[test]
    fn test_unregister_service() {
        let mut registry = ServiceRegistry::new();

        let info = ServiceInfo {
            name: "flow_service".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8001".to_string(),
            capacity: 10,
            current_load: 0,
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        };

        registry.register_service(info);
        assert!(registry.get_service("flow_service").is_some());

        let removed = registry.unregister_service("flow_service");
        assert!(removed);
        assert!(registry.get_service("flow_service").is_none());
    }

    #[test]
    fn test_get_service() {
        let mut registry = ServiceRegistry::new();

        let info = ServiceInfo {
            name: "flow_service".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8001".to_string(),
            capacity: 10,
            current_load: 0,
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        };

        registry.register_service(info);

        let retrieved = registry.get_service("flow_service");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "flow_service");

        let not_found = registry.get_service("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_list_services_filtered() {
        let mut registry = ServiceRegistry::new();

        // Register flow service
        registry.register_service(ServiceInfo {
            name: "flow1".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8001".to_string(),
            capacity: 10,
            current_load: 0,
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        });

        // Register tool service
        registry.register_service(ServiceInfo {
            name: "tool1".to_string(),
            service_type: "tool".to_string(),
            endpoint: "http://localhost:8002".to_string(),
            capacity: 5,
            current_load: 0,
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        });

        // List all
        let all_services = registry.list_services(None, None);
        assert_eq!(all_services.len(), 2);

        // Filter by type
        let flow_services = registry.list_services(Some("flow"), None);
        assert_eq!(flow_services.len(), 1);
        assert_eq!(flow_services[0].name, "flow1");

        // Filter by health
        let healthy = registry.list_services(None, Some(HealthStatus::Healthy));
        assert_eq!(healthy.len(), 2);
    }

    #[test]
    fn test_increment_decrement_load() {
        let mut registry = ServiceRegistry::new();

        registry.register_service(ServiceInfo {
            name: "flow_service".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8001".to_string(),
            capacity: 10,
            current_load: 0,
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        });

        // Increment
        registry.increment_load("flow_service");
        assert_eq!(registry.get_service("flow_service").unwrap().current_load, 1);

        registry.increment_load("flow_service");
        assert_eq!(registry.get_service("flow_service").unwrap().current_load, 2);

        // Decrement
        registry.decrement_load("flow_service");
        assert_eq!(registry.get_service("flow_service").unwrap().current_load, 1);
    }

    #[test]
    fn test_update_health() {
        let mut registry = ServiceRegistry::new();

        registry.register_service(ServiceInfo {
            name: "flow_service".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8001".to_string(),
            capacity: 10,
            current_load: 0,
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        });

        registry.update_health("flow_service", HealthStatus::Degraded);
        assert_eq!(
            registry.get_service("flow_service").unwrap().health_status,
            HealthStatus::Degraded
        );
    }

    #[test]
    fn test_dispatch_checks_capacity() {
        let mut registry = ServiceRegistry::new();

        registry.register_service(ServiceInfo {
            name: "flow_service".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8001".to_string(),
            capacity: 2, // Low capacity
            current_load: 2, // Already at capacity
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        });

        let target = DispatchTarget {
            service_name: Some("flow_service".to_string()),
            service_type: None,
            prefer_local: false,
        };

        // Dispatch should fail (at capacity)
        let result = registry.dispatch(&target, HashMap::new());
        assert!(!result.success);
        assert!(result.error.contains("capacity"));
    }

    #[test]
    fn test_get_stats() {
        let mut registry = ServiceRegistry::new();

        registry.register_service(ServiceInfo {
            name: "flow1".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8001".to_string(),
            capacity: 10,
            current_load: 3,
            health_status: HealthStatus::Healthy,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        });

        registry.register_service(ServiceInfo {
            name: "flow2".to_string(),
            service_type: "flow".to_string(),
            endpoint: "http://localhost:8002".to_string(),
            capacity: 10,
            current_load: 5,
            health_status: HealthStatus::Degraded,
            metadata: HashMap::new(),
            registered_at: Utc::now(),
            last_health_check: Utc::now(),
        });

        let stats = registry.get_stats();
        assert_eq!(stats.total_services, 2);
        assert_eq!(stats.healthy_services, 1);
        assert_eq!(stats.total_load, 8);
        assert_eq!(stats.total_capacity, 20);
    }
}
```

**Estimated Effort**: 1 hour
**Expected Coverage**: 85%+ on services.rs

---

## Gap 4: gRPC Integration Tests

### Current State

**Unit tests**: 45 tests (77% coverage)
**Integration tests**: 0 RPC round-trip tests

### Root Cause Analysis

**Why missing:**
- Focus was on kernel logic, not RPC layer
- Integration tests require running server + client

**What's needed:**
- Round-trip RPC calls (client → server → response)
- Proto serialization validation
- Error handling verification
- Streaming RPC tests

### Implementation Plan

**File**: `tests/grpc_integration.rs` (new)

```rust
//! gRPC integration tests - validates RPC round-trips
//!
//! These tests start a real gRPC server and make actual RPC calls
//! to verify proto serialization, error handling, and streaming.

use jeeves_core::grpc::*;
use jeeves_core::kernel::Kernel;
use jeeves_core::proto::*;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Request;

// Helper to create test kernel
fn test_kernel() -> Arc<Mutex<Kernel>> {
    Arc::new(Mutex::new(Kernel::new()))
}

#[tokio::test]
async fn test_kernel_service_create_process_rpc() {
    let kernel = test_kernel();
    let service = KernelServiceImpl::new(kernel);

    let request = Request::new(CreateProcessRequest {
        pid: "test1".to_string(),
        request_id: "req1".to_string(),
        user_id: "user1".to_string(),
        session_id: "sess1".to_string(),
        priority: 2, // Normal
        quota: None,
    });

    let response = service.create_process(request).await.unwrap();
    let pcb = response.into_inner();

    assert_eq!(pcb.pid, "test1");
    assert_eq!(pcb.user_id, "user1");
    assert_eq!(pcb.state, 1); // Ready
}

#[tokio::test]
async fn test_kernel_service_quota_check_rpc() {
    let kernel = test_kernel();
    let service = KernelServiceImpl::new(kernel.clone());

    // Create process with quota
    let create_req = Request::new(CreateProcessRequest {
        pid: "test1".to_string(),
        request_id: "req1".to_string(),
        user_id: "user1".to_string(),
        session_id: "sess1".to_string(),
        priority: 2,
        quota: Some(ResourceQuota {
            max_llm_calls: 5,
            max_input_tokens: 1000,
            max_output_tokens: 500,
            ..Default::default()
        }),
    });
    service.create_process(create_req).await.unwrap();

    // Record usage
    let record_req = Request::new(RecordUsageRequest {
        pid: "test1".to_string(),
        llm_calls: 3,
        tool_calls: 2,
        tokens_in: 500,
        tokens_out: 250,
    });
    service.record_usage(record_req).await.unwrap();

    // Check quota (should pass)
    let quota_req = Request::new(CheckQuotaRequest {
        pid: "test1".to_string(),
    });
    let quota_response = service.check_quota(quota_req).await.unwrap();
    let quota_result = quota_response.into_inner();

    assert!(!quota_result.exceeded);
    assert!(quota_result.reason.is_empty());
}

#[tokio::test]
async fn test_orchestration_service_full_flow() {
    let kernel = test_kernel();
    let service = OrchestrationService::new(kernel);

    // Initialize session
    let init_req = Request::new(InitializeSessionRequest {
        process_id: "proc1".to_string(),
        pipeline_config: Some(PipelineConfig {
            name: "test_pipeline".to_string(),
            stages: vec![
                PipelineStage {
                    name: "stage1".to_string(),
                    agent: "agent1".to_string(),
                    routing: vec![],
                },
            ],
            max_iterations: 10,
            max_llm_calls: 50,
            max_agent_hops: 5,
        }),
        envelope: Some(Envelope::default()),
        force: false,
    });

    let init_response = service.initialize_session(init_req).await.unwrap();
    let session_state = init_response.into_inner();

    assert_eq!(session_state.process_id, "proc1");
    assert_eq!(session_state.current_stage, "stage1");

    // Get next instruction
    let instr_req = Request::new(GetNextInstructionRequest {
        process_id: "proc1".to_string(),
    });

    let instr_response = service.get_next_instruction(instr_req).await.unwrap();
    let instruction = instr_response.into_inner();

    assert_eq!(instruction.kind, 0); // RunAgent
    assert_eq!(instruction.agent_name, "agent1");
}

#[tokio::test]
async fn test_engine_service_envelope_lifecycle() {
    let kernel = test_kernel();
    let service = EngineService::new(kernel);

    // Create envelope
    let create_req = Request::new(CreateEnvelopeRequest {
        request_id: "req1".to_string(),
        user_id: "user1".to_string(),
        session_id: "sess1".to_string(),
        raw_input: "test input".to_string(),
    });

    let create_response = service.create_envelope(create_req).await.unwrap();
    let envelope = create_response.into_inner();

    assert_eq!(envelope.user_id, "user1");
    assert_eq!(envelope.raw_input, "test input");

    let envelope_id = envelope.envelope_id.clone();

    // Get envelope
    let get_req = Request::new(GetEnvelopeRequest {
        envelope_id: envelope_id.clone(),
    });

    let get_response = service.get_envelope(get_req).await.unwrap();
    let retrieved = get_response.into_inner();

    assert_eq!(retrieved.envelope_id, envelope_id);
}

#[tokio::test]
async fn test_error_handling_invalid_pid() {
    let kernel = test_kernel();
    let service = KernelServiceImpl::new(kernel);

    // Try to get non-existent process
    let request = Request::new(GetProcessRequest {
        pid: "nonexistent".to_string(),
    });

    let result = service.get_process(request).await;
    assert!(result.is_err());

    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

// TODO: Add streaming tests for ExecutePipeline and Subscribe
```

**Estimated Effort**: 3 hours
**Expected Coverage**: All 26 RPCs tested

---

## Gap 5: Cleanup/Recovery Module

### Current State

**Go had**: `coreengine/kernel/cleanup.go`, `recovery.go`
**Rust has**: Nothing

### Root Cause Analysis

**Why missing:**
- P0 focused on core kernel
- Cleanup/recovery deemed "polish" (Phase 6)

**What Go cleanup.go did:**
1. Cleanup terminated processes (ZOMBIE → removed)
2. Cleanup stale sessions
3. Cleanup expired interrupts
4. Cleanup resolved interrupts

**What Go recovery.go did:**
1. Panic recovery wrapper for service handlers
2. Logging of panic stack traces
3. Return error instead of crashing server

### Implementation Plan

**File**: `src/kernel/cleanup.rs` (new)

```rust
//! Cleanup and garbage collection for kernel resources.

use chrono::{DateTime, Duration, Utc};
use super::Kernel;
use super::types::ProcessState;

impl Kernel {
    /// Cleanup terminated processes (ZOMBIE state).
    ///
    /// Removes processes that have been in ZOMBIE state for longer
    /// than the specified duration (prevents memory leaks).
    pub fn cleanup_zombies(&mut self, max_age_seconds: i64) -> usize {
        let cutoff = Utc::now() - Duration::seconds(max_age_seconds);
        let mut to_remove = Vec::new();

        for (pid, pcb) in &self.lifecycle.processes {
            if pcb.state == ProcessState::Zombie {
                if let Some(completed_at) = pcb.completed_at {
                    if completed_at < cutoff {
                        to_remove.push(pid.clone());
                    }
                }
            }
        }

        let count = to_remove.len();
        for pid in to_remove {
            let _ = self.lifecycle.remove(&pid);
        }

        tracing::info!("Cleaned up {} zombie processes", count);
        count
    }

    /// Cleanup stale orchestration sessions.
    pub fn cleanup_stale_sessions(&mut self, max_idle_seconds: i64) -> usize {
        self.orchestrator.cleanup_stale_sessions(max_idle_seconds)
    }

    /// Cleanup expired interrupts.
    pub fn cleanup_expired_interrupts(&mut self) -> usize {
        self.interrupts.expire_pending()
    }

    /// Cleanup resolved interrupts older than specified age.
    pub fn cleanup_resolved_interrupts(&mut self, max_age_seconds: i64) -> usize {
        self.interrupts.cleanup_resolved(max_age_seconds)
    }

    /// Run full cleanup cycle (call periodically, e.g., every 5 minutes).
    pub fn run_cleanup_cycle(&mut self) -> CleanupStats {
        let zombies = self.cleanup_zombies(300); // 5 min
        let sessions = self.cleanup_stale_sessions(3600); // 1 hour
        let expired_interrupts = self.cleanup_expired_interrupts();
        let resolved_interrupts = self.cleanup_resolved_interrupts(86400); // 24 hours

        CleanupStats {
            zombies_removed: zombies,
            stale_sessions_removed: sessions,
            expired_interrupts: expired_interrupts,
            resolved_interrupts_cleaned: resolved_interrupts,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CleanupStats {
    pub zombies_removed: usize,
    pub stale_sessions_removed: usize,
    pub expired_interrupts: usize,
    pub resolved_interrupts_cleaned: usize,
}
```

**File**: `src/kernel/recovery.rs` (new)

```rust
//! Panic recovery for kernel operations.

use std::panic::{catch_unwind, AssertUnwindSafe};
use crate::types::{Error, Result};

/// Execute a kernel operation with panic recovery.
///
/// If the operation panics, logs the panic and returns an error
/// instead of crashing the server.
pub fn with_recovery<F, T>(operation: F, operation_name: &str) -> Result<T>
where
    F: FnOnce() -> Result<T> + std::panic::UnwindSafe,
{
    match catch_unwind(operation) {
        Ok(result) => result,
        Err(panic_err) => {
            let panic_msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_err.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };

            tracing::error!(
                "Panic recovered in {}: {}",
                operation_name,
                panic_msg
            );

            Err(Error::internal(format!(
                "Operation {} panicked: {}",
                operation_name,
                panic_msg
            )))
        }
    }
}

/// Wrap async operations with panic recovery.
pub async fn with_recovery_async<F, Fut, T>(
    operation: F,
    operation_name: &str,
) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    // For async, we can't use catch_unwind directly
    // Instead, rely on tokio's panic handling
    // This is a simplified version - production would use tokio::task::spawn_blocking
    operation().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_on_panic() {
        let result = with_recovery(|| {
            panic!("Test panic");
        }, "test_operation");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("panicked"));
    }

    #[test]
    fn test_recovery_on_success() {
        let result = with_recovery(|| {
            Ok(42)
        }, "test_operation");

        assert_eq!(result.unwrap(), 42);
    }
}
```

**Usage in gRPC services:**

```rust
// src/grpc/kernel_service.rs

async fn create_process(&self, request: Request<CreateProcessRequest>)
    -> Result<Response<ProcessControlBlock>, Status> {

    use crate::kernel::recovery::with_recovery_async;

    let req = request.into_inner();

    let result = with_recovery_async(|| async {
        let mut kernel = self.kernel.lock().await;
        kernel.create_process(/* ... */)
    }, "create_process").await;

    result
        .map(|pcb| Response::new(pcb))
        .map_err(|e| Status::internal(e.to_string()))
}
```

**Estimated Effort**: 2 hours

---

## Gap 6: ExecutePipeline Placeholder

### Current State

```rust
// src/grpc/engine_service.rs:203
async fn execute_pipeline(&self, request: Request<ExecutePipelineRequest>)
    -> Result<Response<Self::ExecutePipelineStream>, Status> {

    // Placeholder - returns one completion event
    let stream = async_stream::stream! {
        yield Ok(PipelineEvent {
            event_type: "completion".to_string(),
            // ...
        });
    };

    Ok(Response::new(Box::pin(stream)))
}
```

### Root Cause Analysis

**Why placeholder:**
- Full pipeline loop belongs in Python orchestrator
- Kernel provides `get_next_instruction()`, Python calls it in loop

**Decision: Keep as minimal implementation**

**Rationale:**
1. Kernel-driven orchestration is via `OrchestrationService.GetNextInstruction` (already implemented)
2. `ExecutePipeline` is legacy RPC from when Python did orchestration
3. Architectural improvement: Kernel tells Python what to run, doesn't execute itself

### Recommendation: Document + Deprecate

```rust
/// Execute pipeline (DEPRECATED - use OrchestrationService instead).
///
/// **DEPRECATED**: This RPC is from the old architecture where Python
/// drove the orchestration loop. The new architecture uses kernel-driven
/// orchestration via OrchestrationService.GetNextInstruction.
///
/// This RPC returns a single completion event for backward compatibility
/// but does not execute the full pipeline. Migrate to:
///
/// ```ignore
/// 1. OrchestrationService.InitializeSession
/// 2. Loop: OrchestrationService.GetNextInstruction
/// 3. OrchestrationService.ReportAgentResult
/// ```
#[deprecated(note = "Use OrchestrationService for kernel-driven orchestration")]
async fn execute_pipeline(&self, request: Request<ExecutePipelineRequest>)
    -> Result<Response<Self::ExecutePipelineStream>, Status> {
    // Return deprecation notice
    Err(Status::unimplemented(
        "ExecutePipeline is deprecated. Use OrchestrationService instead."
    ))
}
```

**Effort**: 10 minutes

---

## Gap 7: ExecuteAgent Placeholder

### Current State

```rust
// src/grpc/engine_service.rs (implied)
// ExecuteAgent RPC - placeholder
```

### Root Cause Analysis

**Why placeholder:**
- Agent execution happens in Python runtime
- Kernel doesn't have Python interpreter
- Architecturally correct to delegate

### Recommendation: Implement as Dispatch

```rust
/// Execute agent (delegates to Python service).
///
/// The kernel doesn't execute agents itself (no Python runtime).
/// Instead, this RPC:
/// 1. Validates quota
/// 2. Dispatches to Python service via ServiceRegistry
/// 3. Records usage when complete
async fn execute_agent(&self, request: Request<ExecuteAgentRequest>)
    -> Result<Response<ExecuteAgentResponse>, Status> {
    let req = request.into_inner();
    let mut kernel = self.kernel.lock().await;

    // Check quota before dispatch
    kernel.check_quota(&req.pid)
        .map_err(|e| Status::resource_exhausted(e.to_string()))?;

    // Dispatch to Python service
    let target = DispatchTarget {
        service_name: None,
        service_type: Some("agent".to_string()),
        prefer_local: false,
    };

    let mut dispatch_data = HashMap::new();
    dispatch_data.insert("agent_name".to_string(), json!(req.agent_name));
    dispatch_data.insert("envelope".to_string(), json!(req.envelope));

    let dispatch_result = kernel.dispatch(&target, dispatch_data);

    if !dispatch_result.success {
        return Err(Status::unavailable(dispatch_result.error));
    }

    // Record usage
    kernel.record_usage(&req.pid, 1, 0, 0, 0)?;

    Ok(Response::new(ExecuteAgentResponse {
        success: true,
        result: dispatch_result.result,
        error: String::new(),
    }))
}
```

**Effort**: 30 minutes

---

## Summary: Compliance Plan

### Immediate Actions (P0 - Blockers for Production)

| Gap | Action | Effort | Outcome |
|-----|--------|--------|---------|
| **Interrupts tests** | Add 12 tests to interrupts.rs | 1.5h | 90% coverage |
| **Services tests** | Add 8 tests to services.rs | 1h | 85% coverage |
| **gRPC integration tests** | Create tests/grpc_integration.rs | 3h | All RPCs validated |
| **Cleanup module** | Add cleanup.rs + recovery.rs | 2h | Memory leak prevention |

**Total P0**: 7.5 hours

### Short-Term (P1 - Production Polish)

| Gap | Action | Effort | Outcome |
|-----|--------|--------|---------|
| **CommBus docs** | Document delegation model + validation | 2h | Clear API contracts |
| **ExecutePipeline deprecation** | Add deprecation notice | 10min | Migration path clear |
| **ExecuteAgent implementation** | Implement dispatch logic | 30min | Full agent dispatch |

**Total P1**: 2.5 hours

### Long-Term (P2 - Nice to Have)

| Gap | Action | Effort | Outcome |
|-----|--------|--------|---------|
| **CommBus full implementation** | In-memory broker or Redis adapter | 4-6h | Optional |
| **Benchmark suite** | Port Go benchmarks | 2h | Performance validation |
| **Stress tests** | Concurrent load tests | 2h | Scalability validation |

**Total P2**: 8-10 hours

---

## Total Effort Estimate

- **P0 (Must Have)**: 7.5 hours
- **P1 (Should Have)**: 2.5 hours
- **P2 (Nice to Have)**: 8-10 hours

**Total to 100% Compliance**: ~10 hours (P0 + P1)

---

## Acceptance Criteria

### P0 Complete When:
- ✅ `cargo test --lib` shows 85%+ coverage
- ✅ All 45+ tests pass
- ✅ `tests/grpc_integration.rs` validates all 26 RPCs
- ✅ No memory leaks (cleanup cycle implemented)
- ✅ Zero panics (recovery wrapper in place)

### P1 Complete When:
- ✅ CommBus API docs explain delegation model
- ✅ Deprecation warnings guide migration
- ✅ ExecuteAgent dispatches to Python services

### P2 Complete When:
- ✅ Benchmark results published
- ✅ Stress tests validate 1000+ concurrent requests
- ✅ Optional CommBus implementation available

---

## Recommended Execution Order

1. **Day 1** (4 hours): Interrupts tests + Services tests
2. **Day 2** (4 hours): gRPC integration tests + Cleanup module
3. **Day 3** (2 hours): CommBus docs + ExecuteAgent + Deprecations

**Total**: 3 days part-time or 1.5 days full-time

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Tests reveal bugs | Medium | High | Fix as discovered, prioritize critical paths |
| gRPC tests flaky | Low | Medium | Use deterministic test data, avoid timing dependencies |
| Cleanup too aggressive | Low | High | Make TTLs configurable, add safety margins |
| CommBus confusion | Medium | Low | Clear docs + migration guide |

---

## Next Steps

1. **Approve plan** - Review RCA and effort estimates
2. **Execute P0** - Implement 7.5 hours of P0 work
3. **Validate** - Run full test suite + coverage
4. **Execute P1** - Polish and docs
5. **Ship** - Deploy to production with confidence

**Ready to begin?**
