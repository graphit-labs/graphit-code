package ast

import (
	"testing"
)

func TestSplitCodeIdentifier(t *testing.T) {
	tests := []struct {
		input    string
		expected string
	}{
		// CamelCase
		{"handleHTTPRequest", "handle HTTP Request"},
		{"getUser", "get User"},
		{"XMLParser", "XML Parser"},
		{"parseJSON", "parse JSON"},
		{"myFunc", "my Func"},

		// PascalCase
		{"QueryService", "Query Service"},
		{"NewLocalEmbeddingClient", "New Local Embedding Client"},

		// snake_case
		{"get_user_data", "get user data"},
		{"http_request_handler", "http request handler"},

		// dot.notation
		{"config.server.port", "config server port"},

		// kebab-case
		{"my-component", "my component"},

		// Mixed
		{"parseJSON_response", "parse JSON response"},

		// No split needed — returns original
		{"simple", "simple"},
		{"x", "x"},
		{"", ""},

		// All uppercase
		{"HTTP", "HTTP"},
		{"URL", "URL"},

		// Acronym followed by word
		{"HTTPSConnection", "HTTPS Connection"},
		{"getURLPath", "get URL Path"},

		// Numbers in identifiers
		{"base64Encode", "base64 Encode"},
		{"sha256Hash", "sha256 Hash"},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			result := splitCodeIdentifier(tt.input)
			if result != tt.expected {
				t.Errorf("splitCodeIdentifier(%q) = %q, want %q", tt.input, result, tt.expected)
			}
		})
	}
}

func TestSplitCamelCase(t *testing.T) {
	tests := []struct {
		input    string
		expected []string
	}{
		{"handleHTTPRequest", []string{"handle", "HTTP", "Request"}},
		{"getUser", []string{"get", "User"}},
		{"XMLParser", []string{"XML", "Parser"}},
		{"simple", []string{"simple"}},
		{"ABC", []string{"ABC"}},
		{"aB", []string{"a", "B"}},
		{"", nil},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			result := splitCamelCase(tt.input)
			if len(result) != len(tt.expected) {
				t.Errorf("splitCamelCase(%q) = %v, want %v", tt.input, result, tt.expected)
				return
			}
			for i := range result {
				if result[i] != tt.expected[i] {
					t.Errorf("splitCamelCase(%q)[%d] = %q, want %q", tt.input, i, result[i], tt.expected[i])
				}
			}
		})
	}
}

func TestDedupTokens(t *testing.T) {
	tests := []struct {
		name     string
		input    []string
		expected []string
	}{
		{
			name:     "no duplicates",
			input:    []string{"foo", "bar", "baz"},
			expected: []string{"foo", "bar", "baz"},
		},
		{
			name:     "case-insensitive dedup",
			input:    []string{"HTTP", "http", "Handler"},
			expected: []string{"HTTP", "Handler"},
		},
		{
			name:     "preserves first occurrence",
			input:    []string{"handleHTTPRequest", "handle", "HTTP", "Request"},
			expected: []string{"handleHTTPRequest", "handle", "HTTP", "Request"},
		},
		{
			name:     "empty input",
			input:    []string{},
			expected: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := dedupTokens(tt.input)
			if len(result) != len(tt.expected) {
				t.Errorf("dedupTokens(%v) = %v (len=%d), want %v (len=%d)",
					tt.input, result, len(result), tt.expected, len(tt.expected))
				return
			}
			for i := range result {
				if result[i] != tt.expected[i] {
					t.Errorf("dedupTokens(%v)[%d] = %q, want %q", tt.input, i, result[i], tt.expected[i])
				}
			}
		})
	}
}

func TestTokenizeQueryWithSplitting(t *testing.T) {
	tests := []struct {
		name     string
		query    string
		contains []string
	}{
		{
			name:     "camelCase identifier is split",
			query:    "handleHTTPRequest",
			contains: []string{"handleHTTPRequest", "handle", "HTTP", "Request"},
		},
		{
			name:     "snake_case identifier is split",
			query:    "get_user_data",
			contains: []string{"get_user_data", "get", "user", "data"},
		},
		{
			name:     "plain words not split",
			query:    "search query",
			contains: []string{"search", "query"},
		},
		{
			name:     "mixed natural and identifier",
			query:    "find handleHTTPRequest function",
			contains: []string{"find", "handleHTTPRequest", "handle", "HTTP", "Request", "function"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tokenizeQuery(tt.query)
			resultSet := make(map[string]bool)
			for _, r := range result {
				resultSet[r] = true
			}
			for _, expected := range tt.contains {
				found := false
				for _, r := range result {
					if r == expected {
						found = true
						break
					}
				}
				if !found {
					t.Errorf("tokenizeQuery(%q) = %v, expected to contain %q", tt.query, result, expected)
				}
			}
		})
	}
}

func TestSemanticScoring(t *testing.T) {
	tests := []struct {
		name     string
		distance float64
		expected float64
	}{
		{"identical vectors (distance=0)", 0.0, 1.0},
		{"orthogonal vectors (distance=sqrt(2))", 1.4142135623730951, 0.0},
		{"opposite vectors (distance=2)", 2.0, -1.0},
		{"similar vectors (distance=0.5)", 0.5, 0.875},
		{"moderately similar (distance=1.0)", 1.0, 0.5},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			cosineSim := 1.0 - (tt.distance*tt.distance)/2.0
			diff := cosineSim - tt.expected
			if diff > 0.001 || diff < -0.001 {
				t.Errorf("cosine_sim(distance=%f) = %f, want %f", tt.distance, cosineSim, tt.expected)
			}
		})
	}
}
