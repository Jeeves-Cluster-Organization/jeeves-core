// Package kernel provides rate limiting using sliding window algorithm.
//
// Features:
//   - Per-user rate limits
//   - Per-endpoint rate limits
//   - Configurable time windows (minute, hour, day)
//   - Burst allowance
//   - Thread-safe implementation
package kernel

import (
	"sync"
	"time"
)

// =============================================================================
// Rate Limit Config & Result
// =============================================================================

// RateLimitConfig defines rate limiting thresholds.
type RateLimitConfig struct {
	RequestsPerMinute int `json:"requests_per_minute"`
	RequestsPerHour   int `json:"requests_per_hour"`
	RequestsPerDay    int `json:"requests_per_day"`
	BurstSize         int `json:"burst_size"`
}

// DefaultRateLimitConfig returns sensible defaults.
func DefaultRateLimitConfig() *RateLimitConfig {
	return &RateLimitConfig{
		RequestsPerMinute: 60,
		RequestsPerHour:   1000,
		RequestsPerDay:    10000,
		BurstSize:         10,
	}
}

// RateLimitResult represents the result of a rate limit check.
type RateLimitResult struct {
	Allowed    bool    `json:"allowed"`
	Exceeded   bool    `json:"exceeded"`
	LimitType  string  `json:"limit_type,omitempty"`  // "minute", "hour", "day"
	Current    int     `json:"current"`               // Current request count
	Limit      int     `json:"limit"`                 // Configured limit
	Remaining  int     `json:"remaining"`             // Remaining requests
	RetryAfter float64 `json:"retry_after,omitempty"` // Seconds until retry allowed
}

// OK creates an allowed result.
func (r *RateLimitResult) OK(remaining int) *RateLimitResult {
	return &RateLimitResult{
		Allowed:   true,
		Exceeded:  false,
		Remaining: remaining,
	}
}

// ExceededLimit creates a rate limit exceeded result.
func ExceededLimit(limitType string, current, limit int, retryAfter float64) *RateLimitResult {
	return &RateLimitResult{
		Allowed:    false,
		Exceeded:   true,
		LimitType:  limitType,
		Current:    current,
		Limit:      limit,
		Remaining:  0,
		RetryAfter: retryAfter,
	}
}

// AllowedResult creates an allowed result.
func AllowedResult(remaining int) *RateLimitResult {
	return &RateLimitResult{
		Allowed:   true,
		Exceeded:  false,
		Remaining: remaining,
	}
}

// =============================================================================
// Sliding Window
// =============================================================================

// SlidingWindow implements a sliding window counter for rate limiting.
// Uses sub-buckets for accurate sliding window calculation.
type SlidingWindow struct {
	windowSeconds int
	bucketCount   int
	buckets       map[int64]int
	totalCount    int
	mu            sync.RWMutex
}

// NewSlidingWindow creates a new sliding window.
func NewSlidingWindow(windowSeconds int) *SlidingWindow {
	return &SlidingWindow{
		windowSeconds: windowSeconds,
		bucketCount:   10,
		buckets:       make(map[int64]int),
		totalCount:    0,
	}
}

// Record records a request and returns the current count.
func (w *SlidingWindow) Record(timestamp float64) int {
	w.mu.Lock()
	defer w.mu.Unlock()

	bucketSize := float64(w.windowSeconds) / float64(w.bucketCount)
	currentBucket := int64(timestamp / bucketSize)

	// Clean up old buckets
	minBucket := currentBucket - int64(w.bucketCount)
	for b := range w.buckets {
		if b < minBucket {
			w.totalCount -= w.buckets[b]
			delete(w.buckets, b)
		}
	}

	// Record in current bucket
	w.buckets[currentBucket]++
	w.totalCount++

	return w.getCountLocked(timestamp)
}

// GetCount returns the current count in the sliding window.
func (w *SlidingWindow) GetCount(timestamp float64) int {
	w.mu.RLock()
	defer w.mu.RUnlock()
	return w.getCountLocked(timestamp)
}

// getCountLocked returns count (must hold lock).
func (w *SlidingWindow) getCountLocked(timestamp float64) int {
	bucketSize := float64(w.windowSeconds) / float64(w.bucketCount)
	currentBucket := int64(timestamp / bucketSize)
	minBucket := currentBucket - int64(w.bucketCount)

	count := 0
	for bucket, bucketCount := range w.buckets {
		if bucket >= minBucket {
			count += bucketCount
		}
	}
	return count
}

// TimeUntilSlotAvailable calculates seconds until a slot becomes available.
func (w *SlidingWindow) TimeUntilSlotAvailable(timestamp float64, limit int) float64 {
	w.mu.RLock()
	defer w.mu.RUnlock()

	if w.getCountLocked(timestamp) < limit {
		return 0.0
	}

	bucketSize := float64(w.windowSeconds) / float64(w.bucketCount)
	currentBucket := int64(timestamp / bucketSize)
	minBucket := currentBucket - int64(w.bucketCount)

	// Collect and sort valid buckets
	type bucketEntry struct {
		bucket int64
		count  int
	}
	var sortedBuckets []bucketEntry
	for b, c := range w.buckets {
		if b >= minBucket {
			sortedBuckets = append(sortedBuckets, bucketEntry{b, c})
		}
	}

	// Sort by bucket (oldest first)
	for i := 0; i < len(sortedBuckets)-1; i++ {
		for j := i + 1; j < len(sortedBuckets); j++ {
			if sortedBuckets[j].bucket < sortedBuckets[i].bucket {
				sortedBuckets[i], sortedBuckets[j] = sortedBuckets[j], sortedBuckets[i]
			}
		}
	}

	// Calculate when enough requests will expire
	excess := w.getCountLocked(timestamp) - limit + 1
	expired := 0
	for _, entry := range sortedBuckets {
		expired += entry.count
		if expired >= excess {
			bucketEnd := float64(entry.bucket+1) * bucketSize
			result := bucketEnd - timestamp + float64(w.windowSeconds)
			if result < 0 {
				return 0
			}
			return result
		}
	}

	return float64(w.windowSeconds)
}

// IsEmpty returns true if window has no activity.
func (w *SlidingWindow) IsEmpty() bool {
	w.mu.RLock()
	defer w.mu.RUnlock()
	return len(w.buckets) == 0
}

// =============================================================================
// Rate Limiter
// =============================================================================

// windowKey identifies a rate limit window.
type windowKey struct {
	userID     string
	endpoint   string
	windowType string // "minute", "hour", "day"
}

// RateLimiter implements rate limiting using sliding window algorithm.
// Thread-safe implementation supporting multiple windows per user/endpoint.
type RateLimiter struct {
	defaultConfig   *RateLimitConfig
	userConfigs     map[string]*RateLimitConfig
	endpointConfigs map[string]*RateLimitConfig
	windows         map[windowKey]*SlidingWindow
	mu              sync.RWMutex
}

// NewRateLimiter creates a new rate limiter.
func NewRateLimiter(defaultConfig *RateLimitConfig) *RateLimiter {
	if defaultConfig == nil {
		defaultConfig = DefaultRateLimitConfig()
	}
	return &RateLimiter{
		defaultConfig:   defaultConfig,
		userConfigs:     make(map[string]*RateLimitConfig),
		endpointConfigs: make(map[string]*RateLimitConfig),
		windows:         make(map[windowKey]*SlidingWindow),
	}
}

// SetDefaultConfig sets the default rate limit config.
func (r *RateLimiter) SetDefaultConfig(config *RateLimitConfig) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.defaultConfig = config
}

// SetUserLimits sets rate limits for a specific user.
func (r *RateLimiter) SetUserLimits(userID string, config *RateLimitConfig) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.userConfigs[userID] = config
}

// SetEndpointLimits sets rate limits for a specific endpoint.
// Endpoint limits override user limits for that endpoint.
func (r *RateLimiter) SetEndpointLimits(endpoint string, config *RateLimitConfig) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.endpointConfigs[endpoint] = config
}

// GetConfig returns the effective rate limit config.
func (r *RateLimiter) GetConfig(userID, endpoint string) *RateLimitConfig {
	r.mu.RLock()
	defer r.mu.RUnlock()

	// Endpoint config takes precedence
	if endpoint != "" {
		if cfg, ok := r.endpointConfigs[endpoint]; ok {
			return cfg
		}
	}

	// Then user config
	if cfg, ok := r.userConfigs[userID]; ok {
		return cfg
	}

	// Fall back to default
	return r.defaultConfig
}

// CheckRateLimit checks if a request is within rate limits.
func (r *RateLimiter) CheckRateLimit(userID, endpoint string, record bool) *RateLimitResult {
	now := float64(time.Now().UnixNano()) / 1e9 // seconds with fractional part
	config := r.GetConfig(userID, endpoint)

	r.mu.Lock()
	defer r.mu.Unlock()

	// Define window checks
	checks := []struct {
		windowType    string
		windowSeconds int
		limit         int
	}{
		{"minute", 60, config.RequestsPerMinute},
		{"hour", 3600, config.RequestsPerHour},
		{"day", 86400, config.RequestsPerDay},
	}

	// Check each window
	for _, check := range checks {
		if check.limit <= 0 {
			continue // No limit for this window
		}

		key := windowKey{userID, endpoint, check.windowType}
		window, exists := r.windows[key]
		if !exists {
			window = NewSlidingWindow(check.windowSeconds)
			r.windows[key] = window
		}

		current := window.GetCount(now)
		if current >= check.limit {
			retryAfter := window.TimeUntilSlotAvailable(now, check.limit)
			return ExceededLimit(check.windowType, current, check.limit, retryAfter)
		}
	}

	// All checks passed, record the request
	if record {
		for _, check := range checks {
			if check.limit <= 0 {
				continue
			}
			key := windowKey{userID, endpoint, check.windowType}
			if _, exists := r.windows[key]; !exists {
				r.windows[key] = NewSlidingWindow(check.windowSeconds)
			}
			r.windows[key].Record(now)
		}
	}

	// Calculate remaining for minute window
	minuteKey := windowKey{userID, endpoint, "minute"}
	remaining := config.RequestsPerMinute
	if window, exists := r.windows[minuteKey]; exists {
		remaining = config.RequestsPerMinute - window.GetCount(now)
		if remaining < 0 {
			remaining = 0
		}
	}

	return AllowedResult(remaining)
}

// GetUsage returns current rate limit usage for a user/endpoint.
func (r *RateLimiter) GetUsage(userID, endpoint string) map[string]map[string]any {
	now := float64(time.Now().UnixNano()) / 1e9
	config := r.GetConfig(userID, endpoint)
	usage := make(map[string]map[string]any)

	r.mu.RLock()
	defer r.mu.RUnlock()

	windows := []struct {
		windowType    string
		windowSeconds int
		limit         int
	}{
		{"minute", 60, config.RequestsPerMinute},
		{"hour", 3600, config.RequestsPerHour},
		{"day", 86400, config.RequestsPerDay},
	}

	for _, w := range windows {
		key := windowKey{userID, endpoint, w.windowType}
		current := 0
		if window, exists := r.windows[key]; exists {
			current = window.GetCount(now)
		}

		remaining := w.limit - current
		if remaining < 0 {
			remaining = 0
		}

		usage[w.windowType] = map[string]any{
			"current":          current,
			"limit":            w.limit,
			"remaining":        remaining,
			"reset_in_seconds": w.windowSeconds, // Approximate
		}
	}

	return usage
}

// ResetUser resets all rate limit windows for a user.
func (r *RateLimiter) ResetUser(userID string) int {
	r.mu.Lock()
	defer r.mu.Unlock()

	count := 0
	for key := range r.windows {
		if key.userID == userID {
			delete(r.windows, key)
			count++
		}
	}
	return count
}

// CleanupExpired cleans up expired window data.
// Should be called periodically to prevent memory growth.
func (r *RateLimiter) CleanupExpired() int {
	now := float64(time.Now().UnixNano()) / 1e9
	cleaned := 0

	r.mu.Lock()
	defer r.mu.Unlock()

	for key, window := range r.windows {
		if window.GetCount(now) == 0 && window.IsEmpty() {
			delete(r.windows, key)
			cleaned++
		}
	}

	return cleaned
}
