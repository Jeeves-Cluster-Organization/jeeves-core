package kernel

import (
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestDefaultCleanupConfig(t *testing.T) {
	cfg := DefaultCleanupConfig()

	assert.Equal(t, 5*time.Minute, cfg.Interval)
	assert.Equal(t, 24*time.Hour, cfg.ProcessRetention)
	assert.Equal(t, 1*time.Hour, cfg.SessionRetention)
	assert.Equal(t, 1*time.Hour, cfg.RateLimiterRetention)
}

func TestKernel_StartCleanupLoop(t *testing.T) {
	logger := &testLogger{}
	k := NewKernel(logger, nil)

	// Use a very short interval for testing
	cfg := CleanupConfig{
		Interval:         10 * time.Millisecond,
		ProcessRetention: 1 * time.Millisecond,
		SessionRetention: 1 * time.Millisecond,
	}

	// Start the cleanup loop
	stop := k.StartCleanupLoop(cfg)
	require.NotNil(t, stop)

	// Wait for at least one cleanup cycle
	time.Sleep(50 * time.Millisecond)

	// Stop the loop
	stop()

	// Verify logger was called (cleanup_cycle_completed)
	logger.mu.Lock()
	found := false
	for _, log := range logger.logs {
		if strings.Contains(log, "cleanup_cycle_completed") {
			found = true
			break
		}
	}
	logger.mu.Unlock()
	assert.True(t, found, "expected cleanup_cycle_completed log entry")
}

func TestKernel_StartCleanupLoop_DefaultConfig(t *testing.T) {
	logger := &testLogger{}
	k := NewKernel(logger, nil)

	// Use zero config to get defaults
	cfg := CleanupConfig{}

	stop := k.StartCleanupLoop(cfg)
	require.NotNil(t, stop)

	// Stop immediately - just testing that defaults work
	stop()
}

func TestKernel_runCleanupCycle(t *testing.T) {
	logger := &testLogger{}
	k := NewKernel(logger, nil)

	cfg := CleanupConfig{
		Interval:         time.Minute,
		ProcessRetention: 1 * time.Millisecond,
		SessionRetention: 1 * time.Millisecond,
	}

	// Create a terminated process
	pcb, err := k.Submit("test-pid", "req-1", "user-1", "session-1", PriorityNormal, nil)
	require.NoError(t, err)
	require.NotNil(t, pcb)

	// Terminate the process
	err = k.Terminate("test-pid", "test", true)
	require.NoError(t, err)

	// Wait for retention period to pass
	time.Sleep(5 * time.Millisecond)

	// Run cleanup
	k.runCleanupCycle(cfg)

	// Process should be cleaned up
	assert.Nil(t, k.GetProcess("test-pid"))
}

func TestKernel_runCleanupCycle_PanicRecovery(t *testing.T) {
	logger := &testLogger{}
	k := NewKernel(logger, nil)

	// Create a kernel with nil orchestrator to trigger potential panic
	k.orchestrator = nil

	cfg := CleanupConfig{
		Interval:         time.Minute,
		ProcessRetention: time.Hour,
		SessionRetention: time.Hour,
	}

	// This should not panic
	assert.NotPanics(t, func() {
		k.runCleanupCycle(cfg)
	})
}

func TestLifecycleManager_CleanupTerminated(t *testing.T) {
	lm := NewLifecycleManager(nil)

	// Create and terminate a process
	pcb, _ := lm.Submit("pid-1", "req-1", "user-1", "session-1", PriorityNormal, nil)
	require.NotNil(t, pcb)

	err := lm.Terminate("pid-1", "test", true)
	require.NoError(t, err)

	// Set CompletedAt to past
	pcb.CompletedAt = timePtr(time.Now().UTC().Add(-2 * time.Hour))

	// Create another active process
	_, _ = lm.Submit("pid-2", "req-2", "user-2", "session-2", PriorityNormal, nil)

	// Cleanup with 1 hour retention
	cleaned := lm.CleanupTerminated(1 * time.Hour)

	assert.Equal(t, 1, cleaned)
	assert.Nil(t, lm.GetProcess("pid-1"))
	assert.NotNil(t, lm.GetProcess("pid-2"))
}

func TestLifecycleManager_CleanupTerminated_NoCompletedAt(t *testing.T) {
	lm := NewLifecycleManager(nil)

	// Create and terminate a process without CompletedAt
	pcb, _ := lm.Submit("pid-1", "req-1", "user-1", "session-1", PriorityNormal, nil)
	require.NotNil(t, pcb)

	// Manually set state without CompletedAt
	pcb.State = ProcessStateTerminated
	pcb.CompletedAt = nil

	// Cleanup should not affect it (no CompletedAt)
	cleaned := lm.CleanupTerminated(1 * time.Millisecond)

	assert.Equal(t, 0, cleaned)
	assert.NotNil(t, lm.GetProcess("pid-1"))
}

func TestLifecycleManager_CleanupTerminated_RecentTermination(t *testing.T) {
	lm := NewLifecycleManager(nil)

	// Create and terminate a process
	pcb, _ := lm.Submit("pid-1", "req-1", "user-1", "session-1", PriorityNormal, nil)
	require.NotNil(t, pcb)

	err := lm.Terminate("pid-1", "test", true)
	require.NoError(t, err)

	// Cleanup with long retention should not remove recent termination
	cleaned := lm.CleanupTerminated(24 * time.Hour)

	assert.Equal(t, 0, cleaned)
	assert.NotNil(t, lm.GetProcess("pid-1"))
}

func TestOrchestrator_CleanupStaleSessions(t *testing.T) {
	logger := &testLogger{}
	orch := NewOrchestrator(nil, logger)

	// Manually add a stale session
	orch.mu.Lock()
	orch.sessions["stale-session"] = &OrchestrationSession{
		ProcessID:      "stale-session",
		Terminated:     false,
		LastActivityAt: time.Now().UTC().Add(-2 * time.Hour),
	}
	orch.sessions["active-session"] = &OrchestrationSession{
		ProcessID:      "active-session",
		Terminated:     false,
		LastActivityAt: time.Now().UTC(),
	}
	orch.sessions["terminated-session"] = &OrchestrationSession{
		ProcessID:      "terminated-session",
		Terminated:     true,
		LastActivityAt: time.Now().UTC(),
	}
	orch.mu.Unlock()

	// Cleanup with 1 hour retention
	cleaned := orch.CleanupStaleSessions(1 * time.Hour)

	assert.Equal(t, 2, cleaned) // stale + terminated

	orch.mu.RLock()
	_, staleExists := orch.sessions["stale-session"]
	_, activeExists := orch.sessions["active-session"]
	_, terminatedExists := orch.sessions["terminated-session"]
	orch.mu.RUnlock()

	assert.False(t, staleExists, "stale session should be cleaned")
	assert.True(t, activeExists, "active session should remain")
	assert.False(t, terminatedExists, "terminated session should be cleaned")
}

func TestOrchestrator_CleanupStaleSessions_Empty(t *testing.T) {
	orch := NewOrchestrator(nil, nil)

	cleaned := orch.CleanupStaleSessions(1 * time.Hour)

	assert.Equal(t, 0, cleaned)
}

func TestCleanupLoop_MultipleCycles(t *testing.T) {
	logger := &testLogger{}
	k := NewKernel(logger, nil)

	var cycleCount atomic.Int32

	// Use very short interval
	cfg := CleanupConfig{
		Interval:         5 * time.Millisecond,
		ProcessRetention: time.Hour,
		SessionRetention: time.Hour,
	}

	stop := k.StartCleanupLoop(cfg)

	// Wait for multiple cycles
	time.Sleep(30 * time.Millisecond)
	stop()

	// Count cleanup_cycle_completed entries
	logger.mu.Lock()
	for _, log := range logger.logs {
		if strings.Contains(log, "cleanup_cycle_completed") {
			cycleCount.Add(1)
		}
	}
	logger.mu.Unlock()

	// Should have run multiple cycles
	assert.GreaterOrEqual(t, int(cycleCount.Load()), 2)
}

// Helper function
func timePtr(t time.Time) *time.Time {
	return &t
}
