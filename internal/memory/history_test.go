//go:build lancedb

package memory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func newLocalService(t *testing.T) *MemoryService {
	t.Helper()
	t.Setenv("GRAPHIT_HUB_BUCKET", "")

	return &MemoryService{
		scope:    MemoryScopeProject,
		scopeID:  "proj-1",
		store:    &MemoryStore{tableBase: filepath.Join(t.TempDir(), "tables")},
		tableURI: filepath.Join(t.TempDir(), "table"),
	}
}

// A memory is born at revision 1, with the unit that wrote it and no previous version.
func TestMemoryStartsAtRevisionOneWithNoPrevious(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

	id, err := svc.AddMemory("First", "the body", MemoryOpts{})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}

	data := mustReadStored(t, svc, MemoryFileName(id))
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

func TestUpdateArchivesThePreviousVersionAndPointsAtIt(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

	id, err := svc.AddMemory("First title", "first body", MemoryOpts{Type: MemoryTypeFact})
	if err != nil {
		t.Fatalf("AddMemory: %v", err)
	}
	before := mustReadStored(t, svc, MemoryFileName(id))

	if err := svc.UpdateMemory(id, "Second title", "second body"); err != nil {
		t.Fatalf("UpdateMemory: %v", err)
	}

	after := mustReadStored(t, svc, MemoryFileName(id))
	fm := ParseMemoryFrontmatter(string(after))

	if fm.Revision != 2 {
		t.Errorf("revision = %d, want 2", fm.Revision)
	}
	if fm.Title != "Second title" {
		t.Errorf("title = %q, want the new one", fm.Title)
	}
	if want := onlyArchivePath(t, svc, id); fm.Previous != want {
		t.Fatalf("previous = %q, want %q", fm.Previous, want)
	}
	if fm.Next != "" {
		t.Errorf("next = %q on the live memory, want empty — the head of a chain has no successor", fm.Next)
	}
	if fm.RevisionID != "" {
		t.Errorf("revision_id = %q on the live memory, want empty", fm.RevisionID)
	}

	archived := mustReadStored(t, svc, fm.Previous)
	if !sameMemoryBody(string(archived), string(before)) {
		t.Errorf("the archived revision is not what the memory said before:\n--- archived\n%s\n--- before\n%s",
			archived, before)
	}

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

func onlyArchivePath(t *testing.T, svc *MemoryService, id string) string {
	t.Helper()
	found := archivePaths(t, svc, id)
	if len(found) != 1 {
		t.Fatalf("history of %s holds %d revisions, want 1: %v", id, len(found), found)
	}
	return found[0]
}

// Three writes make a chain that walks all the way back.
func TestRevisionChainWalksBackToTheFirstVersion(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

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

	data := mustReadStored(t, svc, MemoryFileName(id))

	titles := []string{}
	for content := string(data); ; {
		fm := ParseMemoryFrontmatter(content)
		titles = append(titles, fm.Title)
		if fm.Previous == "" {
			break
		}
		raw := mustReadStored(t, svc, fm.Previous)
		content = string(raw)
		if len(titles) > 5 {
			t.Fatal("the chain does not terminate")
		}
	}

	if got := strings.Join(titles, ","); got != "v3,v2,v1" {
		t.Errorf("walking the chain gave %q, want \"v3,v2,v1\"", got)
	}
}

func TestArchivedRevisionsAreCompiledButNotListed(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

	id, err := svc.AddMemory("Only one", "body", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "Only one, edited", ""); err != nil {
		t.Fatal(err)
	}

	archivePath := onlyArchivePath(t, svc, id)
	if _, ok := readStored(t, svc, archivePath); !ok {
		t.Fatal("expected an archived revision")
	}

	list, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories: %v", err)
	}
	if len(list) != 1 {
		t.Errorf("ListMemories returned %d entries, want 1 — an archived revision is being catalogued", len(list))
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	res := compileFromTable(t, svc, wikiDir)
	if res.ArticlesWritten != 2 {
		t.Errorf("the wiki has %d articles, want 2 — the live memory and its superseded revision", res.ArticlesWritten)
	}

	live, superseded, _ := indexedMemoryPages(t, wikiDir)
	if live != 1 || superseded != 1 {
		t.Errorf("indexed %d live and %d superseded rows, want 1 and 1", live, superseded)
	}
}

func TestSearchCollapsesAChainToItsCurrentRevision(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

	id, err := svc.AddMemory("Indexing throughput", "the shared marker zarquon and the plesiosaur benchmark", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "Indexing throughput", "the shared marker zarquon and nothing else"); err != nil {
		t.Fatal(err)
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	compileFromTable(t, svc, wikiDir)

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

func TestSearchChainsWidensUntilTopKDistinctMemories(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)
	id, err := svc.AddMemory("Long history", "cursor-chain-marker revision 0", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	for i := 1; i <= 25; i++ {
		if err := svc.UpdateMemory(id, "Long history", fmt.Sprintf("cursor-chain-marker revision %d", i)); err != nil {
			t.Fatal(err)
		}
	}
	for i := range 3 {
		if _, err := svc.AddMemory(fmt.Sprintf("Other %d", i), "cursor-chain-marker distinct", MemoryOpts{}); err != nil {
			t.Fatal(err)
		}
	}
	wikiDir := filepath.Join(t.TempDir(), "wiki")
	compileFromTable(t, svc, wikiDir)
	results := SearchChains(context.Background(), wikiDir, "cursor-chain-marker", 4)
	if len(results) != 4 {
		t.Fatalf("got %d distinct chains after widening, want 4", len(results))
	}
	seen := map[string]bool{}
	for _, result := range results {
		if seen[result.MemoryID] {
			t.Fatalf("duplicate chain %q in %+v", result.MemoryID, results)
		}
		seen[result.MemoryID] = true
	}
}

func TestRemoveArchivesTheDeletedVersion(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc := newLocalService(t)

	id, err := svc.AddMemory("Doomed", "body", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.RemoveMemory(id); err != nil {
		t.Fatalf("RemoveMemory: %v", err)
	}

	if _, ok := readStored(t, svc, MemoryFileName(id)); ok {
		t.Error("the live memory should be gone from the store")
	}
	archived := mustReadStored(t, svc, onlyArchivePath(t, svc, id))
	if !strings.Contains(string(archived), "Doomed") {
		t.Error("the archive does not hold the deleted memory")
	}
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
	svc := newLocalService(t)

	id, err := svc.AddMemory("f1", "body one", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	for _, title := range []string{"f2", "f3"} {
		if err := svc.UpdateMemory(id, title, "body "+title); err != nil {
			t.Fatal(err)
		}
	}

	names := archivePaths(t, svc, id)
	if len(names) != 2 {
		t.Fatalf("history holds %d revisions, want 2", len(names))
	}
	sort.Strings(names)

	hops := 0
	at := HistoryDirFor(id) + "/" + names[0]
	for {
		data := mustReadStored(t, svc, at)
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

func readStored(t *testing.T, svc *MemoryService, rel string) (string, bool) {
	t.Helper()
	ctx := context.Background()
	tbl, err := svc.openTable(ctx)
	if err != nil {
		t.Fatalf("openTable: %v", err)
	}
	defer func() { _ = tbl.Close() }()

	key := MemoryIDFromFileName(rel)
	if isHistorySource(rel) {
		key = archiveKeyFromPath(rel)
	}
	rec, ok, err := tbl.Get(ctx, key)
	if err != nil {
		t.Fatalf("Get(%q): %v", key, err)
	}
	if !ok {
		return "", false
	}
	return rec.Markdown(), true
}

func mustReadStored(t *testing.T, svc *MemoryService, rel string) string {
	t.Helper()
	content, ok := readStored(t, svc, rel)
	if !ok {
		t.Fatalf("no stored record for %q", rel)
	}
	return content
}

func compileFromTable(t *testing.T, svc *MemoryService, wikiDir string) *WikiResult {
	t.Helper()
	ctx := context.Background()
	tbl, err := svc.openTable(ctx)
	if err != nil {
		t.Fatalf("openTable: %v", err)
	}
	defer func() { _ = tbl.Close() }()

	res, err := GenerateMemoryWikiFromTable(ctx, tbl, wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWikiFromTable: %v", err)
	}
	return res
}

func archivePaths(t *testing.T, svc *MemoryService, id string) []string {
	t.Helper()
	ctx := context.Background()
	tbl, err := svc.openTable(ctx)
	if err != nil {
		t.Fatalf("openTable: %v", err)
	}
	defer func() { _ = tbl.Close() }()

	revs, err := tbl.Revisions(ctx, id)
	if err != nil {
		t.Fatalf("reading the revisions of %s: %v", id, err)
	}
	out := make([]string, 0, len(revs))
	for _, r := range revs {
		out = append(out, HistoryPath(id, r.RevisionID))
	}
	return out
}
