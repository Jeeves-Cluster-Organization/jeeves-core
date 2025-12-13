// Package main provides the envelope CLI for Python-Go hybrid execution.
//
// This CLI reads JSON envelope state from stdin, performs operations,
// and writes the result to stdout. Designed for subprocess-based interop.
//
// Usage:
//
//	# Process envelope (validate and transform)
//	echo '{"envelope_id": "..."}' | go-envelope process
//
//	# Create new envelope
//	echo '{"raw_input": "hello", "user_id": "u1"}' | go-envelope create
//
//	# Check if envelope can continue
//	echo '{"envelope_id": "..."}' | go-envelope can-continue
//
//	# Get envelope result dict
//	echo '{"envelope_id": "..."}' | go-envelope result
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/envelope"
)

const (
	cmdProcess     = "process"
	cmdCreate      = "create"
	cmdCanContinue = "can-continue"
	cmdResult      = "result"
	cmdValidate    = "validate"
	cmdVersion     = "version"
)

// Version information
const (
	Version   = "1.0.0"
	BuildTime = "2025-12-10"
)

func main() {
	if len(os.Args) < 2 {
		printUsage()
		os.Exit(1)
	}

	cmd := os.Args[1]

	switch cmd {
	case cmdVersion:
		handleVersion()
	case cmdProcess:
		handleProcess()
	case cmdCreate:
		handleCreate()
	case cmdCanContinue:
		handleCanContinue()
	case cmdResult:
		handleResult()
	case cmdValidate:
		handleValidate()
	default:
		fmt.Fprintf(os.Stderr, "Unknown command: %s\n", cmd)
		printUsage()
		os.Exit(1)
	}
}

func printUsage() {
	fmt.Fprintln(os.Stderr, `Usage: go-envelope <command>

Commands:
  process       Read envelope JSON from stdin, process, write to stdout
  create        Create new envelope from input JSON
  can-continue  Check if envelope can continue processing
  result        Get envelope result dictionary
  validate      Validate envelope JSON structure
  version       Print version information

Input/Output:
  All commands read JSON from stdin and write JSON to stdout.
  Errors are written to stderr.

Examples:
  echo '{"raw_input":"hello","user_id":"u1"}' | go-envelope create
  cat envelope.json | go-envelope can-continue
  cat envelope.json | go-envelope result`)
}

// handleVersion prints version information.
func handleVersion() {
	output := map[string]string{
		"version":    Version,
		"build_time": BuildTime,
		"go_version": "1.21+",
	}
	writeJSON(output)
}

// handleProcess reads envelope, processes it, and writes back.
func handleProcess() {
	input, err := readInput()
	if err != nil {
		writeError("read_error", err.Error())
		os.Exit(1)
	}

	// Parse state dict
	var stateDict map[string]any
	if err := json.Unmarshal(input, &stateDict); err != nil {
		writeError("parse_error", fmt.Sprintf("Invalid JSON: %s", err.Error()))
		os.Exit(1)
	}

	// Create envelope from state
	env := envelope.FromStateDict(stateDict)

	// Validate bounds
	if !env.CanContinue() {
		writeError("bounds_exceeded", fmt.Sprintf("Cannot continue: %v", env.TerminalReason_))
		os.Exit(1)
	}

	// Write back the processed envelope
	result := env.ToStateDict()
	writeJSON(result)
}

// handleCreate creates a new envelope from input.
func handleCreate() {
	input, err := readInput()
	if err != nil {
		writeError("read_error", err.Error())
		os.Exit(1)
	}

	// Parse input
	var createInput struct {
		RawInput   string         `json:"raw_input"`
		UserID     string         `json:"user_id"`
		SessionID  string         `json:"session_id"`
		RequestID  *string        `json:"request_id,omitempty"`
		Metadata   map[string]any `json:"metadata,omitempty"`
		StageOrder []string       `json:"stage_order,omitempty"`
	}

	if err := json.Unmarshal(input, &createInput); err != nil {
		writeError("parse_error", fmt.Sprintf("Invalid JSON: %s", err.Error()))
		os.Exit(1)
	}

	// Create envelope
	env := envelope.CreateGenericEnvelope(
		createInput.RawInput,
		createInput.UserID,
		createInput.SessionID,
		createInput.RequestID,
		createInput.Metadata,
		createInput.StageOrder,
	)

	// Write result
	result := env.ToStateDict()
	writeJSON(result)
}

// handleCanContinue checks if envelope can continue.
func handleCanContinue() {
	input, err := readInput()
	if err != nil {
		writeError("read_error", err.Error())
		os.Exit(1)
	}

	// Parse state dict
	var stateDict map[string]any
	if err := json.Unmarshal(input, &stateDict); err != nil {
		writeError("parse_error", fmt.Sprintf("Invalid JSON: %s", err.Error()))
		os.Exit(1)
	}

	// Create envelope and check
	env := envelope.FromStateDict(stateDict)
	canContinue := env.CanContinue()

	var reason *string
	if env.TerminalReason_ != nil {
		r := string(*env.TerminalReason_)
		reason = &r
	}

	result := map[string]any{
		"can_continue":    canContinue,
		"terminal_reason": reason,
		"iteration":       env.Iteration,
		"llm_call_count":  env.LLMCallCount,
		"agent_hop_count": env.AgentHopCount,
	}
	writeJSON(result)
}

// handleResult gets the envelope result dictionary.
func handleResult() {
	input, err := readInput()
	if err != nil {
		writeError("read_error", err.Error())
		os.Exit(1)
	}

	// Parse state dict
	var stateDict map[string]any
	if err := json.Unmarshal(input, &stateDict); err != nil {
		writeError("parse_error", fmt.Sprintf("Invalid JSON: %s", err.Error()))
		os.Exit(1)
	}

	// Create envelope and get result
	env := envelope.FromStateDict(stateDict)
	result := env.ToResultDict()
	writeJSON(result)
}

// handleValidate validates the envelope JSON structure.
func handleValidate() {
	input, err := readInput()
	if err != nil {
		writeError("read_error", err.Error())
		os.Exit(1)
	}

	// Parse state dict
	var stateDict map[string]any
	if err := json.Unmarshal(input, &stateDict); err != nil {
		result := map[string]any{
			"valid":  false,
			"errors": []string{fmt.Sprintf("Invalid JSON: %s", err.Error())},
		}
		writeJSON(result)
		return
	}

	// Validate required fields
	errors := []string{}
	requiredStrings := []string{"envelope_id", "user_id"}
	for _, field := range requiredStrings {
		if _, ok := stateDict[field].(string); !ok {
			if stateDict[field] == nil {
				// Allow missing fields (will get defaults)
				continue
			}
			errors = append(errors, fmt.Sprintf("Field '%s' must be a string", field))
		}
	}

	// Try to create envelope (validates structure)
	env := envelope.FromStateDict(stateDict)
	if env.EnvelopeID == "" {
		errors = append(errors, "envelope_id is empty after parsing")
	}

	result := map[string]any{
		"valid":       len(errors) == 0,
		"errors":      errors,
		"envelope_id": env.EnvelopeID,
	}
	writeJSON(result)
}

// readInput reads all input from stdin.
func readInput() ([]byte, error) {
	reader := bufio.NewReader(os.Stdin)
	return io.ReadAll(reader)
}

// writeJSON writes a JSON object to stdout.
func writeJSON(v any) {
	encoder := json.NewEncoder(os.Stdout)
	encoder.SetIndent("", "")
	if err := encoder.Encode(v); err != nil {
		fmt.Fprintf(os.Stderr, "Error encoding JSON: %s\n", err.Error())
		os.Exit(1)
	}
}

// writeError writes an error response to stdout.
func writeError(code, message string) {
	result := map[string]any{
		"error":   true,
		"code":    code,
		"message": message,
	}
	writeJSON(result)
}
