//! Kernel actor — single &mut Kernel behind an mpsc channel.
//!
//! Replaces the IPC server's `run_kernel_actor` + `router.rs` + all handler
//! modules. Typed match on KernelCommand, calls kernel methods directly.

use tracing::Instrument;

use crate::kernel::protocol::Instruction;
use crate::kernel::Kernel;
use crate::kernel::handle::{KernelCommand, KernelHandle};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Spawn the kernel actor as a tokio task. Returns a cloneable handle.
pub fn spawn(kernel: Kernel, cancel: CancellationToken) -> KernelHandle {
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
            run_id,
            pipeline_config,
            envelope,
            force,
            resp_tx,
        } => {
            // Auto-create PCB if not already registered
            if kernel.lifecycle.get(&run_id).is_none() {
                let _ = kernel.create_process(
                    run_id.clone(),
                    envelope.identity.request_id.clone(),
                    envelope.identity.user_id.clone(),
                    envelope.identity.session_id.clone(),
                    None,
                );
            }
            let result = kernel.initialize_orchestration(
                run_id.clone(),
                *pipeline_config,
                *envelope,
                force,
            );
            let _ = resp_tx.send(result);
        }

        KernelCommand::GetNextInstruction {
            run_id,
            resp_tx,
        } => {
            let result = kernel.get_next_instruction(&run_id);
            // Auto-terminate PCB when orchestrator says TERMINATE
            if let Ok(ref instr) = result {
                if matches!(instr, Instruction::Terminate { .. }) {
                    let _ = kernel.terminate_process(&run_id);
                }
            }
            let _ = resp_tx.send(result);
        }

        KernelCommand::ProcessAgentResult {
            run_id,
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
                &run_id,
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
            run_id,
            resp_tx,
        } => {
            let result = kernel.get_orchestration_state(&run_id);
            let _ = resp_tx.send(result);
        }

        KernelCommand::CreateProcess {
            run_id,
            request_id,
            user_id,
            session_id,
            resp_tx,
        } => {
            let result = kernel.create_process(
                run_id,
                request_id,
                user_id,
                session_id,
                None,
            );
            let _ = resp_tx.send(result);
        }

        KernelCommand::TerminateProcess {
            run_id,
            resp_tx,
        } => {
            let result = kernel.terminate_process(&run_id);
            let _ = resp_tx.send(result);
        }

        KernelCommand::GetSystemStatus { resp_tx } => {
            let status = kernel.get_system_status();
            let _ = resp_tx.send(status);
        }

        KernelCommand::ResolveInterrupt {
            run_id,
            interrupt_id,
            response,
            resp_tx,
        } => {
            let result = kernel.resolve_process_interrupt(&run_id, &interrupt_id, response);
            let _ = resp_tx.send(result);
        }

        KernelCommand::SetProcessInterrupt {
            run_id,
            interrupt,
            resp_tx,
        } => {
            let result = kernel.set_process_interrupt(&run_id, interrupt);
            let _ = resp_tx.send(result);
        }

        KernelCommand::GetToolHealth { tool_name, resp_tx } => {
            let report = match tool_name {
                Some(ref name) => serde_json::to_value(kernel.tools.health.check_tool_health(name)),
                None => serde_json::to_value(kernel.tools.health.check_system_health()),
            };
            let result = report.map_err(|e| {
                crate::types::Error::internal(format!("Health serialization: {}", e))
            });
            let _ = resp_tx.send(result);
        }

        KernelCommand::RegisterRoutingFn { name, routing_fn, resp_tx } => {
            kernel.register_routing_fn(name, routing_fn);
            let _ = resp_tx.send(());
        }
    }
}
