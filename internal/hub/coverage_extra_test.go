package hub

import (
	"archive/zip"
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"mime/multipart"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// service.go – Uninstall deeper paths

func TestHubService_Uninstall_WithMembers(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	dir := t.TempDir()
	lf := &Lockfile{
		Project: ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
			TypeRule: {
				"parent-rule": {Version: "1.0.0", RemoteID: "parent-rule", Members: []string{"child-rule"}},
				"child-rule":  {Version: "1.0.0", RemoteID: "child-rule"},
			},
		},
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	err := svc.Uninstall(context.Background(), "parent-rule", TypeRule, true, "claude", dir)
	if err != nil {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestHubService_Uninstall_WithInstalledBy(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	t.Run("has multiple parents", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"child-rule": {Version: "1.0.0", RemoteID: "child-rule", InstalledBy: []string{"parent1", "parent2"}},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		err := svc.Uninstall(context.Background(), "child-rule", TypeRule, false, "claude", dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(lockPath)
		meta := lf2.Artifacts[TypeRule]["child-rule"]
		if meta == nil {
			t.Fatal("expected child-rule to still exist")
		}
		if len(meta.InstalledBy) != 1 {
			t.Errorf("expected 1 parent, got %d", len(meta.InstalledBy))
		}
	})
}

func TestHubService_Uninstall_IDEArtifactRemoval(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	dir := t.TempDir()
	artDir := filepath.Join(dir, ".claude", "rules", "my-rule")
	_ = os.MkdirAll(artDir, 0o755)
	_ = os.WriteFile(filepath.Join(artDir, "RULE.md"), []byte("# Rule"), 0o644)

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

	err := svc.Uninstall(context.Background(), "my-rule", TypeRule, true, "claude", dir)
	if err != nil {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestHubService_Uninstall_CleanupEmptyTypeMap(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	dir := t.TempDir()
	lf := &Lockfile{
		Project: ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
			TypeRule: {
				"only-rule": {Version: "1.0.0", RemoteID: "only-rule"},
			},
		},
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	_ = svc.Uninstall(context.Background(), "only-rule", TypeRule, true, "claude", dir)

	lf2, _ := LoadLockfile(lockPath)
	if _, exists := lf2.Artifacts[TypeRule]; exists {
		t.Error("expected TypeRule map to be deleted when empty")
	}
}

// service.go – Link deeper paths

func TestHubService_Link_AST(t *testing.T) {
	// Not parallel: the source project's store is resolved against HOME.
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	dir := t.TempDir()
	sourceDir := t.TempDir()

	// Linking points at the source project's GLOBAL store, so that is what has to
	// exist — there is nothing inside the source project to link to any more. A
	// store counts as built with its mounted DDL (schema.cypher) present: the local
	// graph is icebug filesystem in-memory, so there is no database file to check.
	_ = os.MkdirAll(store.ASTProjectIcebugDir(sourceDir), 0o755)
	_ = os.WriteFile(filepath.Join(store.ASTProjectIcebugDir(sourceDir), "schema.cypher"), []byte("// mount"), 0o644)

	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	result, err := svc.Link(context.Background(), "my-ast", sourceDir, "claude", TypeAST, dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.ArtType != TypeAST {
		t.Errorf("expected TypeAST, got %q", result.ArtType)
	}
}

func TestHubService_Link_Knowledge(t *testing.T) {
	// Not parallel: the source project's wiki is resolved against HOME.
	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	dir := t.TempDir()
	sourceDir := t.TempDir()

	knSourceDir := store.KnowledgeProjectDir(sourceDir)
	_ = os.MkdirAll(knSourceDir, 0o755)
	_ = os.WriteFile(filepath.Join(knSourceDir, "index.md"), []byte("# Wiki"), 0o644)

	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	result, err := svc.Link(context.Background(), "my-knowledge", sourceDir, "claude", TypeKnowledge, dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.ArtType != TypeKnowledge {
		t.Errorf("expected TypeKnowledge, got %q", result.ArtType)
	}
}

func TestHubService_Link_AST_SourceNotFound(t *testing.T) {
	t.Parallel()
	sourceDir := t.TempDir()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}
	_, err := svc.Link(context.Background(), "my-ast", sourceDir, "claude", TypeAST, t.TempDir())
	if err == nil {
		t.Error("expected error for missing AST source")
	}
}

func TestHubService_Link_Knowledge_SourceNotFound(t *testing.T) {
	t.Parallel()
	sourceDir := t.TempDir()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}
	_, err := svc.Link(context.Background(), "my-kn", sourceDir, "claude", TypeKnowledge, t.TempDir())
	if err == nil {
		t.Error("expected error for missing knowledge source")
	}
}

// service.go – Unlink deeper paths

func TestHubService_Unlink_Success(t *testing.T) {
	t.Parallel()

	t.Run("AST type", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		dotDir := brand.DotDir()
		astLink := filepath.Join(dir, dotDir, "ast", "my-ast")
		_ = os.MkdirAll(filepath.Dir(astLink), 0o755)
		_ = os.Symlink("/tmp/fake", astLink)

		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeAST: {
					"my-ast": {Version: "local", Origin: "link", LinkSource: "/tmp/fake"},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		m := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		svc := &HubService{registry: m}
		err := svc.Unlink(context.Background(), "my-ast", "claude", TypeAST, dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})

	t.Run("knowledge type", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		dotDir := brand.DotDir()
		knLink := filepath.Join(dir, dotDir, "knowledge", "my-kn")
		_ = os.MkdirAll(filepath.Dir(knLink), 0o755)
		_ = os.Symlink("/tmp/fake", knLink)

		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeKnowledge: {
					"my-kn": {Version: "local", Origin: "link", LinkSource: "/tmp/fake"},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		m := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		svc := &HubService{registry: m}
		err := svc.Unlink(context.Background(), "my-kn", "claude", TypeKnowledge, dir)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})

	t.Run("not a link artifact", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"my-rule": {Version: "1.0.0", Origin: "hub"},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		m := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		svc := &HubService{registry: m}
		err := svc.Unlink(context.Background(), "my-rule", "claude", TypeRule, dir)
		if err == nil {
			t.Error("expected error for non-link artifact")
		}
	})

	t.Run("artifact not found", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)

		m := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		svc := &HubService{registry: m}
		err := svc.Unlink(context.Background(), "nonexistent", "claude", TypeRule, dir)
		if err == nil {
			t.Error("expected error for not found artifact")
		}
	})
}

func TestHubService_ResolveKnowledgeMount_VersionBranches(t *testing.T) {
	t.Parallel()

	t.Run("invalid version constraint", func(t *testing.T) {
		t.Parallel()
		m := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		m.entries[TypeKnowledge] = map[string]*Entry{
			"my-kn": {ID: "my-kn", Type: TypeKnowledge, Latest: "1.0.0"},
		}
		svc := &HubService{registry: m}
		_, err := svc.ResolveKnowledgeMount(context.Background(), "my-kn@!!!invalid")
		if err == nil {
			t.Error("expected error for invalid constraint")
		}
	})

	t.Run("version constraint with versions list - no match", func(t *testing.T) {
		t.Parallel()
		m := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		m.entries[TypeKnowledge] = map[string]*Entry{
			"my-kn": {ID: "my-kn", Type: TypeKnowledge, Latest: "2.0.0", Versions: []string{"1.0.0", "2.0.0"}},
		}
		svc := &HubService{registry: m}
		_, err := svc.ResolveKnowledgeMount(context.Background(), "my-kn@^99.0.0")
		if err == nil {
			t.Error("expected error for no matching version")
		}
	})
}

func TestHubService_RecordPublish_ExistingWithEmptyOrigin(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	lf := &Lockfile{
		Project: ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
			TypeRule: {
				"my-rule": {Version: "0.9.0", Origin: ""},
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
	if meta.Origin != "publish" {
		t.Errorf("expected 'publish', got %q", meta.Origin)
	}
}

func TestHubService_UpdateOne_WithConstraint(t *testing.T) {
	t.Parallel()

	t.Run("same version, same hash", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		m := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
		}
		m.entries[TypeRule] = map[string]*Entry{
			"my-rule": {
				ID: "my-rule", Type: TypeRule, Latest: "1.0.0",
				Hashes: map[string]string{"1.0.0": "abc123"},
			},
		}
		svc := &HubService{registry: m}

		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"my-rule": {Version: "1.0.0", RemoteID: "my-rule", Hash: "abc123", LinkSource: dir},
				},
			},
		}
		lockPath := filepath.Join(dir, brand.LockFileName())
		_ = SaveLockfile(lockPath, lf)
		_ = os.WriteFile(filepath.Join(dir, "RULE.md"), []byte("# Rule"), 0o644)

		err := svc.UpdateOne(context.Background(), "my-rule", TypeRule, "claude", dir)
		if err != nil {
			t.Logf("UpdateOne: %v", err)
		}
	})
}

func TestHubService_UninstallAll_WithEntries(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	svc := &HubService{registry: m}

	dir := t.TempDir()
	lf := &Lockfile{
		Project: ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
			TypeRule: {
				"r1": {Version: "1.0.0", RemoteID: "r1"},
				"r2": {Version: "1.0.0", RemoteID: "r2"},
			},
		},
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	err := svc.UninstallAll(context.Background(), "claude", dir)
	if err != nil {
		t.Logf("UninstallAll: %v", err)
	}
}

func TestHubService_UpdateAll_WithRemoteEntries(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.entries[TypeRule] = map[string]*Entry{
		"r1": {ID: "r1", Type: TypeRule, Latest: "1.0.0"},
	}
	svc := &HubService{registry: m}

	dir := t.TempDir()
	lf := &Lockfile{
		Project: ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
			TypeRule: {
				"r1": {Version: "1.0.0", RemoteID: "r1"},
			},
		},
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	results := svc.UpdateAll(context.Background(), "claude", dir)
	_ = results
}

func TestReconcileManagedArtifacts_WithEntries(t *testing.T) {
	t.Parallel()

	t.Run("entries matching project ID", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lockPath := filepath.Join(dir, brand.LockFileName())
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "my-project", Name: "Test"},
			IDEs:      []string{"claude"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		_ = SaveLockfile(lockPath, lf)

		reg := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
			store:    &S3Store{},
		}
		reg.entries[TypeRule] = map[string]*Entry{
			"proj-rule": {ID: "proj-rule", Type: TypeRule, Latest: "1.0.0", ProjectID: "my-project"},
		}

		err := ReconcileManagedArtifacts(reg, lockPath)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(lockPath)
		if lf2.Artifacts[TypeRule]["proj-rule"] == nil {
			t.Error("expected proj-rule to be reconciled")
		}
	})

	t.Run("entries not matching project ID", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lockPath := filepath.Join(dir, brand.LockFileName())
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "my-project", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		_ = SaveLockfile(lockPath, lf)

		reg := &RegistryManager{
			entries:  make(map[ArtifactType]map[string]*Entry),
			projects: make(map[string]*Project),
			store:    &S3Store{},
		}
		reg.entries[TypeRule] = map[string]*Entry{
			"other-rule": {ID: "other-rule", Type: TypeRule, Latest: "1.0.0", ProjectID: "other-project"},
		}

		err := ReconcileManagedArtifacts(reg, lockPath)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(lockPath)
		if lf2.Artifacts[TypeRule]["other-rule"] != nil {
			t.Error("expected other-rule to NOT be reconciled (different project)")
		}
	})
}

func TestArtifactExistsForIDE_UnknownTypeDir(t *testing.T) {
	t.Parallel()
	lf := &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
	entry := &Entry{ID: "test"}
	result := artifactExistsForIDE(lf, TypeMCP, entry, "claude", "/tmp")
	if result {
		t.Error("expected false for type without IDE type dir")
	}
}

func TestUIServer_handleUpload_MissingID(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	_ = writer.WriteField("type", "rule")
	writer.Close()

	req := httptest.NewRequest("POST", "/api/upload", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for missing ID")
	}
}

func TestUIServer_handleUpload_NoFile_NotPower(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	_ = writer.WriteField("id", "test-artifact")
	_ = writer.WriteField("type", "rule")
	writer.Close()

	req := httptest.NewRequest("POST", "/api/upload", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure when no file is uploaded for non-power type")
	}
}

func TestUIServer_handleUpload_WithFile(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	_ = writer.WriteField("id", "test-artifact")
	_ = writer.WriteField("type", "rule")
	_ = writer.WriteField("version", "1.0.0")
	_ = writer.WriteField("name", "Test Rule")
	_ = writer.WriteField("description", "A test rule")
	_ = writer.WriteField("tags", "go, test")
	_ = writer.WriteField("author", "testuser")
	_ = writer.WriteField("scope", "global")

	part, _ := writer.CreateFormFile("file", "rule.md")
	_, _ = part.Write([]byte("# Rule content"))
	writer.Close()

	req := httptest.NewRequest("POST", "/api/upload", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleUpload_WithZipFile(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	var zipBuf bytes.Buffer
	zw := zip.NewWriter(&zipBuf)
	f, _ := zw.Create("RULE.md")
	_, _ = f.Write([]byte("# Rule"))
	zw.Close()

	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	_ = writer.WriteField("id", "test-zip-art")
	_ = writer.WriteField("type", "rule")
	_ = writer.WriteField("version", "1.0.0")

	part, _ := writer.CreateFormFile("file", "artifact.zip")
	_, _ = part.Write(zipBuf.Bytes())
	writer.Close()

	req := httptest.NewRequest("POST", "/api/upload", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleUpload_WithDependencies(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	_ = writer.WriteField("id", "test-dep-art")
	_ = writer.WriteField("type", "power")
	_ = writer.WriteField("version", "1.0.0")
	_ = writer.WriteField("dependencies", `[{"id":"dep1","type":"rule","version":"1.0.0"}]`)

	part, _ := writer.CreateFormFile("file", "power.md")
	_, _ = part.Write([]byte("# Power"))
	writer.Close()

	req := httptest.NewRequest("POST", "/api/upload", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleUpload_ProjectScoped(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	_ = writer.WriteField("id", "test-proj-art")
	_ = writer.WriteField("type", "rule")
	_ = writer.WriteField("version", "1.0.0")
	_ = writer.WriteField("scope", "project")
	_ = writer.WriteField("project_dir", dir)

	part, _ := writer.CreateFormFile("file", "rule.md")
	_, _ = part.Write([]byte("# Rule"))
	writer.Close()

	req := httptest.NewRequest("POST", "/api/upload", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleUpload_ProjectScopedNoDir(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	_ = writer.WriteField("id", "test-proj-art2")
	_ = writer.WriteField("type", "rule")
	_ = writer.WriteField("version", "1.0.0")
	_ = writer.WriteField("scope", "project")

	part, _ := writer.CreateFormFile("file", "rule.md")
	_, _ = part.Write([]byte("# Rule"))
	writer.Close()

	req := httptest.NewRequest("POST", "/api/upload", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["success"] != false {
		t.Error("expected failure for project-scoped without project_dir")
	}
}

func TestUIServer_handleUpload_PowerNoFile(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := &bytes.Buffer{}
	writer := multipart.NewWriter(body)
	_ = writer.WriteField("id", "test-power")
	_ = writer.WriteField("type", "power")
	_ = writer.WriteField("version", "1.0.0")
	_ = writer.WriteField("scope", "global")
	writer.Close()

	req := httptest.NewRequest("POST", "/api/upload", body)
	req.Header.Set("Content-Type", writer.FormDataContentType())
	w := httptest.NewRecorder()
	s.handleUpload(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleSubmit_WithDeps(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	body := fmt.Sprintf(`{
		"id":"test-dep",
		"path":"%s",
		"type":"power",
		"version":"1.0.0",
		"tags":"go, test",
		"author":"testuser",
		"global":true,
		"dependencies":[{"id":"dep1","type":"rule","version":"1.0.0"},{"id":"","type":"","version":""}]
	}`, dir)
	req := httptest.NewRequest("POST", "/api/submit", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleSubmit(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleSubmit_ProjectScopedWithLockfile(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()
	artDir := t.TempDir()

	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "proj-123", Name: "Test"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	body := fmt.Sprintf(`{
		"id":"test-proj-scoped",
		"path":"%s",
		"global":false,
		"project_dir":"%s"
	}`, artDir, dir)
	req := httptest.NewRequest("POST", "/api/submit", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleSubmit(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleUnpublish_WithProjectDir(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/unpublish", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUnpublish(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
}

func TestUIServer_handleUnlink_WithIDEOverride(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","ide":"cursor","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/unlink", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUnlink(w, req)

	if w.Code == http.StatusBadRequest {
		t.Error("unexpected bad request for valid JSON body")
	}
}

func TestUIServer_handleProjectArtifacts_WithLockfile(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()
	dotDir := brand.DotDir()

	lf := &Lockfile{
		Project: ProjectIdentity{ID: "test-id", Name: "TestProject"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
			TypeRule: {
				"imported-rule": {Version: "1.0.0", RemoteID: "imported-rule", Origin: "hub", RequestedVersion: "^1.0.0"},
			},
		},
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	knDir := filepath.Join(dir, dotDir, "knowledge", "project")
	_ = os.MkdirAll(knDir, 0o755)
	_ = os.WriteFile(filepath.Join(knDir, "index.md"), []byte("# Wiki"), 0o644)

	astDir := filepath.Join(dir, dotDir, "ast", "project")
	_ = os.MkdirAll(astDir, 0o755)

	req := httptest.NewRequest("GET", "/api/project-artifacts?project_dir="+dir, nil)
	w := httptest.NewRecorder()
	s.handleProjectArtifacts(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	if resp["project_name"] == nil {
		t.Error("expected project_name in response")
	}
}

func TestUIServer_handleRegistry_WithLockfile(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	lf := &Lockfile{
		Project: ProjectIdentity{ID: "test-id", Name: "TestProject"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
			TypeRule: {
				"hub-rule":     {Version: "1.0.0", RemoteID: "hub-rule", Origin: "hub"},
				"managed-rule": {Version: "1.0.0", RemoteID: "managed-rule", Origin: "managed"},
				"publish-rule": {Version: "1.0.0", RemoteID: "publish-rule", Origin: "publish"},
			},
		},
	}
	lockPath := filepath.Join(dir, brand.LockFileName())
	_ = SaveLockfile(lockPath, lf)

	req := httptest.NewRequest("GET", "/api/registry?project_dir="+dir, nil)
	w := httptest.NewRecorder()
	s.handleRegistry(w, req)

	var resp map[string]any
	_ = json.Unmarshal(w.Body.Bytes(), &resp)
	_, ok := resp["installed"].([]any)
	if !ok {
		t.Fatal("expected installed array")
	}
}

func TestScanMCPArtifacts_WithServers(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	coreServer := brand.MCPServerName("code-stdio")
	managedKey := brand.ManagedMCPKey()

	t.Run("with managed filter", func(t *testing.T) {
		t.Parallel()
		p := filepath.Join(dir, "mcp1.json")
		data := fmt.Sprintf(`{
			"mcpServers":{"%s":{},"my-server":{},"other-server":{}},
			"%s":{"my-server":{}}
		}`, coreServer, managedKey)
		_ = os.WriteFile(p, []byte(data), 0o644)

		result := scanMCPArtifacts(p)
		if len(result) != 1 {
			t.Errorf("expected 1 result, got %d", len(result))
		}
	})

	t.Run("no managed key", func(t *testing.T) {
		t.Parallel()
		p := filepath.Join(dir, "mcp2.json")
		data := fmt.Sprintf(`{"mcpServers":{"%s":{},"my-server":{}}}`, coreServer)
		_ = os.WriteFile(p, []byte(data), 0o644)

		result := scanMCPArtifacts(p)
		if len(result) != 1 {
			t.Errorf("expected 1 non-core server, got %d", len(result))
		}
	})
}

func TestUIServer_handleInstall_IDEFallback(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","project_dir":"/tmp/test","ide":""}`
	req := httptest.NewRequest("POST", "/api/install", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleInstall(w, req)

	if w.Header().Get("Content-Type") != "application/json" {
		t.Error("expected JSON response")
	}
}

func TestUIServer_handleUpdateAll_IDEFallback(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)
	dir := t.TempDir()

	body := fmt.Sprintf(`{"ide":"","project_dir":"%s"}`, dir)
	req := httptest.NewRequest("POST", "/api/update_all", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUpdateAll(w, req)

	if w.Header().Get("Content-Type") != "application/json" {
		t.Error("expected JSON response")
	}
}

func TestUIServer_handleUpdateOne_IDEFallback(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","ide":"","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/update_one", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUpdateOne(w, req)

	if w.Header().Get("Content-Type") != "application/json" {
		t.Error("expected JSON response")
	}
}

func TestResolveArtifactPath_NoLinkSource(t *testing.T) {
	t.Parallel()
	pp := &paths.ProjectPaths{
		ActiveProjectDir: t.TempDir(),
		ResourcesDir:     t.TempDir(),
	}
	meta := &LockfileArtifactMeta{Version: "1.0.0", RemoteID: "my-rule"}
	result := resolveArtifactPath(meta, TypeRule, "my-rule", pp)
	if result == "" {
		t.Error("expected non-empty path")
	}
}

func TestHubService_Install_InvalidVersionConstraint(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.entries[TypeRule] = map[string]*Entry{
		"my-rule": {ID: "my-rule", Type: TypeRule, Latest: "1.0.0"},
	}
	svc := &HubService{registry: m}

	_, err := svc.Install(context.Background(), "my-rule@!!!invalid", "", "claude", TypeRule, "", "/tmp")
	if err == nil {
		t.Error("expected error for invalid version constraint")
	}
}

func TestHubService_Install_VersionConstraintNoMatch(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.entries[TypeRule] = map[string]*Entry{
		"my-rule": {ID: "my-rule", Type: TypeRule, Latest: "2.0.0", Versions: []string{"2.0.0"}},
	}
	svc := &HubService{registry: m}

	_, err := svc.Install(context.Background(), "my-rule@^99.0.0", "", "claude", TypeRule, "", "/tmp")
	if err == nil {
		t.Error("expected error for no matching version")
	}
}

func TestHubService_Install_WithVersionConstraintAndVersionsList(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.entries[TypeRule] = map[string]*Entry{
		"my-rule": {ID: "my-rule", Type: TypeRule, Latest: "2.0.0", Versions: []string{"1.0.0", "2.0.0"}},
	}
	svc := &HubService{registry: m}

	dir := t.TempDir()
	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	_ = SaveLockfile(filepath.Join(dir, brand.LockFileName()), lf)

	_, err := svc.Install(context.Background(), "my-rule@^1.0.0", "", "claude", TypeRule, "", dir)
	if err == nil {
		t.Log("Install succeeded unexpectedly (no git store)")
	}
}

func TestHubService_Install_ExactVersionNoVersionsList(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
	}
	m.entries[TypeRule] = map[string]*Entry{
		"my-rule": {ID: "my-rule", Type: TypeRule, Latest: "1.0.0"},
	}
	svc := &HubService{registry: m}

	dir := t.TempDir()
	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	_ = SaveLockfile(filepath.Join(dir, brand.LockFileName()), lf)

	_, err := svc.Install(context.Background(), "my-rule@1.0.0", "", "claude", TypeRule, "", dir)
	if err == nil {
		t.Log("Install succeeded unexpectedly")
	}
}

func TestHubService_Log(t *testing.T) {
	t.Parallel()
	svc := &HubService{}
	logger := svc.log()
	if logger == nil {
		t.Error("expected non-nil logger")
	}
}

func TestRegistryManager_LogExtra(t *testing.T) {
	t.Parallel()
	m := &RegistryManager{}
	logger := m.log()
	if logger == nil {
		t.Error("expected non-nil logger")
	}
}

func TestUIServer_Log(t *testing.T) {
	t.Parallel()
	s := &UIServer{}
	logger := s.log()
	if logger == nil {
		t.Error("expected non-nil logger")
	}
}

func TestGlobalLockManager_LogExtra(t *testing.T) {
	t.Parallel()
	m := &GlobalLockManager{}
	logger := m.log()
	if logger == nil {
		t.Error("expected non-nil logger")
	}
}

func TestUIServer_handleInstall_IDWithAtSign(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test@1.0.0","type":"rule","version":"2.0.0","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/install", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleInstall(w, req)

	if w.Header().Get("Content-Type") != "application/json" {
		t.Error("expected JSON response")
	}
}

func TestUIServer_handleUninstall_IDEFallback(t *testing.T) {
	t.Parallel()
	s := newTestUIServer(t)

	body := `{"id":"test","type":"rule","ide":"","project_dir":"/tmp/test"}`
	req := httptest.NewRequest("POST", "/api/uninstall", strings.NewReader(body))
	w := httptest.NewRecorder()
	s.handleUninstall(w, req)

	if w.Header().Get("Content-Type") != "application/json" {
		t.Error("expected JSON response")
	}
}

func TestSyncIDEAdapter_Exported(t *testing.T) {
	dir := t.TempDir()

	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "test-id"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	err := SyncIDEAdapter("claude", dir, lf)
	_ = err
}
