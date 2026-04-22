//! `pi` — minimal pi-style coding-agent CLI.
//!
//! Modes:
//!   * `interactive` (default)   — read/respond loop on stdin/stdout.
//!   * `print`                   — one-shot, print final assistant text.
//!
//! RPC / TUI modes intentionally omitted from this skeleton.

use agent_core::{Agent, GenaiProvider};
use clap::Parser;
use harness_pi::{context, settings::Settings, tools};
use std::io::{BufRead, Write};
use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(name = "pi", about = "Minimal pi-style coding agent")]
struct Cli {
    /// Run in print mode (one prompt, then exit).
    #[arg(long)]
    print: bool,

    /// Prompt for print mode, or the initial message for interactive mode.
    prompt: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    agent_core::observability::init_tracing("warn");

    let cli = Cli::parse();
    let settings = Settings::load().unwrap_or_default();
    let system_prompt = context::load_system_prompt().unwrap_or_default();

    let provider: Arc<dyn agent_core::LlmProvider> =
        Arc::new(GenaiProvider::from_env(settings.model.clone()));

    let mut builder = Agent::builder()
        .provider(provider)
        .model(settings.model.clone())
        .tools(tools::default_tools());
    if !system_prompt.is_empty() {
        builder = builder.system_prompt(system_prompt);
    } else if let Some(s) = settings.system_prompt.clone() {
        builder = builder.system_prompt(s);
    }
    if let Some(max) = settings.max_iterations {
        builder = builder.budget(agent_core::Budget {
            max_iterations: Some(max),
            max_llm_calls: None,
        });
    }

    let mut agent = builder.build()?;

    if cli.print {
        let prompt = cli
            .prompt
            .ok_or_else(|| anyhow::anyhow!("--print requires a prompt"))?;
        if let Some(text) = agent.run(&prompt).await? {
            println!("{text}");
        }
        return Ok(());
    }

    // Interactive loop.
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    if let Some(p) = cli.prompt.as_deref() {
        run_turn(&mut agent, p).await?;
    }
    loop {
        write!(stdout, "\n> ")?;
        stdout.flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if matches!(line, "/exit" | "/quit") {
            break;
        }
        run_turn(&mut agent, line).await?;
    }
    Ok(())
}

async fn run_turn(agent: &mut Agent, input: &str) -> anyhow::Result<()> {
    match agent.run(input).await {
        Ok(Some(text)) => println!("{text}"),
        Ok(None) => {}
        Err(e) => eprintln!("error: {e}"),
    }
    Ok(())
}
