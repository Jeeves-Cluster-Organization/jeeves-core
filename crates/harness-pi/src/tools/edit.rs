use agent_core::{Error, Result, Tool, ToolOutput};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Debug)]
pub struct EditTool;

#[derive(Debug, Deserialize)]
struct Args {
    path: String,
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }
    fn description(&self) -> &str {
        "Replace `old_string` with `new_string` in a file. Args: {path, old_string, new_string, replace_all?}. \
         Non-replace_all mode requires `old_string` to be unique in the file."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path":        { "type": "string" },
                "old_string":  { "type": "string" },
                "new_string":  { "type": "string" },
                "replace_all": { "type": "boolean" },
            },
            "required": ["path", "old_string", "new_string"],
        })
    }

    async fn call(&self, args: serde_json::Value) -> Result<ToolOutput> {
        let args: Args = serde_json::from_value(args).map_err(|e| Error::Tool(e.to_string()))?;
        let body = tokio::fs::read_to_string(&args.path)
            .await
            .map_err(|e| Error::Tool(format!("{}: {e}", args.path)))?;

        let new_body = if args.replace_all {
            body.replace(&args.old_string, &args.new_string)
        } else {
            let count = body.matches(&args.old_string).count();
            if count == 0 {
                return Err(Error::Tool("old_string not found".into()));
            }
            if count > 1 {
                return Err(Error::Tool(format!(
                    "old_string matches {count} times — pass replace_all=true or disambiguate"
                )));
            }
            body.replacen(&args.old_string, &args.new_string, 1)
        };

        tokio::fs::write(&args.path, new_body.as_bytes())
            .await
            .map_err(|e| Error::Tool(format!("{}: {e}", args.path)))?;
        Ok(ToolOutput::text(format!("edited {}", args.path)))
    }
}
