// Package config provides core orchestration configuration - NO infrastructure URLs.
//
// This module contains ONLY configuration that is relevant to core orchestration:
//   - Timeouts
//   - Limits
//   - Agent behavior toggles
//
// Infrastructure configuration (database URLs, LLM endpoints, search settings,
// embedding settings) should be in jeeves_avionics/settings.
//
// Domain/vertical configuration (NLI thresholds, etc.) should be in
// vertical-specific config modules.
//
// Phase 4 Centralization: os.Getenv removed from core_engine.
// Environment parsing happens in mission_system/bootstrap.
package config

import (
	"sync"
)

// ExecutionConfig holds core orchestration configuration.
//
// This configuration is infrastructure-agnostic and can be used
// regardless of what backends (LLM, database) are being used.
//
// Feature flags control orchestration behavior without vertical involvement.
type ExecutionConfig struct {
	// Execution Limits
	MaxPlanSteps     int `json:"max_plan_steps"`
	MaxToolRetries   int `json:"max_tool_retries"`
	MaxLLMRetries    int `json:"max_llm_retries"`
	MaxLoopIterations int `json:"max_loop_iterations"`

	// Timeouts (seconds)
	LLMTimeout      int `json:"llm_timeout"`
	ExecutorTimeout int `json:"executor_timeout"`
	ToolTimeout     int `json:"tool_timeout"`
	AgentTimeout    int `json:"agent_timeout"` // Per-agent timeout

	// Confidence Thresholds (orchestration only)
	ClarificationThreshold  float64 `json:"clarification_threshold"`  // Ask for clarification below this
	HighConfidenceThreshold float64 `json:"high_confidence_threshold"`

	// Agent Behavior
	MetaValidationEnabled bool    `json:"meta_validation_enabled"`
	MetaValidationDelay   float64 `json:"meta_validation_delay"`

	// Orchestration Feature Flags
	EnableLoopBack                    bool `json:"enable_loop_back"`                     // Allow agents to trigger loop back to earlier stages
	EnableArbiter                     bool `json:"enable_arbiter"`                       // Enable policy arbiter stage
	SkipArbiterForReadOnly            bool `json:"skip_arbiter_for_read_only"`           // Skip arbiter if all steps are read-only
	RequireConfirmationForDestructive bool `json:"require_confirmation_for_destructive"` // Force confirmation for destructive ops

	// Loop Control
	MaxReplanIterations    int  `json:"max_replan_iterations"`     // Max replans before escalating
	MaxLoopBackRejections  int  `json:"max_loop_back_rejections"`  // Max loop back rejections before forcing completion
	ReplanOnPartialSuccess bool `json:"replan_on_partial_success"` // Replan if some steps failed

	// Determinism
	LLMSeed                   *int `json:"llm_seed,omitempty"`           // Seed for LLM calls (nil = non-deterministic)
	StrictTransitionValidation bool `json:"strict_transition_validation"` // Validate pipeline transitions

	// Idempotency
	EnableIdempotency          bool `json:"enable_idempotency"`             // Enable idempotency tracking for safe retries
	IdempotencyTTLMs           int  `json:"idempotency_ttl_ms"`             // TTL for idempotency records (1 hour default)
	EnforceIdempotencyForTools  bool `json:"enforce_idempotency_for_tools"`  // Track tool executions for dedup
	EnforceIdempotencyForAgents bool `json:"enforce_idempotency_for_agents"` // Track agent executions (usually off)

	// Logging
	LogLevel string `json:"log_level"`
}

// DefaultExecutionConfig returns a ExecutionConfig with default values.
func DefaultExecutionConfig() *ExecutionConfig {
	return &ExecutionConfig{
		// Execution Limits
		MaxPlanSteps:      20,
		MaxToolRetries:    2,
		MaxLLMRetries:     3,
		MaxLoopIterations: 10,

		// Timeouts (seconds)
		LLMTimeout:      120,
		ExecutorTimeout: 60,
		ToolTimeout:     30,
		AgentTimeout:    300,

		// Confidence Thresholds
		ClarificationThreshold:  0.7,
		HighConfidenceThreshold: 0.9,

		// Agent Behavior
		MetaValidationEnabled: true,
		MetaValidationDelay:   0.5,

		// Orchestration Feature Flags
		EnableLoopBack:                    true,
		EnableArbiter:                     true,
		SkipArbiterForReadOnly:            true,
		RequireConfirmationForDestructive: true,

		// Loop Control
		MaxReplanIterations:    3,
		MaxLoopBackRejections:  2,
		ReplanOnPartialSuccess: true,

		// Determinism
		LLMSeed:                   nil,
		StrictTransitionValidation: true,

		// Idempotency
		EnableIdempotency:          true,
		IdempotencyTTLMs:           3600000, // 1 hour
		EnforceIdempotencyForTools:  true,
		EnforceIdempotencyForAgents: false,

		// Logging
		LogLevel: "INFO",
	}
}

// FromMap creates ExecutionConfig from a map.
// Unknown keys are ignored.
func ExecutionConfigFromMap(config map[string]any) *ExecutionConfig {
	c := DefaultExecutionConfig()

	if v, ok := config["max_plan_steps"].(int); ok {
		c.MaxPlanSteps = v
	} else if v, ok := config["max_plan_steps"].(float64); ok {
		c.MaxPlanSteps = int(v)
	}
	if v, ok := config["max_tool_retries"].(int); ok {
		c.MaxToolRetries = v
	} else if v, ok := config["max_tool_retries"].(float64); ok {
		c.MaxToolRetries = int(v)
	}
	if v, ok := config["max_llm_retries"].(int); ok {
		c.MaxLLMRetries = v
	} else if v, ok := config["max_llm_retries"].(float64); ok {
		c.MaxLLMRetries = int(v)
	}
	if v, ok := config["max_loop_iterations"].(int); ok {
		c.MaxLoopIterations = v
	} else if v, ok := config["max_loop_iterations"].(float64); ok {
		c.MaxLoopIterations = int(v)
	}
	if v, ok := config["llm_timeout"].(int); ok {
		c.LLMTimeout = v
	} else if v, ok := config["llm_timeout"].(float64); ok {
		c.LLMTimeout = int(v)
	}
	if v, ok := config["executor_timeout"].(int); ok {
		c.ExecutorTimeout = v
	} else if v, ok := config["executor_timeout"].(float64); ok {
		c.ExecutorTimeout = int(v)
	}
	if v, ok := config["tool_timeout"].(int); ok {
		c.ToolTimeout = v
	} else if v, ok := config["tool_timeout"].(float64); ok {
		c.ToolTimeout = int(v)
	}
	if v, ok := config["agent_timeout"].(int); ok {
		c.AgentTimeout = v
	} else if v, ok := config["agent_timeout"].(float64); ok {
		c.AgentTimeout = int(v)
	}
	if v, ok := config["clarification_threshold"].(float64); ok {
		c.ClarificationThreshold = v
	}
	if v, ok := config["high_confidence_threshold"].(float64); ok {
		c.HighConfidenceThreshold = v
	}
	if v, ok := config["meta_validation_enabled"].(bool); ok {
		c.MetaValidationEnabled = v
	}
	if v, ok := config["meta_validation_delay"].(float64); ok {
		c.MetaValidationDelay = v
	}
	if v, ok := config["enable_loop_back"].(bool); ok {
		c.EnableLoopBack = v
	}
	if v, ok := config["enable_arbiter"].(bool); ok {
		c.EnableArbiter = v
	}
	if v, ok := config["skip_arbiter_for_read_only"].(bool); ok {
		c.SkipArbiterForReadOnly = v
	}
	if v, ok := config["require_confirmation_for_destructive"].(bool); ok {
		c.RequireConfirmationForDestructive = v
	}
	if v, ok := config["max_replan_iterations"].(int); ok {
		c.MaxReplanIterations = v
	} else if v, ok := config["max_replan_iterations"].(float64); ok {
		c.MaxReplanIterations = int(v)
	}
	if v, ok := config["max_loop_back_rejections"].(int); ok {
		c.MaxLoopBackRejections = v
	} else if v, ok := config["max_loop_back_rejections"].(float64); ok {
		c.MaxLoopBackRejections = int(v)
	}
	if v, ok := config["replan_on_partial_success"].(bool); ok {
		c.ReplanOnPartialSuccess = v
	}
	if v, ok := config["llm_seed"].(int); ok {
		c.LLMSeed = &v
	} else if v, ok := config["llm_seed"].(float64); ok {
		seed := int(v)
		c.LLMSeed = &seed
	}
	if v, ok := config["strict_transition_validation"].(bool); ok {
		c.StrictTransitionValidation = v
	}
	if v, ok := config["enable_idempotency"].(bool); ok {
		c.EnableIdempotency = v
	}
	if v, ok := config["idempotency_ttl_ms"].(int); ok {
		c.IdempotencyTTLMs = v
	} else if v, ok := config["idempotency_ttl_ms"].(float64); ok {
		c.IdempotencyTTLMs = int(v)
	}
	if v, ok := config["enforce_idempotency_for_tools"].(bool); ok {
		c.EnforceIdempotencyForTools = v
	}
	if v, ok := config["enforce_idempotency_for_agents"].(bool); ok {
		c.EnforceIdempotencyForAgents = v
	}
	if v, ok := config["log_level"].(string); ok {
		c.LogLevel = v
	}

	return c
}

// ToMap converts config to a map.
func (c *ExecutionConfig) ToMap() map[string]any {
	result := map[string]any{
		"max_plan_steps":                     c.MaxPlanSteps,
		"max_tool_retries":                   c.MaxToolRetries,
		"max_llm_retries":                    c.MaxLLMRetries,
		"max_loop_iterations":                c.MaxLoopIterations,
		"llm_timeout":                        c.LLMTimeout,
		"executor_timeout":                   c.ExecutorTimeout,
		"tool_timeout":                       c.ToolTimeout,
		"agent_timeout":                      c.AgentTimeout,
		"clarification_threshold":            c.ClarificationThreshold,
		"high_confidence_threshold":          c.HighConfidenceThreshold,
		"meta_validation_enabled":            c.MetaValidationEnabled,
		"meta_validation_delay":              c.MetaValidationDelay,
		"enable_loop_back":                     c.EnableLoopBack,
		"enable_arbiter":                       c.EnableArbiter,
		"skip_arbiter_for_read_only":           c.SkipArbiterForReadOnly,
		"require_confirmation_for_destructive": c.RequireConfirmationForDestructive,
		"max_replan_iterations":                c.MaxReplanIterations,
		"max_loop_back_rejections":             c.MaxLoopBackRejections,
		"replan_on_partial_success":          c.ReplanOnPartialSuccess,
		"strict_transition_validation":       c.StrictTransitionValidation,
		"enable_idempotency":                 c.EnableIdempotency,
		"idempotency_ttl_ms":                 c.IdempotencyTTLMs,
		"enforce_idempotency_for_tools":      c.EnforceIdempotencyForTools,
		"enforce_idempotency_for_agents":     c.EnforceIdempotencyForAgents,
		"log_level":                          c.LogLevel,
	}
	if c.LLMSeed != nil {
		result["llm_seed"] = *c.LLMSeed
	}
	return result
}

// =============================================================================
// CONFIG PROVIDER INTERFACE (Dependency Injection)
// =============================================================================

// ConfigProvider provides configuration values.
// Use this interface for dependency injection instead of global state.
type ConfigProvider interface {
	// GetExecutionConfig returns the core configuration.
	GetExecutionConfig() *ExecutionConfig
}

// DefaultConfigProvider provides the global configuration.
// This is the default implementation that uses the global singleton.
type DefaultConfigProvider struct{}

// GetExecutionConfig returns the global core configuration.
func (p *DefaultConfigProvider) GetExecutionConfig() *ExecutionConfig {
	return GetGlobalExecutionConfig()
}

// StaticConfigProvider provides a static configuration.
// Useful for testing with specific config values.
type StaticConfigProvider struct {
	Config *ExecutionConfig
}

// GetExecutionConfig returns the static configuration.
func (p *StaticConfigProvider) GetExecutionConfig() *ExecutionConfig {
	if p.Config == nil {
		return DefaultExecutionConfig()
	}
	return p.Config
}

// NewStaticConfigProvider creates a new StaticConfigProvider.
func NewStaticConfigProvider(config *ExecutionConfig) *StaticConfigProvider {
	return &StaticConfigProvider{Config: config}
}

// =============================================================================
// GLOBAL CONFIG (set by mission_system bootstrap)
// =============================================================================

var (
	globalExecutionConfig *ExecutionConfig
	configMu         sync.RWMutex
)

// GetGlobalExecutionConfig gets the global core configuration instance.
// Returns the injected config or defaults.
// Prefer using ConfigProvider interface for new code.
func GetGlobalExecutionConfig() *ExecutionConfig {
	configMu.RLock()
	defer configMu.RUnlock()

	if globalExecutionConfig == nil {
		return DefaultExecutionConfig()
	}
	return globalExecutionConfig
}

// GetExecutionConfig gets the core configuration instance.
// Deprecated: Use GetGlobalExecutionConfig or inject ConfigProvider for testability.
func GetExecutionConfig() *ExecutionConfig {
	return GetGlobalExecutionConfig()
}

// SetExecutionConfig sets the core configuration instance.
// Called by mission_system bootstrap after parsing environment variables.
func SetExecutionConfig(config *ExecutionConfig) {
	configMu.Lock()
	defer configMu.Unlock()

	globalExecutionConfig = config
}

// ResetExecutionConfig resets core config to nil (useful for testing).
// After reset, GetExecutionConfig() will return defaults.
func ResetExecutionConfig() {
	configMu.Lock()
	defer configMu.Unlock()

	globalExecutionConfig = nil
}
