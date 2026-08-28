package ast

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// withHome points brand.GlobalDir() at a temporary directory for the duration of a
// test, so the assertions below never touch the developer's real ~/.graphit.
func withHome(t *testing.T) string {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	return home
}

func TestHubContextDir_IsVersionScopedUnderGlobalDir(t *testing.T) {
	home := withHome(t)

	dir := HubContextDir("01ABCDEF", "1.2.3")
	want := filepath.Join(home, "."+brand.Brand, "ast", "hub", "01abcdef", "1.2.3")
	if dir != want {
		t.Errorf("HubContextDir = %q, want %q", dir, want)
	}

	icebug := store.ASTHubIcebugDir("01ABCDEF", "1.2.3")
	if icebug != filepath.Join(want, "graph.icebug") {
		t.Errorf("Hub icebug bundle = %q, want %q", icebug, filepath.Join(want, "graph.icebug"))
	}
}

func TestHubContextDir_SeparatesVersions(t *testing.T) {
	withHome(t)

	// The whole point of the shared store is that two projects on the same version
	// share one directory while two versions never collide.
	a := HubContextDir("proj", "1.0.0")
	b := HubContextDir("proj", "2.0.0")
	if a == b {
		t.Fatal("different versions resolved to the same directory")
	}
	if HubContextDir("proj", "1.0.0") != a {
		t.Error("the same project/version pair did not resolve to the same directory")
	}
}

func TestIsUnderHubContextsRoot(t *testing.T) {
	withHome(t)
	root := HubContextsRoot()

	if !IsUnderHubContextsRoot(HubContextDir("proj", "1.0.0")) {
		t.Error("a store built by the helpers must be reported inside the root")
	}
	if IsUnderHubContextsRoot(root) {
		t.Error("the root itself is not a store and must never be deletable")
	}
	if IsUnderHubContextsRoot(filepath.Join(root, "..", "elsewhere")) {
		t.Error("a path escaping the root must be rejected")
	}
	// A sibling whose name merely starts with the root's is the case a string
	// prefix check gets wrong.
	if IsUnderHubContextsRoot(root + "-other") {
		t.Error("a sibling directory sharing the root's prefix must be rejected")
	}
	if IsUnderHubContextsRoot("") {
		t.Error("an empty path must be rejected")
	}
}

func TestHubContextID(t *testing.T) {
	t.Parallel()
	if got := HubContextID("01ACME", "acme-ast"); got != "01ACME" {
		t.Errorf("HubContextID with a project = %q, want 01ACME", got)
	}
	if got := HubContextID("", "acme-ast"); got != "acme-ast" {
		t.Errorf("HubContextID without a project = %q, want acme-ast", got)
	}
}

func TestContextDirIn_PrefersHubStoreOverLocalDirectory(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()

	lock := `{"artifacts":{"ast":{"a":{"version":"3.0.0","project_id":"shared","origin":"hub"}}}}`
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(lock), 0o600); err != nil {
		t.Fatal(err)
	}

	// A directory left over from the previous per-project layout must not win: it is
	// a stale graph, and the lockfile names the authoritative one.
	stale := filepath.Join(projectDir, brand.DotDir(), "ast", "shared")
	if err := os.MkdirAll(filepath.Join(stale, "ladybugdb"), 0o755); err != nil {
		t.Fatal(err)
	}

	got := ContextDirIn(projectDir, "shared")
	if want := HubContextDir("shared", "3.0.0"); got != want {
		t.Errorf("ContextDirIn = %q, want the shared store %q", got, want)
	}
}

func TestContextDirIn_FallsBackToTheGlobalContextStore(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()
	// A name the lockfile does not claim resolves to the one store that name can
	// mean. There is nothing project-local to look for: the store is global and the
	// project's registry only decides what `ast list` reports.
	got := ContextDirIn(projectDir, "sibling")
	if want := store.ASTContextDir("sibling"); got != want {
		t.Errorf("ContextDirIn = %q, want %q", got, want)
	}
}

// A linked context points at a store this project does not own. What is recorded is the
// SIBLING'S DIRECTORY; the store is derived from it, so the link follows the sibling if
// it reindexes or re-keys instead of freezing at link time.
func TestContextDirIn_DerivesTheStoreOfALinkedSibling(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()
	siblingDir := t.TempDir()
	if err := LinkImportedContext(projectDir, "sibling", siblingDir); err != nil {
		t.Fatal(err)
	}
	want := store.ASTProjectDir(siblingDir)
	if got := ContextDirIn(projectDir, "sibling"); got != want {
		t.Errorf("ContextDirIn = %q, want the sibling's own store %q", got, want)
	}
}

// The recorded path is relative, so the same lockfile resolves after a clone. This is
// the property that makes it shareable, and it is checked on the stored bytes.
func TestALinkIsRecordedRelativeToTheProject(t *testing.T) {
	withHome(t)
	base := t.TempDir()
	projectDir := filepath.Join(base, "app")
	siblingDir := filepath.Join(base, "lib")
	for _, d := range []string{projectDir, siblingDir} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	if err := LinkImportedContext(projectDir, "lib", siblingDir); err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), `"source_path": "../lib"`) {
		t.Errorf("the lockfile should record the sibling relative to the project; got:\n%s", data)
	}
	if strings.Contains(string(data), base) {
		t.Errorf("the lockfile leaked an absolute path, which would not survive a clone:\n%s", data)
	}
}

func TestListImportedContextsIn_IncludesBuiltHubStores(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()

	lock := `{"artifacts":{"ast":{"a":{"version":"1.0.0","project_id":"01ACME","origin":"hub"},
	                            "b":{"version":"1.0.0","project_id":"01GHOST","origin":"hub"}}}}`
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(lock), 0o600); err != nil {
		t.Fatal(err)
	}

	// Only 01ACME has actually been built. 01GHOST is a claim with no store, which
	// must not be offered as a queryable context.
	built := HubContextDir("01ACME", "1.0.0")
	if err := os.MkdirAll(built, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(built, "schema.cypher"),
		[]byte("// mount schema"), 0o644); err != nil {
		t.Fatal(err)
	}

	contexts := ListImportedContextsIn(projectDir)

	if ictx, ok := contexts["01ACME"]; !ok {
		t.Error("expected the built Hub store to be listed")
	} else if ictx.StoreDir != built {
		t.Errorf("StoreDir = %q, want %q", ictx.StoreDir, built)
	}
	if _, ok := contexts["01GHOST"]; ok {
		t.Error("an unbuilt Hub store must not be listed")
	}
}

// A locally imported context is listed from the project's registry, and only once
// its store has actually been built — a registered name with no graph would offer a
// context whose every query fails.
func TestListImportedContextsIn_ListsRegisteredContextsWithABuiltStore(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()

	if _, err := AddImportedContext(projectDir, "real-context", "/src/real"); err != nil {
		t.Fatal(err)
	}
	if _, err := AddImportedContext(projectDir, "never-built", "/src/ghost"); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(store.ASTContextDir("real-context"), 0o755); err != nil {
		t.Fatal(err)
	}
	// A store counts as built only with the mount schema in place.
	if err := os.WriteFile(filepath.Join(store.ASTContextDir("real-context"), "schema.cypher"),
		[]byte("// placeholder mount"), 0o644); err != nil {
		t.Fatal(err)
	}

	contexts := ListImportedContextsIn(projectDir)
	if ictx, ok := contexts["real-context"]; !ok {
		t.Error("expected a registered, built context to be listed")
	} else if ictx.SourcePath != "/src/real" {
		t.Errorf("SourcePath = %q, want /src/real", ictx.SourcePath)
	}
	if _, ok := contexts["never-built"]; ok {
		t.Error("a registered context with no store must not be listed")
	}

	// Another project shares the same global store and claims nothing.
	if got := ListImportedContextsIn(t.TempDir()); len(got) != 0 {
		t.Errorf("a project that imported nothing listed %v", got)
	}

	// And dropping the claim un-lists it without touching the shared store.
	if err := RemoveImportedContext(projectDir, "real-context"); err != nil {
		t.Fatal(err)
	}
	if _, ok := ListImportedContextsIn(projectDir)["real-context"]; ok {
		t.Error("the context survived RemoveImportedContext")
	}
	if _, err := os.Stat(store.ASTContextDir("real-context")); err != nil {
		t.Error("RemoveImportedContext deleted the shared store")
	}
}

// The properties the removed HubContextsForProject / LookupHubContext / HubStoreExists
// tests used to cover, expressed against the single registry that replaced them.

func TestAHubContextIsNamedAfterItsPublishingProject(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()
	lock := `{"artifacts":{"ast":{"acme-graph":{"version":"1.2.0","project_id":"01ACME","origin":"hub"}}}}`
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(lock), 0o600); err != nil {
		t.Fatal(err)
	}

	ctxs := store.ListContexts(projectDir, store.KindAST)
	rec, ok := ctxs["01ACME"]
	if !ok {
		t.Fatalf("the context is not named after its publishing project: %+v", ctxs)
	}
	if rec.ArtifactID != "acme-graph" {
		t.Errorf("ArtifactID = %q, want the lockfile key", rec.ArtifactID)
	}
	if !rec.IsHub() {
		t.Error("a versioned hub entry should report itself as a hub context")
	}
	// Both names reach it, because callers echo back whichever one they were shown.
	for _, name := range []string{"01ACME", "acme-graph"} {
		if _, found := store.LookupContext(projectDir, store.KindAST, name); !found {
			t.Errorf("LookupContext(%q) found nothing", name)
		}
	}
}

func TestAHubContextResolvesToItsVersionedStore(t *testing.T) {
	withHome(t)
	projectDir := t.TempDir()
	lock := `{"artifacts":{"ast":{"acme-graph":{"version":"1.2.0","project_id":"01ACME","origin":"hub"}}}}`
	if err := os.WriteFile(filepath.Join(projectDir, brand.LockFileName()), []byte(lock), 0o600); err != nil {
		t.Fatal(err)
	}

	want := HubContextDir("01ACME", "1.2.0")
	if got := ContextDirIn(projectDir, "01ACME"); got != want {
		t.Errorf("ContextDirIn = %q, want the version-keyed store %q", got, want)
	}
}

func TestAProjectWithNoLockfileClaimsNothing(t *testing.T) {
	withHome(t)
	if ctxs := store.ListContexts(t.TempDir(), store.KindAST); len(ctxs) != 0 {
		t.Errorf("expected no contexts, got %+v", ctxs)
	}
}
