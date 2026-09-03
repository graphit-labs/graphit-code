package hub

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
)

func TestNewHubService(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := NewHubService(m)
	if svc == nil {
		t.Fatal("expected non-nil service")
	}
	if svc.registry != m {
		t.Error("expected registry to be set")
	}
}

func TestHubService_ListEntries(t *testing.T) {
	t.Parallel()

	t.Run("nil registry", func(t *testing.T) {
		t.Parallel()
		svc := &HubService{}
		result := svc.ListEntries("")
		if result != nil {
			t.Error("expected nil for nil registry")
		}
	})

	t.Run("with entries", func(t *testing.T) {
		t.Parallel()
		m := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		m.entries[TypeRule] = map[string]*Entry{
			"r1": {ID: "r1", Type: TypeRule},
		}
		svc := &HubService{registry: m}
		result := svc.ListEntries("")
		if len(result) != 1 {
			t.Errorf("expected 1 entry, got %d", len(result))
		}
	})
}

func TestHubService_Uninstall(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("lockfile not found", func(t *testing.T) {
		t.Parallel()
		err := svc.Uninstall(context.Background(), "test", TypeRule, false, "claude", "/nonexistent")
		if err != nil {
			t.Errorf("expected nil (no lockfile = skip), got: %v", err)
		}
	})

	t.Run("entry not found not forceRoot", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.Uninstall(context.Background(), "nonexistent", TypeRule, false, "claude", dir)
		if err == nil {
			t.Error("expected error for nonexistent entry")
		}
	})

	t.Run("entry not found forceRoot", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.Uninstall(context.Background(), "nonexistent", TypeRule, true, "claude", dir)
		if err != nil {
			t.Errorf("expected nil for forceRoot, got: %v", err)
		}
	})

	t.Run("find by scanning all types", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"my-rule": {Version: "1.0.0", RemoteID: "my-rule"},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.Uninstall(context.Background(), "my-rule", "", true, "claude", dir)
		if err != nil {
			t.Errorf("unexpected error: %v", err)
		}
	})
}

func TestHubService_UninstallAll(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("no lockfile", func(t *testing.T) {
		t.Parallel()
		err := svc.UninstallAll(context.Background(), "claude", "/nonexistent")
		if err != nil {
			t.Errorf("expected nil for no lockfile, got: %v", err)
		}
	})
}

func TestHubService_UpdateAll(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("no lockfile", func(t *testing.T) {
		t.Parallel()
		result := svc.UpdateAll(context.Background(), "claude", "/nonexistent")
		if len(result) != 0 {
			t.Errorf("expected empty result, got %d", len(result))
		}
	})

	t.Run("with lockfile, entries with no remoteID", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"local-rule": {Version: "1.0.0", RemoteID: ""},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		result := svc.UpdateAll(context.Background(), "claude", dir)
		if len(result) != 0 {
			t.Errorf("expected empty result, got %d", len(result))
		}
	})
}

func TestHubService_UpdateOne(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("no lockfile", func(t *testing.T) {
		t.Parallel()
		err := svc.UpdateOne(context.Background(), "test", TypeRule, "claude", "/nonexistent")
		if err == nil {
			t.Error("expected error for no lockfile")
		}
	})

	t.Run("not installed", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.UpdateOne(context.Background(), "nonexistent", TypeRule, "claude", dir)
		if err == nil {
			t.Error("expected error for nonexistent entry")
		}
	})

	t.Run("search by alias", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"real-id": {Version: "1.0.0", RemoteID: "real-id", Alias: "my-alias"},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.UpdateOne(context.Background(), "my-alias", TypeRule, "claude", dir)
		if err == nil {
			t.Error("expected error since entry not in registry")
		}
	})

	t.Run("search by alias without type", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"real-id": {Version: "1.0.0", RemoteID: "real-id", Alias: "my-alias"},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.UpdateOne(context.Background(), "my-alias", "", "claude", dir)
		if err == nil {
			t.Error("expected error since entry not in registry")
		}
	})

	t.Run("not found by alias without type", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"real-id": {Version: "1.0.0", RemoteID: "real-id", Alias: "other-alias"},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.UpdateOne(context.Background(), "nonexistent", "", "claude", dir)
		if err == nil {
			t.Error("expected error for nonexistent alias")
		}
	})

	t.Run("entry latest is empty", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		m2 := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		m2.entries[TypeRule] = map[string]*Entry{
			"my-rule": {ID: "my-rule", Type: TypeRule, Latest: ""},
		}
		svc2 := &HubService{registry: m2}

		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"my-rule": {Version: "1.0.0", RemoteID: "my-rule"},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc2.UpdateOne(context.Background(), "my-rule", TypeRule, "claude", dir)
		if err != nil {
			t.Errorf("expected nil for empty Latest, got: %v", err)
		}
	})
}

func TestHubService_RecordPublish(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("no lockfile", func(t *testing.T) {
		t.Parallel()
		err := svc.RecordPublish(context.Background(), "test", TypeRule, "1.0.0", "claude", "/nonexistent")
		if err == nil {
			t.Error("expected error for no lockfile")
		}
	})

	t.Run("lockfile not initialized", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		err := svc.RecordPublish(context.Background(), "test", TypeRule, "1.0.0", "claude", dir)
		if err == nil {
			t.Error("expected error for uninitialized lockfile")
		}
	})

	t.Run("success with existing entry", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"my-rule": {Version: "0.9.0", Origin: "local", RemoteID: ""},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.RecordPublish(context.Background(), "my-rule", TypeRule, "1.0.0", "claude", dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(lockPath)
		meta := lf2.Artifacts[TypeRule]["my-rule"]
		if meta.Version != "1.0.0" {
			t.Errorf("expected version 1.0.0, got %q", meta.Version)
		}
		if meta.Origin != "publish" {
			t.Errorf("expected origin 'publish', got %q", meta.Origin)
		}
	})

	t.Run("success with new entry", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.RecordPublish(context.Background(), "new-rule", TypeRule, "1.0.0", "claude", dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(lockPath)
		meta := lf2.Artifacts[TypeRule]["new-rule"]
		if meta == nil {
			t.Fatal("expected meta to be created")
		}
		if meta.Origin != "publish" {
			t.Errorf("expected origin 'publish', got %q", meta.Origin)
		}
	})
}

func TestHubService_RecordPublishInGlobalLock(t *testing.T) {
	t.Parallel()
	svc := &HubService{lockMgr: nil}
	svc.recordPublishInGlobalLock("test", TypeRule, "1.0.0", "proj", "/dir")
}

func TestHubService_Install_ValidationErrors(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("invalid artifact ID", func(t *testing.T) {
		t.Parallel()
		_, err := svc.Install(context.Background(), "../evil", "", "claude", TypeRule, "", "")
		if err == nil {
			t.Error("expected error for invalid artifact ID")
		}
	})

	t.Run("entry not found", func(t *testing.T) {
		t.Parallel()
		_, err := svc.Install(context.Background(), "nonexistent", "", "claude", TypeRule, "", "")
		if err == nil {
			t.Error("expected error for nonexistent entry")
		}
	})

	t.Run("with version constraint", func(t *testing.T) {
		t.Parallel()
		_, err := svc.Install(context.Background(), "nonexistent@1.0.0", "", "claude", TypeRule, "", "")
		if err == nil {
			t.Error("expected error for nonexistent entry")
		}
	})
}

func TestHubService_postInstallHook(t *testing.T) {
	t.Parallel()
	svc := &HubService{}
	pp := &paths.ProjectPaths{ActiveProjectDir: t.TempDir()}
	err := svc.postInstallHook(context.Background(), TypeRule, "test", "/tmp", pp)
	if err != nil {
		t.Errorf("expected no error, got: %v", err)
	}
}

func TestHubService_preUninstallHook(t *testing.T) {
	t.Parallel()

	t.Run("knowledge type with symlink", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		pp := &paths.ProjectPaths{ActiveProjectDir: d}
		dotDir := brand.DotDir()
		knDir := filepath.Join(d, dotDir, "knowledge")
		_ = os.MkdirAll(knDir, 0o755)
		target := filepath.Join(knDir, "test-proj")
		_ = os.Symlink("/tmp/fake", target)

		svc := &HubService{}
		meta := &LockfileArtifactMeta{ProjectID: "test-proj"}
		err := svc.preUninstallHook(context.Background(), TypeKnowledge, "test", meta, pp)
		if err != nil {
			t.Errorf("unexpected error: %v", err)
		}
	})

	t.Run("ast type with directory", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		pp := &paths.ProjectPaths{ActiveProjectDir: d}
		dotDir := brand.DotDir()
		astDir := filepath.Join(d, dotDir, "ast")
		_ = os.MkdirAll(filepath.Join(astDir, "test-proj"), 0o755)

		svc := &HubService{}
		meta := &LockfileArtifactMeta{ProjectID: "test-proj"}
		err := svc.preUninstallHook(context.Background(), TypeAST, "test", meta, pp)
		if err != nil {
			t.Errorf("unexpected error: %v", err)
		}
	})

	t.Run("knowledge with nil meta", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		pp := &paths.ProjectPaths{ActiveProjectDir: d}
		svc := &HubService{}
		err := svc.preUninstallHook(context.Background(), TypeKnowledge, "test-id", nil, pp)
		if err != nil {
			t.Errorf("unexpected error: %v", err)
		}
	})

	t.Run("ast with nil meta", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		pp := &paths.ProjectPaths{ActiveProjectDir: d}
		svc := &HubService{}
		err := svc.preUninstallHook(context.Background(), TypeAST, "test-id", nil, pp)
		if err != nil {
			t.Errorf("unexpected error: %v", err)
		}
	})

	t.Run("other type", func(t *testing.T) {
		t.Parallel()
		d := t.TempDir()
		pp := &paths.ProjectPaths{ActiveProjectDir: d}
		svc := &HubService{}
		err := svc.preUninstallHook(context.Background(), TypeRule, "test", nil, pp)
		if err != nil {
			t.Errorf("unexpected error: %v", err)
		}
	})
}

func TestFindCanonicalFile(t *testing.T) {
	t.Parallel()

	t.Run("rule with RULE.md", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "RULE.md"), []byte("# Rule"), 0o644)
		result := findCanonicalFile("rule", dir)
		if !filepath.IsAbs(result) || !strings.HasSuffix(result, "RULE.md") {
			t.Errorf("expected path ending with RULE.md, got %q", result)
		}
	})

	t.Run("agent with AGENT.md", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "AGENT.md"), []byte("# Agent"), 0o644)
		result := findCanonicalFile("agent", dir)
		if !strings.HasSuffix(result, "AGENT.md") {
			t.Errorf("expected AGENT.md, got %q", result)
		}
	})

	t.Run("skill with SKILL.md", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "SKILL.md"), []byte("# Skill"), 0o644)
		result := findCanonicalFile("skill", dir)
		if !strings.HasSuffix(result, "SKILL.md") {
			t.Errorf("expected SKILL.md, got %q", result)
		}
	})

	t.Run("command with COMMAND.md", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "COMMAND.md"), []byte("# Cmd"), 0o644)
		result := findCanonicalFile("command", dir)
		if !strings.HasSuffix(result, "COMMAND.md") {
			t.Errorf("expected COMMAND.md, got %q", result)
		}
	})

	t.Run("canonical not found, fallback to first file", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "other.txt"), []byte("data"), 0o644)
		result := findCanonicalFile("rule", dir)
		if !strings.HasSuffix(result, "other.txt") {
			t.Errorf("expected other.txt, got %q", result)
		}
	})

	t.Run("only dirs, no files", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.MkdirAll(filepath.Join(dir, "subdir"), 0o755)
		result := findCanonicalFile("rule", dir)
		if result != "" {
			t.Errorf("expected empty string for dir-only, got %q", result)
		}
	})

	t.Run("empty dir", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		result := findCanonicalFile("rule", dir)
		if result != "" {
			t.Errorf("expected empty string for empty dir, got %q", result)
		}
	})

	t.Run("unknown type with no canonical, fallback", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		_ = os.WriteFile(filepath.Join(dir, "file.txt"), []byte("data"), 0o644)
		result := findCanonicalFile("workflow", dir)
		if !strings.HasSuffix(result, "file.txt") {
			t.Errorf("expected file.txt, got %q", result)
		}
	})

	t.Run("nonexistent dir", func(t *testing.T) {
		t.Parallel()
		result := findCanonicalFile("rule", "/nonexistent/dir")
		if result != "" {
			t.Errorf("expected empty for nonexistent dir, got %q", result)
		}
	})
}

func TestResolveArtifactPath(t *testing.T) {
	t.Parallel()

	t.Run("with link source", func(t *testing.T) {
		t.Parallel()
		pp := &paths.ProjectPaths{ActiveProjectDir: t.TempDir()}
		meta := &LockfileArtifactMeta{LinkSource: "/tmp/linked"}
		result := resolveArtifactPath(meta, TypeRule, "test", pp)
		if result != "/tmp/linked" {
			t.Errorf("expected /tmp/linked, got %q", result)
		}
	})
}

func TestHubService_Link_Validation(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("invalid artifact ID", func(t *testing.T) {
		t.Parallel()
		_, err := svc.Link(context.Background(), "../evil", "/tmp", "claude", TypeRule, "")
		if err == nil {
			t.Error("expected error for invalid artifact ID")
		}
	})

	t.Run("empty type", func(t *testing.T) {
		t.Parallel()
		_, err := svc.Link(context.Background(), "test", "/tmp", "claude", "", "")
		if err == nil {
			t.Error("expected error for empty type")
		}
	})

	t.Run("source not a directory", func(t *testing.T) {
		t.Parallel()
		_, err := svc.Link(context.Background(), "test", "/nonexistent/path", "claude", TypeRule, "")
		if err == nil {
			t.Error("expected error for nonexistent source")
		}
	})
}

func TestHubService_Unlink_Validation(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("empty type", func(t *testing.T) {
		t.Parallel()
		err := svc.Unlink(context.Background(), "test", "claude", "", "")
		if err == nil {
			t.Error("expected error for empty type")
		}
	})

	t.Run("lockfile not found", func(t *testing.T) {
		t.Parallel()
		err := svc.Unlink(context.Background(), "test", "claude", TypeRule, "/nonexistent")
		if err == nil {
			t.Error("expected error for no lockfile")
		}
	})
}

func TestHubService_ResolveKnowledgeMount(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("entry not found", func(t *testing.T) {
		t.Parallel()
		_, err := svc.ResolveKnowledgeMount(context.Background(), "nonexistent")
		if err == nil {
			t.Error("expected error for nonexistent entry")
		}
	})

	t.Run("with version", func(t *testing.T) {
		t.Parallel()
		_, err := svc.ResolveKnowledgeMount(context.Background(), "nonexistent@1.0.0")
		if err == nil {
			t.Error("expected error for nonexistent entry")
		}
	})

	t.Run("found but no git store", func(t *testing.T) {
		t.Parallel()
		m2 := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		m2.entries[TypeKnowledge] = map[string]*Entry{
			"my-knowledge": {ID: "my-knowledge", Type: TypeKnowledge, Latest: "1.0.0", Versions: []string{"1.0.0"}},
		}
		svc2 := &HubService{registry: m2}
		_, err := svc2.ResolveKnowledgeMount(context.Background(), "my-knowledge")
		if err == nil {
			t.Error("expected error when registry not ready")
		}
	})
}
