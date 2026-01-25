# mission_system.events - Event Integration Layer

**Package:** `mission_system.events`  
**Purpose:** Event integration between Control Tower and Mission System  
**Updated:** 2026-01-23

---

## Overview

The `events` package provides the bridge between Control Tower's EventAggregator and Mission System's event streaming infrastructure. It translates kernel events to frontend-friendly formats and forwards them to WebSocket clients.

---

## Module Index

| Module | Description |
|--------|-------------|
| `__init__.py` | Package exports (`EventBridge`) |
| `bridge.py` | `EventBridge` - Control Tower to WebSocket bridge |

---

## EventBridge (`bridge.py`)

### Architecture

```
ControlTower.EventAggregator (kernel events)
       ↓ (subscribe)
EventBridge (translation layer)
       ↓ (forward)
WebSocketEventManager (frontend streaming)
```

### Class Definition

```python
class EventBridge:
    """
    Bridges Control Tower kernel events to Mission System event streams.
    
    This is the integration layer between:
    - Control Tower's EventAggregator (kernel-level events)
    - Mission System's WebSocketEventManager (frontend streaming)
    """
    
    def __init__(
        self,
        event_aggregator: Any,  # EventAggregatorProtocol
        websocket_manager: Any,  # WebSocketEventManager
        logger: LoggerProtocol,
    ) -> None
    
    def start(self) -> None:
        """Start bridging events (subscribes to all kernel events)."""
    
    def stop(self) -> None:
        """Stop bridging events."""
```

### Event Translation

The EventBridge translates kernel events to frontend event formats:

| Kernel Event | Frontend Event | Description |
|--------------|----------------|-------------|
| `process.created` | `orchestrator.started` | Request processing started |
| `process.state_changed` (terminated) | `orchestrator.completed` | Request completed |
| `interrupt.raised` (CLARIFICATION) | `orchestrator.clarification` | Clarification needed |
| `interrupt.raised` (CONFIRMATION) | `orchestrator.confirmation` | Confirmation needed |
| `interrupt.raised` (RESOURCE_EXHAUSTED) | `orchestrator.resource_exhausted` | Quota exceeded |
| `resource.exhausted` | `orchestrator.resource_exhausted` | Resource limit hit |
| `process.cancelled` | `orchestrator.cancelled` | Request cancelled |

### Usage

```python
from mission_system.events import EventBridge

# Create bridge during server startup
event_bridge = EventBridge(
    event_aggregator=control_tower.events,
    websocket_manager=event_manager,
    logger=logger,
)

# Start bridging
event_bridge.start()

# On shutdown
event_bridge.stop()
```

---

## Event Translation Details

### `process.created`

**Input:**
```python
{
    "event_type": "process.created",
    "pid": "env_123",
    "data": {"request_id": "req_456"}
}
```

**Output:**
```python
{
    "type": "orchestrator.started",
    "data": {
        "request_id": "req_456",
        "pid": "env_123"
    }
}
```

### `interrupt.raised` (Clarification)

**Input:**
```python
{
    "event_type": "interrupt.raised",
    "pid": "env_123",
    "data": {
        "interrupt_type": "clarification",
        "question": "Which file are you referring to?"
    }
}
```

**Output:**
```python
{
    "type": "orchestrator.clarification",
    "data": {
        "request_id": "env_123",
        "clarification_question": "Which file are you referring to?",
        "thread_id": "env_123"
    }
}
```

### `interrupt.raised` (Confirmation)

**Input:**
```python
{
    "event_type": "interrupt.raised",
    "pid": "env_123",
    "data": {
        "interrupt_type": "confirmation",
        "message": "This will modify 5 files. Proceed?",
        "confirmation_id": "conf_789"
    }
}
```

**Output:**
```python
{
    "type": "orchestrator.confirmation",
    "data": {
        "request_id": "env_123",
        "confirmation_message": "This will modify 5 files. Proceed?",
        "confirmation_id": "conf_789"
    }
}
```

### `resource.exhausted`

**Input:**
```python
{
    "event_type": "resource.exhausted",
    "pid": "env_123",
    "data": {
        "resource": "llm_calls",
        "usage": 10,
        "quota": 10
    }
}
```

**Output:**
```python
{
    "type": "orchestrator.resource_exhausted",
    "data": {
        "request_id": "env_123",
        "resource": "llm_calls",
        "usage": 10,
        "quota": 10
    }
}
```

---

## Interrupt Kinds

From `protocols`:

| Kind | Value | Description |
|------|-------|-------------|
| `CLARIFICATION` | `"clarification"` | User clarification needed |
| `CONFIRMATION` | `"confirmation"` | User approval required |
| `AGENT_REVIEW` | `"agent_review"` | Agent review decision point |
| `CHECKPOINT` | `"checkpoint"` | Checkpoint save point |
| `RESOURCE_EXHAUSTED` | `"resource_exhausted"` | Resource quota exceeded |

---

## WebSocket Integration

Events are broadcast to connected WebSocket clients via the `WebSocketEventManager`:

```python
await websocket_manager.broadcast(
    event_type="orchestrator.clarification",
    data={
        "request_id": "env_123",
        "clarification_question": "Which file?",
    }
)
```

### Frontend Consumption

```javascript
// WebSocket client
ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    switch (data.type) {
        case 'orchestrator.started':
            showLoadingIndicator(data.data.request_id);
            break;
        case 'orchestrator.clarification':
            showClarificationDialog(data.data.clarification_question);
            break;
        case 'orchestrator.completed':
            hideLoadingIndicator();
            break;
    }
};
```

---

## All Exports

```python
from mission_system.events import (
    EventBridge,
)
```

---

## Navigation

- [← Back to README](README.md)
- [← Contracts Documentation](contracts.md)
- [← Orchestrator Documentation](orchestrator.md)
- [→ Services Documentation](services.md)
