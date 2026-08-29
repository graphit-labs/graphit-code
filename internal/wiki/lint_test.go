package wiki

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestHasIssues(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name   string
		errors int
		want   bool
	}{
		{"zero_errors", 0, false},
		{"one_error", 1, true},
		{"many_errors", 42, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			r := &LintReport{Errors: tt.errors}
			if got := r.HasIssues(); got != tt.want {
				t.Errorf("HasIssues() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestSummary(t *testing.T) {
	t.Parallel()
	t.Run("no_issues", func(t *testing.T) {
		t.Parallel()
		r := &LintReport{TotalPages: 5, Errors: 0}
		got := r.Summary()
		if !strings.Contains(got, "5 pages") || !strings.Contains(got, "no issues") {
			t.Errorf("Summary() = %q, expected page count and no issues", got)
		}
	})

	t.Run("with_issues", func(t *testing.T) {
		t.Parallel()
		r := &LintReport{TotalPages: 10, Errors: 3}
		got := r.Summary()
		if !strings.Contains(got, "10 pages") || !strings.Contains(got, "3 issue") {
			t.Errorf("Summary() = %q, expected page count and issue count", got)
		}
	})
}

func TestLintWiki(t *testing.T) {
	t.Parallel()

	t.Run("clean_wiki", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "index.md", "# Index\n[[alpha]] [[beta]]")
		writeFile(t, dir, "alpha.md", fmPage("Alpha", "alpha content here with enough words to pass empty check and more text"))
		writeFile(t, dir, "beta.md", fmPage("Beta", "beta content here with enough words to pass empty check and more text"))

		report, err := LintWiki(dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if report.TotalPages != 3 {
			t.Errorf("TotalPages = %d, want 3", report.TotalPages)
		}
	})

	t.Run("orphan_detection", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "index.md", "# Index\nNothing linked.")
		writeFile(t, dir, "orphan.md", fmPage("Orphan", "orphan content here with enough words to not be empty page verified"))

		report, err := LintWiki(dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.Orphans) == 0 {
			t.Error("expected orphan pages")
		}
	})

	t.Run("broken_links", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "page.md", fmPage("Page", "[[nonexistent]] link here with enough words to pass empty check"))

		report, err := LintWiki(dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.BrokenLinks) == 0 {
			t.Error("expected broken links")
		}
	})

	t.Run("stale_pages", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "index.md", "# Index\n[[stale]]")
		staleContent := "---\ntype: document\ntitle: Stale\ngenerated: { by: process:graphit-knowledge-wiki, at: 2020-01-01 }\n---\n# Stale\nStale page content here with enough words to pass the empty page check."
		writeFile(t, dir, "stale.md", staleContent)

		report, err := LintWiki(dir, LintConfig{StaleDays: 30})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.StalePages) == 0 {
			t.Error("expected stale pages")
		}
	})

	t.Run("empty_pages", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "empty.md", fmPage("Empty", "short"))

		report, err := LintWiki(dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.EmptyPages) == 0 {
			t.Error("expected empty pages")
		}
	})

	t.Run("missing_frontmatter_fields", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		writeFile(t, dir, "page.md", "---\ntitle: Test\n---\n# Page\nContent here with enough words to not be empty page with many more words.")

		report, err := LintWiki(dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.MissingFields) == 0 {
			t.Error("expected missing fields (tags, updated)")
		}
	})

	t.Run("invalid_dir", func(t *testing.T) {
		t.Parallel()
		_, err := LintWiki(filepath.Join(t.TempDir(), "nonexistent"), LintConfig{})
		if err == nil {
			t.Error("expected error for invalid dir")
		}
	})
}

func TestLintWikiWithFix(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	writeFile(t, dir, "source.md", fmPage("Source", "[[target]] reference with enough words to pass the empty page check and more"))
	writeFile(t, dir, "target.md", fmPage("Target", "target content here with enough words to pass the empty page check and more"))

	report, err := LintWiki(dir, LintConfig{Fix: true})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if report.FixesApplied == 0 {
		t.Error("expected fixes to be applied (backlinks)")
	}

	data, _ := os.ReadFile(filepath.Join(dir, "target.md"))
	if !strings.Contains(string(data), "## Backlinks") {
		t.Error("expected backlinks section after fix")
	}
}

func TestCheckFrontmatter(t *testing.T) {
	t.Parallel()
	// OKF v0.2 §11 requires exactly one field of a concept document: a non-empty `type`.
	// Anything else is RECOMMENDED, and §11 forbids a consumer rejecting a document for
	// missing an optional field — so `title`, `tags` and `updated` are not failures.
	tests := []struct {
		name    string
		content string
		wantLen int
	}{
		{
			"okf_minimum_is_type_alone",
			"---\ntype: specification\n---\n# Body",
			0,
		},
		{
			"generated_page_shape",
			"---\ntype: document\ntitle: Test\ngenerated: { by: process:graphit-knowledge-wiki, at: 2026-08-29 }\ntags:\n  - knowledge\n---\n# Body",
			0,
		},
		{
			"recommended_fields_alone_are_not_conformant",
			"---\ntitle: Test\ndescription: A page\n---\n# Body",
			1,
		},
		{
			"no_frontmatter",
			"# Body\nNo frontmatter.",
			1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := checkFrontmatter(tt.content)
			if len(got) != tt.wantLen {
				t.Errorf("checkFrontmatter() missing fields = %v (len %d), want len %d", got, len(got), tt.wantLen)
			}
		})
	}
}

func TestExtractFrontmatter(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{
			"valid",
			"---\ntitle: Test\ntags: [a]\n---\n# Body",
			"title: Test\ntags: [a]",
		},
		{
			"no_frontmatter",
			"# Body\nNo frontmatter.",
			"",
		},
		{
			"unclosed",
			"---\ntitle: Test\nno closing",
			"title: Test\nno closing",
		},
		{
			"empty_frontmatter",
			"---\n---\n# Body",
			"",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := extractFrontmatter(tt.content)
			if got != tt.want {
				t.Errorf("extractFrontmatter() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestIsStale(t *testing.T) {
	t.Parallel()

	today := time.Now().Format("2006-01-02")
	oldDate := "2020-01-01"

	tests := []struct {
		name      string
		content   string
		staleDays int
		want      bool
	}{
		{
			"recent_not_stale",
			fmt.Sprintf("---\ntype: document\ngenerated: { by: process:x, at: %s }\n---\nBody", today),
			30,
			false,
		},
		{
			"old_is_stale",
			fmt.Sprintf("---\ntype: document\ngenerated: { by: process:x, at: %s }\n---\nBody", oldDate),
			30,
			true,
		},
		{
			// A page whose age is unknown is not a page known to be old. Reporting it as
			// stale invents a fact, and §11 forbids rejecting a concept over a missing
			// optional field — which `generated` is.
			"no_date_is_not_stale",
			"---\ntype: document\ntitle: T\n---\nBody",
			30,
			false,
		},
		{
			"invalid_date_format",
			"---\ntype: document\ngenerated: { by: process:x, at: not-a-date }\n---\nBody",
			30,
			false,
		},
		{
			"okf_generated_inline_mapping_recent",
			fmt.Sprintf("---\ntype: document\ngenerated: { by: process:graphit-knowledge-wiki, at: %s }\n---\nBody", today),
			30,
			false,
		},
		{
			"okf_generated_inline_mapping_old",
			fmt.Sprintf("---\ntype: document\ngenerated: { by: process:graphit-knowledge-wiki, at: %s }\n---\nBody", oldDate),
			30,
			true,
		},
		{
			"okf_generated_block_mapping_old",
			fmt.Sprintf("---\ntype: document\ngenerated:\n  by: process:graphit-knowledge-wiki\n  at: %s\n---\nBody", oldDate),
			30,
			true,
		},
		{
			// §5.5: an absolute instant beats any window the caller passes in.
			"stale_after_in_the_past_wins",
			fmt.Sprintf("---\ntype: document\nstale_after: %s\ngenerated: { by: process:x, at: %s }\n---\nBody", oldDate, today),
			30,
			true,
		},
		{
			"stale_after_in_the_future_wins",
			fmt.Sprintf("---\ntype: document\nstale_after: %s\ngenerated: { by: process:x, at: %s }\n---\nBody",
				time.Now().AddDate(1, 0, 0).Format("2006-01-02"), oldDate),
			30,
			false,
		},
		{
			"quoted_date",
			fmt.Sprintf("---\ntype: document\ngenerated: { by: process:x, at: \"%s\" }\n---\nBody", today),
			30,
			false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			got := isStale(tt.content, tt.staleDays)
			if got != tt.want {
				t.Errorf("isStale() = %v, want %v", got, tt.want)
			}
		})
	}
}

func fmPage(title, body string) string {
	today := time.Now().Format("2006-01-02")
	return fmt.Sprintf("---\ntype: document\ntitle: %s\ngenerated: { by: process:graphit-knowledge-wiki, at: %s }\ntags:\n  - test\n---\n# %s\n%s", title, today, title, body)
}
