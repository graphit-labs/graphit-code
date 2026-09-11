package wikisvc

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// TestResolveEcosystemSource_FullIntegration tests the ecosystem source resolution
// using a real GlobalLockManager with a synthetic global.lock.json and temp HOME.
func TestResolveEcosystemSource_FullIntegration(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()
	ecoProjectID := "01ARZ3NDEKTSV4RRFFQ69G5FAV"
	ecoProject2ID := "01BX5ZZKBKACTAV9WEVGEMMVRZ"
	ecoProject3ID := "01C3V8X7PA58Q9RFPXG2K6M1TY"
	otherProjectID := "01D4W9Y8QB69RAGQYH3M7N2VUZ"

	origHome := os.Getenv("HOME")
	t.Setenv("HOME", tmp)
	t.Cleanup(func() {
		os.Setenv("HOME", origHome) //nolint:errcheck
	})

	globalDir := filepath.Join(tmp, "."+brand.Brand)
	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		t.Fatal(err)
	}

	ecoProjectDir := filepath.Join(tmp, "eco-proj")
	if err := os.MkdirAll(ecoProjectDir, 0o755); err != nil {
		t.Fatal(err)
	}

	lockFilePath := filepath.Join(ecoProjectDir, brand.LockFileName())
	if err := os.WriteFile(lockFilePath, []byte(`{"project":{"id":"`+ecoProjectID+`"}}`), 0o644); err != nil {
		t.Fatal(err)
	}

	wikiDir := knowledgeWikiDirFor(t, ecoProjectDir)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	lockData := hub.GlobalHubLock{
		Version: hub.GlobalLockVersion,
		Projects: map[string]*hub.ProjectEntry{
			ecoProjectID: {
				Instances: []hub.InstanceEntry{
					{
						Dir:          ecoProjectDir,
						RegisteredAt: "2025-01-01T00:00:00Z",
					},
				},
			},
		},
		Artifacts: make(map[string]*hub.GlobalArtifact),
	}
	lockJSON, err := json.MarshalIndent(lockData, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	lockPath := filepath.Join(globalDir, hub.GlobalHubLockFile)
	if err := os.WriteFile(lockPath, lockJSON, 0o644); err != nil {
		t.Fatal(err)
	}

	newGlobalLockManager = hub.NewGlobalLockManager

	t.Run("project found with wiki dir", func(t *testing.T) {
		svc := NewWikiService(t.TempDir())
		src, err := svc.resolveEcosystemSource(ecoProjectID)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if src.ID != ecoProjectID {
			t.Errorf("ID = %q; want %q", src.ID, ecoProjectID)
		}
		if src.Label != filepath.Base(ecoProjectDir) {
			t.Errorf("Label = %q; want %q", src.Label, filepath.Base(ecoProjectDir))
		}
		if src.Dir != wikiDir {
			t.Errorf("Dir = %q; want %q", src.Dir, wikiDir)
		}
	})

	t.Run("project found but wiki dir missing", func(t *testing.T) {
		ecoProject2Dir := filepath.Join(tmp, "eco-proj2")
		if err := os.MkdirAll(ecoProject2Dir, 0o755); err != nil {
			t.Fatal(err)
		}
		lockFilePath2 := filepath.Join(ecoProject2Dir, brand.LockFileName())
		if err := os.WriteFile(lockFilePath2, []byte(`{"project":{"id":"`+ecoProject2ID+`"}}`), 0o644); err != nil {
			t.Fatal(err)
		}

		lockData2 := hub.GlobalHubLock{
			Version: hub.GlobalLockVersion,
			Projects: map[string]*hub.ProjectEntry{
				ecoProjectID: {
					Instances: []hub.InstanceEntry{
						{Dir: ecoProjectDir, RegisteredAt: "2025-01-01T00:00:00Z"},
					},
				},
				ecoProject2ID: {
					Instances: []hub.InstanceEntry{
						{Dir: ecoProject2Dir, RegisteredAt: "2025-01-01T00:00:00Z"},
					},
				},
			},
			Artifacts: make(map[string]*hub.GlobalArtifact),
		}
		lockJSON2, err := json.MarshalIndent(lockData2, "", "  ")
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(lockPath, lockJSON2, 0o644); err != nil {
			t.Fatal(err)
		}

		svc := NewWikiService(t.TempDir())
		_, err = svc.resolveEcosystemSource(ecoProject2ID)
		if err == nil {
			t.Fatal("expected error when wiki dir doesn't exist")
		}
		if got := err.Error(); !strings.Contains(got, "wiki not found for project "+ecoProject2ID) {
			t.Errorf("error = %q; want missing wiki for %q", got, ecoProject2ID)
		}
	})

	t.Run("project found with wiki subdir fallback", func(t *testing.T) {

		ecoProject3Dir := filepath.Join(tmp, "eco-proj3")
		if err := os.MkdirAll(ecoProject3Dir, 0o755); err != nil {
			t.Fatal(err)
		}
		lockFilePath3 := filepath.Join(ecoProject3Dir, brand.LockFileName())
		if err := os.WriteFile(lockFilePath3, []byte(`{"project":{"id":"`+ecoProject3ID+`"}}`), 0o644); err != nil {
			t.Fatal(err)
		}

		knowledgeProject := knowledgeWikiDirFor(t, ecoProject3Dir)
		wikiSub := filepath.Join(knowledgeProject, "wiki")
		if err := os.MkdirAll(wikiSub, 0o755); err != nil {
			t.Fatal(err)
		}

		lockData3 := hub.GlobalHubLock{
			Version: hub.GlobalLockVersion,
			Projects: map[string]*hub.ProjectEntry{
				ecoProject3ID: {
					Instances: []hub.InstanceEntry{
						{Dir: ecoProject3Dir, RegisteredAt: "2025-01-01T00:00:00Z"},
					},
				},
			},
			Artifacts: make(map[string]*hub.GlobalArtifact),
		}
		lockJSON3, err := json.MarshalIndent(lockData3, "", "  ")
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(lockPath, lockJSON3, 0o644); err != nil {
			t.Fatal(err)
		}

		svc := NewWikiService(t.TempDir())
		src, err := svc.resolveEcosystemSource(ecoProject3ID)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if src.ID != ecoProject3ID {
			t.Errorf("ID = %q; want %q", src.ID, ecoProject3ID)
		}
	})

	t.Run("project not found in ecosystem", func(t *testing.T) {
		svc := NewWikiService(t.TempDir())
		_, err := svc.resolveEcosystemSource("nonexistent-project")
		if err == nil {
			t.Fatal("expected error for nonexistent project")
		}
		if got := err.Error(); !strings.Contains(got, "not found in ecosystem") {
			t.Errorf("error = %q; want to contain 'not found in ecosystem'", got)
		}
	})

	t.Run("project ID doesn't match (skip)", func(t *testing.T) {
		lockData4 := hub.GlobalHubLock{
			Version: hub.GlobalLockVersion,
			Projects: map[string]*hub.ProjectEntry{
				otherProjectID: {
					Instances: []hub.InstanceEntry{
						{Dir: ecoProjectDir, RegisteredAt: "2025-01-01T00:00:00Z"},
					},
				},
			},
			Artifacts: make(map[string]*hub.GlobalArtifact),
		}
		lockJSON4, err := json.MarshalIndent(lockData4, "", "  ")
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(lockPath, lockJSON4, 0o644); err != nil {
			t.Fatal(err)
		}

		svc := NewWikiService(t.TempDir())
		_, err = svc.resolveEcosystemSource("wanted-proj")
		if err == nil {
			t.Fatal("expected error when project not in lock")
		}
	})
}

// TestResolveEcosystemSource_ListActiveProjectsError tests the path where
// ListActiveProjects returns an error by creating a corrupt lock file.
func TestResolveEcosystemSource_ListActiveProjectsError(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	origHome := os.Getenv("HOME")
	t.Setenv("HOME", tmp)
	t.Cleanup(func() {
		os.Setenv("HOME", origHome) //nolint:errcheck
	})

	globalDir := filepath.Join(tmp, "."+brand.Brand)
	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		t.Fatal(err)
	}

	lockPath := filepath.Join(globalDir, hub.GlobalHubLockFile)
	if err := os.WriteFile(lockPath, []byte("{invalid json"), 0o644); err != nil {
		t.Fatal(err)
	}

	newGlobalLockManager = hub.NewGlobalLockManager

	svc := NewWikiService(t.TempDir())
	_, err := svc.resolveEcosystemSource("some-project")
	if err == nil {
		t.Fatal("expected error when ListActiveProjects fails")
	}
	if got := err.Error(); !strings.Contains(got, "cannot list ecosystem projects") {
		t.Errorf("error = %q; want to contain 'cannot list ecosystem projects'", got)
	}
}

// TestResolveLocalSource_WikiSubdirFallback tests the case where the base dir
// is NOT a valid stat target (e.g., removed after creation) but wiki/ subdir exists.
// In practice this means we need to simulate: stat(dir) fails but stat(dir/wiki) succeeds.
// On a standard filesystem this is impossible with os.MkdirAll since creating
// dir/wiki implicitly creates dir. To trigger this path, we use a permission trick.
func TestResolveLocalSource_WikiSubdirFallback(t *testing.T) {

	tmp := t.TempDir()
	svc := NewWikiService(tmp)
	nonExistentDir := filepath.Join(tmp, "does", "not", "exist")
	_, err := svc.resolveLocalSource("test", "label", nonExistentDir)
	if err == nil {
		t.Fatal("expected error when neither dir nor wiki/ subdir exist")
	}
}

func TestResolveWikiSource_EcosystemViaResolveWikiSource(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()
	projectID := "01ARZ3NDEKTSV4RRFFQ69G5FAV"

	origHome := os.Getenv("HOME")
	t.Setenv("HOME", tmp)
	t.Cleanup(func() {
		os.Setenv("HOME", origHome) //nolint:errcheck
	})

	globalDir := filepath.Join(tmp, "."+brand.Brand)
	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		t.Fatal(err)
	}

	ecoDir := filepath.Join(tmp, "my-eco-project")
	if err := os.MkdirAll(ecoDir, 0o755); err != nil {
		t.Fatal(err)
	}
	lockFile := filepath.Join(ecoDir, brand.LockFileName())
	if err := os.WriteFile(lockFile, []byte(`{"project":{"id":"`+projectID+`"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	wikiDir := knowledgeWikiDirFor(t, ecoDir)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	lockData := hub.GlobalHubLock{
		Version: hub.GlobalLockVersion,
		Projects: map[string]*hub.ProjectEntry{
			projectID: {
				Instances: []hub.InstanceEntry{
					{Dir: ecoDir, RegisteredAt: "2025-01-01T00:00:00Z"},
				},
			},
		},
		Artifacts: make(map[string]*hub.GlobalArtifact),
	}
	lockJSON, err := json.MarshalIndent(lockData, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(globalDir, hub.GlobalHubLockFile), lockJSON, 0o644); err != nil {
		t.Fatal(err)
	}

	newGlobalLockManager = hub.NewGlobalLockManager

	svc := NewWikiService(t.TempDir())
	src, err := svc.ResolveWikiSource(projectID)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if src.ID != projectID {
		t.Errorf("ID = %q; want %q", src.ID, projectID)
	}
}

func TestSearchMultiWiki_WithHubRefs(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	hubDir := filepath.Join(tmp, "hub-wiki")
	if err := os.MkdirAll(hubDir, 0o755); err != nil {
		t.Fatal(err)
	}

	newRegistryManager = func(_ context.Context) (*hub.RegistryManager, error) {
		return &hub.RegistryManager{}, nil
	}
	newHubService = func(_ *hub.RegistryManager) interface {
		ResolveKnowledgeMount(ctx context.Context, ref string) (hub.MountedWiki, error)
	} {
		return &mockHubService{dir: hubDir}
	}
	newAIClientFromConfig = func() (ai.Client, error) {
		return &mockAIClient{}, nil
	}
	searchMultiWiki = func(_ context.Context, _ ai.Client, _ string, _ wiki.MultiWikiSearchConfig) (*wiki.SearchResult, error) {
		return &wiki.SearchResult{Answer: "hub answer", Turns: 1}, nil
	}
	newChatSession = func(projectDir string, sources []chat.Source, query string) *chat.ChatSession {
		return &chat.ChatSession{
			ID:         "hub-session",
			ProjectDir: projectDir,
		}
	}

	svc := NewWikiService(tmp)
	result, err := svc.SearchMultiWiki(context.Background(), WikiSearchOpts{
		Query:   "test",
		HubRefs: []string{"some-hub@v1"},
		TopK:    5,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.SessionID != "hub-session" {
		t.Errorf("SessionID = %q; want %q", result.SessionID, "hub-session")
	}
}
