package kernel

import (
	"errors"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestSafeExecute_Success(t *testing.T) {
	logger := &testLogger{}

	err := SafeExecute(logger, "test_operation", func() error {
		return nil
	})

	assert.NoError(t, err)
}

func TestSafeExecute_Error(t *testing.T) {
	logger := &testLogger{}
	expectedErr := errors.New("test error")

	err := SafeExecute(logger, "test_operation", func() error {
		return expectedErr
	})

	assert.Equal(t, expectedErr, err)
}

func TestSafeExecute_Panic(t *testing.T) {
	logger := &testLogger{}

	err := SafeExecute(logger, "test_operation", func() error {
		panic("test panic")
	})

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "panic in test_operation")
	assert.Contains(t, err.Error(), "test panic")

	// Check logger was called
	logger.mu.Lock()
	found := false
	for _, log := range logger.logs {
		if strings.Contains(log, "panic_recovered") {
			found = true
			break
		}
	}
	logger.mu.Unlock()
	assert.True(t, found, "expected panic_recovered log entry")
}

func TestSafeExecute_NilLogger(t *testing.T) {
	// Should not panic even with nil logger
	err := SafeExecute(nil, "test_operation", func() error {
		panic("test panic")
	})

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "panic")
}

func TestSafeExecuteWithResult_Success(t *testing.T) {
	logger := &testLogger{}

	result, err := SafeExecuteWithResult(logger, "test_operation", func() (int, error) {
		return 42, nil
	})

	assert.NoError(t, err)
	assert.Equal(t, 42, result)
}

func TestSafeExecuteWithResult_Error(t *testing.T) {
	logger := &testLogger{}
	expectedErr := errors.New("test error")

	result, err := SafeExecuteWithResult(logger, "test_operation", func() (int, error) {
		return 0, expectedErr
	})

	assert.Equal(t, expectedErr, err)
	assert.Equal(t, 0, result)
}

func TestSafeExecuteWithResult_Panic(t *testing.T) {
	logger := &testLogger{}

	result, err := SafeExecuteWithResult(logger, "test_operation", func() (string, error) {
		panic("test panic")
	})

	assert.Error(t, err)
	assert.Contains(t, err.Error(), "panic in test_operation")
	assert.Equal(t, "", result) // Zero value for string
}

func TestSafeExecuteWithResult_StringResult(t *testing.T) {
	logger := &testLogger{}

	result, err := SafeExecuteWithResult(logger, "test_operation", func() (string, error) {
		return "hello", nil
	})

	assert.NoError(t, err)
	assert.Equal(t, "hello", result)
}

func TestSafeGo_Success(t *testing.T) {
	logger := &testLogger{}
	var wg sync.WaitGroup
	wg.Add(1)

	executed := false
	SafeGo(logger, "test_goroutine", func() {
		executed = true
		wg.Done()
	}, nil)

	wg.Wait()
	assert.True(t, executed)
}

func TestSafeGo_Panic(t *testing.T) {
	logger := &testLogger{}
	var wg sync.WaitGroup
	wg.Add(1)

	var recoveredValue any
	SafeGo(logger, "test_goroutine", func() {
		defer wg.Done()
		panic("goroutine panic")
	}, func(r any) {
		recoveredValue = r
	})

	wg.Wait()
	time.Sleep(10 * time.Millisecond) // Give time for logging

	assert.Equal(t, "goroutine panic", recoveredValue)

	// Check logger was called
	logger.mu.Lock()
	found := false
	for _, log := range logger.logs {
		if strings.Contains(log, "goroutine_panic_recovered") {
			found = true
			break
		}
	}
	logger.mu.Unlock()
	assert.True(t, found, "expected goroutine_panic_recovered log entry")
}

func TestSafeGo_PanicNilCallback(t *testing.T) {
	logger := &testLogger{}
	done := make(chan struct{})

	SafeGo(logger, "test_goroutine", func() {
		defer close(done)
		panic("goroutine panic")
	}, nil) // nil callback

	<-done
	time.Sleep(10 * time.Millisecond)

	// Should not panic with nil callback
	logger.mu.Lock()
	found := false
	for _, log := range logger.logs {
		if strings.Contains(log, "goroutine_panic_recovered") {
			found = true
			break
		}
	}
	logger.mu.Unlock()
	assert.True(t, found)
}

func TestSafeGo_NilLogger(t *testing.T) {
	done := make(chan struct{})

	// Should not panic even with nil logger
	SafeGo(nil, "test_goroutine", func() {
		defer close(done)
		panic("goroutine panic")
	}, func(r any) {
		// Callback should still be called
		assert.Equal(t, "goroutine panic", r)
	})

	<-done
}

func TestShutdownError_Error(t *testing.T) {
	// No errors
	err := &ShutdownError{Errors: nil}
	assert.Equal(t, "shutdown completed with no errors", err.Error())

	// Single error
	err = &ShutdownError{Errors: []error{errors.New("error1")}}
	assert.Equal(t, "shutdown error: error1", err.Error())

	// Multiple errors
	err = &ShutdownError{Errors: []error{
		errors.New("error1"),
		errors.New("error2"),
		errors.New("error3"),
	}}
	assert.Equal(t, "shutdown completed with 3 errors", err.Error())
}

func TestShutdownError_Unwrap(t *testing.T) {
	// No errors
	err := &ShutdownError{Errors: nil}
	assert.Nil(t, err.Unwrap())

	// With errors
	firstErr := errors.New("first error")
	err = &ShutdownError{Errors: []error{firstErr, errors.New("second")}}
	assert.Equal(t, firstErr, err.Unwrap())
}
