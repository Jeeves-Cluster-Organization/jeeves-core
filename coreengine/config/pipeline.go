// Package config provides pipeline and agent configuration for Go-native orchestration.
package config

import (
	"fmt"
	"sort"
)

// ToolAccess represents tool access levels for agents.
type ToolAccess string

const (
	ToolAccessNone  ToolAccess = "none"  // No tool access (pure reasoning)
	ToolAccessRead  ToolAccess = "read"  // Read-only tools
	ToolAccessWrite ToolAccess = "write" // Write-only tools
	ToolAccessAll   ToolAccess = "all"   // All tools (per arbiter approval)
)

// AgentCapability represents capability flags for agents.
type AgentCapability string

const (
	CapabilityLLM      AgentCapability = "llm"      // Agent uses LLM for reasoning
	CapabilityTools    AgentCapability = "tools"    // Agent executes tools
	CapabilityPolicies AgentCapability = "policies" // Agent applies policy rules
	CapabilityService  AgentCapability = "service"  // Agent calls internal services
)

// RoutingRule defines conditional transitions between stages.
type RoutingRule struct {
	Condition string `json:"condition"` // Key in agent output to check
	Value     any    `json:"value"`     // Expected value
	Target    string `json:"target"`    // Next stage to route to
}

// JoinStrategy defines how to handle multiple prerequisites.
type JoinStrategy string

const (
	JoinAll JoinStrategy = "all" // Wait for ALL prerequisites (default)
	JoinAny JoinStrategy = "any" // Proceed when ANY prerequisite completes
)

// EdgeLimit configures per-edge transition limits.
// Capability layer defines these to control how many times
// a specific transition (e.g., critic -> intent) can occur.
// Core doesn't judge graph structure - capability defines it.
type EdgeLimit struct {
	From     string `json:"from"`      // Source stage
	To       string `json:"to"`        // Target stage
	MaxCount int    `json:"max_count"` // Maximum transitions on this edge (0 = use global)
}

// AgentConfig is the declarative agent configuration.
// Enables capability layer to define agents without writing classes.
type AgentConfig struct {
	// Identity
	Name       string `json:"name"`        // Unique agent name
	StageOrder int    `json:"stage_order"` // Execution order in pipeline (for backward compat)

	// Dependencies
	// Requires: Hard dependencies - these stages MUST complete successfully before this runs
	Requires []string `json:"requires,omitempty"`

	// After: Soft ordering - run after these stages IF they are in the pipeline
	After []string `json:"after,omitempty"`

	// Capability Flags
	HasLLM      bool `json:"has_llm"`      // Whether agent needs LLM provider
	HasTools    bool `json:"has_tools"`    // Whether agent executes tools
	HasPolicies bool `json:"has_policies"` // Whether agent applies policy rules

	// Tool Access
	ToolAccess   ToolAccess      `json:"tool_access"`   // Tool access level
	AllowedTools map[string]bool `json:"allowed_tools"` // Tool whitelist (nil = all per access level)

	// LLM Configuration
	ModelRole   string   `json:"model_role"`   // Role for LLM provider factory
	PromptKey   string   `json:"prompt_key"`   // Prompt registry key
	Temperature *float64 `json:"temperature"`  // LLM temperature override
	MaxTokens   *int     `json:"max_tokens"`   // Max tokens for LLM response

	// Output Configuration
	OutputKey            string   `json:"output_key"`             // Key in envelope.outputs
	RequiredOutputFields []string `json:"required_output_fields"` // Fields that must be present

	// Routing Configuration
	RoutingRules []RoutingRule `json:"routing_rules"` // Conditional routing rules
	DefaultNext  string        `json:"default_next"`  // Default next stage
	ErrorNext    string        `json:"error_next"`    // Stage to route to on error

	// Bounds
	TimeoutSeconds int `json:"timeout_seconds"` // Agent-specific timeout
	MaxRetries     int `json:"max_retries"`     // Max retries on failure
}

// Validate validates the agent configuration.
func (c *AgentConfig) Validate() error {
	if c.Name == "" {
		return fmt.Errorf("AgentConfig.Name is required")
	}
	if c.OutputKey == "" {
		c.OutputKey = c.Name // Default to agent name
	}
	if c.HasLLM && c.ModelRole == "" {
		return fmt.Errorf("agent '%s' has_llm=true but no model_role", c.Name)
	}
	// Default join strategy
	if c.JoinStrategy == "" {
		c.JoinStrategy = JoinAll
	}
	return nil
}

// GetAllDependencies returns all stages this agent depends on (Requires + After).
func (c *AgentConfig) GetAllDependencies() []string {
	deps := make([]string, 0, len(c.Requires)+len(c.After))
	deps = append(deps, c.Requires...)
	deps = append(deps, c.After...)
	return deps
}

// HasDependencies returns true if agent has any dependencies.
func (c *AgentConfig) HasDependencies() bool {
	return len(c.Requires) > 0 || len(c.After) > 0
}

// PipelineConfig defines an ordered sequence of agents.
type PipelineConfig struct {
	Name   string         `json:"name"`   // Pipeline name for logging/metrics
	Agents []*AgentConfig `json:"agents"` // Ordered list of agent configs

	// Bounds - Go enforces these authoritatively
	MaxIterations         int `json:"max_iterations"`          // Max pipeline loop iterations
	MaxLLMCalls           int `json:"max_llm_calls"`           // Max total LLM calls
	MaxAgentHops          int `json:"max_agent_hops"`          // Max agent transitions
	DefaultTimeoutSeconds int `json:"default_timeout_seconds"` // Default agent timeout

	// EdgeLimits defines per-edge cycle limits.
	// Cycles ARE allowed - this is REINTENT architecture.
	// Example: {From: "critic", To: "intent", MaxCount: 3} limits REINTENT to 3 loops.
	EdgeLimits []EdgeLimit `json:"edge_limits,omitempty"`

	// Computed at validation time (internal)
	adjacencyList map[string][]string // stage -> stages that depend on it
	edgeLimitMap  map[string]int      // "from->to" -> max count
}

// NewPipelineConfig creates a new pipeline config with defaults.
// Cycles ARE allowed - this is REINTENT architecture.
func NewPipelineConfig(name string) *PipelineConfig {
	return &PipelineConfig{
		Name:                  name,
		Agents:                make([]*AgentConfig, 0),
		MaxIterations:         3,
		MaxLLMCalls:           10,
		MaxAgentHops:          21,
		DefaultTimeoutSeconds: 300,
		EdgeLimits:            []EdgeLimit{},
	}
}

// AddAgent adds an agent to the pipeline.
func (p *PipelineConfig) AddAgent(agent *AgentConfig) error {
	if err := agent.Validate(); err != nil {
		return err
	}
	p.Agents = append(p.Agents, agent)
	return nil
}

// Validate validates the pipeline configuration.
func (p *PipelineConfig) Validate() error {
	if p.Name == "" {
		return fmt.Errorf("PipelineConfig.Name is required")
	}

	// Sort agents by stage_order (for backward compatibility)
	sort.Slice(p.Agents, func(i, j int) bool {
		return p.Agents[i].StageOrder < p.Agents[j].StageOrder
	})

	// Validate unique names and build name set
	names := make(map[string]bool)
	for _, agent := range p.Agents {
		if err := agent.Validate(); err != nil {
			return err
		}
		if names[agent.Name] {
			return fmt.Errorf("duplicate agent name: %s", agent.Name)
		}
		names[agent.Name] = true
	}

	// Validate routing targets exist
	validTargets := make(map[string]bool)
	for _, agent := range p.Agents {
		validTargets[agent.Name] = true
	}
	validTargets["end"] = true
	validTargets["clarification"] = true
	validTargets["confirmation"] = true

	for _, agent := range p.Agents {
		for _, rule := range agent.RoutingRules {
			if !validTargets[rule.Target] {
				return fmt.Errorf("agent '%s' routes to unknown target '%s'", agent.Name, rule.Target)
			}
		}
		if agent.DefaultNext != "" && !validTargets[agent.DefaultNext] {
			return fmt.Errorf("agent '%s' default_next '%s' not found", agent.Name, agent.DefaultNext)
		}
	}

	// Build dependency graph and edge limits
	if err := p.buildGraph(names); err != nil {
		return err
	}

	return nil
}

// buildGraph builds the dependency graph and edge limit map.
// Core doesn't validate graph structure - capability defines it.
// If capability creates cycles, edge limits bound them at runtime.
func (p *PipelineConfig) buildGraph(validNames map[string]bool) error {
	// Validate all dependencies reference valid stages
	for _, agent := range p.Agents {
		for _, dep := range agent.Requires {
			if !validNames[dep] {
				return fmt.Errorf("agent '%s' requires unknown stage '%s'", agent.Name, dep)
			}
			if dep == agent.Name {
				return fmt.Errorf("agent '%s' cannot require itself", agent.Name)
			}
		}
		for _, dep := range agent.After {
			if !validNames[dep] {
				return fmt.Errorf("agent '%s' after unknown stage '%s'", agent.Name, dep)
			}
			if dep == agent.Name {
				return fmt.Errorf("agent '%s' cannot be after itself", agent.Name)
			}
		}
	}

	// Build adjacency list
	p.adjacencyList = make(map[string][]string)
	for _, agent := range p.Agents {
		p.adjacencyList[agent.Name] = []string{}
	}
	for _, agent := range p.Agents {
		for _, dep := range agent.GetAllDependencies() {
			p.adjacencyList[dep] = append(p.adjacencyList[dep], agent.Name)
		}
	}

	// Build edge limit map
	p.edgeLimitMap = make(map[string]int)
	for _, limit := range p.EdgeLimits {
		key := limit.From + "->" + limit.To
		p.edgeLimitMap[key] = limit.MaxCount
	}

	return nil
}

// GetEdgeLimit returns the cycle limit for an edge, or 0 if no limit.
func (p *PipelineConfig) GetEdgeLimit(from, to string) int {
	if p.edgeLimitMap == nil {
		return 0
	}
	return p.edgeLimitMap[from+"->"+to]
}

// GetReadyStages returns stages that are ready to execute given completed stages.
// A stage is ready if all its Requires dependencies are satisfied.
func (p *PipelineConfig) GetReadyStages(completed map[string]bool) []string {
	ready := make([]string, 0)

	for _, agent := range p.Agents {
		if completed[agent.Name] {
			continue // Already completed
		}

		// Check if all Requires are satisfied
		requiresSatisfied := true
		for _, req := range agent.Requires {
			if !completed[req] {
				requiresSatisfied = false
				break
			}
		}

		// Check if all After (soft ordering) are satisfied
		afterSatisfied := true
		for _, after := range agent.After {
			if !completed[after] {
				afterSatisfied = false
				break
			}
		}

		// For JoinAny, only need one Require satisfied (if any exist)
		if agent.JoinStrategy == JoinAny && len(agent.Requires) > 0 {
			requiresSatisfied = false
			for _, req := range agent.Requires {
				if completed[req] {
					requiresSatisfied = true
					break
				}
			}
		}

		if requiresSatisfied && afterSatisfied {
			ready = append(ready, agent.Name)
		}
	}

	return ready
}

// GetDependents returns stages that depend on the given stage.
func (p *PipelineConfig) GetDependents(stageName string) []string {
	if p.adjacencyList == nil {
		return nil
	}
	return p.adjacencyList[stageName]
}

// GetAgent gets an agent config by name.
func (p *PipelineConfig) GetAgent(name string) *AgentConfig {
	for _, agent := range p.Agents {
		if agent.Name == name {
			return agent
		}
	}
	return nil
}

// GetStageOrder returns ordered list of agent names.
func (p *PipelineConfig) GetStageOrder() []string {
	order := make([]string, len(p.Agents))
	for i, agent := range p.Agents {
		order[i] = agent.Name
	}
	return order
}
