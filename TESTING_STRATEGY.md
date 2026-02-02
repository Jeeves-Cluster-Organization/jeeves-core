# Testing Strategy - Go → Rust Parity
**Date**: 2026-02-02
**Status**: Test migration plan

---

## Go Test Coverage Analysis

### Test Inventory (10,213 LOC)

| Module | File | Test Functions | Focus Area |
|--------|------|---------------|------------|
| **Kernel** | kernel_test.go | 158 | ResourceTracker, LifecycleManager, RateLimiter, Process management |
| **Kernel** | orchestrator_test.go | 58 | Pipeline orchestration, instruction generation, bounds checking |
| **Kernel** | cleanup_test.go | 11 | Process cleanup, resource deallocation |
| **Kernel** | recovery_test.go | 14 | Panic recovery, error handling |
| **Kernel** | benchmark_test.go | 0 (benchmarks) | Performance benchmarks |
| **gRPC** | kernel_server_test.go | ~30 | KernelService RPCs |
| **gRPC** | orchestration_server_test.go | ~25 | OrchestrationService RPCs |
| **gRPC** | commbus_server_test.go | ~20 | CommBusService RPCs |
| **gRPC** | server_integration_test.go | ~15 | End-to-end service tests |
| **gRPC** | interceptors_test.go | ~10 | gRPC middleware |
| **gRPC** | validation_test.go | ~8 | Request validation |

**Total**: ~349 test functions across all modules

### Key Test Patterns

1. **Unit Tests**: Test individual functions (ResourceTracker, RateLimiter)
2. **Integration Tests**: Test subsystem interactions (Kernel ↔ Lifecycle)
3. **gRPC Tests**: Test RPC handlers with mock clients
4. **Concurrency Tests**: Test with multiple goroutines (stress tests)
5. **Error Path Tests**: Test error handling and recovery

---

## Rust Test Strategy

### Phase 1: Unit Tests (Priority 1 - Critical)

#### Resource Tracking Tests
**File**: `src/kernel/resources.rs`
**Coverage Target**: 90%

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_tracker_new() {
        let tracker = ResourceTracker::new();
        assert_eq!(tracker.total_llm_calls, 0);
        assert_eq!(tracker.total_tokens_in, 0);
    }

    #[test]
    fn test_record_usage() {
        let mut tracker = ResourceTracker::new();
        tracker.record_usage("user1", 5, 10, 1000, 500);

        assert_eq!(tracker.total_llm_calls, 5);
        assert_eq!(tracker.total_tool_calls, 10);
        assert_eq!(tracker.total_tokens_in, 1000);
        assert_eq!(tracker.total_tokens_out, 500);
    }

    #[test]
    fn test_quota_enforcement() {
        let pcb = ProcessControlBlock {
            quota: ResourceQuota {
                max_llm_calls: 10,
                max_input_tokens: 1000,
                ..Default::default()
            },
            usage: ResourceUsage {
                llm_calls: 5,
                tokens_in: 500,
                ..Default::default()
            },
            ..Default::default()
        };

        let tracker = ResourceTracker::new();
        assert!(tracker.check_quota(&pcb).is_ok());

        // Exceed quota
        let pcb_exceeded = ProcessControlBlock {
            usage: ResourceUsage {
                llm_calls: 15, // Over limit
                ..pcb.usage
            },
            ..pcb
        };
        assert!(tracker.check_quota(&pcb_exceeded).is_err());
    }
}
```

#### Lifecycle Manager Tests
**File**: `src/kernel/lifecycle.rs`
**Coverage Target**: 95%

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submit_process() {
        let mut lifecycle = LifecycleManager::default();
        let pcb = lifecycle.submit(
            "env1".to_string(),
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            SchedulingPriority::Normal,
            None,
        ).unwrap();

        assert_eq!(pcb.state, ProcessState::New);
        assert_eq!(pcb.user_id, "user1");
    }

    #[test]
    fn test_priority_scheduling() {
        let mut lifecycle = LifecycleManager::default();

        // Submit high priority
        lifecycle.submit(
            "env1".to_string(), "req1".to_string(),
            "user1".to_string(), "sess1".to_string(),
            SchedulingPriority::High, None,
        ).unwrap();
        lifecycle.schedule("env1").unwrap();

        // Submit normal priority
        lifecycle.submit(
            "env2".to_string(), "req2".to_string(),
            "user2".to_string(), "sess2".to_string(),
            SchedulingPriority::Normal, None,
        ).unwrap();
        lifecycle.schedule("env2").unwrap();

        // High priority should come first
        let next = lifecycle.get_next_runnable().unwrap();
        assert_eq!(next.pid, "env1");
    }

    #[test]
    fn test_state_transitions() {
        let mut lifecycle = LifecycleManager::default();
        lifecycle.submit(
            "env1".to_string(), "req1".to_string(),
            "user1".to_string(), "sess1".to_string(),
            SchedulingPriority::Normal, None,
        ).unwrap();

        lifecycle.schedule("env1").unwrap();
        assert_eq!(lifecycle.get("env1").unwrap().state, ProcessState::Ready);

        lifecycle.start("env1").unwrap();
        assert_eq!(lifecycle.get("env1").unwrap().state, ProcessState::Running);

        lifecycle.terminate("env1").unwrap();
        assert_eq!(lifecycle.get("env1").unwrap().state, ProcessState::Terminated);
    }
}
```

#### Rate Limiter Tests
**File**: `src/kernel/rate_limiter.rs`
**Coverage Target**: 90%

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_rate_limit_allows() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            requests_per_hour: 1000,
            requests_per_day: 10000,
            burst_size: 10,
        };
        let mut limiter = RateLimiter::new(Some(config));

        // First request should succeed
        assert!(limiter.check_rate_limit("user1").is_ok());
    }

    #[test]
    fn test_rate_limit_burst() {
        let config = RateLimitConfig {
            requests_per_minute: 60,
            burst_size: 5,
            ..Default::default()
        };
        let mut limiter = RateLimiter::new(Some(config));

        // First 5 requests (burst) should succeed
        for i in 0..5 {
            assert!(limiter.check_rate_limit("user1").is_ok(), "Request {} failed", i);
        }

        // 6th should fail (burst exhausted)
        assert!(limiter.check_rate_limit("user1").is_err());
    }

    #[test]
    fn test_rate_limit_window_expiry() {
        let config = RateLimitConfig {
            requests_per_minute: 2,
            burst_size: 2,
            ..Default::default()
        };
        let mut limiter = RateLimiter::new(Some(config));

        // Use up burst
        limiter.check_rate_limit("user1").unwrap();
        limiter.check_rate_limit("user1").unwrap();
        assert!(limiter.check_rate_limit("user1").is_err());

        // Wait for window to expire
        sleep(Duration::from_secs(61));

        // Should succeed again
        assert!(limiter.check_rate_limit("user1").is_ok());
    }
}
```

### Phase 2: Subsystem Integration Tests (Priority 2)

#### Kernel Integration Tests
**File**: `src/kernel/mod.rs`

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_kernel_create_and_schedule_process() {
        let mut kernel = Kernel::new();

        let pcb = kernel.create_process(
            "env1".to_string(),
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            SchedulingPriority::Normal,
            None,
        ).unwrap();

        assert_eq!(pcb.state, ProcessState::Ready);

        let next = kernel.get_next_runnable().unwrap();
        assert_eq!(next.pid, "env1");
    }

    #[test]
    fn test_kernel_quota_enforcement() {
        let quota = ResourceQuota {
            max_llm_calls: 5,
            ..Default::default()
        };

        let mut kernel = Kernel::with_config(Some(quota), None);

        kernel.create_process(
            "env1".to_string(),
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            SchedulingPriority::Normal,
            None,
        ).unwrap();

        // Record usage up to limit
        for _ in 0..5 {
            kernel.record_usage("user1", 1, 0, 0, 0);
        }

        // Should fail quota check
        assert!(kernel.check_quota("env1").is_err());
    }

    #[test]
    fn test_kernel_interrupt_flow() {
        let mut kernel = Kernel::new();

        let interrupt = kernel.create_interrupt(
            InterruptKind::Clarification,
            "req1".to_string(),
            "user1".to_string(),
            "sess1".to_string(),
            "env1".to_string(),
            Some("What should I do?".to_string()),
            None,
            None,
        );

        assert_eq!(interrupt.status, InterruptStatus::Pending);

        let response = InterruptResponse {
            text: "Do this".to_string(),
            approved: true,
            ..Default::default()
        };

        let resolved = kernel.resolve_interrupt(&interrupt.flow_interrupt.id, response, Some("user1"));
        assert!(resolved);
    }
}
```

### Phase 3: gRPC Service Tests (Priority 3)

#### KernelService Tests
**File**: `tests/grpc_kernel_service.rs` (new)

```rust
use jeeves_core::grpc::KernelServiceImpl;
use jeeves_core::kernel::Kernel;
use jeeves_core::proto::*;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Request;

#[tokio::test]
async fn test_create_process_rpc() {
    let kernel = Arc::new(Mutex::new(Kernel::new()));
    let service = KernelServiceImpl::new(kernel);

    let request = Request::new(CreateProcessRequest {
        pid: "env1".to_string(),
        request_id: "req1".to_string(),
        user_id: "user1".to_string(),
        session_id: "sess1".to_string(),
        priority: SchedulingPriority::Normal as i32,
        quota: None,
    });

    let response = service.create_process(request).await.unwrap();
    let pcb = response.into_inner();

    assert_eq!(pcb.pid, "env1");
    assert_eq!(pcb.state, ProcessState::Ready as i32);
}

#[tokio::test]
async fn test_get_next_runnable_rpc() {
    let kernel = Arc::new(Mutex::new(Kernel::new()));
    let service = KernelServiceImpl::new(kernel.clone());

    // Create process first
    let create_req = Request::new(CreateProcessRequest {
        pid: "env1".to_string(),
        request_id: "req1".to_string(),
        user_id: "user1".to_string(),
        session_id: "sess1".to_string(),
        priority: SchedulingPriority::Normal as i32,
        quota: None,
    });
    service.create_process(create_req).await.unwrap();

    // Get next runnable
    let request = Request::new(GetNextRunnableRequest {});
    let response = service.get_next_runnable(request).await.unwrap();
    let pcb = response.into_inner();

    assert_eq!(pcb.pid, "env1");
}
```

---

## Rust Code Coverage Setup

### Using Tarpaulin (Recommended)

**Install**:
```bash
cargo install cargo-tarpaulin
```

**Run**:
```bash
cargo tarpaulin --out Html --output-dir coverage
```

**CI Integration** (`.github/workflows/test.yml`):
```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run tests with coverage
        run: |
          cargo install cargo-tarpaulin
          cargo tarpaulin --out Xml --out Html
      - name: Upload coverage
        uses: codecov/codecov-action@v2
```

### Using llvm-cov (Alternative)

**Install**:
```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
```

**Run**:
```bash
cargo llvm-cov --html --open
```

---

## Testing Roadmap

### Immediate (Next Session - 2 hours)
1. Create `src/kernel/resources.rs` tests (30 min)
2. Create `src/kernel/lifecycle.rs` tests (45 min)
3. Create `src/kernel/rate_limiter.rs` tests (30 min)
4. Run `cargo test` - verify all pass (15 min)

### Short Term (Post-Cutover - 4 hours)
5. Create `src/kernel/interrupts.rs` tests (1 hour)
6. Create `src/kernel/orchestrator.rs` tests (1 hour)
7. Create kernel integration tests (1 hour)
8. Create gRPC service tests (1 hour)

### Coverage Targets

| Module | Target | Priority |
|--------|--------|----------|
| `kernel/resources.rs` | 90% | P0 |
| `kernel/lifecycle.rs` | 95% | P0 |
| `kernel/rate_limiter.rs` | 90% | P0 |
| `kernel/interrupts.rs` | 85% | P1 |
| `kernel/orchestrator.rs` | 85% | P1 |
| `kernel/services.rs` | 80% | P2 |
| `grpc/*` | 70% | P2 |
| **Overall** | **85%** | Target |

---

## Test Execution Plan

### Baseline (Pre-Cutover)
```bash
# Create basic tests (2 hours)
vim src/kernel/resources.rs  # Add #[cfg(test)] mod
vim src/kernel/lifecycle.rs  # Add #[cfg(test)] mod
vim src/kernel/rate_limiter.rs  # Add #[cfg(test)] mod

# Run tests
cargo test

# Expected: ~30 tests passing

# Install coverage tool
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin --out Html
```

**Success Criteria**:
- All tests pass
- Coverage > 60% on critical modules
- Zero panics or unwraps in test execution

### Post-Cutover Validation
```bash
# Full test suite (4 hours work)
cargo test --all-features
cargo tarpaulin --out Html --output-dir coverage

# Benchmark comparison (vs Go)
cargo bench
```

**Success Criteria**:
- Coverage > 85% overall
- All critical paths tested
- Performance within 10% of Go

---

## Test Prioritization

### Must-Have (Blocker for Go deletion)
- [x] Build succeeds (binary compiles)
- [ ] ResourceTracker tests
- [ ] LifecycleManager tests
- [ ] RateLimiter tests
- [ ] Basic smoke test (server starts)

### Should-Have (Pre-production)
- [ ] Interrupt tests
- [ ] Orchestrator tests
- [ ] Kernel integration tests
- [ ] gRPC service tests

### Nice-to-Have (Post-production)
- [ ] Benchmark tests
- [ ] Stress tests
- [ ] Fuzz tests
- [ ] Property-based tests (proptest)

---

## Comparison: Go vs Rust Testing

| Aspect | Go | Rust |
|--------|-----|------|
| **Test Framework** | testing package | Built-in `#[test]` |
| **Async Tests** | goroutines | `#[tokio::test]` |
| **Mocking** | interfaces | trait objects / mockall |
| **Coverage Tool** | go test -cover | cargo-tarpaulin |
| **Benchmarks** | testing.B | criterion |
| **Property Tests** | gopter | proptest |
| **Test LOC** | ~10,213 | ~3,000 (target) |

**Rust Advantage**: Compile-time safety means fewer tests needed for type errors.

---

## Decision: Test Now or Test Later?

### Option A: Test Before Cutover (Recommended)
**Time**: 2 hours
**Coverage**: 60% on critical modules
**Risk**: LOW - Validates parity before deleting Go

### Option B: Test After Cutover
**Time**: Save 2 hours now
**Coverage**: Defer to post-cutover
**Risk**: MEDIUM - May discover issues after Go deleted

### Recommendation: **Option A** (2-hour delay, high confidence)

1. Add tests to 3 critical modules (resources, lifecycle, rate_limiter)
2. Run `cargo test` - verify ~30 tests pass
3. Run basic coverage - target 60%+
4. THEN delete Go with confidence

---

## Summary

- **Go Tests**: 349 functions, ~10,213 LOC
- **Rust Tests**: Need ~3,000 LOC (30% of Go due to type safety)
- **Coverage Target**: 85% overall
- **Time to Baseline**: 2 hours (critical modules only)
- **Time to Full Parity**: 6 hours (all modules + integration)

**Next Action**: Create tests for ResourceTracker, LifecycleManager, RateLimiter (~2 hours), then proceed with cutover.
