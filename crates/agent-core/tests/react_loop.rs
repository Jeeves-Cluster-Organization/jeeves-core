//! End-to-end smoke test for the Agent ReAct loop with a mock provider.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use agent_core::{
    llm::{ChatRequest, ChatResponse, LlmProvider, StreamChunk, TokenUsage},
    Agent, Error, Result, Tool, ToolCall, ToolOutput,
};
use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

/// Mock LLM that replays a fixed script of responses.
#[derive(Debug)]
struct ScriptedProvider {
    script: Mutex<std::collections::VecDeque<ChatResponse>>,
    seen_requests: AtomicUsize,
}

impl ScriptedProvider {
    fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            script: Mutex::new(responses.into_iter().collect()),
            seen_requests: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl LlmProvider for ScriptedProvider {
    async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse> {
        self.seen_requests.fetch_add(1, Ordering::SeqCst);
        #[allow(clippy::unwrap_used)]
        let resp = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| Error::Llm("script exhausted".into()))?;
        Ok(resp)
    }

    async fn chat_stream(
        &self,
        _req: &ChatRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        Err(Error::Llm(
            "streaming not supported in scripted provider".into(),
        ))
    }
}

#[derive(Debug)]
struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "returns its `text` arg"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"],
        })
    }

    async fn call(&self, args: serde_json::Value) -> Result<ToolOutput> {
        #[derive(serde::Deserialize)]
        struct A {
            text: String,
        }
        let a: A = serde_json::from_value(args).map_err(|e| Error::Tool(e.to_string()))?;
        Ok(ToolOutput::text(a.text))
    }
}

#[tokio::test]
async fn completes_with_no_tool_calls() {
    let provider = Arc::new(ScriptedProvider::new(vec![ChatResponse {
        content: Some("hello there".into()),
        tool_calls: vec![],
        usage: TokenUsage::default(),
        model: "mock".into(),
    }]));

    let mut agent = Agent::builder()
        .provider(provider.clone())
        .model("mock")
        .build()
        .unwrap();

    let text = agent.run("hi").await.unwrap();
    assert_eq!(text.as_deref(), Some("hello there"));
    assert_eq!(provider.seen_requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn executes_tool_then_finalizes() {
    let provider = Arc::new(ScriptedProvider::new(vec![
        ChatResponse {
            content: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "echo".into(),
                arguments: serde_json::json!({"text": "pong"}),
            }],
            usage: TokenUsage::default(),
            model: "mock".into(),
        },
        ChatResponse {
            content: Some("final".into()),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            model: "mock".into(),
        },
    ]));

    let mut agent = Agent::builder()
        .provider(provider.clone())
        .model("mock")
        .tool(Arc::new(EchoTool))
        .build()
        .unwrap();

    let text = agent.run("please call echo").await.unwrap();
    assert_eq!(text.as_deref(), Some("final"));
    assert_eq!(agent.state().counters.tool_calls, 1);
    assert_eq!(agent.state().counters.llm_calls, 2);
}

#[tokio::test]
async fn budget_exhaustion_is_reported() {
    let provider = Arc::new(ScriptedProvider::new(vec![
        // One tool call → forces a second LLM call → trips budget of 1.
        ChatResponse {
            content: None,
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "echo".into(),
                arguments: serde_json::json!({"text": "x"}),
            }],
            usage: TokenUsage::default(),
            model: "mock".into(),
        },
    ]));

    let mut agent = Agent::builder()
        .provider(provider)
        .model("mock")
        .tool(Arc::new(EchoTool))
        .budget(agent_core::Budget {
            max_iterations: None,
            max_llm_calls: Some(1),
        })
        .build()
        .unwrap();

    let err = agent.run("go").await.unwrap_err();
    assert!(matches!(err, Error::BudgetExceeded(_)), "got {err:?}");
}
