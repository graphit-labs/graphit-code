package hub

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

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

func TestSyncIDEAdapterKeepsMCPAndHooksMachineIndependent(t *testing.T) {
	t.Setenv("HOME", t.TempDir())

	tests := []struct {
		ide      string
		hookPath string
		mcpPath  string
	}{
		{ide: "antigravity", hookPath: filepath.Join(".agents", "hooks.json"), mcpPath: filepath.Join(".agents", "mcp_config.json")},
		{ide: "cursor", hookPath: filepath.Join(".cursor", "hooks.json"), mcpPath: filepath.Join(".cursor", "mcp.json")},
		{ide: "claude", hookPath: filepath.Join(".claude", "settings.json"), mcpPath: ".mcp.json"},
		{ide: "kiro", hookPath: filepath.Join(".kiro", "hooks", "graphit-memory.json"), mcpPath: filepath.Join(".kiro", "settings", "mcp.json")},
		{ide: "codex", hookPath: filepath.Join(".codex", "hooks.json"), mcpPath: filepath.Join(".codex", "config.toml")},
		{ide: "opencode", hookPath: filepath.Join(".opencode", "plugins", "graphit-memory-session-start.js"), mcpPath: "opencode.json"},
		{ide: "gemini", hookPath: filepath.Join(".gemini", "settings.json"), mcpPath: filepath.Join(".gemini", "settings.json")},
	}

	for _, tc := range tests {
		t.Run(tc.ide, func(t *testing.T) {
			projectDir := t.TempDir()
			lf := &Lockfile{
				Project:   ProjectIdentity{ID: "sync-mcp-hooks-" + tc.ide},
				Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
			}

			const oldExecutable = "/opt/graphit-old/bin/graphit"
			const newExecutable = "/opt/graphit-new/bin/graphit"
			t.Setenv("GRAPHIT_LAUNCHER_PATH", oldExecutable)
			if err := SyncIDEAdapter(tc.ide, projectDir, lf); err != nil {
				t.Fatalf("first sync: %v", err)
			}

			t.Setenv("GRAPHIT_LAUNCHER_PATH", newExecutable)
			if err := SyncIDEAdapter(tc.ide, projectDir, lf); err != nil {
				t.Fatalf("second sync: %v", err)
			}

			hookPath := filepath.Join(projectDir, tc.hookPath)
			hookContent, err := os.ReadFile(hookPath)
			if err != nil {
				t.Fatalf("read hook surface %s: %v", hookPath, err)
			}
			if strings.Contains(string(hookContent), oldExecutable) || strings.Contains(string(hookContent), newExecutable) {
				t.Errorf("hook surface persisted a machine-specific executable: %s", hookContent)
			}
			portableHookCommand := "'" + brand.BinName() + "' _session-hook"
			if tc.ide == "opencode" {
				portableHookCommand = `Bun.spawnSync(["` + brand.BinName() + `", "_session-hook"`
			}
			if !strings.Contains(string(hookContent), portableHookCommand) {
				t.Errorf("hook surface does not contain the portable command %q: %s", portableHookCommand, hookContent)
			}

			mcpPath := filepath.Join(projectDir, tc.mcpPath)
			mcpContent, err := os.ReadFile(mcpPath)
			if err != nil {
				t.Fatalf("read MCP surface %s: %v", mcpPath, err)
			}
			if tc.ide == "gemini" {
				var settings map[string]any
				if err := json.Unmarshal(mcpContent, &settings); err != nil {
					t.Fatalf("decode Gemini settings %s: %v", mcpPath, err)
				}
				mcpContent, err = json.Marshal(settings["mcpServers"])
				if err != nil {
					t.Fatalf("encode Gemini MCP settings %s: %v", mcpPath, err)
				}
			}
			if strings.Contains(string(mcpContent), oldExecutable) || strings.Contains(string(mcpContent), newExecutable) {
				t.Errorf("MCP surface persisted a machine-specific executable: %s", mcpContent)
			}
			portableJSONCommand := `"` + brand.BinName() + `"`
			portableTOMLCommand := "command = '" + brand.BinName() + "'"
			if !strings.Contains(string(mcpContent), portableJSONCommand) && !strings.Contains(string(mcpContent), portableTOMLCommand) {
				t.Errorf("MCP surface does not contain the portable command %q: %s", brand.BinName(), mcpContent)
			}
		})
	}
}

// OnInit must register the project in the global lock. This is the fix for the
// scenario where a fresh setup followed by init on a project that already has a
// lockfile leaves the project unregistered.
func TestOnInitRegistersProjectInGlobalLock(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	const ide = "claude"
	target := t.TempDir()

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

	reg := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	if err := OnInit(context.Background(), reg, ide, target); err != nil {
		t.Fatalf("OnInit: %v", err)
	}

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
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)

	const ide = "claude"
	target := t.TempDir()

	reg := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	if err := OnInit(context.Background(), reg, ide, target); err != nil {
		t.Fatalf("OnInit: %v", err)
	}

	pp := paths.GetPathsForProject(ide, target)
	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil || lf == nil {
		t.Fatalf("lockfile not created: %v", err)
	}
	if lf.Project.ID == "" {
		t.Fatalf("lockfile has no project ID")
	}

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
