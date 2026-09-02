//go:build lancedb

package uiserver

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// The tests in this file pin one property: /api/wiki/modules reports the wikis the
// STORE resolves for a project, and never a directory inside the project.
//
// The regression they exist for shipped with the storage centralization. Every wiki
// moved to the global brand directory keyed by identity, and this handler kept probing
// `<project>/.graphit/knowledge/project`, `<project>/.graphit/memory/{project,user}` and
// `<global>/{knowledge,memory}` — all four of which stopped holding anything. The
// symptom was not an error: the endpoint answered 200 with an empty list, so the
// knowledge context page showed "no contexts available" and the memory entries
// disappeared from the sidebar while all three wikis sat intact on disk.
//
// The tests that used to cover discovery walked a tree they created inside the project,
// so they passed throughout. That is why these build their fixtures through the store
// resolvers instead, in a HOME this test owns.

// isolateHome points the global store at a directory this test owns, so nothing it
// resolves — or reports — can come from the developer's real ~/.graphit.
func isolateHome(t *testing.T) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	return home
}

const testProjectID = "01TESTPROJECT0000000000000"

// initProject writes the lockfile that gives a project its identity, which is what
// keys its stores.
func initProject(t *testing.T, projectDir, name string) {
	t.Helper()
	body := `{"project":{"id":"` + testProjectID + `","name":"` + name + `"},"artifacts":{}}`
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(body), 0o644); err != nil {
		t.Fatalf("write lockfile: %v", err)
	}
}

// writeWiki compiles a wiki holding the named pages.
//
// It used to drop one `.md` file per name into the directory, which is what the handler counted.
// The handler counts INDEXED pages, and reports HasLog from the `sync_log` table rather than from the
// existence of a `log.md` — so the fixture records one sync, which is what a compiled wiki always has.
func writeWiki(t *testing.T, dir string, pages ...string) {
	t.Helper()
	if dir == "" {
		t.Fatal("empty wiki directory — the store resolved nothing")
	}
	chunks := make([]wiki.WikiChunk, 0, len(pages))
	for _, p := range pages {
		slug := strings.TrimSuffix(p, ".md")
		chunks = append(chunks, wiki.WikiChunk{
			Slug: slug, Title: slug, Body: "# " + p, DocType: "document",
			WordCount: 2, ClusterID: -1,
		})
	}
	entry := &wiki.SyncLogEntry{TotalDocs: len(chunks), ArticlesWritten: len(chunks)}
	if err := wiki.SyncDB(context.Background(), dir, chunks, nil, entry); err != nil {
		t.Fatalf("compiling the wiki at %s: %v", dir, err)
	}
}

func moduleByID(modules []WikiModule, id string) (WikiModule, bool) {
	for _, m := range modules {
		if m.ID == id {
			return m, true
		}
	}
	return WikiModule{}, false
}

func TestProjectWikisResolveFromTheGlobalStore(t *testing.T) {
	home := isolateHome(t)
	project := t.TempDir()
	initProject(t, project, "acme")

	knowledgeDir := store.KnowledgeProjectDir(project)
	writeWiki(t, knowledgeDir, "index.md", "overview.md", "log.md")
	memProjectDir := store.MemoryWikiDir("project", testProjectID)
	writeWiki(t, memProjectDir, "index.md")

	modules := discoverModules(project)

	kn, ok := moduleByID(modules, "knowledge")
	if !ok {
		t.Fatalf("no knowledge module; got %+v", modules)
	}
	if kn.Path != knowledgeDir {
		t.Errorf("knowledge path = %q; want the store's %q", kn.Path, knowledgeDir)
	}
	if !strings.HasPrefix(kn.Path, home) {
		t.Errorf("knowledge path = %q; want it under the global store at %q", kn.Path, home)
	}
	if kn.Label != "acme" {
		t.Errorf("knowledge label = %q; want the lockfile project name", kn.Label)
	}
	if kn.Context != "project" {
		t.Errorf("knowledge context = %q; want %q — the UI keys its project styling on it", kn.Context, "project")
	}
	if kn.Pages != 3 {
		t.Errorf("knowledge pages = %d; want 3", kn.Pages)
	}
	if !kn.HasLog {
		t.Error("knowledge hasLog = false; the fixture recorded a sync")
	}

	mem, ok := moduleByID(modules, "memory-project")
	if !ok {
		t.Fatalf("no memory-project module; got %+v", modules)
	}
	if mem.Path != memProjectDir {
		t.Errorf("memory-project path = %q; want %q", mem.Path, memProjectDir)
	}
	// The sidebar builds the Memory section from ids prefixed "memory-", and routes to
	// /memory/explorer/<context>. Both halves have to keep holding.
	if !strings.HasPrefix(mem.ID, "memory-") || mem.Context != "project" {
		t.Errorf("memory-project module = %+v; want id memory-* and context %q", mem, "project")
	}
}

// A wiki left at the pre-centralization location must not be reported: reporting it
// would resurrect the split that made a project answer from a stale replica.
func TestALegacyProjectLocalWikiIsNotReported(t *testing.T) {
	isolateHome(t)
	project := t.TempDir()
	initProject(t, project, "acme")

	legacy := filepath.Join(project, brand.DotDir(), "knowledge", "project")
	writeWiki(t, legacy, "index.md", "stale.md")

	for _, m := range discoverModules(project) {
		if strings.HasPrefix(m.Path, project) {
			t.Errorf("module %q resolved inside the project at %q; every wiki is global now", m.ID, m.Path)
		}
	}
}

func TestTheUserMemoryWikiIsListedForAnyProject(t *testing.T) {
	isolateHome(t)
	project := t.TempDir()
	// Deliberately NOT initialised: user memory is the user's, and is readable from a
	// project that has no identity of its own.

	userDir := memory.WikiDirFor(project, string(memory.MemoryScopeUser))
	if userDir == "" {
		t.Skip("no git user.email configured, so the user scope has no id to key on")
	}
	writeWiki(t, userDir, "index.md")

	mem, ok := moduleByID(discoverModules(project), "memory-user")
	if !ok {
		t.Fatal("no memory-user module for a project with a user memory wiki")
	}
	if mem.Context != "user" {
		t.Errorf("memory-user context = %q; want %q — the sidebar labels the entry from it", mem.Context, "user")
	}
	if mem.Path != userDir {
		t.Errorf("memory-user path = %q; want %q", mem.Path, userDir)
	}
}

// Membership is a per-project record. A context this project never claimed exists in
// the same global root as one it did, so a directory listing would report both.
func TestKnowledgeContextsComeFromTheProjectsClaims(t *testing.T) {
	isolateHome(t)
	project := t.TempDir()
	initProject(t, project, "acme")

	writeWiki(t, store.KnowledgeContextDir("claimed"), "index.md", "api.md")
	writeWiki(t, store.KnowledgeContextDir("somebody-elses"), "index.md")
	if err := store.AddContext(project, store.KindKnowledge,
		store.ContextRecord{Name: "claimed"}); err != nil {
		t.Fatal(err)
	}

	modules := discoverModules(project)

	claimed, ok := moduleByID(modules, "knowledge/claimed")
	if !ok {
		t.Fatalf("the claimed context is missing; got %+v", modules)
	}
	if claimed.Context != "claimed" {
		t.Errorf("context = %q; want %q — the explorer route is built from it", claimed.Context, "claimed")
	}
	if claimed.Pages != 2 {
		t.Errorf("pages = %d; want 2", claimed.Pages)
	}
	if _, found := moduleByID(modules, "knowledge/somebody-elses"); found {
		t.Error("an unclaimed context was reported; membership is the lockfile, not the directory")
	}
}

// A project whose stores were never compiled reports nothing, rather than reporting
// modules whose pages cannot be opened.
func TestAProjectWithNoCompiledWikiReportsNoModules(t *testing.T) {
	isolateHome(t)
	project := t.TempDir()
	initProject(t, project, "acme")

	if modules := discoverModules(project); len(modules) != 0 {
		t.Errorf("got %d modules for a project with nothing compiled: %+v", len(modules), modules)
	}
}

func TestHandleModulesServesTheResolvedWikisSorted(t *testing.T) {
	isolateHome(t)
	project := t.TempDir()
	initProject(t, project, "acme")
	writeWiki(t, store.KnowledgeProjectDir(project), "index.md")
	writeWiki(t, store.MemoryWikiDir("project", testProjectID), "index.md")
	writeWiki(t, store.KnowledgeContextDir("vendor-api"), "index.md")
	if err := store.AddContext(project, store.KindKnowledge,
		store.ContextRecord{Name: "vendor-api"}); err != nil {
		t.Fatal(err)
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/modules", corsJSON(h.handleModules))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/modules?project_dir="+project, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d", w.Code, http.StatusOK)
	}
	var modules []WikiModule
	if err := json.NewDecoder(w.Body).Decode(&modules); err != nil {
		t.Fatalf("decode: %v", err)
	}

	var ids []string
	for _, m := range modules {
		ids = append(ids, m.ID)
	}
	for _, want := range []string{"knowledge", "knowledge/vendor-api", "memory-project"} {
		if _, ok := moduleByID(modules, want); !ok {
			t.Errorf("module %q missing from %v", want, ids)
		}
	}
	for i := 1; i < len(modules); i++ {
		if modules[i-1].ID > modules[i].ID {
			t.Errorf("modules not sorted by id: %v", ids)
			break
		}
	}
}

// The pages of a resolved module must be readable through the endpoint the UI calls
// next, or the list is decoration: a module whose Path the pages handler rejects looks
// present and browses empty.
func TestPagesOfADiscoveredModuleAreServedFromItsPath(t *testing.T) {
	isolateHome(t)
	project := t.TempDir()
	initProject(t, project, "acme")
	writeWiki(t, store.KnowledgeProjectDir(project), "index.md", "billing.md")

	kn, ok := moduleByID(discoverModules(project), "knowledge")
	if !ok {
		t.Fatal("no knowledge module")
	}

	h := &WikiHandler{}
	mux := http.NewServeMux()
	mux.HandleFunc("/api/wiki/pages", corsJSON(h.handlePages))

	req := httptest.NewRequest(http.MethodGet, "/api/wiki/pages?dir="+kn.Path, nil)
	w := httptest.NewRecorder()
	mux.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("status = %d; want %d — body %q", w.Code, http.StatusOK, w.Body.String())
	}
	var pages []WikiPageMeta
	if err := json.NewDecoder(w.Body).Decode(&pages); err != nil {
		t.Fatalf("decode: %v", err)
	}
	if len(pages) != 2 {
		t.Errorf("got %d pages; want 2", len(pages))
	}
}
