package hub

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestGlobalLockManager_LoadAndSave(t *testing.T) {
	t.Parallel()

	t.Run("load nonexistent creates empty", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}
		lock, err := mgr.Load()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if lock.Version != GlobalLockVersion {
			t.Errorf("Version = %d, want %d", lock.Version, GlobalLockVersion)
		}
		if lock.Projects == nil {
			t.Error("expected Projects to be initialized")
		}
		if lock.Artifacts == nil {
			t.Error("expected Artifacts to be initialized")
		}
	})

	t.Run("save and load round-trip", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}
		lock := &GlobalHubLock{
			Version:   GlobalLockVersion,
			Projects:  map[string]*ProjectEntry{"p1": {Instances: []InstanceEntry{{Dir: "/tmp/p1"}}}},
			Artifacts: make(map[string]*GlobalArtifact),
		}
		if err := mgr.save(lock); err != nil {
			t.Fatalf("save error: %v", err)
		}
		loaded, err := mgr.Load()
		if err != nil {
			t.Fatalf("load error: %v", err)
		}
		if len(loaded.Projects) != 1 {
			t.Errorf("expected 1 project, got %d", len(loaded.Projects))
		}
	})

	t.Run("invalid JSON returns error", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "global.lock.json")
		if err := os.WriteFile(path, []byte("invalid"), 0o644); err != nil {
			t.Fatal(err)
		}
		mgr := &GlobalLockManager{lockPath: path}
		_, err := mgr.Load()
		if err == nil {
			t.Error("expected error for invalid JSON")
		}
	})

	t.Run("null projects/artifacts initialized", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "global.lock.json")
		data := `{"version":2}`
		if err := os.WriteFile(path, []byte(data), 0o644); err != nil {
			t.Fatal(err)
		}
		mgr := &GlobalLockManager{lockPath: path}
		lock, err := mgr.Load()
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if lock.Projects == nil {
			t.Error("expected Projects initialized")
		}
		if lock.Artifacts == nil {
			t.Error("expected Artifacts initialized")
		}
	})
}

func TestGlobalLockManager_LockPath(t *testing.T) {
	t.Parallel()
	mgr := &GlobalLockManager{lockPath: "/some/path"}
	if mgr.LockPath() != "/some/path" {
		t.Errorf("LockPath() = %q, want %q", mgr.LockPath(), "/some/path")
	}
}

func TestGlobalLockManager_RegisterInstall(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	// First install
	art, err := mgr.RegisterInstall(InstallRecord{ID: "my-rule", Version: "1.0.0", Type: TypeRule, Name: "My Rule", Description: "desc", Hash: "hash123", CachePath: "/cache", Owner: "proj1", OwnerDir: "/proj", LocalPath: "/local"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if art.ID != "my-rule" {
		t.Errorf("ID = %q, want %q", art.ID, "my-rule")
	}
	if art.Name != "My Rule" {
		t.Errorf("Name = %q, want %q", art.Name, "My Rule")
	}

	// Update existing
	art2, err := mgr.RegisterInstall(InstallRecord{ID: "my-rule", Version: "1.0.0", Type: TypeRule, Name: "Updated Name", Description: "new desc", Hash: "newhash", CachePath: "/newcache", Owner: "proj2", OwnerDir: "/proj2", LocalPath: "/local2"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if art2.Name != "Updated Name" {
		t.Errorf("Name = %q, want %q", art2.Name, "Updated Name")
	}
	if len(art2.Projects) != 2 {
		t.Errorf("expected 2 projects, got %d", len(art2.Projects))
	}
}

func TestGlobalLockManager_RegisterUninstall(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	_, _ = mgr.RegisterInstall(InstallRecord{ID: "my-rule", Version: "1.0.0", Type: TypeRule, Name: "My Rule", Owner: "proj1"})

	orphaned, err := mgr.RegisterUninstall("my-rule", "1.0.0", TypeRule, "proj1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !orphaned {
		t.Error("expected orphaned=true")
	}

	orphaned, err = mgr.RegisterUninstall("non-existent", "1.0.0", TypeRule, "proj1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if orphaned {
		t.Error("expected orphaned=false for non-existent")
	}
}

func TestGlobalLockManager_GCOrphans(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	cacheDir := filepath.Join(dir, "cache")
	if err := os.MkdirAll(cacheDir, 0o755); err != nil {
		t.Fatal(err)
	}

	// Manually create a lock with an orphaned artifact (no projects)
	lock := &GlobalHubLock{
		Version:  GlobalLockVersion,
		Projects: make(map[string]*ProjectEntry),
		Artifacts: map[string]*GlobalArtifact{
			"rule/my-rule@1.0.0": {
				ID:        "my-rule",
				Version:   "1.0.0",
				Type:      TypeRule,
				CachePath: cacheDir,
				Projects:  map[string]*ProjectInstall{}, // empty = orphan
			},
		},
	}
	if err := mgr.save(lock); err != nil {
		t.Fatal(err)
	}

	removed, err := mgr.GCOrphans()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(removed) != 1 {
		t.Errorf("expected 1 removed, got %d", len(removed))
	}

	// Verify cache dir was removed
	if _, err := os.Stat(cacheDir); !os.IsNotExist(err) {
		t.Error("expected cache dir to be removed")
	}
}

func TestGlobalLockManager_ListInstalledInProject(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	_, _ = mgr.RegisterInstall(InstallRecord{ID: "r1", Version: "1.0.0", Type: TypeRule, Name: "R1", Owner: "proj1"})
	_, _ = mgr.RegisterInstall(InstallRecord{ID: "r2", Version: "1.0.0", Type: TypeRule, Name: "R2", Owner: "proj2"})

	arts, err := mgr.ListInstalledInProject("proj1")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(arts) != 1 {
		t.Errorf("expected 1 artifact, got %d", len(arts))
	}
}

func TestGlobalLockManager_RegisterProject(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	err := mgr.RegisterProject("proj1", dir, WithProjectName("My Project"), WithProjectDescription("desc"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Load and verify
	lock, err := mgr.Load()
	if err != nil {
		t.Fatal(err)
	}
	entry := lock.Projects["proj1"]
	if entry == nil {
		t.Fatal("expected project entry")
	}
	if len(entry.Instances) != 1 {
		t.Fatalf("expected 1 instance, got %d", len(entry.Instances))
	}
	if entry.Instances[0].Name != "My Project" {
		t.Errorf("Name = %q, want %q", entry.Instances[0].Name, "My Project")
	}

	// Re-register updates existing
	err = mgr.RegisterProject("proj1", dir, WithProjectName("Updated"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	lock, _ = mgr.Load()
	if lock.Projects["proj1"].Instances[0].Name != "Updated" {
		t.Error("expected updated name")
	}
	if len(lock.Projects["proj1"].Instances) != 1 {
		t.Error("expected single instance after re-register")
	}
}

func TestGlobalLockManager_UnregisterProject(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	_ = mgr.RegisterProject("proj1", dir)

	err := mgr.UnregisterProject("proj1", dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	lock, _ := mgr.Load()
	if lock.Projects["proj1"] != nil {
		t.Error("expected project to be removed")
	}

	// Unregister non-existent is no-op
	err = mgr.UnregisterProject("nonexistent", dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestGlobalLockManager_SetCluster(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	_ = mgr.RegisterProject("proj1", dir)

	err := mgr.SetCluster("proj1", dir, "team", "backend")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	vals, err := mgr.GetCluster("proj1", dir, "team")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(vals) != 1 || vals[0] != "backend" {
		t.Errorf("expected [backend], got %v", vals)
	}

	// Set duplicate value is no-op
	err = mgr.SetCluster("proj1", dir, "team", "backend")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	vals, _ = mgr.GetCluster("proj1", dir, "team")
	if len(vals) != 1 {
		t.Errorf("expected 1 value, got %d", len(vals))
	}

	err = mgr.SetCluster("proj1", dir, "team", "frontend")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	vals, _ = mgr.GetCluster("proj1", dir, "team")
	if len(vals) != 2 {
		t.Errorf("expected 2 values, got %d", len(vals))
	}
}

func TestGlobalLockManager_UnsetCluster(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	_ = mgr.RegisterProject("proj1", dir)
	_ = mgr.SetCluster("proj1", dir, "team", "backend")

	err := mgr.UnsetCluster("proj1", dir, "team")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	vals, _ := mgr.GetCluster("proj1", dir, "team")
	if vals != nil {
		t.Errorf("expected nil, got %v", vals)
	}

	// Unset on nonexistent instance is no-op
	err = mgr.UnsetCluster("proj1", "/nonexistent", "team")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestGlobalLockManager_GetAllClusterLabels(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	_ = mgr.RegisterProject("proj1", dir)
	_ = mgr.SetCluster("proj1", dir, "team", "backend")
	_ = mgr.SetCluster("proj1", dir, "env", "prod")

	labels, err := mgr.GetAllClusterLabels("proj1", dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(labels) != 2 {
		t.Errorf("expected 2 labels, got %d", len(labels))
	}

	labels, err = mgr.GetAllClusterLabels("nonexistent", dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if labels != nil {
		t.Errorf("expected nil, got %v", labels)
	}
}

func TestGlobalLockManager_ValidateProjectDirs(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	_ = mgr.RegisterProject("proj1", "/nonexistent/dir")
	_ = mgr.RegisterProject("proj2", dir)

	// Create lockfile for proj2
	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "proj2", Name: "test"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	lockPath := filepath.Join(dir, "graphit.lock.json")
	_ = SaveLockfile(lockPath, lf)

	cleaned, err := mgr.ValidateProjectDirs()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if cleaned < 1 {
		t.Errorf("expected at least 1 cleaned, got %d", cleaned)
	}
}

func TestGlobalLockManager_ListActiveProjects(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	mgr := &GlobalLockManager{lockPath: filepath.Join(dir, "global.lock.json")}

	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "proj1", Name: "test"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	lockPath := filepath.Join(dir, "graphit.lock.json")
	_ = SaveLockfile(lockPath, lf)

	_ = mgr.RegisterProject("proj1", dir)

	active, err := mgr.ListActiveProjects()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(active) != 1 {
		t.Errorf("expected 1 active, got %d", len(active))
	}
}

func TestClusterMapUnmarshalJSON(t *testing.T) {
	t.Parallel()

	t.Run("standard format", func(t *testing.T) {
		t.Parallel()
		data := `{"team":["a","b"]}`
		var cm ClusterMap
		if err := json.Unmarshal([]byte(data), &cm); err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(cm["team"]) != 2 {
			t.Errorf("expected 2 values, got %d", len(cm["team"]))
		}
	})

	t.Run("string values", func(t *testing.T) {
		t.Parallel()
		data := `{"team":"backend"}`
		var cm ClusterMap
		if err := json.Unmarshal([]byte(data), &cm); err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(cm["team"]) != 1 || cm["team"][0] != "backend" {
			t.Errorf("expected [backend], got %v", cm["team"])
		}
	})

	t.Run("invalid JSON", func(t *testing.T) {
		t.Parallel()
		data := `invalid`
		var cm ClusterMap
		err := json.Unmarshal([]byte(data), &cm)
		if err == nil {
			t.Error("expected error for invalid JSON")
		}
	})

	t.Run("mixed array types", func(t *testing.T) {
		t.Parallel()
		data := `{"key":["str",123]}`
		var cm ClusterMap
		if err := json.Unmarshal([]byte(data), &cm); err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		// Non-string items should be skipped
		if len(cm["key"]) != 1 {
			t.Errorf("expected 1 value (only strings), got %d", len(cm["key"]))
		}
	})
}

func TestArtifactKey(t *testing.T) {
	t.Parallel()
	key := artifactKey("my-rule", "1.0.0", TypeRule)
	if key != "rule/my-rule@1.0.0" {
		t.Errorf("got %q, want %q", key, "rule/my-rule@1.0.0")
	}
}

func TestWithProjectName(t *testing.T) {
	t.Parallel()
	inst := &InstanceEntry{}
	WithProjectName("test-name")(inst)
	if inst.Name != "test-name" {
		t.Errorf("Name = %q, want %q", inst.Name, "test-name")
	}
}

func TestWithProjectDescription(t *testing.T) {
	t.Parallel()
	inst := &InstanceEntry{}
	WithProjectDescription("test-desc")(inst)
	if inst.Description != "test-desc" {
		t.Errorf("Description = %q, want %q", inst.Description, "test-desc")
	}
}
