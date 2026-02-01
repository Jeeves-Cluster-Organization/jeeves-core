// Package kernel provides background cleanup for resource management.
//
// CleanupLoop periodically cleans up:
//   - Terminated processes (configurable retention)
//   - Stale orchestration sessions (configurable retention)
//   - Expired rate limit windows
//   - Resolved interrupts
package kernel

import (
	"time"
)

// CleanupConfig holds configurable cleanup parameters.
type CleanupConfig struct {
	// Interval is how often to run cleanup (default: 5 minutes).
	Interval time.Duration
	// ProcessRetention is how long to keep terminated processes (default: 24 hours).
	ProcessRetention time.Duration
	// SessionRetention is how long to keep stale sessions (default: 1 hour).
	SessionRetention time.Duration
	// RateLimiterRetention is how long to keep empty rate windows (default: 1 hour).
	RateLimiterRetention time.Duration
}

// DefaultCleanupConfig returns default cleanup configuration.
func DefaultCleanupConfig() CleanupConfig {
	return CleanupConfig{
		Interval:             5 * time.Minute,
		ProcessRetention:     24 * time.Hour,
		SessionRetention:     1 * time.Hour,
		RateLimiterRetention: 1 * time.Hour,
	}
}

// StartCleanupLoop starts a background goroutine that periodically performs cleanup.
// Returns a stop function that should be called to stop the cleanup loop.
func (k *Kernel) StartCleanupLoop(cfg CleanupConfig) func() {
	if cfg.Interval == 0 {
		cfg = DefaultCleanupConfig()
	}

	ticker := time.NewTicker(cfg.Interval)
	done := make(chan struct{})

	go func() {
		for {
			select {
			case <-ticker.C:
				k.runCleanupCycle(cfg)
			case <-done:
				ticker.Stop()
				return
			}
		}
	}()

	return func() { close(done) }
}

// runCleanupCycle performs a single cleanup cycle with panic recovery.
func (k *Kernel) runCleanupCycle(cfg CleanupConfig) {
	defer func() {
		if r := recover(); r != nil {
			if k.logger != nil {
				k.logger.Error("cleanup_panic_recovered", "error", r)
			}
		}
	}()

	// Clean up terminated processes older than retention period
	processCount := k.lifecycle.CleanupTerminated(cfg.ProcessRetention)

	// Clean up stale orchestration sessions
	sessionCount := 0
	if k.orchestrator != nil {
		sessionCount = k.orchestrator.CleanupStaleSessions(cfg.SessionRetention)
	}

	// Clean up expired rate limit windows
	k.rateLimiter.CleanupExpired()

	// Clean up resolved interrupts
	k.interrupts.CleanupResolved(cfg.SessionRetention)

	if k.logger != nil {
		k.logger.Debug("cleanup_cycle_completed",
			"processes_cleaned", processCount,
			"sessions_cleaned", sessionCount,
		)
	}
}
