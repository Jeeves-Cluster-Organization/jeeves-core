use agent_core::{Error, Result, Tool, ToolOutput};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug)]
pub struct WriteTool;

#[derive(Debug, Deserialize)]
struct Args {
    path: String,
    content: String,
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }
    fn description(&self) -> &str {
        "Write (or overwrite) a file. Args: {path, content}."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path":    { "type": "string" },
                "content": { "type": "string" },
            },
            "required": ["path", "content"],
        })
    }

    async fn call(&self, args: serde_json::Value) -> Result<ToolOutput> {
        let args: Args = serde_json::from_value(args).map_err(|e| Error::Tool(e.to_string()))?;
        if let Some(parent) = std::path::Path::new(&args.path).parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        tokio::fs::write(&args.path, args.content.as_bytes())
            .await
            .map_err(|e| Error::Tool(format!("{}: {e}", args.path)))?;
        Ok(ToolOutput::text(format!(
            "wrote {} ({} bytes)",
            args.path,
            args.content.len()
        )))
    }
}
