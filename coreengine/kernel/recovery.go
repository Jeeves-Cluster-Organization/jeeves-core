// Package kernel provides panic recovery utilities for kernel operations.
//
// These utilities ensure that panics in kernel operations don't crash
// the entire system but are instead gracefully handled and logged.
package kernel

import (
	"fmt"
	"runtime/debug"
)

// RecoveryResult represents the result of a recovered operation.
type RecoveryResult struct {
	Recovered bool
	PanicValue any
	StackTrace string
}

// SafeExecute executes a function with panic recovery.
// If the function panics, the panic is logged and an error is returned.
// The operation parameter is used for logging context.
func SafeExecute(logger Logger, operation string, fn func() error) error {
	var panicErr error

	func() {
		defer func() {
			if r := recover(); r != nil {
				stack := string(debug.Stack())
				if logger != nil {
					logger.Error("panic_recovered",
						"operation", operation,
						"panic", r,
						"stack", stack,
					)
				}
				panicErr = fmt.Errorf("panic in %s: %v", operation, r)
			}
		}()
		panicErr = fn()
	}()

	return panicErr
}

// SafeExecuteWithResult executes a function with panic recovery and returns both result and error.
// Useful for operations that return a value in addition to an error.
func SafeExecuteWithResult[T any](logger Logger, operation string, fn func() (T, error)) (T, error) {
	var result T
	var err error

	func() {
		defer func() {
			if r := recover(); r != nil {
				stack := string(debug.Stack())
				if logger != nil {
					logger.Error("panic_recovered",
						"operation", operation,
						"panic", r,
						"stack", stack,
					)
				}
				err = fmt.Errorf("panic in %s: %v", operation, r)
			}
		}()
		result, err = fn()
	}()

	return result, err
}

// SafeGo runs a goroutine with panic recovery.
// If the goroutine panics, the panic is logged and the onPanic callback is called.
func SafeGo(logger Logger, operation string, fn func(), onPanic func(recovered any)) {
	go func() {
		defer func() {
			if r := recover(); r != nil {
				stack := string(debug.Stack())
				if logger != nil {
					logger.Error("goroutine_panic_recovered",
						"operation", operation,
						"panic", r,
						"stack", stack,
					)
				}
				if onPanic != nil {
					onPanic(r)
				}
			}
		}()
		fn()
	}()
}
