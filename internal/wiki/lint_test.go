//go:build lancedb

package wiki

import (
	"context"
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

func lintChunk(slug, title string) WikiChunk {
	return WikiChunk{
		Slug:      slug,
		Title:     title,
		Body:      "content here with enough words to pass the empty page check and then some more",
		Summary:   "a summary",
		DocType:   "document",
		WordCount: 15,
		Updated:   time.Now().Format("2006-01-02"),
		ClusterID: -1,
	}
}

func indexedWikiWithXRefs(t *testing.T, chunks []WikiChunk, xrefs map[string][]string) string {
	t.Helper()
	dir := t.TempDir()
	if err := SyncDB(context.Background(), dir, chunks, xrefs, nil); err != nil {
		t.Fatalf("building the lint fixture index: %v", err)
	}
	return dir
}

func TestLintWiki(t *testing.T) {
	t.Parallel()

	t.Run("clean_wiki", func(t *testing.T) {
		t.Parallel()
		dir := indexedWikiWithXRefs(t,
			[]WikiChunk{lintChunk("alpha", "Alpha"), lintChunk("beta", "Beta")},
			map[string][]string{"alpha": {"beta"}, "beta": {"alpha"}})

		report, err := LintWiki(context.Background(), dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if report.TotalPages != 2 {
			t.Errorf("TotalPages = %d, want 2", report.TotalPages)
		}
		if report.Errors != 0 {
			t.Errorf("Errors = %d, want 0 (report: %+v)", report.Errors, report)
		}
	})

	t.Run("orphan_detection", func(t *testing.T) {
		t.Parallel()
		dir := indexedWikiWithXRefs(t, []WikiChunk{lintChunk("orphan", "Orphan")}, nil)

		report, err := LintWiki(context.Background(), dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.Orphans) == 0 {
			t.Error("expected orphan pages")
		}
	})

	t.Run("broken_links", func(t *testing.T) {
		t.Parallel()
		dir := indexedWikiWithXRefs(t, []WikiChunk{lintChunk("page", "Page")},
			map[string][]string{"page": {"nonexistent"}})

		report, err := LintWiki(context.Background(), dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.BrokenLinks) == 0 {
			t.Error("expected broken links")
		}
	})

	t.Run("stale_by_age", func(t *testing.T) {
		t.Parallel()
		old := lintChunk("stale", "Stale")
		old.Updated = "2020-01-01"
		dir := indexedWikiWithXRefs(t, []WikiChunk{old}, nil)

		report, err := LintWiki(context.Background(), dir, LintConfig{StaleDays: 30})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.StalePages) == 0 {
			t.Error("expected stale pages")
		}
	})

	t.Run("stale_since_beats_the_window", func(t *testing.T) {
		t.Parallel()
		flagged := lintChunk("flagged", "Flagged")
		flagged.StaleSince = time.Now().Format("2006-01-02")
		flagged.StaleReason = "source changed"
		dir := indexedWikiWithXRefs(t, []WikiChunk{flagged}, nil)

		report, err := LintWiki(context.Background(), dir, LintConfig{StaleDays: 3650})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.StalePages) != 1 {
			t.Errorf("StalePages = %v, want [flagged]", report.StalePages)
		}
	})

	t.Run("no_updated_is_not_stale", func(t *testing.T) {
		t.Parallel()
		undated := lintChunk("undated", "Undated")
		undated.Updated = ""
		dir := indexedWikiWithXRefs(t, []WikiChunk{undated}, nil)

		report, err := LintWiki(context.Background(), dir, LintConfig{StaleDays: 1})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.StalePages) != 0 {
			t.Errorf("StalePages = %v, want none", report.StalePages)
		}
	})

	t.Run("empty_pages", func(t *testing.T) {
		t.Parallel()
		thin := lintChunk("empty", "Empty")
		thin.Body = "short"
		thin.WordCount = 1
		dir := indexedWikiWithXRefs(t, []WikiChunk{thin}, nil)

		report, err := LintWiki(context.Background(), dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.EmptyPages) == 0 {
			t.Error("expected empty pages")
		}
	})

	t.Run("missing_required_type_is_an_error", func(t *testing.T) {
		t.Parallel()
		untyped := lintChunk("page", "Page")
		untyped.DocType = ""
		dir := indexedWikiWithXRefs(t, []WikiChunk{untyped}, nil)

		report, err := LintWiki(context.Background(), dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.MissingFields) == 0 {
			t.Fatal("expected `type` to be reported as missing")
		}
		if report.MissingFields[0].MissingFields[0] != "type" {
			t.Errorf("missing = %v, want [type]", report.MissingFields[0].MissingFields)
		}
	})

	t.Run("missing_recommended_fields_are_not_errors", func(t *testing.T) {
		t.Parallel()
		weak := lintChunk("page", "Page")
		weak.Title = ""
		weak.Summary = ""
		dir := indexedWikiWithXRefs(t, []WikiChunk{weak},
			map[string][]string{"page": {"page"}})

		report, err := LintWiki(context.Background(), dir, LintConfig{})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(report.WeakFields) != 1 || len(report.WeakFields[0].MissingFields) != 2 {
			t.Fatalf("WeakFields = %+v, want one page missing title and description", report.WeakFields)
		}
		if report.Errors != len(report.Orphans) {
			t.Errorf("Errors = %d, want only the %d orphan(s)", report.Errors, len(report.Orphans))
		}
	})

	t.Run("no_index_is_refused", func(t *testing.T) {
		t.Parallel()
		_, err := LintWiki(context.Background(), filepath.Join(t.TempDir(), "nonexistent"), LintConfig{})
		if err == nil {
			t.Error("expected an error for a wiki that holds no pages")
		}
	})
}

func TestParseFMInstant(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		raw  string
		ok   bool
	}{
		{"date", "2026-08-29", true},
		{"rfc3339", "2026-08-29T10:11:12Z", true},
		{"quoted", "\"2026-08-29\"", true},
		{"space_separated", "2026-08-29 10:11:12", true},
		{"garbage", "not-a-date", false},
		{"empty", "", false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			if _, ok := parseFMInstant(tt.raw); ok != tt.ok {
				t.Errorf("parseFMInstant(%q) ok = %v, want %v", tt.raw, ok, tt.ok)
			}
		})
	}
}

// The table is the authority for the fast path: a source hash newer than the stored row must force
// a sync.
func TestFastPathCheckTrustsOnlyTheStoredTable(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	const slug = "edited"
	const oldHash = "1111111111111111"
	const newHash = "2222222222222222"

	dir := indexedWikiWithXRefs(t, []WikiChunk{{
		Slug: slug, Title: "Edited", Body: "the previously indexed body", DocType: "document",
		ContentHash: oldHash, WordCount: 4, ClusterID: -1,
	}}, nil)

	entries := []DocHashEntry{{ContentHash: newHash, Slug: slug}}

	if FastPathCheck(ctx, dir, entries) {
		t.Fatal("the fast path skipped a sync for an edited document")
	}

	current := indexedWikiWithXRefs(t, []WikiChunk{{
		Slug: slug, Title: "Edited", Body: "the freshly read body", DocType: "document",
		ContentHash: newHash, WordCount: 4, ClusterID: -1,
	}}, nil)
	if !FastPathCheck(ctx, current, entries) {
		t.Error("the fast path refused a corpus the index already holds at the same hash")
	}
}

func TestFastPathCheckNoticesAdditionsAndDeletions(t *testing.T) {
	t.Parallel()
	ctx := context.Background()

	const hash = "3333333333333333"
	chunk := func(slug string) WikiChunk {
		return WikiChunk{
			Slug: slug, Title: slug, Body: "body of " + slug, DocType: "document",
			ContentHash: hash, WordCount: 3, ClusterID: -1,
		}
	}
	dir := indexedWikiWithXRefs(t, []WikiChunk{chunk("alpha"), chunk("beta")}, nil)
	entry := func(slug string) DocHashEntry {
		return DocHashEntry{ContentHash: hash, Slug: slug}
	}

	if !FastPathCheck(ctx, dir, []DocHashEntry{entry("alpha"), entry("beta")}) {
		t.Fatal("an unchanged corpus must take the fast path")
	}
	if FastPathCheck(ctx, dir, []DocHashEntry{entry("alpha"), entry("beta"), entry("gamma")}) {
		t.Error("a document the index has never seen must not take the fast path")
	}
	if FastPathCheck(ctx, dir, []DocHashEntry{entry("alpha")}) {
		t.Error("an indexed document that no entry claims — a deletion — must not take the fast path")
	}
}
