package wikisvc

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

func saveAndRestoreHooks(t *testing.T) {
	t.Helper()

	origNewAIClient := newAIClientFromConfig
	origNewGlobalLock := newGlobalLockManager
	origNewRegistry := newRegistryManager
	origNewHubSvc := newHubService
	origSearchMulti := searchMultiWiki
	origNewChatSession := newChatSession
	origLoadChatSession := loadChatSession
	origNewChatEngine := newChatEngine
	origListSessions := listChatSessions
	origDeleteSession := deleteChatSession

	t.Cleanup(func() {
		newAIClientFromConfig = origNewAIClient
		newGlobalLockManager = origNewGlobalLock
		newRegistryManager = origNewRegistry
		newHubService = origNewHubSvc
		searchMultiWiki = origSearchMulti
		newChatSession = origNewChatSession
		loadChatSession = origLoadChatSession
		newChatEngine = origNewChatEngine
		listChatSessions = origListSessions
		deleteChatSession = origDeleteSession
	})
}

type mockAIClient struct{}

func (m *mockAIClient) Complete(_ context.Context, _, _ string) (string, error) {
	return "mock answer", nil
}

type mockChatEngine struct {
	response string
	err      error
}

func (m *mockChatEngine) Send(_ context.Context, _ string) (string, error) {
	return m.response, m.err
}

type mockHubService struct {
	dir string
	err error
}

func (m *mockHubService) ResolveKnowledgeMount(_ context.Context, _ string) (hub.MountedWiki, error) {
	return hub.MountedWiki{Config: lancestore.Config{URI: m.dir}}, m.err
}

func TestResolveEcosystemSource_LockManagerError(t *testing.T) {
	saveAndRestoreHooks(t)

	newGlobalLockManager = func() (*hub.GlobalLockManager, error) {
		return nil, errors.New("lock unavailable")
	}

	svc := NewWikiService(t.TempDir())
	_, err := svc.ResolveWikiSource("some-eco-project")
	if err == nil {
		t.Fatal("expected error when lock manager fails")
	}
	if got := err.Error(); !contains(got, "cannot access global lock") {
		t.Errorf("error = %q; want to contain 'cannot access global lock'", got)
	}
}

func TestResolveEcosystemSource_ProjectFound_WithWikiDir(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	projDir := filepath.Join(tmp, "eco-project")
	wikiDir := knowledgeWikiDirFor(t, projDir)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	lockFile := filepath.Join(projDir, brand.LockFileName())
	if err := os.WriteFile(lockFile, []byte("{}"), 0o644); err != nil {
		t.Fatal(err)
	}

	globalDir := filepath.Join(tmp, "global")
	if err := os.MkdirAll(globalDir, 0o755); err != nil {
		t.Fatal(err)
	}
	lockPath := filepath.Join(globalDir, hub.GlobalHubLockFile)

	lockJSON := `{
		"version": 2,
		"projects": {
			"eco-project": {
				"instances": [{"dir": "` + jsonEscape(projDir) + `", "registeredAt": "2025-01-01T00:00:00Z"}]
			}
		},
		"artifacts": {}
	}`
	if err := os.WriteFile(lockPath, []byte(lockJSON), 0o644); err != nil {
		t.Fatal(err)
	}

	newGlobalLockManager = func() (*hub.GlobalLockManager, error) {
		return &hub.GlobalLockManager{}, nil
	}

}

func TestResolveEcosystemSource_ProjectNotFound(t *testing.T) {
	saveAndRestoreHooks(t)

}

func TestResolveHubKnowledgeSource_RegistryError(t *testing.T) {
	saveAndRestoreHooks(t)

	newRegistryManager = func(_ context.Context) (*hub.RegistryManager, error) {
		return nil, errors.New("registry failed")
	}

	svc := NewWikiService(t.TempDir())
	_, err := svc.ResolveHubKnowledgeSource(context.Background(), "some-artifact")
	if err == nil {
		t.Fatal("expected error")
	}
	if got := err.Error(); !contains(got, "hub registry not available") {
		t.Errorf("error = %q; want 'hub registry not available'", got)
	}
}

func TestResolveHubKnowledgeSource_EnsureKnowledgeError(t *testing.T) {
	saveAndRestoreHooks(t)

	newRegistryManager = func(_ context.Context) (*hub.RegistryManager, error) {
		return &hub.RegistryManager{}, nil
	}

	newHubService = func(_ *hub.RegistryManager) interface {
		ResolveKnowledgeMount(ctx context.Context, ref string) (hub.MountedWiki, error)
	} {
		return &mockHubService{err: errors.New("knowledge not found")}
	}

	svc := NewWikiService(t.TempDir())
	_, err := svc.ResolveHubKnowledgeSource(context.Background(), "missing-artifact@v1")
	if err == nil {
		t.Fatal("expected error")
	}
	if got := err.Error(); !contains(got, "knowledge not found") {
		t.Errorf("error = %q; want 'knowledge not found'", got)
	}
}

func TestResolveHubKnowledgeSource_Success_WithVersion(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	wikiDir := filepath.Join(tmp, "hub-wiki")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	newRegistryManager = func(_ context.Context) (*hub.RegistryManager, error) {
		return &hub.RegistryManager{}, nil
	}

	newHubService = func(_ *hub.RegistryManager) interface {
		ResolveKnowledgeMount(ctx context.Context, ref string) (hub.MountedWiki, error)
	} {
		return &mockHubService{dir: wikiDir}
	}

	svc := NewWikiService(tmp)
	src, err := svc.ResolveHubKnowledgeSource(context.Background(), "my-artifact@v2.0")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if src.ID != "hub/my-artifact" {
		t.Errorf("ID = %q; want %q", src.ID, "hub/my-artifact")
	}
	if src.Label != "my-artifact" {
		t.Errorf("Label = %q; want %q", src.Label, "my-artifact")
	}
	if src.Dir != wikiDir {
		t.Errorf("Dir = %q; want %q", src.Dir, wikiDir)
	}
	if src.StoreConfig == nil || src.StoreConfig.URI != wikiDir {
		t.Fatalf("StoreConfig = %#v; want mounted URI %q", src.StoreConfig, wikiDir)
	}
}

func TestResolveHubKnowledgeSource_Success_WithoutVersion(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	wikiDir := filepath.Join(tmp, "hub-wiki")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	newRegistryManager = func(_ context.Context) (*hub.RegistryManager, error) {
		return &hub.RegistryManager{}, nil
	}

	newHubService = func(_ *hub.RegistryManager) interface {
		ResolveKnowledgeMount(ctx context.Context, ref string) (hub.MountedWiki, error)
	} {
		return &mockHubService{dir: wikiDir}
	}

	svc := NewWikiService(tmp)
	src, err := svc.ResolveHubKnowledgeSource(context.Background(), "plain-artifact")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if src.ID != "hub/plain-artifact" {
		t.Errorf("ID = %q; want %q", src.ID, "hub/plain-artifact")
	}
}

func TestResolveSources_WithValidHubRef(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	wikiDir := filepath.Join(tmp, "hub-wiki")
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	newRegistryManager = func(_ context.Context) (*hub.RegistryManager, error) {
		return &hub.RegistryManager{}, nil
	}

	newHubService = func(_ *hub.RegistryManager) interface {
		ResolveKnowledgeMount(ctx context.Context, ref string) (hub.MountedWiki, error)
	} {
		return &mockHubService{dir: wikiDir}
	}

	svc := NewWikiService(tmp)
	ctx := context.Background()
	sources, errs := svc.ResolveSources(ctx, nil, []string{"hub-artifact@v1"})
	if len(errs) > 0 {
		t.Errorf("unexpected errors: %v", errs)
	}
	if len(sources) != 1 {
		t.Errorf("expected 1 source, got %d", len(sources))
	}
}

func TestResolveSources_WikiAndHubMixed(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	projWikiDir := knowledgeWikiDirFor(t, tmp)
	if err := os.MkdirAll(projWikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

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

	svc := NewWikiService(tmp)
	ctx := context.Background()
	sources, errs := svc.ResolveSources(ctx, []string{"project"}, []string{"hub-ref"})
	if len(errs) > 0 {
		t.Errorf("unexpected errors: %v", errs)
	}
	if len(sources) != 2 {
		t.Errorf("expected 2 sources, got %d", len(sources))
	}
}

func TestSearchMultiWiki_AIClientError(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	wikiDir := knowledgeWikiDirFor(t, tmp)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	newAIClientFromConfig = func() (ai.Client, error) {
		return nil, errors.New("no AI configured")
	}

	svc := NewWikiService(tmp)
	ctx := context.Background()
	_, err := svc.SearchMultiWiki(ctx, WikiSearchOpts{
		Query: "test query",
		Wikis: []string{"project"},
	})
	if err == nil {
		t.Fatal("expected error when AI client fails")
	}
	if got := err.Error(); !contains(got, "AI not configured") {
		t.Errorf("error = %q; want 'AI not configured'", got)
	}
}

func TestSearchMultiWiki_SearchError(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	wikiDir := knowledgeWikiDirFor(t, tmp)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	newAIClientFromConfig = func() (ai.Client, error) {
		return &mockAIClient{}, nil
	}

	searchMultiWiki = func(_ context.Context, _ ai.Client, _ string, _ wiki.MultiWikiSearchConfig) (*wiki.SearchResult, error) {
		return nil, errors.New("search failed")
	}

	svc := NewWikiService(tmp)
	ctx := context.Background()
	_, err := svc.SearchMultiWiki(ctx, WikiSearchOpts{
		Query: "test query",
		Wikis: []string{"project"},
		TopK:  5,
	})
	if err == nil {
		t.Fatal("expected error when search fails")
	}
	if got := err.Error(); !contains(got, "search failed") {
		t.Errorf("error = %q; want 'search failed'", got)
	}
}

func TestSearchMultiWiki_Success(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	wikiDir := knowledgeWikiDirFor(t, tmp)
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		t.Fatal(err)
	}

	newAIClientFromConfig = func() (ai.Client, error) {
		return &mockAIClient{}, nil
	}

	searchMultiWiki = func(_ context.Context, _ ai.Client, _ string, cfg wiki.MultiWikiSearchConfig) (*wiki.SearchResult, error) {
		return &wiki.SearchResult{
			Answer: "The answer is 42",
			Turns:  3,
		}, nil
	}

	newChatSession = func(projectDir string, sources []chat.Source, query string) *chat.ChatSession {
		return &chat.ChatSession{
			ID:         "test-session-123",
			ProjectDir: projectDir,
			Sources:    sources,
		}
	}

	svc := NewWikiService(tmp)
	ctx := context.Background()
	result, err := svc.SearchMultiWiki(ctx, WikiSearchOpts{
		Query: "meaning of life",
		Wikis: []string{"project"},
		TopK:  10,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer != "The answer is 42" {
		t.Errorf("Answer = %q; want %q", result.Answer, "The answer is 42")
	}
	if result.SessionID != "test-session-123" {
		t.Errorf("SessionID = %q; want %q", result.SessionID, "test-session-123")
	}
	if result.Turns != 3 {
		t.Errorf("Turns = %d; want 3", result.Turns)
	}
}

func TestSearchMultiWiki_NoSources_AllFail(t *testing.T) {
	saveAndRestoreHooks(t)

	svc := NewWikiService(t.TempDir())
	ctx := context.Background()
	_, err := svc.SearchMultiWiki(ctx, WikiSearchOpts{
		Query: "test",
		Wikis: []string{},
	})
	if err == nil {
		t.Fatal("expected error when no sources")
	}
	if got := err.Error(); !contains(got, "no valid wiki sources found") {
		t.Errorf("error = %q; want 'no valid wiki sources found'", got)
	}
}

func TestContinueChat_LoadSessionError(t *testing.T) {
	saveAndRestoreHooks(t)

	loadChatSession = func(_ string) (*chat.ChatSession, error) {
		return nil, errors.New("session not found")
	}

	svc := NewWikiService(t.TempDir())
	_, err := svc.ContinueChat(context.Background(), "bad-id", "hello")
	if err == nil {
		t.Fatal("expected error")
	}
	if got := err.Error(); !contains(got, "session not found") {
		t.Errorf("error = %q; want 'session not found'", got)
	}
}

func TestContinueChat_AIClientError(t *testing.T) {
	saveAndRestoreHooks(t)

	loadChatSession = func(_ string) (*chat.ChatSession, error) {
		return &chat.ChatSession{ID: "test-session"}, nil
	}

	newAIClientFromConfig = func() (ai.Client, error) {
		return nil, errors.New("AI not available")
	}

	svc := NewWikiService(t.TempDir())
	_, err := svc.ContinueChat(context.Background(), "test-session", "hello")
	if err == nil {
		t.Fatal("expected error when AI client fails")
	}
	if got := err.Error(); !contains(got, "AI not configured") {
		t.Errorf("error = %q; want 'AI not configured'", got)
	}
}

func TestContinueChat_EngineError(t *testing.T) {
	saveAndRestoreHooks(t)

	loadChatSession = func(_ string) (*chat.ChatSession, error) {
		return &chat.ChatSession{ID: "test-session"}, nil
	}

	newAIClientFromConfig = func() (ai.Client, error) {
		return &mockAIClient{}, nil
	}

	newChatEngine = func(_ ai.Client, _ *chat.ChatSession) interface {
		Send(ctx context.Context, message string) (string, error)
	} {
		return &mockChatEngine{err: errors.New("engine send failed")}
	}

	svc := NewWikiService(t.TempDir())
	_, err := svc.ContinueChat(context.Background(), "test-session", "hello")
	if err == nil {
		t.Fatal("expected error when engine.Send fails")
	}
	if got := err.Error(); !contains(got, "engine send failed") {
		t.Errorf("error = %q; want 'engine send failed'", got)
	}
}

func TestContinueChat_Success(t *testing.T) {
	saveAndRestoreHooks(t)

	loadChatSession = func(_ string) (*chat.ChatSession, error) {
		return &chat.ChatSession{ID: "test-session"}, nil
	}

	newAIClientFromConfig = func() (ai.Client, error) {
		return &mockAIClient{}, nil
	}

	newChatEngine = func(_ ai.Client, _ *chat.ChatSession) interface {
		Send(ctx context.Context, message string) (string, error)
	} {
		return &mockChatEngine{response: "Hello! How can I help?"}
	}

	svc := NewWikiService(t.TempDir())
	reply, err := svc.ContinueChat(context.Background(), "test-session", "hello")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if reply != "Hello! How can I help?" {
		t.Errorf("reply = %q; want %q", reply, "Hello! How can I help?")
	}
}

func TestListSessions_ViaHook_Success(t *testing.T) {
	saveAndRestoreHooks(t)

	expected := []*chat.ChatSession{
		{ID: "session-1"},
		{ID: "session-2"},
	}

	listChatSessions = func(_ string) ([]*chat.ChatSession, error) {
		return expected, nil
	}

	svc := NewWikiService(t.TempDir())
	sessions, err := svc.ListSessions()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(sessions) != 2 {
		t.Errorf("expected 2 sessions, got %d", len(sessions))
	}
}

func TestListSessions_ViaHook_Error(t *testing.T) {
	saveAndRestoreHooks(t)

	listChatSessions = func(_ string) ([]*chat.ChatSession, error) {
		return nil, errors.New("read error")
	}

	svc := NewWikiService(t.TempDir())
	_, err := svc.ListSessions()
	if err == nil {
		t.Fatal("expected error")
	}
}

func TestDeleteSession_ViaHook_Success(t *testing.T) {
	saveAndRestoreHooks(t)

	deleteChatSession = func(_ string) error {
		return nil
	}

	svc := NewWikiService(t.TempDir())
	err := svc.DeleteSession("session-to-delete")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestDeleteSession_ViaHook_Error(t *testing.T) {
	saveAndRestoreHooks(t)

	deleteChatSession = func(_ string) error {
		return errors.New("delete failed")
	}

	svc := NewWikiService(t.TempDir())
	err := svc.DeleteSession("session-to-delete")
	if err == nil {
		t.Fatal("expected error")
	}
}

func TestResolveLocalSource_NeitherDirNorWikiSubExist(t *testing.T) {
	tmp := t.TempDir()
	svc := NewWikiService(tmp)
	_, err := svc.resolveLocalSource("test", "Test", filepath.Join(tmp, "nonexistent"))
	if err == nil {
		t.Fatal("expected error when neither dir nor wiki/ subdir exist")
	}
}

func TestResolveLocalSource_DirExistsDirectly(t *testing.T) {
	tmp := t.TempDir()
	dir := filepath.Join(tmp, "mydir")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	svc := NewWikiService(tmp)
	src, err := svc.resolveLocalSource("test", "Test Label", dir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if src.ID != "test" {
		t.Errorf("ID = %q; want %q", src.ID, "test")
	}
	if src.Label != "Test Label" {
		t.Errorf("Label = %q; want %q", src.Label, "Test Label")
	}
	if src.Dir != dir {
		t.Errorf("Dir = %q; want %q", src.Dir, dir)
	}
}

func TestSearchMultiWiki_MultipleSources(t *testing.T) {
	saveAndRestoreHooks(t)
	tmp := t.TempDir()

	projDir := knowledgeWikiDirFor(t, tmp)
	if err := os.MkdirAll(projDir, 0o755); err != nil {
		t.Fatal(err)
	}

	memDir := memoryWikiDirFor(t, tmp)
	if err := os.MkdirAll(memDir, 0o755); err != nil {
		t.Fatal(err)
	}

	newAIClientFromConfig = func() (ai.Client, error) {
		return &mockAIClient{}, nil
	}

	searchMultiWiki = func(_ context.Context, _ ai.Client, _ string, cfg wiki.MultiWikiSearchConfig) (*wiki.SearchResult, error) {
		if len(cfg.Sources) != 2 {
			return nil, errors.New("expected 2 sources")
		}
		return &wiki.SearchResult{Answer: "combined answer", Turns: 2}, nil
	}

	newChatSession = func(projectDir string, sources []chat.Source, query string) *chat.ChatSession {
		return &chat.ChatSession{
			ID:         "multi-session",
			ProjectDir: projectDir,
			Sources:    sources,
		}
	}

	svc := NewWikiService(tmp)
	result, err := svc.SearchMultiWiki(context.Background(), WikiSearchOpts{
		Query: "test query",
		Wikis: []string{"project", "memory"},
		TopK:  3,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Answer != "combined answer" {
		t.Errorf("Answer = %q; want %q", result.Answer, "combined answer")
	}
}

func TestWikiSearchOpts_Fields(t *testing.T) {
	opts := WikiSearchOpts{
		Query:   "test",
		Wikis:   []string{"project"},
		HubRefs: []string{"hub-ref@v1"},
		TopK:    10,
	}
	if opts.Query != "test" {
		t.Errorf("Query = %q; want %q", opts.Query, "test")
	}
	if len(opts.Wikis) != 1 {
		t.Errorf("Wikis length = %d; want 1", len(opts.Wikis))
	}
	if len(opts.HubRefs) != 1 {
		t.Errorf("HubRefs length = %d; want 1", len(opts.HubRefs))
	}
	if opts.TopK != 10 {
		t.Errorf("TopK = %d; want 10", opts.TopK)
	}
}

func TestWikiSearchResult_Fields(t *testing.T) {
	result := WikiSearchResult{
		Answer:    "answer",
		SessionID: "sid",
		Turns:     5,
	}
	if result.Answer != "answer" {
		t.Errorf("Answer = %q; want %q", result.Answer, "answer")
	}
	if result.SessionID != "sid" {
		t.Errorf("SessionID = %q; want %q", result.SessionID, "sid")
	}
	if result.Turns != 5 {
		t.Errorf("Turns = %d; want 5", result.Turns)
	}
}

func TestDefaultHook_SearchMultiWiki(t *testing.T) {
	defaultSearchMultiWiki := searchMultiWiki

	_, err := defaultSearchMultiWiki(context.Background(), &mockAIClient{}, "test", wiki.MultiWikiSearchConfig{})
	if err == nil {
		t.Error("expected error from default searchMultiWiki hook")
	}
}

func TestDefaultHook_NewChatEngine(t *testing.T) {
	defaultNewChatEngine := newChatEngine

	engine := defaultNewChatEngine(&mockAIClient{}, &chat.ChatSession{ID: "test"})
	if engine == nil {
		t.Error("expected non-nil engine from default newChatEngine hook")
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && containsSubstr(s, substr)
}

func containsSubstr(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

func jsonEscape(s string) string {
	result := ""
	for _, c := range s {
		switch c {
		case '\\':
			result += "\\\\"
		case '"':
			result += "\\\""
		default:
			result += string(c)
		}
	}
	return result
}
