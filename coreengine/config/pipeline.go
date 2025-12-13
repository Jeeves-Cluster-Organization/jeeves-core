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

// AgentConfig is the declarative agent configuration.
// Enables capability layer to define agents without writing classes.
type AgentConfig struct {
	// Identity
	Name       string `json:"name"`        // Unique agent name
	StageOrder int    `json:"stage_order"` // Execution order in pipeline (for backward compat)

	// === DAG Dependencies (systemd-style) ===
	// Requires: Hard dependencies - these stages MUST complete successfully before this runs
	// Similar to systemd's Requires= directive
	Requires []string `json:"requires,omitempty"`

	// After: Soft ordering - run after these stages IF they are in the pipeline
	// Does not create a hard dependency, just ordering preference
	// Similar to systemd's After= directive
	After []string `json:"after,omitempty"`

	// RunsWith: Parallelization hint - can run concurrently with these stages
	// Stages in the same RunsWith group may execute in parallel
	RunsWith []string `json:"runs_with,omitempty"`

	// JoinStrategy: How to handle multiple Requires dependencies
	// "all" (default) = wait for ALL prerequisites
	// "any" = proceed when ANY prerequisite completes (race)
	JoinStrategy JoinStrategy `json:"join_strategy,omitempty"`

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

	// Global Configuration
	MaxIterations         int `json:"max_iterations"`          // Max pipeline loop iterations
	MaxLLMCalls           int `json:"max_llm_calls"`           // Max total LLM calls
	MaxAgentHops          int `json:"max_agent_hops"`          // Max agent transitions
	DefaultTimeoutSeconds int `json:"default_timeout_seconds"` // Default agent timeout

	// Feature Flags
	EnableArbiter          bool `json:"enable_arbiter"`            // Whether to include arbiter
	SkipArbiterForReadOnly bool `json:"skip_arbiter_for_read_only"` // Skip for read-only tools

	// DAG Execution Mode
	// When true, uses DAG-based parallel execution instead of sequential
	// Requires agents to have Requires/After dependencies defined
	EnableDAGExecution bool `json:"enable_dag_execution,omitempty"`

	// Computed at validation time (internal)
	topologicalOrder []string          // Computed topological sort
	adjacencyList    map[string][]string // stage -> stages that depend on it
}

// NewPipelineConfig creates a new pipeline config with defaults.
func NewPipelineConfig(name string) *PipelineConfig {
	return &PipelineConfig{
		Name:                   name,
		Agents:                 make([]*AgentConfig, 0),
		MaxIterations:          3,
		MaxLLMCalls:            10,
		MaxAgentHops:           21,
		DefaultTimeoutSeconds:  300,
		EnableArbiter:          true,
		SkipArbiterForReadOnly: true,
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

	// Validate DAG dependencies if DAG execution is enabled
	if p.EnableDAGExecution {
		if err := p.validateDAG(names); err != nil {
			return err
		}
	}

	return nil
}

// validateDAG validates DAG dependencies and computes topological order.
func (p *PipelineConfig) validateDAG(validNames map[string]bool) error {
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
		for _, dep := range agent.RunsWith {
			if !validNames[dep] {
				return fmt.Errorf("agent '%s' runs_with unknown stage '%s'", agent.Name, dep)
			}
		}
	}

	// Build adjacency list (dependency graph)
	// For each stage, track which stages depend on it
	p.adjacencyList = make(map[string][]string)
	inDegree := make(map[string]int)

	for _, agent := range p.Agents {
		p.adjacencyList[agent.Name] = []string{}
		inDegree[agent.Name] = 0
	}

	for _, agent := range p.Agents {
		for _, dep := range agent.GetAllDependencies() {
			p.adjacencyList[dep] = append(p.adjacencyList[dep], agent.Name)
			inDegree[agent.Name]++
		}
	}

	// Kahn's algorithm for topological sort + cycle detection
	queue := make([]string, 0)
	for name, degree := range inDegree {
		if degree == 0 {
			queue = append(queue, name)
		}
	}

	p.topologicalOrder = make([]string, 0, len(p.Agents))

	for len(queue) > 0 {
		// Pop from queue
		current := queue[0]
		queue = queue[1:]
		p.topologicalOrder = append(p.topologicalOrder, current)

		// Reduce in-degree of dependents
		for _, dependent := range p.adjacencyList[current] {
			inDegree[dependent]--
			if inDegree[dependent] == 0 {
				queue = append(queue, dependent)
			}
		}
	}

	// If we didn't process all nodes, there's a cycle
	if len(p.topologicalOrder) != len(p.Agents) {
		// Find nodes involved in cycle
		cycleNodes := []string{}
		for name, degree := range inDegree {
			if degree > 0 {
				cycleNodes = append(cycleNodes, name)
			}
		}
		return fmt.Errorf("dependency cycle detected involving stages: %v", cycleNodes)
	}

	return nil
}

// GetTopologicalOrder returns the computed topological order.
// Returns nil if DAG execution is not enabled or validation hasn't run.
func (p *PipelineConfig) GetTopologicalOrder() []string {
	return p.topologicalOrder
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
