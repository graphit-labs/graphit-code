package hub

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestReconcileEntry(t *testing.T) {
	t.Parallel()

	t.Run("new entry", func(t *testing.T) {
		t.Parallel()
		lf := &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
		entry := &Entry{ID: "test-rule", Latest: "1.0.0"}
		dirty := reconcileEntry(lf, TypeRule, entry)
		if !dirty {
			t.Error("expected dirty=true for new entry")
		}
		meta := lf.Artifacts[TypeRule]["test-rule"]
		if meta == nil {
			t.Fatal("expected artifact to be created")
		}
		if meta.Origin != "managed" {
			t.Errorf("Origin = %q, want %q", meta.Origin, "managed")
		}
		if meta.Version != "1.0.0" {
			t.Errorf("Version = %q, want %q", meta.Version, "1.0.0")
		}
	})

	t.Run("existing with empty origin", func(t *testing.T) {
		t.Parallel()
		lf := &Lockfile{
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {"test-rule": {Version: "1.0.0", Origin: ""}},
			},
		}
		entry := &Entry{ID: "test-rule", Latest: "2.0.0"}
		dirty := reconcileEntry(lf, TypeRule, entry)
		if !dirty {
			t.Error("expected dirty=true for empty origin")
		}
		if lf.Artifacts[TypeRule]["test-rule"].Origin != "managed" {
			t.Error("expected origin to be set to managed")
		}
	})

	t.Run("existing with origin set", func(t *testing.T) {
		t.Parallel()
		lf := &Lockfile{
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {"test-rule": {Version: "1.0.0", Origin: "hub"}},
			},
		}
		entry := &Entry{ID: "test-rule", Latest: "2.0.0"}
		dirty := reconcileEntry(lf, TypeRule, entry)
		if dirty {
			t.Error("expected dirty=false when origin already set")
		}
	})
}

func TestArtifactExistsForIDE(t *testing.T) {
	t.Parallel()

	t.Run("IDE-independent type always exists", func(t *testing.T) {
		t.Parallel()
		lf := &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
		entry := &Entry{ID: "test"}
		result := artifactExistsForIDE(lf, TypeAST, entry, "claude", "/tmp")
		if !result {
			t.Error("expected true for IDE-independent type")
		}
	})

	t.Run("exists in lockfile", func(t *testing.T) {
		t.Parallel()
		lf := &Lockfile{
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {"test-rule": {Version: "1.0.0"}},
			},
		}
		entry := &Entry{ID: "test-rule"}
		result := artifactExistsForIDE(lf, TypeRule, entry, "claude", "/tmp")
		if !result {
			t.Error("expected true when exists in lockfile")
		}
	})

	t.Run("exists on disk", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		artDir := filepath.Join(dir, ".claude", "rules", "test-rule")
		if err := os.MkdirAll(artDir, 0o755); err != nil {
			t.Fatal(err)
		}

		lf := &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
		entry := &Entry{ID: "test-rule"}
		result := artifactExistsForIDE(lf, TypeRule, entry, "claude", dir)
		if !result {
			t.Error("expected true when exists on disk")
		}
	})

	t.Run("not found", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		lf := &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
		entry := &Entry{ID: "nonexistent"}
		result := artifactExistsForIDE(lf, TypeRule, entry, "claude", dir)
		if result {
			t.Error("expected false when not found")
		}
	})

	t.Run("unknown IDE", func(t *testing.T) {
		t.Parallel()
		lf := &Lockfile{Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta)}
		entry := &Entry{ID: "test"}
		result := artifactExistsForIDE(lf, TypeRule, entry, "unknown-ide", "/tmp")
		if result {
			t.Error("expected false for unknown IDE")
		}
	})
}

func TestIdeRootDir(t *testing.T) {
	t.Parallel()
	tests := []struct {
		ide  string
		want string
	}{
		{"antigravity", ".agents"},
		{"cursor", ".cursor"},
		{"claude", ".claude"},
		{"kiro", ".kiro"},
		{"codex", ".codex"},
		{"opencode", ".opencode"},
		{"gemini", ".gemini"},
		{"unknown", ""},
	}

	for _, tt := range tests {
		t.Run(tt.ide, func(t *testing.T) {
			t.Parallel()
			got := ideRootDir(tt.ide)
			if got != tt.want {
				t.Errorf("ideRootDir(%q) = %q, want %q", tt.ide, got, tt.want)
			}
		})
	}
}

func TestIdeTypeDir(t *testing.T) {
	t.Parallel()
	tests := []struct {
		artType ArtifactType
		want    string
	}{
		{TypeRule, "rules"},
		{TypeSkill, "skills"},
		{TypeAgent, "agents"},
		{TypeCommand, "commands"},
		{TypeWorkflow, "workflows"},
		{TypeKnowledge, ""},
		{"unknown", ""},
	}

	for _, tt := range tests {
		t.Run(string(tt.artType), func(t *testing.T) {
			t.Parallel()
			got := ideTypeDir(tt.artType)
			if got != tt.want {
				t.Errorf("ideTypeDir(%q) = %q, want %q", tt.artType, got, tt.want)
			}
		})
	}
}

func TestReconcileManagedArtifacts(t *testing.T) {
	t.Parallel()

	t.Run("nil registry", func(t *testing.T) {
		t.Parallel()
		err := ReconcileManagedArtifacts(nil, "/nonexistent")
		if err != nil {
			t.Fatalf("expected nil error, got: %v", err)
		}
	})

	t.Run("no lockfile", func(t *testing.T) {
		t.Parallel()
		// Create a fake not-ready registry
		reg := &RegistryManager{}
		err := ReconcileManagedArtifacts(reg, "/nonexistent/lock.json")
		if err != nil {
			t.Fatalf("expected nil error, got: %v", err)
		}
	})
}

func TestReconcileManagedArtifactsFromDir(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	lockPath := filepath.Join(dir, brand.LockFileName())
	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "test-id", Name: "test"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	_ = SaveLockfile(lockPath, lf)

	// nil registry is fine
	err := ReconcileManagedArtifactsFromDir(nil, dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}
