//! Minimal ReAct agent loop.
//!
//! The `Agent` is a policy-free runtime. A single `run(input)` call:
//!   1. Appends the user message to state.
//!   2. Loops: call LLM → (optional) transform_context → execute tool calls → feed results back.
//!   3. Terminates when the LLM replies with no tool calls, or when `abort()` is called,
//!      or when a budget is exceeded.
//!
//! Hooks control policy:
//!   - `BeforeToolCall` → confirmation gates, ACL, sandboxing
//!   - `AfterToolCall`  → redaction, logging
//!   - `TransformContext` → compaction, pruning

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::events::{channel, Event, EventSender};
use crate::hooks::{DynAfter, DynBefore, DynTransform, HookDecision};
use crate::llm::{ChatMessage, ChatRequest, LlmProvider, ToolCallRef};
use crate::state::{AgentState, Budget};
use crate::tools::{DynTool, ToolCall, ToolOutput};

#[derive(Debug, Clone)]
pub struct AgentBuilder {
    model: String,
    system_prompt: Option<String>,
    provider: Option<Arc<dyn LlmProvider>>,
    tools: Vec<DynTool>,
    before: Vec<DynBefore>,
    after: Vec<DynAfter>,
    transform: Option<DynTransform>,
    budget: Budget,
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    session_id: Option<String>,
    event_capacity: usize,
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self {
            model: String::new(),
            system_prompt: None,
            provider: None,
            tools: Vec::new(),
            before: Vec::new(),
            after: Vec::new(),
            transform: None,
            budget: Budget::default(),
            temperature: None,
            max_tokens: None,
            session_id: None,
            event_capacity: 128,
        }
    }
}

impl AgentBuilder {
    pub fn model(mut self, m: impl Into<String>) -> Self {
        self.model = m.into();
        self
    }
    pub fn system_prompt(mut self, s: impl Into<String>) -> Self {
        self.system_prompt = Some(s.into());
        self
    }
    pub fn provider(mut self, p: Arc<dyn LlmProvider>) -> Self {
        self.provider = Some(p);
        self
    }
    pub fn tool(mut self, t: DynTool) -> Self {
        self.tools.push(t);
        self
    }
    pub fn tools(mut self, ts: impl IntoIterator<Item = DynTool>) -> Self {
        self.tools.extend(ts);
        self
    }
    pub fn before_tool_call(mut self, h: DynBefore) -> Self {
        self.before.push(h);
        self
    }
    pub fn after_tool_call(mut self, h: DynAfter) -> Self {
        self.after.push(h);
        self
    }
    pub fn transform_context(mut self, h: DynTransform) -> Self {
        self.transform = Some(h);
        self
    }
    pub fn budget(mut self, b: Budget) -> Self {
        self.budget = b;
        self
    }
    pub fn temperature(mut self, t: f64) -> Self {
        self.temperature = Some(t);
        self
    }
    pub fn max_tokens(mut self, m: i32) -> Self {
        self.max_tokens = Some(m);
        self
    }
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }
    pub fn event_capacity(mut self, c: usize) -> Self {
        self.event_capacity = c;
        self
    }

    pub fn build(self) -> Result<Agent> {
        if self.model.is_empty() {
            return Err(Error::Invalid("model is required".into()));
        }
        let provider = self
            .provider
            .ok_or_else(|| Error::Invalid("provider is required".into()))?;
        let (tx, _) = channel(self.event_capacity);
        let mut state = AgentState::new(self.model);
        state.system_prompt = self.system_prompt;
        state.tools = self.tools;
        state.budget = self.budget;
        state.session_id = self.session_id;
        Ok(Agent {
            state,
            provider,
            before: self.before,
            after: self.after,
            transform: self.transform,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            events: tx,
            aborted: Arc::new(AtomicBool::new(false)),
        })
    }
}

#[derive(Debug)]
pub struct Agent {
    state: AgentState,
    provider: Arc<dyn LlmProvider>,
    before: Vec<DynBefore>,
    after: Vec<DynAfter>,
    transform: Option<DynTransform>,
    temperature: Option<f64>,
    max_tokens: Option<i32>,
    events: EventSender,
    aborted: Arc<AtomicBool>,
}

impl Agent {
    pub fn builder() -> AgentBuilder {
        AgentBuilder::default()
    }

    pub fn events(&self) -> &EventSender {
        &self.events
    }
    pub fn state(&self) -> &AgentState {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut AgentState {
        &mut self.state
    }

    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
    }

    pub fn reset_abort(&self) {
        self.aborted.store(false, Ordering::SeqCst);
    }

    /// Run a single conversational turn to completion (drives the ReAct loop until
    /// the LLM stops calling tools). Returns the final assistant text, if any.
    #[tracing::instrument(skip(self, input), fields(model = %self.state.model))]
    pub async fn run(&mut self, input: &str) -> Result<Option<String>> {
        self.reset_abort();
        self.state.push(ChatMessage::user(input));

        let tool_schemas = build_tool_schemas(&self.state.tools);

        loop {
            if self.aborted.load(Ordering::SeqCst) {
                return Err(Error::Aborted);
            }
            self.state.counters.iterations += 1;
            let _ = self.events.send(Event::TurnStart {
                turn: self.state.counters.iterations,
            });

            if let Some(max) = self.state.budget.max_iterations {
                if self.state.counters.iterations > max {
                    return Err(Error::BudgetExceeded(format!("max_iterations={max}")));
                }
            }
            if let Some(max) = self.state.budget.max_llm_calls {
                if self.state.counters.llm_calls >= max {
                    return Err(Error::BudgetExceeded(format!("max_llm_calls={max}")));
                }
            }

            let mut outgoing = self.state.outgoing();
            if let Some(tx) = &self.transform {
                tx.transform(&mut outgoing);
            }

            let req = ChatRequest {
                messages: outgoing,
                temperature: self.temperature,
                max_tokens: self.max_tokens,
                model: Some(self.state.model.clone()),
                tools: if tool_schemas.is_empty() {
                    None
                } else {
                    Some(tool_schemas.clone())
                },
            };

            self.state.counters.llm_calls += 1;
            let resp = self.provider.chat(&req).await?;

            if !resp.tool_calls.is_empty() {
                // Record the assistant tool-call request for the next round.
                let tcs: Vec<ToolCallRef> = resp.tool_calls.iter().map(ToolCallRef::from).collect();
                self.state.push(ChatMessage::assistant(
                    resp.content.clone().unwrap_or_default(),
                    Some(tcs),
                ));

                for call in &resp.tool_calls {
                    let _ = self.events.send(Event::ToolCallStart {
                        call_id: call.id.clone(),
                        name: call.name.clone(),
                    });

                    let output = self.dispatch_tool(call).await;
                    let ok = output.is_ok();
                    let _ = self.events.send(Event::ToolCallEnd {
                        call_id: call.id.clone(),
                        ok,
                    });

                    self.state.counters.tool_calls += 1;
                    let out_msg = match output {
                        Ok(out) => ChatMessage::tool_result(call.id.clone(), out.as_llm_text()),
                        Err(e) => ChatMessage::tool_result(call.id.clone(), format!("error: {e}")),
                    };
                    self.state.push(out_msg);
                }
                // Continue the loop so the model sees the tool results.
                continue;
            }

            let text = resp.content.clone();
            self.state.push(ChatMessage::assistant(
                text.clone().unwrap_or_default(),
                None,
            ));
            if let Some(t) = &text {
                let _ = self.events.send(Event::MessageEnd {
                    text: Some(t.clone()),
                });
            }
            let _ = self.events.send(Event::TurnEnd {
                turn: self.state.counters.iterations,
                final_text: text.clone(),
            });
            return Ok(text);
        }
    }

    async fn dispatch_tool(&self, call: &ToolCall) -> Result<ToolOutput> {
        // Run all BeforeToolCall hooks in order.
        for h in &self.before {
            match h.call(call).await {
                HookDecision::Continue => {}
                HookDecision::Replace(out) => return Ok(out),
                HookDecision::Block { reason } => {
                    return Ok(ToolOutput::text(format!("blocked: {reason}")));
                }
            }
        }

        let tool = self
            .state
            .tools
            .iter()
            .find(|t| t.name() == call.name)
            .ok_or_else(|| Error::Tool(format!("unknown tool: {}", call.name)))?;

        let mut out = tool.call(call.arguments.clone()).await?;

        for h in &self.after {
            h.call(call, &mut out).await;
        }
        Ok(out)
    }
}

fn build_tool_schemas(tools: &[DynTool]) -> Arc<Vec<serde_json::Value>> {
    let v: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name(),
                "description": t.description(),
                "parameters": t.schema(),
            })
        })
        .collect();
    Arc::new(v)
}
