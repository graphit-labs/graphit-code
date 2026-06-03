package wikisvc

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/chat"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// Package-level function hooks for dependency injection in tests.
// Each defaults to the real implementation.
var (
	newAIClientFromConfig = ai.NewClientFromConfig

	newGlobalLockManager = hub.NewGlobalLockManager

	newRegistryManager = func(ctx context.Context) (*hub.RegistryManager, error) {
		return hub.NewRegistryManager(ctx)
	}

	newHubService = func(reg *hub.RegistryManager) interface {
		EnsureKnowledgeAvailable(ctx context.Context, ref string) (string, error)
	} {
		return hub.NewHubService(reg)
	}

	searchMultiWiki = func(ctx context.Context, client ai.Client, query string, cfg wiki.MultiWikiSearchConfig) (*wiki.SearchResult, error) {
		return wiki.SearchMultiWiki(ctx, client, query, cfg)
	}

	newChatSession = chat.NewSession

	loadChatSession = chat.LoadSession

	newChatEngine = func(client ai.Client, session *chat.ChatSession) interface {
		Send(ctx context.Context, message string) (string, error)
	} {
		return chat.NewChatEngine(client, session)
	}

	listChatSessions = chat.ListSessions

	latestChatSession = chat.LatestSession

	deleteChatSession = chat.DeleteSession
)

// WikiSearchOpts is a view-agnostic DTO for multi-wiki search orchestration.
type WikiSearchOpts struct {
	Query   string
	Wikis   []string
	HubRefs []string
	TopK    int
}

// WikiSearchResult is a view-agnostic DTO for search output.
type WikiSearchResult struct {
	Answer    string
	SessionID string
	Turns     int
}

// WikiService orchestrates wiki search, source resolution, and chat sessions
// across multiple domain modules (wiki, chat, hub, ai).
type WikiService struct {
	projectDir string
}

func NewWikiService(projectDir string) *WikiService {
	return &WikiService{projectDir: projectDir}
}

func (s *WikiService) ResolveWikiSource(name string) (wiki.WikiSource, error) {
	switch name {
	case "project":
		return s.resolveLocalSource(name, filepath.Base(s.projectDir),
			filepath.Join(s.projectDir, brand.DotDir(), "knowledge", "project"))

	case "memory":
		return s.resolveLocalSource(name, "Memory (project)",
			filepath.Join(s.projectDir, brand.DotDir(), "memory", "project"))

	default:
		return s.resolveEcosystemSource(name)
	}
}

func (s *WikiService) resolveLocalSource(id, label, dir string) (wiki.WikiSource, error) {
	if _, err := os.Stat(dir); err != nil {
		wikiSub := filepath.Join(dir, "wiki")
		if _, err := os.Stat(wikiSub); err == nil {
			dir = wikiSub
		}
	}
	if _, err := os.Stat(dir); err != nil {
		return wiki.WikiSource{}, fmt.Errorf("%s wiki not found at %s", id, dir)
	}
	return wiki.WikiSource{ID: id, Label: label, Dir: dir}, nil
}

func (s *WikiService) resolveEcosystemSource(projectID string) (wiki.WikiSource, error) {
	lockMgr, err := newGlobalLockManager()
	if err != nil {
		return wiki.WikiSource{}, fmt.Errorf("cannot access global lock: %w", err)
	}

	projects, err := lockMgr.ListActiveProjects()
	if err != nil {
		return wiki.WikiSource{}, fmt.Errorf("cannot list ecosystem projects: %w", err)
	}

	for _, p := range projects {
		if p.ID != projectID {
			continue
		}
		dir := filepath.Join(p.Dir, brand.DotDir(), "knowledge", "project")
		if _, err := os.Stat(dir); err != nil {
			wikiSub := filepath.Join(dir, "wiki")
			if _, err := os.Stat(wikiSub); err == nil {
				dir = wikiSub
			}
		}
		if _, err := os.Stat(dir); err != nil {
			return wiki.WikiSource{}, fmt.Errorf("wiki not found for project %s at %s", projectID, dir)
		}
		return wiki.WikiSource{
			ID:    projectID,
			Label: filepath.Base(p.Dir),
			Dir:   dir,
		}, nil
	}

	return wiki.WikiSource{}, fmt.Errorf("project %q not found in ecosystem — check global.lock.json", projectID)
}

func (s *WikiService) ResolveHubKnowledgeSource(ctx context.Context, ref string) (wiki.WikiSource, error) {
	reg, err := newRegistryManager(ctx)
	if err != nil {
		return wiki.WikiSource{}, fmt.Errorf("hub registry not available: %w", err)
	}

	hubSvc := newHubService(reg)
	wikiDir, err := hubSvc.EnsureKnowledgeAvailable(ctx, ref)
	if err != nil {
		return wiki.WikiSource{}, err
	}

	artifactID := ref
	if parts := strings.SplitN(ref, "@", 2); len(parts) == 2 {
		artifactID = parts[0]
	}

	return wiki.WikiSource{
		ID:    "hub/" + artifactID,
		Label: artifactID,
		Dir:   wikiDir,
	}, nil
}

func (s *WikiService) ResolveSources(ctx context.Context, wikis, hubRefs []string) ([]wiki.WikiSource, []error) {
	var sources []wiki.WikiSource
	var errs []error

	for _, w := range wikis {
		src, err := s.ResolveWikiSource(w)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		sources = append(sources, src)
	}

	for _, ref := range hubRefs {
		src, err := s.ResolveHubKnowledgeSource(ctx, ref)
		if err != nil {
			errs = append(errs, fmt.Errorf("hub knowledge %q: %w", ref, err))
			continue
		}
		sources = append(sources, src)
	}

	return sources, errs
}

func (s *WikiService) SearchMultiWiki(ctx context.Context, opts WikiSearchOpts) (*WikiSearchResult, error) {
	sources, _ := s.ResolveSources(ctx, opts.Wikis, opts.HubRefs)
	if len(sources) == 0 {
		return nil, fmt.Errorf("no valid wiki sources found — specify wikis or hub_refs")
	}

	aiClient, err := newAIClientFromConfig()
	if err != nil {
		return nil, fmt.Errorf("AI not configured: %w", err)
	}

	topK := opts.TopK
	result, err := searchMultiWiki(ctx, aiClient, opts.Query, wiki.MultiWikiSearchConfig{
		Sources:           sources,
		UseBM25:           true,
		BM25TopNPerSource: topK,
	})
	if err != nil {
		return nil, err
	}

	chatSources := make([]chat.WikiSource, len(sources))
	for i, src := range sources {
		chatSources[i] = chat.WikiSource{ID: src.ID, Label: src.Label, Dir: src.Dir}
	}
	session := newChatSession(s.projectDir, chatSources, opts.Query)

	_ = session.Append(chat.ChatMessage{Role: "user", Content: opts.Query})
	_ = session.Append(chat.ChatMessage{Role: "assistant", Content: result.Answer})

	return &WikiSearchResult{
		Answer:    result.Answer,
		SessionID: session.ID,
		Turns:     result.Turns,
	}, nil
}

func (s *WikiService) ContinueChat(ctx context.Context, sessionID, message string) (string, error) {
	session, err := loadChatSession(sessionID)
	if err != nil {
		return "", fmt.Errorf("session not found: %w", err)
	}

	aiClient, err := newAIClientFromConfig()
	if err != nil {
		return "", fmt.Errorf("AI not configured: %w", err)
	}

	engine := newChatEngine(aiClient, session)
	return engine.Send(ctx, message)
}

func (s *WikiService) ListSessions() ([]*chat.ChatSession, error) {
	return listChatSessions(s.projectDir)
}

func (s *WikiService) LatestSession() (*chat.ChatSession, error) {
	return latestChatSession(s.projectDir)
}

func (s *WikiService) DeleteSession(id string) error {
	return deleteChatSession(id)
}
