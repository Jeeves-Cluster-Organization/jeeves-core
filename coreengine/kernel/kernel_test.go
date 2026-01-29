package kernel

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// =============================================================================
// Test Logger
// =============================================================================

type testLogger struct {
	logs []string
	mu   sync.Mutex
}

func (l *testLogger) Debug(msg string, keysAndValues ...any) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.logs = append(l.logs, "DEBUG: "+msg)
}

func (l *testLogger) Info(msg string, keysAndValues ...any) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.logs = append(l.logs, "INFO: "+msg)
}

func (l *testLogger) Warn(msg string, keysAndValues ...any) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.logs = append(l.logs, "WARN: "+msg)
}

func (l *testLogger) Error(msg string, keysAndValues ...any) {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.logs = append(l.logs, "ERROR: "+msg)
}

// =============================================================================
// ResourceTracker Tests
// =============================================================================

func TestResourceTracker_Allocate(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)

	// Test successful allocation
	if !tracker.Allocate("pid-1", nil) {
		t.Error("first allocation should succeed")
	}

	// Test duplicate allocation
	if tracker.Allocate("pid-1", nil) {
		t.Error("duplicate allocation should fail")
	}

	// Verify tracking
	if !tracker.IsTracked("pid-1") {
		t.Error("pid-1 should be tracked")
	}
}

func TestResourceTracker_RecordUsage(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	// Record usage
	usage := tracker.RecordUsage("pid-1", 1, 2, 1, 100, 50)

	if usage.LLMCalls != 1 {
		t.Errorf("expected 1 LLM call, got %d", usage.LLMCalls)
	}
	if usage.ToolCalls != 2 {
		t.Errorf("expected 2 tool calls, got %d", usage.ToolCalls)
	}
	if usage.AgentHops != 1 {
		t.Errorf("expected 1 agent hop, got %d", usage.AgentHops)
	}
	if usage.TokensIn != 100 {
		t.Errorf("expected 100 tokens in, got %d", usage.TokensIn)
	}
	if usage.TokensOut != 50 {
		t.Errorf("expected 50 tokens out, got %d", usage.TokensOut)
	}

	// Verify system usage
	sysUsage := tracker.GetSystemUsage()
	if sysUsage.SystemLLMCalls != 1 {
		t.Errorf("expected system LLM calls 1, got %d", sysUsage.SystemLLMCalls)
	}
}

func TestResourceTracker_CheckQuota(t *testing.T) {
	quota := &ResourceQuota{
		MaxLLMCalls:    2,
		MaxToolCalls:   5,
		TimeoutSeconds: 3600, // 1 hour - prevent timeout in unit tests
	}
	tracker := NewResourceTracker(quota, nil)
	tracker.Allocate("pid-1", quota)

	// Within quota
	tracker.RecordUsage("pid-1", 1, 0, 0, 0, 0)
	if exceeded := tracker.CheckQuota("pid-1"); exceeded != "" {
		t.Errorf("should be within quota, got: %s", exceeded)
	}

	// Exceed quota
	tracker.RecordUsage("pid-1", 2, 0, 0, 0, 0)
	if exceeded := tracker.CheckQuota("pid-1"); exceeded != "max_llm_calls_exceeded" {
		t.Errorf("expected max_llm_calls_exceeded, got: %s", exceeded)
	}
}

func TestResourceTracker_Release(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	if !tracker.Release("pid-1") {
		t.Error("release should succeed")
	}

	if tracker.IsTracked("pid-1") {
		t.Error("pid-1 should not be tracked after release")
	}

	if tracker.Release("pid-1") {
		t.Error("second release should fail")
	}
}

func TestResourceTracker_GetRemainingBudget(t *testing.T) {
	quota := &ResourceQuota{
		MaxLLMCalls:    10,
		MaxToolCalls:   20,
		TimeoutSeconds: 300,
	}
	tracker := NewResourceTracker(quota, nil)
	tracker.Allocate("pid-1", quota)
	tracker.RecordUsage("pid-1", 3, 5, 0, 0, 0)

	budget := tracker.GetRemainingBudget("pid-1")
	if budget == nil {
		t.Fatal("budget should not be nil")
	}
	if budget.LLMCalls != 7 {
		t.Errorf("expected 7 remaining LLM calls, got %d", budget.LLMCalls)
	}
	if budget.ToolCalls != 15 {
		t.Errorf("expected 15 remaining tool calls, got %d", budget.ToolCalls)
	}
}

// =============================================================================
// InterruptService Tests
// =============================================================================

func TestInterruptService_CreateInterrupt(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
		WithInterruptQuestion("What file?"),
	)

	if interrupt == nil {
		t.Fatal("interrupt should not be nil")
	}
	if interrupt.Kind != envelope.InterruptKindClarification {
		t.Errorf("expected clarification kind, got %s", interrupt.Kind)
	}
	if interrupt.Question != "What file?" {
		t.Errorf("expected question 'What file?', got %s", interrupt.Question)
	}
	if interrupt.Status != InterruptStatusPending {
		t.Errorf("expected pending status, got %s", interrupt.Status)
	}
}

func TestInterruptService_Resolve(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)

	// Resolve with correct user
	response := &envelope.InterruptResponse{
		Text: strPtr("main.py"),
	}
	resolved := service.Resolve(interrupt.ID, response, "user-1")

	if resolved == nil {
		t.Fatal("resolved interrupt should not be nil")
	}
	if resolved.Status != InterruptStatusResolved {
		t.Errorf("expected resolved status, got %s", resolved.Status)
	}
	if resolved.Response == nil || *resolved.Response.Text != "main.py" {
		t.Error("response text should be 'main.py'")
	}
}

func TestInterruptService_ResolveWrongUser(t *testing.T) {
	logger := &testLogger{}
	service := NewInterruptService(logger, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)

	// Try to resolve with wrong user
	resolved := service.Resolve(interrupt.ID, nil, "user-2")

	if resolved != nil {
		t.Error("resolve should fail with wrong user")
	}
}

func TestInterruptService_GetPendingForRequest(t *testing.T) {
	service := NewInterruptService(nil, nil)

	// Create interrupts
	service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)
	interrupt2 := service.CreateInterrupt(
		envelope.InterruptKindConfirmation,
		"req-1", "user-1", "sess-1", "env-1",
	)

	// Get pending - should return most recent
	pending := service.GetPendingForRequest("req-1")
	if pending == nil {
		t.Fatal("pending should not be nil")
	}
	if pending.ID != interrupt2.ID {
		t.Error("should return most recent pending interrupt")
	}
}

func TestInterruptService_Cancel(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)

	cancelled := service.Cancel(interrupt.ID, "user requested")

	if cancelled == nil {
		t.Fatal("cancelled interrupt should not be nil")
	}
	if cancelled.Status != InterruptStatusCancelled {
		t.Errorf("expected cancelled status, got %s", cancelled.Status)
	}
}

func TestInterruptService_ExpirePending(t *testing.T) {
	// Create service with short TTL config
	configs := map[envelope.InterruptKind]*InterruptConfig{
		envelope.InterruptKindClarification: {
			DefaultTTL: 1 * time.Millisecond,
			AutoExpire: true,
		},
	}
	service := NewInterruptService(nil, configs)

	service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)

	// Wait for expiry
	time.Sleep(5 * time.Millisecond)

	// Expire pending
	count := service.ExpirePending()
	if count != 1 {
		t.Errorf("expected 1 expired, got %d", count)
	}
}

// =============================================================================
// ServiceRegistry Tests
// =============================================================================

func TestServiceRegistry_RegisterService(t *testing.T) {
	registry := NewServiceRegistry(nil)

	info := NewServiceInfo("test-service", "flow")
	info.Capabilities = []string{"execute"}

	// First registration should succeed
	if !registry.RegisterService(info) {
		t.Error("first registration should succeed")
	}

	// Duplicate registration should fail
	if registry.RegisterService(info) {
		t.Error("duplicate registration should fail")
	}
}

func TestServiceRegistry_ListServices(t *testing.T) {
	registry := NewServiceRegistry(nil)

	registry.RegisterService(NewServiceInfo("flow-1", "flow"))
	registry.RegisterService(NewServiceInfo("flow-2", "flow"))
	registry.RegisterService(NewServiceInfo("worker-1", "worker"))

	// List all
	all := registry.ListServices("", false)
	if len(all) != 3 {
		t.Errorf("expected 3 services, got %d", len(all))
	}

	// List by type
	flows := registry.ListServices("flow", false)
	if len(flows) != 2 {
		t.Errorf("expected 2 flow services, got %d", len(flows))
	}
}

func TestServiceRegistry_Health(t *testing.T) {
	registry := NewServiceRegistry(nil)

	info := NewServiceInfo("test-service", "flow")
	registry.RegisterService(info)

	// Mark unhealthy
	registry.UpdateHealth("test-service", ServiceStatusUnhealthy)

	// List healthy only
	healthy := registry.ListServices("", true)
	if len(healthy) != 0 {
		t.Errorf("expected 0 healthy services, got %d", len(healthy))
	}

	// Get healthy count
	if registry.GetHealthyCount() != 0 {
		t.Error("healthy count should be 0")
	}
}

func TestServiceRegistry_Load(t *testing.T) {
	registry := NewServiceRegistry(nil)

	info := NewServiceInfo("test-service", "flow")
	info.MaxConcurrent = 2
	registry.RegisterService(info)

	// Increment load
	if !registry.IncrementLoad("test-service") {
		t.Error("first increment should succeed")
	}
	if !registry.IncrementLoad("test-service") {
		t.Error("second increment should succeed")
	}
	if registry.IncrementLoad("test-service") {
		t.Error("third increment should fail (at capacity)")
	}

	// Decrement load
	registry.DecrementLoad("test-service")
	if registry.GetLoad("test-service") != 1 {
		t.Error("load should be 1 after decrement")
	}
}

func TestServiceRegistry_Dispatch(t *testing.T) {
	registry := NewServiceRegistry(nil)

	info := NewServiceInfo("test-service", "flow")
	registry.RegisterService(info)

	// Register handler
	handlerCalled := false
	registry.RegisterHandler("test-service", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		handlerCalled = true
		return &DispatchResult{
			Success: true,
			Data:    map[string]any{"result": "ok"},
		}, nil
	})

	target := &DispatchTarget{
		ServiceName:    "test-service",
		Method:         "execute",
		TimeoutSeconds: 10,
	}

	result := registry.Dispatch(context.Background(), target, nil)

	if !result.Success {
		t.Errorf("dispatch should succeed, got error: %s", result.Error)
	}
	if !handlerCalled {
		t.Error("handler should have been called")
	}
}

func TestServiceRegistry_DispatchUnknownService(t *testing.T) {
	registry := NewServiceRegistry(nil)

	target := &DispatchTarget{
		ServiceName: "unknown-service",
	}

	result := registry.Dispatch(context.Background(), target, nil)

	if result.Success {
		t.Error("dispatch to unknown service should fail")
	}
	if result.Error == "" {
		t.Error("error message should be set")
	}
}

// =============================================================================
// LifecycleManager Tests
// =============================================================================

func TestLifecycleManager_Submit(t *testing.T) {
	lm := NewLifecycleManager(nil)

	pcb, err := lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	if err != nil {
		t.Fatalf("submit failed: %v", err)
	}

	if pcb.State != ProcessStateNew {
		t.Errorf("expected NEW state, got %s", pcb.State)
	}
	if pcb.Priority != PriorityNormal {
		t.Errorf("expected NORMAL priority, got %s", pcb.Priority)
	}
}

func TestLifecycleManager_Schedule(t *testing.T) {
	lm := NewLifecycleManager(nil)
	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)

	if err := lm.Schedule("pid-1"); err != nil {
		t.Fatalf("schedule failed: %v", err)
	}

	pcb := lm.GetProcess("pid-1")
	if pcb.State != ProcessStateReady {
		t.Errorf("expected READY state, got %s", pcb.State)
	}
}

func TestLifecycleManager_GetNextRunnable(t *testing.T) {
	lm := NewLifecycleManager(nil)

	// Submit and schedule processes with different priorities
	lm.Submit("pid-low", "req-1", "user-1", "sess-1", PriorityLow, nil)
	lm.Submit("pid-high", "req-2", "user-1", "sess-1", PriorityHigh, nil)
	lm.Submit("pid-normal", "req-3", "user-1", "sess-1", PriorityNormal, nil)

	lm.Schedule("pid-low")
	lm.Schedule("pid-high")
	lm.Schedule("pid-normal")

	// Should get high priority first
	pcb := lm.GetNextRunnable()
	if pcb == nil {
		t.Fatal("should get a runnable process")
	}
	if pcb.PID != "pid-high" {
		t.Errorf("expected pid-high, got %s", pcb.PID)
	}
	if pcb.State != ProcessStateRunning {
		t.Errorf("expected RUNNING state, got %s", pcb.State)
	}
}

func TestLifecycleManager_Terminate(t *testing.T) {
	lm := NewLifecycleManager(nil)
	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Schedule("pid-1")

	// Get runnable to transition to RUNNING
	lm.GetNextRunnable()

	// Terminate with force
	if err := lm.Terminate("pid-1", "test", true); err != nil {
		t.Fatalf("terminate failed: %v", err)
	}

	pcb := lm.GetProcess("pid-1")
	if pcb.State != ProcessStateTerminated {
		t.Errorf("expected TERMINATED state, got %s", pcb.State)
	}
}

// =============================================================================
// RateLimiter Tests
// =============================================================================

func TestRateLimiter_CheckRateLimit(t *testing.T) {
	config := &RateLimitConfig{
		RequestsPerMinute: 2,
		RequestsPerHour:   100,
		RequestsPerDay:    1000,
	}
	rl := NewRateLimiter(config)

	// First two should pass
	for i := 0; i < 2; i++ {
		result := rl.CheckRateLimit("user-1", "", true)
		if !result.Allowed {
			t.Errorf("request %d should be allowed", i+1)
		}
	}

	// Third should fail
	result := rl.CheckRateLimit("user-1", "", true)
	if result.Allowed {
		t.Error("third request should be rate limited")
	}
	if result.LimitType != "minute" {
		t.Errorf("expected minute limit, got %s", result.LimitType)
	}
}

func TestRateLimiter_UserLimits(t *testing.T) {
	rl := NewRateLimiter(DefaultRateLimitConfig())

	// Set user-specific limits
	rl.SetUserLimits("premium-user", &RateLimitConfig{
		RequestsPerMinute: 1000,
		RequestsPerHour:   10000,
		RequestsPerDay:    100000,
	})

	// Premium user should have higher limits
	for i := 0; i < 100; i++ {
		result := rl.CheckRateLimit("premium-user", "", true)
		if !result.Allowed {
			t.Errorf("premium user request %d should be allowed", i+1)
		}
	}
}

// =============================================================================
// Kernel Tests
// =============================================================================

func TestKernel_Submit(t *testing.T) {
	logger := &testLogger{}
	kernel := NewKernel(logger, nil)

	pcb, err := kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	if err != nil {
		t.Fatalf("submit failed: %v", err)
	}

	if pcb.State != ProcessStateNew {
		t.Errorf("expected NEW state, got %s", pcb.State)
	}

	// Verify resources allocated
	if !kernel.Resources().IsTracked("pid-1") {
		t.Error("resources should be allocated for pid-1")
	}
}

func TestKernel_RecordLLMCall(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, &ResourceQuota{
		MaxLLMCalls:    2,
		TimeoutSeconds: 3600, // 1 hour - prevent timeout in unit tests
	})

	// First call - within quota
	exceeded := kernel.RecordLLMCall("pid-1", 100, 50)
	if exceeded != "" {
		t.Errorf("first call should be within quota, got: %s", exceeded)
	}

	// Second call - still within quota
	exceeded = kernel.RecordLLMCall("pid-1", 100, 50)
	if exceeded != "" {
		t.Errorf("second call should be within quota, got: %s", exceeded)
	}

	// Third call - exceeds quota
	exceeded = kernel.RecordLLMCall("pid-1", 100, 50)
	if exceeded != "max_llm_calls_exceeded" {
		t.Errorf("third call should exceed quota, got: %s", exceeded)
	}
}

func TestKernel_Interrupts(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("env-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)

	// Create interrupt
	interrupt := kernel.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
		WithInterruptQuestion("What file?"),
	)

	if interrupt == nil {
		t.Fatal("interrupt should not be nil")
	}

	// Get pending interrupt
	pending := kernel.GetPendingInterrupt("req-1")
	if pending == nil {
		t.Fatal("pending interrupt should not be nil")
	}
	if pending.ID != interrupt.ID {
		t.Error("pending interrupt ID should match")
	}

	// Resolve interrupt
	response := &envelope.InterruptResponse{
		Text: strPtr("main.py"),
	}
	resolved := kernel.ResolveInterrupt(interrupt.ID, response, "user-1")
	if resolved == nil {
		t.Fatal("resolved interrupt should not be nil")
	}

	// No more pending
	pending = kernel.GetPendingInterrupt("req-1")
	if pending != nil {
		t.Error("should have no pending interrupt after resolve")
	}
}

func TestKernel_ServiceDispatch(t *testing.T) {
	kernel := NewKernel(nil, nil)

	// Register service
	info := NewServiceInfo("test-service", "flow")
	kernel.RegisterService(info)

	// Register handler
	handlerCalled := false
	kernel.RegisterHandler("test-service", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		handlerCalled = true
		return &DispatchResult{Success: true}, nil
	})

	// Dispatch
	target := &DispatchTarget{
		ServiceName:    "test-service",
		Method:         "execute",
		TimeoutSeconds: 10,
	}
	result := kernel.Dispatch(context.Background(), target, nil)

	if !result.Success {
		t.Errorf("dispatch should succeed, got error: %s", result.Error)
	}
	if !handlerCalled {
		t.Error("handler should have been called")
	}
}

func TestKernel_SystemStatus(t *testing.T) {
	kernel := NewKernel(nil, nil)

	// Submit some processes
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	kernel.Submit("pid-2", "req-2", "user-1", "sess-1", PriorityHigh, nil)
	kernel.Schedule("pid-1")

	// Register a service
	kernel.RegisterService(NewServiceInfo("test-service", "flow"))

	status := kernel.GetSystemStatus()

	processes, ok := status["processes"].(map[string]any)
	if !ok {
		t.Fatal("status should have processes")
	}
	if processes["total"] != 2 {
		t.Errorf("expected 2 total processes, got %v", processes["total"])
	}

	services, ok := status["services"].(map[string]any)
	if !ok {
		t.Fatal("status should have services")
	}
	if services["total_services"] != 1 {
		t.Errorf("expected 1 service, got %v", services["total_services"])
	}
}

func TestKernel_EventHandlers(t *testing.T) {
	kernel := NewKernel(nil, nil)

	var receivedEvents []*KernelEvent
	var mu sync.Mutex

	kernel.OnEvent(func(event *KernelEvent) {
		mu.Lock()
		defer mu.Unlock()
		receivedEvents = append(receivedEvents, event)
	})

	// Submit should emit process created event
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)

	mu.Lock()
	eventCount := len(receivedEvents)
	mu.Unlock()

	if eventCount == 0 {
		t.Error("should have received at least one event")
	}

	// Find process created event
	mu.Lock()
	found := false
	for _, event := range receivedEvents {
		if event.EventType == KernelEventProcessCreated {
			found = true
			break
		}
	}
	mu.Unlock()

	if !found {
		t.Error("should have received process.created event")
	}
}

// =============================================================================
// Concurrent Access Tests
// =============================================================================

func TestResourceTracker_ConcurrentAccess(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	var wg sync.WaitGroup
	for i := 0; i < 100; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			tracker.RecordUsage("pid-1", 1, 0, 0, 10, 5)
		}()
	}
	wg.Wait()

	usage := tracker.GetUsage("pid-1")
	if usage.LLMCalls != 100 {
		t.Errorf("expected 100 LLM calls after concurrent updates, got %d", usage.LLMCalls)
	}
}

func TestKernel_ConcurrentSubmit(t *testing.T) {
	kernel := NewKernel(nil, nil)

	var wg sync.WaitGroup
	for i := 0; i < 50; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			pid := "pid-" + string(rune('a'+n%26)) + string(rune('0'+n/26))
			kernel.Submit(pid, "req-"+pid, "user-1", "sess-1", PriorityNormal, nil)
		}(i)
	}
	wg.Wait()

	// Verify all processes were created
	processes := kernel.ListProcesses(nil, "")
	if len(processes) != 50 {
		t.Errorf("expected 50 processes, got %d", len(processes))
	}
}

// =============================================================================
// ProcessState Tests
// =============================================================================

func TestProcessState_IsTerminal(t *testing.T) {
	tests := []struct {
		state    ProcessState
		expected bool
	}{
		{ProcessStateNew, false},
		{ProcessStateReady, false},
		{ProcessStateRunning, false},
		{ProcessStateWaiting, false},
		{ProcessStateBlocked, false},
		{ProcessStateTerminated, true},
		{ProcessStateZombie, true},
	}

	for _, tt := range tests {
		t.Run(string(tt.state), func(t *testing.T) {
			if got := tt.state.IsTerminal(); got != tt.expected {
				t.Errorf("ProcessState(%s).IsTerminal() = %v, want %v", tt.state, got, tt.expected)
			}
		})
	}
}

func TestProcessState_CanSchedule(t *testing.T) {
	tests := []struct {
		state    ProcessState
		expected bool
	}{
		{ProcessStateNew, true},
		{ProcessStateReady, true},
		{ProcessStateRunning, false},
		{ProcessStateWaiting, false},
		{ProcessStateBlocked, false},
		{ProcessStateTerminated, false},
		{ProcessStateZombie, false},
	}

	for _, tt := range tests {
		t.Run(string(tt.state), func(t *testing.T) {
			if got := tt.state.CanSchedule(); got != tt.expected {
				t.Errorf("ProcessState(%s).CanSchedule() = %v, want %v", tt.state, got, tt.expected)
			}
		})
	}
}

func TestProcessState_IsRunnable(t *testing.T) {
	tests := []struct {
		state    ProcessState
		expected bool
	}{
		{ProcessStateNew, false},
		{ProcessStateReady, true},
		{ProcessStateRunning, false},
		{ProcessStateWaiting, false},
		{ProcessStateBlocked, false},
		{ProcessStateTerminated, false},
		{ProcessStateZombie, false},
	}

	for _, tt := range tests {
		t.Run(string(tt.state), func(t *testing.T) {
			if got := tt.state.IsRunnable(); got != tt.expected {
				t.Errorf("ProcessState(%s).IsRunnable() = %v, want %v", tt.state, got, tt.expected)
			}
		})
	}
}

// =============================================================================
// SchedulingPriority Tests
// =============================================================================

func TestSchedulingPriority_Weight(t *testing.T) {
	tests := []struct {
		priority SchedulingPriority
		expected int
	}{
		{PriorityRealtime, 100},
		{PriorityHigh, 75},
		{PriorityNormal, 50},
		{PriorityLow, 25},
		{PriorityIdle, 1},
		{SchedulingPriority("unknown"), 50}, // Default case
	}

	for _, tt := range tests {
		t.Run(string(tt.priority), func(t *testing.T) {
			if got := tt.priority.Weight(); got != tt.expected {
				t.Errorf("SchedulingPriority(%s).Weight() = %d, want %d", tt.priority, got, tt.expected)
			}
		})
	}
}

// =============================================================================
// ResourceQuota Tests
// =============================================================================

func TestResourceQuota_IsWithinBounds(t *testing.T) {
	quota := &ResourceQuota{
		MaxLLMCalls:   10,
		MaxToolCalls:  20,
		MaxAgentHops:  5,
		MaxIterations: 3,
	}

	tests := []struct {
		name       string
		llmCalls   int
		toolCalls  int
		agentHops  int
		iterations int
		expected   bool
	}{
		{"all_within", 5, 10, 2, 1, true},
		{"at_limits", 10, 20, 5, 3, true},
		{"llm_exceeded", 11, 10, 2, 1, false},
		{"tool_exceeded", 5, 21, 2, 1, false},
		{"hops_exceeded", 5, 10, 6, 1, false},
		{"iterations_exceeded", 5, 10, 2, 4, false},
		{"all_zero", 0, 0, 0, 0, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := quota.IsWithinBounds(tt.llmCalls, tt.toolCalls, tt.agentHops, tt.iterations); got != tt.expected {
				t.Errorf("IsWithinBounds() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestDefaultQuota(t *testing.T) {
	quota := DefaultQuota()
	if quota == nil {
		t.Fatal("DefaultQuota() should not return nil")
	}
	if quota.MaxLLMCalls <= 0 {
		t.Error("DefaultQuota should have positive MaxLLMCalls")
	}
	if quota.TimeoutSeconds <= 0 {
		t.Error("DefaultQuota should have positive TimeoutSeconds")
	}
}

// =============================================================================
// ResourceUsage Tests
// =============================================================================

func TestResourceUsage_ExceedsQuota(t *testing.T) {
	quota := &ResourceQuota{
		MaxLLMCalls:    10,
		MaxToolCalls:   20,
		MaxAgentHops:   5,
		MaxIterations:  3,
		TimeoutSeconds: 300,
	}

	tests := []struct {
		name     string
		usage    *ResourceUsage
		expected string
	}{
		{
			"within_quota",
			&ResourceUsage{LLMCalls: 5, ToolCalls: 10, AgentHops: 2, Iterations: 1, ElapsedSeconds: 100},
			"",
		},
		{
			"llm_exceeded",
			&ResourceUsage{LLMCalls: 11, ToolCalls: 10, AgentHops: 2, Iterations: 1, ElapsedSeconds: 100},
			"max_llm_calls_exceeded",
		},
		{
			"tool_exceeded",
			&ResourceUsage{LLMCalls: 5, ToolCalls: 21, AgentHops: 2, Iterations: 1, ElapsedSeconds: 100},
			"max_tool_calls_exceeded",
		},
		{
			"hops_exceeded",
			&ResourceUsage{LLMCalls: 5, ToolCalls: 10, AgentHops: 6, Iterations: 1, ElapsedSeconds: 100},
			"max_agent_hops_exceeded",
		},
		{
			"iterations_exceeded",
			&ResourceUsage{LLMCalls: 5, ToolCalls: 10, AgentHops: 2, Iterations: 4, ElapsedSeconds: 100},
			"max_iterations_exceeded",
		},
		{
			"timeout_exceeded",
			&ResourceUsage{LLMCalls: 5, ToolCalls: 10, AgentHops: 2, Iterations: 1, ElapsedSeconds: 301},
			"timeout_exceeded",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.usage.ExceedsQuota(quota); got != tt.expected {
				t.Errorf("ExceedsQuota() = %q, want %q", got, tt.expected)
			}
		})
	}
}

func TestResourceUsage_Clone(t *testing.T) {
	original := &ResourceUsage{
		LLMCalls:       5,
		ToolCalls:      10,
		AgentHops:      2,
		Iterations:     1,
		TokensIn:       100,
		TokensOut:      50,
		ElapsedSeconds: 30.5,
	}

	clone := original.Clone()

	if clone == original {
		t.Error("Clone should return a new instance")
	}
	if clone.LLMCalls != original.LLMCalls {
		t.Errorf("LLMCalls: got %d, want %d", clone.LLMCalls, original.LLMCalls)
	}
	if clone.ElapsedSeconds != original.ElapsedSeconds {
		t.Errorf("ElapsedSeconds: got %f, want %f", clone.ElapsedSeconds, original.ElapsedSeconds)
	}

	// Modify clone and verify original unchanged
	clone.LLMCalls = 100
	if original.LLMCalls == 100 {
		t.Error("Modifying clone should not affect original")
	}
}

// =============================================================================
// ProcessControlBlock Tests
// =============================================================================

func TestNewProcessControlBlock(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")

	if pcb.PID != "pid-1" {
		t.Errorf("PID: got %s, want pid-1", pcb.PID)
	}
	if pcb.RequestID != "req-1" {
		t.Errorf("RequestID: got %s, want req-1", pcb.RequestID)
	}
	if pcb.UserID != "user-1" {
		t.Errorf("UserID: got %s, want user-1", pcb.UserID)
	}
	if pcb.SessionID != "sess-1" {
		t.Errorf("SessionID: got %s, want sess-1", pcb.SessionID)
	}
	if pcb.State != ProcessStateNew {
		t.Errorf("State: got %s, want new", pcb.State)
	}
	if pcb.Priority != PriorityNormal {
		t.Errorf("Priority: got %s, want normal", pcb.Priority)
	}
	if pcb.Quota == nil {
		t.Error("Quota should not be nil")
	}
	if pcb.Usage == nil {
		t.Error("Usage should not be nil")
	}
	if pcb.ChildPIDs == nil {
		t.Error("ChildPIDs should not be nil")
	}
}

func TestProcessControlBlock_CanSchedule(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")

	if !pcb.CanSchedule() {
		t.Error("New PCB should be schedulable")
	}

	pcb.State = ProcessStateRunning
	if pcb.CanSchedule() {
		t.Error("Running PCB should not be schedulable")
	}
}

func TestProcessControlBlock_IsRunnable(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")

	if pcb.IsRunnable() {
		t.Error("New PCB should not be runnable")
	}

	pcb.State = ProcessStateReady
	if !pcb.IsRunnable() {
		t.Error("Ready PCB should be runnable")
	}
}

func TestProcessControlBlock_IsTerminated(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")

	if pcb.IsTerminated() {
		t.Error("New PCB should not be terminated")
	}

	pcb.State = ProcessStateTerminated
	if !pcb.IsTerminated() {
		t.Error("Terminated PCB should be terminated")
	}
}

func TestProcessControlBlock_Start(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.Start()

	if pcb.State != ProcessStateRunning {
		t.Errorf("State: got %s, want running", pcb.State)
	}
	if pcb.StartedAt == nil {
		t.Error("StartedAt should be set")
	}
	if pcb.LastScheduledAt == nil {
		t.Error("LastScheduledAt should be set")
	}
}

func TestProcessControlBlock_Complete(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.Start()
	time.Sleep(10 * time.Millisecond)
	pcb.Complete()

	if pcb.State != ProcessStateTerminated {
		t.Errorf("State: got %s, want terminated", pcb.State)
	}
	if pcb.CompletedAt == nil {
		t.Error("CompletedAt should be set")
	}
	if pcb.Usage.ElapsedSeconds <= 0 {
		t.Error("ElapsedSeconds should be positive")
	}
}

func TestProcessControlBlock_Block(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.Block("resource_unavailable")

	if pcb.State != ProcessStateBlocked {
		t.Errorf("State: got %s, want blocked", pcb.State)
	}
	if pcb.InterruptData == nil {
		t.Error("InterruptData should be set")
	}
	if pcb.InterruptData["block_reason"] != "resource_unavailable" {
		t.Error("Block reason should be set")
	}
}

func TestProcessControlBlock_Wait(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.Wait(envelope.InterruptKindClarification)

	if pcb.State != ProcessStateWaiting {
		t.Errorf("State: got %s, want waiting", pcb.State)
	}
	if pcb.PendingInterrupt == nil {
		t.Error("PendingInterrupt should be set")
	}
	if *pcb.PendingInterrupt != envelope.InterruptKindClarification {
		t.Error("PendingInterrupt kind should be clarification")
	}
}

func TestProcessControlBlock_Resume(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.Block("resource_unavailable")
	pcb.Resume()

	if pcb.State != ProcessStateReady {
		t.Errorf("State: got %s, want ready", pcb.State)
	}
	if pcb.PendingInterrupt != nil {
		t.Error("PendingInterrupt should be nil after resume")
	}
}

func TestProcessControlBlock_RecordLLMCall(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.RecordLLMCall(100, 50)

	if pcb.Usage.LLMCalls != 1 {
		t.Errorf("LLMCalls: got %d, want 1", pcb.Usage.LLMCalls)
	}
	if pcb.Usage.TokensIn != 100 {
		t.Errorf("TokensIn: got %d, want 100", pcb.Usage.TokensIn)
	}
	if pcb.Usage.TokensOut != 50 {
		t.Errorf("TokensOut: got %d, want 50", pcb.Usage.TokensOut)
	}
}

func TestProcessControlBlock_RecordToolCall(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.RecordToolCall()
	pcb.RecordToolCall()

	if pcb.Usage.ToolCalls != 2 {
		t.Errorf("ToolCalls: got %d, want 2", pcb.Usage.ToolCalls)
	}
}

func TestProcessControlBlock_RecordAgentHop(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.RecordAgentHop()

	if pcb.Usage.AgentHops != 1 {
		t.Errorf("AgentHops: got %d, want 1", pcb.Usage.AgentHops)
	}
}

func TestProcessControlBlock_CheckQuota(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.Quota = &ResourceQuota{MaxLLMCalls: 2}

	// Within quota
	pcb.RecordLLMCall(100, 50)
	if reason := pcb.CheckQuota(); reason != "" {
		t.Errorf("Should be within quota, got: %s", reason)
	}

	// Exceed quota
	pcb.RecordLLMCall(100, 50)
	pcb.RecordLLMCall(100, 50)
	if reason := pcb.CheckQuota(); reason != "max_llm_calls_exceeded" {
		t.Errorf("Should exceed quota, got: %s", reason)
	}
}

// =============================================================================
// ServiceDescriptor Tests
// =============================================================================

func TestNewServiceDescriptor(t *testing.T) {
	sd := NewServiceDescriptor("test-service", "flow")

	if sd.Name != "test-service" {
		t.Errorf("Name: got %s, want test-service", sd.Name)
	}
	if sd.ServiceType != "flow" {
		t.Errorf("ServiceType: got %s, want flow", sd.ServiceType)
	}
	if sd.Version != "1.0.0" {
		t.Errorf("Version: got %s, want 1.0.0", sd.Version)
	}
	if !sd.Healthy {
		t.Error("Should be healthy by default")
	}
	if sd.MaxConcurrent != 10 {
		t.Errorf("MaxConcurrent: got %d, want 10", sd.MaxConcurrent)
	}
}

func TestServiceDescriptor_CanAccept(t *testing.T) {
	sd := NewServiceDescriptor("test-service", "flow")
	sd.MaxConcurrent = 2

	if !sd.CanAccept() {
		t.Error("Should accept when healthy and under capacity")
	}

	sd.CurrentLoad = 2
	if sd.CanAccept() {
		t.Error("Should not accept when at capacity")
	}

	sd.CurrentLoad = 0
	sd.Healthy = false
	if sd.CanAccept() {
		t.Error("Should not accept when unhealthy")
	}
}

// =============================================================================
// KernelEvent Tests
// =============================================================================

func TestNewKernelEvent(t *testing.T) {
	event := NewKernelEvent(KernelEventProcessCreated, "pid-1", "req-1", "user-1", "sess-1")

	if event.EventType != KernelEventProcessCreated {
		t.Errorf("EventType: got %s, want process.created", event.EventType)
	}
	if event.PID != "pid-1" {
		t.Errorf("PID: got %s, want pid-1", event.PID)
	}
	if event.Timestamp.IsZero() {
		t.Error("Timestamp should be set")
	}
}

func TestProcessCreatedEvent(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.Priority = PriorityHigh
	event := ProcessCreatedEvent(pcb)

	if event.EventType != KernelEventProcessCreated {
		t.Errorf("EventType: got %s, want process.created", event.EventType)
	}
	if event.Data["priority"] != "high" {
		t.Errorf("Priority: got %v, want high", event.Data["priority"])
	}
}

func TestProcessStateChangedEvent(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	pcb.State = ProcessStateRunning
	event := ProcessStateChangedEvent(pcb, ProcessStateReady)

	if event.EventType != KernelEventProcessStateChanged {
		t.Errorf("EventType: got %s, want process.state_changed", event.EventType)
	}
	if event.Data["old_state"] != "ready" {
		t.Errorf("old_state: got %v, want ready", event.Data["old_state"])
	}
	if event.Data["new_state"] != "running" {
		t.Errorf("new_state: got %v, want running", event.Data["new_state"])
	}
}

func TestInterruptRaisedEvent(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	data := map[string]any{"question": "Which file?"}
	event := InterruptRaisedEvent(pcb, envelope.InterruptKindClarification, data)

	if event.EventType != KernelEventInterruptRaised {
		t.Errorf("EventType: got %s, want interrupt.raised", event.EventType)
	}
	if event.Data["interrupt_type"] != "clarification" {
		t.Errorf("interrupt_type: got %v, want clarification", event.Data["interrupt_type"])
	}
	if event.Data["question"] != "Which file?" {
		t.Errorf("question: got %v, want 'Which file?'", event.Data["question"])
	}
}

func TestResourceExhaustedEvent(t *testing.T) {
	pcb := NewProcessControlBlock("pid-1", "req-1", "user-1", "sess-1")
	event := ResourceExhaustedEvent(pcb, "llm_calls", 11, 10)

	if event.EventType != KernelEventResourceExhausted {
		t.Errorf("EventType: got %s, want resource.exhausted", event.EventType)
	}
	if event.Data["resource"] != "llm_calls" {
		t.Errorf("resource: got %v, want llm_calls", event.Data["resource"])
	}
	if event.Data["usage"] != 11 {
		t.Errorf("usage: got %v, want 11", event.Data["usage"])
	}
	if event.Data["quota"] != 10 {
		t.Errorf("quota: got %v, want 10", event.Data["quota"])
	}
}

// =============================================================================
// IsValidTransition Tests
// =============================================================================

func TestIsValidTransition(t *testing.T) {
	tests := []struct {
		name     string
		from     ProcessState
		to       ProcessState
		expected bool
	}{
		{"new_to_ready", ProcessStateNew, ProcessStateReady, true},
		{"new_to_terminated", ProcessStateNew, ProcessStateTerminated, true},
		{"new_to_running", ProcessStateNew, ProcessStateRunning, false},
		{"ready_to_running", ProcessStateReady, ProcessStateRunning, true},
		{"ready_to_terminated", ProcessStateReady, ProcessStateTerminated, true},
		{"running_to_ready", ProcessStateRunning, ProcessStateReady, true},
		{"running_to_waiting", ProcessStateRunning, ProcessStateWaiting, true},
		{"running_to_blocked", ProcessStateRunning, ProcessStateBlocked, true},
		{"running_to_terminated", ProcessStateRunning, ProcessStateTerminated, true},
		{"waiting_to_ready", ProcessStateWaiting, ProcessStateReady, true},
		{"waiting_to_terminated", ProcessStateWaiting, ProcessStateTerminated, true},
		{"waiting_to_running", ProcessStateWaiting, ProcessStateRunning, false},
		{"blocked_to_ready", ProcessStateBlocked, ProcessStateReady, true},
		{"blocked_to_terminated", ProcessStateBlocked, ProcessStateTerminated, true},
		{"terminated_to_zombie", ProcessStateTerminated, ProcessStateZombie, true},
		{"terminated_to_ready", ProcessStateTerminated, ProcessStateReady, false},
		{"zombie_to_anything", ProcessStateZombie, ProcessStateReady, false},
		{"unknown_state", ProcessState("unknown"), ProcessStateReady, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := IsValidTransition(tt.from, tt.to); got != tt.expected {
				t.Errorf("IsValidTransition(%s, %s) = %v, want %v", tt.from, tt.to, got, tt.expected)
			}
		})
	}
}

// =============================================================================
// LifecycleManager Edge Case Tests
// =============================================================================

func TestLifecycleManager_TransitionState(t *testing.T) {
	lm := NewLifecycleManager(nil)

	// Submit and schedule
	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Schedule("pid-1")
	lm.GetNextRunnable() // Transition to RUNNING

	// Test valid transition: RUNNING -> WAITING
	if err := lm.TransitionState("pid-1", ProcessStateWaiting, "awaiting_user_input"); err != nil {
		t.Errorf("Valid transition should succeed: %v", err)
	}

	pcb := lm.GetProcess("pid-1")
	if pcb.State != ProcessStateWaiting {
		t.Errorf("State should be WAITING, got %s", pcb.State)
	}

	// Test valid transition: WAITING -> READY (should re-add to queue)
	if err := lm.TransitionState("pid-1", ProcessStateReady, "user_responded"); err != nil {
		t.Errorf("Valid transition should succeed: %v", err)
	}

	if lm.GetQueueDepth() != 1 {
		t.Errorf("Queue depth should be 1, got %d", lm.GetQueueDepth())
	}

	// Test invalid transition
	if err := lm.TransitionState("pid-1", ProcessStateNew, "invalid"); err == nil {
		t.Error("Invalid transition should fail")
	}

	// Test unknown pid
	if err := lm.TransitionState("unknown-pid", ProcessStateReady, "test"); err == nil {
		t.Error("Unknown pid should fail")
	}
}

func TestLifecycleManager_TransitionToTerminated(t *testing.T) {
	lm := NewLifecycleManager(nil)
	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Schedule("pid-1")
	lm.GetNextRunnable()

	// Transition to TERMINATED
	err := lm.TransitionState("pid-1", ProcessStateTerminated, "completed")
	if err != nil {
		t.Fatalf("Transition to terminated failed: %v", err)
	}

	pcb := lm.GetProcess("pid-1")
	if pcb.CompletedAt == nil {
		t.Error("CompletedAt should be set after termination")
	}
}

func TestLifecycleManager_ListProcesses(t *testing.T) {
	lm := NewLifecycleManager(nil)

	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Submit("pid-2", "req-2", "user-1", "sess-1", PriorityHigh, nil)
	lm.Submit("pid-3", "req-3", "user-2", "sess-1", PriorityNormal, nil)
	lm.Schedule("pid-1")

	// List all
	all := lm.ListProcesses(nil, "")
	if len(all) != 3 {
		t.Errorf("Expected 3 processes, got %d", len(all))
	}

	// List by state
	newState := ProcessStateNew
	newProcesses := lm.ListProcesses(&newState, "")
	if len(newProcesses) != 2 {
		t.Errorf("Expected 2 NEW processes, got %d", len(newProcesses))
	}

	// List by user
	user1Processes := lm.ListProcesses(nil, "user-1")
	if len(user1Processes) != 2 {
		t.Errorf("Expected 2 user-1 processes, got %d", len(user1Processes))
	}

	// List by state and user
	readyState := ProcessStateReady
	user1Ready := lm.ListProcesses(&readyState, "user-1")
	if len(user1Ready) != 1 {
		t.Errorf("Expected 1 user-1 READY process, got %d", len(user1Ready))
	}
}

func TestLifecycleManager_Cleanup(t *testing.T) {
	lm := NewLifecycleManager(nil)

	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Schedule("pid-1")
	lm.GetNextRunnable()
	lm.Terminate("pid-1", "done", true)

	// Cleanup should succeed for terminated process
	if err := lm.Cleanup("pid-1"); err != nil {
		t.Errorf("Cleanup should succeed: %v", err)
	}

	// Process should be removed
	if pcb := lm.GetProcess("pid-1"); pcb != nil {
		t.Error("Process should be removed after cleanup")
	}

	// Cleanup unknown pid should fail
	if err := lm.Cleanup("unknown-pid"); err == nil {
		t.Error("Cleanup unknown pid should fail")
	}

	// Cleanup active process should fail
	lm.Submit("pid-2", "req-2", "user-1", "sess-1", PriorityNormal, nil)
	if err := lm.Cleanup("pid-2"); err == nil {
		t.Error("Cleanup active process should fail")
	}
}

func TestLifecycleManager_GetQueueDepth(t *testing.T) {
	lm := NewLifecycleManager(nil)

	if lm.GetQueueDepth() != 0 {
		t.Error("Initial queue depth should be 0")
	}

	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Submit("pid-2", "req-2", "user-1", "sess-1", PriorityHigh, nil)
	lm.Schedule("pid-1")
	lm.Schedule("pid-2")

	if lm.GetQueueDepth() != 2 {
		t.Errorf("Queue depth should be 2, got %d", lm.GetQueueDepth())
	}

	lm.GetNextRunnable()
	if lm.GetQueueDepth() != 1 {
		t.Errorf("Queue depth should be 1 after GetNextRunnable, got %d", lm.GetQueueDepth())
	}
}

func TestLifecycleManager_GetProcessCount(t *testing.T) {
	lm := NewLifecycleManager(nil)

	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Submit("pid-2", "req-2", "user-1", "sess-1", PriorityHigh, nil)
	lm.Schedule("pid-1")
	lm.GetNextRunnable()

	counts := lm.GetProcessCount()

	if counts[ProcessStateNew] != 1 {
		t.Errorf("Expected 1 NEW process, got %d", counts[ProcessStateNew])
	}
	if counts[ProcessStateRunning] != 1 {
		t.Errorf("Expected 1 RUNNING process, got %d", counts[ProcessStateRunning])
	}
}

func TestLifecycleManager_GetTotalProcesses(t *testing.T) {
	lm := NewLifecycleManager(nil)

	if lm.GetTotalProcesses() != 0 {
		t.Error("Initial total should be 0")
	}

	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Submit("pid-2", "req-2", "user-1", "sess-1", PriorityHigh, nil)

	if lm.GetTotalProcesses() != 2 {
		t.Errorf("Expected 2 total processes, got %d", lm.GetTotalProcesses())
	}
}

func TestLifecycleManager_TerminateRunningWithoutForce(t *testing.T) {
	lm := NewLifecycleManager(nil)
	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Schedule("pid-1")
	lm.GetNextRunnable()

	// Try to terminate without force
	err := lm.Terminate("pid-1", "test", false)
	if err == nil {
		t.Error("Terminating running process without force should fail")
	}
}

func TestLifecycleManager_Schedule_InvalidState(t *testing.T) {
	lm := NewLifecycleManager(nil)
	lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	lm.Schedule("pid-1")

	// Try to schedule again (now in READY state)
	err := lm.Schedule("pid-1")
	if err == nil {
		t.Error("Scheduling non-NEW process should fail")
	}
}

func TestLifecycleManager_Schedule_UnknownPid(t *testing.T) {
	lm := NewLifecycleManager(nil)

	err := lm.Schedule("unknown-pid")
	if err == nil {
		t.Error("Scheduling unknown pid should fail")
	}
}

func TestLifecycleManager_GetNextRunnable_EmptyQueue(t *testing.T) {
	lm := NewLifecycleManager(nil)

	pcb := lm.GetNextRunnable()
	if pcb != nil {
		t.Error("GetNextRunnable on empty queue should return nil")
	}
}

func TestLifecycleManager_SubmitDuplicate(t *testing.T) {
	lm := NewLifecycleManager(nil)

	pcb1, _ := lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	pcb2, _ := lm.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityHigh, nil)

	// Should return existing PCB
	if pcb1 != pcb2 {
		t.Error("Duplicate submit should return existing PCB")
	}
	if pcb2.Priority != PriorityNormal {
		t.Error("Duplicate submit should not change priority")
	}
}

// =============================================================================
// RateLimiter Edge Case Tests
// =============================================================================

func TestRateLimitResult_OK(t *testing.T) {
	result := &RateLimitResult{}
	ok := result.OK(50)

	if !ok.Allowed {
		t.Error("OK result should be allowed")
	}
	if ok.Exceeded {
		t.Error("OK result should not be exceeded")
	}
	if ok.Remaining != 50 {
		t.Errorf("Remaining should be 50, got %d", ok.Remaining)
	}
}

func TestExceededLimit(t *testing.T) {
	result := ExceededLimit("minute", 61, 60, 30.5)

	if result.Allowed {
		t.Error("Exceeded result should not be allowed")
	}
	if !result.Exceeded {
		t.Error("Exceeded result should be exceeded")
	}
	if result.LimitType != "minute" {
		t.Errorf("LimitType should be minute, got %s", result.LimitType)
	}
	if result.Current != 61 {
		t.Errorf("Current should be 61, got %d", result.Current)
	}
	if result.Limit != 60 {
		t.Errorf("Limit should be 60, got %d", result.Limit)
	}
	if result.RetryAfter != 30.5 {
		t.Errorf("RetryAfter should be 30.5, got %f", result.RetryAfter)
	}
}

func TestAllowedResult(t *testing.T) {
	result := AllowedResult(25)

	if !result.Allowed {
		t.Error("Allowed result should be allowed")
	}
	if result.Exceeded {
		t.Error("Allowed result should not be exceeded")
	}
	if result.Remaining != 25 {
		t.Errorf("Remaining should be 25, got %d", result.Remaining)
	}
}

func TestSlidingWindow_IsEmpty(t *testing.T) {
	window := NewSlidingWindow(60)

	if !window.IsEmpty() {
		t.Error("New window should be empty")
	}

	now := float64(time.Now().UnixNano()) / 1e9
	window.Record(now)

	if window.IsEmpty() {
		t.Error("Window with records should not be empty")
	}
}

func TestSlidingWindow_GetCount(t *testing.T) {
	window := NewSlidingWindow(60)
	now := float64(time.Now().UnixNano()) / 1e9

	if window.GetCount(now) != 0 {
		t.Error("New window should have count 0")
	}

	window.Record(now)
	window.Record(now)
	window.Record(now)

	if window.GetCount(now) != 3 {
		t.Errorf("Expected count 3, got %d", window.GetCount(now))
	}
}

func TestSlidingWindow_TimeUntilSlotAvailable_UnderLimit(t *testing.T) {
	window := NewSlidingWindow(60)
	now := float64(time.Now().UnixNano()) / 1e9

	window.Record(now)

	// Under limit, should return 0
	timeUntil := window.TimeUntilSlotAvailable(now, 10)
	if timeUntil != 0 {
		t.Errorf("Under limit, time until should be 0, got %f", timeUntil)
	}
}

func TestSlidingWindow_TimeUntilSlotAvailable_AtLimit(t *testing.T) {
	window := NewSlidingWindow(60)
	now := float64(time.Now().UnixNano()) / 1e9

	// Record exactly at limit
	for i := 0; i < 5; i++ {
		window.Record(now)
	}

	// At limit, should return positive value
	timeUntil := window.TimeUntilSlotAvailable(now, 5)
	if timeUntil <= 0 {
		t.Errorf("At limit, time until should be positive, got %f", timeUntil)
	}
}

func TestRateLimiter_SetDefaultConfig(t *testing.T) {
	rl := NewRateLimiter(nil)

	newConfig := &RateLimitConfig{
		RequestsPerMinute: 1000,
		RequestsPerHour:   10000,
		RequestsPerDay:    100000,
	}

	rl.SetDefaultConfig(newConfig)

	// Verify new config is used
	config := rl.GetConfig("any-user", "")
	if config.RequestsPerMinute != 1000 {
		t.Errorf("Expected 1000 RPM, got %d", config.RequestsPerMinute)
	}
}

func TestRateLimiter_SetEndpointLimits(t *testing.T) {
	rl := NewRateLimiter(DefaultRateLimitConfig())

	rl.SetEndpointLimits("/api/expensive", &RateLimitConfig{
		RequestsPerMinute: 5,
		RequestsPerHour:   50,
		RequestsPerDay:    500,
	})

	// Endpoint config should override user config
	config := rl.GetConfig("any-user", "/api/expensive")
	if config.RequestsPerMinute != 5 {
		t.Errorf("Expected 5 RPM for endpoint, got %d", config.RequestsPerMinute)
	}
}

func TestRateLimiter_GetUsage(t *testing.T) {
	config := &RateLimitConfig{
		RequestsPerMinute: 60,
		RequestsPerHour:   1000,
		RequestsPerDay:    10000,
	}
	rl := NewRateLimiter(config)

	// Make some requests
	rl.CheckRateLimit("user-1", "/api/test", true)
	rl.CheckRateLimit("user-1", "/api/test", true)

	usage := rl.GetUsage("user-1", "/api/test")

	if usage["minute"] == nil {
		t.Error("Should have minute usage")
	}
	minuteUsage := usage["minute"]
	if minuteUsage["current"] != 2 {
		t.Errorf("Expected 2 current requests, got %v", minuteUsage["current"])
	}
	if minuteUsage["limit"] != 60 {
		t.Errorf("Expected limit 60, got %v", minuteUsage["limit"])
	}
}

func TestRateLimiter_ResetUser(t *testing.T) {
	config := &RateLimitConfig{
		RequestsPerMinute: 60,
		RequestsPerHour:   1000,
		RequestsPerDay:    10000,
	}
	rl := NewRateLimiter(config)

	// Make some requests
	rl.CheckRateLimit("user-1", "/api/test", true)
	rl.CheckRateLimit("user-1", "/api/other", true)

	// Reset user
	count := rl.ResetUser("user-1")
	if count != 6 { // 3 windows per endpoint * 2 endpoints
		t.Errorf("Expected 6 windows reset, got %d", count)
	}

	// Verify usage is reset
	usage := rl.GetUsage("user-1", "/api/test")
	if usage["minute"]["current"] != 0 {
		t.Errorf("Usage should be reset to 0, got %v", usage["minute"]["current"])
	}
}

func TestRateLimiter_CleanupExpired(t *testing.T) {
	config := &RateLimitConfig{
		RequestsPerMinute: 60,
		RequestsPerHour:   1000,
		RequestsPerDay:    10000,
	}
	rl := NewRateLimiter(config)

	// Make a request
	rl.CheckRateLimit("user-1", "/api/test", true)

	// Cleanup should not remove active windows
	cleaned := rl.CleanupExpired()
	// Windows with activity shouldn't be cleaned
	if cleaned > 0 {
		t.Logf("Cleaned %d windows (may include empty ones)", cleaned)
	}
}

func TestRateLimiter_CheckRateLimit_NoRecord(t *testing.T) {
	config := &RateLimitConfig{
		RequestsPerMinute: 2,
		RequestsPerHour:   100,
		RequestsPerDay:    1000,
	}
	rl := NewRateLimiter(config)

	// Check without recording
	result := rl.CheckRateLimit("user-1", "", false)
	if !result.Allowed {
		t.Error("First check should be allowed")
	}

	// Check again without recording
	result = rl.CheckRateLimit("user-1", "", false)
	if !result.Allowed {
		t.Error("Second check should still be allowed (not recorded)")
	}
}

func TestRateLimiter_CheckRateLimit_HourLimit(t *testing.T) {
	config := &RateLimitConfig{
		RequestsPerMinute: 1000, // High minute limit
		RequestsPerHour:   3,    // Low hour limit
		RequestsPerDay:    1000,
	}
	rl := NewRateLimiter(config)

	// Make 3 requests (at hour limit)
	for i := 0; i < 3; i++ {
		result := rl.CheckRateLimit("user-1", "", true)
		if !result.Allowed {
			t.Errorf("Request %d should be allowed", i+1)
		}
	}

	// Fourth should be limited by hour
	result := rl.CheckRateLimit("user-1", "", true)
	if result.Allowed {
		t.Error("Fourth request should be rate limited")
	}
	if result.LimitType != "hour" {
		t.Errorf("Expected hour limit, got %s", result.LimitType)
	}
}

func TestRateLimiter_CheckRateLimit_DayLimit(t *testing.T) {
	config := &RateLimitConfig{
		RequestsPerMinute: 1000,
		RequestsPerHour:   1000,
		RequestsPerDay:    2, // Low day limit
	}
	rl := NewRateLimiter(config)

	// Make 2 requests
	for i := 0; i < 2; i++ {
		result := rl.CheckRateLimit("user-1", "", true)
		if !result.Allowed {
			t.Errorf("Request %d should be allowed", i+1)
		}
	}

	// Third should be limited by day
	result := rl.CheckRateLimit("user-1", "", true)
	if result.Allowed {
		t.Error("Third request should be rate limited")
	}
	if result.LimitType != "day" {
		t.Errorf("Expected day limit, got %s", result.LimitType)
	}
}

func TestRateLimiter_NilConfig(t *testing.T) {
	rl := NewRateLimiter(nil)

	// Should use default config
	config := rl.GetConfig("user-1", "")
	if config == nil {
		t.Fatal("Config should not be nil")
	}
	if config.RequestsPerMinute != 60 {
		t.Errorf("Default RPM should be 60, got %d", config.RequestsPerMinute)
	}
}

func TestRateLimiter_ZeroLimit(t *testing.T) {
	config := &RateLimitConfig{
		RequestsPerMinute: 0, // No minute limit
		RequestsPerHour:   0, // No hour limit
		RequestsPerDay:    2,
	}
	rl := NewRateLimiter(config)

	// Should only check day limit
	for i := 0; i < 2; i++ {
		result := rl.CheckRateLimit("user-1", "", true)
		if !result.Allowed {
			t.Errorf("Request %d should be allowed", i+1)
		}
	}

	result := rl.CheckRateLimit("user-1", "", true)
	if result.Allowed {
		t.Error("Third request should be rate limited by day")
	}
}

// =============================================================================
// ResourceTracker Edge Case Tests
// =============================================================================

func TestResourceTracker_RecordUsageCreatesProcess(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)

	// Record usage for non-existent process should create it
	usage := tracker.RecordUsage("new-pid", 1, 0, 0, 100, 50)
	if usage == nil {
		t.Fatal("Usage should not be nil")
	}
	if !tracker.IsTracked("new-pid") {
		t.Error("Process should be tracked after RecordUsage")
	}
}

func TestResourceTracker_RecordLLMCall(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	usage := tracker.RecordLLMCall("pid-1", 100, 50)
	if usage.LLMCalls != 1 {
		t.Errorf("Expected 1 LLM call, got %d", usage.LLMCalls)
	}
	if usage.TokensIn != 100 {
		t.Errorf("Expected 100 tokens in, got %d", usage.TokensIn)
	}
}

func TestResourceTracker_RecordToolCall(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	usage := tracker.RecordToolCall("pid-1")
	if usage.ToolCalls != 1 {
		t.Errorf("Expected 1 tool call, got %d", usage.ToolCalls)
	}
}

func TestResourceTracker_RecordAgentHop(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	usage := tracker.RecordAgentHop("pid-1")
	if usage.AgentHops != 1 {
		t.Errorf("Expected 1 agent hop, got %d", usage.AgentHops)
	}
}

func TestResourceTracker_GetQuota(t *testing.T) {
	customQuota := &ResourceQuota{
		MaxLLMCalls:  100,
		MaxToolCalls: 200,
	}
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", customQuota)

	quota := tracker.GetQuota("pid-1")
	if quota == nil {
		t.Fatal("Quota should not be nil")
	}
	if quota.MaxLLMCalls != 100 {
		t.Errorf("Expected MaxLLMCalls 100, got %d", quota.MaxLLMCalls)
	}

	// Modifying returned quota should not affect original
	quota.MaxLLMCalls = 999
	original := tracker.GetQuota("pid-1")
	if original.MaxLLMCalls != 100 {
		t.Error("Original quota should not be modified")
	}

	// Non-existent process
	if tracker.GetQuota("unknown-pid") != nil {
		t.Error("Unknown pid should return nil quota")
	}
}

func TestResourceTracker_UpdateElapsedTime(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	time.Sleep(10 * time.Millisecond)
	elapsed := tracker.UpdateElapsedTime("pid-1")

	if elapsed <= 0 {
		t.Error("Elapsed time should be positive")
	}

	// Unknown pid
	if tracker.UpdateElapsedTime("unknown-pid") != -1 {
		t.Error("Unknown pid should return -1")
	}
}

func TestResourceTracker_AdjustQuota(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	adjustments := map[string]int{
		"max_llm_calls":        100,
		"max_tool_calls":       200,
		"max_agent_hops":       10,
		"max_iterations":       5,
		"timeout_seconds":      600,
		"soft_timeout_seconds": 500,
		"max_input_tokens":     8000,
		"max_output_tokens":    4000,
		"max_context_tokens":   32000,
	}

	err := tracker.AdjustQuota("pid-1", adjustments)
	if err != nil {
		t.Fatalf("AdjustQuota failed: %v", err)
	}

	quota := tracker.GetQuota("pid-1")
	if quota.MaxLLMCalls != 100 {
		t.Errorf("MaxLLMCalls should be 100, got %d", quota.MaxLLMCalls)
	}
	if quota.MaxToolCalls != 200 {
		t.Errorf("MaxToolCalls should be 200, got %d", quota.MaxToolCalls)
	}
	if quota.TimeoutSeconds != 600 {
		t.Errorf("TimeoutSeconds should be 600, got %d", quota.TimeoutSeconds)
	}

	// Unknown pid
	err = tracker.AdjustQuota("unknown-pid", adjustments)
	if err == nil {
		t.Error("AdjustQuota for unknown pid should fail")
	}
}

func TestResourceTracker_GetAllUsage(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)
	tracker.Allocate("pid-2", nil)

	tracker.RecordUsage("pid-1", 1, 2, 0, 100, 50)
	tracker.RecordUsage("pid-2", 3, 4, 1, 200, 100)

	allUsage := tracker.GetAllUsage()
	if len(allUsage) != 2 {
		t.Errorf("Expected 2 processes, got %d", len(allUsage))
	}

	if allUsage["pid-1"].LLMCalls != 1 {
		t.Errorf("pid-1 LLMCalls should be 1, got %d", allUsage["pid-1"].LLMCalls)
	}
	if allUsage["pid-2"].LLMCalls != 3 {
		t.Errorf("pid-2 LLMCalls should be 3, got %d", allUsage["pid-2"].LLMCalls)
	}
}

func TestResourceTracker_GetProcessCount(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)

	if tracker.GetProcessCount() != 0 {
		t.Error("Initial process count should be 0")
	}

	tracker.Allocate("pid-1", nil)
	tracker.Allocate("pid-2", nil)

	if tracker.GetProcessCount() != 2 {
		t.Errorf("Process count should be 2, got %d", tracker.GetProcessCount())
	}

	tracker.Release("pid-1")
	if tracker.GetProcessCount() != 1 {
		t.Errorf("Process count should be 1 after release, got %d", tracker.GetProcessCount())
	}
}

func TestResourceTracker_WithLogger(t *testing.T) {
	logger := &testLogger{}
	quota := &ResourceQuota{
		MaxLLMCalls:        10,
		SoftTimeoutSeconds: 1,
	}
	tracker := NewResourceTracker(quota, logger)
	tracker.Allocate("pid-1", quota)

	// Trigger 80% warning
	tracker.RecordUsage("pid-1", 8, 0, 0, 0, 0)

	// Check if warning was logged
	found := false
	for _, log := range logger.logs {
		if log == "WARN: approaching_llm_limit" {
			found = true
			break
		}
	}
	if !found {
		t.Error("Should log warning when approaching LLM limit")
	}
}

func TestResourceTracker_DuplicateAllocateWithLogger(t *testing.T) {
	logger := &testLogger{}
	tracker := NewResourceTracker(nil, logger)

	tracker.Allocate("pid-1", nil)
	tracker.Allocate("pid-1", nil) // Duplicate

	found := false
	for _, log := range logger.logs {
		if log == "WARN: duplicate_allocation" {
			found = true
			break
		}
	}
	if !found {
		t.Error("Should log warning on duplicate allocation")
	}
}

func TestResourceTracker_CheckQuota_NonTracked(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)

	// Non-tracked process should return empty (no limits)
	if exceeded := tracker.CheckQuota("unknown-pid"); exceeded != "" {
		t.Errorf("Non-tracked process should return empty, got: %s", exceeded)
	}
}

func TestResourceTracker_GetUsage_NonTracked(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)

	if usage := tracker.GetUsage("unknown-pid"); usage != nil {
		t.Error("Non-tracked process should return nil usage")
	}
}

func TestResourceTracker_GetRemainingBudget_NonTracked(t *testing.T) {
	tracker := NewResourceTracker(nil, nil)

	if budget := tracker.GetRemainingBudget("unknown-pid"); budget != nil {
		t.Error("Non-tracked process should return nil budget")
	}
}

// =============================================================================
// DispatchTarget Tests
// =============================================================================

func TestDispatchTarget_CanRetry(t *testing.T) {
	target := &DispatchTarget{
		ServiceName: "test-service",
		MaxRetries:  3,
		RetryCount:  0,
	}

	if !target.CanRetry() {
		t.Error("Should be able to retry when RetryCount < MaxRetries")
	}

	target.RetryCount = 3
	if target.CanRetry() {
		t.Error("Should not be able to retry when RetryCount >= MaxRetries")
	}
}

func TestDispatchTarget_IncrementRetry(t *testing.T) {
	target := &DispatchTarget{
		ServiceName: "test-service",
		RetryCount:  0,
	}

	target.IncrementRetry()
	if target.RetryCount != 1 {
		t.Errorf("RetryCount should be 1 after increment, got %d", target.RetryCount)
	}

	target.IncrementRetry()
	if target.RetryCount != 2 {
		t.Errorf("RetryCount should be 2 after second increment, got %d", target.RetryCount)
	}
}

// =============================================================================
// ServiceInfo Tests
// =============================================================================

func TestServiceInfo_IsHealthy(t *testing.T) {
	tests := []struct {
		status   ServiceStatus
		expected bool
	}{
		{ServiceStatusHealthy, true},
		{ServiceStatusDegraded, true},
		{ServiceStatusUnhealthy, false},
		{ServiceStatusUnknown, false},
	}

	for _, tt := range tests {
		t.Run(string(tt.status), func(t *testing.T) {
			info := NewServiceInfo("test", "flow")
			info.Status = tt.status
			if got := info.IsHealthy(); got != tt.expected {
				t.Errorf("IsHealthy() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestServiceInfo_Clone(t *testing.T) {
	original := NewServiceInfo("test-service", "flow")
	original.Capabilities = []string{"execute", "query"}
	original.Metadata = map[string]any{"key": "value", "count": 42}
	original.CurrentLoad = 5

	clone := original.Clone()

	// Verify values are copied
	if clone.Name != original.Name {
		t.Errorf("Name: got %s, want %s", clone.Name, original.Name)
	}
	if clone.CurrentLoad != original.CurrentLoad {
		t.Errorf("CurrentLoad: got %d, want %d", clone.CurrentLoad, original.CurrentLoad)
	}
	if len(clone.Capabilities) != len(original.Capabilities) {
		t.Errorf("Capabilities length mismatch")
	}
	if len(clone.Metadata) != len(original.Metadata) {
		t.Errorf("Metadata length mismatch")
	}

	// Verify modifying clone doesn't affect original
	clone.Name = "modified"
	clone.Capabilities[0] = "modified"
	clone.Metadata["key"] = "modified"

	if original.Name == "modified" {
		t.Error("Modifying clone should not affect original name")
	}
	if original.Capabilities[0] == "modified" {
		t.Error("Modifying clone should not affect original capabilities")
	}
	if original.Metadata["key"] == "modified" {
		t.Error("Modifying clone should not affect original metadata")
	}
}

func TestServiceInfo_Clone_NilFields(t *testing.T) {
	original := NewServiceInfo("test-service", "flow")
	original.Capabilities = nil
	original.Metadata = nil

	clone := original.Clone()

	if clone.Capabilities != nil {
		t.Error("Clone should have nil capabilities when original is nil")
	}
	if clone.Metadata != nil {
		t.Error("Clone should have nil metadata when original is nil")
	}
}

// =============================================================================
// ServiceRegistry Additional Tests
// =============================================================================

func TestServiceRegistry_UnregisterService(t *testing.T) {
	registry := NewServiceRegistry(nil)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))
	registry.RegisterHandler("test-service", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		return &DispatchResult{Success: true}, nil
	})

	// Unregister should succeed
	if !registry.UnregisterService("test-service") {
		t.Error("Unregister should succeed for existing service")
	}

	// Service should no longer exist
	if registry.HasService("test-service") {
		t.Error("Service should not exist after unregistration")
	}

	// Handler should also be removed
	if registry.HasHandler("test-service") {
		t.Error("Handler should not exist after unregistration")
	}

	// Unregister non-existent service should fail
	if registry.UnregisterService("non-existent") {
		t.Error("Unregister should fail for non-existent service")
	}
}

func TestServiceRegistry_UnregisterService_WithLogger(t *testing.T) {
	logger := &testLogger{}
	registry := NewServiceRegistry(logger)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))

	registry.UnregisterService("test-service")

	found := false
	for _, log := range logger.logs {
		if log == "INFO: service_unregistered" {
			found = true
			break
		}
	}
	if !found {
		t.Error("Should log service unregistration")
	}
}

func TestServiceRegistry_GetService(t *testing.T) {
	registry := NewServiceRegistry(nil)
	info := NewServiceInfo("test-service", "flow")
	info.Metadata = map[string]any{"key": "value"}
	registry.RegisterService(info)

	// Get existing service
	retrieved := registry.GetService("test-service")
	if retrieved == nil {
		t.Fatal("Should return service info")
	}
	if retrieved.Name != "test-service" {
		t.Errorf("Name should be test-service, got %s", retrieved.Name)
	}

	// Verify it's a clone
	retrieved.Name = "modified"
	original := registry.GetService("test-service")
	if original.Name == "modified" {
		t.Error("GetService should return a clone")
	}

	// Get non-existent service
	if registry.GetService("non-existent") != nil {
		t.Error("Should return nil for non-existent service")
	}
}

func TestServiceRegistry_GetServiceNames(t *testing.T) {
	registry := NewServiceRegistry(nil)
	registry.RegisterService(NewServiceInfo("service-a", "flow"))
	registry.RegisterService(NewServiceInfo("service-b", "worker"))
	registry.RegisterService(NewServiceInfo("service-c", "vertical"))

	names := registry.GetServiceNames()
	if len(names) != 3 {
		t.Errorf("Expected 3 service names, got %d", len(names))
	}

	// Verify all names are present
	nameMap := make(map[string]bool)
	for _, name := range names {
		nameMap[name] = true
	}
	if !nameMap["service-a"] || !nameMap["service-b"] || !nameMap["service-c"] {
		t.Error("All service names should be present")
	}
}

func TestServiceRegistry_HasService(t *testing.T) {
	registry := NewServiceRegistry(nil)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))

	if !registry.HasService("test-service") {
		t.Error("HasService should return true for existing service")
	}

	if registry.HasService("non-existent") {
		t.Error("HasService should return false for non-existent service")
	}
}

func TestServiceRegistry_HasHandler(t *testing.T) {
	registry := NewServiceRegistry(nil)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))
	registry.RegisterHandler("test-service", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		return &DispatchResult{Success: true}, nil
	})

	if !registry.HasHandler("test-service") {
		t.Error("HasHandler should return true for service with handler")
	}

	if registry.HasHandler("service-without-handler") {
		t.Error("HasHandler should return false for service without handler")
	}
}

func TestServiceRegistry_GetServiceStats(t *testing.T) {
	registry := NewServiceRegistry(nil)

	info1 := NewServiceInfo("service-a", "flow")
	info1.CurrentLoad = 2
	registry.RegisterService(info1)
	registry.RegisterHandler("service-a", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		return &DispatchResult{Success: true}, nil
	})

	info2 := NewServiceInfo("service-b", "worker")
	info2.Status = ServiceStatusUnhealthy
	registry.RegisterService(info2)

	stats := registry.GetServiceStats()
	if len(stats) != 2 {
		t.Errorf("Expected 2 services in stats, got %d", len(stats))
	}

	serviceA := stats["service-a"]
	if serviceA == nil {
		t.Fatal("Should have stats for service-a")
	}
	if serviceA["type"] != "flow" {
		t.Errorf("service-a type should be flow, got %v", serviceA["type"])
	}
	if serviceA["current_load"] != 2 {
		t.Errorf("service-a current_load should be 2, got %v", serviceA["current_load"])
	}
	if serviceA["has_handler"] != true {
		t.Errorf("service-a should have handler")
	}

	serviceB := stats["service-b"]
	if serviceB == nil {
		t.Fatal("Should have stats for service-b")
	}
	if serviceB["status"] != "unhealthy" {
		t.Errorf("service-b status should be unhealthy, got %v", serviceB["status"])
	}
	if serviceB["has_handler"] != false {
		t.Errorf("service-b should not have handler")
	}
}

func TestServiceRegistry_Dispatch_UnhealthyService(t *testing.T) {
	registry := NewServiceRegistry(nil)
	info := NewServiceInfo("test-service", "flow")
	info.Status = ServiceStatusUnhealthy
	registry.RegisterService(info)

	target := &DispatchTarget{ServiceName: "test-service"}
	result := registry.Dispatch(context.Background(), target, nil)

	if result.Success {
		t.Error("Dispatch to unhealthy service should fail")
	}
	if result.Error == "" {
		t.Error("Error message should be set")
	}
}

func TestServiceRegistry_Dispatch_AtCapacity(t *testing.T) {
	registry := NewServiceRegistry(nil)
	info := NewServiceInfo("test-service", "flow")
	info.MaxConcurrent = 1
	info.CurrentLoad = 1 // Already at capacity
	registry.RegisterService(info)

	target := &DispatchTarget{ServiceName: "test-service"}
	result := registry.Dispatch(context.Background(), target, nil)

	if result.Success {
		t.Error("Dispatch to service at capacity should fail")
	}
}

func TestServiceRegistry_Dispatch_NoHandler(t *testing.T) {
	registry := NewServiceRegistry(nil)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))
	// No handler registered

	target := &DispatchTarget{ServiceName: "test-service"}
	result := registry.Dispatch(context.Background(), target, nil)

	if result.Success {
		t.Error("Dispatch without handler should fail")
	}
}

func TestServiceRegistry_Dispatch_WithRetry(t *testing.T) {
	registry := NewServiceRegistry(nil)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))

	callCount := 0
	registry.RegisterHandler("test-service", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		callCount++
		if callCount < 3 {
			return nil, fmt.Errorf("transient error")
		}
		return &DispatchResult{Success: true}, nil
	})

	target := &DispatchTarget{
		ServiceName: "test-service",
		MaxRetries:  3,
		RetryCount:  0,
	}
	result := registry.Dispatch(context.Background(), target, nil)

	if !result.Success {
		t.Errorf("Dispatch with retry should eventually succeed, got error: %s", result.Error)
	}
	if result.Retries != 2 {
		t.Errorf("Should have retried 2 times, got %d", result.Retries)
	}
}

func TestServiceRegistry_Dispatch_RetryExhausted(t *testing.T) {
	registry := NewServiceRegistry(nil)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))

	registry.RegisterHandler("test-service", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		return nil, fmt.Errorf("persistent error")
	})

	target := &DispatchTarget{
		ServiceName: "test-service",
		MaxRetries:  2,
		RetryCount:  0,
	}
	result := registry.Dispatch(context.Background(), target, nil)

	if result.Success {
		t.Error("Dispatch should fail when retries exhausted")
	}
	if result.Retries != 2 {
		t.Errorf("Should have retried max times, got %d", result.Retries)
	}
}

func TestServiceRegistry_Dispatch_NilResult(t *testing.T) {
	registry := NewServiceRegistry(nil)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))

	registry.RegisterHandler("test-service", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		return nil, nil // Handler returns nil result
	})

	target := &DispatchTarget{
		ServiceName:    "test-service",
		TimeoutSeconds: 10,
	}
	result := registry.Dispatch(context.Background(), target, nil)

	if !result.Success {
		t.Error("Dispatch should succeed with nil result")
	}
	// Duration is set even for very fast operations, but may be 0 in some cases
	// The important thing is that result is not nil and Success is true
}

func TestServiceRegistry_GetLoad_NonExistent(t *testing.T) {
	registry := NewServiceRegistry(nil)

	load := registry.GetLoad("non-existent")
	if load != -1 {
		t.Errorf("GetLoad for non-existent service should return -1, got %d", load)
	}
}

func TestServiceRegistry_DecrementLoad_NonExistent(t *testing.T) {
	registry := NewServiceRegistry(nil)
	// Should not panic
	registry.DecrementLoad("non-existent")
}

func TestServiceRegistry_DecrementLoad_AtZero(t *testing.T) {
	registry := NewServiceRegistry(nil)
	info := NewServiceInfo("test-service", "flow")
	info.CurrentLoad = 0
	registry.RegisterService(info)

	registry.DecrementLoad("test-service")

	// Load should still be 0 (not negative)
	if registry.GetLoad("test-service") != 0 {
		t.Error("Load should not go below 0")
	}
}

func TestServiceRegistry_RegisterService_WithLogger(t *testing.T) {
	logger := &testLogger{}
	registry := NewServiceRegistry(logger)

	registry.RegisterService(NewServiceInfo("test-service", "flow"))

	found := false
	for _, log := range logger.logs {
		if log == "INFO: service_registered" {
			found = true
			break
		}
	}
	if !found {
		t.Error("Should log service registration")
	}
}

func TestServiceRegistry_RegisterHandler_WithLogger(t *testing.T) {
	logger := &testLogger{}
	registry := NewServiceRegistry(logger)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))

	registry.RegisterHandler("test-service", func(ctx context.Context, target *DispatchTarget, data map[string]any) (*DispatchResult, error) {
		return &DispatchResult{Success: true}, nil
	})

	found := false
	for _, log := range logger.logs {
		if log == "DEBUG: handler_registered" {
			found = true
			break
		}
	}
	if !found {
		t.Error("Should log handler registration")
	}
}

func TestServiceRegistry_UpdateHealth_WithLogger(t *testing.T) {
	logger := &testLogger{}
	registry := NewServiceRegistry(logger)
	registry.RegisterService(NewServiceInfo("test-service", "flow"))

	registry.UpdateHealth("test-service", ServiceStatusDegraded)

	found := false
	for _, log := range logger.logs {
		if log == "DEBUG: service_health_updated" {
			found = true
			break
		}
	}
	if !found {
		t.Error("Should log health update")
	}
}

func TestServiceRegistry_UpdateHealth_NonExistent(t *testing.T) {
	registry := NewServiceRegistry(nil)

	if registry.UpdateHealth("non-existent", ServiceStatusHealthy) {
		t.Error("UpdateHealth should return false for non-existent service")
	}
}

// =============================================================================
// Kernel Component Accessors Tests
// =============================================================================

func TestKernel_ComponentAccessors(t *testing.T) {
	kernel := NewKernel(nil, nil)

	if kernel.Lifecycle() == nil {
		t.Error("Lifecycle() should not return nil")
	}
	if kernel.Resources() == nil {
		t.Error("Resources() should not return nil")
	}
	if kernel.RateLimiter() == nil {
		t.Error("RateLimiter() should not return nil")
	}
	if kernel.Interrupts() == nil {
		t.Error("Interrupts() should not return nil")
	}
	if kernel.Services() == nil {
		t.Error("Services() should not return nil")
	}
}

func TestKernel_GetNextRunnable(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	kernel.Schedule("pid-1")

	pcb := kernel.GetNextRunnable()
	if pcb == nil {
		t.Fatal("Should get a runnable process")
	}
	if pcb.State != ProcessStateRunning {
		t.Errorf("State should be RUNNING, got %s", pcb.State)
	}
}

func TestKernel_TransitionState(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	kernel.Schedule("pid-1")
	kernel.GetNextRunnable()

	// Transition to WAITING
	err := kernel.TransitionState("pid-1", ProcessStateWaiting, "awaiting_input")
	if err != nil {
		t.Fatalf("TransitionState failed: %v", err)
	}

	pcb := kernel.GetProcess("pid-1")
	if pcb.State != ProcessStateWaiting {
		t.Errorf("State should be WAITING, got %s", pcb.State)
	}
}

func TestKernel_Terminate(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	kernel.Schedule("pid-1")
	kernel.GetNextRunnable()

	err := kernel.Terminate("pid-1", "test", true)
	if err != nil {
		t.Fatalf("Terminate failed: %v", err)
	}

	pcb := kernel.GetProcess("pid-1")
	if pcb.State != ProcessStateTerminated {
		t.Errorf("State should be TERMINATED, got %s", pcb.State)
	}
}

func TestKernel_GetProcess(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)

	pcb := kernel.GetProcess("pid-1")
	if pcb == nil {
		t.Fatal("Should return process")
	}
	if pcb.PID != "pid-1" {
		t.Errorf("PID should be pid-1, got %s", pcb.PID)
	}

	// Non-existent process
	if kernel.GetProcess("non-existent") != nil {
		t.Error("Should return nil for non-existent process")
	}
}

func TestKernel_RecordToolCall(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, &ResourceQuota{
		MaxToolCalls:   2,
		TimeoutSeconds: 3600, // 1 hour - prevent timeout in unit tests
	})

	// First call
	exceeded := kernel.RecordToolCall("pid-1")
	if exceeded != "" {
		t.Errorf("First call should be within quota, got: %s", exceeded)
	}

	// Second call
	exceeded = kernel.RecordToolCall("pid-1")
	if exceeded != "" {
		t.Errorf("Second call should be within quota, got: %s", exceeded)
	}

	// Third call - exceeds quota
	exceeded = kernel.RecordToolCall("pid-1")
	if exceeded != "max_tool_calls_exceeded" {
		t.Errorf("Third call should exceed quota, got: %s", exceeded)
	}
}

func TestKernel_RecordAgentHop(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, &ResourceQuota{
		MaxAgentHops:   1,
		TimeoutSeconds: 3600, // 1 hour - prevent timeout in unit tests
	})

	exceeded := kernel.RecordAgentHop("pid-1")
	if exceeded != "" {
		t.Errorf("First hop should be within quota, got: %s", exceeded)
	}

	exceeded = kernel.RecordAgentHop("pid-1")
	if exceeded != "max_agent_hops_exceeded" {
		t.Errorf("Second hop should exceed quota, got: %s", exceeded)
	}
}

func TestKernel_CheckQuota(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, &ResourceQuota{
		MaxLLMCalls: 1,
	})

	if exceeded := kernel.CheckQuota("pid-1"); exceeded != "" {
		t.Error("Should be within quota initially")
	}

	kernel.RecordLLMCall("pid-1", 100, 50)
	kernel.RecordLLMCall("pid-1", 100, 50)

	if exceeded := kernel.CheckQuota("pid-1"); exceeded != "max_llm_calls_exceeded" {
		t.Errorf("Should exceed quota, got: %s", exceeded)
	}
}

func TestKernel_GetUsage(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	kernel.RecordLLMCall("pid-1", 100, 50)

	usage := kernel.GetUsage("pid-1")
	if usage == nil {
		t.Fatal("Should return usage")
	}
	if usage.LLMCalls != 1 {
		t.Errorf("LLMCalls should be 1, got %d", usage.LLMCalls)
	}
}

func TestKernel_GetRemainingBudget(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, &ResourceQuota{
		MaxLLMCalls: 10,
	})
	kernel.RecordLLMCall("pid-1", 100, 50)

	budget := kernel.GetRemainingBudget("pid-1")
	if budget == nil {
		t.Fatal("Should return budget")
	}
	if budget.LLMCalls != 9 {
		t.Errorf("Remaining LLM calls should be 9, got %d", budget.LLMCalls)
	}
}

func TestKernel_CheckRateLimit(t *testing.T) {
	kernel := NewKernel(nil, nil)

	result := kernel.CheckRateLimit("user-1", "/api/test", true)
	if !result.Allowed {
		t.Error("First request should be allowed")
	}
}

func TestKernel_GetRateLimitUsage(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.CheckRateLimit("user-1", "/api/test", true)

	usage := kernel.GetRateLimitUsage("user-1", "/api/test")
	if usage == nil {
		t.Fatal("Should return usage")
	}
	if usage["minute"] == nil {
		t.Error("Should have minute usage")
	}
}

func TestKernel_UnregisterService(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.RegisterService(NewServiceInfo("test-service", "flow"))

	if !kernel.UnregisterService("test-service") {
		t.Error("Unregister should succeed")
	}

	// Verify service is removed
	services := kernel.Services().ListServices("", false)
	for _, svc := range services {
		if svc.Name == "test-service" {
			t.Error("Service should be removed")
		}
	}
}

func TestKernel_GetRequestStatus(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	kernel.RecordLLMCall("pid-1", 100, 50)

	status := kernel.GetRequestStatus("pid-1")
	if status == nil {
		t.Fatal("Should return status")
	}

	if status["pid"] != "pid-1" {
		t.Errorf("PID should be pid-1, got %v", status["pid"])
	}

	if status["state"] != "new" {
		t.Errorf("State should be new, got %v", status["state"])
	}

	usage, ok := status["usage"].(map[string]any)
	if !ok || usage == nil {
		t.Error("Should have usage info")
	}
	if usage["llm_calls"] != 1 {
		t.Errorf("llm_calls should be 1, got %v", usage["llm_calls"])
	}

	// Non-existent process
	status = kernel.GetRequestStatus("non-existent")
	if status != nil {
		t.Error("Should return nil for non-existent process")
	}
}

func TestKernel_Cleanup(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)

	// Cleanup performs periodic cleanup tasks (rate limiter, interrupts)
	// Should not panic
	kernel.Cleanup()
}

func TestKernel_Shutdown(t *testing.T) {
	kernel := NewKernel(nil, nil)
	kernel.Submit("pid-1", "req-1", "user-1", "sess-1", PriorityNormal, nil)
	kernel.Schedule("pid-1")
	kernel.GetNextRunnable()

	// Shutdown should terminate running processes
	err := kernel.Shutdown(context.Background())
	if err != nil {
		t.Fatalf("Shutdown failed: %v", err)
	}

	pcb := kernel.GetProcess("pid-1")
	if pcb != nil && pcb.State != ProcessStateTerminated {
		t.Error("Running process should be terminated on shutdown")
	}
}

// =============================================================================
// InterruptService Additional Tests
// =============================================================================

func TestInterruptService_WithInterruptMessage(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
		WithInterruptMessage("This is a message"),
	)

	if interrupt.Message != "This is a message" {
		t.Errorf("Message should be set, got: %s", interrupt.Message)
	}
}

func TestInterruptService_WithInterruptData(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
		WithInterruptData(map[string]any{"key": "value"}),
	)

	if interrupt.Data == nil {
		t.Fatal("Data should be set")
	}
	if interrupt.Data["key"] != "value" {
		t.Errorf("Data key should be 'value', got: %v", interrupt.Data["key"])
	}
}

func TestInterruptService_WithInterruptTTL(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
		WithInterruptTTL(60*time.Second),
	)

	// Check that ExpiresAt is roughly 60 seconds in the future
	expectedExpiry := time.Now().Add(60 * time.Second)
	if interrupt.ExpiresAt == nil {
		t.Fatal("ExpiresAt should be set")
	}
	diff := interrupt.ExpiresAt.Sub(expectedExpiry)
	if diff < -time.Second || diff > time.Second {
		t.Error("ExpiresAt should be approximately 60 seconds in the future")
	}
}

func TestInterruptService_WithTraceContext(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
		WithTraceContext("trace-123", "span-456"),
	)

	if interrupt.TraceID != "trace-123" {
		t.Errorf("TraceID should be trace-123, got: %s", interrupt.TraceID)
	}
	if interrupt.SpanID != "span-456" {
		t.Errorf("SpanID should be span-456, got: %s", interrupt.SpanID)
	}
}

func TestInterruptService_CreateClarification(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateClarification("req-1", "user-1", "sess-1", "env-1", "Which file?", nil)

	if interrupt.Kind != envelope.InterruptKindClarification {
		t.Errorf("Kind should be clarification, got: %s", interrupt.Kind)
	}
	if interrupt.Question != "Which file?" {
		t.Errorf("Question should be 'Which file?', got: %s", interrupt.Question)
	}
}

func TestInterruptService_CreateConfirmation(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateConfirmation("req-1", "user-1", "sess-1", "env-1", "Delete this file?", nil)

	if interrupt.Kind != envelope.InterruptKindConfirmation {
		t.Errorf("Kind should be confirmation, got: %s", interrupt.Kind)
	}
	if interrupt.Message != "Delete this file?" {
		t.Errorf("Message should be 'Delete this file?', got: %s", interrupt.Message)
	}
}

func TestInterruptService_CreateResourceExhausted(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateResourceExhausted("req-1", "user-1", "sess-1", "env-1", "llm_calls", 30.5)

	if interrupt.Kind != envelope.InterruptKindResourceExhausted {
		t.Errorf("Kind should be resource_exhausted, got: %s", interrupt.Kind)
	}
	if interrupt.Data["resource_type"] != "llm_calls" {
		t.Errorf("resource_type should be llm_calls, got: %v", interrupt.Data["resource_type"])
	}
	if interrupt.Data["retry_after_seconds"] != 30.5 {
		t.Errorf("retry_after_seconds should be 30.5, got: %v", interrupt.Data["retry_after_seconds"])
	}
}

func TestInterruptService_GetInterrupt(t *testing.T) {
	service := NewInterruptService(nil, nil)

	created := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)

	retrieved := service.GetInterrupt(created.ID)
	if retrieved == nil {
		t.Fatal("Should return interrupt")
	}
	if retrieved.ID != created.ID {
		t.Error("IDs should match")
	}

	// Non-existent
	if service.GetInterrupt("non-existent") != nil {
		t.Error("Should return nil for non-existent interrupt")
	}
}

func TestInterruptService_GetPendingForSession(t *testing.T) {
	service := NewInterruptService(nil, nil)

	service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)
	service.CreateInterrupt(
		envelope.InterruptKindConfirmation,
		"req-2", "user-1", "sess-1", "env-2",
	)

	// Get all pending for session (nil kinds = all kinds)
	pending := service.GetPendingForSession("sess-1", nil)
	if len(pending) != 2 {
		t.Errorf("Should have 2 pending interrupts, got %d", len(pending))
	}

	// Filter by kind
	pendingClarifications := service.GetPendingForSession("sess-1", []envelope.InterruptKind{envelope.InterruptKindClarification})
	if len(pendingClarifications) != 1 {
		t.Errorf("Should have 1 clarification interrupt, got %d", len(pendingClarifications))
	}

	// Different session
	pending = service.GetPendingForSession("sess-2", nil)
	if len(pending) != 0 {
		t.Errorf("Should have 0 pending interrupts for sess-2, got %d", len(pending))
	}
}

func TestInterruptService_CleanupResolved(t *testing.T) {
	service := NewInterruptService(nil, nil)

	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)

	// Resolve the interrupt
	response := &envelope.InterruptResponse{Text: strPtr("answer")}
	service.Resolve(interrupt.ID, response, "user-1")

	// Wait briefly so the interrupt is older than our cleanup threshold
	time.Sleep(10 * time.Millisecond)

	// Cleanup resolved (with a duration that includes our interrupt)
	// The function removes resolved interrupts older than the given duration
	// Using a small positive duration to clean up recently resolved interrupts
	count := service.CleanupResolved(1 * time.Millisecond)
	if count != 1 {
		t.Errorf("Should have cleaned up 1 resolved interrupt, got %d", count)
	}

	// Verify it's removed
	if service.GetInterrupt(interrupt.ID) != nil {
		t.Error("Resolved interrupt should be removed")
	}
}

func TestInterruptService_GetPendingCount(t *testing.T) {
	service := NewInterruptService(nil, nil)

	if service.GetPendingCount() != 0 {
		t.Error("Initial pending count should be 0")
	}

	service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)
	service.CreateInterrupt(
		envelope.InterruptKindConfirmation,
		"req-2", "user-1", "sess-1", "env-2",
	)

	if service.GetPendingCount() != 2 {
		t.Errorf("Pending count should be 2, got %d", service.GetPendingCount())
	}
}

func TestInterruptService_GetStats(t *testing.T) {
	service := NewInterruptService(nil, nil)

	service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
	)
	interrupt2 := service.CreateInterrupt(
		envelope.InterruptKindConfirmation,
		"req-2", "user-1", "sess-1", "env-2",
	)
	service.Resolve(interrupt2.ID, &envelope.InterruptResponse{Approved: boolPtr(true)}, "user-1")

	stats := service.GetStats()
	if stats["total"] != 2 {
		t.Errorf("Total should be 2, got %v", stats["total"])
	}
	if stats["pending"] != 1 {
		t.Errorf("Pending should be 1, got %v", stats["pending"])
	}
	if stats["resolved"] != 1 {
		t.Errorf("Resolved should be 1, got %v", stats["resolved"])
	}
}

func TestInterruptService_IsExpired(t *testing.T) {
	service := NewInterruptService(nil, nil)

	// Create interrupt with very short TTL
	interrupt := service.CreateInterrupt(
		envelope.InterruptKindClarification,
		"req-1", "user-1", "sess-1", "env-1",
		WithInterruptTTL(1*time.Millisecond),
	)

	// Wait for expiry
	time.Sleep(5 * time.Millisecond)

	if !interrupt.IsExpired() {
		t.Error("Interrupt should be expired")
	}
}

// =============================================================================
// Helper Functions
// =============================================================================

func strPtr(s string) *string {
	return &s
}

func boolPtr(b bool) *bool {
	return &b
}
