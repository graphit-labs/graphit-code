package ast

import (
	"testing"
)

// ---------------------------------------------------------------------------
// scoreFunctionYAML
// ---------------------------------------------------------------------------

func TestScoreFunctionYAML(t *testing.T) {
	tests := []struct {
		name          string
		funcName      string
		decorators    string
		isExported    bool
		lang          string
		nameRules     []NameScoreRule
		decRules      []DecoratorScoreRule
		exportBonus   int
		maxScore      int
		expectedScore int
	}{
		{
			name:          "no_rules_no_score",
			funcName:      "helper",
			decorators:    "",
			isExported:    false,
			lang:          "go",
			nameRules:     nil,
			decRules:      nil,
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 0,
		},
		{
			name:     "exact_name_match",
			funcName: "main",
			nameRules: []NameScoreRule{
				{Pattern: "main", Score: 50},
			},
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 50,
		},
		{
			name:     "prefix_wildcard_match",
			funcName: "TestMyFunction",
			nameRules: []NameScoreRule{
				{Pattern: "Test*", Score: 30},
			},
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 30,
		},
		{
			name:     "suffix_wildcard_match",
			funcName: "MyHandler",
			nameRules: []NameScoreRule{
				{Pattern: "*Handler", Score: 25},
			},
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 25,
		},
		{
			name:     "contains_wildcard_match",
			funcName: "processUserInput",
			nameRules: []NameScoreRule{
				{Pattern: "*user*", Score: 15},
			},
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 15,
		},
		{
			name:       "exported_bonus",
			funcName:   "SomeFunc",
			isExported: true,
			nameRules: []NameScoreRule{
				{Pattern: "SomeFunc", Score: 20},
			},
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 30, // 20 + 10
		},
		{
			name:     "decorator_match",
			funcName: "handle_request",
			decRules: []DecoratorScoreRule{
				{Name: "route", Score: 40},
			},
			decorators:    "app.route",
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 40,
		},
		{
			name:     "decorator_exact_match",
			funcName: "handler",
			decRules: []DecoratorScoreRule{
				{Name: "Get", Score: 35},
			},
			decorators:    "Get",
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 35,
		},
		{
			name:     "capped_at_max",
			funcName: "main",
			nameRules: []NameScoreRule{
				{Pattern: "main", Score: 80},
				{Pattern: "*", Score: 50},
			},
			isExported:    true,
			exportBonus:   10,
			maxScore:      100,
			expectedScore: 100, // 80+50+10 = 140, capped at 100
		},
		{
			name:     "case_insensitive_name",
			funcName: "Main",
			nameRules: []NameScoreRule{
				{Pattern: "main", Score: 50},
			},
			exportBonus:   0,
			maxScore:      100,
			expectedScore: 50,
		},
		{
			name:     "multiple_rules_accumulate",
			funcName: "TestMainHandler",
			nameRules: []NameScoreRule{
				{Pattern: "Test*", Score: 10},
				{Pattern: "*Handler", Score: 15},
			},
			exportBonus:   0,
			maxScore:      100,
			expectedScore: 25,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			score := scoreFunctionYAML(
				tt.funcName, tt.decorators, tt.isExported, tt.lang,
				tt.nameRules, tt.decRules, tt.exportBonus, tt.maxScore,
			)
			if score != tt.expectedScore {
				t.Errorf("scoreFunctionYAML() = %d, want %d", score, tt.expectedScore)
			}
		})
	}
}

// ---------------------------------------------------------------------------
// compileImportRegex
// ---------------------------------------------------------------------------

func TestCompileImportRegex(t *testing.T) {
	t.Run("non_regex_returns_nil", func(t *testing.T) {
		re := compileImportRegex("exact", "some.pattern")
		if re != nil {
			t.Error("expected nil for non-regex match")
		}
	})

	t.Run("valid_regex_compiles", func(t *testing.T) {
		re := compileImportRegex("regex", `^django\..*`)
		if re == nil {
			t.Fatal("expected non-nil compiled regex")
		}
		if !re.MatchString("django.core") {
			t.Error("expected regex to match 'django.core'")
		}
	})

	t.Run("invalid_regex_returns_nil", func(t *testing.T) {
		re := compileImportRegex("regex", `[invalid`)
		if re != nil {
			t.Error("expected nil for invalid regex")
		}
	})

	t.Run("prefix_match_returns_nil", func(t *testing.T) {
		re := compileImportRegex("prefix", "react")
		if re != nil {
			t.Error("expected nil for prefix match type")
		}
	})
}
