package memory

import (
	"context"
	"crypto/sha256"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	gitmod "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/oklog/ulid/v2"
)

type MemoryScope string

const (
	MemoryScopeProject MemoryScope = "project"
	MemoryScopeUser    MemoryScope = "user"
	MemoryScopeContext MemoryScope = "context"
)

type MemoryType string

const (
	MemoryTypeConvention MemoryType = "convention"
	MemoryTypeCorrection MemoryType = "correction"
	MemoryTypeDecision   MemoryType = "decision"
	MemoryTypeTension    MemoryType = "tension"
	MemoryTypeFact       MemoryType = "fact"
	MemoryTypeSkill      MemoryType = "skill"
)

func ValidMemoryType(t string) bool {
	switch MemoryType(t) {
	case MemoryTypeConvention, MemoryTypeCorrection, MemoryTypeDecision,
		MemoryTypeTension, MemoryTypeFact, MemoryTypeSkill:
		return true
	}
	return false
}

type MemoryEntry struct {
	ID        string
	Title     string
	CreatedAt string
	Scope     MemoryScope
	ScopeID   string
	Important bool
	Type      MemoryType
	Tags      []string
}

type MemoryService struct {
	scope          MemoryScope
	scopeID        string
	gitStore       *MemoryGitStore
	localDir       string
	projectLinkDir string
	wikiDir        string
}

func NewMemoryService(scope MemoryScope, scopeID string, store *MemoryGitStore) *MemoryService {
	localDir := MemoryLocalDir(string(scope))
	projectLinkDir := MemoryProjectLinkDir(string(scope))
	return newMemorySvcInternal(scope, scopeID, localDir, projectLinkDir, store)
}

func NewMemoryServiceForContext(contextName string, store *MemoryGitStore) *MemoryService {
	localDir := MemoryGlobalContextDir(contextName)
	projectLinkDir := filepath.Join(brand.DotDir(), "memory", contextName)
	return newMemorySvcInternal(MemoryScopeContext, contextName, localDir, projectLinkDir, store)
}

func newMemorySvcInternal(scope MemoryScope, scopeID, localDir, projectLinkDir string, store *MemoryGitStore) *MemoryService {

	wikiDir := MemoryWikiGlobalDir(string(scope), scopeID)
	return &MemoryService{
		scope:          scope,
		scopeID:        scopeID,
		gitStore:       store,
		localDir:       localDir,
		projectLinkDir: projectLinkDir,
		wikiDir:        wikiDir,
	}
}

func (m *MemoryService) Close() error { return nil }

func (m *MemoryService) LocalDir() string { return m.localDir }

func (m *MemoryService) WikiDir() string { return m.wikiDir }

func (m *MemoryService) HubBranch() string {
	switch m.scope {
	case MemoryScopeContext:
		return fmt.Sprintf("memory/project/%s", m.scopeID)
	default:
		return fmt.Sprintf("memory/%s/%s", m.scope, m.scopeID)
	}
}

func (m *MemoryService) EnsureInitialised() error {

	if m.gitStore != nil {
		if err := m.gitStore.EnsureInitialised(); err != nil {
			return fmt.Errorf("initialising memory git repo: %w", err)
		}
	}

	if err := m.syncToLocalFast(); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] init %s/%s: local sync failed: %v\n", m.scope, m.scopeID, err)
	}
	return nil
}

func (m *MemoryService) ensureProjectSymlink(wtDir string) {
	if m.projectLinkDir == "" {
		return
	}
	projectDir := gitProjectDir()
	if projectDir == "" {
		return
	}

	if err := os.MkdirAll(m.wikiDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] symlink: mkdir %s failed: %v\n", m.wikiDir, err)
	}

	linkPath := filepath.Join(projectDir, m.projectLinkDir)
	if err := os.MkdirAll(filepath.Dir(linkPath), 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] symlink: mkdir parent %s failed: %v\n", filepath.Dir(linkPath), err)
	}
	if err := safeMemorySymlink(m.wikiDir, linkPath); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] symlink: %s → %s failed: %v\n", linkPath, m.wikiDir, err)
	}
}

type MemoryOpts struct {
	ProjectID string
	Important bool
	Type      MemoryType
	Tags      []string
}

func (m *MemoryService) AddMemory(title, body string, opts MemoryOpts) (string, error) {
	if m.gitStore == nil {
		return "", fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	id := ulid.Make().String()

	assocProject := ""
	if m.scope == MemoryScopeUser {
		assocProject = opts.ProjectID
	}
	memType := opts.Type
	if memType == "" {
		memType = MemoryTypeFact
	}
	content := buildMemoryFile(id, title, body, string(m.scope), m.scopeID, assocProject, opts.Important, string(memType), opts.Tags)

	branch := m.HubBranch()
	wt, err := m.gitStore.MemoryWorktreeLocal(branch)
	if err != nil {
		return "", fmt.Errorf("open memory branch: %w", err)
	}

	var fileName string
	if opts.Important {
		fileName = ImportantFileName(id)
	} else {
		fileName = NormalFileName(id)
	}
	if err := wt.WriteFile(fileName, []byte(content)); err != nil {
		return "", fmt.Errorf("writing memory file: %w", err)
	}

	msg := fmt.Sprintf("memory: add %s (%s/%s) [%s]", id, m.scope, m.scopeID, memType)
	if opts.Important {
		msg += " [important]"
	}
	if err := wt.CommitAndPush(msg); err != nil {
		return "", fmt.Errorf("committing memory: %w", err)
	}

	if err := m.syncToLocalFast(); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] add %s: post-write sync failed: %v\n", id, err)
	}
	return id, nil
}

func (m *MemoryService) UpdateMemory(id, newTitle, newBody string) error {
	if m.gitStore == nil {
		return fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	branch := m.HubBranch()
	wt, err := m.gitStore.MemoryWorktreeLocal(branch)
	if err != nil {
		return fmt.Errorf("open memory branch: %w", err)
	}

	normalPath := NormalFileName(id)
	importantPath := ImportantFileName(id)
	var relPath string
	var data []byte

	data, err = wt.ReadFile(normalPath)
	if err != nil {
		data, err = wt.ReadFile(importantPath)
		if err != nil {
			return fmt.Errorf("memory %q not found", id)
		}
		relPath = importantPath
	} else {
		relPath = normalPath
	}

	oldContent := string(data)
	title := newTitle
	if title == "" {

		if fm := reFrontmatterTitle.FindStringSubmatch(oldContent); fm != nil {
			title = strings.TrimSpace(fm[1])
		}
	}

	createdAt := ""
	if fm := reFrontmatterCreatedAt.FindStringSubmatch(oldContent); fm != nil {
		createdAt = strings.TrimSpace(fm[1])
	}

	now := time.Now().UTC().Format(time.RFC3339)
	var b strings.Builder
	b.WriteString("---\n")
	b.WriteString(fmt.Sprintf("id: %s\n", id))
	b.WriteString(fmt.Sprintf("title: %s\n", title))
	b.WriteString(fmt.Sprintf("scope: %s\n", m.scope))
	b.WriteString(fmt.Sprintf("scope_id: %s\n", m.scopeID))
	if createdAt != "" {
		b.WriteString(fmt.Sprintf("created_at: %s\n", createdAt))
	}
	b.WriteString(fmt.Sprintf("updated_at: %s\n", now))
	b.WriteString("tags: [memory]\n")
	b.WriteString("---\n\n")
	b.WriteString(fmt.Sprintf("# %s\n\n", title))
	if newBody != "" {
		b.WriteString(newBody)
		if !strings.HasSuffix(newBody, "\n") {
			b.WriteString("\n")
		}
	}

	if err := wt.WriteFile(relPath, []byte(b.String())); err != nil {
		return fmt.Errorf("writing updated memory: %w", err)
	}

	msg := fmt.Sprintf("memory: update %s (%s/%s)", id, m.scope, m.scopeID)
	if err := wt.CommitAndPush(msg); err != nil {
		return fmt.Errorf("committing update: %w", err)
	}

	if err := m.syncToLocalFast(); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] update %s: post-write sync failed: %v\n", id, err)
	}
	return nil
}

func (m *MemoryService) RemoveMemory(id string) error {
	if m.gitStore == nil {
		return fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	branch := m.HubBranch()
	wt, err := m.gitStore.MemoryWorktreeLocal(branch)
	if err != nil {
		return fmt.Errorf("open memory branch: %w", err)
	}

	normalPath := NormalFileName(id)
	importantPath := ImportantFileName(id)

	relPath := normalPath
	if err := wt.RemoveFile(normalPath); err != nil {
		if !os.IsNotExist(err) {
			return fmt.Errorf("removing memory file: %w", err)
		}

		relPath = importantPath
		if err := wt.RemoveFile(importantPath); err != nil {
			if os.IsNotExist(err) {
				return fmt.Errorf("memory %q not found", id)
			}
			return fmt.Errorf("removing memory file: %w", err)
		}
	}
	_ = relPath

	msg := fmt.Sprintf("memory: remove %s (%s/%s)", id, m.scope, m.scopeID)
	if err := wt.CommitAndPush(msg); err != nil {
		return fmt.Errorf("committing removal: %w", err)
	}

	if err := m.syncToLocalFast(); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] remove %s: post-write sync failed: %v\n", id, err)
	}
	return nil
}

func (m *MemoryService) PromoteMemory(id string) error {
	return m.changeRelevance(id, true)
}

func (m *MemoryService) DemoteMemory(id string) error {
	return m.changeRelevance(id, false)
}

func (m *MemoryService) changeRelevance(id string, promote bool) error {
	if m.gitStore == nil {
		return fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	branch := m.HubBranch()
	wt, err := m.gitStore.MemoryWorktreeLocal(branch)
	if err != nil {
		return fmt.Errorf("open memory branch: %w", err)
	}

	var oldName, newName string
	if promote {
		oldName = NormalFileName(id)
		newName = ImportantFileName(id)
	} else {
		oldName = ImportantFileName(id)
		newName = NormalFileName(id)
	}

	oldPath := oldName
	newPath := newName

	data, err := wt.ReadFile(oldPath)
	if err != nil {
		if os.IsNotExist(err) {
			if promote {
				return fmt.Errorf("memory %q not found (or already important)", id)
			}
			return fmt.Errorf("memory %q not found (or not marked important)", id)
		}
		return fmt.Errorf("reading memory file: %w", err)
	}

	if err := wt.WriteFile(newPath, data); err != nil {
		return fmt.Errorf("writing renamed memory file: %w", err)
	}

	if err := wt.RemoveFile(oldPath); err != nil {
		return fmt.Errorf("removing old memory file: %w", err)
	}

	verb := "promote"
	if !promote {
		verb = "demote"
	}
	msg := fmt.Sprintf("memory: %s %s (%s/%s)", verb, id, m.scope, m.scopeID)
	if err := wt.CommitAndPush(msg); err != nil {
		return fmt.Errorf("committing relevance change: %w", err)
	}

	if err := m.syncToLocalFast(); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] %s %s: post-write sync failed: %v\n", verb, id, err)
	}
	return nil
}

func (m *MemoryService) ListMemories() ([]MemoryEntry, error) {
	rawDir := m.localDir
	entries, err := os.ReadDir(rawDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}

	var memories []MemoryEntry
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := e.Name()
		important := IsImportantMemory(name)
		var id string
		if important {
			id = strings.TrimSuffix(name, ImportantMemorySuffix+".md")
		} else {
			id = strings.TrimSuffix(name, ".md")
		}
		title, createdAt := parseMemoryMeta(filepath.Join(rawDir, name))
		memories = append(memories, MemoryEntry{
			ID:        id,
			Title:     title,
			CreatedAt: createdAt,
			Scope:     m.scope,
			ScopeID:   m.scopeID,
			Important: important,
		})
	}
	return memories, nil
}

func (m *MemoryService) SyncToLocal() error {
	return m.syncToLocalInternal(false)
}

func (m *MemoryService) syncToLocalFast() error {
	return m.syncToLocalInternal(true)
}

func (m *MemoryService) syncToLocalInternal(skipNetwork bool) error {
	if m.gitStore == nil {
		return fmt.Errorf("memory repository not configured")
	}

	branch := m.HubBranch()
	var wt *MemoryWorktree
	var err error
	if skipNetwork {
		wt, err = m.gitStore.MemoryWorktreeLocal(branch)
	} else {
		wt, err = m.gitStore.MemoryWorktree(branch)
	}
	if err != nil {
		return fmt.Errorf("opening memory worktree: %w", err)
	}
	wtDir := wt.Dir()

	m.localDir = wtDir

	m.wikiDir = MemoryWikiGlobalDir(string(m.scope), m.scopeID)

	m.ensureProjectSymlink(wtDir)

	if err := os.MkdirAll(m.wikiDir, 0o755); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] sync: mkdir wiki %s failed: %v\n", m.wikiDir, err)
	}

	ctx := context.Background()
	if err := m.IndexMemories(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "[memory] sync %s/%s: wiki indexing failed: %v\n", m.scope, m.scopeID, err)
	}
	return nil
}

func (m *MemoryService) IndexMemories(ctx context.Context) error {
	rawDir := m.localDir
	if _, err := os.Stat(rawDir); os.IsNotExist(err) {
		return nil
	}

	if _, err := GenerateMemoryWiki(ctx, rawDir, m.wikiDir); err != nil {
		return fmt.Errorf("generating memory wiki: %w", err)
	}
	return nil
}

func MemoryLocalDir(scope string) string {
	d := brand.GlobalDir()
	if d == "" {
		return filepath.Join(brand.DotDir(), "memory", scope)
	}
	return filepath.Join(d, "memory", scope)
}

func MemoryProjectLinkDir(scope string) string {
	return filepath.Join(brand.DotDir(), "memory", scope)
}

func MemoryGlobalContextDir(contextName string) string {
	d := brand.GlobalDir()
	if d == "" {
		return filepath.Join(brand.DotDir(), "memory", contextName)
	}
	return filepath.Join(d, "memory", contextName)
}

func MemoryWikiGlobalDir(scope, scopeID string) string {
	d := brand.GlobalDir()
	if d == "" {
		return filepath.Join(brand.DotDir(), "wiki", "memory", scope, scopeID)
	}
	return filepath.Join(d, "wiki", "memory", scope, scopeID)
}

func gitProjectDir() string {
	out, err := gitmod.Default().RunGlobalOutput("rev-parse", "--show-toplevel")
	if err == nil && out != "" {
		return out
	}
	wd, _ := os.Getwd()
	return wd
}

func UserHashFromGit() (string, error) {
	out, err := gitmod.Default().RunGlobalOutput("config", "user.email")
	if err != nil || out == "" {
		return "", fmt.Errorf("could not read git user.email — configure it with: git config --global user.email you@example.com")
	}
	sum := sha256.Sum256([]byte(out))
	return fmt.Sprintf("%x", sum)[:16], nil
}

func buildMemoryFile(id, title, body, scope, scopeID, origProjectID string, important bool, memType string, userTags []string) string {
	now := time.Now().UTC().Format(time.RFC3339)
	var b strings.Builder
	b.WriteString("---\n")
	b.WriteString(fmt.Sprintf("id: %s\n", id))
	b.WriteString(fmt.Sprintf("title: %s\n", title))
	b.WriteString(fmt.Sprintf("scope: %s\n", scope))
	b.WriteString(fmt.Sprintf("scope_id: %s\n", scopeID))
	if origProjectID != "" {
		b.WriteString(fmt.Sprintf("project_id: %s\n", origProjectID))
	}
	if memType != "" {
		b.WriteString(fmt.Sprintf("type: %s\n", memType))
	}
	if important {
		b.WriteString("important: true\n")
	}
	b.WriteString(fmt.Sprintf("created_at: %s\n", now))
	b.WriteString(fmt.Sprintf("updated_at: %s\n", now))

	tagSet := []string{"memory", scope}
	if memType != "" {
		tagSet = append(tagSet, memType)
	}
	tagSet = append(tagSet, userTags...)
	b.WriteString(fmt.Sprintf("tags: [%s]\n", strings.Join(tagSet, ", ")))
	b.WriteString("---\n\n")
	b.WriteString(fmt.Sprintf("# %s\n\n", title))
	if body != "" {
		b.WriteString(body)
		if !strings.HasSuffix(body, "\n") {
			b.WriteString("\n")
		}
	}
	return b.String()
}

var reFrontmatterTitle = regexp.MustCompile(`(?m)^title:\s*(.+)$`)
var reFrontmatterCreatedAt = regexp.MustCompile(`(?m)^created_at:\s*(.+)$`)

func parseMemoryMeta(path string) (title, createdAt string) {
	data, err := os.ReadFile(path)
	if err != nil {
		return filepath.Base(path), ""
	}
	content := string(data)
	if m := reFrontmatterTitle.FindStringSubmatch(content); m != nil {
		title = strings.TrimSpace(m[1])
	} else {
		for _, line := range strings.Split(content, "\n") {
			if strings.HasPrefix(line, "# ") {
				title = strings.TrimPrefix(line, "# ")
				break
			}
		}
	}
	if title == "" {
		title = strings.TrimSuffix(filepath.Base(path), ".md")
	}
	if m := reFrontmatterCreatedAt.FindStringSubmatch(content); m != nil {
		createdAt = strings.TrimSpace(m[1])
	}
	return title, createdAt
}

func ParseMemoryMetaPublic(path string) (title, createdAt string) {
	return parseMemoryMeta(path)
}
