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

	wikiDir := knowledgeWikiDirFor(t, tmp)
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

	wikiDir := memoryWikiDirFor(t, tmp)
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

	baseDir := knowledgeWikiDirFor(t, tmp)
	wikiSub := filepath.Join(baseDir, "wiki")
	if err := os.MkdirAll(wikiSub, 0o755); err != nil {
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
}

func TestResolveSources_ValidWikis(t *testing.T) {
	tmp := t.TempDir()
	ctx := context.Background()

	wikiDir := knowledgeWikiDirFor(t, tmp)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	memDir := memoryWikiDirFor(t, tmp)
	if err := os.MkdirAll(memDir, 0o755); err != nil {
		t.Fatal(err)
	}

	svc := NewWikiService(tmp)

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

	wikiDir := knowledgeWikiDirFor(t, tmp)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	svc := NewWikiService(tmp)

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

func TestListSessions_Empty(t *testing.T) {
	svc := NewWikiService(t.TempDir())
	sessions, err := svc.ListSessions()
	if err != nil {
		t.Fatalf("ListSessions error: %v", err)
	}
	if len(sessions) != 0 {
		t.Errorf("expected 0 sessions, got %d", len(sessions))
	}
}

func TestDeleteSession_NotFound(t *testing.T) {
	svc := NewWikiService(t.TempDir())
	err := svc.DeleteSession("nonexistent-session-id")
	if err == nil {
		t.Error("expected error when session not found")
	}
}

func TestContinueChat_SessionNotFound(t *testing.T) {
	svc := NewWikiService(t.TempDir())
	ctx := context.Background()
	_, err := svc.ContinueChat(ctx, "nonexistent-session", "hello")
	if err == nil {
		t.Error("expected error when session not found")
	}
}

func TestSearchMultiWiki_NoSources(t *testing.T) {
	svc := NewWikiService(t.TempDir())
	ctx := context.Background()
	_, err := svc.SearchMultiWiki(ctx, WikiSearchOpts{
		Query: "test",
		Wikis: []string{"nonexistent"},
	})
	if err == nil {
		t.Error("expected error when no valid sources")
	}
}

func TestResolveHubKnowledgeSource_Error(t *testing.T) {
	svc := NewWikiService(t.TempDir())
	ctx := context.Background()
	_, err := svc.ResolveHubKnowledgeSource(ctx, "nonexistent-artifact@v1")
	if err == nil {
		t.Error("expected error when hub registry not available")
	}
}

func TestResolveSources_WithHubRef(t *testing.T) {
	svc := NewWikiService(t.TempDir())
	ctx := context.Background()
	sources, errs := svc.ResolveSources(ctx, nil, []string{"some-hub-ref@v1"})
	if len(sources) != 0 {
		t.Errorf("expected 0 sources from invalid hub ref, got %d", len(sources))
	}
	if len(errs) == 0 {
		t.Error("expected errors for invalid hub ref")
	}
}

func TestResolveWikiSource_Ecosystem_NotInLock(t *testing.T) {
	svc := NewWikiService(t.TempDir())
	_, err := svc.ResolveWikiSource("custom-project")
	if err == nil {
		t.Error("expected error for ecosystem project not in lock file")
	}
}
