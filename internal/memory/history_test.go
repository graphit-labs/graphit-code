package memory

import (
	"context"
	"os"
	"path/filepath"
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
	if want := HistoryPath(id, 1); fm.Previous != want {
		t.Fatalf("previous = %q, want %q", fm.Previous, want)
	}

	// The pointer has to resolve, and to the exact bytes the memory had before.
	archived, err := w.ReadFile(fm.Previous)
	if err != nil {
		t.Fatalf("previous points at %q which cannot be read: %v", fm.Previous, err)
	}
	if string(archived) != string(before) {
		t.Errorf("the archived revision is not what the memory said before:\n--- archived\n%s\n--- before\n%s",
			archived, before)
	}
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

// An archived revision must never be mistaken for a memory: not listed, not compiled into the
// wiki. It is a subdirectory, and every listing in this package reads one level and skips
// directories — this test is what keeps that true.
func TestArchivedRevisionsAreNotMemories(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	svc, w := newLocalService(t)

	id, err := svc.AddMemory("Only one", "body", MemoryOpts{})
	if err != nil {
		t.Fatal(err)
	}
	if err := svc.UpdateMemory(id, "Only one, edited", ""); err != nil {
		t.Fatal(err)
	}

	// The archive exists.
	if _, err := w.ReadFile(HistoryPath(id, 1)); err != nil {
		t.Fatalf("expected an archived revision: %v", err)
	}

	svc.localDir = w.Dir()
	list, err := svc.ListMemories()
	if err != nil {
		t.Fatalf("ListMemories: %v", err)
	}
	if len(list) != 1 {
		t.Errorf("ListMemories returned %d entries, want 1 — an archived revision is being counted", len(list))
	}

	wikiDir := filepath.Join(t.TempDir(), "wiki")
	res, err := GenerateMemoryWiki(context.Background(), w.Dir(), wikiDir)
	if err != nil {
		t.Fatalf("GenerateMemoryWiki: %v", err)
	}
	if res.ArticlesWritten != 1 {
		t.Errorf("the wiki has %d articles, want 1 — an archived revision was compiled", res.ArticlesWritten)
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
	archived, err := w.ReadFile(HistoryPath(id, 1))
	if err != nil {
		t.Fatalf("the deleted version was not archived: %v", err)
	}
	if !strings.Contains(string(archived), "Doomed") {
		t.Error("the archive does not hold the deleted memory")
	}
}

// A memory written before revisions existed has none. Its first edit must become revision 2
// rather than restarting the count, or the chain would claim the edit was the original.
func TestAMemoryWithoutARevisionIsTreatedAsTheFirst(t *testing.T) {
	t.Setenv("HOME", t.TempDir())

	old := "---\nid: X1\ntitle: Legacy\nscope: project\nscope_id: p\ntags: [memory, project]\n---\n\n# Legacy\n\nbody\n"
	got := updatedMemoryContent(old, memoryUpdate{
		ID: "X1", Scope: "project", ScopeID: "p",
		NewTitle: "Legacy edited", Previous: HistoryPath("X1", 1),
	})

	fm := ParseMemoryFrontmatter(got)
	if fm.Revision != 2 {
		t.Errorf("revision = %d, want 2", fm.Revision)
	}
	if fm.Previous != HistoryPath("X1", 1) {
		t.Errorf("previous = %q, want the first revision's path", fm.Previous)
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
