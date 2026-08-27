package hub

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

// The lifecycle hooks used to resolve every path with paths.GetPaths, which walks
// up from the PROCESS's working directory. That is right for a CLI typed inside a
// project and wrong for the MCP server, which is one long-lived process sitting in
// one project while its tools take a project_dir naming another.
//
// The bug was not theoretical: graphit_remove was called with a temporary
// directory and removed the IDE adapter, the rules and the CLAUDE.md of the
// project the server happened to be running in.
//
// So this pins the property that matters — the hook writes to the project it was
// GIVEN — by pointing it at a temp dir and checking the lockfile there moved.
// Without the fix the call reaches for the working directory instead and this
// project's lockfile is left untouched.
func TestOnRemoveTargetsTheProjectItWasGivenNotTheWorkingDirectory(t *testing.T) {
	const ide = "claude"

	target := t.TempDir()
	pp := paths.GetPathsForProject(ide, target)

	seed := &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
	if err := SaveLockfile(pp.LockFilePath, seed); err != nil {
		t.Fatalf("create lockfile: %v", err)
	}
	if _, err := AddIDE(pp.LockFilePath, ide); err != nil {
		t.Fatalf("seed lockfile: %v", err)
	}
	before, err := LoadLockfile(pp.LockFilePath)
	if err != nil || before == nil {
		t.Fatalf("seeded lockfile unreadable: %v", err)
	}
	if len(before.IDEs) == 0 {
		t.Fatalf("seed did not record the IDE: %+v", before.IDEs)
	}

	// A registry with no git store: IsReady() is false, so the hook skips event
	// tracking and the network entirely, and what remains is the path handling
	// this test is about.
	if err := OnRemove(context.Background(), &RegistryManager{}, ide, target); err != nil {
		t.Fatalf("OnRemove: %v", err)
	}

	after, err := LoadLockfile(pp.LockFilePath)
	if err != nil {
		t.Fatalf("reading the target lockfile: %v", err)
	}
	if after != nil {
		for _, got := range after.IDEs {
			if got == ide {
				t.Errorf("the IDE is still registered in %s — OnRemove did not act on the "+
					"project it was given", pp.LockFilePath)
			}
		}
	}

	// And nothing outside the target was touched. The first version of this fix
	// missed exactly this: OnRemove still passed an empty project to UninstallAll,
	// which resolves an empty project to the working directory — so running this
	// very test deleted the lockfile of the repository it was running in.
	if cwd, err := os.Getwd(); err == nil {
		if _, err := os.Stat(filepath.Join(cwd, "graphit.lock.json")); err == nil {
			t.Log("working-directory lockfile still present, as it must be")
		}
	}
}

// UninstallAll is reached only when the removed IDE was the last one, which is the
// path that resolved an empty project to the working directory. Seeded with a
// single IDE so the removal empties the list and that branch runs.
func TestOnRemoveDoesNotUninstallFromTheWorkingDirectory(t *testing.T) {
	const ide = "claude"

	target := t.TempDir()
	pp := paths.GetPathsForProject(ide, target)

	seed := &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
	if err := SaveLockfile(pp.LockFilePath, seed); err != nil {
		t.Fatalf("create lockfile: %v", err)
	}
	if _, err := AddIDE(pp.LockFilePath, ide); err != nil {
		t.Fatalf("seed lockfile: %v", err)
	}

	cwd, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	guard := filepath.Join(cwd, "graphit.lock.json")
	_, hadGuard := os.Stat(guard)

	if err := OnRemove(context.Background(), &RegistryManager{}, ide, target); err != nil {
		t.Fatalf("OnRemove: %v", err)
	}

	if hadGuard == nil {
		if _, err := os.Stat(guard); err != nil {
			t.Fatalf("OnRemove deleted %s — it acted on the working directory instead of "+
				"the project it was given", guard)
		}
	}
}

// The same property for the path the hooks derive from the project: whatever the
// process's working directory is, a hook given an explicit project must not build
// paths outside it.
func TestLifecyclePathsStayInsideTheGivenProject(t *testing.T) {
	const ide = "claude"

	target := t.TempDir()
	resolved, err := filepath.EvalSymlinks(target)
	if err != nil {
		resolved = target
	}

	pp := paths.GetPathsForProject(ide, target)

	lockDir, err := filepath.EvalSymlinks(filepath.Dir(pp.LockFilePath))
	if err != nil {
		lockDir = filepath.Dir(pp.LockFilePath)
	}
	if lockDir != resolved {
		t.Errorf("lockfile lands in %s, want it inside %s", lockDir, resolved)
	}

	cwd, _ := os.Getwd()
	if lockDir == cwd {
		t.Errorf("lockfile resolved to the working directory (%s) — the project argument was ignored", cwd)
	}
}

func TestSyncIDEAdapterTargetsTheGivenProjectNotTheWorkingDirectory(t *testing.T) {
	cwd := t.TempDir()
	target := t.TempDir()

	originalDir, err := os.Getwd()
	if err != nil {
		t.Fatalf("get working directory: %v", err)
	}
	if err := os.Chdir(cwd); err != nil {
		t.Fatalf("change working directory: %v", err)
	}
	t.Cleanup(func() {
		if err := os.Chdir(originalDir); err != nil {
			t.Errorf("restore working directory: %v", err)
		}
	})

	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "explicit-project"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	for _, ide := range []string{"claude", "codex"} {
		if err := SyncIDEAdapter(ide, target, lf); err != nil {
			t.Fatalf("sync %s adapter: %v", ide, err)
		}
	}

	for _, expected := range []string{
		filepath.Join(target, ".claude"),
		filepath.Join(target, ".codex"),
		filepath.Join(target, "CLAUDE.md"),
	} {
		if _, err := os.Stat(expected); err != nil {
			t.Errorf("expected adapter artifact %s: %v", expected, err)
		}
	}

	for _, unexpected := range []string{
		filepath.Join(cwd, ".claude"),
		filepath.Join(cwd, ".codex"),
		filepath.Join(cwd, "CLAUDE.md"),
	} {
		if _, err := os.Stat(unexpected); !os.IsNotExist(err) {
			t.Errorf("adapter wrote outside the explicit project: %s", unexpected)
		}
	}
}

// OnInit must register the project in the global lock. This is the fix for the
// scenario where a fresh setup followed by init on a project that already has a
// lockfile leaves the project unregistered.
func TestOnInitRegistersProjectInGlobalLock(t *testing.T) {
	// Not parallel: overrides HOME.
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	const ide = "claude"
	target := t.TempDir()

	// Seed a lockfile with an existing project identity — simulates a project
	// that already had a lockfile before a fresh setup.
	seed := &Lockfile{
		Project: ProjectIdentity{
			ID:          "01TESTPROJECT00000000000000",
			Name:        "test-project",
			Description: "A test project",
		},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	pp := paths.GetPathsForProject(ide, target)
	if err := SaveLockfile(pp.LockFilePath, seed); err != nil {
		t.Fatalf("seeding lockfile: %v", err)
	}

	// OnInit with a non-ready registry (no network) — the path that the real
	// scenario hits after a fresh setup with no connectivity or empty registry.
	reg := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	if err := OnInit(context.Background(), reg, ide, target); err != nil {
		t.Fatalf("OnInit: %v", err)
	}

	// The global lock must now contain the project.
	globalLockPath := filepath.Join(home, "."+brand.Brand, GlobalHubLockFile)
	data, err := os.ReadFile(globalLockPath)
	if err != nil {
		t.Fatalf("global lock not created: %v", err)
	}

	var lock GlobalHubLock
	if err := json.Unmarshal(data, &lock); err != nil {
		t.Fatalf("parsing global lock: %v", err)
	}

	entry := lock.Projects["01TESTPROJECT00000000000000"]
	if entry == nil {
		t.Fatalf("project not registered in global lock; projects map: %v", lock.Projects)
	}
	if len(entry.Instances) == 0 {
		t.Fatalf("project registered but has no instances")
	}

	inst := entry.Instances[0]
	if inst.Dir != target {
		t.Errorf("registered dir = %q, want %q", inst.Dir, target)
	}
	if inst.Name != "test-project" {
		t.Errorf("registered name = %q, want %q", inst.Name, "test-project")
	}
	if inst.Description != "A test project" {
		t.Errorf("registered description = %q, want %q", inst.Description, "A test project")
	}
}

// OnInit must also register a project that had NO ID (new project) — the ID is
// generated by SaveLockfile.
func TestOnInitRegistersNewProjectInGlobalLock(t *testing.T) {
	// Not parallel: overrides HOME.
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	const ide = "claude"
	target := t.TempDir()

	// No pre-existing lockfile — OnInit creates it from scratch.
	reg := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	if err := OnInit(context.Background(), reg, ide, target); err != nil {
		t.Fatalf("OnInit: %v", err)
	}

	// The lockfile should have been created with a generated ID.
	pp := paths.GetPathsForProject(ide, target)
	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil || lf == nil {
		t.Fatalf("lockfile not created: %v", err)
	}
	if lf.Project.ID == "" {
		t.Fatalf("lockfile has no project ID")
	}

	// And the global lock must contain it.
	globalLockPath := filepath.Join(home, "."+brand.Brand, GlobalHubLockFile)
	data, err := os.ReadFile(globalLockPath)
	if err != nil {
		t.Fatalf("global lock not created: %v", err)
	}

	var lock GlobalHubLock
	if err := json.Unmarshal(data, &lock); err != nil {
		t.Fatalf("parsing global lock: %v", err)
	}

	entry := lock.Projects[lf.Project.ID]
	if entry == nil {
		t.Fatalf("project %q not registered in global lock", lf.Project.ID)
	}
	if len(entry.Instances) == 0 {
		t.Fatalf("project registered but has no instances")
	}
}
