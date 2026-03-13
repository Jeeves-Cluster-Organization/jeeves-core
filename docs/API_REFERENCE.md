# HTTP API Reference

Base URL: `http://{JEEVES_HTTP_ADDR}` (default `http://0.0.0.0:8080`)

## Diagnostics

### GET `/health`
Liveness probe. Always returns 200.

```json
{ "status": "healthy" }
```

### GET `/ready`
Readiness probe. Returns 200 when kernel is operational, 503 otherwise.

```json
{ "status": "ready" }
```

### GET `/api/v1/status`
System status with process counts and health metrics.

```json
{
  "processes_total": 0,
  "processes_by_state": {},
  "services_healthy": 0,
  "services_degraded": 0,
  "services_unhealthy": 0,
  "active_orchestration_sessions": 0
}
```

---

## Pipeline Execution

### POST `/api/v1/chat/messages`
Run a pipeline with input and return the response.

**Body:**
```json
{
  "message": "string (required)",
  "user_id": "string (optional, auto-generated if omitted)",
  "session_id": "string (optional, auto-generated if omitted)",
  "pipeline_config": { },
  "metadata": { }
}
```

**Response (200):**
```json
{
  "terminated": true,
  "terminal_reason": "Completed",
  "outputs": {
    "stage_name": { "key": "value" }
  }
}
```

**Errors:** 400 (missing pipeline_config), 500

### POST `/api/v1/pipelines/run`
Alias for `/api/v1/chat/messages`. Same request/response format.

---

## Session State

### GET `/api/v1/sessions/{id}`
Get orchestration session state for a process.

**Response (200):** `SessionState` — process_id, stage_order, current_stage, terminated status, outputs.

**Errors:** 404 (session not found), 500

---

## Global Behavior

- **CORS:** All origins allowed, all methods/headers, credentials enabled.
- **Error responses:** Internal errors return JSON `{ "error": "message" }`.
