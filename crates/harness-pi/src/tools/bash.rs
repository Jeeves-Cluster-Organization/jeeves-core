use agent_core::{Error, Result, Tool, ToolOutput};
use async_trait::async_trait;
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::time::{timeout, Duration};

#[derive(Debug)]
pub struct BashTool;

#[derive(Debug, Deserialize)]
struct Args {
    command: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }
    fn description(&self) -> &str {
        "Run a shell command via `/bin/sh -c`. Args: {command, timeout_ms?}. Combines stdout+stderr."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command":    { "type": "string" },
                "timeout_ms": { "type": "integer", "minimum": 0 },
            },
            "required": ["command"],
        })
    }

    async fn call(&self, args: serde_json::Value) -> Result<ToolOutput> {
        let args: Args = serde_json::from_value(args).map_err(|e| Error::Tool(e.to_string()))?;
        let mut child = tokio::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&args.command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Tool(e.to_string()))?;

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();

        let dur = Duration::from_millis(args.timeout_ms.unwrap_or(120_000));
        let run = async {
            let mut out = String::new();
            let mut err = String::new();
            if let Some(mut s) = stdout.take() {
                let _ = s.read_to_string(&mut out).await;
            }
            if let Some(mut s) = stderr.take() {
                let _ = s.read_to_string(&mut err).await;
            }
            let status = child.wait().await.map_err(|e| Error::Tool(e.to_string()))?;
            Ok::<_, Error>((status, out, err))
        };

        match timeout(dur, run).await {
            Ok(res) => {
                let (status, out, err) = res?;
                let combined = if err.is_empty() {
                    out
                } else {
                    format!("{out}\n--- stderr ---\n{err}")
                };
                let body = if status.success() {
                    combined
                } else {
                    format!("(exit {}) {combined}", status.code().unwrap_or(-1))
                };
                Ok(ToolOutput::text(body))
            }
            Err(_) => Err(Error::Tool(format!("timeout after {}ms", dur.as_millis()))),
        }
    }
}
