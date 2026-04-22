use agent_core::{Error, Result, Tool, ToolOutput};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug)]
pub struct ReadTool;

#[derive(Debug, Deserialize)]
struct Args {
    path: String,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &str {
        "read"
    }
    fn description(&self) -> &str {
        "Read a file's contents. Args: {path, offset?, limit?} — offset/limit are line numbers."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path":   { "type": "string" },
                "offset": { "type": "integer", "minimum": 0 },
                "limit":  { "type": "integer", "minimum": 1 },
            },
            "required": ["path"],
        })
    }

    async fn call(&self, args: serde_json::Value) -> Result<ToolOutput> {
        let args: Args = serde_json::from_value(args).map_err(|e| Error::Tool(e.to_string()))?;
        let body = tokio::fs::read_to_string(&args.path)
            .await
            .map_err(|e| Error::Tool(format!("{}: {e}", args.path)))?;

        let text = match (args.offset, args.limit) {
            (None, None) => body,
            (o, l) => {
                let start = o.unwrap_or(0);
                let lines: Vec<&str> = body.lines().collect();
                let end = l
                    .map(|l| (start + l).min(lines.len()))
                    .unwrap_or(lines.len());
                if start >= lines.len() {
                    String::new()
                } else {
                    lines[start..end].join("\n")
                }
            }
        };
        Ok(ToolOutput::text(text))
    }
}
