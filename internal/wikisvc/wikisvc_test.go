package wikisvc

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

func TestNewWikiService(t *testing.T) {
	svc := NewWikiService("/some/project")
	if svc == nil {
		t.Fatal("NewWikiService returned nil")
	}
	if svc.projectDir != "/some/project" {
		t.Errorf("projectDir = %q; want %q", svc.projectDir, "/some/project")
	}
}

func TestResolveWikiSource_Project(t *testing.T) {
	tmp := t.TempDir()

	// Create the knowledge/project wiki directory
	wikiDir := filepath.Join(tmp, ".graphit", "knowledge", "project")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	svc := NewWikiService(tmp)
	src, err := svc.ResolveWikiSource("project")
	if err != nil {
		t.Fatalf("ResolveWikiSource(project) error: %v", err)
	}
	if src.ID != "project" {
		t.Errorf("ID = %q; want %q", src.ID, "project")
	}
	if src.Label != filepath.Base(tmp) {
		t.Errorf("Label = %q; want %q", src.Label, filepath.Base(tmp))
	}
}

func TestResolveWikiSource_Memory(t *testing.T) {
	tmp := t.TempDir()

	// Create the memory/project wiki directory
	wikiDir := filepath.Join(tmp, ".graphit", "memory", "project")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	svc := NewWikiService(tmp)
	src, err := svc.ResolveWikiSource("memory")
	if err != nil {
		t.Fatalf("ResolveWikiSource(memory) error: %v", err)
	}
	if src.ID != "memory" {
		t.Errorf("ID = %q; want %q", src.ID, "memory")
	}
	if src.Label != "Memory (project)" {
		t.Errorf("Label = %q; want %q", src.Label, "Memory (project)")
	}
}

func TestResolveWikiSource_ProjectNotFound(t *testing.T) {
	tmp := t.TempDir()
	// Don't create the wiki directory — should fail.
	svc := NewWikiService(tmp)
	_, err := svc.ResolveWikiSource("project")
	if err == nil {
		t.Error("expected error when wiki directory does not exist")
	}
}

func TestResolveWikiSource_MemoryNotFound(t *testing.T) {
	tmp := t.TempDir()
	svc := NewWikiService(tmp)
	_, err := svc.ResolveWikiSource("memory")
	if err == nil {
		t.Error("expected error when memory directory does not exist")
	}
}

func TestResolveLocalSource_FallbackToWikiSubdir(t *testing.T) {
	tmp := t.TempDir()

	// Create dir/wiki subdirectory (but not dir itself as a valid wiki)
	baseDir := filepath.Join(tmp, ".graphit", "knowledge", "project")
	wikiSub := filepath.Join(baseDir, "wiki")
	if err := os.MkdirAll(wikiSub, 0o755); err != nil {
		t.Fatal(err)
	}

	svc := NewWikiService(tmp)
	src, err := svc.ResolveWikiSource("project")
	if err != nil {
		t.Fatalf("ResolveWikiSource(project) error: %v", err)
	}
	// Should have resolved successfully.
	if src.ID != "project" {
		t.Errorf("ID = %q; want %q", src.ID, "project")
	}
}

func TestResolveSources_ValidWikis(t *testing.T) {
	tmp := t.TempDir()
	ctx := context.Background()

	// Create project wiki dir
	wikiDir := filepath.Join(tmp, ".graphit", "knowledge", "project")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	// Create memory wiki dir
	memDir := filepath.Join(tmp, ".graphit", "memory", "project")
	if err := os.MkdirAll(memDir, 0o755); err != nil {
		t.Fatal(err)
	}

	svc := NewWikiService(tmp)

	// Test with valid wikis only (no hub refs, since those need real registries)
	sources, errs := svc.ResolveSources(ctx, []string{"project", "memory"}, nil)
	if len(errs) > 0 {
		t.Errorf("unexpected errors: %v", errs)
	}
	if len(sources) != 2 {
		t.Errorf("expected 2 sources, got %d", len(sources))
	}
}

func TestResolveSources_MixedResults(t *testing.T) {
	tmp := t.TempDir()
	ctx := context.Background()

	// Create only the project wiki dir (nonexistent will fail as ecosystem lookup)
	wikiDir := filepath.Join(tmp, ".graphit", "knowledge", "project")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	svc := NewWikiService(tmp)

	// "project" should resolve, "nonexistent" should fail (ecosystem lookup)
	sources, errs := svc.ResolveSources(ctx, []string{"project", "nonexistent"}, nil)
	if len(sources) != 1 {
		t.Errorf("expected 1 source, got %d", len(sources))
	}
	if len(errs) == 0 {
		t.Error("expected at least 1 error for nonexistent wiki")
	}
}

func TestResolveSources_EmptyInput(t *testing.T) {
	svc := NewWikiService(t.TempDir())
	ctx := context.Background()
	sources, errs := svc.ResolveSources(ctx, nil, nil)
	if len(sources) != 0 {
		t.Errorf("expected 0 sources, got %d", len(sources))
	}
	if len(errs) != 0 {
		t.Errorf("expected 0 errors, got %d", len(errs))
	}
}
