package memory

import (
	"context"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// newLocalService wires a memory service to a local-only store, which is all the history chain
// needs: the chain is written into the raw directory, and Publish uploads it as ordinary files.
func newLocalService(t *testing.T) (*MemoryService, *ScopeStore) {
	t.Helper()
	t.Setenv("GRAPHIT_HUB_BUCKET", "")

	st := &MemoryStore{rawBase: filepath.Join(t.TempDir(), "raw")}
	svc := &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "proj-1",
		localDir: t.TempDir(),
		store:    st,
	}
	w, err := st.OpenScopeLocal(svc.ScopePrefix())
	if err != nil {
		t.Fatalf("open scope: %v", err)
	}
	return svc, w
}

// A memory is born at revision 1, with the unit that wrote it and no previous version.
func TestMemoryStartsAtRevisionOneWithNoPrevious(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("First", "the body", MemoryOpts{})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}

	data, err := w.ReadFile(MemoryFileName(id))
	if err != nil {
		t.Fatalf("reading the memory: %v", err)
	}
	fm := ParseMemoryFrontmatter(string(data))

	if fm.Revision != 1 {
		t.Errorf("revision = %d, want 1", fm.Revision)
	}
	if fm.Previous != "" {
		t.Errorf("previous = %q, want empty on a first revision", fm.Previous)
	}
	if fm.UpdatedBy == "" {
		t.Error("updated_by is empty — the unit identity did not reach the frontmatter")
	}
}

// THE ASK: the history git carried lives in the frontmatter, and points at the previous version's
// path. Following `previous` must land on a file holding exactly what the memory said before.
func TestUpdateArchivesThePreviousVersionAndPointsAtIt(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("First title", "first body", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}
	before, err := w.ReadFile(MemoryFileName(id))
	if err != nil {
		t.Fatal(err)
	}

	if err := svc.UpdateMemory(id, "Second title", "second body"); err != nil {
		t.Fatalf("UpdateMemory: %v", err)
	}

	after, err := w.ReadFile(MemoryFileName(id))
	if err != nil {
		t.Fatal(err)
	}
	fm := ParseMemoryFrontmatter(string(after))

	if fm.Revision != 2 {
		t.Errorf("revision = %d, want 2", fm.Revision)
	}
	if fm.Title != "Second title" {
		t.Errorf("title = %q, want the new one", fm.Title)
	}
	if want := onlyArchivePath(t, w, id); fm.Previous != want {
		t.Fatalf("previous = %q, want %q", fm.Previous, want)
	}
	if fm.Next != "" {
		t.Errorf("next = %q on the live memory, want empty — the head of a chain has no successor", fm.Next)
	}
	if fm.RevisionID != "" {
		t.Errorf("revision_id = %q on the live memory, want empty", fm.RevisionID)
	}

	// The pointer has to resolve, and the archive has to hold what the memory said before.
	archived, err := w.ReadFile(fm.Previous)
	if err != nil {
		t.Fatalf("previous points at %q which cannot be read: %v", fm.Previous, err)
	}
	if !sameMemoryBody(string(archived), string(before)) {
		t.Errorf("the archived revision is not what the memory said before:\n--- archived\n%s\n--- before\n%s",
			archived, before)
	}

	// The other half of the chain: the archive names what replaced it, and itself.
	afm := ParseMemoryFrontmatter(string(archived))
	if afm.Next != MemoryFileName(id) {
		t.Errorf("archive next = %q, want %q", afm.Next, MemoryFileName(id))
	}
	if afm.RevisionID != RevisionIDFromHistoryPath(fm.Previous) {
		t.Errorf("archive revision_id = %q, want %q", afm.RevisionID, RevisionIDFromHistoryPath(fm.Previous))
	}
	if afm.ID != id {
		t.Errorf("archive id = %q, want the chain id %q — an old revision must name its current memory", afm.ID, id)
	}
	if !afm.IsArchivedRevision() {
		t.Error("the archive does not report itself as a superseded revision")
	}
	if ParseMemoryFrontmatter(string(after)).IsArchivedRevision() {
		t.Error("the live memory reports itself as a superseded revision")
	}
}

// onlyArchivePath returns the single archived revision of a chain, failing when there is not
// exactly one. Archive names are ULIDs, so a test cannot spell one — it asks the store.
func onlyArchivePath(t *testing.T, w *ScopeStore, id string) string {
	t.Helper()
	entries, err := w.ListDir(HistoryDirFor(id))
	if err != nil {
		t.Fatalf("listing the history of %s: %v", id, err)
	}
	var found []string
	for _, e := range entries {
		if !e.IsDir() && filepath.Ext(e.Name()) == ".md" {
			found = append(found, HistoryDirFor(id)+"/"+e.Name())
		}
	}
	if len(found) != 1 {
		t.Fatalf("history of %s holds %d revisions, want 1: %v", id, len(found), found)
	}
	return found[0]
}

// Three writes make a chain that walks all the way back.
func TestRevisionChainWalksBackToTheFirstVersion(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("v1", "body one", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "v2", "body two"); err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "v3", "body three"); err != nil {
		t.Fatal(err)
	}

	data, err := w.ReadFile(MemoryFileName(id))
	if err != nil {
		t.Fatal(err)
	}

	titles := []string{}
	for content := string(data); ; {
		fm := ParseMemoryFrontmatter(content)
		titles = append(titles, fm.Title)
		if fm.Previous == "" {
			break
		}
		raw, readErr := w.ReadFile(fm.Previous)
		if readErr != nil {
			t.Fatalf("chain broken at %q: %v", fm.Previous, readErr)
		}
		content = string(raw)
		if len(titles) > 5 {
			t.Fatal("the chain does not terminate")
		}
	}

	if got := strings.Join(titles, ","); got != "v3,v2,v1" {
		t.Errorf("walking the chain gave %q, want \"v3,v2,v1\"", got)
	}
}

// An archived revision is COMPILED — it is searchable and readable like any memory — but it is
// not part of the CATALOGUE of what the project knows.
//
// This test used to assert the opposite for the wiki half, because history was deliberately
// unreachable. See docs/decisions/2026-09-01-memory-history-is-searchable-and-the-chain-is-two-way.md
// for why that reversed, and note what did NOT reverse: ListMemories still answers with live
// memories only, and the two pages must not share a slug.
func TestArchivedRevisionsAreCompiledButNotListed(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("Only one", "body", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "Only one, edited", ""); err != nil {
		t.Fatal(err)
	}

	archivePath := onlyArchivePath(t, w, id)
	if _, err := w.ReadFile(archivePath); err != nil {
		t.Fatalf("expected an archived revision: %v", err)
	}

	svc.localDir = w.Dir()
	list, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories: %v", err)
	}
	if len(list) != 1 {
		t.Errorf("ListMemories returned %d entries, want 1 — an archived revision is being catalogued", len(list))
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	res, err := GenerateMemoryWiki(context.Background(), w.Dir(), wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}
	if res.ArticlesWritten != 2 {
		t.Errorf("the wiki has %d articles, want 2 — the live memory and its superseded revision", res.ArticlesWritten)
	}

	live, superseded, _ := indexedMemoryPages(t, wikiDir)
	if live != 1 || superseded != 1 {
		t.Errorf("indexed %d live and %d superseded rows, want 1 and 1", live, superseded)
	}
}

// Two hits from one chain are one memory, so a search must answer with the current revision and
// say nothing about the old one. This is the property that made it safe to compile history at all.
func TestSearchCollapsesAChainToItsCurrentRevision(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	// zarquon is in both revisions, so a query for it matches the whole chain. plesiosaur is only
	// in the first, so a query for it reaches a revision the current memory cannot answer.
	id, err := svc.AddMemory("Indexing throughput", "the shared marker zarquon and the plesiosaur benchmark", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "Indexing throughput", "the shared marker zarquon and nothing else"); err != nil {
		t.Fatal(err)
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	if _, err := GenerateMemoryWiki(context.Background(), w.Dir(), wikiDir); err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}

	results := SearchChains(context.Background(), wikiDir, "zarquon", 10)
	if len(results) != 1 {
		for _, r := range results {
			t.Logf("hit %s superseded=%v current=%s", r.Path, r.Superseded, r.Current)
		}
		t.Fatalf("got %d results, want 1 — both revisions of one memory were returned", len(results))
	}
	if results[0].Superseded {
		t.Error("the surviving result is the superseded revision, want the current one")
	}
	if results[0].MemoryID != id {
		t.Errorf("result memory_id = %q, want %q", results[0].MemoryID, id)
	}

	// And a query only the old revision answers must still reach it, annotated with the current id.
	old := SearchChains(context.Background(), wikiDir, "plesiosaur", 10)
	if len(old) == 0 {
		t.Fatal("a query answered only by a superseded revision returned nothing — history is not searchable")
	}
	var sawSuperseded bool
	for _, r := range old {
		if r.Superseded && r.Current == id {
			sawSuperseded = true
		}
	}
	if !sawSuperseded {
		for _, r := range old {
			t.Logf("hit %s superseded=%v current=%s", r.Path, r.Superseded, r.Current)
		}
		t.Error("a superseded revision that matched alone was not returned with the current memory id")
	}
}

// Deleting a memory keeps its trail, which is what git did — a removed file stayed reachable in
// history. Nothing points at it afterwards, because the memory that would have carried the
// pointer is the one that went away.
func TestRemoveArchivesTheDeletedVersion(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("Doomed", "body", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.RemoveMemory(id); err != nil {
		t.Fatalf("RemoveMemory: %v", err)
	}

	if _, err := w.ReadFile(MemoryFileName(id)); !os.IsNotExist(err) {
		t.Errorf("the memory itself should be gone, got %v", err)
	}
	archived, err := w.ReadFile(onlyArchivePath(t, w, id))
	if err != nil {
		t.Fatalf("the deleted version was not archived: %v", err)
	}
	if !strings.Contains(string(archived), "Doomed") {
		t.Error("the archive does not hold the deleted memory")
	}
	// Nothing replaced it, and that empty `next` is what distinguishes the last state of a
	// deleted memory from a revision that was superseded.
	if got := ParseMemoryFrontmatter(string(archived)).Next; got != "" {
		t.Errorf("next = %q on the archive of a deleted memory, want empty", got)
	}
}

// A memory written before revisions existed has none. Its first edit must become revision 2
// rather than restarting the count, or the chain would claim the edit was the original.
func TestAMemoryWithoutARevisionIsTreatedAsTheFirst(t *testing.T) {
	t.Setenv("HOME", t.TempDir())

	old := "---\nid: X1\ntitle: Legacy\nscope: project\nscope_id: p\ntags: [memory, project]\n---\n\n# Legacy\n\nbody\n"
	archive := HistoryPath("X1", "01M1FZZZZZZZZZZZZZZZZZZZZZ")
	got := updatedMemoryContent(old, memoryUpdate{
		ID: "X1", Scope: "project", ScopeID: "p",
		NewTitle: "Legacy edited", Previous: archive,
	})

	fm := ParseMemoryFrontmatter(got)
	if fm.Revision != 2 {
		t.Errorf("revision = %d, want 2", fm.Revision)
	}
	if fm.Previous != archive {
		t.Errorf("previous = %q, want the first revision's path", fm.Previous)
	}
}

// The chain walks forward as well as back: every archive names its successor, and the newest one
// names the live memory.
func TestRevisionChainWalksForwardToTheLiveMemory(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("f1", "body one", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	for _, title := range []string{"f2", "f3"} {
		if err := svc.UpdateMemory(id, title, "body "+title); err != nil {
			t.Fatal(err)
		}
	}

	// Start at the oldest revision and walk `next` until the live memory.
	entries, err := w.ListDir(HistoryDirFor(id))
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, e := range entries {
		if !e.IsDir() && filepath.Ext(e.Name()) == ".md" {
			names = append(names, e.Name())
		}
	}
	if len(names) != 2 {
		t.Fatalf("history holds %d revisions, want 2", len(names))
	}
	sort.Strings(names)

	hops := 0
	at := HistoryDirFor(id) + "/" + names[0]
	for {
		data, readErr := w.ReadFile(at)
		if readErr != nil {
			t.Fatalf("chain broken at %q: %v", at, readErr)
		}
		fm := ParseMemoryFrontmatter(string(data))
		if fm.Next == "" {
			break
		}
		at = fm.Next
		hops++
		if hops > 5 {
			t.Fatal("the forward chain does not terminate")
		}
	}
	if at != MemoryFileName(id) {
		t.Errorf("walking next landed on %q, want the live memory %q", at, MemoryFileName(id))
	}
	if hops != 2 {
		t.Errorf("took %d hops from the oldest revision to the live memory, want 2", hops)
	}
}

// The user scope follows the unit, which is what makes `unit.id` meaningful: two units keep two
// user scopes, and setting the same id on two machines makes them one.
//
// The identity itself is tested in internal/config — it is not a memory concept.
func TestUserScopeIDFollowsTheUnit(t *testing.T) {
	t.Setenv("HOME", t.TempDir())

	t.Setenv("GRAPHIT_UNIT_ID", "unit-a")
	a, err := UserScopeID()
	if err != nil {
		t.Fatal(err)
	}

	t.Setenv("GRAPHIT_UNIT_ID", "unit-b")
	b, err := UserScopeID()
	if err != nil {
		t.Fatal(err)
	}

	if a == b {
		t.Error("two different units resolved to the same user scope")
	}
	// The raw value may be anything a person finds memorable, so the scope id has to be a token
	// that is safe in a directory name and in an object key.
	for _, got := range []string{a, b} {
		if len(got) != 16 || strings.ContainsAny(got, "/@. ") {
			t.Errorf("UserScopeID = %q, want a 16-character path-safe token", got)
		}
	}
}

// The project root comes from the lockfile, not from git. A project without git must resolve, and
// a nested project must resolve to itself rather than to an enclosing directory.
func TestProjectRootComesFromTheLockfileNotGit(t *testing.T) {
	root := t.TempDir()
	inner := filepath.Join(root, "packages", "inner")
	if err := os.MkdirAll(inner, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, brand.LockFileName()), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}

	t.Chdir(inner)
	if got := projectRootDir(); got != resolvePath(t, root) {
		t.Errorf("projectRootDir() = %q, want %q", got, root)
	}

	// A nested project with its own lockfile wins over the outer one — which `git rev-parse
	// --show-toplevel` could not do, because it answers with the repository root.
	if err := os.WriteFile(filepath.Join(inner, brand.LockFileName()), []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := projectRootDir(); got != resolvePath(t, inner) {
		t.Errorf("projectRootDir() = %q, want the nested project %q", got, inner)
	}
}

func resolvePath(t *testing.T, p string) string {
	t.Helper()
	resolved, err := filepath.EvalSymlinks(p)
	if err != nil {
		return p
	}
	return resolved
}
