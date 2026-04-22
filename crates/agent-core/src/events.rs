use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::tools::ToolCall;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    TurnStart {
        turn: u32,
    },
    MessageDelta {
        text: String,
    },
    MessageEnd {
        text: Option<String>,
    },
    ToolCallStart {
        call_id: String,
        name: String,
    },
    ToolCallEnd {
        call_id: String,
        ok: bool,
    },
    TurnEnd {
        turn: u32,
        final_text: Option<String>,
    },
    Error {
        message: String,
    },
}

pub type EventSender = broadcast::Sender<Event>;
pub type EventReceiver = broadcast::Receiver<Event>;

pub fn channel(capacity: usize) -> (EventSender, EventReceiver) {
    broadcast::channel(capacity)
}

#[allow(dead_code)]
pub(crate) fn tool_start(id: &str, call: &ToolCall) -> Event {
    Event::ToolCallStart {
        call_id: id.to_string(),
        name: call.name.clone(),
    }
}
