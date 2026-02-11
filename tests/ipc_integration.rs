//! IPC integration tests — validates codec→dispatch→kernel→response round-trip.

use jeeves_core::ipc::codec::{
    write_frame, MSG_ERROR, MSG_REQUEST, MSG_RESPONSE, MSG_STREAM_CHUNK,
};
use jeeves_core::ipc::IpcServer;
use jeeves_core::kernel::Kernel;
use jeeves_core::IpcConfig;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Helper: spin up an IpcServer on a random port, return (addr, server_task).
async fn start_test_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let kernel = Arc::new(Mutex::new(Kernel::new()));

    // Bind temporarily to get a free port, then drop immediately
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let handle = tokio::spawn(async move {
        let server = IpcServer::new(kernel, addr, IpcConfig::default());
        let _ = server.serve().await;
    });

    // Give the server a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    (addr, handle)
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

    let payload = rmp_serde::to_vec_named(&request).unwrap();
    write_frame(stream, MSG_REQUEST, &payload).await.unwrap();

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
    assert_eq!(resp_body.get("pid").unwrap().as_str().unwrap(), "test-proc-1");
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
    assert_eq!(resp_body.get("pid").unwrap().as_str().unwrap(), "shared-test");
}

// =============================================================================
// Engine service tests
// =============================================================================

/// Helper: Create an envelope via engine.CreateEnvelope and return the serialized envelope.
async fn create_test_envelope(stream: &mut TcpStream) -> serde_json::Value {
    let body = serde_json::json!({
        "raw_input": "test input",
        "request_id": "req-e1",
        "user_id": "user-e1",
        "session_id": "sess-e1",
        "stage_order": ["classify", "respond"],
    });
    let (msg_type, response) = round_trip(stream, "engine", "CreateEnvelope", body).await;
    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    response.get("body").unwrap().clone()
}

#[tokio::test]
async fn test_update_envelope() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Create envelope
    let envelope = create_test_envelope(&mut stream).await;
    let env_id = envelope.get("identity").unwrap()
        .get("envelope_id").unwrap().as_str().unwrap().to_string();

    // Update it (change raw_input)
    let mut updated = envelope.clone();
    updated.as_object_mut().unwrap()
        .insert("raw_input".to_string(), serde_json::json!("updated input"));

    let (msg_type, response) = round_trip(
        &mut stream,
        "engine",
        "UpdateEnvelope",
        serde_json::json!({ "envelope": updated }),
    ).await;

    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    let body = response.get("body").unwrap();
    assert_eq!(body.get("raw_input").unwrap().as_str().unwrap(), "updated input");
    assert_eq!(
        body.get("identity").unwrap().get("envelope_id").unwrap().as_str().unwrap(),
        env_id,
    );
}

#[tokio::test]
async fn test_update_envelope_not_found() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Create an envelope, tweak its ID to a non-existent one, try to update
    let envelope = create_test_envelope(&mut stream).await;
    let mut fake = envelope.clone();
    fake.as_object_mut().unwrap()
        .get_mut("identity").unwrap()
        .as_object_mut().unwrap()
        .insert("envelope_id".to_string(), serde_json::json!("env_does_not_exist"));

    let (msg_type, response) = round_trip(
        &mut stream,
        "engine",
        "UpdateEnvelope",
        serde_json::json!({ "envelope": fake }),
    ).await;

    assert_eq!(msg_type, MSG_ERROR);
    let error = response.get("error").unwrap();
    assert_eq!(error.get("code").unwrap().as_str().unwrap(), "NOT_FOUND");
}

#[tokio::test]
async fn test_execute_pipeline() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Create an envelope first
    let envelope = create_test_envelope(&mut stream).await;
    let envelope_str = serde_json::to_string(&envelope).unwrap();

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
    let pipeline_config_str = serde_json::to_string(&pipeline_config).unwrap();

    let (msg_type, response) = round_trip(
        &mut stream,
        "engine",
        "ExecutePipeline",
        serde_json::json!({
            "envelope": envelope_str,
            "pipeline_config": pipeline_config_str,
        }),
    ).await;

    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    let body = response.get("body").unwrap();
    // First instruction should be RUN_AGENT for the entry stage
    assert_eq!(body.get("kind").unwrap().as_str().unwrap(), "RUN_AGENT");
    assert_eq!(body.get("agent_name").unwrap().as_str().unwrap(), "classifier");
}

#[tokio::test]
async fn test_clone_envelope() {
    let (addr, _handle) = start_test_server().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();

    let envelope = create_test_envelope(&mut stream).await;
    let original_id = envelope.get("identity").unwrap()
        .get("envelope_id").unwrap().as_str().unwrap().to_string();

    let (msg_type, response) = round_trip(
        &mut stream,
        "engine",
        "CloneEnvelope",
        serde_json::json!({ "envelope": envelope }),
    ).await;

    assert_eq!(msg_type, MSG_RESPONSE);
    assert_eq!(response.get("ok").unwrap().as_bool().unwrap(), true);
    let body = response.get("body").unwrap();
    let cloned_id = body.get("identity").unwrap()
        .get("envelope_id").unwrap().as_str().unwrap();
    // Clone gets a new ID
    assert_ne!(cloned_id, original_id);
    // Content preserved
    assert_eq!(body.get("raw_input").unwrap().as_str().unwrap(), "test input");
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
    ).await;

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
    ).await;

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
    ).await;

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
    write_frame(&mut sub_stream, MSG_REQUEST, &payload).await.unwrap();

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
        ).await;
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
        ).await.expect("Timed out waiting for stream chunk").unwrap();

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
        assert_eq!(body.get("event_type").unwrap().as_str().unwrap(), "test.stream");
        assert_eq!(body.get("source").unwrap().as_str().unwrap(), "stream-test");
    }

    // Drop the subscriber — server should send MSG_STREAM_END when it detects disconnect,
    // but since we're closing from the client side, we just verify the chunks arrived.
    drop(sub_stream);
}
