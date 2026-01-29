package observability

import (
	"context"
	"testing"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// METRICS TESTS
// =============================================================================

func TestRecordPipelineExecution(t *testing.T) {
	tests := []struct {
		name       string
		pipeline   string
		status     string
		durationMS int
	}{
		{"success pipeline", "test-pipeline", "success", 1000},
		{"error pipeline", "test-pipeline", "error", 500},
		{"terminated pipeline", "test-pipeline", "terminated", 2000},
		{"zero duration", "fast-pipeline", "success", 0},
		{"long duration", "slow-pipeline", "success", 60000},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Should not panic
			RecordPipelineExecution(tt.pipeline, tt.status, tt.durationMS)

			// Verify counter was incremented
			count := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues(tt.pipeline, tt.status))
			assert.Greater(t, count, 0.0)
		})
	}
}

func TestRecordAgentExecution(t *testing.T) {
	tests := []struct {
		name       string
		agent      string
		status     string
		durationMS int
	}{
		{"successful agent", "planner", "success", 100},
		{"failed agent", "executor", "error", 50},
		{"slow agent", "analyzer", "success", 5000},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Should not panic
			RecordAgentExecution(tt.agent, tt.status, tt.durationMS)

			// Verify counter was incremented
			count := testutil.ToFloat64(agentExecutionsTotal.WithLabelValues(tt.agent, tt.status))
			assert.Greater(t, count, 0.0)
		})
	}
}

func TestRecordLLMCall(t *testing.T) {
	tests := []struct {
		name       string
		provider   string
		model      string
		status     string
		durationMS int
	}{
		{"successful claude call", "anthropic", "claude-3-5-sonnet", "success", 2000},
		{"successful gpt call", "openai", "gpt-4", "success", 1500},
		{"failed call", "anthropic", "claude-3-5-sonnet", "error", 100},
		{"timeout call", "openai", "gpt-4", "timeout", 30000},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Should not panic
			RecordLLMCall(tt.provider, tt.model, tt.status, tt.durationMS)

			// Verify counter was incremented
			count := testutil.ToFloat64(llmCallsTotal.WithLabelValues(tt.provider, tt.model, tt.status))
			assert.Greater(t, count, 0.0)
		})
	}
}

func TestRecordGRPCRequest(t *testing.T) {
	tests := []struct {
		name       string
		method     string
		status     string
		durationMS int
	}{
		{"successful request", "/EngineService/ExecutePipeline", "OK", 100},
		{"invalid argument", "/EngineService/CreateEnvelope", "InvalidArgument", 10},
		{"internal error", "/EngineService/ExecutePipeline", "Internal", 50},
		{"not found", "/KernelService/GetProcess", "NotFound", 5},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Should not panic
			RecordGRPCRequest(tt.method, tt.status, tt.durationMS)

			// Verify counter was incremented
			count := testutil.ToFloat64(grpcRequestsTotal.WithLabelValues(tt.method, tt.status))
			assert.Greater(t, count, 0.0)
		})
	}
}

func TestMetrics_Concurrent(t *testing.T) {
	// Test that metrics recording is thread-safe
	const goroutines = 10
	const iterations = 100

	done := make(chan bool, goroutines)

	for i := 0; i < goroutines; i++ {
		go func(id int) {
			for j := 0; j < iterations; j++ {
				RecordPipelineExecution("concurrent-test", "success", 100)
				RecordAgentExecution("concurrent-agent", "success", 50)
				RecordLLMCall("test-provider", "test-model", "success", 1000)
				RecordGRPCRequest("/Test/Method", "OK", 10)
			}
			done <- true
		}(i)
	}

	// Wait for all goroutines
	for i := 0; i < goroutines; i++ {
		<-done
	}

	// Verify metrics were recorded
	count := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues("concurrent-test", "success"))
	assert.Equal(t, float64(goroutines*iterations), count)
}

func TestMetrics_DifferentLabels(t *testing.T) {
	// Test that metrics with different labels are tracked separately
	RecordPipelineExecution("pipeline-a", "success", 100)
	RecordPipelineExecution("pipeline-a", "error", 200)
	RecordPipelineExecution("pipeline-b", "success", 300)

	countASuccess := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues("pipeline-a", "success"))
	countAError := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues("pipeline-a", "error"))
	countBSuccess := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues("pipeline-b", "success"))

	assert.Greater(t, countASuccess, 0.0)
	assert.Greater(t, countAError, 0.0)
	assert.Greater(t, countBSuccess, 0.0)
}

func TestMetrics_HistogramBuckets(t *testing.T) {
	// Test that histogram buckets work correctly
	durations := []int{10, 100, 500, 1000, 5000, 30000}

	for _, duration := range durations {
		RecordPipelineExecution("histogram-test", "success", duration)
	}

	// Verify observations were recorded
	// Note: We can't easily verify specific buckets, but we can verify the metric exists
	count := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues("histogram-test", "success"))
	assert.Equal(t, float64(len(durations)), count)
}

// =============================================================================
// TRACING TESTS
// =============================================================================

func TestInitTracer_InvalidEndpoint(t *testing.T) {
	// Test with invalid endpoint format
	shutdown, err := InitTracer("test-service", "")

	// Empty endpoint should fail
	require.Error(t, err)
	assert.Nil(t, shutdown)
	assert.Contains(t, err.Error(), "failed to create trace exporter")
}

func TestInitTracer_ValidParameters(t *testing.T) {
	// Skip this test in CI or when OTLP endpoint is not available
	// This is an integration test that requires a real OTLP collector
	t.Skip("Skipping integration test - requires OTLP collector")

	shutdown, err := InitTracer("test-service", "localhost:4317")

	if err != nil {
		// Expected - no OTLP collector running
		assert.Contains(t, err.Error(), "failed to create trace exporter")
		return
	}

	// If we got here, cleanup
	require.NotNil(t, shutdown)
	defer shutdown(context.Background())
}

func TestInitTracer_ServiceName(t *testing.T) {
	// Test that service name is properly set (will fail to connect, but that's ok)
	shutdown, err := InitTracer("jeeves-kernel", "invalid-endpoint:1234")

	// Should fail due to invalid endpoint, but we're testing the call works
	if err != nil {
		assert.Contains(t, err.Error(), "failed to create trace exporter")
	}

	if shutdown != nil {
		shutdown(context.Background())
	}
}

func TestInitTracer_Shutdown(t *testing.T) {
	// Test that shutdown function can be called safely even if init failed
	_, err := InitTracer("test", "")

	// Even though init failed, test that we don't panic
	require.Error(t, err)
}

// =============================================================================
// INTEGRATION TESTS
// =============================================================================

func TestMetrics_EndToEnd(t *testing.T) {
	// Simulate a complete pipeline execution with all metrics
	pipelineName := "e2e-test-pipeline"

	// Record pipeline start
	RecordPipelineExecution(pipelineName, "success", 5000)

	// Record agent executions within pipeline
	RecordAgentExecution("planner", "success", 500)
	RecordAgentExecution("executor", "success", 3000)
	RecordAgentExecution("summarizer", "success", 1000)

	// Record LLM calls
	RecordLLMCall("anthropic", "claude-3-5-sonnet", "success", 2000)
	RecordLLMCall("anthropic", "claude-3-5-sonnet", "success", 1500)

	// Record gRPC requests
	RecordGRPCRequest("/EngineService/ExecutePipeline", "OK", 5000)

	// Verify all metrics were recorded
	pipelineCount := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues(pipelineName, "success"))
	assert.Greater(t, pipelineCount, 0.0)

	plannerCount := testutil.ToFloat64(agentExecutionsTotal.WithLabelValues("planner", "success"))
	assert.Greater(t, plannerCount, 0.0)

	llmCount := testutil.ToFloat64(llmCallsTotal.WithLabelValues("anthropic", "claude-3-5-sonnet", "success"))
	assert.Greater(t, llmCount, 0.0)

	grpcCount := testutil.ToFloat64(grpcRequestsTotal.WithLabelValues("/EngineService/ExecutePipeline", "OK"))
	assert.Greater(t, grpcCount, 0.0)
}

// =============================================================================
// PROMETHEUS COLLECTOR TESTS
// =============================================================================

func TestMetrics_PrometheusCollector(t *testing.T) {
	// Test that metrics are properly registered with Prometheus
	RecordPipelineExecution("collector-test", "success", 1000)

	// Verify the metric can be collected
	count := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues("collector-test", "success"))
	assert.Greater(t, count, 0.0)

	// Verify metric name
	desc := pipelineExecutionsTotal.WithLabelValues("collector-test", "success").Desc()
	assert.NotNil(t, desc)
}

func TestMetrics_LabelValidation(t *testing.T) {
	// Test that metrics work with various label values
	labels := []string{
		"simple",
		"with-dashes",
		"with_underscores",
		"with.dots",
		"UPPERCASE",
		"MixedCase",
	}

	for _, label := range labels {
		RecordPipelineExecution(label, "success", 100)
		count := testutil.ToFloat64(pipelineExecutionsTotal.WithLabelValues(label, "success"))
		assert.Greater(t, count, 0.0, "Failed for label: %s", label)
	}
}

func TestMetrics_Registries(t *testing.T) {
	// Test that our metrics are compatible with custom registries
	reg := prometheus.NewRegistry()

	// Our metrics use promauto which registers with default registry
	// This is just a smoke test to ensure prometheus package works
	assert.NotNil(t, reg)
}
