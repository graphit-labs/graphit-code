package memory

import (
	"context"
	"fmt"
	"log/slog"
	"os"
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
	Mandatory bool
	Type      MemoryType
	Tags      []string
}

type MemoryService struct {
	Logger   *slog.Logger
	scope    MemoryScope
	scopeID  string
	store    *MemoryStore
	tableURI string
	wikiDir  string
	baseCtx  context.Context
}

func (m *MemoryService) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

func NewMemoryService(scope MemoryScope, scopeID string, store *MemoryStore) *MemoryService {
	return newMemorySvcInternal(scope, scopeID, store)
}

func NewMemoryServiceForContext(contextName string, store *MemoryStore) *MemoryService {
	return newMemorySvcInternal(MemoryScopeContext, contextName, store)
}

func newMemorySvcInternal(scope MemoryScope, scopeID string, store *MemoryStore) *MemoryService {
	svc := &MemoryService{
		scope:   scope,
		scopeID: scopeID,
		store:   store,
		wikiDir: MemoryWikiGlobalDir(localScope(scope, scopeID)),
		baseCtx: context.Background(),
	}
	svc.tableURI = MemoryTableURI(svc.ScopePrefix(), TableDirFor(localScope(scope, scopeID)))
	if store != nil {
		store.Logger = svc.Logger
	}
	return svc
}

func (m *MemoryService) WithContext(ctx context.Context) *MemoryService {
	clone := *m
	if ctx == nil {
		ctx = context.Background()
	}
	clone.baseCtx = ctx
	return &clone
}

func (m *MemoryService) operationContext() context.Context {
	if m.baseCtx != nil {
		return m.baseCtx
	}
	return context.Background()
}

func localScope(scope MemoryScope, scopeID string) (string, string) {
	if scope == MemoryScopeContext {
		return scopeID, scopeID
	}
	return string(scope), scopeID
}

func (m *MemoryService) openTable(ctx context.Context) (*MemoryTable, error) {
	uri := m.resolveTableURI()
	if uri == "" {
		return nil, fmt.Errorf("memory store not configured — run '%s setup' first", brand.BinName())
	}
	return OpenMemoryTable(ctx, uri)
}

func (m *MemoryService) resolveTableURI() string {
	if m.tableURI != "" {
		return m.tableURI
	}
	if m.scopeID == "" {
		return ""
	}
	return MemoryTableURI(m.ScopePrefix(), TableDirFor(localScope(m.scope, m.scopeID)))
}

func (m *MemoryService) putMarkdown(ctx context.Context, tbl *MemoryTable, rel, content string) error {
	rec, ok := recordFromMarkdown(rel, []byte(content))
	if !ok {
		return fmt.Errorf("refusing to store %s: its frontmatter did not parse", rel)
	}
	return tbl.Put(ctx, rec)
}

func (m *MemoryService) readMarkdown(ctx context.Context, tbl *MemoryTable, key string) (string, bool, error) {
	rec, ok, err := tbl.Get(ctx, key)
	if err != nil || !ok {
		return "", ok, err
	}
	return rec.Markdown(), true, nil
}

func (m *MemoryService) Close() error { return nil }

func (m *MemoryService) WikiDir() string { return m.wikiDir }

// TableURI is where this scope's store table lives: normally an `s3://` prefix when a bucket is
// configured, otherwise a local directory. Anonymous user memory is always local. See
// MemoryTableURI for why both forms exist.
func (m *MemoryService) TableURI() string { return m.resolveTableURI() }

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

	if err := m.syncWikiAfterWrite(); err != nil {
		m.log().Warn("init local sync failed", "scope", m.scope, "scopeID", m.scopeID, "error", err)
	}
	return nil
}

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
	Mandatory bool
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
	content := buildMemoryFileWithRelevance(id, title, body, string(m.scope), m.scopeID, assocProject,
		opts.Important, opts.Mandatory, string(memType), opts.Tags)

	ctx := m.operationContext()
	tbl, err := m.openTable(ctx)
	if err != nil {
		return "", err
	}
	defer func() { _ = tbl.Close() }()

	if err := m.putMarkdown(ctx, tbl, MemoryFileName(id), content); err != nil {
		return "", fmt.Errorf("storing the memory: %w", err)
	}

	if err := m.syncWikiAfterWrite(); err != nil {
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

	ctx := m.operationContext()
	tbl, err := m.openTable(ctx)
	if err != nil {
		return err
	}
	defer func() { _ = tbl.Close() }()

	relPath := MemoryFileName(id)
	data, ok, err := m.readMarkdown(ctx, tbl, id)
	if err != nil {
		return fmt.Errorf("reading memory %q: %w", id, err)
	}
	if !ok {
		return fmt.Errorf("memory %q not found", id)
	}

	archived, err := m.archiveRevision(ctx, tbl, id, data, relPath)
	if err != nil {
		return err
	}

	content := updatedMemoryContent(data, memoryUpdate{
		ID:       id,
		Scope:    string(m.scope),
		ScopeID:  m.scopeID,
		NewTitle: newTitle,
		NewBody:  newBody,
		NewType:  memType,
		Previous: archived,
	})

	if err := m.putMarkdown(ctx, tbl, relPath, content); err != nil {
		return fmt.Errorf("storing the updated memory: %w", err)
	}

	if err := m.syncWikiAfterWrite(); err != nil {
		m.log().Warn("post-write sync failed", "op", "update", "id", id, "error", err)
	}
	return nil
}

func (m *MemoryService) RemoveMemory(id string) error {
	if m.store == nil {
		return fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	ctx := m.operationContext()
	tbl, err := m.openTable(ctx)
	if err != nil {
		return err
	}
	defer func() { _ = tbl.Close() }()

	if _, ok, getErr := tbl.Get(ctx, id); getErr != nil {
		return fmt.Errorf("reading memory %q: %w", id, getErr)
	} else if !ok {
		return fmt.Errorf("memory %q not found", id)
	}

	m.archiveBeforeDelete(ctx, tbl, id)

	if err := tbl.Delete(ctx, id); err != nil {
		return fmt.Errorf("removing the memory: %w", err)
	}

	if err := m.syncWikiAfterWrite(); err != nil {
		m.log().Warn("post-write sync failed", "op", "remove", "id", id, "error", err)
	}
	return nil
}

func (m *MemoryService) archiveBeforeDelete(ctx context.Context, tbl *MemoryTable, id string) {
	data, ok, err := m.readMarkdown(ctx, tbl, id)
	if err != nil || !ok {
		return
	}
	if _, err := m.archiveRevision(ctx, tbl, id, data, ""); err != nil {
		m.log().Warn("archiving before delete failed", "id", id, "error", err)
	}
}

func (m *MemoryService) archiveRevision(ctx context.Context, tbl *MemoryTable, id, content, nextPath string) (string, error) {
	revisionID := NewRevisionID()
	archived := HistoryPath(id, revisionID)

	if err := m.putMarkdown(ctx, tbl, archived, archivedRevisionContent(id, content, revisionID, nextPath)); err != nil {
		return "", fmt.Errorf("archiving the previous revision: %w", err)
	}

	m.repointArchiveNext(ctx, tbl, ParseMemoryFrontmatter(content).Previous, archived)
	return archived, nil
}

func (m *MemoryService) repointArchiveNext(ctx context.Context, tbl *MemoryTable, archiveRel, nextPath string) {
	if archiveRel == "" {
		return
	}
	key := archiveKeyFromPath(archiveRel)
	if key == "" {
		return
	}
	data, ok, err := m.readMarkdown(ctx, tbl, key)
	if err != nil || !ok {
		return
	}
	fm, parsed := ParseMemoryFrontmatterOK(data)
	if !parsed {
		m.log().Warn("archive frontmatter unreadable; leaving it alone", "archive", archiveRel)
		return
	}
	if fm.Next == nextPath {
		return
	}
	fm.Next = nextPath
	if fm.RevisionID == "" {
		fm.RevisionID = RevisionIDFromHistoryPath(archiveRel)
	}
	body := extractBodyAfterFrontmatter(data)
	if err := m.putMarkdown(ctx, tbl, archiveRel, renderMemoryFile(fm, body)); err != nil {
		m.log().Warn("repointing the previous archive failed", "archive", archiveRel, "error", err)
	}
}

func archiveKeyFromPath(archiveRel string) string {
	id := chainIDFromHistoryPath(archiveRel)
	rev := RevisionIDFromHistoryPath(archiveRel)
	if id == "" || rev == "" {
		return ""
	}
	return id + "/" + rev
}

func (m *MemoryService) PromoteMemory(id string) error {
	return m.changeImportance(id, true)
}

func (m *MemoryService) DemoteMemory(id string) error {
	return m.changeImportance(id, false)
}

func (m *MemoryService) MarkMandatory(id string) error {
	return m.changeMandatory(id, true)
}

func (m *MemoryService) UnmarkMandatory(id string) error {
	return m.changeMandatory(id, false)
}

func (m *MemoryService) changeImportance(id string, promote bool) error {
	return m.changeRelevance(id, "importance", promote)
}

func (m *MemoryService) changeMandatory(id string, mandatory bool) error {
	return m.changeRelevance(id, "mandatory", mandatory)
}

func (m *MemoryService) changeRelevance(id, field string, enabled bool) error {
	if m.store == nil {
		return fmt.Errorf("memory repository not configured — run '%s setup' first", brand.BinName())
	}

	ctx := m.operationContext()
	tbl, err := m.openTable(ctx)
	if err != nil {
		return err
	}
	defer func() { _ = tbl.Close() }()

	relPath := MemoryFileName(id)
	data, ok, err := m.readMarkdown(ctx, tbl, id)
	if err != nil {
		return fmt.Errorf("reading memory %q: %w", id, err)
	}
	if !ok {
		return fmt.Errorf("memory %q not found", id)
	}

	fm, parsed := ParseMemoryFrontmatterOK(data)
	if !parsed {
		return fmt.Errorf("refusing to change %s of %q: its frontmatter did not parse", field, id)
	}
	current := fm.Important
	if field == "mandatory" {
		current = fm.Mandatory
	}
	if current == enabled {
		return nil
	}

	updated := withImportantFlag(data, enabled)
	if field == "mandatory" {
		updated = withMandatoryFlag(data, enabled)
	}
	if err := m.putMarkdown(ctx, tbl, relPath, updated); err != nil {
		return fmt.Errorf("storing the relevance change: %w", err)
	}

	verb := "promote"
	if field == "mandatory" {
		verb = "mark_mandatory"
	}
	if !enabled {
		verb = "demote"
		if field == "mandatory" {
			verb = "unmark_mandatory"
		}
	}

	if err := m.syncWikiAfterWrite(); err != nil {
		m.log().Warn("post-write sync failed", "op", verb, "id", id, "error", err)
	}
	return nil
}

func (m *MemoryService) ListMemories() ([]MemoryEntry, error) {
	ctx := m.operationContext()
	tbl, err := m.openTable(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tbl.Close() }()

	records, err := tbl.Live(ctx)
	if err != nil {
		return nil, err
	}

	memories := make([]MemoryEntry, 0, len(records))
	for _, rec := range records {
		memories = append(memories, MemoryEntry{
			ID:        rec.ID,
			Title:     rec.Title,
			CreatedAt: rec.CreatedAt,
			Scope:     m.scope,
			ScopeID:   m.scopeID,
			Important: rec.Important,
			Mandatory: rec.Mandatory,
			Type:      MemoryType(rec.Type),
			Tags:      rec.Tags,
		})
	}
	return memories, nil
}

// LiveMemories is the scope's live records, bodies included.
//
// ListMemories answers the same question for a catalogue — id, title, type, importance — and
// deliberately drops the bodies, because listing 400 memories does not need 400 bodies. This is for
// the caller that has to compare CONTENT: consolidation, which decides what duplicates what.
func (m *MemoryService) LiveMemories(ctx context.Context) ([]MemoryRecord, error) {
	tbl, err := m.openTable(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { _ = tbl.Close() }()
	return tbl.Live(ctx)
}

// SyncWiki recompiles this scope's local query projection from its authoritative table.
func (m *MemoryService) SyncWiki() error {
	m.ensureWikiDir()
	if err := m.IndexMemories(m.operationContext()); err != nil {
		m.log().Warn("wiki indexing failed", "scope", m.scope, "scopeID", m.scopeID, "error", err)
	}
	return nil
}

func (m *MemoryService) syncWikiAfterWrite() error {
	return m.SyncWiki()
}

func (m *MemoryService) IndexMemories(ctx context.Context) error {
	tbl, err := m.openTable(ctx)
	if err != nil {
		return err
	}
	defer func() { _ = tbl.Close() }()

	if _, err := GenerateMemoryWikiFromTable(ctx, tbl, m.wikiDir); err != nil {
		return fmt.Errorf("generating memory wiki: %w", err)
	}
	return nil
}

func buildMemoryFile(id, title, body, scope, scopeID, origProjectID string, important bool, memType string, userTags []string) string {
	return buildMemoryFileWithRelevance(id, title, body, scope, scopeID, origProjectID, important, false, memType, userTags)
}

func buildMemoryFileWithRelevance(id, title, body, scope, scopeID, origProjectID string, important, mandatory bool, memType string, userTags []string) string {
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
		Mandatory: mandatory,
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
	Mandatory bool   `yaml:"mandatory,omitempty"`
	CreatedAt string `yaml:"created_at,omitempty"`
	UpdatedAt string `yaml:"updated_at"`

	Revision int `yaml:"revision"`

	// UpdatedBy is the unit that made the last write. It replaces the git author, and it is why
	// the identity had to survive git's removal rather than disappear with it.
	UpdatedBy string `yaml:"updated_by,omitempty"`

	// Previous is the path of the version this one replaced, relative to the scope's directory,
	// empty on the first revision. Following it walks the whole chain backwards.
	Previous string `yaml:"previous,omitempty"`

	// Next is the path of the version that replaced this one, relative to the scope's directory.
	// It is set only on an ARCHIVED revision, and it is the other half of Previous: the two make
	// the chain walkable in both directions. Its value is another archive's path, or `<id>.md`
	// when the thing that replaced this revision is the live memory itself.
	//
	// A live memory has no Next, and that absence is its definition: it is the head of its chain.
	Next string `yaml:"next,omitempty"`

	RevisionID string `yaml:"revision_id,omitempty"`

	Tags []string `yaml:"tags,flow"`
}

// IsArchivedRevision reports whether frontmatter describes a superseded revision rather than a
// live memory.
func (fm MemoryFrontmatter) IsArchivedRevision() bool {
	return fm.RevisionID != ""
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
	fm, _ := ParseMemoryFrontmatterOK(content)
	return fm
}

func ParseMemoryFrontmatterOK(content string) (MemoryFrontmatter, bool) {
	block, ok := wiki.FrontmatterBlock(content)
	if !ok {
		return MemoryFrontmatter{}, false
	}

	var fm MemoryFrontmatter
	if err := yaml.Unmarshal([]byte(block), &fm); err == nil {
		return fm, true
	}

	repaired := quoteUnquotedScalars(block)
	if err := yaml.Unmarshal([]byte(repaired), &fm); err == nil {
		return fm, true
	}
	return MemoryFrontmatter{}, false
}

var frontmatterScalarKeys = map[string]bool{
	"title": true, "scope": true, "scope_id": true, "project_id": true,
	"type": true, "created_at": true, "updated_at": true,
	"updated_by": true, "previous": true, "next": true, "revision_id": true,
}

// quoteUnquotedScalars single-quotes the value of any scalar key that is not already quoted.
//
// It is deliberately line-based and deliberately narrow: it touches only the keys above, only when
// the value is unquoted, and it never reorders or drops a line. Anything it cannot make sense of it
// leaves alone, so the worst case is the parse failing exactly as it did before.
func quoteUnquotedScalars(block string) string {
	lines := strings.Split(block, "\n")
	for i, line := range lines {
		colon := strings.Index(line, ":")
		if colon <= 0 || strings.HasPrefix(line, " ") || strings.HasPrefix(line, "\t") ||
			strings.HasPrefix(strings.TrimSpace(line), "#") {
			continue
		}
		key := line[:colon]
		if !frontmatterScalarKeys[key] {
			continue
		}
		value := strings.TrimSpace(line[colon+1:])
		if value == "" || strings.HasPrefix(value, "'") || strings.HasPrefix(value, "\"") {
			continue
		}
		lines[i] = key + ": '" + strings.ReplaceAll(value, "'", "''") + "'"
	}
	return strings.Join(lines, "\n")
}

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

func updatedMemoryContent(oldContent string, u memoryUpdate) string {
	fm, parsed := ParseMemoryFrontmatterOK(oldContent)
	// SAFETY: an unreadable frontmatter must not be replaced by an empty one. The title is the one
	// field recoverable from the body, and keeping it is what stops the memory becoming anonymous.
	if !parsed {
		fm.Title = firstH1(oldContent)
	}
	fm.ID = u.ID
	fm.Scope = u.Scope
	fm.ScopeID = u.ScopeID
	fm.UpdatedAt = time.Now().UTC().Format(time.RFC3339)
	if fm.Revision < 1 {
		fm.Revision = 1
	}
	fm.Revision++
	fm.Previous = u.Previous
	fm.UpdatedBy = unitOrEmpty()
	fm.Next = ""
	fm.RevisionID = ""

	if u.NewTitle != "" {
		fm.Title = u.NewTitle
	}
	if u.NewType != "" && ValidMemoryType(u.NewType) && u.NewType != fm.Type {
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
		body = extractBodyAfterFrontmatter(oldContent)
	}

	return renderMemoryFile(fm, body)
}

func archivedRevisionContent(chainID, content, revisionID, nextPath string) string {
	fm, parsed := ParseMemoryFrontmatterOK(content)
	if !parsed {
		fm.Title = firstH1(content)
	}
	if chainID != "" {
		fm.ID = chainID
	}
	fm.RevisionID = revisionID
	fm.Next = nextPath
	if fm.Revision < 1 {
		fm.Revision = 1
	}
	return renderMemoryFile(fm, extractBodyAfterFrontmatter(content))
}

// withImportantFlag re-renders a memory file with the importance flag set or
// cleared, leaving every other field — including the body and updated_at — as it
// was. Promotion is not an edit of the memory's content, so it must not look like
// one.
func withImportantFlag(content string, important bool) string {
	fm, parsed := ParseMemoryFrontmatterOK(content)
	if !parsed {
		return content
	}
	fm.Important = important
	return renderMemoryFile(fm, extractBodyAfterFrontmatter(content))
}

func withMandatoryFlag(content string, mandatory bool) string {
	fm, parsed := ParseMemoryFrontmatterOK(content)
	if !parsed {
		return content
	}
	fm.Mandatory = mandatory
	return renderMemoryFile(fm, extractBodyAfterFrontmatter(content))
}

func firstH1(content string) string {
	for _, line := range strings.Split(content, "\n") {
		if strings.HasPrefix(line, "# ") {
			return strings.TrimSpace(strings.TrimPrefix(line, "# "))
		}
	}
	return ""
}

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

func sameMemoryBody(a, b string) bool {
	return strings.TrimSpace(extractBodyAfterFrontmatter(a)) ==
		strings.TrimSpace(extractBodyAfterFrontmatter(b))
}
