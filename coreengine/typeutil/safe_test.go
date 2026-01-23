package typeutil

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// =============================================================================
// MAP[STRING]ANY TESTS
// =============================================================================

func TestSafeMapStringAny(t *testing.T) {
	tests := []struct {
		name     string
		input    any
		wantMap  map[string]any
		wantBool bool
	}{
		{
			name:     "valid map",
			input:    map[string]any{"key": "value"},
			wantMap:  map[string]any{"key": "value"},
			wantBool: true,
		},
		{
			name:     "nil value",
			input:    nil,
			wantMap:  nil,
			wantBool: false,
		},
		{
			name:     "wrong type string",
			input:    "not a map",
			wantMap:  nil,
			wantBool: false,
		},
		{
			name:     "wrong type int",
			input:    42,
			wantMap:  nil,
			wantBool: false,
		},
		{
			name:     "empty map",
			input:    map[string]any{},
			wantMap:  map[string]any{},
			wantBool: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := SafeMapStringAny(tt.input)
			assert.Equal(t, tt.wantBool, ok)
			assert.Equal(t, tt.wantMap, got)
		})
	}
}

func TestSafeMapStringAnyDefault(t *testing.T) {
	defaultVal := map[string]any{"default": true}

	// Valid map - should return the map
	result := SafeMapStringAnyDefault(map[string]any{"key": "value"}, defaultVal)
	assert.Equal(t, "value", result["key"])

	// Invalid type - should return default
	result = SafeMapStringAnyDefault("not a map", defaultVal)
	assert.Equal(t, defaultVal, result)

	// Nil - should return default
	result = SafeMapStringAnyDefault(nil, defaultVal)
	assert.Equal(t, defaultVal, result)
}

// =============================================================================
// STRING TESTS
// =============================================================================

func TestSafeString(t *testing.T) {
	tests := []struct {
		name       string
		input      any
		wantString string
		wantBool   bool
	}{
		{
			name:       "valid string",
			input:      "hello",
			wantString: "hello",
			wantBool:   true,
		},
		{
			name:       "empty string",
			input:      "",
			wantString: "",
			wantBool:   true,
		},
		{
			name:       "nil value",
			input:      nil,
			wantString: "",
			wantBool:   false,
		},
		{
			name:       "wrong type int",
			input:      42,
			wantString: "",
			wantBool:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := SafeString(tt.input)
			assert.Equal(t, tt.wantBool, ok)
			assert.Equal(t, tt.wantString, got)
		})
	}
}

func TestSafeStringDefault(t *testing.T) {
	assert.Equal(t, "hello", SafeStringDefault("hello", "default"))
	assert.Equal(t, "default", SafeStringDefault(nil, "default"))
	assert.Equal(t, "default", SafeStringDefault(42, "default"))
}

// =============================================================================
// INT TESTS
// =============================================================================

func TestSafeInt(t *testing.T) {
	tests := []struct {
		name     string
		input    any
		wantInt  int
		wantBool bool
	}{
		{
			name:     "int value",
			input:    42,
			wantInt:  42,
			wantBool: true,
		},
		{
			name:     "int64 value",
			input:    int64(100),
			wantInt:  100,
			wantBool: true,
		},
		{
			name:     "int32 value",
			input:    int32(50),
			wantInt:  50,
			wantBool: true,
		},
		{
			name:     "float64 value from JSON",
			input:    float64(123),
			wantInt:  123,
			wantBool: true,
		},
		{
			name:     "nil value",
			input:    nil,
			wantInt:  0,
			wantBool: false,
		},
		{
			name:     "string value",
			input:    "42",
			wantInt:  0,
			wantBool: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := SafeInt(tt.input)
			assert.Equal(t, tt.wantBool, ok)
			assert.Equal(t, tt.wantInt, got)
		})
	}
}

func TestSafeIntDefault(t *testing.T) {
	assert.Equal(t, 42, SafeIntDefault(42, 0))
	assert.Equal(t, 99, SafeIntDefault(nil, 99))
	assert.Equal(t, 99, SafeIntDefault("not int", 99))
}

// =============================================================================
// FLOAT64 TESTS
// =============================================================================

func TestSafeFloat64(t *testing.T) {
	tests := []struct {
		name      string
		input     any
		wantFloat float64
		wantBool  bool
	}{
		{
			name:      "float64 value",
			input:     3.14,
			wantFloat: 3.14,
			wantBool:  true,
		},
		{
			name:      "int value",
			input:     42,
			wantFloat: 42.0,
			wantBool:  true,
		},
		{
			name:      "nil value",
			input:     nil,
			wantFloat: 0,
			wantBool:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := SafeFloat64(tt.input)
			assert.Equal(t, tt.wantBool, ok)
			assert.Equal(t, tt.wantFloat, got)
		})
	}
}

// =============================================================================
// BOOL TESTS
// =============================================================================

func TestSafeBool(t *testing.T) {
	tests := []struct {
		name     string
		input    any
		wantBool bool
		wantOk   bool
	}{
		{
			name:     "true value",
			input:    true,
			wantBool: true,
			wantOk:   true,
		},
		{
			name:     "false value",
			input:    false,
			wantBool: false,
			wantOk:   true,
		},
		{
			name:     "nil value",
			input:    nil,
			wantBool: false,
			wantOk:   false,
		},
		{
			name:     "string value",
			input:    "true",
			wantBool: false,
			wantOk:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := SafeBool(tt.input)
			assert.Equal(t, tt.wantOk, ok)
			assert.Equal(t, tt.wantBool, got)
		})
	}
}

func TestSafeBoolDefault(t *testing.T) {
	assert.True(t, SafeBoolDefault(true, false))
	assert.False(t, SafeBoolDefault(false, true))
	assert.True(t, SafeBoolDefault(nil, true))
	assert.False(t, SafeBoolDefault("not bool", false))
}

// =============================================================================
// SLICE TESTS
// =============================================================================

func TestSafeSlice(t *testing.T) {
	tests := []struct {
		name      string
		input     any
		wantSlice []any
		wantBool  bool
	}{
		{
			name:      "valid slice",
			input:     []any{1, "two", 3.0},
			wantSlice: []any{1, "two", 3.0},
			wantBool:  true,
		},
		{
			name:      "nil value",
			input:     nil,
			wantSlice: nil,
			wantBool:  false,
		},
		{
			name:      "wrong type",
			input:     "not a slice",
			wantSlice: nil,
			wantBool:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := SafeSlice(tt.input)
			assert.Equal(t, tt.wantBool, ok)
			assert.Equal(t, tt.wantSlice, got)
		})
	}
}

func TestSafeStringSlice(t *testing.T) {
	tests := []struct {
		name      string
		input     any
		wantSlice []string
		wantBool  bool
	}{
		{
			name:      "direct string slice",
			input:     []string{"a", "b", "c"},
			wantSlice: []string{"a", "b", "c"},
			wantBool:  true,
		},
		{
			name:      "any slice with strings",
			input:     []any{"a", "b", "c"},
			wantSlice: []string{"a", "b", "c"},
			wantBool:  true,
		},
		{
			name:      "any slice with mixed types",
			input:     []any{"a", 1, "c"},
			wantSlice: nil,
			wantBool:  false,
		},
		{
			name:      "nil value",
			input:     nil,
			wantSlice: nil,
			wantBool:  false,
		},
		{
			name:      "wrong type",
			input:     "not a slice",
			wantSlice: nil,
			wantBool:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := SafeStringSlice(tt.input)
			assert.Equal(t, tt.wantBool, ok)
			assert.Equal(t, tt.wantSlice, got)
		})
	}
}

// =============================================================================
// MUST FUNCTIONS TESTS
// =============================================================================

func TestMustMapStringAny_Success(t *testing.T) {
	input := map[string]any{"key": "value"}
	result := MustMapStringAny(input, "test context")
	assert.Equal(t, input, result)
}

func TestMustMapStringAny_Panic(t *testing.T) {
	assert.Panics(t, func() {
		MustMapStringAny("not a map", "test context")
	})
}

func TestMustString_Success(t *testing.T) {
	result := MustString("hello", "test context")
	assert.Equal(t, "hello", result)
}

func TestMustString_Panic(t *testing.T) {
	assert.Panics(t, func() {
		MustString(42, "test context")
	})
}

// =============================================================================
// NESTED VALUE TESTS
// =============================================================================

func TestGetNestedValue(t *testing.T) {
	data := map[string]any{
		"user": map[string]any{
			"profile": map[string]any{
				"name": "John",
				"age":  30,
			},
		},
		"simple": "value",
	}

	tests := []struct {
		name      string
		path      string
		wantValue any
		wantBool  bool
	}{
		{
			name:      "simple path",
			path:      "simple",
			wantValue: "value",
			wantBool:  true,
		},
		{
			name:      "nested path",
			path:      "user.profile.name",
			wantValue: "John",
			wantBool:  true,
		},
		{
			name:      "nested int",
			path:      "user.profile.age",
			wantValue: 30,
			wantBool:  true,
		},
		{
			name:      "missing key",
			path:      "user.missing",
			wantValue: nil,
			wantBool:  false,
		},
		{
			name:      "empty path",
			path:      "",
			wantValue: nil,
			wantBool:  false,
		},
		{
			name:      "path through non-map",
			path:      "simple.nested",
			wantValue: nil,
			wantBool:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := GetNestedValue(data, tt.path)
			assert.Equal(t, tt.wantBool, ok)
			assert.Equal(t, tt.wantValue, got)
		})
	}
}

func TestGetNestedValue_NilMap(t *testing.T) {
	_, ok := GetNestedValue(nil, "any.path")
	assert.False(t, ok)
}

func TestGetNestedString(t *testing.T) {
	data := map[string]any{
		"user": map[string]any{
			"name": "John",
		},
	}

	name, ok := GetNestedString(data, "user.name")
	require.True(t, ok)
	assert.Equal(t, "John", name)

	// Non-existent path
	_, ok = GetNestedString(data, "user.missing")
	assert.False(t, ok)
}

func TestGetNestedInt(t *testing.T) {
	data := map[string]any{
		"config": map[string]any{
			"port": 8080,
		},
	}

	port, ok := GetNestedInt(data, "config.port")
	require.True(t, ok)
	assert.Equal(t, 8080, port)

	// Non-existent path
	_, ok = GetNestedInt(data, "config.missing")
	assert.False(t, ok)
}

// =============================================================================
// SPLIT PATH TESTS
// =============================================================================

func TestSplitPath(t *testing.T) {
	tests := []struct {
		path string
		want []string
	}{
		{"", nil},
		{"simple", []string{"simple"}},
		{"a.b.c", []string{"a", "b", "c"}},
		{"user.profile.name", []string{"user", "profile", "name"}},
	}

	for _, tt := range tests {
		t.Run(tt.path, func(t *testing.T) {
			got := splitPath(tt.path)
			assert.Equal(t, tt.want, got)
		})
	}
}
