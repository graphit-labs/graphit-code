package wiki

import (
	"context"
	"errors"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/textslice"
)

func probeWiki(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	body := `# Granada Module

Granada issues xenolito tokens.

## Micaxisto

The anfibolio path is the slow one.
`
	chunks := []WikiChunk{
		{Slug: "1._Granada_Module", Title: "Granada Module", Body: body,
			DocType: "specification", Confidence: 0.9, WordCount: 12},
		{Slug: "wollastonita", Title: "Wollastonita", Body: "# Wollastonita\n", WordCount: 1},
	}
	if err := RebuildDB(context.Background(), dir, chunks, nil, nil, nil); err != nil {
		t.Fatalf("building the probe index: %v", err)
	}
	return dir
}

// Wiki file names are generated from titles, so the slug an agent gets back from
// search rarely matches the file name's casing. Requiring an exact match would
// make the tool unusable with its own search results.
func TestReadPageResolvesSlugCaseInsensitivelyAndWithoutExtension(t *testing.T) {
	t.Parallel()
	dir := probeWiki(t)

	for _, page := range []string{
		"1._Granada_Module",
		"1._Granada_Module.md",
		"1._granada_module",
		"1._GRANADA_MODULE.MD",
	} {
		got, err := ReadPageAt(context.Background(), dir, page, textslice.Request{})
		if err != nil {
			t.Errorf("ReadPageAt(context.Background(), %q) failed: %v", page, err)
			continue
		}
		if got.Title != "Granada Module" {
			t.Errorf("ReadPageAt(context.Background(), %q) title = %q, want the first heading", page, got.Title)
		}
	}
}

// The slicing is the same as the code-source tool's, which is the point: an agent
// reading a wiki page should not have to pull the whole file to see part of it.
func TestReadPageSlicesLikeTheSourceTool(t *testing.T) {
	t.Parallel()
	dir := probeWiki(t)

	got, err := ReadPageAt(context.Background(), dir, "1._Granada_Module", textslice.Request{Head: 4})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if strings.Contains(got.Source, "anfibolio") {
		t.Errorf("head:4 returned more than the first four lines:\n%s", got.Source)
	}

	got, err = ReadPageAt(context.Background(), dir, "1._Granada_Module", textslice.Request{Pattern: "xenolito", Before: 1, After: 1})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(got.Matches) == 0 {
		t.Error("pattern search found nothing in a page that contains the term")
	}
	if !strings.Contains(got.Source, ">") {
		t.Errorf("matches must be marked, got:\n%s", got.Source)
	}
}

// A page reference is caller input, so it cannot be allowed to address anything
// outside the wiki directory.
func TestReadPageRefusesToEscapeTheWikiDirectory(t *testing.T) {
	t.Parallel()
	dir := probeWiki(t)

	for _, page := range []string{
		"../../../etc/passwd",
		"..",
		filepath.Join("..", "outside.md"),
	} {
		_, err := ReadPageAt(context.Background(), dir, page, textslice.Request{})
		if err == nil {
			t.Errorf("ReadPageAt(context.Background(), %q) must be refused", page)
			continue
		}
		if errors.Is(err, ErrPageNotFound) {
			t.Errorf("ReadPageAt(context.Background(), %q) was reported as merely missing; a refusal must keep its own reason", page)
		}
	}

	if _, err := ReadPageAt(context.Background(), dir, filepath.Join(dir, "1._Granada_Module.md"), textslice.Request{}); err == nil {
		t.Error("an absolute path must be refused — the page is relative to the wiki directory")
	}
}

// A mistyped slug and a refused path are different problems: only the first is
// helped by listing what exists, and ErrPageNotFound is how a caller tells them
// apart.
func TestReadPageReportsAMissingPageDistinctly(t *testing.T) {
	t.Parallel()
	dir := probeWiki(t)

	_, err := ReadPageAt(context.Background(), dir, "does-not-exist", textslice.Request{})
	if !errors.Is(err, ErrPageNotFound) {
		t.Errorf("a missing page must be ErrPageNotFound, got %v", err)
	}
}

func TestReadPageRejectsEmptyInput(t *testing.T) {
	t.Parallel()

	if _, err := ReadPageAt(context.Background(), "", "granada", textslice.Request{}); err == nil {
		t.Error("an unbuilt wiki must be reported rather than read")
	}
	if _, err := ReadPageAt(context.Background(), probeWiki(t), "  ", textslice.Request{}); err == nil {
		t.Error("a blank page reference must be rejected")
	}
}

func TestListPagesNamesWhatIsThere(t *testing.T) {
	t.Parallel()
	dir := probeWiki(t)

	pages := ListPagesAt(context.Background(), dir)
	if len(pages) != 2 {
		t.Fatalf("got %d pages, want 2: %v", len(pages), pages)
	}
	for _, p := range pages {
		if strings.HasSuffix(p, ".md") {
			t.Errorf("page name %q still carries its extension", p)
		}
	}
}

func TestFirstHeadingSkipsFrontmatter(t *testing.T) {
	t.Parallel()

	if got := firstHeading("---\ntitle: not this\n---\n\n# Granada\n"); got != "Granada" {
		t.Errorf("firstHeading = %q, want %q", got, "Granada")
	}
	if got := firstHeading("no heading here\n"); got != "" {
		t.Errorf("firstHeading = %q, want empty", got)
	}
}
