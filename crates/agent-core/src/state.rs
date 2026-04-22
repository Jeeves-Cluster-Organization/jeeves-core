use crate::llm::ChatMessage;
use crate::tools::DynTool;

#[derive(Debug, Default, Clone, Copy)]
pub struct Budget {
    pub max_iterations: Option<u32>,
    pub max_llm_calls: Option<u32>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Counters {
    pub iterations: u32,
    pub llm_calls: u32,
    pub tool_calls: u32,
}

#[derive(Debug)]
pub struct AgentState {
    pub system_prompt: Option<String>,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<DynTool>,
    pub session_id: Option<String>,
    pub budget: Budget,
    pub counters: Counters,
}

impl AgentState {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            system_prompt: None,
            model: model.into(),
            messages: Vec::new(),
            tools: Vec::new(),
            session_id: None,
            budget: Budget::default(),
            counters: Counters::default(),
        }
    }

    /// Build the messages slice that will be sent to the LLM,
    /// prepending the system prompt (if any).
    pub fn outgoing(&self) -> Vec<ChatMessage> {
        let mut out = Vec::with_capacity(self.messages.len() + 1);
        if let Some(sys) = &self.system_prompt {
            if !sys.is_empty() {
                out.push(ChatMessage::system(sys.clone()));
            }
        }
        out.extend(self.messages.iter().cloned());
        out
    }

    pub fn push(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
    }
}
