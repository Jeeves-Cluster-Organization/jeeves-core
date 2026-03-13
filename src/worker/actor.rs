//! Kernel actor — single &mut Kernel behind an mpsc channel.
//!
//! Replaces the IPC server's `run_kernel_actor` + `router.rs` + all handler
//! modules. Typed match on KernelCommand, calls kernel methods directly.

use crate::kernel::orchestrator_types::InstructionKind;
use crate::kernel::{Kernel, SchedulingPriority};
use crate::worker::handle::{KernelCommand, KernelHandle};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Spawn the kernel actor as a tokio task. Returns a cloneable handle.
pub fn spawn_kernel(kernel: Kernel, cancel: CancellationToken) -> KernelHandle {
    let (tx, rx) = mpsc::channel(256);
    tokio::spawn(run_kernel_actor(kernel, rx, cancel));
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
                dispatch(&mut kernel, cmd);
            }
        }
    }
}

/// Dispatch a single command to the kernel. All kernel methods are sync
/// (except execute_query, which we don't expose through this path).
fn dispatch(kernel: &mut Kernel, cmd: KernelCommand) {
    match cmd {
        KernelCommand::InitializeSession {
            process_id,
            pipeline_config,
            envelope,
            force,
            resp_tx,
        } => {
            // Auto-create PCB if not already registered
            if kernel.get_process(&process_id).is_none() {
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
                if instr.kind == InstructionKind::Terminate {
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
    }
}
