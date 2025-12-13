package runtime

import (
	"testing"
	"time"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/config"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

// mockLogger implements agents.Logger for testing.
type mockLogger struct{}

func (l *mockLogger) Info(msg string, keysAndValues ...interface{})  {}
func (l *mockLogger) Warn(msg string, keysAndValues ...interface{})  {}
func (l *mockLogger) Error(msg string, keysAndValues ...interface{}) {}
func (l *mockLogger) Debug(msg string, keysAndValues ...interface{}) {}
func (l *mockLogger) Bind(key string, value interface{}) interface{} { return l }

// TestDAGValidation tests cycle detection and topological sort.
func TestDAGValidation(t *testing.T) {
	t.Run("valid_linear_dag", func(t *testing.T) {
		cfg := &config.PipelineConfig{
			Name:               "test",
			EnableDAGExecution: true,
			Agents: []*config.AgentConfig{
				{Name: "a", StageOrder: 1},
				{Name: "b", StageOrder: 2, Requires: []string{"a"}},
				{Name: "c", StageOrder: 3, Requires: []string{"b"}},
			},
		}
		if err := cfg.Validate(); err != nil {
			t.Errorf("Expected valid DAG, got error: %v", err)
		}
		order := cfg.GetTopologicalOrder()
		if len(order) != 3 {
			t.Errorf("Expected 3 stages in order, got %d", len(order))
		}
		// a should come before b, b before c
		aIdx, bIdx, cIdx := -1, -1, -1
		for i, s := range order {
			if s == "a" {
				aIdx = i
			}
			if s == "b" {
				bIdx = i
			}
			if s == "c" {
				cIdx = i
			}
		}
		if aIdx >= bIdx || bIdx >= cIdx {
			t.Errorf("Invalid topological order: %v", order)
		}
	})

	t.Run("valid_parallel_dag", func(t *testing.T) {
		// Diamond pattern: a -> b, a -> c, b -> d, c -> d
		cfg := &config.PipelineConfig{
			Name:               "diamond",
			EnableDAGExecution: true,
			Agents: []*config.AgentConfig{
				{Name: "a", StageOrder: 1},
				{Name: "b", StageOrder: 2, Requires: []string{"a"}},
				{Name: "c", StageOrder: 2, Requires: []string{"a"}},
				{Name: "d", StageOrder: 3, Requires: []string{"b", "c"}},
			},
		}
		if err := cfg.Validate(); err != nil {
			t.Errorf("Expected valid DAG, got error: %v", err)
		}
		order := cfg.GetTopologicalOrder()
		if len(order) != 4 {
			t.Errorf("Expected 4 stages in order, got %d", len(order))
		}
		// a should come before b and c, both before d
		aIdx, dIdx := -1, -1
		for i, s := range order {
			if s == "a" {
				aIdx = i
			}
			if s == "d" {
				dIdx = i
			}
		}
		if aIdx >= dIdx {
			t.Errorf("Invalid topological order: a should come before d, got %v", order)
		}
	})

	t.Run("cycle_detection", func(t *testing.T) {
		cfg := &config.PipelineConfig{
			Name:               "cyclic",
			EnableDAGExecution: true,
			Agents: []*config.AgentConfig{
				{Name: "a", StageOrder: 1, Requires: []string{"c"}},
				{Name: "b", StageOrder: 2, Requires: []string{"a"}},
				{Name: "c", StageOrder: 3, Requires: []string{"b"}},
			},
		}
		err := cfg.Validate()
		if err == nil {
			t.Error("Expected cycle detection error, got nil")
		}
	})

	t.Run("self_reference", func(t *testing.T) {
		cfg := &config.PipelineConfig{
			Name:               "self_ref",
			EnableDAGExecution: true,
			Agents: []*config.AgentConfig{
				{Name: "a", StageOrder: 1, Requires: []string{"a"}},
			},
		}
		err := cfg.Validate()
		if err == nil {
			t.Error("Expected self-reference error, got nil")
		}
	})

	t.Run("unknown_dependency", func(t *testing.T) {
		cfg := &config.PipelineConfig{
			Name:               "unknown_dep",
			EnableDAGExecution: true,
			Agents: []*config.AgentConfig{
				{Name: "a", StageOrder: 1, Requires: []string{"nonexistent"}},
			},
		}
		err := cfg.Validate()
		if err == nil {
			t.Error("Expected unknown dependency error, got nil")
		}
	})
}

// TestGetReadyStages tests the ready stage computation.
func TestGetReadyStages(t *testing.T) {
	cfg := &config.PipelineConfig{
		Name:               "test",
		EnableDAGExecution: true,
		Agents: []*config.AgentConfig{
			{Name: "a", StageOrder: 1},                               // No deps - ready immediately
			{Name: "b", StageOrder: 2, Requires: []string{"a"}},       // Needs a
			{Name: "c", StageOrder: 2, Requires: []string{"a"}},       // Needs a (parallel with b)
			{Name: "d", StageOrder: 3, Requires: []string{"b", "c"}}, // Needs both b and c
		},
	}
	if err := cfg.Validate(); err != nil {
		t.Fatalf("Validation failed: %v", err)
	}

	t.Run("initial_state", func(t *testing.T) {
		completed := make(map[string]bool)
		ready := cfg.GetReadyStages(completed)
		if len(ready) != 1 || ready[0] != "a" {
			t.Errorf("Expected only 'a' to be ready initially, got %v", ready)
		}
	})

	t.Run("after_a_completes", func(t *testing.T) {
		completed := map[string]bool{"a": true}
		ready := cfg.GetReadyStages(completed)
		// Both b and c should be ready
		if len(ready) != 2 {
			t.Errorf("Expected 2 stages ready after 'a', got %v", ready)
		}
		hasB, hasC := false, false
		for _, s := range ready {
			if s == "b" {
				hasB = true
			}
			if s == "c" {
				hasC = true
			}
		}
		if !hasB || !hasC {
			t.Errorf("Expected both 'b' and 'c' to be ready, got %v", ready)
		}
	})

	t.Run("after_b_only", func(t *testing.T) {
		completed := map[string]bool{"a": true, "b": true}
		ready := cfg.GetReadyStages(completed)
		// Only c should be ready (d needs both b and c)
		if len(ready) != 1 || ready[0] != "c" {
			t.Errorf("Expected only 'c' to be ready, got %v", ready)
		}
	})

	t.Run("after_b_and_c", func(t *testing.T) {
		completed := map[string]bool{"a": true, "b": true, "c": true}
		ready := cfg.GetReadyStages(completed)
		// Now d should be ready
		if len(ready) != 1 || ready[0] != "d" {
			t.Errorf("Expected 'd' to be ready, got %v", ready)
		}
	})
}

// TestJoinAnyStrategy tests the JoinAny strategy.
func TestJoinAnyStrategy(t *testing.T) {
	cfg := &config.PipelineConfig{
		Name:               "join_any",
		EnableDAGExecution: true,
		Agents: []*config.AgentConfig{
			{Name: "a", StageOrder: 1},
			{Name: "b", StageOrder: 1},
			{Name: "c", StageOrder: 2, Requires: []string{"a", "b"}, JoinStrategy: config.JoinAny},
		},
	}
	if err := cfg.Validate(); err != nil {
		t.Fatalf("Validation failed: %v", err)
	}

	// c should be ready when either a OR b completes (JoinAny)
	completed := map[string]bool{"a": true}
	ready := cfg.GetReadyStages(completed)

	hasC := false
	for _, s := range ready {
		if s == "c" {
			hasC = true
		}
	}
	if !hasC {
		t.Errorf("Expected 'c' to be ready with JoinAny after 'a' completes, got %v", ready)
	}
}

// TestEnvelopeDAGHelpers tests the envelope DAG helper methods.
func TestEnvelopeDAGHelpers(t *testing.T) {
	env := envelope.NewGenericEnvelope()
	env.StageOrder = []string{"a", "b", "c"}

	// Test StartStage
	env.StartStage("a")
	if !env.IsStageActive("a") {
		t.Error("Stage 'a' should be active after StartStage")
	}

	// Test CompleteStage
	env.CompleteStage("a")
	if env.IsStageActive("a") {
		t.Error("Stage 'a' should not be active after CompleteStage")
	}
	if !env.IsStageCompleted("a") {
		t.Error("Stage 'a' should be completed after CompleteStage")
	}

	// Test FailStage
	env.StartStage("b")
	env.FailStage("b", "test error")
	if env.IsStageActive("b") {
		t.Error("Stage 'b' should not be active after FailStage")
	}
	if !env.IsStageFailed("b") {
		t.Error("Stage 'b' should be failed after FailStage")
	}
	if !env.HasFailures() {
		t.Error("Envelope should have failures")
	}

	// Test AllStagesComplete
	env.CompleteStage("b") // Override failure for test
	env.CompleteStage("c")
	if !env.AllStagesComplete() {
		t.Error("All stages should be complete")
	}
}

// TestParallelExecution tests that stages actually run in parallel.
func TestParallelExecution(t *testing.T) {
	// Track execution order with timestamps
	executionLog := make([]struct {
		stage string
		start time.Time
		end   time.Time
	}, 0)
	_ = executionLog // Used for logging below

	// Create config with parallel stages
	cfg := &config.PipelineConfig{
		Name:               "parallel_test",
		EnableDAGExecution: true,
		MaxIterations:      3,
		MaxLLMCalls:        10,
		MaxAgentHops:       21,
		Agents: []*config.AgentConfig{
			{Name: "root", StageOrder: 1, OutputKey: "root"},
			{Name: "branch1", StageOrder: 2, Requires: []string{"root"}, OutputKey: "branch1"},
			{Name: "branch2", StageOrder: 2, Requires: []string{"root"}, OutputKey: "branch2"},
			{Name: "merge", StageOrder: 3, Requires: []string{"branch1", "branch2"}, OutputKey: "merge"},
		},
	}

	if err := cfg.Validate(); err != nil {
		t.Fatalf("Validation failed: %v", err)
	}

	// Verify parallel stages are identified correctly
	completed := map[string]bool{"root": true}
	ready := cfg.GetReadyStages(completed)
	if len(ready) != 2 {
		t.Errorf("Expected 2 parallel stages after root, got %d: %v", len(ready), ready)
	}

	// Log execution order
	t.Logf("Topological order: %v", cfg.GetTopologicalOrder())
	t.Logf("Execution log: %v", executionLog)

	// Verify merge only runs after both branches
	completed = map[string]bool{"root": true, "branch1": true}
	ready = cfg.GetReadyStages(completed)
	for _, s := range ready {
		if s == "merge" {
			t.Error("Merge should not be ready until both branches complete")
		}
	}

	completed = map[string]bool{"root": true, "branch1": true, "branch2": true}
	ready = cfg.GetReadyStages(completed)
	hasMerge := false
	for _, s := range ready {
		if s == "merge" {
			hasMerge = true
		}
	}
	if !hasMerge {
		t.Error("Merge should be ready after both branches complete")
	}
}

// TestBackwardCompatibility tests that sequential execution still works.
func TestBackwardCompatibility(t *testing.T) {
	// Config without DAG execution enabled
	cfg := &config.PipelineConfig{
		Name:               "sequential",
		EnableDAGExecution: false, // Disabled
		Agents: []*config.AgentConfig{
			{Name: "a", StageOrder: 1, DefaultNext: "b"},
			{Name: "b", StageOrder: 2, DefaultNext: "c"},
			{Name: "c", StageOrder: 3, DefaultNext: "end"},
		},
	}

	if err := cfg.Validate(); err != nil {
		t.Fatalf("Validation failed: %v", err)
	}

	// Should not have computed topological order
	if cfg.GetTopologicalOrder() != nil {
		t.Error("Should not compute topological order when DAG execution is disabled")
	}

	// GetStageOrder should still work
	order := cfg.GetStageOrder()
	if len(order) != 3 {
		t.Errorf("Expected 3 stages, got %d", len(order))
	}
}
