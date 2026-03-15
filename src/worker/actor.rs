//! Kernel actor — single &mut Kernel behind an mpsc channel.
//!
//! Replaces the IPC server's `run_kernel_actor` + `router.rs` + all handler
//! modules. Typed match on KernelCommand, calls kernel methods directly.

use tracing::Instrument;

use crate::kernel::orchestrator_types::Instruction;
use crate::kernel::{Kernel, SchedulingPriority};
use crate::worker::handle::{KernelCommand, KernelHandle};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Spawn the kernel actor as a tokio task. Returns a cloneable handle.
pub fn spawn_kernel(kernel: Kernel, cancel: CancellationToken) -> KernelHandle {
    let (tx, rx) = mpsc::channel(256);
    tokio::spawn(run_kernel_actor(kernel, rx, cancel).instrument(tracing::Span::current()));
    KernelHandle::new(tx)
}

/// The kernel actor loop. Processes commands sequentially (single &mut).
async fn run_kernel_actor(
    mut kernel: Kernel,
    mut rx: mpsc::Receiver<KernelCommand>,
    cancel: CancellationToken,
) {
    tracing::info!("Kernel actor started");
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                tracing::info!("Kernel actor shutting down");
                break;
            }
            cmd = rx.recv() => {
                let Some(cmd) = cmd else {
                    tracing::info!("Kernel actor channel closed");
                    break;
                };
                dispatch(&mut kernel, cmd).await;
            }
        }
    }
}

/// Dispatch a single command to the kernel. Async for CommBusQuery fire-and-spawn.
#[tracing::instrument(skip(kernel, cmd))]
async fn dispatch(kernel: &mut Kernel, cmd: KernelCommand) {
    match cmd {
        KernelCommand::InitializeSession {
            process_id,
            pipeline_config,
            envelope,
            force,
            resp_tx,
        } => {
            // Auto-create PCB if not already registered
            if kernel.lifecycle.get(&process_id).is_none() {
                let _ = kernel.create_process(
                    process_id.clone(),
                    envelope.identity.request_id.clone(),
                    envelope.identity.user_id.clone(),
                    envelope.identity.session_id.clone(),
                    SchedulingPriority::Normal,
                    None,
                );
            }
            let result = kernel.initialize_orchestration(
                process_id.clone(),
                *pipeline_config,
                *envelope,
                force,
            );
            if result.is_ok() {
                kernel.emit_envelope_snapshot(&process_id, "initialized");
            }
            let _ = resp_tx.send(result);
        }

        KernelCommand::GetNextInstruction {
            process_id,
            resp_tx,
        } => {
            let result = kernel.get_next_instruction(&process_id);
            // Auto-terminate PCB when orchestrator says TERMINATE
            if let Ok(ref instr) = result {
                if matches!(instr, Instruction::Terminate { .. }) {
                    let _ = kernel.terminate_process(&process_id);
                }
            }
            let _ = resp_tx.send(result);
        }

        KernelCommand::ProcessAgentResult {
            process_id,
            agent_name,
            output,
            metadata_updates,
            metrics,
            success,
            error_message,
            break_loop,
            resp_tx,
        } => {
            let result = kernel.process_agent_result(
                &process_id,
                &agent_name,
                output,
                metadata_updates,
                metrics,
                success,
                &error_message,
                break_loop,
            );
            let _ = resp_tx.send(result);
        }

        KernelCommand::GetSessionState {
            process_id,
            resp_tx,
        } => {
            let result = kernel.get_orchestration_state(&process_id);
            let _ = resp_tx.send(result);
        }

        KernelCommand::CreateProcess {
            process_id,
            request_id,
            user_id,
            session_id,
            priority,
            resp_tx,
        } => {
            let result = kernel.create_process(
                process_id,
                request_id,
                user_id,
                session_id,
                priority,
                None,
            );
            let _ = resp_tx.send(result);
        }

        KernelCommand::TerminateProcess {
            process_id,
            resp_tx,
        } => {
            let result = kernel.terminate_process(&process_id);
            let _ = resp_tx.send(result);
        }

        KernelCommand::GetSystemStatus { resp_tx } => {
            let status = kernel.get_system_status();
            let _ = resp_tx.send(status);
        }

        KernelCommand::ResolveInterrupt {
            process_id,
            interrupt_id,
            response,
            resp_tx,
        } => {
            let resolved = kernel.interrupts.resolve(&interrupt_id, response, None);
            let result = if resolved {
                kernel.resume_process(&process_id)
            } else {
                Err(crate::types::Error::not_found(format!("Interrupt {} not found", interrupt_id)))
            };
            let _ = resp_tx.send(result);
        }

        // =====================================================================
        // CommBus Federation
        // =====================================================================

        KernelCommand::PublishEvent { event, resp_tx } => {
            let result = kernel.comm.bus.publish(event);
            let _ = resp_tx.send(result);
        }

        KernelCommand::Subscribe {
            subscriber_id,
            event_types,
            resp_tx,
        } => {
            let result = kernel.comm.bus.subscribe(subscriber_id, event_types);
            let _ = resp_tx.send(result);
        }

        KernelCommand::Unsubscribe {
            subscription,
            resp_tx,
        } => {
            kernel.comm.bus.unsubscribe(&subscription);
            let _ = resp_tx.send(());
        }

        KernelCommand::CommBusQuery { query, resp_tx } => {
            // Fire-and-spawn to prevent deadlock: get handler, then spawn
            // a task to send query + await response outside the actor loop.
            if let Some(handler) = kernel.comm.bus.get_query_handler(&query.query_type) {
                tokio::spawn(async move {
                    let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                    if handler.send((query.clone(), response_tx)).is_err() {
                        let _ = resp_tx.send(Err(crate::types::Error::internal(
                            format!("Failed to send query to handler: {}", query.query_type),
                        )));
                        return;
                    }
                    let timeout_ms = query.timeout_ms;
                    match tokio::time::timeout(
                        tokio::time::Duration::from_millis(timeout_ms),
                        response_rx,
                    )
                    .await
                    {
                        Ok(Ok(response)) => {
                            let _ = resp_tx.send(Ok(response));
                        }
                        Ok(Err(_)) => {
                            let _ = resp_tx.send(Err(crate::types::Error::internal(
                                "Query response channel closed",
                            )));
                        }
                        Err(_) => {
                            let _ = resp_tx.send(Err(crate::types::Error::timeout(format!(
                                "Query timeout after {}ms",
                                timeout_ms,
                            ))));
                        }
                    }
                }.instrument(tracing::debug_span!("commbus_query")));
            } else {
                let _ = resp_tx.send(Err(crate::types::Error::validation(format!(
                    "No handler registered for query type: {}",
                    query.query_type,
                ))));
            }
        }

        KernelCommand::ListAgentCards { filter, resp_tx } => {
            let cards = kernel
                .comm.agent_cards
                .list(filter.as_deref())
                .into_iter()
                .cloned()
                .collect();
            let _ = resp_tx.send(cards);
        }

        // =====================================================================
        // Checkpoint/Resume
        // =====================================================================

        KernelCommand::Checkpoint { process_id, resp_tx } => {
            let result = kernel.checkpoint(&process_id);
            let _ = resp_tx.send(result);
        }

        KernelCommand::ResumeFromCheckpoint { snapshot, pipeline_config, resp_tx } => {
            let result = kernel.resume_from_checkpoint(*snapshot, *pipeline_config);
            let _ = resp_tx.send(result);
        }
    }
}
