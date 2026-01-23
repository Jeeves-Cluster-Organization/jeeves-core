// Package typeutil provides safe type assertion helpers to prevent panics from failed type casts.
// These helpers follow Go best practices by using the comma-ok idiom for type assertions.
package typeutil

import (
	"fmt"
)

// SafeMapStringAny safely asserts value to map[string]any.
// Returns the map and true if successful, or an empty map and false if not.
func SafeMapStringAny(value any) (map[string]any, bool) {
	if value == nil {
		return nil, false
	}
	m, ok := value.(map[string]any)
	return m, ok
}

// SafeMapStringAnyDefault safely asserts value to map[string]any with a default fallback.
// Returns the map if assertion succeeds, otherwise returns the default value.
func SafeMapStringAnyDefault(value any, defaultVal map[string]any) map[string]any {
	if m, ok := SafeMapStringAny(value); ok {
		return m
	}
	return defaultVal
}

// SafeString safely asserts value to string.
// Returns the string and true if successful, or empty string and false if not.
func SafeString(value any) (string, bool) {
	if value == nil {
		return "", false
	}
	s, ok := value.(string)
	return s, ok
}

// SafeStringDefault safely asserts value to string with a default fallback.
// Returns the string if assertion succeeds, otherwise returns the default value.
func SafeStringDefault(value any, defaultVal string) string {
	if s, ok := SafeString(value); ok {
		return s
	}
	return defaultVal
}

// SafeInt safely asserts value to int.
// Returns the int and true if successful, or 0 and false if not.
// Also handles float64 (common from JSON unmarshaling).
func SafeInt(value any) (int, bool) {
	if value == nil {
		return 0, false
	}
	switch v := value.(type) {
	case int:
		return v, true
	case int64:
		return int(v), true
	case int32:
		return int(v), true
	case float64:
		return int(v), true
	case float32:
		return int(v), true
	default:
		return 0, false
	}
}

// SafeIntDefault safely asserts value to int with a default fallback.
func SafeIntDefault(value any, defaultVal int) int {
	if i, ok := SafeInt(value); ok {
		return i
	}
	return defaultVal
}

// SafeFloat64 safely asserts value to float64.
// Returns the float64 and true if successful, or 0 and false if not.
// Also handles int types.
func SafeFloat64(value any) (float64, bool) {
	if value == nil {
		return 0, false
	}
	switch v := value.(type) {
	case float64:
		return v, true
	case float32:
		return float64(v), true
	case int:
		return float64(v), true
	case int64:
		return float64(v), true
	case int32:
		return float64(v), true
	default:
		return 0, false
	}
}

// SafeFloat64Default safely asserts value to float64 with a default fallback.
func SafeFloat64Default(value any, defaultVal float64) float64 {
	if f, ok := SafeFloat64(value); ok {
		return f
	}
	return defaultVal
}

// SafeBool safely asserts value to bool.
// Returns the bool and true if successful, or false and false if not.
func SafeBool(value any) (bool, bool) {
	if value == nil {
		return false, false
	}
	b, ok := value.(bool)
	return b, ok
}

// SafeBoolDefault safely asserts value to bool with a default fallback.
func SafeBoolDefault(value any, defaultVal bool) bool {
	if b, ok := SafeBool(value); ok {
		return b
	}
	return defaultVal
}

// SafeSlice safely asserts value to []any.
// Returns the slice and true if successful, or nil and false if not.
func SafeSlice(value any) ([]any, bool) {
	if value == nil {
		return nil, false
	}
	s, ok := value.([]any)
	return s, ok
}

// SafeSliceDefault safely asserts value to []any with a default fallback.
func SafeSliceDefault(value any, defaultVal []any) []any {
	if s, ok := SafeSlice(value); ok {
		return s
	}
	return defaultVal
}

// SafeStringSlice safely asserts value to []string.
// Also handles []any containing strings.
func SafeStringSlice(value any) ([]string, bool) {
	if value == nil {
		return nil, false
	}

	// Direct type assertion
	if s, ok := value.([]string); ok {
		return s, true
	}

	// Handle []any containing strings (common from JSON)
	if anySlice, ok := value.([]any); ok {
		result := make([]string, 0, len(anySlice))
		for _, item := range anySlice {
			if str, ok := item.(string); ok {
				result = append(result, str)
			} else {
				return nil, false
			}
		}
		return result, true
	}

	return nil, false
}

// SafeStringSliceDefault safely asserts value to []string with a default fallback.
func SafeStringSliceDefault(value any, defaultVal []string) []string {
	if s, ok := SafeStringSlice(value); ok {
		return s
	}
	return defaultVal
}

// MustMapStringAny asserts value to map[string]any or panics with a descriptive error.
// Use this only when the type is guaranteed (e.g., after validation).
func MustMapStringAny(value any, context string) map[string]any {
	if m, ok := SafeMapStringAny(value); ok {
		return m
	}
	panic(fmt.Sprintf("typeutil.MustMapStringAny: expected map[string]any, got %T at %s", value, context))
}

// MustString asserts value to string or panics with a descriptive error.
func MustString(value any, context string) string {
	if s, ok := SafeString(value); ok {
		return s
	}
	panic(fmt.Sprintf("typeutil.MustString: expected string, got %T at %s", value, context))
}

// GetNestedValue safely gets a nested value from a map[string]any using a dot-separated path.
// Example: GetNestedValue(data, "user.profile.name") returns data["user"]["profile"]["name"]
func GetNestedValue(data map[string]any, path string) (any, bool) {
	if data == nil || path == "" {
		return nil, false
	}

	keys := splitPath(path)
	current := any(data)

	for _, key := range keys {
		m, ok := SafeMapStringAny(current)
		if !ok {
			return nil, false
		}
		current, ok = m[key]
		if !ok {
			return nil, false
		}
	}

	return current, true
}

// GetNestedString safely gets a nested string value from a map.
func GetNestedString(data map[string]any, path string) (string, bool) {
	v, ok := GetNestedValue(data, path)
	if !ok {
		return "", false
	}
	return SafeString(v)
}

// GetNestedInt safely gets a nested int value from a map.
func GetNestedInt(data map[string]any, path string) (int, bool) {
	v, ok := GetNestedValue(data, path)
	if !ok {
		return 0, false
	}
	return SafeInt(v)
}

// splitPath splits a dot-separated path into keys.
func splitPath(path string) []string {
	if path == "" {
		return nil
	}
	result := make([]string, 0, 4)
	start := 0
	for i := 0; i < len(path); i++ {
		if path[i] == '.' {
			if i > start {
				result = append(result, path[start:i])
			}
			start = i + 1
		}
	}
	if start < len(path) {
		result = append(result, path[start:])
	}
	return result
}
