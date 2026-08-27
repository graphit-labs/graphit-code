package ast

import (
	"strings"
	"testing"
)

func TestFormatRecordsTOON_Escaping(t *testing.T) {
	t.Run("pipe_escaped", func(t *testing.T) {
		records := []QueryRecord{
			{"name": "foo(a|b)", "path": "a.go"},
		}
		got := FormatRecordsTOON(records)
		if !strings.Contains(got, `foo(a\|b)`) {
			t.Errorf("expected pipe escaped as \\|, got:\n%s", got)
		}
	})

	t.Run("newline_escaped", func(t *testing.T) {
		records := []QueryRecord{
			{"name": "Foo", "source": "func foo() {\n\treturn 42\n}"},
		}
		got := FormatRecordsTOON(records)
		if strings.Contains(got, "\n\t") {
			t.Errorf("expected newline escaped, got literal newline in TOON:\n%s", got)
		}
		if !strings.Contains(got, `func foo() {\n`) {
			t.Errorf("expected escaped newline \\n in output, got:\n%s", got)
		}
	})

	t.Run("backslash_escaped", func(t *testing.T) {
		records := []QueryRecord{
			{"name": "Foo", "path": `C:\Users\foo`},
		}
		got := FormatRecordsTOON(records)
		if !strings.Contains(got, `C:\\Users\\foo`) {
			t.Errorf("expected backslash escaped as \\\\, got:\n%s", got)
		}
	})
}

func TestFormatRecordsTOON(t *testing.T) {
	t.Run("empty_records", func(t *testing.T) {
		got := FormatRecordsTOON(nil)
		if got != "results[0]{}:" {
			t.Errorf("expected empty header, got %q", got)
		}
	})

	t.Run("single_record", func(t *testing.T) {
		records := []QueryRecord{
			{"name": "Func", "path": "a.go"},
		}
		got := FormatRecordsTOON(records)
		if !strings.Contains(got, "results[1]") {
			t.Errorf("expected results[1], got %q", got)
		}
		if !strings.Contains(got, "Func") {
			t.Errorf("expected 'Func' in output, got %q", got)
		}
		if !strings.Contains(got, "a.go") {
			t.Errorf("expected 'a.go' in output, got %q", got)
		}
	})

	t.Run("multiple_records", func(t *testing.T) {
		records := []QueryRecord{
			{"name": "A", "line": 10},
			{"name": "B", "line": 20},
		}
		got := FormatRecordsTOON(records)
		if !strings.Contains(got, "results[2]") {
			t.Errorf("expected results[2], got %q", got)
		}
	})

	t.Run("columns_sorted", func(t *testing.T) {
		records := []QueryRecord{
			{"z_col": "last", "a_col": "first"},
		}
		got := FormatRecordsTOON(records)
		idx_a := strings.Index(got, "a_col")
		idx_z := strings.Index(got, "z_col")
		if idx_a > idx_z {
			t.Errorf("expected columns sorted alphabetically, got %q", got)
		}
	})

	t.Run("label_column_renamed", func(t *testing.T) {
		records := []QueryRecord{
			{"LABEL(n)": "Function", "name": "foo"},
		}
		got := FormatRecordsTOON(records)
		// The header should use "label" instead of "LABEL(n)"
		lines := strings.SplitN(got, "\n", 2)
		if !strings.Contains(lines[0], "label") {
			t.Errorf("expected 'label' in header, got %q", lines[0])
		}
	})

	t.Run("pipe_separated", func(t *testing.T) {
		records := []QueryRecord{
			{"a": "1", "b": "2"},
		}
		got := FormatRecordsTOON(records)
		lines := strings.Split(got, "\n")
		if len(lines) < 2 {
			t.Fatalf("expected at least 2 lines, got %d", len(lines))
		}
		// Data line should contain pipe separator
		if !strings.Contains(lines[1], "|") {
			t.Errorf("expected pipe separator in data line, got %q", lines[1])
		}
	})
}

func TestFormatTOONValue(t *testing.T) {
	tests := []struct {
		name  string
		input any
		want  string
	}{
		{"nil", nil, ""},
		{"string", "hello", "hello"},
		{"int", 42, "42"},
		{"int64", int64(100), "100"},
		{"float_whole", float64(10), "10"},
		{"float_decimal", float64(3.14), "3.14"},
		{"bool_true", true, "true"},
		{"bool_false", false, "false"},
		{"list", []any{"a", "b"}, "[a,b]"},
		{"map", map[string]any{"k": "v"}, "{k:v}"},
		{"nested_list", []any{1, "two"}, "[1,two]"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := formatTOONValue(tt.input)
			if got != tt.want {
				t.Errorf("formatTOONValue(%v) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}
