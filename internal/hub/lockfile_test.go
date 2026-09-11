package hub

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestLoadLockfile(t *testing.T) {
	t.Parallel()

	t.Run("file not found", func(t *testing.T) {
		t.Parallel()
		lf, err := LoadLockfile("/nonexistent/path/lockfile.json")
		if err != nil {
			t.Errorf("expected nil error for not found, got: %v", err)
		}
		if lf != nil {
			t.Error("expected nil lockfile")
		}
	})

	t.Run("valid lockfile", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		data := `{"project":{"id":"test-id","name":"Test"},"artifacts":{"rule":{"my-rule":{"version":"1.0.0"}}}}`
		_ = os.WriteFile(path, []byte(data), 0o644)

		lf, err := LoadLockfile(path)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if lf.Project.ID != "test-id" {
			t.Errorf("expected project ID 'test-id', got %q", lf.Project.ID)
		}
		if lf.Artifacts[TypeRule] == nil {
			t.Error("expected rule artifacts")
		}
	})

	t.Run("agent field has no legacy alias", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		data := `{"project":{"id":"test-id"},"agents":["codex"],"ides":["cursor"],"artifacts":{}}`
		if err := os.WriteFile(path, []byte(data), 0o644); err != nil {
			t.Fatal(err)
		}

		lf, err := LoadLockfile(path)
		if err != nil {
			t.Fatal(err)
		}
		if len(lf.Agents) != 1 || lf.Agents[0] != "codex" {
			t.Fatalf("Agents = %v, want only the agent field value", lf.Agents)
		}

		encoded, err := json.Marshal(lf)
		if err != nil {
			t.Fatal(err)
		}
		if bytes.Contains(encoded, []byte(`"ides"`)) {
			t.Fatalf("lockfile still serializes the removed ides field: %s", encoded)
		}
	})

	t.Run("invalid json", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		_ = os.WriteFile(path, []byte("not json"), 0o644)

		_, err := LoadLockfile(path)
		if err == nil {
			t.Error("expected error for invalid JSON")
		}
	})

	t.Run("null artifacts", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		data := `{"project":{"id":"test"}}`
		_ = os.WriteFile(path, []byte(data), 0o644)

		lf, err := LoadLockfile(path)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if lf.Artifacts == nil {
			t.Error("expected non-nil artifacts map")
		}
	})

	t.Run("permission error", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		_ = os.WriteFile(path, []byte(`{}`), 0o000)
		defer func() { _ = os.Chmod(path, 0o644) }()

		_, err := LoadLockfile(path)
		if err == nil {
			t.Error("expected error for permission denied")
		}
	})
}

func TestSaveLockfile(t *testing.T) {
	t.Parallel()

	t.Run("basic save", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		err := SaveLockfile(path, lf)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, err := LoadLockfile(path)
		if err != nil {
			t.Fatalf("unexpected error on load: %v", err)
		}
		if lf2.Project.ID != "test-id" {
			t.Errorf("expected test-id, got %q", lf2.Project.ID)
		}
	})

	t.Run("creates parent dirs", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "subdir", "nested", "lock.json")
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		err := SaveLockfile(path, lf)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})

	t.Run("sets project identity when empty", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		lf := &Lockfile{
			Project:   ProjectIdentity{},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		err := SaveLockfile(path, lf)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(path)
		if lf2.Project.ID == "" {
			t.Error("expected project ID to be set")
		}
	})

	t.Run("canonicalizes existing name", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		lf := &Lockfile{
			Project:   ProjectIdentity{Name: "Preserved Name"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		err := SaveLockfile(path, lf)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(path)
		if lf2.Project.Name != "preserved-name" {
			t.Errorf("expected 'preserved-name', got %q", lf2.Project.Name)
		}
	})

	t.Run("preserves existing description", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		lf := &Lockfile{
			Project:   ProjectIdentity{Description: "My Description"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		err := SaveLockfile(path, lf)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(path)
		if lf2.Project.Description != "My Description" {
			t.Errorf("expected 'My Description', got %q", lf2.Project.Description)
		}
	})

	t.Run("nil artifacts initialized", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, "lock.json")
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: nil,
		}
		err := SaveLockfile(path, lf)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		lf2, _ := LoadLockfile(path)
		if lf2.Artifacts == nil {
			t.Error("expected non-nil artifacts after save")
		}
	})
}

func TestAddAgent(t *testing.T) {
	t.Parallel()

	t.Run("lockfile not found", func(t *testing.T) {
		t.Parallel()
		_, err := AddAgent("/nonexistent/lock.json", "claude")
		if err == nil {
			t.Error("expected error for missing lockfile")
		}
	})

	t.Run("add new Agent", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, brand.LockFileName())
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		_ = SaveLockfile(path, lf)

		agents, err := AddAgent(path, "claude")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(agents) != 1 || agents[0] != "claude" {
			t.Errorf("expected [claude], got %v", agents)
		}
	})

	t.Run("duplicate Agent", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, brand.LockFileName())
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Agents:    []string{"claude"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		_ = SaveLockfile(path, lf)

		agents, err := AddAgent(path, "Claude")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(agents) != 1 {
			t.Errorf("expected 1 Agent (no duplicate), got %d", len(agents))
		}
	})

	t.Run("add second Agent", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, brand.LockFileName())
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Agents:    []string{"claude"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		_ = SaveLockfile(path, lf)

		agents, err := AddAgent(path, "vscode")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(agents) != 2 {
			t.Errorf("expected 2 Agents, got %d", len(agents))
		}
	})
}

func TestRemoveAgent(t *testing.T) {
	t.Parallel()

	t.Run("lockfile not found", func(t *testing.T) {
		t.Parallel()
		agents, err := RemoveAgent("/nonexistent/lock.json", "claude")
		if err != nil {
			t.Error("expected nil error for nil lockfile")
		}
		if agents != nil {
			t.Error("expected nil agents")
		}
	})

	t.Run("remove existing Agent", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, brand.LockFileName())
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Agents:    []string{"claude", "vscode"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		_ = SaveLockfile(path, lf)

		agents, err := RemoveAgent(path, "claude")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(agents) != 1 || agents[0] != "vscode" {
			t.Errorf("expected [vscode], got %v", agents)
		}
	})

	t.Run("remove nonexistent Agent", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		path := filepath.Join(dir, brand.LockFileName())
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id", Name: "Test"},
			Agents:    []string{"claude"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}
		_ = SaveLockfile(path, lf)

		agents, err := RemoveAgent(path, "vscode")
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if len(agents) != 1 || agents[0] != "claude" {
			t.Errorf("expected [claude], got %v", agents)
		}
	})
}

func TestValidateArtifactID(t *testing.T) {
	t.Parallel()
	tests := []struct {
		id      string
		wantErr bool
	}{
		{"", true},
		{"valid-name", false},
		{"valid_name.v2", false},
		{"org/repo", false},
		{"org@version", false},
		{"../evil", true},
		{"/absolute", true},
		{"//double", true},
		{"has spaces", true},
		{"has!special", true},
		{"valid-name-123", false},
	}
	for _, tc := range tests {
		t.Run(tc.id, func(t *testing.T) {
			t.Parallel()
			err := ValidateArtifactID(tc.id)
			if (err != nil) != tc.wantErr {
				t.Errorf("ValidateArtifactID(%q) = %v, wantErr = %v", tc.id, err, tc.wantErr)
			}
		})
	}
}

func TestLockfileArtifactMeta_IsHubInstalled(t *testing.T) {
	t.Parallel()
	tests := []struct {
		name string
		meta LockfileArtifactMeta
		want bool
	}{
		{"publish origin", LockfileArtifactMeta{Origin: "publish"}, false},
		{"has RemoteID", LockfileArtifactMeta{RemoteID: "some-id"}, true},
		{"hub origin", LockfileArtifactMeta{Origin: "hub"}, true},
		{"managed origin", LockfileArtifactMeta{Origin: "managed"}, true},
		{"link origin", LockfileArtifactMeta{Origin: "link"}, false},
		{"local origin", LockfileArtifactMeta{Origin: "local"}, false},
		{"empty", LockfileArtifactMeta{}, false},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			got := tc.meta.IsHubInstalled()
			if got != tc.want {
				t.Errorf("IsHubInstalled() = %v, want %v", got, tc.want)
			}
		})
	}
}

func TestLockfile_JSON_RoundTrip(t *testing.T) {
	t.Parallel()
	lf := &Lockfile{
		Project: ProjectIdentity{ID: "test-id", Name: "Test", Description: "Desc"},
		Agents:  []string{"claude", "vscode"},
		Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
			TypeRule: {
				"r1": {Version: "1.0.0", RemoteID: "r1", Origin: "hub"},
			},
		},
		Config: map[string]any{"key": "value"},
	}

	data, err := json.Marshal(lf)
	if err != nil {
		t.Fatal(err)
	}

	var lf2 Lockfile
	if err := json.Unmarshal(data, &lf2); err != nil {
		t.Fatal(err)
	}

	if lf2.Project.ID != lf.Project.ID {
		t.Error("project ID mismatch")
	}
	if len(lf2.Agents) != 2 {
		t.Error("Agents mismatch")
	}
}
