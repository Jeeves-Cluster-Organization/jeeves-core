# IPC Protocol Specification

Jeeves Core communicates via a custom binary IPC protocol over TCP using MessagePack-encoded payloads.

Default listen address: `127.0.0.1:50051`

## Wire Format

```
┌──────────┬──────────┬────────────────────────┐
│ len (4B) │ type(1B) │   msgpack payload      │
│ u32 BE   │ u8       │                        │
└──────────┴──────────┴────────────────────────┘
```

- **len**: Big-endian `u32`. Equals `sizeof(type) + sizeof(payload)` (excludes the 4-byte prefix itself).
- **type**: Single byte identifying the message kind.
- **payload**: MessagePack-encoded JSON object.

Maximum frame size: **5 MB** (configurable via `IpcConfig.max_frame_bytes`).

## Message Types

| Type | Hex | Direction | Purpose |
|------|-----|-----------|---------|
| `MSG_REQUEST` | `0x01` | Client → Server | RPC request |
| `MSG_RESPONSE` | `0x02` | Server → Client | Single response |
| `MSG_STREAM_CHUNK` | `0x03` | Server → Client | Streaming response chunk |
| `MSG_STREAM_END` | `0x04` | Server → Client | End of streaming response |
| `MSG_ERROR` | `0xFF` | Server → Client | Error response |

## Request Format

Every request is a MessagePack object with:

```json
{
  "id": "unique-request-id",
  "service": "kernel",
  "method": "CreateProcess",
  "body": { ... }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | Yes | Correlation ID echoed in responses |
| `service` | string | Yes | Target service (`kernel`, `engine`, `orchestration`, `commbus`) |
| `method` | string | Yes | Method name within the service |
| `body` | object | Yes | Method-specific parameters |
- Missing required request fields (`id`, `service`, `method`, `body`) are rejected with `INVALID_ARGUMENT` and do not close the TCP connection.

## Response Format

**Success (MSG_RESPONSE):**
```json
{
  "id": "echoed-request-id",
  "ok": true,
  "body": { ... }
}
```

**Error (MSG_ERROR):**
```json
{
  "id": "echoed-request-id",
  "ok": false,
  "error": {
    "code": "NOT_FOUND",
    "message": "Process proc-123 not found"
  }
}
```

**Streaming (MSG_STREAM_CHUNK + MSG_STREAM_END):**
```json
// Each chunk:
{ "id": "echoed-request-id", "body": { ... } }

// Final sentinel:
{ "id": "echoed-request-id" }
```

## Error Codes

| Code | Meaning |
|------|---------|
| `NOT_FOUND` | Resource does not exist |
| `INVALID_ARGUMENT` | Validation failure or malformed input |
| `FAILED_PRECONDITION` | Invalid process state transition |
| `RESOURCE_EXHAUSTED` | Quota exceeded or kernel request queue saturated |
| `UNAVAILABLE` | Kernel actor unavailable or terminated mid-request |
| `CANCELLED` | Operation cancelled |
| `TIMEOUT` | Operation timed out |
| `INTERNAL` | Unexpected server error |

## Connection Limits

| Parameter | Default | Description |
|-----------|---------|-------------|
| `max_connections` | 1000 | Concurrent TCP connections (semaphore backpressure) |
| `kernel_queue_capacity` | 2048 | Max in-flight requests queued for the kernel actor |
| `read_timeout_secs` | 30 | Idle read timeout per frame |
| `write_timeout_secs` | 10 | Write timeout per frame (slow consumer protection) |
| `max_frame_bytes` | 5 MB | Maximum frame payload size |

When `kernel_queue_capacity` is full, server responds with `MSG_ERROR`:
- `code`: `RESOURCE_EXHAUSTED`
- `retryable`: `true`
- `kernel_queue_capacity`: configured capacity

---

## Services

### kernel (14 methods)

Process lifecycle, quota enforcement, rate limiting, and system status.

| Method | Required Fields | Description |
|--------|----------------|-------------|
| `CreateProcess` | `pid` | Create a new process. Optional: `request_id`, `user_id`, `session_id`, `priority`, `quota` |
| `GetProcess` | `pid` | Retrieve process by PID |
| `ScheduleProcess` | `pid` | Transition process to Ready state |
| `GetNextRunnable` | — | Dequeue highest-priority Ready process |
| `TransitionState` | `pid`, `new_state` | Transition process state. Optional: `reason` |
| `TerminateProcess` | `pid` | Terminate a process |
| `CheckQuota` | `pid` | Check if process is within resource bounds |
| `RecordUsage` | `pid` | Record resource consumption. Optional: `llm_calls`, `tool_calls`, `tokens_in`, `tokens_out` |
| `CheckRateLimit` | `user_id` | Check per-user rate limit. Optional: `record` (default true) |
| `ListProcesses` | — | List processes. Optional filters: `state`, `user_id` |
| `GetProcessCounts` | — | Count processes grouped by state |
| `SetQuotaDefaults` | — | Update default quota configuration |
| `GetQuotaDefaults` | — | Get current default quota configuration |
| `GetSystemStatus` | — | Full system status (processes, services, orchestration, commbus) |

**Process states:** `NEW`, `READY`, `RUNNING`, `WAITING`, `BLOCKED`, `TERMINATED`, `ZOMBIE`

**Priority levels:** `REALTIME`, `HIGH`, `NORMAL` (default), `LOW`, `IDLE`

**Lifecycle events emitted to CommBus:** `process.created`, `process.state_changed`, `process.terminated`, `resource.exhausted`

### engine (5 methods)

Envelope (execution state container) management and pipeline execution.

| Method | Required Fields | Description |
|--------|----------------|-------------|
| `CreateEnvelope` | — | Create new envelope. Optional: `raw_input`, `request_id`, `user_id`, `session_id`, `stage_order` |
| `CheckBounds` | (full envelope in body) | Check if envelope is within execution limits |
| `UpdateEnvelope` | `envelope` | Replace stored envelope by ID |
| `ExecutePipeline` | `envelope`, `pipeline_config` | Initialize orchestration and get first instruction |
| `CloneEnvelope` | `envelope` | Deep-clone envelope with new ID |

### orchestration (4 methods)

Multi-agent pipeline session management.

| Method | Required Fields | Description |
|--------|----------------|-------------|
| `InitializeSession` | `process_id`, `pipeline_config`, `envelope` | Start orchestration session. Optional: `force` |
| `GetNextInstruction` | `process_id` | Get next agent instruction for process |
| `ReportAgentResult` | `process_id`, `agent_name` | Report agent execution result. Optional: `success`, `error`, `output`, `metrics` |
| `GetSessionState` | `process_id` | Get current session state |

`ReportAgentResult` semantics:
- `success` defaults to `true`.
- If `success=false`, kernel records `{ "success": false, "error": ... }` in that agent's output.
- `metrics.tokens_in` and `metrics.tokens_out` are optional.
- Omitted token metrics are treated as unknown and do not increment token counters.

### commbus (4 methods)

Internal message bus for events, commands, and queries.

| Method | Required Fields | Description |
|--------|----------------|-------------|
| `Publish` | `event_type` | Publish event to subscribers. Optional: `payload` (JSON string), `source` |
| `Send` | `command_type` | Send point-to-point command. Optional: `payload`, `source` |
| `Query` | `query_type` | Request/response query. Optional: `payload`, `source`, `timeout_ms` |
| `Subscribe` | `event_types` | Subscribe to events (streaming response). Optional: `subscriber_id` |

**Query timeouts:** Default 5s, max 30s (configurable via `IpcConfig`).

**Subscribe** is the only streaming method — returns `MSG_STREAM_CHUNK` frames until the subscription is cancelled.
