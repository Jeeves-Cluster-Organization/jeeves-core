package kernel

import (
	"fmt"
	"sync"
	"testing"
)

// =============================================================================
// Process Scheduling Benchmarks
// =============================================================================

// BenchmarkLifecycleManager_Submit benchmarks process submission.
func BenchmarkLifecycleManager_Submit(b *testing.B) {
	lm := NewLifecycleManager(nil)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		lm.Submit(pid, "req-1", "user-1", "sess-1", PriorityNormal, nil)
	}
}

// BenchmarkLifecycleManager_Schedule benchmarks process scheduling.
func BenchmarkLifecycleManager_Schedule(b *testing.B) {
	lm := NewLifecycleManager(nil)

	// Pre-create processes
	for i := 0; i < b.N; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		lm.Submit(pid, "req-1", "user-1", "sess-1", PriorityNormal, nil)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		lm.Schedule(pid)
	}
}

// BenchmarkLifecycleManager_GetNextRunnable benchmarks getting runnable processes.
func BenchmarkLifecycleManager_GetNextRunnable(b *testing.B) {
	lm := NewLifecycleManager(nil)

	// Pre-create and schedule processes
	for i := 0; i < b.N; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		lm.Submit(pid, "req-1", "user-1", "sess-1", PriorityNormal, nil)
		lm.Schedule(pid)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		lm.GetNextRunnable()
	}
}

// BenchmarkLifecycleManager_FullCycle benchmarks complete process lifecycle.
func BenchmarkLifecycleManager_FullCycle(b *testing.B) {
	lm := NewLifecycleManager(nil)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		lm.Submit(pid, "req-1", "user-1", "sess-1", PriorityNormal, nil)
		lm.Schedule(pid)
		lm.GetNextRunnable()
		lm.Terminate(pid, "done", true)
		lm.Cleanup(pid)
	}
}

// BenchmarkLifecycleManager_PriorityScheduling benchmarks priority-based scheduling.
func BenchmarkLifecycleManager_PriorityScheduling(b *testing.B) {
	priorities := []SchedulingPriority{PriorityRealtime, PriorityHigh, PriorityNormal, PriorityLow, PriorityIdle}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		lm := NewLifecycleManager(nil)

		// Submit processes with different priorities
		for j, p := range priorities {
			pid := fmt.Sprintf("pid-%d-%d", i, j)
			lm.Submit(pid, "req-1", "user-1", "sess-1", p, nil)
			lm.Schedule(pid)
		}

		// Get all runnable processes
		for range priorities {
			lm.GetNextRunnable()
		}
	}
}

// =============================================================================
// Rate Limiter Benchmarks
// =============================================================================

// BenchmarkRateLimiter_CheckRateLimit benchmarks rate limit checking.
func BenchmarkRateLimiter_CheckRateLimit(b *testing.B) {
	config := &RateLimitConfig{
		RequestsPerMinute: 10000,
		RequestsPerHour:   100000,
		RequestsPerDay:    1000000,
	}
	rl := NewRateLimiter(config)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		rl.CheckRateLimit("user-1", "/api/test", true)
	}
}

// BenchmarkRateLimiter_CheckRateLimit_NoRecord benchmarks check without recording.
func BenchmarkRateLimiter_CheckRateLimit_NoRecord(b *testing.B) {
	config := &RateLimitConfig{
		RequestsPerMinute: 10000,
		RequestsPerHour:   100000,
		RequestsPerDay:    1000000,
	}
	rl := NewRateLimiter(config)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		rl.CheckRateLimit("user-1", "/api/test", false)
	}
}

// BenchmarkRateLimiter_MultipleUsers benchmarks rate limiting with multiple users.
func BenchmarkRateLimiter_MultipleUsers(b *testing.B) {
	config := &RateLimitConfig{
		RequestsPerMinute: 10000,
		RequestsPerHour:   100000,
		RequestsPerDay:    1000000,
	}
	rl := NewRateLimiter(config)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		userID := fmt.Sprintf("user-%d", i%100)
		rl.CheckRateLimit(userID, "/api/test", true)
	}
}

// BenchmarkRateLimiter_Concurrent benchmarks concurrent rate limit checking.
func BenchmarkRateLimiter_Concurrent(b *testing.B) {
	config := &RateLimitConfig{
		RequestsPerMinute: 1000000,
		RequestsPerHour:   10000000,
		RequestsPerDay:    100000000,
	}
	rl := NewRateLimiter(config)

	b.ResetTimer()
	b.RunParallel(func(pb *testing.PB) {
		i := 0
		for pb.Next() {
			userID := fmt.Sprintf("user-%d", i%100)
			rl.CheckRateLimit(userID, "/api/test", true)
			i++
		}
	})
}

// BenchmarkSlidingWindow_Record benchmarks window recording.
func BenchmarkSlidingWindow_Record(b *testing.B) {
	window := NewSlidingWindow(60)
	now := float64(1000000)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		window.Record(now + float64(i)/1000)
	}
}

// BenchmarkSlidingWindow_GetCount benchmarks window count retrieval.
func BenchmarkSlidingWindow_GetCount(b *testing.B) {
	window := NewSlidingWindow(60)
	now := float64(1000000)

	// Pre-record some data
	for i := 0; i < 100; i++ {
		window.Record(now + float64(i)/10)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		window.GetCount(now + float64(i)/1000)
	}
}

// =============================================================================
// Resource Tracker Benchmarks
// =============================================================================

// BenchmarkResourceTracker_Allocate benchmarks resource allocation.
func BenchmarkResourceTracker_Allocate(b *testing.B) {
	tracker := NewResourceTracker(nil, nil)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		tracker.Allocate(pid, nil)
	}
}

// BenchmarkResourceTracker_RecordUsage benchmarks usage recording.
func BenchmarkResourceTracker_RecordUsage(b *testing.B) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tracker.RecordUsage("pid-1", 1, 1, 0, 100, 50)
	}
}

// BenchmarkResourceTracker_CheckQuota benchmarks quota checking.
func BenchmarkResourceTracker_CheckQuota(b *testing.B) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)
	tracker.RecordUsage("pid-1", 5, 10, 2, 500, 250)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tracker.CheckQuota("pid-1")
	}
}

// BenchmarkResourceTracker_Concurrent benchmarks concurrent resource tracking.
func BenchmarkResourceTracker_Concurrent(b *testing.B) {
	tracker := NewResourceTracker(nil, nil)

	// Pre-allocate processes
	for i := 0; i < 100; i++ {
		tracker.Allocate(fmt.Sprintf("pid-%d", i), nil)
	}

	b.ResetTimer()
	b.RunParallel(func(pb *testing.PB) {
		i := 0
		for pb.Next() {
			pid := fmt.Sprintf("pid-%d", i%100)
			tracker.RecordUsage(pid, 1, 1, 0, 100, 50)
			i++
		}
	})
}

// BenchmarkResourceTracker_GetRemainingBudget benchmarks budget calculation.
func BenchmarkResourceTracker_GetRemainingBudget(b *testing.B) {
	tracker := NewResourceTracker(nil, nil)
	tracker.Allocate("pid-1", nil)
	tracker.RecordUsage("pid-1", 5, 10, 2, 500, 250)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tracker.GetRemainingBudget("pid-1")
	}
}

// =============================================================================
// Kernel Integration Benchmarks
// =============================================================================

// BenchmarkKernel_FullWorkflow benchmarks complete kernel workflow.
func BenchmarkKernel_FullWorkflow(b *testing.B) {
	kernel := NewKernel(nil, nil)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		kernel.Submit(pid, "req-1", "user-1", "sess-1", PriorityNormal, nil)
		kernel.Schedule(pid)
		kernel.RecordLLMCall(pid, 100, 50)
		kernel.RecordToolCall(pid)
	}
}

// BenchmarkKernel_Concurrent_Submit benchmarks concurrent process submission.
func BenchmarkKernel_Concurrent_Submit(b *testing.B) {
	kernel := NewKernel(nil, nil)

	b.ResetTimer()
	b.RunParallel(func(pb *testing.PB) {
		i := 0
		for pb.Next() {
			pid := fmt.Sprintf("pid-%d-%d", b.N, i)
			kernel.Submit(pid, "req-1", "user-1", "sess-1", PriorityNormal, nil)
			i++
		}
	})
}

// =============================================================================
// Priority Queue Benchmarks
// =============================================================================

// BenchmarkPriorityQueue_PushPop benchmarks heap operations.
func BenchmarkPriorityQueue_PushPop(b *testing.B) {
	lm := NewLifecycleManager(nil)

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		lm.Submit(pid, "req-1", "user-1", "sess-1", PriorityNormal, nil)
		lm.Schedule(pid)
		lm.GetNextRunnable()
	}
}

// =============================================================================
// Concurrent Access Stress Tests
// =============================================================================

// BenchmarkLifecycleManager_ConcurrentAccess benchmarks concurrent lifecycle operations.
func BenchmarkLifecycleManager_ConcurrentAccess(b *testing.B) {
	lm := NewLifecycleManager(nil)

	// Pre-create processes
	for i := 0; i < 100; i++ {
		pid := fmt.Sprintf("pid-%d", i)
		lm.Submit(pid, "req-1", "user-1", "sess-1", PriorityNormal, nil)
	}

	b.ResetTimer()
	var wg sync.WaitGroup
	for i := 0; i < b.N; i++ {
		wg.Add(3)

		// Reader 1: GetProcess
		go func(n int) {
			defer wg.Done()
			lm.GetProcess(fmt.Sprintf("pid-%d", n%100))
		}(i)

		// Reader 2: ListProcesses
		go func() {
			defer wg.Done()
			lm.ListProcesses(nil, "")
		}()

		// Reader 3: GetProcessCount
		go func() {
			defer wg.Done()
			lm.GetProcessCount()
		}()
	}
	wg.Wait()
}
