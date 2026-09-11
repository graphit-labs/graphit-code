package store

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
)

func TestRegistryRoundTrip(t *testing.T) {
	projectDir := t.TempDir()
	sourcePath := filepath.Join(filepath.Dir(projectDir), "src", "other")

	if got := ListContexts(projectDir, KindAST); len(got) != 0 {
		t.Fatalf("a project with no registry reported %d contexts", len(got))
	}

	if err := AddContext(projectDir, KindAST, ContextRecord{Name: "Other Repo", SourcePath: sourcePath}); err != nil {
		t.Fatalf("AddContext: %v", err)
	}
	if err := AddContext(projectDir, KindKnowledge, ContextRecord{Name: "some-docs"}); err != nil {
		t.Fatalf("AddContext knowledge: %v", err)
	}

	for _, name := range []string{"Other Repo", "other-repo"} {
		rec, ok := LookupContext(projectDir, KindAST, name)
		if !ok {
			t.Fatalf("LookupContext(%q) missed", name)
		}
		if rec.SourcePath != sourcePath {
			t.Errorf("LookupContext(%q).SourcePath = %q", name, rec.SourcePath)
		}
	}

	if HasContext(projectDir, KindKnowledge, "other-repo") {
		t.Error("an AST context leaked into the knowledge kind")
	}

	if got := ContextNames(projectDir, KindAST); len(got) != 1 || got[0] != "other-repo" {
		t.Errorf("ContextNames = %v, want [other-repo]", got)
	}

	if err := RemoveContext(projectDir, KindAST, "Other Repo"); err != nil {
		t.Fatalf("RemoveContext: %v", err)
	}
	if HasContext(projectDir, KindAST, "other-repo") {
		t.Error("the context survived RemoveContext")
	}
	if !HasContext(projectDir, KindKnowledge, "some-docs") {
		t.Error("removing an AST context also removed a knowledge context")
	}

	if err := RemoveContext(projectDir, KindAST, "never-added"); err == nil {
		t.Error("RemoveContext reported success for a context that was never added")
	}
}

func TestMembershipIsRecordedInTheLockfileAndNowhereElse(t *testing.T) {
	projectDir := t.TempDir()
	if err := AddContext(projectDir, KindAST, ContextRecord{Name: "x"}); err != nil {
		t.Fatalf("AddContext: %v", err)
	}

	lockPath := filepath.Join(projectDir, brand.LockFileName())
	data, err := os.ReadFile(lockPath)
	if err != nil {
		t.Fatalf("membership not written to the lockfile at %s: %v", lockPath, err)
	}
	if !strings.Contains(string(data), `"x"`) {
		t.Errorf("the lockfile does not name the context:\n%s", data)
	}

	gone := filepath.Join(projectDir, brand.DotDir(), "contexts.json")
	if _, err := os.Stat(gone); err == nil {
		t.Errorf("a second registry was written at %s", gone)
	}
}

// A re-import overwrites rather than duplicating: the same name is the same context.
func TestAddContextIsIdempotentOnName(t *testing.T) {
	projectDir := t.TempDir()
	_ = AddContext(projectDir, KindAST, ContextRecord{Name: "repo", SourcePath: filepath.Join(projectDir, "a")})
	_ = AddContext(projectDir, KindAST, ContextRecord{Name: "repo", SourcePath: filepath.Join(projectDir, "b")})

	ctxs := ListContexts(projectDir, KindAST)
	if len(ctxs) != 1 {
		t.Fatalf("re-import produced %d entries, want 1", len(ctxs))
	}
	if got, want := ctxs["repo"].SourcePath, filepath.Join(projectDir, "b"); got != want {
		t.Errorf("SourcePath = %q, want the re-import to win (%q)", got, want)
	}
}

// A link records the sibling's DIRECTORY, and reading it back yields an absolute path
// again. No store path is stored: it is derived on every read, so it cannot freeze at
// the moment the link was made.
func TestALinkRecordsTheSiblingDirectory(t *testing.T) {
	base := t.TempDir()
	projectDir := filepath.Join(base, "app")
	sibling := filepath.Join(base, "lib")
	if err := os.MkdirAll(projectDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := AddContext(projectDir, KindAST, ContextRecord{
		Name: "sibling", SourcePath: sibling, Origin: projectlock.OriginLink,
	}); err != nil {
		t.Fatal(err)
	}
	rec, ok := LookupContext(projectDir, KindAST, "sibling")
	if !ok {
		t.Fatal("linked context not found")
	}
	if !rec.IsLink() {
		t.Error("the record does not report itself as a link")
	}
	if rec.SourcePath != sibling {
		t.Errorf("SourcePath = %q, want %q", rec.SourcePath, sibling)
	}
}
