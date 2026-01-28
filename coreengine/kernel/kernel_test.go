package kernel

import (
	"context"
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
		MaxLLMCalls:  2,
		MaxToolCalls: 5,
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
		MaxLLMCalls: 2,
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
// Helper Functions
// =============================================================================

func strPtr(s string) *string {
	return &s
}
