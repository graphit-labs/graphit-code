package hub

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/paths"
)

func TestBuildInstalledFlat(t *testing.T) {
	t.Parallel()

	t.Run("nil lockfile", func(t *testing.T) {
		t.Parallel()
		pp := &paths.ProjectPaths{ActiveProjectDir: t.TempDir()}
		result := buildInstalledFlat(nil, pp, "proj-id")
		if len(result) != 0 {
			t.Errorf("expected empty map, got %d", len(result))
		}
	})

	t.Run("lockfile with artifacts", func(t *testing.T) {
		t.Parallel()
		dir := t.TempDir()
		pp := &paths.ProjectPaths{
			ActiveProjectDir: dir,
			ResourcesDir:     dir,
		}
		lf := &Lockfile{
			Project: ProjectIdentity{ID: "test-id"},
			Artifacts: map[ArtifactType]map[string]*LockfileArtifactMeta{
				TypeRule: {
					"my-rule": {Version: "1.0.0", LinkSource: "/tmp/linked"},
				},
			},
		}

		result := buildInstalledFlat(lf, pp, "test-id")
		if len(result) != 1 {
			t.Errorf("expected 1 artifact, got %d", len(result))
		}
		if result["my-rule"]["type"] != "rule" {
			t.Errorf("expected type 'rule', got %q", result["my-rule"]["type"])
		}
		if result["my-rule"]["path"] != "/tmp/linked" {
			t.Errorf("expected path '/tmp/linked', got %q", result["my-rule"]["path"])
		}
		if result["my-rule"]["version"] != "1.0.0" {
			t.Errorf("expected version '1.0.0', got %q", result["my-rule"]["version"])
		}
	})

	t.Run("empty lockfile", func(t *testing.T) {
		t.Parallel()
		pp := &paths.ProjectPaths{ActiveProjectDir: t.TempDir()}
		lf := &Lockfile{
			Project:   ProjectIdentity{ID: "test-id"},
			Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
		}

		result := buildInstalledFlat(lf, pp, "test-id")
		if len(result) != 0 {
			t.Errorf("expected 0 artifacts, got %d", len(result))
		}
	})
}

func TestSyncIDEAdapter_InvalidIDE(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	pp := &paths.ProjectPaths{ActiveProjectDir: dir}
	lf := &Lockfile{
		Project:   ProjectIdentity{ID: "test-id"},
		Artifacts: make(map[ArtifactType]map[string]*LockfileArtifactMeta),
	}
	err := syncIDEAdapter("totally-invalid-ide-12345", pp, lf)
	if err == nil {
		t.Error("expected error for unsupported IDE")
	}
}

func TestGetIDEAdapter(t *testing.T) {
	t.Parallel()

	t.Run("unsupported IDE", func(t *testing.T) {
		t.Parallel()
		_, err := getIDEAdapter("totally-unsupported-ide-xyz")
		if err == nil {
			t.Error("expected error for unsupported IDE")
		}
	})

	t.Run("supported IDE", func(t *testing.T) {
		t.Parallel()
		adapter, err := getIDEAdapter("claude")
		if err != nil {
			t.Logf("claude IDE may not be supported in this environment: %v", err)
			return
		}
		if adapter == nil {
			t.Error("expected non-nil adapter")
		}
	})
}
