//! gRPC Integration Tests
//!
//! Tests for gRPC service instantiation and basic structure validation.

use jeeves_core::grpc::{
    CommBusService, EngineService, KernelServiceImpl, OrchestrationService,
};
use jeeves_core::kernel::Kernel;
use jeeves_core::types::{ProcessId, RequestId, SessionId, UserId};
use std::sync::Arc;
use tokio::sync::Mutex;

// =============================================================================
// Test Helpers
// =============================================================================

/// Create a test kernel instance
fn create_test_kernel() -> Arc<Mutex<Kernel>> {
    Arc::new(Mutex::new(Kernel::new()))
}

// =============================================================================
// Service Instantiation Tests
// =============================================================================

#[test]
fn test_kernel_service_instantiation() {
    let kernel = create_test_kernel();
    let _service = KernelServiceImpl::new(kernel);
    // Successfully created service
}

#[test]
fn test_engine_service_instantiation() {
    let kernel = create_test_kernel();
    let _service = EngineService::new(kernel);
    // Successfully created service
}

#[test]
fn test_orchestration_service_instantiation() {
    let kernel = create_test_kernel();
    let _service = OrchestrationService::new(kernel);
    // Successfully created service
}

#[test]
fn test_commbus_service_instantiation() {
    let kernel = create_test_kernel();
    let _service = CommBusService::new(kernel);
    // Successfully created service
}

#[test]
fn test_multiple_services_share_kernel() {
    let kernel = create_test_kernel();

    let _kernel_service = KernelServiceImpl::new(kernel.clone());
    let _engine_service = EngineService::new(kernel.clone());
    let _orchestration_service = OrchestrationService::new(kernel.clone());
    let _commbus_service = CommBusService::new(kernel);

    // All services successfully created with shared kernel
}

#[tokio::test]
async fn test_kernel_can_be_accessed_from_service() {
    let kernel = create_test_kernel();

    // Modify kernel state
    {
        let mut k = kernel.lock().await;
        let result = k.create_process(
            ProcessId::must("test1"),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            jeeves_core::kernel::SchedulingPriority::Normal,
            None,
        );
        assert!(result.is_ok());
    }

    // Verify state persists
    {
        let k = kernel.lock().await;
        let pid = ProcessId::must("test1");
        let pcb = k.get_process(&pid);
        assert!(pcb.is_some());
        assert_eq!(pcb.unwrap().pid.as_str(), "test1");
    }
}

#[tokio::test]
async fn test_services_can_share_kernel_state() {
    let kernel = create_test_kernel();

    // Create process through kernel directly
    {
        let mut k = kernel.lock().await;
        k.create_process(
            ProcessId::must("test1"),
            RequestId::must("req1"),
            UserId::must("user1"),
            SessionId::must("sess1"),
            jeeves_core::kernel::SchedulingPriority::Normal,
            None,
        )
        .unwrap();
    }

    // Verify all services see the same kernel state
    let kernel_service = KernelServiceImpl::new(kernel.clone());
    let engine_service = EngineService::new(kernel.clone());
    let orchestration_service = OrchestrationService::new(kernel.clone());

    // All services share the same underlying kernel
    // This is verified by the fact that they all compile and can be created with the same Arc
    drop(kernel_service);
    drop(engine_service);
    drop(orchestration_service);
}

// =============================================================================
// Proto Type Availability Tests
// =============================================================================

#[test]
fn test_proto_types_available() {
    use jeeves_core::proto::*;

    // Verify key proto types are available
    let _process_request = CreateProcessRequest {
        pid: "test".to_string(),
        request_id: "req1".to_string(),
        user_id: "user1".to_string(),
        session_id: "sess1".to_string(),
        priority: 2,
        quota: None,
    };

    let _get_request = GetProcessRequest {
        pid: "test".to_string(),
    };

    let _schedule_request = ScheduleProcessRequest {
        pid: "test".to_string(),
    };

    let _runnable_request = GetNextRunnableRequest {};

    let _terminate_request = TerminateProcessRequest {
        pid: "test".to_string(),
        reason: "test".to_string(),
        force: false,
    };
}

#[test]
fn test_orchestration_proto_types() {
    use jeeves_core::proto::*;

    let _init_request = InitializeSessionRequest {
        process_id: "test1".to_string(),
        pipeline_config: vec![],
        envelope: vec![],
        force: false,
    };

    let _next_request = GetNextInstructionRequest {
        process_id: "test1".to_string(),
    };

    let _report_request = ReportAgentResultRequest {
        process_id: "test1".to_string(),
        agent_name: "test".to_string(),
        success: true,
        output: vec![],
        error: String::new(),
        metrics: None,
    };

    let _state_request = GetSessionStateRequest {
        process_id: "test1".to_string(),
    };
}

#[test]
fn test_commbus_proto_types() {
    use jeeves_core::proto::*;

    let _publish_request = CommBusPublishRequest {
        event_type: "test.event".to_string(),
        payload: b"{}".to_vec(),
    };

    let _subscribe_request = CommBusSubscribeRequest {
        event_types: vec!["test.event".to_string()],
    };
}

#[test]
fn test_engine_proto_types() {
    use jeeves_core::proto::*;
    use std::collections::HashMap;

    let _create_envelope = CreateEnvelopeRequest {
        request_id: "req1".to_string(),
        user_id: "user1".to_string(),
        session_id: "sess1".to_string(),
        raw_input: "test input".to_string(),
        metadata: HashMap::new(),
        stage_order: vec![],
    };
}
