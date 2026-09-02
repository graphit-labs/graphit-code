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
//
// It applies to the WHOLE page, frontmatter included, exactly as it did when the page was a file —
// so `head` now shows the header. Asserting only that a late term is absent would pass for the
// wrong reason, so this pins where the first lines actually come from.
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
	if !strings.HasPrefix(got.Source, "---\n") {
		t.Errorf("head:4 must start at the page's first line, which is the frontmatter delimiter:\n%s", got.Source)
	}
	if !strings.Contains(got.Source, "type: specification") {
		t.Errorf("head:4 lost the header the page opens with:\n%s", got.Source)
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

// 🔒 THE MEMORY PROTOCOL'S CHAIN WALK, which is the instruction this restored.
//
// The memory skill tells an agent to read `previous` / `next` off a revision's page — literally
// `graphit_wiki_source(path: "<slug>", wiki: "memory", pattern: "previous", after: 1)`. That
// returned nothing from the moment page reads moved to the index, because a read became the `body`
// column and the chain lived in the header. Six columns were added so the facts survived; this is
// the test that the SURFACE survived too, so the documented call works rather than the
// documentation being reworded to describe the loss.
func TestReadPageCarriesTheRevisionChainTheMemoryProtocolReadsOffIt(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	archived := WikiChunk{
		Slug:        "Some_memory--r0001",
		Title:       "Some memory",
		Body:        "What: the earlier wording.\n",
		DocType:     "fact",
		Source:      "history/01ENTITY/0001.md",
		WordCount:   4,
		ClusterID:   -1,
		Created:     "2026-07-01T00:00:00Z",
		ContentHash: "abc123",
		EntityID:    "01ENTITY",
		RevisionID:  "0001",
		Superseded:  true,
		CurrentID:   "01ENTITY",
		Revision:    2,
		Previous:    "0000",
		Next:        "01ENTITY.md",
	}
	if err := RebuildDB(context.Background(), dir, []WikiChunk{archived}, nil, nil, nil); err != nil {
		t.Fatalf("building the index: %v", err)
	}

	whole, err := ReadPageAt(context.Background(), dir, "Some_memory--r0001", textslice.Request{})
	if err != nil {
		t.Fatalf("reading the page: %v", err)
	}

	// Every field the protocol names, under the name it names it by.
	for _, want := range []string{
		"type: fact",
		"id: 01ENTITY",
		"superseded: true",
		"current: 01ENTITY",
		"revision_id: \"0001\"",
		"revision: 2",
		"previous: \"0000\"",
		"next: 01ENTITY.md",
		"created: \"2026-07-01T00:00:00Z\"",
	} {
		if !strings.Contains(whole.Source, want) {
			t.Errorf("the page read is missing %q:\n%s", want, whole.Source)
		}
	}
	// And the body is still there — the header is added, not substituted.
	if !strings.Contains(whole.Source, "the earlier wording") {
		t.Errorf("the page read lost its body:\n%s", whole.Source)
	}

	// The exact call the skill prescribes.
	sliced, err := ReadPageAt(context.Background(), dir, "Some_memory--r0001",
		textslice.Request{Pattern: "previous", After: 1})
	if err != nil {
		t.Fatalf("pattern read: %v", err)
	}
	if len(sliced.Matches) == 0 {
		t.Fatalf("pattern \"previous\" found nothing — the documented chain walk is broken:\n%s", sliced.Source)
	}
	if !strings.Contains(sliced.Source, "0000") {
		t.Errorf("the pattern read did not surface the previous revision:\n%s", sliced.Source)
	}

	// A page with no chain carries no chain keys, so a knowledge page is not littered with empty
	// memory fields.
	plain := probeWiki(t)
	got, err := ReadPageAt(context.Background(), plain, "wollastonita", textslice.Request{})
	if err != nil {
		t.Fatalf("reading a chainless page: %v", err)
	}
	for _, absent := range []string{"superseded:", "previous:", "next:", "revision:", "current:"} {
		if strings.Contains(got.Source, absent) {
			t.Errorf("a page with no revision chain must not carry %q:\n%s", absent, got.Source)
		}
	}
}

// The header a read produces has to PARSE, for the same reason the exported one does: a title
// containing `: ` is what made 47 memories unreadable, and an agent reading a page back through
// FrontmatterField gets nothing from a block that does not parse.
func TestReadPageHeaderParsesEvenForAHostileTitle(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	if err := RebuildDB(context.Background(), dir, []WikiChunk{{
		Slug: "hostile", Title: "Storage: where every artifact lives", Body: "body\n",
		DocType: "decision", Summary: "> a folded scalar header", WordCount: 1, ClusterID: -1,
	}}, nil, nil, nil); err != nil {
		t.Fatalf("building the index: %v", err)
	}

	got, err := ReadPageAt(context.Background(), dir, "hostile", textslice.Request{})
	if err != nil {
		t.Fatalf("reading: %v", err)
	}
	title, ok := FrontmatterField(got.Source, "title")
	if !ok {
		t.Fatalf("the read page's frontmatter does not parse:\n%s", got.Source)
	}
	if title != "Storage: where every artifact lives" {
		t.Errorf("title = %q; the colon must survive the round trip", title)
	}
}
