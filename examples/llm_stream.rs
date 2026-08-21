use jeeves_core::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let llm = Arc::new(MockLlmProvider::new([vec![
        ModelStreamEvent::Text("Detective Mara ".into()),
        ModelStreamEvent::Text("found the ledger ".into()),
        ModelStreamEvent::Text("behind the clock tower.".into()),
    ]]));

    let workflow = Workflow::builder("briefing")
        .stage(Stage::llm(
            "speak",
            LlmAction::text(Prompt::text(
                "Summarize the case file in one dramatic sentence.",
            )),
        ))
        .build()?;

    let engine = Engine::builder().llm(llm).workflow(workflow)?.build()?;
    let mut handle = engine.start("briefing", "case 41: the missing ledger")?;

    let mut events = handle
        .take_events()
        .ok_or_else(|| Error::internal("event stream already taken"))?;

    let mut final_outcome = None;
    while let Some(event) = events.recv().await {
        match event {
            RunEvent::TextDelta { content, .. } => print!("{content}"),
            RunEvent::Finished(outcome) => final_outcome = Some(outcome),
            _ => {}
        }
    }

    println!();
    let outcome =
        final_outcome.ok_or_else(|| Error::internal("run ended without a Finished event"))?;
    println!("completed: {}", outcome.completed());
    println!("model calls: {}", outcome.result().usage.llm_calls);

    Ok(())
}
