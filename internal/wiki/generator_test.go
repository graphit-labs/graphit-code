package wiki

import "testing"

func TestSafeSlug(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{"simple", "hello", "hello"},
		{"spaces_to_underscores", "hello world", "hello_world"},
		{"slashes_to_dashes", "path/to/file", "path-to-file"},
		{"backslashes_to_dashes", "path\\to\\file", "path-to-file"},
		{"colons_to_dashes", "title: subtitle", "title-_subtitle"},
		{"question_marks_removed", "what?", "what"},
		{"asterisks_removed", "star*", "star"},
		{"unicode_letters_preserved", "café", "café"},
		{"numbers_preserved", "page123", "page123"},
		{"dots_preserved", "file.name", "file.name"},
		{"multiple_underscores_collapsed", "a___b", "a_b"},
		{"multiple_dashes_collapsed", "a---b", "a-b"},
		{"leading_trailing_stripped", "_hello_", "hello"},
		{"leading_dash_stripped", "-hello-", "hello"},
		{"empty_string", "", ""},
		{"only_special_chars", "???***", ""},
		{"complex_mixed", "My Page: What/Why? *important*", "My_Page-_What-Why_important"},
		{"mixed_underscores_and_dashes", "__--hello--__", "hello"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := SafeSlug(tt.input)
			if got != tt.want {
				t.Errorf("SafeSlug(%q) = %q, want %q", tt.input, got, tt.want)
			}
		})
	}
}
