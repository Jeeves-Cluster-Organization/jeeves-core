package config

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// AgentConfig Tests
// =============================================================================

func TestAgentConfigValidate(t *testing.T) {
	t.Run("valid minimal config", func(t *testing.T) {
		agent := &AgentConfig{
			Name: "test-agent",
		}
		err := agent.Validate()
		require.NoError(t, err)
		assert.Equal(t, "test-agent", agent.OutputKey) // Should default to name
	})

	t.Run("missing name", func(t *testing.T) {
		agent := &AgentConfig{}
		err := agent.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "Name is required")
	})

	t.Run("has_llm but no model_role", func(t *testing.T) {
		agent := &AgentConfig{
			Name:   "llm-agent",
			HasLLM: true,
		}
		err := agent.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "has_llm=true but no model_role")
	})

	t.Run("has_llm with model_role", func(t *testing.T) {
		agent := &AgentConfig{
			Name:      "llm-agent",
			HasLLM:    true,
			ModelRole: "default",
		}
		err := agent.Validate()
		require.NoError(t, err)
	})

	t.Run("default join strategy", func(t *testing.T) {
		agent := &AgentConfig{
			Name: "test-agent",
		}
		err := agent.Validate()
		require.NoError(t, err)
		assert.Equal(t, JoinAll, agent.JoinStrategy)
	})

	t.Run("preserves existing output_key", func(t *testing.T) {
		agent := &AgentConfig{
			Name:      "test-agent",
			OutputKey: "custom_output",
		}
		err := agent.Validate()
		require.NoError(t, err)
		assert.Equal(t, "custom_output", agent.OutputKey)
	})
}

func TestAgentConfigGetAllDependencies(t *testing.T) {
	agent := &AgentConfig{
		Name:     "test-agent",
		Requires: []string{"stage-a", "stage-b"},
		After:    []string{"stage-c"},
	}

	deps := agent.GetAllDependencies()
	assert.Len(t, deps, 3)
	assert.Contains(t, deps, "stage-a")
	assert.Contains(t, deps, "stage-b")
	assert.Contains(t, deps, "stage-c")
}

func TestAgentConfigGetAllDependenciesEmpty(t *testing.T) {
	agent := &AgentConfig{
		Name: "test-agent",
	}

	deps := agent.GetAllDependencies()
	assert.Empty(t, deps)
}

func TestAgentConfigHasDependencies(t *testing.T) {
	t.Run("no dependencies", func(t *testing.T) {
		agent := &AgentConfig{Name: "test"}
		assert.False(t, agent.HasDependencies())
	})

	t.Run("has requires", func(t *testing.T) {
		agent := &AgentConfig{Name: "test", Requires: []string{"a"}}
		assert.True(t, agent.HasDependencies())
	})

	t.Run("has after", func(t *testing.T) {
		agent := &AgentConfig{Name: "test", After: []string{"a"}}
		assert.True(t, agent.HasDependencies())
	})
}

// =============================================================================
// PipelineConfig Tests
// =============================================================================

func TestNewPipelineConfig(t *testing.T) {
	p := NewPipelineConfig("test-pipeline")

	assert.Equal(t, "test-pipeline", p.Name)
	assert.NotNil(t, p.Agents)
	assert.Empty(t, p.Agents)
	assert.Equal(t, 3, p.MaxIterations)
	assert.Equal(t, 10, p.MaxLLMCalls)
	assert.Equal(t, 21, p.MaxAgentHops)
	assert.Equal(t, 300, p.DefaultTimeoutSeconds)
	assert.NotNil(t, p.EdgeLimits)
}

func TestPipelineConfigAddAgent(t *testing.T) {
	p := NewPipelineConfig("test")

	t.Run("valid agent", func(t *testing.T) {
		err := p.AddAgent(&AgentConfig{Name: "agent-1", StageOrder: 1})
		require.NoError(t, err)
		assert.Len(t, p.Agents, 1)
	})

	t.Run("invalid agent", func(t *testing.T) {
		err := p.AddAgent(&AgentConfig{HasLLM: true}) // Missing name and model_role
		require.Error(t, err)
	})
}

func TestPipelineConfigValidate(t *testing.T) {
	t.Run("valid minimal pipeline", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1", StageOrder: 1},
			{Name: "agent-2", StageOrder: 2},
		}
		err := p.Validate()
		require.NoError(t, err)
	})

	t.Run("missing name", func(t *testing.T) {
		p := &PipelineConfig{}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "Name is required")
	})

	t.Run("duplicate agent names", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1"},
			{Name: "agent-1"}, // Duplicate
		}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "duplicate agent name")
	})

	t.Run("invalid routing target", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{
				Name:         "agent-1",
				RoutingRules: []RoutingRule{{Target: "nonexistent"}},
			},
		}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "unknown target 'nonexistent'")
	})

	t.Run("valid routing to special targets", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{
				Name: "agent-1",
				RoutingRules: []RoutingRule{
					{Target: "end"},
					{Target: "clarification"},
					{Target: "confirmation"},
				},
			},
		}
		err := p.Validate()
		require.NoError(t, err)
	})

	t.Run("invalid default_next", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1", DefaultNext: "nonexistent"},
		}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "default_next 'nonexistent' not found")
	})

	t.Run("valid routing to another agent", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1", RoutingRules: []RoutingRule{{Target: "agent-2"}}},
			{Name: "agent-2"},
		}
		err := p.Validate()
		require.NoError(t, err)
	})

	t.Run("invalid agent config", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1"},
			{Name: "llm-agent", HasLLM: true}, // Missing model_role
		}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "has_llm=true but no model_role")
	})

	t.Run("sorts by stage_order", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "third", StageOrder: 3},
			{Name: "first", StageOrder: 1},
			{Name: "second", StageOrder: 2},
		}
		err := p.Validate()
		require.NoError(t, err)
		assert.Equal(t, "first", p.Agents[0].Name)
		assert.Equal(t, "second", p.Agents[1].Name)
		assert.Equal(t, "third", p.Agents[2].Name)
	})
}

func TestPipelineConfigBuildGraph(t *testing.T) {
	t.Run("unknown requires", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1", Requires: []string{"nonexistent"}},
		}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "requires unknown stage")
	})

	t.Run("self-require", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1", Requires: []string{"agent-1"}},
		}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "cannot require itself")
	})

	t.Run("unknown after", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1", After: []string{"nonexistent"}},
		}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "after unknown stage")
	})

	t.Run("self-after", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1", After: []string{"agent-1"}},
		}
		err := p.Validate()
		require.Error(t, err)
		assert.Contains(t, err.Error(), "cannot be after itself")
	})

	t.Run("builds adjacency list", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-1"},
			{Name: "agent-2", Requires: []string{"agent-1"}},
			{Name: "agent-3", After: []string{"agent-1"}},
		}
		err := p.Validate()
		require.NoError(t, err)

		dependents := p.GetDependents("agent-1")
		assert.Len(t, dependents, 2)
		assert.Contains(t, dependents, "agent-2")
		assert.Contains(t, dependents, "agent-3")
	})

	t.Run("builds edge limit map", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "agent-a"},
			{Name: "agent-b"},
		}
		p.EdgeLimits = []EdgeLimit{
			{From: "agent-b", To: "agent-a", MaxCount: 3},
		}
		err := p.Validate()
		require.NoError(t, err)

		assert.Equal(t, 3, p.GetEdgeLimit("agent-b", "agent-a"))
		assert.Equal(t, 0, p.GetEdgeLimit("agent-a", "agent-b"))
	})
}

func TestPipelineConfigGetEdgeLimit(t *testing.T) {
	t.Run("nil map", func(t *testing.T) {
		p := &PipelineConfig{}
		assert.Equal(t, 0, p.GetEdgeLimit("a", "b"))
	})

	t.Run("existing edge", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{{Name: "a"}, {Name: "b"}}
		p.EdgeLimits = []EdgeLimit{{From: "a", To: "b", MaxCount: 5}}
		err := p.Validate()
		require.NoError(t, err)
		assert.Equal(t, 5, p.GetEdgeLimit("a", "b"))
	})
}

func TestPipelineConfigGetReadyStages(t *testing.T) {
	p := NewPipelineConfig("test")
	p.Agents = []*AgentConfig{
		{Name: "start"},
		{Name: "middle", Requires: []string{"start"}},
		{Name: "end", Requires: []string{"middle"}},
	}
	err := p.Validate()
	require.NoError(t, err)

	t.Run("nothing completed", func(t *testing.T) {
		ready := p.GetReadyStages(map[string]bool{})
		assert.Len(t, ready, 1)
		assert.Equal(t, "start", ready[0])
	})

	t.Run("start completed", func(t *testing.T) {
		ready := p.GetReadyStages(map[string]bool{"start": true})
		assert.Len(t, ready, 1)
		assert.Equal(t, "middle", ready[0])
	})

	t.Run("start and middle completed", func(t *testing.T) {
		ready := p.GetReadyStages(map[string]bool{"start": true, "middle": true})
		assert.Len(t, ready, 1)
		assert.Equal(t, "end", ready[0])
	})

	t.Run("all completed", func(t *testing.T) {
		ready := p.GetReadyStages(map[string]bool{"start": true, "middle": true, "end": true})
		assert.Empty(t, ready)
	})
}

func TestPipelineConfigGetReadyStagesWithAfter(t *testing.T) {
	p := NewPipelineConfig("test")
	p.Agents = []*AgentConfig{
		{Name: "main"},
		{Name: "optional"},
		{Name: "final", Requires: []string{"main"}, After: []string{"optional"}},
	}
	err := p.Validate()
	require.NoError(t, err)

	t.Run("main completed but not optional", func(t *testing.T) {
		ready := p.GetReadyStages(map[string]bool{"main": true})
		// final should NOT be ready because After optional is not completed
		assert.Contains(t, ready, "optional")
		assert.NotContains(t, ready, "final")
	})

	t.Run("both main and optional completed", func(t *testing.T) {
		ready := p.GetReadyStages(map[string]bool{"main": true, "optional": true})
		assert.Contains(t, ready, "final")
	})
}

func TestPipelineConfigGetReadyStagesJoinAny(t *testing.T) {
	p := NewPipelineConfig("test")
	p.Agents = []*AgentConfig{
		{Name: "path-a"},
		{Name: "path-b"},
		{Name: "joiner", Requires: []string{"path-a", "path-b"}, JoinStrategy: JoinAny},
	}
	err := p.Validate()
	require.NoError(t, err)

	t.Run("no paths completed", func(t *testing.T) {
		ready := p.GetReadyStages(map[string]bool{})
		assert.Len(t, ready, 2) // Both paths ready
		assert.NotContains(t, ready, "joiner")
	})

	t.Run("one path completed", func(t *testing.T) {
		ready := p.GetReadyStages(map[string]bool{"path-a": true})
		assert.Contains(t, ready, "path-b")
		assert.Contains(t, ready, "joiner") // JoinAny: ready when ONE path done
	})
}

func TestPipelineConfigGetDependents(t *testing.T) {
	t.Run("nil adjacency list", func(t *testing.T) {
		p := &PipelineConfig{}
		assert.Nil(t, p.GetDependents("any"))
	})

	t.Run("valid lookup", func(t *testing.T) {
		p := NewPipelineConfig("test")
		p.Agents = []*AgentConfig{
			{Name: "start"},
			{Name: "end", Requires: []string{"start"}},
		}
		err := p.Validate()
		require.NoError(t, err)

		dependents := p.GetDependents("start")
		assert.Len(t, dependents, 1)
		assert.Equal(t, "end", dependents[0])
	})
}

func TestPipelineConfigGetAgent(t *testing.T) {
	p := NewPipelineConfig("test")
	p.Agents = []*AgentConfig{
		{Name: "agent-1"},
		{Name: "agent-2"},
	}

	t.Run("found", func(t *testing.T) {
		agent := p.GetAgent("agent-1")
		require.NotNil(t, agent)
		assert.Equal(t, "agent-1", agent.Name)
	})

	t.Run("not found", func(t *testing.T) {
		agent := p.GetAgent("nonexistent")
		assert.Nil(t, agent)
	})
}

func TestPipelineConfigGetStageOrder(t *testing.T) {
	p := NewPipelineConfig("test")
	p.Agents = []*AgentConfig{
		{Name: "first", StageOrder: 1},
		{Name: "second", StageOrder: 2},
		{Name: "third", StageOrder: 3},
	}
	err := p.Validate()
	require.NoError(t, err)

	order := p.GetStageOrder()
	assert.Equal(t, []string{"first", "second", "third"}, order)
}
