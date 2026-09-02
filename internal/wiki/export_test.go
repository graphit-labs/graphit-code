//go:build lancedb

package wiki

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"gopkg.in/yaml.v3"
)

// The export is where markdown comes from now, so it is where the page renderer's guarantees are
// asserted. Two of them used to live in internal/knowledge and internal/memory, against a renderer
// that ran on every build; the guarantees are the same and the subject moved.

func exportChunk(slug, title string) WikiChunk {
	return WikiChunk{
		Slug:      slug,
		Title:     title,
		Body:      "Body of " + title + ".",
		Summary:   "Summary of " + title + ".",
		DocType:   "document",
		Source:    "docs/" + slug + ".md",
		Updated:   "2026-08-29",
		WordCount: 4,
		ClusterID: -1,
	}
}

func exportTo(t *testing.T, chunks []WikiChunk, xrefs map[string][]string, moduleTag string) (string, *ExportResult) {
	t.Helper()
	wikiDir := t.TempDir()
	if err := SyncDB(context.Background(), wikiDir, chunks, xrefs, nil); err != nil {
		t.Fatalf("building the index: %v", err)
	}
	out := filepath.Join(t.TempDir(), "md")
	result, err := ExportMarkdown(context.Background(), wikiDir, out, moduleTag)
	if err != nil {
		t.Fatalf("ExportMarkdown: %v", err)
	}
	return out, result
}

func readExported(t *testing.T, dir, name string) string {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(dir, name))
	if err != nil {
		t.Fatalf("reading %s: %v", name, err)
	}
	return string(data)
}

func TestExportMarkdownWritesAPagePerChunkPlusTheIndex(t *testing.T) {
	t.Parallel()
	out, result := exportTo(t,
		[]WikiChunk{exportChunk("alpha", "Alpha"), exportChunk("beta", "Beta")},
		map[string][]string{"alpha": {"beta"}}, "knowledge")

	if result.Pages != 2 {
		t.Errorf("Pages = %d, want 2", result.Pages)
	}
	for _, name := range []string{"alpha.md", "beta.md", "index.md"} {
		if _, err := os.Stat(filepath.Join(out, name)); err != nil {
			t.Errorf("%s was not written: %v", name, err)
		}
	}

	alpha := readExported(t, out, "alpha.md")
	if !strings.Contains(alpha, "# Alpha") {
		t.Error("the page has no H1")
	}
	if !strings.Contains(alpha, "Body of Alpha.") {
		t.Error("the page lost its body")
	}
	if !strings.Contains(alpha, "## Cross-References") || !strings.Contains(alpha, "[beta](beta.md)") {
		t.Error("the outbound edge did not reach the page")
	}
	// Backlinks are rendered from the graph rather than injected into a stored body: beta is
	// referenced by alpha, so beta's exported page carries the section.
	beta := readExported(t, out, "beta.md")
	if !strings.Contains(beta, backlinksHeader) || !strings.Contains(beta, "(alpha.md)") {
		t.Errorf("beta has no backlink to alpha:\n%s", beta)
	}

	index := readExported(t, out, "index.md")
	if !strings.Contains(index, "**2 pages**") {
		t.Error("the index does not count the pages")
	}
	if !strings.Contains(index, "[Alpha](alpha.md)") {
		t.Error("the index does not link the pages")
	}
}

// A title containing `: ` is the case that cost 20 files: assembled with Fprintf it produces a
// frontmatter block that does not parse, and a re-render from the failed parse wipes the
// classification. The renderer marshals, so this is structurally impossible — and that is what this
// asserts, by parsing the block back.
func TestExportedFrontmatterAlwaysParses(t *testing.T) {
	t.Parallel()
	hostile := exportChunk("hostile", "Storage: where every artifact lives")
	hostile.Summary = "> a folded scalar header, which breaks a hand-built block"
	hostile.StaleSince = "2026-01-02"
	hostile.StaleReason = "the source changed: twice"

	out, _ := exportTo(t, []WikiChunk{hostile}, nil, "knowledge")
	page := readExported(t, out, "hostile.md")

	block, ok := FrontmatterBlock(page)
	if !ok {
		t.Fatalf("no frontmatter block:\n%s", page)
	}
	var doc map[string]any
	if err := yaml.Unmarshal([]byte(block), &doc); err != nil {
		t.Fatalf("the frontmatter does not parse: %v\n%s", err, block)
	}
	if doc["title"] != "Storage: where every artifact lives" {
		t.Errorf("title = %v; the colon must survive the round trip", doc["title"])
	}
	if doc["description"] != hostile.Summary {
		t.Errorf("description = %v", doc["description"])
	}
	if doc["stale_reason"] != "the source changed: twice" {
		t.Errorf("stale_reason = %v", doc["stale_reason"])
	}
	// §5.5: an absolute instant, derived from the moment the page became stale.
	if doc["stale_after"] != "2026-01-02" {
		t.Errorf("stale_after = %v, want the stale_since instant", doc["stale_after"])
	}
	if _, ok := doc["type"]; !ok {
		t.Error("§11.2: `type` is the one required field")
	}
	gen, ok := doc["generated"].(map[string]any)
	if !ok {
		t.Fatalf("§5.2: `generated` must be a mapping, got %T", doc["generated"])
	}
	if by, _ := gen["by"].(string); !strings.HasPrefix(by, "process:") {
		t.Errorf("generated.by = %q; §5.3 derives the trust tier from the actor prefix", by)
	}
}

// The revision chain has to survive the export, under the names the memory protocol documents. It is
// the reason `revision`, `previous` and `next` became columns: they were page frontmatter, and a page
// read out of the index returns the body, so the chain had nowhere left to live.
func TestExportedMemoryPageCarriesTheRevisionChain(t *testing.T) {
	t.Parallel()
	archived := exportChunk("Some_memory--r0001", "Some memory")
	archived.DocType = "fact"
	archived.EntityID = "01ENTITY"
	archived.RevisionID = "0001"
	archived.Superseded = true
	archived.CurrentID = "01ENTITY"
	archived.Revision = 2
	archived.Previous = "0000"
	archived.Next = "01ENTITY.md"
	archived.Created = "2026-07-01T00:00:00Z"

	out, _ := exportTo(t, []WikiChunk{archived}, nil, "memory")
	page := readExported(t, out, "Some_memory--r0001.md")

	block, ok := FrontmatterBlock(page)
	if !ok {
		t.Fatalf("no frontmatter block:\n%s", page)
	}
	var doc map[string]any
	if err := yaml.Unmarshal([]byte(block), &doc); err != nil {
		t.Fatalf("the frontmatter does not parse: %v\n%s", err, block)
	}
	for key, want := range map[string]any{
		"id":          "01ENTITY",
		"superseded":  true,
		"current":     "01ENTITY",
		"revision_id": "0001",
		"revision":    2,
		"previous":    "0000",
		"next":        "01ENTITY.md",
		"created":     "2026-07-01T00:00:00Z",
	} {
		if doc[key] != want {
			t.Errorf("%s = %v (%T), want %v", key, doc[key], doc[key], want)
		}
	}
	// The banner is the presentation half of the same columns, and it is the part a person reads.
	if !strings.Contains(page, "Superseded revision") {
		t.Error("an archived revision must say so in the body")
	}
	if !strings.Contains(page, "revision 2 of `01ENTITY`") {
		t.Errorf("the banner does not name the revision:\n%s", page)
	}
	if !strings.Contains(page, "replaced by `01ENTITY.md`") {
		t.Error("the banner does not name what replaced it")
	}
}

func TestExportMarkdownWritesTheLogFromTheSyncHistory(t *testing.T) {
	t.Parallel()
	wikiDir := t.TempDir()
	entry := &SyncLogEntry{
		Timestamp:       "2026-08-29T10:00:00Z",
		TotalDocs:       1,
		ArticlesWritten: 1,
		Added:           []string{"alpha"},
		Details:         map[string]LogDocDetails{"alpha": {Title: "Alpha", Summary: "The first page."}},
	}
	if err := SyncDB(context.Background(), wikiDir, []WikiChunk{exportChunk("alpha", "Alpha")}, nil, entry); err != nil {
		t.Fatal(err)
	}

	out := filepath.Join(t.TempDir(), "md")
	result, err := ExportMarkdown(context.Background(), wikiDir, out, "knowledge")
	if err != nil {
		t.Fatal(err)
	}
	if !result.HasLog {
		t.Fatal("a wiki with a sync history must export a log")
	}

	log := readExported(t, out, "log.md")
	// §9: date-grouped headings, prose entries led by a bold kind word, and no frontmatter — §11
	// exempts the reserved filenames, and a frontmatter block here would make log.md the one file in
	// the bundle claiming to be a concept.
	if strings.HasPrefix(strings.TrimSpace(log), "---") {
		t.Error("log.md must not carry frontmatter")
	}
	if !strings.Contains(log, "## 2026-08-29") {
		t.Errorf("the log is not grouped by date:\n%s", log)
	}
	if !strings.Contains(log, "* **Creation**: Added [Alpha](alpha.md)") {
		t.Errorf("the log does not record the added page:\n%s", log)
	}
	if !strings.Contains(log, "The first page.") {
		t.Error("the log lost the page's summary")
	}
}

func TestExportMarkdownRefusesAnEmptyWiki(t *testing.T) {
	t.Parallel()
	if _, err := ExportMarkdown(context.Background(), t.TempDir(), filepath.Join(t.TempDir(), "md"), "knowledge"); err == nil {
		t.Error("exporting a wiki with no pages must be an error, not an empty directory")
	}
}

func TestExportMarkdownRequiresAnOutputDirectory(t *testing.T) {
	t.Parallel()
	dir := indexedWiki(t, []WikiChunk{exportChunk("alpha", "Alpha")})
	if _, err := ExportMarkdown(context.Background(), dir, "", "knowledge"); err == nil {
		t.Error("an empty output directory must be refused")
	}
}
