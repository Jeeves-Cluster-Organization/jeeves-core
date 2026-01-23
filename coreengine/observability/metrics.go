// Package observability provides Prometheus metrics instrumentation for the coreengine.
package observability

import (
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

// =============================================================================
// PIPELINE METRICS
// =============================================================================

var (
	pipelineExecutionsTotal = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "jeeves_pipeline_executions_total",
			Help: "Total number of pipeline executions",
		},
		[]string{"pipeline", "status"}, // status: success, error, terminated
	)

	pipelineDurationSeconds = promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "jeeves_pipeline_duration_seconds",
			Help:    "Pipeline execution duration in seconds",
			Buckets: []float64{0.1, 0.5, 1, 2, 5, 10, 30, 60},
		},
		[]string{"pipeline"},
	)
)

// =============================================================================
// AGENT METRICS
// =============================================================================

var (
	agentExecutionsTotal = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "jeeves_agent_executions_total",
			Help: "Total number of agent executions",
		},
		[]string{"agent", "status"}, // status: success, error
	)

	agentDurationSeconds = promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "jeeves_agent_duration_seconds",
			Help:    "Agent execution duration in seconds",
			Buckets: []float64{0.01, 0.05, 0.1, 0.5, 1, 2, 5, 10},
		},
		[]string{"agent"},
	)
)

// =============================================================================
// LLM METRICS
// =============================================================================

var (
	llmCallsTotal = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "jeeves_llm_calls_total",
			Help: "Total number of LLM API calls from Go layer",
		},
		[]string{"provider", "model", "status"}, // status: success, error
	)

	llmDurationSeconds = promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "jeeves_llm_duration_seconds",
			Help:    "LLM call duration in seconds",
			Buckets: []float64{0.1, 0.5, 1, 2, 5, 10, 30, 60},
		},
		[]string{"provider", "model"},
	)
)

// =============================================================================
// GRPC METRICS
// =============================================================================

var (
	grpcRequestsTotal = promauto.NewCounterVec(
		prometheus.CounterOpts{
			Name: "jeeves_grpc_requests_total",
			Help: "Total gRPC requests",
		},
		[]string{"method", "status"}, // status: OK, InvalidArgument, Internal, etc.
	)

	grpcRequestDurationSeconds = promauto.NewHistogramVec(
		prometheus.HistogramOpts{
			Name:    "jeeves_grpc_request_duration_seconds",
			Help:    "gRPC request duration in seconds",
			Buckets: []float64{0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1, 2, 5},
		},
		[]string{"method"},
	)
)

// =============================================================================
// PUBLIC API
// =============================================================================

// RecordPipelineExecution records pipeline execution metrics.
// This should be called after pipeline execution completes.
func RecordPipelineExecution(pipeline string, status string, durationMS int) {
	pipelineExecutionsTotal.WithLabelValues(pipeline, status).Inc()
	pipelineDurationSeconds.WithLabelValues(pipeline).Observe(float64(durationMS) / 1000.0)
}

// RecordAgentExecution records agent execution metrics.
// This should be called after agent processing completes.
func RecordAgentExecution(agent string, status string, durationMS int) {
	agentExecutionsTotal.WithLabelValues(agent, status).Inc()
	agentDurationSeconds.WithLabelValues(agent).Observe(float64(durationMS) / 1000.0)
}

// RecordLLMCall records LLM call metrics.
// This should be called after LLM generation completes.
func RecordLLMCall(provider string, model string, status string, durationMS int) {
	llmCallsTotal.WithLabelValues(provider, model, status).Inc()
	llmDurationSeconds.WithLabelValues(provider, model).Observe(float64(durationMS) / 1000.0)
}

// RecordGRPCRequest records gRPC request metrics.
// This should be called from gRPC interceptors.
func RecordGRPCRequest(method string, status string, durationMS int) {
	grpcRequestsTotal.WithLabelValues(method, status).Inc()
	grpcRequestDurationSeconds.WithLabelValues(method).Observe(float64(durationMS) / 1000.0)
}
