package ide

import (
	"strings"
	"testing"

	"gopkg.in/yaml.v3"
)

// The values below are the ones a hand-rolled quoter gets wrong. The first two
// are what actually shipped broken — prose with ": " in it — and the rest are
// the cases that would have followed: a quote character, a backslash, YAML's
// reserved indicators, plain scalars that resolve to a non-string type, and
// whitespace that a naive writer loses.
func TestSkillFrontmatterSurvivesValuesThatBreakHandWrittenYAML(t *testing.T) {
	t.Parallel()

	descriptions := map[string]string{
		"colon space":            "Use when: the request names a symbol. MANDATORY: read it first.",
		"colon at end of clause": "AST first. Use for: callers, imports, inheritance.",
		"double quote":           `The agent said "read the schema first" and meant it.`,
		"single quote":           "Mind the project's own conventions.",
		"backslash":              `Escapes like \n and \" are data here, not syntax.`,
		"leading percent":        "%YAML looking line at the start",
		"leading hash":           "# not a heading",
		"leading dash":           "- not a list item",
		"leading question":       "? not a complex key",
		"leading ampersand":      "&anchor is not an anchor here",
		"leading asterisk":       "*alias is not an alias here",
		"looks like bool":        "yes",
		"looks like null":        "null",
		"looks like number":      "1024",
		"looks like a date":      "2026-08-12",
		"trailing space":         "trailing space matters  ",
		"leading space":          "  leading space matters",
		"tab inside":             "before\tafter",
		"newline inside":         "first line\nsecond line",
		"non ascii":              "em dash — and accents: ação, coração",
		"brace and bracket":      "{not a map} [not a list]",
	}

	for name, description := range descriptions {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			block, err := SkillFrontmatter("graphit-ast", description)
			if err != nil {
				t.Fatalf("SkillFrontmatter(%q) returned error: %v", description, err)
			}

			var parsed skillFrontmatter
			if err := yaml.Unmarshal([]byte(frontmatterBody(t, block)), &parsed); err != nil {
				t.Fatalf("frontmatter does not parse as YAML: %v\nblock:\n%s", err, block)
			}
			if parsed.Name != "graphit-ast" {
				t.Errorf("name = %q, want %q", parsed.Name, "graphit-ast")
			}
			if parsed.Description != description {
				t.Errorf("description did not survive the round trip:\n got %q\nwant %q", parsed.Description, description)
			}
		})
	}
}

func TestSkillFrontmatterShape(t *testing.T) {
	t.Parallel()

	block, err := SkillFrontmatter("graphit-memory", "Use when: recalling anything.")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(block, "---\n") {
		t.Errorf("block does not open with a frontmatter delimiter:\n%s", block)
	}
	if !strings.HasSuffix(block, "---\n\n") {
		t.Errorf("block does not close with a delimiter and a blank line:\n%q", block)
	}

	var mapping map[string]string
	if err := yaml.Unmarshal([]byte(frontmatterBody(t, block)), &mapping); err != nil {
		t.Fatal(err)
	}
	if len(mapping) != 2 {
		t.Errorf("frontmatter has %d keys (%v), want exactly name and description", len(mapping), mapping)
	}
}

func TestSkillFrontmatterRejectsWhatAnIDEWouldSilentlyDrop(t *testing.T) {
	t.Parallel()

	longName := strings.Repeat("a", MaxSkillNameLength+1)
	longDescription := strings.Repeat("d", MaxSkillDescriptionLength+1)

	invalid := []struct {
		name        string
		skillName   string
		description string
	}{
		{"empty name", "", "fine"},
		{"uppercase name", "Graphit-AST", "fine"},
		{"underscore in name", "graphit_ast", "fine"},
		{"space in name", "graphit ast", "fine"},
		{"leading hyphen", "-graphit", "fine"},
		{"trailing hyphen", "graphit-", "fine"},
		{"double hyphen", "graphit--ast", "fine"},
		{"name over the limit", longName, "fine"},
		{"empty description", "graphit-ast", ""},
		{"blank description", "graphit-ast", "   \n\t "},
		{"description over the limit", "graphit-ast", longDescription},
	}

	for _, tc := range invalid {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			if _, err := SkillFrontmatter(tc.skillName, tc.description); err == nil {
				t.Errorf("SkillFrontmatter(%q, <%d chars>) returned no error", tc.skillName, len(tc.description))
			}
		})
	}

	t.Run("at the limits", func(t *testing.T) {
		t.Parallel()
		atLimit := strings.Repeat("a", MaxSkillNameLength)
		if _, err := SkillFrontmatter(atLimit, strings.Repeat("d", MaxSkillDescriptionLength)); err != nil {
			t.Errorf("values exactly at the limits were rejected: %v", err)
		}
	})

	t.Run("multibyte description inside the limit", func(t *testing.T) {
		t.Parallel()
		description := strings.Repeat("—", MaxSkillDescriptionLength)
		if _, err := SkillFrontmatter("graphit-ast", description); err != nil {
			t.Errorf("%d-character multibyte description rejected: %v", MaxSkillDescriptionLength, err)
		}
	})
}

func frontmatterBody(t *testing.T, block string) string {
	t.Helper()
	trimmed := strings.TrimPrefix(block, "---\n")
	end := strings.Index(trimmed, "\n---")
	if end < 0 {
		t.Fatalf("no closing delimiter in frontmatter block:\n%s", block)
	}
	return trimmed[:end]
}
