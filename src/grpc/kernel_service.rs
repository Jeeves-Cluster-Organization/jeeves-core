//! KernelService gRPC implementation.
//!
//! Implements kernel_service proto with process lifecycle and resource management.

use crate::kernel::Kernel;
use crate::proto::{
    kernel_service_server::KernelService, CheckQuotaRequest, CheckRateLimitRequest,
    CreateProcessRequest, GetNextRunnableRequest, GetProcessCountsRequest, GetProcessRequest,
    ListProcessesRequest, ListProcessesResponse, ProcessControlBlock, ProcessCountsResponse,
    QuotaResult, RateLimitResult, RecordUsageRequest, ResourceUsage, ScheduleProcessRequest,
    TerminateProcessRequest, TransitionStateRequest,
};
use crate::types::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

/// KernelService implementation wrapping the Kernel actor.
#[derive(Debug)]
pub struct KernelServiceImpl {
    kernel: Arc<Mutex<Kernel>>,
}

impl KernelServiceImpl {
    pub fn new(kernel: Arc<Mutex<Kernel>>) -> Self {
        Self { kernel }
    }
}

#[tonic::async_trait]
impl KernelService for KernelServiceImpl {
    async fn create_process(
        &self,
        request: Request<CreateProcessRequest>,
    ) -> std::result::Result<Response<ProcessControlBlock>, Status> {
        let req = request.into_inner();

        // Validate
        if req.pid.is_empty() {
            return Err(Error::validation("pid is required".to_string()).to_grpc_status());
        }
        if req.user_id.is_empty() {
            return Err(Error::validation("user_id is required".to_string()).to_grpc_status());
        }

        // Parse priority
        let priority = crate::kernel::SchedulingPriority::try_from(req.priority)
            .map_err(|e| e.to_grpc_status())?;

        // Parse quota
        let quota = if let Some(q) = req.quota {
            Some(
                crate::kernel::ResourceQuota::try_from(q).map_err(|e| e.to_grpc_status())?,
            )
        } else {
            None
        };

        // Call kernel
        let mut kernel = self.kernel.lock().await;
        let pcb = kernel
            .create_process(
                req.pid.clone(),
                req.request_id,
                req.user_id,
                req.session_id,
                priority,
                quota,
            )
            .map_err(|e| e.to_grpc_status())?;

        Ok(Response::new(ProcessControlBlock::from(pcb)))
    }

    async fn get_process(
        &self,
        request: Request<GetProcessRequest>,
    ) -> std::result::Result<Response<ProcessControlBlock>, Status> {
        let req = request.into_inner();

        if req.pid.is_empty() {
            return Err(Error::validation("pid is required".to_string()).to_grpc_status());
        }

        let kernel = self.kernel.lock().await;
        let pcb = kernel
            .get_process(&req.pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", req.pid)))?;

        Ok(Response::new(ProcessControlBlock::from(pcb.clone())))
    }

    async fn schedule_process(
        &self,
        request: Request<ScheduleProcessRequest>,
    ) -> std::result::Result<Response<ProcessControlBlock>, Status> {
        let req = request.into_inner();

        if req.pid.is_empty() {
            return Err(Error::validation("pid is required".to_string()).to_grpc_status());
        }

        let mut kernel = self.kernel.lock().await;
        kernel
            .lifecycle
            .schedule(&req.pid)
            .map_err(|e| e.to_grpc_status())?;

        let pcb = kernel
            .get_process(&req.pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", req.pid)))?;

        Ok(Response::new(ProcessControlBlock::from(pcb.clone())))
    }

    async fn get_next_runnable(
        &self,
        _request: Request<GetNextRunnableRequest>,
    ) -> std::result::Result<Response<ProcessControlBlock>, Status> {
        let mut kernel = self.kernel.lock().await;
        let pcb = kernel.get_next_runnable().ok_or_else(|| {
            Error::not_found("No runnable processes".to_string()).to_grpc_status()
        })?;

        Ok(Response::new(ProcessControlBlock::from(pcb)))
    }

    async fn transition_state(
        &self,
        request: Request<TransitionStateRequest>,
    ) -> std::result::Result<Response<ProcessControlBlock>, Status> {
        let req = request.into_inner();

        if req.pid.is_empty() {
            return Err(Error::validation("pid is required".to_string()).to_grpc_status());
        }

        let new_state =
            crate::kernel::ProcessState::try_from(req.new_state).map_err(|e| e.to_grpc_status())?;

        let mut kernel = self.kernel.lock().await;

        // Get current PCB to check transition
        let pcb = kernel
            .get_process(&req.pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", req.pid)))?;

        // Validate transition
        if !pcb.state.can_transition_to(new_state) {
            return Err(Error::state_transition(format!(
                "Invalid transition from {:?} to {:?}",
                pcb.state, new_state
            ))
            .to_grpc_status());
        }

        // Perform state-specific actions
        match new_state {
            crate::kernel::ProcessState::Ready => {
                kernel
                    .lifecycle
                    .schedule(&req.pid)
                    .map_err(|e| e.to_grpc_status())?;
            }
            crate::kernel::ProcessState::Running => {
                kernel
                    .start_process(&req.pid)
                    .map_err(|e| e.to_grpc_status())?;
            }
            crate::kernel::ProcessState::Terminated => {
                kernel
                    .terminate_process(&req.pid)
                    .map_err(|e| e.to_grpc_status())?;
            }
            crate::kernel::ProcessState::Blocked => {
                kernel
                    .block_process(&req.pid, req.reason.clone())
                    .map_err(|e| e.to_grpc_status())?;
            }
            _ => {
                // For other states, just update via lifecycle manager
                // (This is a simplified implementation - full version would handle all cases)
            }
        }

        let pcb = kernel
            .get_process(&req.pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", req.pid)))?;

        Ok(Response::new(ProcessControlBlock::from(pcb.clone())))
    }

    async fn terminate_process(
        &self,
        request: Request<TerminateProcessRequest>,
    ) -> std::result::Result<Response<ProcessControlBlock>, Status> {
        let req = request.into_inner();

        if req.pid.is_empty() {
            return Err(Error::validation("pid is required".to_string()).to_grpc_status());
        }

        let mut kernel = self.kernel.lock().await;
        kernel
            .terminate_process(&req.pid)
            .map_err(|e| e.to_grpc_status())?;

        let pcb = kernel
            .get_process(&req.pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", req.pid)))?;

        Ok(Response::new(ProcessControlBlock::from(pcb.clone())))
    }

    async fn check_quota(
        &self,
        request: Request<CheckQuotaRequest>,
    ) -> std::result::Result<Response<QuotaResult>, Status> {
        let req = request.into_inner();

        if req.pid.is_empty() {
            return Err(Error::validation("pid is required".to_string()).to_grpc_status());
        }

        let kernel = self.kernel.lock().await;
        let pcb = kernel
            .get_process(&req.pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", req.pid)))?;

        let result = kernel.check_quota(&req.pid);
        let within_bounds = result.is_ok();
        let exceeded_reason = result.err().map(|e| e.to_string()).unwrap_or_default();

        Ok(Response::new(QuotaResult {
            within_bounds,
            exceeded_reason,
            usage: Some(ResourceUsage::from(pcb.usage.clone())),
            quota: Some(crate::proto::ResourceQuota::from(pcb.quota.clone())),
        }))
    }

    async fn record_usage(
        &self,
        request: Request<RecordUsageRequest>,
    ) -> std::result::Result<Response<ResourceUsage>, Status> {
        let req = request.into_inner();

        if req.pid.is_empty() {
            return Err(Error::validation("pid is required".to_string()).to_grpc_status());
        }

        let mut kernel = self.kernel.lock().await;

        // Get user_id from PCB
        let user_id = {
            let pcb = kernel
                .get_process(&req.pid)
                .ok_or_else(|| Error::not_found(format!("Process {} not found", req.pid)))?;
            pcb.user_id.clone()
        };

        kernel.record_usage(
            &user_id,
            req.llm_calls,
            req.tool_calls,
            req.tokens_in as i64,
            req.tokens_out as i64,
        );

        // Get updated PCB
        let pcb = kernel
            .get_process(&req.pid)
            .ok_or_else(|| Error::not_found(format!("Process {} not found", req.pid)))?;

        Ok(Response::new(ResourceUsage::from(pcb.usage.clone())))
    }

    async fn check_rate_limit(
        &self,
        request: Request<CheckRateLimitRequest>,
    ) -> std::result::Result<Response<RateLimitResult>, Status> {
        let req = request.into_inner();

        if req.user_id.is_empty() {
            return Err(Error::validation("user_id is required".to_string()).to_grpc_status());
        }

        let mut kernel = self.kernel.lock().await;
        let result = if req.record {
            kernel.rate_limiter.check_rate_limit(&req.user_id)
        } else {
            Ok(())
        };

        let current_count = kernel.rate_limiter.get_current_rate(&req.user_id) as i32;

        let allowed = result.is_ok();
        let reason = result.as_ref().err().map(|e| e.to_string()).unwrap_or_default();

        Ok(Response::new(RateLimitResult {
            allowed,
            exceeded: !allowed,
            reason,
            limit_type: if !allowed {
                "minute".to_string()
            } else {
                String::new()
            },
            current_count,
            limit: 60, // Default from RateLimitConfig
            retry_after_seconds: if !allowed { 60.0 } else { 0.0 },
            remaining: if allowed {
                60 - current_count
            } else {
                0
            },
        }))
    }

    async fn list_processes(
        &self,
        request: Request<ListProcessesRequest>,
    ) -> std::result::Result<Response<ListProcessesResponse>, Status> {
        let req = request.into_inner();

        let kernel = self.kernel.lock().await;
        let processes = kernel.list_processes();

        // Filter by state if specified
        let filtered = if req.state != 0 {
            let state = crate::kernel::ProcessState::try_from(req.state)
                .map_err(|e| e.to_grpc_status())?;
            processes
                .into_iter()
                .filter(|p| p.state == state)
                .collect::<Vec<_>>()
        } else {
            processes
        };

        // Filter by user_id if specified
        let filtered = if !req.user_id.is_empty() {
            filtered
                .into_iter()
                .filter(|p| p.user_id == req.user_id)
                .collect()
        } else {
            filtered
        };

        let proto_processes: Vec<ProcessControlBlock> =
            filtered.into_iter().map(ProcessControlBlock::from).collect();

        Ok(Response::new(ListProcessesResponse {
            processes: proto_processes,
        }))
    }

    async fn get_process_counts(
        &self,
        _request: Request<GetProcessCountsRequest>,
    ) -> std::result::Result<Response<ProcessCountsResponse>, Status> {
        let kernel = self.kernel.lock().await;

        let total = kernel.process_count() as i32;
        let queue_depth = 0; // TODO: implement queue depth in kernel

        let mut counts_by_state = std::collections::HashMap::new();
        for state in &[
            crate::kernel::ProcessState::New,
            crate::kernel::ProcessState::Ready,
            crate::kernel::ProcessState::Running,
            crate::kernel::ProcessState::Waiting,
            crate::kernel::ProcessState::Blocked,
            crate::kernel::ProcessState::Terminated,
            crate::kernel::ProcessState::Zombie,
        ] {
            let count = kernel.process_count_by_state(*state) as i32;
            counts_by_state.insert(format!("{:?}", state).to_lowercase(), count);
        }

        Ok(Response::new(ProcessCountsResponse {
            counts_by_state,
            total,
            queue_depth,
        }))
    }
}
