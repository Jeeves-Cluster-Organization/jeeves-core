//! IPC integration tests — validates codec→dispatch→kernel→response round-trip.

use jeeves_core::ipc::codec::{
    write_frame, MSG_ERROR, MSG_REQUEST, MSG_RESPONSE, MSG_STREAM_CHUNK,
};
use jeeves_core::ipc::IpcServer;
use jeeves_core::kernel::Kernel;
use jeeves_core::commbus::QueryResponse;
use jeeves_core::IpcConfig;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

/// Helper: spin up an IpcServer on a random port, return (addr, server_task).
async fn start_test_server_with(
    kernel: Kernel,
    ipc_config: IpcConfig,
) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    // Bind temporarily to get a free port, then drop immediately
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let handle = tokio::spawn(async move {
        let server = IpcServer::new(kernel, addr, ipc_config);
        let _ = server.serve().await;
    });

    // Give the server a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    (addr, handle)
}

/// Helper: spin up server with default Kernel + IpcConfig.
async fn start_test_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    start_test_server_with(Kernel::new(), IpcConfig::default()).await
}

/// Helper: send an already-built request payload.
async fn send_raw_request(stream: &mut TcpStream, request: serde_json::Value) {
    let payload = rmp_serde::to_vec_named(&request).unwrap();
    write_frame(stream, MSG_REQUEST, &payload).await.unwrap();
}

/// Helper: read one response frame.
async fn read_response(stream: &mut TcpStream) -> (u8, serde_json::Value) {
    // Read response frame
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.unwrap();
    let frame_len = u32::from_be_bytes(len_buf) as usize;
    let mut frame_data = vec![0u8; frame_len];
    stream.read_exact(&mut frame_data).await.unwrap();

    let msg_type = frame_data[0];
    let response: serde_json::Value = rmp_serde::from_slice(&frame_data[1..]).unwrap();
    (msg_type, response)
}

/// Helper: send a request frame, receive and decode the response.
async fn round_trip(
    stream: &mut TcpStream,
    service: &str,
    method: &str,
    body: serde_json::Value,
) -> (u8, serde_json::Value) {
    let request = serde_json::json!({
        "id": "test-1",
        "service": service,
        "method": method,
        "body": body,
    });

    send_raw_request(stream, request).await;
    read_response(stream).await
}

#[tokio::test]
async fn test_create_process_round_trip() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let body = serde_json::json!({
        "pid": "test-proc-1",
        "request_id": "req-1",
        "user_id": "user-1",
        "session_id": "sess-1",
        "priority": "NORMAL",
        "quota": {
            "max_llm_calls": 100,
            "max_tool_calls": 50,
            "max_agent_hops": 10,
            "max_iterations": 20,
            "timeout_seconds": 300,
        },
    });

    let (msg_type, response) = round_trip(&mut stream, "kernel", "CreateProcess", body).await;

    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    let resp_body = response.get("body").unwrap();
    assert_eq!(
        resp_body.get("pid").unwrap().as_str().unwrap(),
        "test-proc-1"
    );
    // create_process returns the PCB from submit() — state is NEW
    // (schedule runs after, but the returned clone is from before scheduling)
    assert_eq!(resp_body.get("state").unwrap().as_str().unwrap(), "NEW");
}

#[tokio::test]
async fn test_unknown_service_returns_error() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let (msg_type, response) =
        round_trip(&mut stream, "nonexistent", "Foo", serde_json::json!({})).await;

    assert_eq!(msg_type, MSG_ERROR);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), false);
    let error = response.get("error").unwrap();
    assert_eq!(error.get("code").unwrap().as_str().unwrap(), "NOT_FOUND");
}

#[tokio::test]
async fn test_missing_id_returns_error_and_connection_stays_open() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Missing id should return INVALID_ARGUMENT with empty correlation id.
    send_raw_request(
        &mut stream,
        serde_json::json!({
            "service": "kernel",
            "method": "GetSystemStatus",
            "body": {},
        }),
    )
    .await;
    let (msg_type, response) = read_response(&mut stream).await;
    assert_eq!(msg_type, MSG_ERROR);
    assert_eq!(response.get("id").unwrap().as_str().unwrap(), "");
    let error = response.get("error").unwrap();
    assert_eq!(error.get("code").unwrap().as_str().unwrap(), "INVALID_ARGUMENT");
    assert!(error.get("message").unwrap().as_str().unwrap().contains("id"));

    // The same connection should remain usable.
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "GetSystemStatus",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
}

#[tokio::test]
async fn test_queue_full_returns_resource_exhausted_and_connection_stays_open() {
    let kernel = Kernel::new();
    let mut query_rx = kernel
        .register_query_handler("slow.query".to_string())
        .await
        .unwrap();

    tokio::spawn(async move {
        while let Some((_query, response_tx)) = query_rx.recv().await {
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            let _ = response_tx.send(QueryResponse {
                success: true,
                result: b"{}".to_vec(),
                error: String::new(),
            });
        }
    });

    let mut ipc_config = IpcConfig::default();
    ipc_config.kernel_queue_capacity = 1;

    let (addr, _handle) = start_test_server_with(kernel, ipc_config.clone()).await;
    let mut stream1 = TcpStream::connect(addr).await.unwrap();
    let mut stream2 = TcpStream::connect(addr).await.unwrap();
    let mut stream3 = TcpStream::connect(addr).await.unwrap();

    let slow_query_body = serde_json::json!({
        "query_type": "slow.query",
        "payload": "{}",
        "source": "integration-test",
        "timeout_ms": 2_000,
    });

    send_raw_request(
        &mut stream1,
        serde_json::json!({
            "id": "q1",
            "service": "commbus",
            "method": "Query",
            "body": slow_query_body.clone(),
        }),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    send_raw_request(
        &mut stream2,
        serde_json::json!({
            "id": "q2",
            "service": "commbus",
            "method": "Query",
            "body": slow_query_body.clone(),
        }),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    send_raw_request(
        &mut stream3,
        serde_json::json!({
            "id": "q3",
            "service": "commbus",
            "method": "Query",
            "body": slow_query_body,
        }),
    )
    .await;

    let (msg_type, response) = read_response(&mut stream3).await;
    assert_eq!(msg_type, MSG_ERROR);
    assert_eq!(response.get("id").unwrap().as_str().unwrap(), "q3");
    let error = response.get("error").unwrap();
    assert_eq!(
        error.get("code").unwrap().as_str().unwrap(),
        "RESOURCE_EXHAUSTED"
    );
    assert_eq!(error.get("retryable").unwrap().as_bool().unwrap(), true);
    assert_eq!(
        error.get("kernel_queue_capacity").unwrap().as_u64().unwrap(),
        ipc_config.kernel_queue_capacity as u64
    );

    // Drain first two responses so the actor catches up.
    let (msg_type, _) = read_response(&mut stream1).await;
    assert_eq!(msg_type, MSG_RESPONSE);
    let (msg_type, _) = read_response(&mut stream2).await;
    assert_eq!(msg_type, MSG_RESPONSE);

    // The saturated connection remains usable.
    let (msg_type, response) = round_trip(
        &mut stream3,
        "kernel",
        "GetSystemStatus",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
}

#[tokio::test]
async fn test_kernel_shared_state() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Create process
    let body = serde_json::json!({
        "pid": "shared-test",
        "request_id": "req-s",
        "user_id": "user-s",
        "session_id": "sess-s",
        "priority": "NORMAL",
    });
    let (msg_type, _) = round_trip(&mut stream, "kernel", "CreateProcess", body).await;
    assert_eq!(msg_type, MSG_RESPONSE);

    // Get it back
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "GetProcess",
        serde_json::json!({"pid": "shared-test"}),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    let resp_body = response.get("body").unwrap();
    assert_eq!(
        resp_body.get("pid").unwrap().as_str().unwrap(),
        "shared-test"
    );
}

// =============================================================================
// Orchestration service tests
// =============================================================================

/// Helper: Build a test envelope JSON value (no IPC — pure construction).
fn build_test_envelope() -> serde_json::Value {
    serde_json::json!({
        "identity": {
            "envelope_id": format!("env_{}", uuid::Uuid::new_v4().simple()),
            "request_id": "req-e1",
            "user_id": "user-e1",
            "session_id": "sess-e1",
        },
        "raw_input": "test input",
        "received_at": chrono::Utc::now().to_rfc3339(),
        "outputs": {},
        "pipeline": {
            "current_stage": "",
            "stage_order": [],
            "iteration": 0,
            "max_iterations": 3,
            "active_stages": [],
            "completed_stage_set": [],
            "failed_stages": {},
            "parallel_mode": false,
        },
        "bounds": {
            "llm_call_count": 0,
            "max_llm_calls": 10,
            "tool_call_count": 0,
            "agent_hop_count": 0,
            "max_agent_hops": 21,
            "tokens_in": 0,
            "tokens_out": 0,
            "terminated": false,
        },
        "interrupts": {
            "interrupt_pending": false,
        },
        "execution": {
            "completed_stages": [],
            "current_stage_number": 1,
            "max_stages": 5,
            "all_goals": [],
            "remaining_goals": [],
            "goal_completion_status": {},
            "prior_plans": [],
            "loop_feedback": [],
        },
        "audit": {
            "processing_history": [],
            "errors": [],
            "created_at": chrono::Utc::now().to_rfc3339(),
            "metadata": {},
        },
    })
}

#[tokio::test]
async fn test_report_agent_result_failure_records_error_and_keeps_unknown_tokens() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let envelope = build_test_envelope();
    let process_id = "proc-report-failure-1";

    let pipeline_config = serde_json::json!({
        "name": "test-pipeline",
        "agents": [
            {
                "name": "classify",
                "agent": "classifier",
                "routing": [
                    {"target": "respond", "condition": null, "value": null}
                ]
            },
            {
                "name": "respond",
                "agent": "responder",
                "routing": []
            }
        ],
        "max_iterations": 5,
        "max_llm_calls": 100,
        "max_agent_hops": 20,
        "edge_limits": []
    });

    let (msg_type, response) = round_trip(
        &mut stream,
        "orchestration",
        "InitializeSession",
        serde_json::json!({
            "process_id": process_id,
            "pipeline_config": pipeline_config,
            "envelope": envelope,
        }),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);

    let (msg_type, response) = round_trip(
        &mut stream,
        "orchestration",
        "ReportAgentResult",
        serde_json::json!({
            "process_id": process_id,
            "agent_name": "classifier",
            "success": false,
            "error": "simulated agent failure",
            "metrics": {
                "llm_calls": 0,
                "tool_calls": 0,
                "duration_ms": 5
            }
        }),
    )
    .await;

    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    let body = response.get("body").unwrap();
    assert_eq!(body.get("kind").unwrap().as_str().unwrap(), "RUN_AGENT");
    assert_eq!(body.get("agent_name").unwrap().as_str().unwrap(), "responder");

    let (msg_type, response) = round_trip(
        &mut stream,
        "orchestration",
        "GetSessionState",
        serde_json::json!({
            "process_id": process_id
        }),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    let body = response.get("body").unwrap();
    let env = body.get("envelope").unwrap();
    let agent_output = env
        .get("outputs")
        .and_then(|v| v.get("classifier"))
        .unwrap();
    assert_eq!(agent_output.get("success").unwrap().as_bool().unwrap(), false);
    assert_eq!(
        agent_output.get("error").unwrap().as_str().unwrap(),
        "simulated agent failure"
    );
    assert_eq!(
        env.get("bounds")
            .and_then(|b| b.get("tokens_in"))
            .and_then(|v| v.as_i64())
            .unwrap(),
        0
    );
    assert_eq!(
        env.get("bounds")
            .and_then(|b| b.get("tokens_out"))
            .and_then(|v| v.as_i64())
            .unwrap(),
        0
    );
}

// =============================================================================
// CommBus service tests
// =============================================================================

#[tokio::test]
async fn test_commbus_publish() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let (msg_type, response) = round_trip(
        &mut stream,
        "commbus",
        "Publish",
        serde_json::json!({
            "event_type": "test.event",
            "payload": "{}",
            "source": "integration-test",
        }),
    )
    .await;

    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    let body = response.get("body").unwrap();
    assert_eq!(body.get("success").unwrap().as_bool().unwrap(), true);
}

#[tokio::test]
async fn test_commbus_send() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let (msg_type, _response) = round_trip(
        &mut stream,
        "commbus",
        "Send",
        serde_json::json!({
            "command_type": "test.command",
            "payload": "{\"key\": \"value\"}",
            "source": "integration-test",
        }),
    )
    .await;

    // Send with no registered handler returns an error
    assert_eq!(msg_type, MSG_ERROR);
}

#[tokio::test]
async fn test_commbus_query_no_handler() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let (msg_type, _response) = round_trip(
        &mut stream,
        "commbus",
        "Query",
        serde_json::json!({
            "query_type": "test.query",
            "payload": "{}",
            "source": "integration-test",
            "timeout_ms": 500,
        }),
    )
    .await;

    // Query with no registered handler returns an error
    assert_eq!(msg_type, MSG_ERROR);
}

#[tokio::test]
async fn test_commbus_subscribe_stream() {
    let (addr, _handle) = start_test_server().await;

    // Connection 1: subscribe to events
    let mut sub_stream = TcpStream::connect(addr).await.unwrap();
    let sub_request = serde_json::json!({
        "id": "sub-1",
        "service": "commbus",
        "method": "Subscribe",
        "body": {
            "event_types": ["test.stream"],
            "subscriber_id": "test-sub-1",
        },
    });
    let payload = rmp_serde::to_vec_named(&sub_request).unwrap();
    write_frame(&mut sub_stream, MSG_REQUEST, &payload)
        .await
        .unwrap();

    // Give subscription time to register
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Connection 2: publish events that the subscriber is listening for
    let mut pub_stream = TcpStream::connect(addr).await.unwrap();
    for i in 0..3 {
        let (msg_type, response) = round_trip(
            &mut pub_stream,
            "commbus",
            "Publish",
            serde_json::json!({
                "event_type": "test.stream",
                "payload": format!("{{\"seq\": {}}}", i),
                "source": "stream-test",
            }),
        )
        .await;
        assert_eq!(msg_type, MSG_RESPONSE);
        assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    }

    // Read stream chunks from the subscriber connection
    let mut chunks = Vec::new();
    for _ in 0..3 {
        let mut len_buf = [0u8; 4];
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sub_stream.read_exact(&mut len_buf),
        )
        .await
        .expect("Timed out waiting for stream chunk")
        .unwrap();

        let frame_len = u32::from_be_bytes(len_buf) as usize;
        let mut frame_data = vec![0u8; frame_len];
        sub_stream.read_exact(&mut frame_data).await.unwrap();

        assert_eq!(frame_data[0], MSG_STREAM_CHUNK);
        let chunk: serde_json::Value = rmp_serde::from_slice(&frame_data[1..]).unwrap();
        chunks.push(chunk);
    }

    assert_eq!(chunks.len(), 3);
    // Each chunk should have the event_type in the body
    for chunk in &chunks {
        let body = chunk.get("body").unwrap();
        assert_eq!(
            body.get("event_type").unwrap().as_str().unwrap(),
            "test.stream"
        );
        assert_eq!(body.get("source").unwrap().as_str().unwrap(), "stream-test");
    }

    // Drop the subscriber — server should send MSG_STREAM_END when it detects disconnect,
    // but since we're closing from the client side, we just verify the chunks arrived.
    drop(sub_stream);
}

// =============================================================================
// Phase 2 IPC tests — quota defaults and system status
// =============================================================================

#[tokio::test]
async fn test_set_and_get_quota_defaults() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // SetQuotaDefaults with max_llm_calls=200
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "SetQuotaDefaults",
        serde_json::json!({
            "quota": {
                "max_llm_calls": 200,
            },
        }),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    let body = response.get("body").unwrap();
    assert_eq!(body.get("max_llm_calls").unwrap().as_i64().unwrap(), 200);

    // GetQuotaDefaults → same merged result
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "GetQuotaDefaults",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    let body = response.get("body").unwrap();
    assert_eq!(body.get("max_llm_calls").unwrap().as_i64().unwrap(), 200);
}

#[tokio::test]
async fn test_get_system_status_round_trip() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // CreateProcess (submit + schedule → Ready)
    let (msg_type, _) = round_trip(
        &mut stream,
        "kernel",
        "CreateProcess",
        serde_json::json!({
            "pid": "status-proc-1",
            "request_id": "req-st1",
            "user_id": "user-st1",
            "session_id": "sess-st1",
            "priority": "NORMAL",
        }),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);

    // GetSystemStatus → total=1, by_state has READY=1
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "GetSystemStatus",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    let body = response.get("body").unwrap();
    let processes = body.get("processes").unwrap();
    assert_eq!(processes.get("total").unwrap().as_i64().unwrap(), 1);
    let by_state = processes.get("by_state").unwrap();
    assert_eq!(by_state.get("READY").unwrap().as_i64().unwrap(), 1);
}

#[tokio::test]
async fn test_quota_defaults_merge_semantics() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // GetQuotaDefaults → note original max_tool_calls (50)
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "GetQuotaDefaults",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    let body = response.get("body").unwrap();
    assert_eq!(body.get("max_tool_calls").unwrap().as_i64().unwrap(), 50);

    // SetQuotaDefaults with only max_llm_calls=999
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "SetQuotaDefaults",
        serde_json::json!({
            "quota": {
                "max_llm_calls": 999,
            },
        }),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    let body = response.get("body").unwrap();
    assert_eq!(body.get("max_llm_calls").unwrap().as_i64().unwrap(), 999);

    // GetQuotaDefaults → max_tool_calls still 50
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "GetQuotaDefaults",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);
    let body = response.get("body").unwrap();
    assert_eq!(body.get("max_tool_calls").unwrap().as_i64().unwrap(), 50);
    assert_eq!(body.get("max_llm_calls").unwrap().as_i64().unwrap(), 999);
}

// =============================================================================
// Phase 0B: Input validation — negative values, overflow
// =============================================================================

#[tokio::test]
async fn test_record_usage_rejects_negative_llm_calls() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Create a process first
    let (msg_type, _) = round_trip(
        &mut stream,
        "kernel",
        "CreateProcess",
        serde_json::json!({
            "pid": "neg-test-1",
            "user_id": "user-neg",
        }),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);

    // RecordUsage with negative llm_calls
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "RecordUsage",
        serde_json::json!({
            "pid": "neg-test-1",
            "llm_calls": -5,
            "tool_calls": 0,
            "tokens_in": 0,
            "tokens_out": 0,
        }),
    )
    .await;

    assert_eq!(msg_type, MSG_ERROR);
    let error = response.get("error").unwrap();
    assert_eq!(
        error.get("code").unwrap().as_str().unwrap(),
        "INVALID_ARGUMENT"
    );
    let msg = error.get("message").unwrap().as_str().unwrap();
    assert!(
        msg.contains("llm_calls"),
        "Error should mention field name: {}",
        msg
    );
}

#[tokio::test]
async fn test_record_usage_rejects_negative_tokens() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let (msg_type, _) = round_trip(
        &mut stream,
        "kernel",
        "CreateProcess",
        serde_json::json!({
            "pid": "neg-test-2",
            "user_id": "user-neg2",
        }),
    )
    .await;
    assert_eq!(msg_type, MSG_RESPONSE);

    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "RecordUsage",
        serde_json::json!({
            "pid": "neg-test-2",
            "llm_calls": 1,
            "tool_calls": 1,
            "tokens_in": -100,
            "tokens_out": 0,
        }),
    )
    .await;

    assert_eq!(msg_type, MSG_ERROR);
    let error = response.get("error").unwrap();
    assert_eq!(
        error.get("code").unwrap().as_str().unwrap(),
        "INVALID_ARGUMENT"
    );
    let msg = error.get("message").unwrap().as_str().unwrap();
    assert!(
        msg.contains("tokens_in"),
        "Error should mention field name: {}",
        msg
    );
}

#[tokio::test]
async fn test_create_process_quota_rejects_overflow() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Quota value exceeding i32::MAX
    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "CreateProcess",
        serde_json::json!({
            "pid": "overflow-test",
            "user_id": "user-of",
            "quota": {
                "max_llm_calls": 3_000_000_000_i64,
            },
        }),
    )
    .await;

    assert_eq!(msg_type, MSG_ERROR);
    let error = response.get("error").unwrap();
    assert_eq!(
        error.get("code").unwrap().as_str().unwrap(),
        "INVALID_ARGUMENT"
    );
    let msg = error.get("message").unwrap().as_str().unwrap();
    assert!(
        msg.contains("max_llm_calls"),
        "Error should mention field name: {}",
        msg
    );
}

#[tokio::test]
async fn test_create_process_quota_rejects_negative() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let (msg_type, response) = round_trip(
        &mut stream,
        "kernel",
        "CreateProcess",
        serde_json::json!({
            "pid": "neg-quota-test",
            "user_id": "user-nq",
            "quota": {
                "max_llm_calls": 100,
                "timeout_seconds": -1,
            },
        }),
    )
    .await;

    assert_eq!(msg_type, MSG_ERROR);
    let error = response.get("error").unwrap();
    assert_eq!(
        error.get("code").unwrap().as_str().unwrap(),
        "INVALID_ARGUMENT"
    );
    let msg = error.get("message").unwrap().as_str().unwrap();
    assert!(
        msg.contains("timeout_seconds"),
        "Error should mention field name: {}",
        msg
    );
}
