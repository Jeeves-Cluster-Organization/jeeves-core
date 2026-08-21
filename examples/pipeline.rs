use jeeves_core::prelude::*;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn build_workflow() -> Result<Workflow> {
    let verify_calls = Arc::new(AtomicU32::new(0));
    let verify_calls_for_stage = verify_calls.clone();

    Workflow::builder("newsroom")
        .stage(
            Stage::deterministic_fn("collect", |run: &RunView<'_>| {
                let topic = run
                    .input_text()
                    .ok_or_else(|| Error::invalid_input("run input must be text"))?;
                Ok(json!({ "topic": topic, "leads": 3 }))
            })
            .next("verify"),
        )
        .stage(
            Stage::deterministic_fn("verify", move |run: &RunView<'_>| {
                let _ = run
                    .latest_output("collect")
                    .and_then(|output| output.get("topic"))
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| Error::invalid_input("collect.topic missing"))?;

                let call = verify_calls_for_stage.fetch_add(1, Ordering::SeqCst);
                if call < 2 {
                    Err(Error::transient("sources unavailable"))
                } else {
                    Ok(json!({ "verified": true }))
                }
            })
            .retry(RetryPolicy::exponential(3, Duration::from_millis(50))),
        )
        .build()
}

#[tokio::main]
async fn main() -> Result<()> {
    let engine = Engine::builder().workflow(build_workflow()?)?.build()?;
    let mut handle = engine.start("newsroom", "city hall")?;

    let mut events = handle
        .take_events()
        .ok_or_else(|| Error::internal("event stream already taken"))?;

    while let Some(event) = events.recv().await {
        match event {
            RunEvent::StageStarted { attempt } => {
                println!(
                    "stage started: {} (visit {}, attempt {})",
                    attempt.stage, attempt.visit, attempt.attempt
                );
            }
            RunEvent::Routed { from, to, reason } => {
                println!(
                    "routed {from} -> {} ({reason:?})",
                    to.as_deref().unwrap_or("<complete>")
                );
            }
            RunEvent::Finished(outcome) => {
                println!("run finished: completed = {}", outcome.completed());
                let result = outcome.result();
                for record in &result.history {
                    println!(
                        "attempt {}:{} -> {:?} ({} failure(s))",
                        record.stage,
                        record.attempt,
                        record.output,
                        record.failures.len()
                    );
                }
                println!("usage: {:?}", result.usage);
            }
            _ => {}
        }
    }

    Ok(())
}
