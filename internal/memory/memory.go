package memory

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/wiki"
	"github.com/oklog/ulid/v2"
	"gopkg.in/yaml.v3"
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
	Logger   *slog.Logger
	scope    MemoryScope
	scopeID  string
	store    *MemoryStore
	localDir string
	wikiDir  string
}

func (m *MemoryService) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

func NewMemoryService(scope MemoryScope, scopeID string, store *MemoryStore) *MemoryService {
	return newMemorySvcInternal(scope, scopeID, RawDirFor(string(scope), scopeID), store)
}

func NewMemoryServiceForContext(contextName string, store *MemoryStore) *MemoryService {
	return newMemorySvcInternal(MemoryScopeContext, contextName,
		RawDirFor(contextName, contextName), store)
}

func newMemorySvcInternal(scope MemoryScope, scopeID, localDir string, store *MemoryStore) *MemoryService {
	svc := &MemoryService{
		scope:    scope,
		scopeID:  scopeID,
		store:    store,
		localDir: localDir,
		wikiDir:  MemoryWikiGlobalDir(string(scope), scopeID),
	}
	if store != nil {
		store.Logger = svc.Logger
	}
	return svc
}

func (m *MemoryService) Close() error { return nil }

func (m *MemoryService) LocalDir() string { return m.localDir }

func (m *MemoryService) WikiDir() string { return m.wikiDir }

func (m *MemoryService) ScopePrefix() string {
	switch m.scope {
	case MemoryScopeContext:
		return fmt.Sprintf("memory/project/%s", m.scopeID)
	default:
		return fmt.Sprintf("memory/%s/%s", m.scope, m.scopeID)
	}
}

func (m *MemoryService) EnsureInitialised() error {
	if m.store != nil {
		if err := m.store.EnsureInitialised(); err != nil {
			return fmt.Errorf("initialising memory store: %w", err)
		}
	}

	if err := m.syncToLocalFast(); err != nil {
		m.log().Warn("init local sync failed", "scope", m.scope, "scopeID", m.scopeID, "error", err)
	}
	return nil
}

// ensureWikiDir makes sure the scope's wiki directory exists, so a reader that opens
// it before anything has been compiled finds an empty wiki rather than a missing
// path.
//
// This replaces ensureProjectCopy, which pushed the compiled wiki into a replica
// inside whichever project the working directory happened to name — one project, on a
// best guess, with the rest left to a daemon fan-out. There is one wiki now and it is
// read where it is written.
func (m *MemoryService) ensureWikiDir() {
	if m.wikiDir == "" {
		return
	}
	if err := os.MkdirAll(m.wikiDir, 0o755); err != nil {
		m.log().Warn("memory wiki: mkdir failed", "dir", m.wikiDir, "error", err)
	}
}

type MemoryOpts struct {
	ProjectID string
	Important bool
	Type      MemoryType
	Tags      []string
}

func (m *MemoryService) AddMemory(title, body string, opts MemoryOpts) (string, error) {
	if m.store == nil {
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

	scopePath := m.ScopePrefix()
	scope, err := m.store.OpenScopeLocal(scopePath)
	if err != nil {
		return "", fmt.Errorf("opening the memory scope: %w", err)
	}

	if err := scope.WriteFile(MemoryFileName(id), []byte(content)); err != nil {
		return "", fmt.Errorf("writing memory file: %w", err)
	}

	msg := fmt.Sprintf("memory: add %s (%s/%s) [%s]", id, m.scope, m.scopeID, memType)
	if opts.Important {
		msg += " [important]"
	}
	if err := scope.Publish(msg); err != nil {
		return "", fmt.Errorf("committing memory: %w", err)
	}

	if err := m.syncToLocalFast(); err != nil {
		m.log().Warn("post-write sync failed", "op", "add", "id", id, "error", err)
	}
	return id, nil
}

// UpdateMemory rewrites a memory's title and/or body, preserving every
// classification field. Pass an empty string to leave title or body unchanged.
func (m *MemoryService) UpdateMemory(id, newTitle, newBody string) error {
	return m.updateMemory(id, newTitle, newBody, "")
}

// UpdateMemoryTyped is UpdateMemory with an explicit type. It exists for
// consolidation: when several memories fold into one, the survivor must end up
// with the most specific type in the group, and that can differ from the type it
// had. An empty memType leaves the existing type alone.
func (m *MemoryService) UpdateMemoryTyped(id, newTitle, newBody, memType string) error {
	return m.updateMemory(id, newTitle, newBody, memType)
}

func (m *MemoryService) updateMemory(id, newTitle, newBody, memType string) error {
	if m.store == nil {
		return fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	scopePath := m.ScopePrefix()
	scope, err := m.store.OpenScopeLocal(scopePath)
	if err != nil {
		return fmt.Errorf("opening the memory scope: %w", err)
	}

	relPath := MemoryFileName(id)
	data, err := scope.ReadFile(relPath)
	if err != nil {
		return fmt.Errorf("memory %q not found", id)
	}

	// Archive the version being replaced BEFORE overwriting it, and point the new one at the
	// archive. This is the history the git repository used to carry: object storage has no commit
	// to hang it on, so the chain lives in the memory itself.
	archived := HistoryPath(id, currentRevision(string(data)))
	if err := scope.WriteFile(archived, data); err != nil {
		return fmt.Errorf("archiving the previous revision: %w", err)
	}

	content := updatedMemoryContent(string(data), memoryUpdate{
		ID:       id,
		Scope:    string(m.scope),
		ScopeID:  m.scopeID,
		NewTitle: newTitle,
		NewBody:  newBody,
		NewType:  memType,
		Previous: archived,
	})

	if err := scope.WriteFile(relPath, []byte(content)); err != nil {
		return fmt.Errorf("writing updated memory: %w", err)
	}

	msg := fmt.Sprintf("memory: update %s (%s/%s)", id, m.scope, m.scopeID)
	if err := scope.Publish(msg); err != nil {
		return fmt.Errorf("committing update: %w", err)
	}

	if err := m.syncToLocalFast(); err != nil {
		m.log().Warn("post-write sync failed", "op", "update", "id", id, "error", err)
	}
	return nil
}

func (m *MemoryService) RemoveMemory(id string) error {
	if m.store == nil {
		return fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	scopePath := m.ScopePrefix()
	scope, err := m.store.OpenScopeLocal(scopePath)
	if err != nil {
		return fmt.Errorf("opening the memory scope: %w", err)
	}

	relPath := MemoryFileName(id)

	// Archive the version being deleted, so the trail survives the deletion — which is what the
	// git repository did, where a removed file stayed reachable in history.
	//
	// Nothing points AT the archive afterwards: the memory that would have carried the `previous`
	// field is the one being removed. So a deleted memory's chain is found by its id under
	// `history/<id>/`, not by following a pointer.
	m.archiveBeforeDelete(scope, id, relPath)

	if err := scope.RemoveFile(relPath); err != nil {
		if os.IsNotExist(err) {
			return fmt.Errorf("memory %q not found", id)
		}
		return fmt.Errorf("removing memory file: %w", err)
	}

	msg := fmt.Sprintf("memory: remove %s (%s/%s)", id, m.scope, m.scopeID)
	if err := scope.Publish(msg); err != nil {
		return fmt.Errorf("committing removal: %w", err)
	}

	if err := m.syncToLocalFast(); err != nil {
		m.log().Warn("post-write sync failed", "op", "remove", "id", id, "error", err)
	}
	return nil
}

// archiveBeforeDelete copies the memory file into the history directory. A failure is logged
// and not returned: losing the archive is bad, and refusing to delete a memory the user asked
// to delete is worse.
func (m *MemoryService) archiveBeforeDelete(scope *ScopeStore, id string, candidates ...string) {
	for _, rel := range candidates {
		data, err := scope.ReadFile(rel)
		if err != nil {
			continue
		}
		if err := scope.WriteFile(HistoryPath(id, currentRevision(string(data))), data); err != nil {
			m.log().Warn("archiving before delete failed", "id", id, "error", err)
		}
		return
	}
}

func (m *MemoryService) PromoteMemory(id string) error {
	return m.changeRelevance(id, true)
}

func (m *MemoryService) DemoteMemory(id string) error {
	return m.changeRelevance(id, false)
}

func (m *MemoryService) changeRelevance(id string, promote bool) error {
	if m.store == nil {
		return fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	scopePath := m.ScopePrefix()
	scope, err := m.store.OpenScopeLocal(scopePath)
	if err != nil {
		return fmt.Errorf("opening the memory scope: %w", err)
	}

	relPath := MemoryFileName(id)

	data, err := scope.ReadFile(relPath)
	if err != nil {
		if os.IsNotExist(err) {
			return fmt.Errorf("memory %q not found", id)
		}
		return fmt.Errorf("reading memory file: %w", err)
	}

	if IsImportantContent(string(data)) == promote {
		return nil
	}

	if err := scope.WriteFile(relPath, []byte(withImportantFlag(string(data), promote))); err != nil {
		return fmt.Errorf("writing memory file: %w", err)
	}

	verb := "promote"
	if !promote {
		verb = "demote"
	}
	msg := fmt.Sprintf("memory: %s %s (%s/%s)", verb, id, m.scope, m.scopeID)
	if err := scope.Publish(msg); err != nil {
		return fmt.Errorf("committing relevance change: %w", err)
	}

	if err := m.syncToLocalFast(); err != nil {
		m.log().Warn("post-write sync failed", "op", verb, "id", id, "error", err)
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
		title, createdAt, important := parseMemoryHeader(filepath.Join(rawDir, name))
		memories = append(memories, MemoryEntry{
			ID:        MemoryIDFromFileName(name),
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
	// Fast path: with no remote configured the raw directory is the only source there is,
	// so an existing one goes straight to indexing. There is nothing to sync from.
	if m.store != nil && !m.store.Configured() {
		scopePath := m.ScopePrefix()
		if m.store.HasLocalScope(scopePath) {
			rawDir := m.store.ScopeDir(scopePath)
			m.localDir = rawDir
			m.wikiDir = MemoryWikiGlobalDir(string(m.scope), m.scopeID)
			m.ensureWikiDir()
			ctx := context.Background()
			if err := m.IndexMemories(ctx); err != nil {
				m.log().Warn("wiki indexing failed", "scope", m.scope, "scopeID", m.scopeID, "error", err)
			}
			return nil
		}
	}
	return m.syncToLocalInternal(false)
}

func (m *MemoryService) syncToLocalFast() error {
	return m.syncToLocalInternal(true)
}

func (m *MemoryService) syncToLocalInternal(skipNetwork bool) error {
	if m.store == nil {
		return fmt.Errorf("memory repository not configured")
	}

	scopePath := m.ScopePrefix()
	var scope *ScopeStore
	var err error
	if skipNetwork {
		scope, err = m.store.OpenScopeLocal(scopePath)
	} else {
		scope, err = m.store.OpenScope(scopePath)
	}
	if err != nil {
		return fmt.Errorf("opening the memory scope: %w", err)
	}
	wtDir := scope.Dir()

	m.localDir = wtDir

	m.wikiDir = MemoryWikiGlobalDir(string(m.scope), m.scopeID)
	m.ensureWikiDir()

	ctx := context.Background()
	if err := m.IndexMemories(ctx); err != nil {
		m.log().Warn("wiki indexing failed", "scope", m.scope, "scopeID", m.scopeID, "error", err)
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

func buildMemoryFile(id, title, body, scope, scopeID, origProjectID string, important bool, memType string, userTags []string) string {
	now := time.Now().UTC().Format(time.RFC3339)

	tagSet := []string{"memory", scope}
	if memType != "" {
		tagSet = append(tagSet, memType)
	}
	tagSet = append(tagSet, userTags...)

	return renderMemoryFile(MemoryFrontmatter{
		ID:        id,
		Title:     title,
		Scope:     scope,
		ScopeID:   scopeID,
		ProjectID: origProjectID,
		Type:      memType,
		Important: important,
		CreatedAt: now,
		UpdatedAt: now,
		Tags:      tagSet,
		Revision:  1,
		UpdatedBy: unitOrEmpty(),
	}, body)
}

// MemoryFrontmatter is every classification field a memory file carries.
//
// It exists because rebuilding frontmatter field-by-field at each write site is
// how classification gets silently dropped: an update that reconstructs only the
// fields its author remembered turns a `correction` tagged `auth,security` into
// an untyped, untagged memory, and nothing reports the loss because the file is
// still valid and the body is still right.
type MemoryFrontmatter struct {
	ID        string `yaml:"id"`
	Title     string `yaml:"title"`
	Scope     string `yaml:"scope"`
	ScopeID   string `yaml:"scope_id"`
	ProjectID string `yaml:"project_id,omitempty"`
	Type      string `yaml:"type,omitempty"`
	Important bool   `yaml:"important,omitempty"`
	CreatedAt string `yaml:"created_at,omitempty"`
	UpdatedAt string `yaml:"updated_at"`

	// Revision counts the writes this memory has had, starting at 1. It is the history the git
	// repository used to carry, moved in-band: object storage has no commit to hang it on, and a
	// memory that cannot say whether it was edited is a memory you cannot trust twice.
	Revision int `yaml:"revision"`

	// UpdatedBy is the unit that made the last write. It replaces the git author, and it is why
	// the identity had to survive git's removal rather than disappear with it.
	UpdatedBy string `yaml:"updated_by,omitempty"`

	// Previous is the path of the version this one replaced, relative to the scope's directory,
	// empty on the first revision. Following it walks the whole chain backwards.
	Previous string `yaml:"previous,omitempty"`

	Tags []string `yaml:"tags,flow"`
}

// ParseMemoryFrontmatter reads the classification fields out of a memory file.
// Absent fields come back as zero values; the caller decides what a missing field
// means.
//
// The YAML parser resolves the block, and it is the only thing that does. A
// line-by-line scan cannot tell a quoted colon from a key separator, and it cannot
// tell the frontmatter from a body that happens to contain `important: true` —
// which is a memory silently promoting itself by quoting one.
func ParseMemoryFrontmatter(content string) MemoryFrontmatter {
	block, ok := wiki.FrontmatterBlock(content)
	if !ok {
		return MemoryFrontmatter{}
	}
	var fm MemoryFrontmatter
	if err := yaml.Unmarshal([]byte(block), &fm); err != nil {
		return MemoryFrontmatter{}
	}
	return fm
}

// memoryUpdate is the input to updatedMemoryContent. Empty NewTitle, NewBody and
// NewType each mean "leave that alone".
type memoryUpdate struct {
	ID       string
	Scope    string
	ScopeID  string
	NewTitle string
	NewBody  string
	NewType  string

	// Previous is the archived path of the version being replaced. The caller writes the archive
	// and passes the path, because only the caller has the store to write it to.
	Previous string
}

// updatedMemoryContent computes the file content for an update, preserving every
// classification field the memory already had.
//
// It is a pure function so the preservation rule is testable without a git store.
// That matters because the rule was silently broken: the update path rebuilt the
// frontmatter from the handful of fields its author remembered — id, title, scope,
// scope_id, created_at, updated_at and a hardcoded `tags: [memory]` — which dropped
// type, importance, project_id and every user tag. An important `correction` tagged
// `auth,security` became an untyped, untagged memory on its first edit, and nothing
// reported it, because the result was still a valid file with the right body.
func updatedMemoryContent(oldContent string, u memoryUpdate) string {
	fm := ParseMemoryFrontmatter(oldContent)
	fm.ID = u.ID
	fm.Scope = u.Scope
	fm.ScopeID = u.ScopeID
	// Overwritten deliberately: this is the one timestamp an update owns.
	fm.UpdatedAt = time.Now().UTC().Format(time.RFC3339)
	// The chain. A memory with no revision recorded is treated as revision 1, so the first edit
	// of one written before this existed becomes revision 2 rather than restarting the count.
	if fm.Revision < 1 {
		fm.Revision = 1
	}
	fm.Revision++
	fm.Previous = u.Previous
	fm.UpdatedBy = unitOrEmpty()

	if u.NewTitle != "" {
		fm.Title = u.NewTitle
	}
	if u.NewType != "" && ValidMemoryType(u.NewType) && u.NewType != fm.Type {
		// Keep the tag list consistent with the type it describes, otherwise a
		// reclassified memory stays searchable under the old one.
		fm.Tags = replaceTypeTag(fm.Tags, fm.Type, u.NewType)
		fm.Type = u.NewType
	}
	if len(fm.Tags) == 0 {
		fm.Tags = []string{"memory", u.Scope}
		if fm.Type != "" {
			fm.Tags = append(fm.Tags, fm.Type)
		}
	}

	body := u.NewBody
	if body == "" {
		// An update that passes no body is changing the title. Erasing the body
		// because it was not restated is not what any caller means.
		body = extractBodyAfterFrontmatter(oldContent)
	}

	return renderMemoryFile(fm, body)
}

// currentRevision is the revision a memory file declares, defaulting to 1.
func currentRevision(content string) int {
	if rev := ParseMemoryFrontmatter(content).Revision; rev >= 1 {
		return rev
	}
	return 1
}

// withImportantFlag re-renders a memory file with the importance flag set or
// cleared, leaving every other field — including the body and updated_at — as it
// was. Promotion is not an edit of the memory's content, so it must not look like
// one.
func withImportantFlag(content string, important bool) string {
	fm := ParseMemoryFrontmatter(content)
	fm.Important = important
	return renderMemoryFile(fm, extractBodyAfterFrontmatter(content))
}

// replaceTypeTag swaps the tag that mirrors the memory type, leaving every other
// tag — including the user's own — untouched.
func replaceTypeTag(tags []string, oldType, newType string) []string {
	out := make([]string, 0, len(tags)+1)
	replaced := false
	for _, t := range tags {
		if oldType != "" && t == oldType {
			out = append(out, newType)
			replaced = true
			continue
		}
		if t == newType {
			replaced = true
		}
		out = append(out, t)
	}
	if !replaced {
		out = append(out, newType)
	}
	return out
}

// renderMemoryFile writes a memory file from its frontmatter and body. It is the
// single place the on-disk shape is defined, so add and update cannot disagree
// about which fields a memory has.
func renderMemoryFile(fm MemoryFrontmatter, body string) string {
	if fm.UpdatedAt == "" {
		fm.UpdatedAt = time.Now().UTC().Format(time.RFC3339)
	}
	if fm.Revision < 1 {
		fm.Revision = 1
	}
	if fm.Tags == nil {
		fm.Tags = []string{}
	}

	front, err := yaml.Marshal(fm)
	if err != nil {
		front = nil
	}

	var b strings.Builder
	b.WriteString("---\n")
	b.Write(front)
	b.WriteString("---\n\n")
	_, _ = fmt.Fprintf(&b, "# %s\n\n", fm.Title)
	if body != "" {
		b.WriteString(body)
		if !strings.HasSuffix(body, "\n") {
			b.WriteString("\n")
		}
	}
	return b.String()
}

func parseMemoryMeta(path string) (title, createdAt string) {
	title, createdAt, _ = parseMemoryHeader(path)
	return title, createdAt
}

func parseMemoryHeader(path string) (title, createdAt string, important bool) {
	data, err := os.ReadFile(path)
	if err != nil {
		return filepath.Base(path), "", false
	}
	content := string(data)
	fm := ParseMemoryFrontmatter(content)
	title = fm.Title
	if title == "" {
		for _, line := range strings.Split(content, "\n") {
			if strings.HasPrefix(line, "# ") {
				title = strings.TrimPrefix(line, "# ")
				break
			}
		}
	}
	if title == "" {
		title = MemoryIDFromFileName(path)
	}
	return title, fm.CreatedAt, fm.Important
}

func ParseMemoryMetaPublic(path string) (title, createdAt string) {
	return parseMemoryMeta(path)
}
