//! Kernel service handler — process lifecycle, quota, rate limiting.

use crate::ipc::dispatch::{str_field, DispatchResponse};
use crate::kernel::{Kernel, ProcessState, ResourceQuota, SchedulingPriority};
use crate::types::{Error, ProcessId, RequestId, Result, SessionId, UserId};
use serde_json::Value;

pub async fn handle(
    kernel: &mut Kernel,
    method: &str,
    body: Value,
) -> Result<DispatchResponse> {
    match method {
        "CreateProcess" => {
            let pid_str = str_field(&body, "pid")?;
            let pid = ProcessId::from_string(pid_str.clone())
                .map_err(|e| Error::validation(e.to_string()))?;

            let request_id_str = body.get("request_id").and_then(|v| v.as_str()).unwrap_or("");
            let request_id = if request_id_str.is_empty() {
                RequestId::new()
            } else {
                RequestId::must(request_id_str)
            };

            let user_id_str = body.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
            let user_id = if user_id_str.is_empty() {
                UserId::from_string("anonymous".to_string())
                    .map_err(|e| Error::validation(e.to_string()))?
            } else {
                UserId::from_string(user_id_str.to_string())
                    .map_err(|e| Error::validation(e.to_string()))?
            };

            let session_id_str = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
            let session_id = if session_id_str.is_empty() {
                SessionId::new()
            } else {
                SessionId::must(session_id_str)
            };

            let priority = parse_priority(&body)?;
            let quota = parse_quota(&body)?;

            let pcb = kernel.create_process(pid, request_id, user_id, session_id, priority, quota)?;
            Ok(DispatchResponse::Single(pcb_to_value(&pcb)))
        }

        "GetProcess" => {
            let pid = parse_pid(&body)?;
            let pcb = kernel
                .get_process(&pid)
                .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
            Ok(DispatchResponse::Single(pcb_to_value(pcb)))
        }

        "ScheduleProcess" => {
            let pid = parse_pid(&body)?;
            kernel.lifecycle.schedule(&pid)?;
            let pcb = kernel
                .get_process(&pid)
                .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
            Ok(DispatchResponse::Single(pcb_to_value(pcb)))
        }

        "GetNextRunnable" => {
            let pcb = kernel
                .get_next_runnable()
                .ok_or_else(|| Error::not_found("No runnable processes"))?;
            Ok(DispatchResponse::Single(pcb_to_value(&pcb)))
        }

        "TransitionState" => {
            let pid = parse_pid(&body)?;
            let new_state_str = str_field(&body, "new_state")?;
            let new_state = parse_process_state(&new_state_str)?;
            let reason = body.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string();

            let pcb = kernel
                .get_process(&pid)
                .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
            if !pcb.state.can_transition_to(new_state) {
                return Err(Error::state_transition(format!(
                    "Invalid transition from {:?} to {:?}",
                    pcb.state, new_state
                )));
            }

            match new_state {
                ProcessState::Ready => kernel.lifecycle.schedule(&pid)?,
                ProcessState::Running => kernel.start_process(&pid)?,
                ProcessState::Terminated => kernel.terminate_process(&pid)?,
                ProcessState::Blocked => kernel.block_process(&pid, reason)?,
                _ => {}
            }

            let pcb = kernel
                .get_process(&pid)
                .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
            Ok(DispatchResponse::Single(pcb_to_value(pcb)))
        }

        "TerminateProcess" => {
            let pid = parse_pid(&body)?;
            kernel.terminate_process(&pid)?;
            let pcb = kernel
                .get_process(&pid)
                .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
            Ok(DispatchResponse::Single(pcb_to_value(pcb)))
        }

        "CheckQuota" => {
            let pid = parse_pid(&body)?;
            let pcb = kernel
                .get_process(&pid)
                .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;

            let result = kernel.check_quota(&pid);
            let within_bounds = result.is_ok();
            let exceeded_reason = result.err().map(|e| e.to_string()).unwrap_or_default();

            Ok(DispatchResponse::Single(serde_json::json!({
                "within_bounds": within_bounds,
                "exceeded_reason": exceeded_reason,
                "llm_calls": pcb.usage.llm_calls,
                "tool_calls": pcb.usage.tool_calls,
                "agent_hops": pcb.usage.agent_hops,
                "tokens_in": pcb.usage.tokens_in,
                "tokens_out": pcb.usage.tokens_out,
            })))
        }

        "RecordUsage" => {
            let pid = parse_pid(&body)?;

            let user_id_str = {
                let pcb = kernel
                    .get_process(&pid)
                    .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
                pcb.user_id.as_str().to_string()
            };

            let llm_calls = body.get("llm_calls").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let tool_calls = body.get("tool_calls").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let tokens_in = body.get("tokens_in").and_then(|v| v.as_i64()).unwrap_or(0);
            let tokens_out = body.get("tokens_out").and_then(|v| v.as_i64()).unwrap_or(0);

            kernel.record_usage(&user_id_str, llm_calls, tool_calls, tokens_in, tokens_out);

            let pcb = kernel
                .get_process(&pid)
                .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;

            Ok(DispatchResponse::Single(serde_json::json!({
                "llm_calls": pcb.usage.llm_calls,
                "tool_calls": pcb.usage.tool_calls,
                "agent_hops": pcb.usage.agent_hops,
                "tokens_in": pcb.usage.tokens_in,
                "tokens_out": pcb.usage.tokens_out,
            })))
        }

        "CheckRateLimit" => {
            let user_id = str_field(&body, "user_id")?;
            if user_id.is_empty() {
                return Err(Error::validation("user_id is required"));
            }

            let record = body.get("record").and_then(|v| v.as_bool()).unwrap_or(true);
            let result = if record {
                kernel.rate_limiter.check_rate_limit(&user_id)
            } else {
                Ok(())
            };

            let current_count = kernel.rate_limiter.get_current_rate(&user_id) as i32;
            let allowed = result.is_ok();
            let reason = result.err().map(|e| e.to_string()).unwrap_or_default();

            Ok(DispatchResponse::Single(serde_json::json!({
                "allowed": allowed,
                "exceeded": !allowed,
                "reason": reason,
                "limit_type": if !allowed { "minute" } else { "" },
                "current_count": current_count,
                "limit": 60,
                "retry_after_seconds": if !allowed { 60.0 } else { 0.0 },
                "remaining": if allowed { 60 - current_count } else { 0 },
            })))
        }

        "ListProcesses" => {
            let state_str = body.get("state").and_then(|v| v.as_str()).unwrap_or("");
            let user_id_str = body.get("user_id").and_then(|v| v.as_str()).unwrap_or("");

            let processes = kernel.list_processes();

            let filtered: Vec<_> = processes
                .into_iter()
                .filter(|p| {
                    if !state_str.is_empty() {
                        let expected = parse_process_state(state_str);
                        match expected {
                            Ok(s) => p.state == s,
                            Err(_) => false,
                        }
                    } else {
                        true
                    }
                })
                .filter(|p| {
                    if !user_id_str.is_empty() {
                        p.user_id.as_str() == user_id_str
                    } else {
                        true
                    }
                })
                .collect();

            let process_values: Vec<Value> = filtered.iter().map(pcb_to_value).collect();

            Ok(DispatchResponse::Single(serde_json::json!({ "processes": process_values })))
        }

        "GetProcessCounts" => {
            let total = kernel.process_count() as i32;
            let mut counts_by_state = serde_json::Map::new();
            for state in &[
                ProcessState::New,
                ProcessState::Ready,
                ProcessState::Running,
                ProcessState::Waiting,
                ProcessState::Blocked,
                ProcessState::Terminated,
                ProcessState::Zombie,
            ] {
                let count = kernel.process_count_by_state(*state) as i32;
                counts_by_state.insert(
                    format!("{:?}", state).to_lowercase(),
                    Value::Number(count.into()),
                );
            }

            Ok(DispatchResponse::Single(serde_json::json!({
                "counts_by_state": counts_by_state,
                "total": total,
                "queue_depth": 0,
            })))
        }

        _ => Err(Error::not_found(format!("Unknown kernel method: {}", method))),
    }
}

// =============================================================================
// Kernel-specific helpers
// =============================================================================

fn parse_pid(body: &Value) -> Result<ProcessId> {
    let pid_str = str_field(body, "pid")?;
    ProcessId::from_string(pid_str).map_err(|e| Error::validation(e.to_string()))
}

fn parse_priority(body: &Value) -> Result<SchedulingPriority> {
    let s = body
        .get("priority")
        .and_then(|v| v.as_str())
        .unwrap_or("NORMAL");
    match s.to_uppercase().as_str() {
        "REALTIME" => Ok(SchedulingPriority::Realtime),
        "HIGH" => Ok(SchedulingPriority::High),
        "NORMAL" => Ok(SchedulingPriority::Normal),
        "LOW" => Ok(SchedulingPriority::Low),
        "IDLE" => Ok(SchedulingPriority::Idle),
        _ => Err(Error::validation(format!("Invalid priority: {}", s))),
    }
}

pub fn parse_process_state(s: &str) -> Result<ProcessState> {
    match s.to_uppercase().as_str() {
        "NEW" => Ok(ProcessState::New),
        "READY" => Ok(ProcessState::Ready),
        "RUNNING" => Ok(ProcessState::Running),
        "WAITING" => Ok(ProcessState::Waiting),
        "BLOCKED" => Ok(ProcessState::Blocked),
        "TERMINATED" => Ok(ProcessState::Terminated),
        "ZOMBIE" => Ok(ProcessState::Zombie),
        _ => Err(Error::validation(format!("Invalid process state: {}", s))),
    }
}

fn parse_quota(body: &Value) -> Result<Option<ResourceQuota>> {
    let q = match body.get("quota") {
        Some(v) if v.is_object() => v,
        _ => return Ok(None),
    };

    Ok(Some(ResourceQuota {
        max_llm_calls: q.get("max_llm_calls").and_then(|v| v.as_i64()).unwrap_or(100) as i32,
        max_tool_calls: q.get("max_tool_calls").and_then(|v| v.as_i64()).unwrap_or(200) as i32,
        max_agent_hops: q.get("max_agent_hops").and_then(|v| v.as_i64()).unwrap_or(200) as i32,
        max_iterations: q.get("max_iterations").and_then(|v| v.as_i64()).unwrap_or(50) as i32,
        timeout_seconds: q.get("timeout_seconds").and_then(|v| v.as_i64()).unwrap_or(300) as i32,
        max_input_tokens: q.get("max_input_tokens").and_then(|v| v.as_i64()).unwrap_or(100_000) as i32,
        max_output_tokens: q.get("max_output_tokens").and_then(|v| v.as_i64()).unwrap_or(50_000) as i32,
        max_context_tokens: q.get("max_context_tokens").and_then(|v| v.as_i64()).unwrap_or(150_000) as i32,
        soft_timeout_seconds: q.get("soft_timeout_seconds").and_then(|v| v.as_i64()).unwrap_or(240) as i32,
        rate_limit_rpm: q.get("rate_limit_rpm").and_then(|v| v.as_i64()).unwrap_or(60) as i32,
        rate_limit_rph: q.get("rate_limit_rph").and_then(|v| v.as_i64()).unwrap_or(1000) as i32,
        rate_limit_burst: q.get("rate_limit_burst").and_then(|v| v.as_i64()).unwrap_or(10) as i32,
        max_inference_requests: q.get("max_inference_requests").and_then(|v| v.as_i64()).unwrap_or(50) as i32,
        max_inference_input_chars: q.get("max_inference_input_chars").and_then(|v| v.as_i64()).unwrap_or(500_000) as i32,
    }))
}

/// Convert PCB to the dict shape expected by `kernel_client.py._dict_to_process_info`.
pub fn pcb_to_value(pcb: &crate::kernel::ProcessControlBlock) -> Value {
    serde_json::json!({
        "pid": pcb.pid.as_str(),
        "request_id": pcb.request_id.as_str(),
        "user_id": pcb.user_id.as_str(),
        "session_id": pcb.session_id.as_str(),
        "state": format!("{:?}", pcb.state).to_uppercase(),
        "priority": format!("{:?}", pcb.priority).to_uppercase(),
        "usage": {
            "llm_calls": pcb.usage.llm_calls,
            "tool_calls": pcb.usage.tool_calls,
            "agent_hops": pcb.usage.agent_hops,
            "tokens_in": pcb.usage.tokens_in,
            "tokens_out": pcb.usage.tokens_out,
        },
        "current_stage": "",
    })
}
