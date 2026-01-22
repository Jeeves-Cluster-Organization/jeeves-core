// Package main provides integration tests for the envelope CLI.
//
// These tests execute the CLI as a subprocess and validate
// stdin/stdout behavior for Python-Go interop.
package main

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// TEST HELPERS
// =============================================================================

// binaryPath returns the path to the built CLI binary.
// Tests build the binary once and reuse it.
var binaryPath string

func TestMain(m *testing.M) {
	// Build the CLI binary for testing
	var err error
	binaryPath, err = buildCLI()
	if err != nil {
		panic("Failed to build CLI for testing: " + err.Error())
	}

	// Run tests
	code := m.Run()

	// Cleanup
	if binaryPath != "" {
		os.Remove(binaryPath)
	}

	os.Exit(code)
}

// buildCLI builds the CLI binary and returns its path.
func buildCLI() (string, error) {
	// Determine output binary name
	binName := "go-envelope-test"
	if runtime.GOOS == "windows" {
		binName += ".exe"
	}

	// Build in temp directory
	tmpDir := os.TempDir()
	binPath := filepath.Join(tmpDir, binName)

	// Build the binary
	cmd := exec.Command("go", "build", "-o", binPath, ".")
	cmd.Dir = "."
	if output, err := cmd.CombinedOutput(); err != nil {
		return "", &exec.ExitError{Stderr: output}
	}

	return binPath, nil
}

// runCLI executes the CLI with the given command and input.
func runCLI(t *testing.T, command string, input string) (string, string, int) {
	t.Helper()

	cmd := exec.Command(binaryPath, command)
	cmd.Stdin = strings.NewReader(input)

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err := cmd.Run()
	exitCode := 0
	if exitErr, ok := err.(*exec.ExitError); ok {
		exitCode = exitErr.ExitCode()
	} else if err != nil {
		t.Fatalf("Failed to run CLI: %v", err)
	}

	return stdout.String(), stderr.String(), exitCode
}

// parseJSON parses JSON output into a map.
func parseJSON(t *testing.T, output string) map[string]any {
	t.Helper()

	var result map[string]any
	if err := json.Unmarshal([]byte(strings.TrimSpace(output)), &result); err != nil {
		t.Fatalf("Failed to parse JSON output: %v\nOutput: %s", err, output)
	}
	return result
}

// =============================================================================
// VERSION COMMAND TESTS
// =============================================================================

func TestCLI_Version(t *testing.T) {
	stdout, _, exitCode := runCLI(t, "version", "")

	assert.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.Equal(t, "1.0.0", result["version"])
	assert.NotEmpty(t, result["build_time"])
	assert.NotEmpty(t, result["go_version"])
}

// =============================================================================
// CREATE COMMAND TESTS
// =============================================================================

func TestCLI_CreateEnvelope(t *testing.T) {
	input := `{
		"raw_input": "Hello, world!",
		"user_id": "user_123",
		"session_id": "session_456"
	}`

	stdout, _, exitCode := runCLI(t, "create", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.Equal(t, "Hello, world!", result["raw_input"])
	assert.Equal(t, "user_123", result["user_id"])
	assert.Equal(t, "session_456", result["session_id"])
	assert.NotEmpty(t, result["envelope_id"])
	assert.NotEmpty(t, result["request_id"])
}

func TestCLI_CreateEnvelopeWithRequestID(t *testing.T) {
	input := `{
		"raw_input": "Test",
		"user_id": "user_1",
		"session_id": "session_1",
		"request_id": "custom_req_123"
	}`

	stdout, _, exitCode := runCLI(t, "create", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.Equal(t, "custom_req_123", result["request_id"])
}

func TestCLI_CreateEnvelopeWithMetadata(t *testing.T) {
	input := `{
		"raw_input": "Test",
		"user_id": "user_1",
		"session_id": "session_1",
		"metadata": {"source": "cli_test", "priority": "high"}
	}`

	stdout, _, exitCode := runCLI(t, "create", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	metadata, ok := result["metadata"].(map[string]any)
	require.True(t, ok, "metadata should be a map")
	assert.Equal(t, "cli_test", metadata["source"])
	assert.Equal(t, "high", metadata["priority"])
}

func TestCLI_CreateEnvelopeWithStageOrder(t *testing.T) {
	input := `{
		"raw_input": "Test",
		"user_id": "user_1",
		"session_id": "session_1",
		"stage_order": ["intake", "analysis", "output"]
	}`

	stdout, _, exitCode := runCLI(t, "create", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	stageOrder, ok := result["stage_order"].([]any)
	require.True(t, ok, "stage_order should be an array")
	assert.Len(t, stageOrder, 3)
	assert.Equal(t, "intake", stageOrder[0])
}

func TestCLI_CreateEnvelopeInvalidJSON(t *testing.T) {
	input := `{not valid json`

	stdout, _, exitCode := runCLI(t, "create", input)

	assert.Equal(t, 1, exitCode)

	result := parseJSON(t, stdout)
	assert.True(t, result["error"].(bool))
	assert.Equal(t, "parse_error", result["code"])
}

// =============================================================================
// CAN-CONTINUE COMMAND TESTS
// =============================================================================

func TestCLI_CanContinueTrue(t *testing.T) {
	// Create envelope with plenty of headroom
	input := `{
		"envelope_id": "env_test",
		"user_id": "user_1",
		"iteration": 0,
		"max_iterations": 5,
		"llm_call_count": 0,
		"max_llm_calls": 10,
		"agent_hop_count": 0,
		"max_agent_hops": 21
	}`

	stdout, _, exitCode := runCLI(t, "can-continue", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.True(t, result["can_continue"].(bool))
	assert.Nil(t, result["terminal_reason"])
}

func TestCLI_CanContinueFalseIterations(t *testing.T) {
	// Iteration exceeds max
	input := `{
		"envelope_id": "env_test",
		"user_id": "user_1",
		"iteration": 6,
		"max_iterations": 5,
		"llm_call_count": 0,
		"max_llm_calls": 10,
		"agent_hop_count": 0,
		"max_agent_hops": 21
	}`

	stdout, _, exitCode := runCLI(t, "can-continue", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.False(t, result["can_continue"].(bool))
	assert.NotNil(t, result["terminal_reason"])
}

func TestCLI_CanContinueFalseLLMCalls(t *testing.T) {
	// LLM calls at max
	input := `{
		"envelope_id": "env_test",
		"user_id": "user_1",
		"iteration": 0,
		"max_iterations": 5,
		"llm_call_count": 10,
		"max_llm_calls": 10,
		"agent_hop_count": 0,
		"max_agent_hops": 21
	}`

	stdout, _, exitCode := runCLI(t, "can-continue", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.False(t, result["can_continue"].(bool))
}

func TestCLI_CanContinueFalseAgentHops(t *testing.T) {
	// Agent hops at max
	input := `{
		"envelope_id": "env_test",
		"user_id": "user_1",
		"iteration": 0,
		"max_iterations": 5,
		"llm_call_count": 0,
		"max_llm_calls": 10,
		"agent_hop_count": 21,
		"max_agent_hops": 21
	}`

	stdout, _, exitCode := runCLI(t, "can-continue", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.False(t, result["can_continue"].(bool))
}

// =============================================================================
// PROCESS COMMAND TESTS
// =============================================================================

func TestCLI_ProcessValidEnvelope(t *testing.T) {
	input := `{
		"envelope_id": "env_process_test",
		"request_id": "req_1",
		"user_id": "user_1",
		"session_id": "session_1",
		"raw_input": "Process this",
		"iteration": 0,
		"max_iterations": 5,
		"llm_call_count": 0,
		"max_llm_calls": 10,
		"agent_hop_count": 0,
		"max_agent_hops": 21
	}`

	stdout, _, exitCode := runCLI(t, "process", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.Equal(t, "env_process_test", result["envelope_id"])
	assert.Equal(t, "Process this", result["raw_input"])
}

func TestCLI_ProcessBoundsExceeded(t *testing.T) {
	// Envelope that exceeds bounds should fail processing
	input := `{
		"envelope_id": "env_exceeded",
		"user_id": "user_1",
		"iteration": 10,
		"max_iterations": 5
	}`

	stdout, _, exitCode := runCLI(t, "process", input)

	assert.Equal(t, 1, exitCode)

	result := parseJSON(t, stdout)
	assert.True(t, result["error"].(bool))
	assert.Equal(t, "bounds_exceeded", result["code"])
}

// =============================================================================
// RESULT COMMAND TESTS
// =============================================================================

func TestCLI_ResultWithOutputs(t *testing.T) {
	input := `{
		"envelope_id": "env_result_test",
		"user_id": "user_1",
		"raw_input": "Original query",
		"current_stage": "end",
		"outputs": {
			"stageA": {"analysis": "complete"},
			"stageB": {"summary": "done"}
		}
	}`

	stdout, _, exitCode := runCLI(t, "result", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.Equal(t, "env_result_test", result["envelope_id"])
	assert.NotNil(t, result["outputs"])
}

func TestCLI_ResultTerminated(t *testing.T) {
	input := `{
		"envelope_id": "env_terminated",
		"user_id": "user_1",
		"terminated": true,
		"termination_reason": "User cancelled"
	}`

	stdout, _, exitCode := runCLI(t, "result", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.True(t, result["terminated"].(bool))
}

// =============================================================================
// VALIDATE COMMAND TESTS
// =============================================================================

func TestCLI_ValidateValidEnvelope(t *testing.T) {
	input := `{
		"envelope_id": "env_valid",
		"user_id": "user_1"
	}`

	stdout, _, exitCode := runCLI(t, "validate", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.True(t, result["valid"].(bool))
	errors, _ := result["errors"].([]any)
	assert.Empty(t, errors)
}

func TestCLI_ValidateInvalidJSON(t *testing.T) {
	input := `{broken json`

	stdout, _, exitCode := runCLI(t, "validate", input)

	require.Equal(t, 0, exitCode) // validate doesn't exit 1 on invalid

	result := parseJSON(t, stdout)
	assert.False(t, result["valid"].(bool))
	errors, _ := result["errors"].([]any)
	assert.NotEmpty(t, errors)
}

func TestCLI_ValidateMissingFields(t *testing.T) {
	// Empty envelope - should still be valid (gets defaults)
	input := `{}`

	stdout, _, exitCode := runCLI(t, "validate", input)

	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	// Empty object is valid (fields get defaults)
	// but envelope_id will be empty after parsing
	assert.NotNil(t, result["envelope_id"])
}

// =============================================================================
// ERROR HANDLING TESTS
// =============================================================================

func TestCLI_UnknownCommand(t *testing.T) {
	cmd := exec.Command(binaryPath, "unknown_command")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr

	err := cmd.Run()
	require.Error(t, err)

	exitErr, ok := err.(*exec.ExitError)
	require.True(t, ok)
	assert.Equal(t, 1, exitErr.ExitCode())
	assert.Contains(t, stderr.String(), "Unknown command")
}

func TestCLI_NoCommand(t *testing.T) {
	cmd := exec.Command(binaryPath)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr

	err := cmd.Run()
	require.Error(t, err)

	exitErr, ok := err.(*exec.ExitError)
	require.True(t, ok)
	assert.Equal(t, 1, exitErr.ExitCode())
	assert.Contains(t, stderr.String(), "Usage")
}

func TestCLI_EmptyInput(t *testing.T) {
	stdout, _, exitCode := runCLI(t, "create", "")

	assert.Equal(t, 1, exitCode)

	result := parseJSON(t, stdout)
	assert.True(t, result["error"].(bool))
}

// =============================================================================
// ROUNDTRIP TESTS
// =============================================================================

func TestCLI_CreateThenProcess(t *testing.T) {
	// Create an envelope
	createInput := `{
		"raw_input": "Roundtrip test",
		"user_id": "user_rt",
		"session_id": "session_rt"
	}`

	stdout, _, exitCode := runCLI(t, "create", createInput)
	require.Equal(t, 0, exitCode)

	// Use created envelope as input to process
	stdout, _, exitCode = runCLI(t, "process", stdout)
	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.Equal(t, "Roundtrip test", result["raw_input"])
	assert.Equal(t, "user_rt", result["user_id"])
}

func TestCLI_CreateProcessCanContinue(t *testing.T) {
	// Full roundtrip: create → process → can-continue
	createInput := `{
		"raw_input": "Full roundtrip",
		"user_id": "user_full",
		"session_id": "session_full"
	}`

	// Create
	stdout, _, exitCode := runCLI(t, "create", createInput)
	require.Equal(t, 0, exitCode)

	// Process
	stdout, _, exitCode = runCLI(t, "process", stdout)
	require.Equal(t, 0, exitCode)

	// Can continue
	stdout, _, exitCode = runCLI(t, "can-continue", stdout)
	require.Equal(t, 0, exitCode)

	result := parseJSON(t, stdout)
	assert.True(t, result["can_continue"].(bool))
}
