# Hybrid Functional + Rust Kernel Architecture

## Overview

This document proposes a hybrid architecture that combines Rust's performance and safety guarantees with functional programming principles for auditability, testability, and correctness.

**Philosophy**: Keep mutable state for performance, but structure code around pure functions and explicit events for everything that matters.

---

## Architecture Principles

### What We Keep (Rust Idioms)

| Component | Rationale |
|-----------|-----------|
| `Arc<Mutex<Kernel>>` | Proven pattern, Tokio ecosystem compatibility |
| Mutable state in Kernel | Performance, avoid copying large structures |
| `&mut self` methods | Rust idiom, borrow checker guarantees |
| HashMap for processes | O(1) lookups, efficient for our scale |
| BinaryHeap for ready queue | O(log n) scheduling |

### What We Add (FP Principles)

| Component | Rationale |
|-----------|-----------|
| Event sourcing layer | Audit trail, time-travel debugging |
| Pure validation functions | Testability, correctness |
| Explicit state transitions | Clear state machine semantics |
| Command/Query separation | Clarity, easier reasoning |
| Immutable event log | Append-only for safety |

---

## Layered Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  gRPC Layer (existing)                                          │
│  • Request handling                                             │
│  • Proto conversion                                             │
│  • Error mapping                                                │
└───────────────────────────┬─────────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────────┐
│  Command Layer (NEW)                                             │
│  • Commands represent intentions                                │
│  • Each command is validated before execution                   │
│  • Commands produce Events on success                           │
└───────────────────────────┬─────────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────────┐
│  Kernel Core (modified)                                          │
│  • Mutable state (performance)                                  │
│  • Applies events to state                                      │
│  • Emits events to EventLog                                     │
└───────────────────────────┬─────────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────────┐
│  Event Log (NEW)                                                 │
│  • Append-only event storage                                    │
│  • Enables replay, audit, debugging                             │
│  • Optional persistence                                         │
└─────────────────────────────────────────────────────────────────┘
```

---

## Core Types

### Commands (Intentions)

Commands represent **what the caller wants to happen**. They may succeed or fail.

```rust
/// Commands are intentions that may succeed or fail.
/// They carry all information needed to validate and execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KernelCommand {
    // Process Lifecycle
    CreateProcess {
        pid: String,
        request_id: String,
        user_id: String,
        session_id: String,
        priority: SchedulingPriority,
        quota: Option<ResourceQuota>,
    },
    ScheduleProcess { pid: String },
    StartProcess { pid: String },
    BlockProcess { pid: String, reason: String },
    WaitProcess { pid: String, interrupt_kind: InterruptKind },
    ResumeProcess { pid: String },
    TerminateProcess { pid: String, reason: Option<String> },
    CleanupProcess { pid: String },

    // Resource Tracking
    RecordUsage {
        pid: String,
        llm_calls: i32,
        tool_calls: i32,
        tokens_in: i64,
        tokens_out: i64,
    },

    // Orchestration
    InitializeSession {
        process_id: String,
        pipeline_config: PipelineConfig,
        envelope: Envelope,
        force: bool,
    },
    ReportAgentResult {
        process_id: String,
        metrics: AgentExecutionMetrics,
        envelope: Envelope,
    },

    // Interrupts
    RaiseInterrupt {
        kind: InterruptKind,
        request_id: String,
        user_id: String,
        session_id: String,
        envelope_id: String,
        question: Option<String>,
        message: Option<String>,
    },
    ResolveInterrupt {
        interrupt_id: String,
        response: InterruptResponse,
        user_id: Option<String>,
    },

    // Services
    RegisterService { info: ServiceInfo },
    UnregisterService { service_name: String },
}
```

### Events (Facts)

Events represent **what happened**. They are facts that cannot be disputed.

```rust
/// Events are immutable facts about what happened.
/// They are the source of truth for the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelEvent {
    /// Unique event ID
    pub event_id: Uuid,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// The actual event data
    pub kind: KernelEventKind,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Optional causation ID (which event/command caused this)
    pub causation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KernelEventKind {
    // Process Lifecycle Events
    ProcessCreated {
        pid: String,
        request_id: String,
        user_id: String,
        session_id: String,
        priority: SchedulingPriority,
        quota: ResourceQuota,
    },
    ProcessScheduled { pid: String },
    ProcessStarted { pid: String, started_at: DateTime<Utc> },
    ProcessBlocked { pid: String, reason: String },
    ProcessWaiting { pid: String, interrupt_kind: InterruptKind },
    ProcessResumed { pid: String },
    ProcessTerminated { pid: String, reason: TerminalReason },
    ProcessCleanedUp { pid: String },
    ProcessRemoved { pid: String },

    // Resource Events
    UsageRecorded {
        pid: String,
        user_id: String,
        llm_calls: i32,
        tool_calls: i32,
        tokens_in: i64,
        tokens_out: i64,
    },
    QuotaExceeded {
        pid: String,
        resource_type: ResourceType,
        limit: i64,
        actual: i64,
    },
    RateLimitExceeded {
        user_id: String,
        limit_type: RateLimitType,
        limit: i32,
        window_seconds: i32,
    },

    // Orchestration Events
    SessionInitialized {
        process_id: String,
        pipeline_name: String,
        stage_order: Vec<String>,
        max_iterations: i32,
        max_llm_calls: i32,
        max_agent_hops: i32,
    },
    AgentExecutionStarted {
        process_id: String,
        agent_name: String,
        stage: String,
    },
    AgentExecutionCompleted {
        process_id: String,
        agent_name: String,
        metrics: AgentExecutionMetrics,
    },
    SessionTerminated {
        process_id: String,
        reason: TerminalReason,
    },

    // Interrupt Events
    InterruptRaised {
        interrupt_id: String,
        kind: InterruptKind,
        envelope_id: String,
        ttl_seconds: i64,
    },
    InterruptResolved {
        interrupt_id: String,
        response_type: String,
        resolved_by: Option<String>,
    },
    InterruptExpired {
        interrupt_id: String,
    },
    InterruptCancelled {
        interrupt_id: String,
    },

    // Service Events
    ServiceRegistered {
        service_name: String,
        capabilities: Vec<String>,
    },
    ServiceUnregistered {
        service_name: String,
    },
    ServiceHealthChanged {
        service_name: String,
        old_status: ServiceStatus,
        new_status: ServiceStatus,
    },
}

/// Resource types for quota tracking
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ResourceType {
    LlmCalls,
    ToolCalls,
    InputTokens,
    OutputTokens,
    Iterations,
    AgentHops,
    Timeout,
}

/// Rate limit types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RateLimitType {
    PerMinute,
    PerHour,
    Burst,
}
```

### Queries (Read Operations)

Queries are pure functions that read state without modifying it.

```rust
/// Queries are pure read operations that don't modify state.
/// They can be cached, parallelized, and are trivially testable.
pub enum KernelQuery {
    GetProcess { pid: String },
    ListProcesses { filter: Option<ProcessFilter> },
    GetProcessCounts,
    CheckQuota { pid: String },
    GetRemainingBudget { pid: String },
    GetSessionState { process_id: String },
    GetNextInstruction { process_id: String },
    GetPendingInterrupt { request_id: String },
    ListServices,
}

#[derive(Debug, Clone)]
pub struct ProcessFilter {
    pub state: Option<ProcessState>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
}
```

---

## Pure Validation Functions

All validation is extracted into pure functions that take immutable references and return `Result<(), ValidationError>`.

```rust
/// Pure validation module - all functions are stateless and side-effect free.
pub mod validation {
    use super::*;

    /// Validate process creation
    pub fn validate_create_process(
        state: &KernelState,
        pid: &str,
        user_id: &str,
        quota: &ResourceQuota,
    ) -> Result<(), ValidationError> {
        // Check PID format
        if pid.is_empty() {
            return Err(ValidationError::EmptyField("pid"));
        }
        if pid.len() > 256 {
            return Err(ValidationError::FieldTooLong("pid", 256));
        }

        // Check for duplicate
        if state.processes.contains_key(pid) {
            return Err(ValidationError::AlreadyExists("process", pid.to_string()));
        }

        // Check rate limit (pure check, doesn't record)
        if state.rate_limiter.would_exceed(user_id) {
            return Err(ValidationError::RateLimitWouldExceed(user_id.to_string()));
        }

        // Validate quota values
        validate_quota(quota)?;

        Ok(())
    }

    /// Validate quota configuration
    pub fn validate_quota(quota: &ResourceQuota) -> Result<(), ValidationError> {
        if quota.max_llm_calls < 0 {
            return Err(ValidationError::InvalidValue("max_llm_calls", "must be non-negative"));
        }
        if quota.max_iterations < 0 {
            return Err(ValidationError::InvalidValue("max_iterations", "must be non-negative"));
        }
        // ... more validations
        Ok(())
    }

    /// Validate state transition
    pub fn validate_state_transition(
        current: ProcessState,
        target: ProcessState,
    ) -> Result<(), ValidationError> {
        if !current.can_transition_to(target) {
            return Err(ValidationError::InvalidStateTransition {
                from: current,
                to: target,
            });
        }
        Ok(())
    }

    /// Check if bounds would be exceeded after operation
    pub fn check_bounds_after_operation(
        envelope: &Envelope,
        additional_llm_calls: i32,
        additional_iterations: i32,
        additional_hops: i32,
    ) -> Option<BoundsExceeded> {
        let new_llm_calls = envelope.llm_call_count + additional_llm_calls;
        if new_llm_calls >= envelope.max_llm_calls {
            return Some(BoundsExceeded::LlmCalls {
                limit: envelope.max_llm_calls,
                would_be: new_llm_calls,
            });
        }

        let new_iterations = envelope.iteration + additional_iterations;
        if new_iterations >= envelope.max_iterations {
            return Some(BoundsExceeded::Iterations {
                limit: envelope.max_iterations,
                would_be: new_iterations,
            });
        }

        let new_hops = envelope.agent_hop_count + additional_hops;
        if new_hops >= envelope.max_agent_hops {
            return Some(BoundsExceeded::AgentHops {
                limit: envelope.max_agent_hops,
                would_be: new_hops,
            });
        }

        None
    }

    /// Validate pipeline configuration
    pub fn validate_pipeline_config(config: &PipelineConfig) -> Result<(), ValidationError> {
        if config.name.is_empty() {
            return Err(ValidationError::EmptyField("pipeline.name"));
        }
        if config.stages.is_empty() {
            return Err(ValidationError::EmptyField("pipeline.stages"));
        }
        if config.max_iterations <= 0 {
            return Err(ValidationError::InvalidValue("max_iterations", "must be positive"));
        }

        // Validate each stage
        for (i, stage) in config.stages.iter().enumerate() {
            if stage.name.is_empty() {
                return Err(ValidationError::EmptyField(&format!("stages[{}].name", i)));
            }
            if stage.agent.is_empty() {
                return Err(ValidationError::EmptyField(&format!("stages[{}].agent", i)));
            }
        }

        Ok(())
    }
}

/// Validation error types
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    #[error("Field '{0}' is required")]
    EmptyField(&'static str),

    #[error("Field '{0}' exceeds maximum length of {1}")]
    FieldTooLong(&'static str, usize),

    #[error("{0} '{1}' already exists")]
    AlreadyExists(&'static str, String),

    #[error("Rate limit would be exceeded for user '{0}'")]
    RateLimitWouldExceed(String),

    #[error("Invalid value for '{0}': {1}")]
    InvalidValue(&'static str, &'static str),

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: ProcessState,
        to: ProcessState,
    },
}

/// Bounds exceeded detail
#[derive(Debug, Clone)]
pub enum BoundsExceeded {
    LlmCalls { limit: i32, would_be: i32 },
    Iterations { limit: i32, would_be: i32 },
    AgentHops { limit: i32, would_be: i32 },
    Timeout { limit_seconds: f64, elapsed: f64 },
}
```

---

## Event Log

The event log provides audit trail and enables time-travel debugging.

```rust
/// Event log for audit trail and replay.
#[derive(Debug)]
pub struct EventLog {
    /// In-memory events (recent)
    events: Vec<KernelEvent>,
    /// Maximum in-memory events before rotation
    max_in_memory: usize,
    /// Optional persistent storage
    persistence: Option<Box<dyn EventPersistence>>,
    /// Subscribers for real-time event streaming
    subscribers: Vec<tokio::sync::mpsc::Sender<KernelEvent>>,
}

impl EventLog {
    pub fn new(max_in_memory: usize) -> Self {
        Self {
            events: Vec::with_capacity(max_in_memory),
            max_in_memory,
            persistence: None,
            subscribers: Vec::new(),
        }
    }

    /// Append an event (the only mutation allowed)
    pub fn append(&mut self, event: KernelEvent) {
        // Notify subscribers
        self.subscribers.retain(|tx| tx.try_send(event.clone()).is_ok());

        // Persist if configured
        if let Some(persistence) = &mut self.persistence {
            if let Err(e) = persistence.persist(&event) {
                tracing::error!("Failed to persist event: {}", e);
            }
        }

        // Add to in-memory log
        self.events.push(event);

        // Rotate if needed
        if self.events.len() > self.max_in_memory {
            self.events.remove(0);
        }
    }

    /// Query events (pure read)
    pub fn query(&self, filter: &EventFilter) -> Vec<&KernelEvent> {
        self.events
            .iter()
            .filter(|e| filter.matches(e))
            .collect()
    }

    /// Get events for a specific process
    pub fn events_for_process(&self, pid: &str) -> Vec<&KernelEvent> {
        self.events
            .iter()
            .filter(|e| e.relates_to_process(pid))
            .collect()
    }

    /// Get events in time range
    pub fn events_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<&KernelEvent> {
        self.events
            .iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .collect()
    }

    /// Subscribe to real-time events
    pub fn subscribe(&mut self) -> tokio::sync::mpsc::Receiver<KernelEvent> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        self.subscribers.push(tx);
        rx
    }

    /// Replay events to rebuild state (for debugging/testing)
    pub fn replay<F>(&self, mut apply: F)
    where
        F: FnMut(&KernelEvent),
    {
        for event in &self.events {
            apply(event);
        }
    }
}

/// Event filter for queries
#[derive(Debug, Default)]
pub struct EventFilter {
    pub process_id: Option<String>,
    pub user_id: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

impl EventFilter {
    pub fn matches(&self, event: &KernelEvent) -> bool {
        if let Some(ref pid) = self.process_id {
            if !event.relates_to_process(pid) {
                return false;
            }
        }
        if let Some(ref since) = self.since {
            if event.timestamp < *since {
                return false;
            }
        }
        if let Some(ref until) = self.until {
            if event.timestamp > *until {
                return false;
            }
        }
        true
    }
}

/// Trait for event persistence
pub trait EventPersistence: Send + Sync {
    fn persist(&mut self, event: &KernelEvent) -> Result<(), Box<dyn std::error::Error>>;
    fn load_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<KernelEvent>, Box<dyn std::error::Error>>;
}
```

---

## Modified Kernel Structure

The kernel now combines mutable state with event emission.

```rust
/// Kernel with hybrid FP architecture.
#[derive(Debug)]
pub struct Kernel {
    // === Mutable State (for performance) ===
    pub lifecycle: LifecycleManager,
    pub resources: ResourceTracker,
    pub rate_limiter: RateLimiter,
    pub interrupts: InterruptService,
    pub services: ServiceRegistry,
    pub orchestrator: Orchestrator,
    pub commbus: CommBus,
    envelopes: HashMap<String, Envelope>,

    // === Event Log (append-only) ===
    event_log: EventLog,

    // === Correlation tracking ===
    current_correlation_id: Option<String>,
}

impl Kernel {
    /// Execute a command (the primary mutation entry point)
    pub fn execute(&mut self, command: KernelCommand) -> Result<CommandResult> {
        // 1. Validate (pure function)
        self.validate_command(&command)?;

        // 2. Execute and collect events
        let events = self.apply_command(command)?;

        // 3. Emit events to log
        for event in &events {
            self.event_log.append(event.clone());
        }

        Ok(CommandResult { events })
    }

    /// Validate a command (pure, no side effects)
    fn validate_command(&self, command: &KernelCommand) -> Result<()> {
        match command {
            KernelCommand::CreateProcess { pid, user_id, quota, .. } => {
                let state = self.as_state_view();
                validation::validate_create_process(
                    &state,
                    pid,
                    user_id,
                    quota.as_ref().unwrap_or(&ResourceQuota::default()),
                )?;
            }
            KernelCommand::ScheduleProcess { pid } => {
                let pcb = self.lifecycle.get(pid)
                    .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?;
                validation::validate_state_transition(pcb.state, ProcessState::Ready)?;
            }
            KernelCommand::InitializeSession { pipeline_config, .. } => {
                validation::validate_pipeline_config(pipeline_config)?;
            }
            // ... other command validations
            _ => {}
        }
        Ok(())
    }

    /// Apply a command and return generated events
    fn apply_command(&mut self, command: KernelCommand) -> Result<Vec<KernelEvent>> {
        let mut events = Vec::new();
        let timestamp = Utc::now();

        match command {
            KernelCommand::CreateProcess {
                pid,
                request_id,
                user_id,
                session_id,
                priority,
                quota,
            } => {
                // Record rate limit
                self.rate_limiter.record_request(&user_id);

                // Create process
                let quota = quota.unwrap_or_default();
                let pcb = self.lifecycle.submit(
                    pid.clone(),
                    request_id.clone(),
                    user_id.clone(),
                    session_id.clone(),
                    priority,
                    Some(quota.clone()),
                )?;

                // Emit event
                events.push(KernelEvent {
                    event_id: Uuid::new_v4(),
                    timestamp,
                    kind: KernelEventKind::ProcessCreated {
                        pid,
                        request_id,
                        user_id,
                        session_id,
                        priority,
                        quota,
                    },
                    correlation_id: self.current_correlation_id.clone(),
                    causation_id: None,
                });
            }

            KernelCommand::ScheduleProcess { pid } => {
                self.lifecycle.schedule(&pid)?;

                events.push(KernelEvent {
                    event_id: Uuid::new_v4(),
                    timestamp,
                    kind: KernelEventKind::ProcessScheduled { pid },
                    correlation_id: self.current_correlation_id.clone(),
                    causation_id: None,
                });
            }

            KernelCommand::RecordUsage {
                pid,
                llm_calls,
                tool_calls,
                tokens_in,
                tokens_out,
            } => {
                let user_id = self.lifecycle.get(&pid)
                    .ok_or_else(|| Error::not_found(format!("Process {} not found", pid)))?
                    .user_id.clone();

                self.resources.record_usage(&user_id, llm_calls, tool_calls, tokens_in, tokens_out);

                events.push(KernelEvent {
                    event_id: Uuid::new_v4(),
                    timestamp,
                    kind: KernelEventKind::UsageRecorded {
                        pid,
                        user_id,
                        llm_calls,
                        tool_calls,
                        tokens_in,
                        tokens_out,
                    },
                    correlation_id: self.current_correlation_id.clone(),
                    causation_id: None,
                });
            }

            // ... implement other commands similarly
            _ => {
                // Placeholder for other commands
            }
        }

        Ok(events)
    }

    /// Query the kernel (pure read, no events)
    pub fn query(&self, query: KernelQuery) -> QueryResult {
        match query {
            KernelQuery::GetProcess { pid } => {
                QueryResult::Process(self.lifecycle.get(&pid).cloned())
            }
            KernelQuery::ListProcesses { filter } => {
                let processes = self.lifecycle.list();
                let filtered = match filter {
                    Some(f) => processes.into_iter()
                        .filter(|p| f.matches(p))
                        .collect(),
                    None => processes,
                };
                QueryResult::ProcessList(filtered)
            }
            KernelQuery::CheckQuota { pid } => {
                match self.lifecycle.get(&pid) {
                    Some(pcb) => QueryResult::QuotaCheck(self.resources.check_quota(pcb)),
                    None => QueryResult::Error(Error::not_found(format!("Process {} not found", pid))),
                }
            }
            KernelQuery::GetNextInstruction { process_id } => {
                // Note: This is a query that may have side effects (updating last_activity_at)
                // In a pure FP model, this would be split into query + command
                QueryResult::Instruction(self.orchestrator.peek_next_instruction(&process_id))
            }
            // ... implement other queries
            _ => QueryResult::Error(Error::internal("Query not implemented".to_string())),
        }
    }

    /// Get a read-only view of kernel state (for pure validation functions)
    fn as_state_view(&self) -> KernelStateView {
        KernelStateView {
            processes: &self.lifecycle.processes,
            rate_limiter: &self.rate_limiter,
            sessions: &self.orchestrator.sessions,
        }
    }

    /// Set correlation ID for tracing
    pub fn with_correlation_id(&mut self, correlation_id: String) -> &mut Self {
        self.current_correlation_id = Some(correlation_id);
        self
    }

    /// Access event log for queries
    pub fn event_log(&self) -> &EventLog {
        &self.event_log
    }
}

/// Read-only state view for pure validation functions
pub struct KernelStateView<'a> {
    pub processes: &'a HashMap<String, ProcessControlBlock>,
    pub rate_limiter: &'a RateLimiter,
    pub sessions: &'a HashMap<String, OrchestrationSession>,
}

/// Result of command execution
#[derive(Debug)]
pub struct CommandResult {
    pub events: Vec<KernelEvent>,
}

/// Result of query execution
#[derive(Debug)]
pub enum QueryResult {
    Process(Option<ProcessControlBlock>),
    ProcessList(Vec<ProcessControlBlock>),
    QuotaCheck(Result<()>),
    Instruction(Result<Instruction, String>),
    Error(Error),
}
```

---

## State Machine with Typestate Pattern

For compile-time safety on state transitions:

```rust
/// Typestate pattern for process lifecycle.
/// Invalid transitions are compile-time errors.
pub mod typestate {
    use std::marker::PhantomData;

    // State markers (zero-sized types)
    pub struct New;
    pub struct Ready;
    pub struct Running;
    pub struct Waiting;
    pub struct Blocked;
    pub struct Terminated;
    pub struct Zombie;

    /// Process with state encoded in type system
    #[derive(Debug)]
    pub struct Process<S> {
        pub pid: String,
        pub request_id: String,
        pub user_id: String,
        pub session_id: String,
        pub quota: ResourceQuota,
        pub usage: ResourceUsage,
        _state: PhantomData<S>,
    }

    impl Process<New> {
        pub fn new(
            pid: String,
            request_id: String,
            user_id: String,
            session_id: String,
            quota: ResourceQuota,
        ) -> Self {
            Self {
                pid,
                request_id,
                user_id,
                session_id,
                quota,
                usage: ResourceUsage::default(),
                _state: PhantomData,
            }
        }

        /// NEW → READY (only valid transition from New)
        pub fn schedule(self) -> Process<Ready> {
            Process {
                pid: self.pid,
                request_id: self.request_id,
                user_id: self.user_id,
                session_id: self.session_id,
                quota: self.quota,
                usage: self.usage,
                _state: PhantomData,
            }
        }
    }

    impl Process<Ready> {
        /// READY → RUNNING
        pub fn start(self) -> Process<Running> {
            Process {
                pid: self.pid,
                request_id: self.request_id,
                user_id: self.user_id,
                session_id: self.session_id,
                quota: self.quota,
                usage: self.usage,
                _state: PhantomData,
            }
        }
    }

    impl Process<Running> {
        /// RUNNING → WAITING
        pub fn wait(self) -> Process<Waiting> {
            Process {
                pid: self.pid,
                request_id: self.request_id,
                user_id: self.user_id,
                session_id: self.session_id,
                quota: self.quota,
                usage: self.usage,
                _state: PhantomData,
            }
        }

        /// RUNNING → BLOCKED
        pub fn block(self) -> Process<Blocked> {
            Process {
                pid: self.pid,
                request_id: self.request_id,
                user_id: self.user_id,
                session_id: self.session_id,
                quota: self.quota,
                usage: self.usage,
                _state: PhantomData,
            }
        }

        /// RUNNING → TERMINATED
        pub fn terminate(self) -> Process<Terminated> {
            Process {
                pid: self.pid,
                request_id: self.request_id,
                user_id: self.user_id,
                session_id: self.session_id,
                quota: self.quota,
                usage: self.usage,
                _state: PhantomData,
            }
        }
    }

    impl Process<Waiting> {
        /// WAITING → READY
        pub fn resume(self) -> Process<Ready> {
            Process {
                pid: self.pid,
                request_id: self.request_id,
                user_id: self.user_id,
                session_id: self.session_id,
                quota: self.quota,
                usage: self.usage,
                _state: PhantomData,
            }
        }
    }

    impl Process<Blocked> {
        /// BLOCKED → READY
        pub fn resume(self) -> Process<Ready> {
            Process {
                pid: self.pid,
                request_id: self.request_id,
                user_id: self.user_id,
                session_id: self.session_id,
                quota: self.quota,
                usage: self.usage,
                _state: PhantomData,
            }
        }
    }

    impl Process<Terminated> {
        /// TERMINATED → ZOMBIE
        pub fn cleanup(self) -> Process<Zombie> {
            Process {
                pid: self.pid,
                request_id: self.request_id,
                user_id: self.user_id,
                session_id: self.session_id,
                quota: self.quota,
                usage: self.usage,
                _state: PhantomData,
            }
        }
    }

    // Compile-time prevention of invalid transitions:
    //
    // let new_proc = Process::<New>::new(...);
    // let ready = new_proc.schedule();  // OK
    // let running = ready.start();       // OK
    //
    // let new_proc = Process::<New>::new(...);
    // let running = new_proc.start();    // COMPILE ERROR: no method `start` for Process<New>
}
```

---

## Advantages of Hybrid Architecture

### 1. Auditability

```rust
// Query: "What happened to process X?"
let events = kernel.event_log().events_for_process("process-123");
for event in events {
    println!("{}: {:?}", event.timestamp, event.kind);
}

// Output:
// 2024-01-15T10:00:00Z: ProcessCreated { pid: "process-123", ... }
// 2024-01-15T10:00:01Z: ProcessScheduled { pid: "process-123" }
// 2024-01-15T10:00:02Z: ProcessStarted { pid: "process-123", ... }
// 2024-01-15T10:00:05Z: UsageRecorded { pid: "process-123", llm_calls: 3, ... }
// 2024-01-15T10:00:10Z: QuotaExceeded { pid: "process-123", resource: LlmCalls, ... }
// 2024-01-15T10:00:10Z: ProcessTerminated { pid: "process-123", reason: MaxLlmCallsExceeded }
```

### 2. Testability

```rust
#[test]
fn test_validate_create_process_duplicate() {
    // Pure function test - no kernel needed
    let mut state = KernelStateView::mock();
    state.processes.insert("pid-1".to_string(), ProcessControlBlock::mock());

    let result = validation::validate_create_process(
        &state,
        "pid-1",  // Duplicate!
        "user-1",
        &ResourceQuota::default(),
    );

    assert!(matches!(result, Err(ValidationError::AlreadyExists(_, _))));
}

#[test]
fn test_bounds_check_pure() {
    let envelope = Envelope {
        llm_call_count: 48,
        max_llm_calls: 50,
        ..Default::default()
    };

    // Would adding 3 more LLM calls exceed bounds?
    let result = validation::check_bounds_after_operation(&envelope, 3, 0, 0);

    assert!(matches!(result, Some(BoundsExceeded::LlmCalls { .. })));
}
```

### 3. Debugging

```rust
// Time-travel debugging: replay to find bug
let events = kernel.event_log().events_in_range(
    start_of_incident,
    end_of_incident,
);

// Replay to reconstruct state
let mut debug_state = KernelState::empty();
for event in events {
    debug_state = apply_event(debug_state, event);
    if debug_state.has_bug_condition() {
        println!("Bug introduced by event: {:?}", event);
        break;
    }
}
```

### 4. Observability

```rust
// Real-time event streaming
let mut subscriber = kernel.event_log().subscribe();

tokio::spawn(async move {
    while let Some(event) = subscriber.recv().await {
        // Send to metrics system
        metrics::record_event(&event);

        // Send to logging
        if event.is_error() {
            tracing::error!("Kernel event: {:?}", event);
        }

        // Send to external audit system
        audit_system::record(event).await;
    }
});
```

### 5. Performance (Kept)

```rust
// Still O(1) process lookup
let pcb = kernel.lifecycle.get("pid-123");

// Still O(log n) scheduling
let next = kernel.lifecycle.get_next_runnable();

// No copying - mutable state
kernel.lifecycle.start("pid-123")?;
```

---

## Trade-offs

| Aspect | Cost | Benefit |
|--------|------|---------|
| Memory | Event log uses memory | Full audit trail |
| Complexity | More types (Command, Event, Query) | Clearer separation of concerns |
| Learning curve | FP concepts to learn | Better correctness guarantees |
| Code size | ~20% more code | Much better testability |
| Performance | Event emission overhead (~μs) | Negligible for our scale |

---

## Migration Path

### Phase 1: Add Event Log (Non-breaking)
- Add `EventLog` to `Kernel`
- Emit events from existing methods
- No changes to gRPC interface

### Phase 2: Extract Pure Validation
- Create `validation` module
- Move validation logic to pure functions
- Keep existing methods as wrappers

### Phase 3: Add Command/Query Types (Optional)
- Define `KernelCommand` enum
- Define `KernelQuery` enum
- Add `execute()` and `query()` methods
- Gradually migrate gRPC handlers

### Phase 4: Typestate for New Code (Optional)
- Use typestate pattern for new features
- Keep existing code working
- Gradually adopt where beneficial

---

## Conclusion

This hybrid architecture provides:

1. **Rust performance** - Mutable state, efficient data structures
2. **FP auditability** - Event sourcing, immutable log
3. **FP testability** - Pure validation functions
4. **FP correctness** - Explicit state transitions, typestate
5. **Pragmatic adoption** - Incremental migration, non-breaking changes

The key insight is that you don't have to choose between FP and Rust idioms. You can have **mutable state for performance** while **emitting immutable events for audit** and using **pure functions for validation**.
