//go:build lancedb

package wiki

import (
	"context"
	"path/filepath"
	"strings"
	"testing"
)

// fts.go is 1494 lines holding the wiki's entire storage and retrieval layer, and
// until this file nothing opened a WikiDB at all. The tests named after search
// cover the AI-driven loop and pure helpers — trigrams, snippets, query strings —
// none of which touch SQLite.
//
// The sharpest consequence is the build tag. The chunk index is an FTS5 virtual
// table, so `go build` without -tags lancedb produces a binary whose wiki fails the
// moment it opens the database. The suite would have stayed green through that.
//
// So these tests fail loudly rather than skipping when the engine is unavailable. A skip would
// restore exactly the blind spot they exist to close — and the shape of that blind spot survived
// the move off SQLite: a binary built without the `lancedb` tag opens nothing either, it just
// says so in a different sentence.

func newTestWikiDB(t *testing.T) *WikiDB {
	t.Helper()
	db, err := OpenWikiDB(context.Background(), t.TempDir())
	if err != nil {
		if strings.Contains(err.Error(), "lancedb tag") {
			t.Fatalf("the LanceDB engine is not available in this binary — it was almost "+
				"certainly built without -tags lancedb, which leaves the wiki unable to open "+
				"its index at runtime: %v", err)
		}
		t.Fatalf("OpenWikiDB: %v", err)
	}
	t.Cleanup(func() { _ = db.Close() })
	return db
}

func testChunks() []WikiChunk {
	return []WikiChunk{
		{
			Slug: "autenticacao", Title: "Autenticação de usuários",
			Body: "O fluxo de login valida credenciais contra o provedor externo " +
				"e emite um token de sessão de curta duração.",
			Summary: "Como o login funciona", DocType: "guide", Source: "docs/auth.md",
			Breadcrumb: "Docs > Auth", ClusterID: 1, ClusterName: "Segurança",
			Confidence: 0.9, ContentHash: "h-auth", WordCount: 20,
			Updated: "2026-07-01", Important: true,
		},
		{
			Slug: "indexacao", Title: "Indexação incremental",
			Body: "O indexador reprocessa apenas os arquivos que mudaram, " +
				"usando o cache de parse para pular o resto.",
			Summary: "Como o índice é atualizado", DocType: "guide", Source: "docs/index.md",
			Breadcrumb: "Docs > Index", ClusterID: 2, ClusterName: "Pipeline",
			Confidence: 0.8, ContentHash: "h-idx", WordCount: 18,
			Updated: "2026-07-02",
		},
		{
			Slug: "implantacao", Title: "Implantação em produção",
			Body: "A implantação publica o binário e reinicia o daemon sem perder " +
				"o índice já construído.",
			Summary: "Passos de deploy", DocType: "runbook", Source: "docs/deploy.md",
			Breadcrumb: "Docs > Ops", ClusterID: 2, ClusterName: "Pipeline",
			Confidence: 0.7, ContentHash: "h-dep", WordCount: 16,
			Updated: "2026-07-03",
		},
	}
}

func rebuiltTestDB(t *testing.T) *WikiDB {
	t.Helper()
	db := newTestWikiDB(t)
	xrefs := map[string][]string{
		"autenticacao": {"indexacao"},
		"indexacao":    {"implantacao"},
	}
	if err := db.Rebuild(context.Background(), testChunks(), xrefs, nil, nil); err != nil {
		t.Fatalf("Rebuild: %v", err)
	}
	return db
}

// Opening the index creates it. This is the test that notices a binary built without the tag.
func TestWikiDBOpensAndCreatesFTSIndex(t *testing.T) {
	db := newTestWikiDB(t)

	if filepath.Base(db.DBPath()) != WikiIndexDirName {
		t.Errorf("index is at %q, want a directory named %q",
			filepath.Base(db.DBPath()), WikiIndexDirName)
	}

	// Search on an empty index must return nothing and no error. An engine that is not linked in
	// fails here instead.
	res, err := db.Search(context.Background(), "qualquer coisa", 5)
	if err != nil {
		t.Fatalf("Search on an empty index: %v", err)
	}
	if len(res) != 0 {
		t.Errorf("empty index returned %d results", len(res))
	}
}

// Reopening must not recreate or corrupt an existing index.
func TestWikiDBReopenKeepsContent(t *testing.T) {
	dir := t.TempDir()

	db, err := OpenWikiDB(context.Background(), dir)
	if err != nil {
		t.Fatalf("OpenWikiDB: %v", err)
	}
	if err := db.Rebuild(context.Background(), testChunks(), nil, nil, nil); err != nil {
		t.Fatalf("Rebuild: %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}

	reopened, err := OpenWikiDB(context.Background(), dir)
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	defer func() { _ = reopened.Close() }()

	chunks, slugs, _, _, err := reopened.Stats(context.Background())
	if err != nil {
		t.Fatalf("Stats: %v", err)
	}
	if chunks != len(testChunks()) {
		t.Errorf("after reopen: %d chunks, want %d", chunks, len(testChunks()))
	}
	if slugs != len(testChunks()) {
		t.Errorf("after reopen: %d slugs, want %d", slugs, len(testChunks()))
	}

	res, err := reopened.Search(context.Background(), "login", 5)
	if err != nil {
		t.Fatalf("Search after reopen: %v", err)
	}
	if len(res) == 0 {
		t.Error("content written before the reopen is no longer searchable")
	}
}

// The round trip that matters: written chunks come back, by content and by title,
// with their metadata and cross-references attached.
func TestWikiDBSearchRoundTrip(t *testing.T) {
	db := rebuiltTestDB(t)

	cases := []struct {
		name, query, wantSlug string
	}{
		{"word from the body", "credenciais", "autenticacao"},
		{"word from the title", "Indexação", "indexacao"},
		{"word from the summary", "deploy", "implantacao"},
		{"multiple words", "reprocessa arquivos", "indexacao"},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			res, err := db.Search(context.Background(), tc.query, 5)
			if err != nil {
				t.Fatalf("Search(%q): %v", tc.query, err)
			}
			if len(res) == 0 {
				t.Fatalf("Search(%q) returned nothing", tc.query)
			}
			found := false
			for _, r := range res {
				if r.Slug == tc.wantSlug {
					found = true
					if r.Title == "" {
						t.Error("result carries no title")
					}
					if r.Score == 0 {
						t.Error("result carries no score, so ranking is meaningless")
					}
				}
			}
			if !found {
				t.Errorf("Search(%q) did not return %q; got %v",
					tc.query, tc.wantSlug, slugsOf(res))
			}
		})
	}
}

// Accented text must survive the round trip. The corpus this indexes is written
// in Portuguese, so a tokenizer that mangles diacritics would be silent damage.
func TestWikiDBSearchHandlesAccents(t *testing.T) {
	db := rebuiltTestDB(t)

	for _, q := range []string{"Autenticação", "autenticacao", "Implantação"} {
		res, err := db.Search(context.Background(), q, 5)
		if err != nil {
			t.Fatalf("Search(%q): %v", q, err)
		}
		if len(res) == 0 {
			t.Errorf("Search(%q) returned nothing — accent handling is lossy", q)
		}
	}
}

// A rebuild replaces the index rather than appending to it: content that is gone
// from the input must be gone from the index.
func TestWikiDBRebuildDropsRemovedChunks(t *testing.T) {
	db := rebuiltTestDB(t)

	if res, _ := db.Search(context.Background(), "credenciais", 5); len(res) == 0 {
		t.Fatal("precondition: the first chunk should be searchable")
	}

	kept := testChunks()[1:]
	if err := db.Rebuild(context.Background(), kept, nil, nil, nil); err != nil {
		t.Fatalf("second Rebuild: %v", err)
	}

	res, err := db.Search(context.Background(), "credenciais", 5)
	if err != nil {
		t.Fatalf("Search: %v", err)
	}
	for _, r := range res {
		if r.Slug == "autenticacao" {
			t.Error("a chunk dropped from the input is still in the index — a rebuild " +
				"is appending instead of replacing")
		}
	}

	chunks, _, _, _, err := db.Stats(context.Background())
	if err != nil {
		t.Fatalf("Stats: %v", err)
	}
	if chunks != len(kept) {
		t.Errorf("%d chunks after the rebuild, want %d", chunks, len(kept))
	}
}

// CheckAllHashesMatch is the fast path that decides whether a rebuild is needed
// at all, so a wrong answer either wastes a full rebuild or serves stale pages.
func TestWikiDBCheckAllHashesMatch(t *testing.T) {
	db := rebuiltTestDB(t)

	if !db.CheckAllHashesMatch(context.Background(), testChunks()) {
		t.Error("identical chunks reported as changed — every sync would rebuild")
	}

	changed := testChunks()
	changed[0].ContentHash = "h-auth-modificado"
	if db.CheckAllHashesMatch(context.Background(), changed) {
		t.Error("a changed hash reported as matching — the wiki would serve stale content")
	}

	if db.CheckAllHashesMatch(context.Background(), testChunks()[1:]) {
		t.Error("a removed chunk reported as matching")
	}
}

func TestWikiDBCrossReferences(t *testing.T) {
	db := rebuiltTestDB(t)

	direct, err := db.FindXRefs(context.Background(), "autenticacao", 1)
	if err != nil {
		t.Fatalf("FindXRefs: %v", err)
	}
	if len(direct) == 0 {
		t.Fatal("no cross-reference from autenticacao, one was written")
	}

	deep, err := db.FindXRefs(context.Background(), "autenticacao", 2)
	if err != nil {
		t.Fatalf("FindXRefs depth 2: %v", err)
	}
	if len(deep) < len(direct) {
		t.Errorf("depth 2 returned fewer results (%d) than depth 1 (%d)", len(deep), len(direct))
	}
}

func TestWikiDBBrowseFilters(t *testing.T) {
	db := rebuiltTestDB(t)

	all, err := db.Browse(context.Background(), BrowseFilter{ClusterID: -1, Limit: 50})
	if err != nil {
		t.Fatalf("Browse: %v", err)
	}
	if len(all) != len(testChunks()) {
		t.Fatalf("unfiltered Browse returned %d, want %d", len(all), len(testChunks()))
	}

	runbooks, err := db.Browse(context.Background(), BrowseFilter{DocType: "runbook", ClusterID: -1, Limit: 50})
	if err != nil {
		t.Fatalf("Browse by doc type: %v", err)
	}
	if len(runbooks) != 1 || runbooks[0].Slug != "implantacao" {
		t.Errorf("doc type filter returned %v, want just implantacao", browseSlugs(runbooks))
	}

	cluster2, err := db.Browse(context.Background(), BrowseFilter{ClusterID: 2, Limit: 50})
	if err != nil {
		t.Fatalf("Browse by cluster: %v", err)
	}
	if len(cluster2) != 2 {
		t.Errorf("cluster 2 returned %d entries, want 2", len(cluster2))
	}

	yes := true
	important, err := db.Browse(context.Background(), BrowseFilter{Important: &yes, ClusterID: -1, Limit: 50})
	if err != nil {
		t.Fatalf("Browse important: %v", err)
	}
	if len(important) != 1 || important[0].Slug != "autenticacao" {
		t.Errorf("important filter returned %v, want just autenticacao", browseSlugs(important))
	}
}

func TestWikiDBSyncLogRoundTrip(t *testing.T) {
	db := rebuiltTestDB(t)

	entry := SyncLogEntry{
		Timestamp: "2026-07-27T10:00:00Z", TotalDocs: 3, ArticlesWritten: 3,
		BacklinksAdded: 2,
		Added:          []string{"autenticacao"},
		Updated:        []string{"indexacao"},
		Deleted:        []string{"obsoleto"},
	}
	if err := db.AppendSyncLog(context.Background(), entry); err != nil {
		t.Fatalf("AppendSyncLog: %v", err)
	}

	got, err := db.QuerySyncLog(context.Background(), 10)
	if err != nil {
		t.Fatalf("QuerySyncLog: %v", err)
	}
	if len(got) == 0 {
		t.Fatal("the entry that was just appended is not in the log")
	}
	last := got[0]
	if last.TotalDocs != entry.TotalDocs || last.ArticlesWritten != entry.ArticlesWritten {
		t.Errorf("counters came back as %+v, want TotalDocs=%d ArticlesWritten=%d",
			last, entry.TotalDocs, entry.ArticlesWritten)
	}
	if len(last.Added) != 1 || last.Added[0] != "autenticacao" {
		t.Errorf("Added came back as %v, want [autenticacao]", last.Added)
	}
	if len(last.Deleted) != 1 || last.Deleted[0] != "obsoleto" {
		t.Errorf("Deleted came back as %v, want [obsoleto]", last.Deleted)
	}
}

// A query made only of FTS5 syntax must not reach the engine as syntax. Users
// paste anything into search, and an unescaped quote or operator is the classic
// way an FTS-backed search turns a typo into an error.
func TestWikiDBSearchSurvivesHostileQueries(t *testing.T) {
	db := rebuiltTestDB(t)

	for _, q := range []string{
		`"`, `""`, `AND`, `OR`, `NOT`, `*`, `^`, `(`, `()`, `a AND`, `- -`,
		`login OR`, `"unbalanced`, `NEAR(`, `token*`, `  `,
	} {
		if _, err := db.Search(context.Background(), q, 5); err != nil {
			t.Errorf("Search(%q) returned an error instead of no results: %v", q, err)
		}
	}
}

func slugsOf(rs []WikiSearchResult) []string {
	out := make([]string, 0, len(rs))
	for _, r := range rs {
		out = append(out, r.Slug)
	}
	return out
}

func browseSlugs(es []BrowseEntry) []string {
	out := make([]string, 0, len(es))
	for _, e := range es {
		out = append(out, e.Slug)
	}
	return out
}
