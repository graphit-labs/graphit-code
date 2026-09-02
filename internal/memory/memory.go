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
	}
	svc.tableURI = MemoryTableURI(svc.ScopePrefix(), TableDirFor(localScope(scope, scopeID)))
	if store != nil {
		store.Logger = svc.Logger
	}
	return svc
}

// localScope is the (scope, scopeID) pair a scope's LOCAL artifacts are named from — both its wiki
// and its table directory.
//
// An imported context names both halves after itself, while the project and user scopes use their
// scope word and their id. That doubling is what lets AllContextDirs recognise a context by its
// directory name alone, rather than by matching the names of the other two scopes.
//
// 🔒 THE WIKI USED TO BE NAMED FROM `string(scope)` INSTEAD, and for a context that is the literal
// word "context" — so a service built by NewMemoryServiceForContext compiled into
// `wiki/memory/context/<name>` while every reader of a context's memories looked in
// `wiki/memory/<name>/<name>`: the UI's picker, `SyncContextFromMemoryRepo`, and the removal path
// that deletes a context. Compiling through the service therefore produced a wiki nothing opened,
// and left an empty `wiki/memory/context/` directory behind as the only sign of it.
func localScope(scope MemoryScope, scopeID string) (string, string) {
	if scope == MemoryScopeContext {
		return scopeID, scopeID
	}
	return string(scope), scopeID
}

// openTable opens this scope's store for one operation.
//
// Per operation rather than held on the service, matching what OpenScopeLocal did: a write is a
// short transaction against object storage, and a long-lived handle would pin a dataset version for
// the life of the process — which is what makes a reader answer from a snapshot taken before every
// write it is about to be asked about.
func (m *MemoryService) openTable(ctx context.Context) (*MemoryTable, error) {
	uri := m.resolveTableURI()
	if uri == "" {
		return nil, fmt.Errorf("memory store not configured — run '%s setup' first", brand.BinName())
	}
	return OpenMemoryTable(ctx, uri)
}

// resolveTableURI is the scope's store location, derived when the field is not set.
//
// The field is an OVERRIDE, not a cache: it exists so a test can point a scope at a temporary
// directory instead of the machine's real store. Deriving when it is empty is not a fallback in the
// sense this project forbids — there is one location for a scope and this computes it from the same
// two inputs the constructor uses. What it removes is the possibility of a service that was built
// without going through the constructor answering "not configured" about a scope that is perfectly
// well defined.
func (m *MemoryService) resolveTableURI() string {
	if m.tableURI != "" {
		return m.tableURI
	}
	if m.scopeID == "" {
		return ""
	}
	return MemoryTableURI(m.ScopePrefix(), TableDirFor(localScope(m.scope, m.scopeID)))
}

// putMarkdown writes content that one of the content transforms produced.
//
// 🔒 THE MARKDOWN IS A TRANSIT FORMAT HERE, NOT A FILE. Nothing is written to disk: the four content
// transforms — buildMemoryFile, updatedMemoryContent, archivedRevisionContent, withImportantFlag —
// keep operating on text, and this converts their output into the row that is actually stored.
//
// Keeping them is deliberate. They carry the revision, previous/next and classification bookkeeping
// that a data-loss bug was once found in, and recordFromMarkdown enforces the guard that came out of
// it: a frontmatter that does not parse is REFUSED rather than re-rendered from an empty struct.
// Rewriting all four to operate on MemoryRecord directly is the cleaner end state and belongs in a
// slice of its own, with tests, not in the one that moves the storage.
func (m *MemoryService) putMarkdown(ctx context.Context, tbl *MemoryTable, rel, content string) error {
	rec, ok := recordFromMarkdown(rel, []byte(content))
	if !ok {
		return fmt.Errorf("refusing to store %s: its frontmatter did not parse", rel)
	}
	return tbl.Put(ctx, rec)
}

// readMarkdown renders a stored record back into the text the content transforms expect.
func (m *MemoryService) readMarkdown(ctx context.Context, tbl *MemoryTable, key string) (string, bool, error) {
	rec, ok, err := tbl.Get(ctx, key)
	if err != nil || !ok {
		return "", ok, err
	}
	return rec.Markdown(), true, nil
}

func (m *MemoryService) Close() error { return nil }

func (m *MemoryService) WikiDir() string { return m.wikiDir }

// TableURI is where this scope's store table lives: an `s3://` prefix when a bucket is configured,
// a local directory when there is not. See MemoryTableURI for why both forms exist.
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

	ctx := context.Background()
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

	ctx := context.Background()
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

	// Archive the version being replaced BEFORE overwriting it, and point the new one at the
	// archive. This is the history the git repository used to carry: object storage has no commit
	// to hang it on, so the chain lives in the memory itself.
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

	ctx := context.Background()
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

	// Archive the version being deleted, so the trail survives the deletion — which is what the
	// git repository did, where a removed file stayed reachable in history.
	//
	// Nothing points AT the archive afterwards: the memory that would have carried the `previous`
	// field is the one being removed. So a deleted memory's chain is found by its id, through
	// MemoryTable.Revisions, not by following a pointer.
	m.archiveBeforeDelete(ctx, tbl, id)

	if err := tbl.Delete(ctx, id); err != nil {
		return fmt.Errorf("removing the memory: %w", err)
	}

	if err := m.syncWikiAfterWrite(); err != nil {
		m.log().Warn("post-write sync failed", "op", "remove", "id", id, "error", err)
	}
	return nil
}

// archiveBeforeDelete copies the memory file into the history directory. A failure is logged
// and not returned: losing the archive is bad, and refusing to delete a memory the user asked
// to delete is worse.
//
// The archive's `next` is left empty, because nothing replaced this revision — the chain ends
// here, and its head is gone. That is what tells a later reader the difference between a
// superseded revision and the last state of a deleted memory.
func (m *MemoryService) archiveBeforeDelete(ctx context.Context, tbl *MemoryTable, id string) {
	data, ok, err := m.readMarkdown(ctx, tbl, id)
	if err != nil || !ok {
		return
	}
	if _, err := m.archiveRevision(ctx, tbl, id, data, ""); err != nil {
		m.log().Warn("archiving before delete failed", "id", id, "error", err)
	}
}

// archiveRevision writes one superseded revision into the chain and returns its path.
//
// nextPath is what replaced this revision: the live memory's file name on an update, empty on a
// delete. The archive that was previously the newest is repointed forward at this one, which is
// what keeps the chain walkable in both directions rather than only backwards.
func (m *MemoryService) archiveRevision(ctx context.Context, tbl *MemoryTable, id, content, nextPath string) (string, error) {
	revisionID := NewRevisionID()
	archived := HistoryPath(id, revisionID)

	// The path is passed as the record's `rel`, which is what makes recordFromMarkdown mark the row
	// superseded and recover its revision id — the same derivation the migration used, so a native
	// archive and a migrated one are indistinguishable rows.
	if err := m.putMarkdown(ctx, tbl, archived, archivedRevisionContent(id, content, revisionID, nextPath)); err != nil {
		return "", fmt.Errorf("archiving the previous revision: %w", err)
	}

	m.repointArchiveNext(ctx, tbl, ParseMemoryFrontmatter(content).Previous, archived)
	return archived, nil
}

// repointArchiveNext moves an older archive's forward pointer onto the revision that now follows
// it.
//
// A failure is logged and swallowed on purpose: the chain degrades to what it was before `next`
// existed — walkable backwards, and with the head still named by every archive's `id` — which is
// not worth failing a write the caller asked for.
// repointArchiveNext moves an older archive's forward pointer onto the revision that now follows it.
//
// `archiveRel` is a `history/<id>/<rev>.md` PATH, because that is what `previous` holds — in the 84
// migrated records that already carry one and in every archive written since. The path is an
// IDENTIFIER now rather than a location: the row key it names is derived from it, which is why the
// values were carried across the migration verbatim instead of being rewritten.
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
		// SAFETY: a forward pointer is not worth the classification of the revision it is on.
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

// archiveKeyFromPath turns a `history/<id>/<rev>.md` path into the row key that holds it.
//
// It mirrors MemoryRecord.Key for an archived revision, and it uses the SAME two derivations the
// migration used — the chain id from the directory, the revision id from the file name — so a
// pointer written before the store was a table still resolves.
func archiveKeyFromPath(archiveRel string) string {
	id := chainIDFromHistoryPath(archiveRel)
	rev := RevisionIDFromHistoryPath(archiveRel)
	if id == "" || rev == "" {
		return ""
	}
	return id + "/" + rev
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

	ctx := context.Background()
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

	// No revision is archived: importance is not a content change. The early return matters for the
	// same reason — a no-op promote must not produce a write, or every idempotent call would add a
	// dataset version to a store whose version history is the recovery path D2 accepted.
	if IsImportantContent(data) == promote {
		return nil
	}

	if err := m.putMarkdown(ctx, tbl, relPath, withImportantFlag(data, promote)); err != nil {
		return fmt.Errorf("storing the relevance change: %w", err)
	}

	verb := "promote"
	if !promote {
		verb = "demote"
	}

	if err := m.syncWikiAfterWrite(); err != nil {
		m.log().Warn("post-write sync failed", "op", verb, "id", id, "error", err)
	}
	return nil
}

// ListMemories is the catalogue of what this scope knows: its LIVE memories, never the archived
// revisions of them.
//
// It reads the store rather than the compiled wiki, which is what makes it the read that cannot be
// behind: a memory written a moment ago is here before search can see it, and that difference is
// documented in the memory protocol as the reason to list when confirming a write.
//
// The id no longer needs recovering. It used to be taken from the file name and then corrected from
// the file's own frontmatter — `MemoryIDFor(data, name)` — because a name could disagree with what
// the memory declared, and that disagreement is what forked 184 memories into twins. A row has one
// id and it is the declared one.
func (m *MemoryService) ListMemories() ([]MemoryEntry, error) {
	ctx := context.Background()
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
	if err := m.IndexMemories(context.Background()); err != nil {
		m.log().Warn("wiki indexing failed", "scope", m.scope, "scopeID", m.scopeID, "error", err)
	}
	return nil
}

// syncWikiAfterWrite keeps the write paths explicit about the derived work they trigger.
func (m *MemoryService) syncWikiAfterWrite() error {
	return m.SyncWiki()
}

func (m *MemoryService) IndexMemories(ctx context.Context) error {
	// THE REPAIR PASS IS GONE, and it is gone because the defect it healed cannot happen.
	//
	// It folded twin memories — 184 of them — that a write path had forked by recovering an id from
	// a FILE NAME. A row is keyed by the id the memory declares, so two twins collapse into one row
	// on upsert and there is nothing left to fold. It ran on every index because the twins lived in
	// the shared bucket and each clone had to heal itself; a defect that cannot be expressed needs
	// no funnel.
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

	// Next is the path of the version that replaced this one, relative to the scope's directory.
	// It is set only on an ARCHIVED revision, and it is the other half of Previous: the two make
	// the chain walkable in both directions. Its value is another archive's path, or `<id>.md`
	// when the thing that replaced this revision is the live memory itself.
	//
	// A live memory has no Next, and that absence is its definition: it is the head of its chain.
	Next string `yaml:"next,omitempty"`

	// RevisionID is an archived revision's own address — the stem of its file under
	// `history/<id>/`. It is empty on a live memory, which is addressed by ID.
	//
	// ID stays the CHAIN id on an archive, deliberately: it is what lets a search hit on an old
	// revision name the current one without walking anything, and what lets two hits from the
	// same chain be recognised as one memory.
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

// ParseMemoryFrontmatterOK is ParseMemoryFrontmatter with the outcome reported.
//
// A false return is a memory whose classification could not be read AT ALL — not a memory with
// missing fields. Any caller that intends to WRITE the file back must check it, because
// re-rendering from a failed parse replaces a full frontmatter with an empty one, and the file
// stays valid markdown while its type, importance, tags and timestamps are gone.
//
// The recovering pass exists because that failure is not hypothetical. A title is a free-text
// sentence, and a sentence containing `: ` is not valid YAML unless it is quoted — so every memory
// whose title reads like "Telemetria do Hub: eventos vão para refs/events/*" was unreadable, and
// silently so: measured at 47 files in this repository's own project scope. Writes go through
// yaml.Marshal and are quoted correctly, so this recovers legacy files rather than excusing new
// ones.
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

// frontmatterScalarKeys are the frontmatter keys whose value is a free-text scalar, and therefore
// the keys whose value can break the block by containing a colon.
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
	fm, parsed := ParseMemoryFrontmatterOK(oldContent)
	// SAFETY: an unreadable frontmatter must not be replaced by an empty one. The title is the one
	// field recoverable from the body, and keeping it is what stops the memory becoming anonymous.
	if !parsed {
		fm.Title = firstH1(oldContent)
	}
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
	// The result of an update is always the head of its chain, so it carries neither of the two
	// fields that mark a superseded revision. Clearing them matters when the source content came
	// from an archive — a repair promoting an orphaned revision back to being the live memory.
	fm.Next = ""
	fm.RevisionID = ""

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

// archivedRevisionContent turns a live memory's content into the archived revision of it,
// preserving every classification field and adding the two that only an archive has.
//
// It is a pure function for the same reason updatedMemoryContent is: the preservation rule is
// the part that breaks silently, and it has to be testable without a store.
func archivedRevisionContent(chainID, content, revisionID, nextPath string) string {
	fm, parsed := ParseMemoryFrontmatterOK(content)
	if !parsed {
		fm.Title = firstH1(content)
	}
	// The chain id is imposed rather than inherited. On a normal update the two already agree, but
	// the repair path archives content whose declared id is the corrupted one it was forked
	// under — and carrying that into the archive would put a revision in the chain that does not
	// name the chain.
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
		// SAFETY: promotion is a one-field change. Re-rendering from a failed parse would trade
		// the whole classification for it, so the change is refused instead.
		return content
	}
	fm.Important = important
	return renderMemoryFile(fm, extractBodyAfterFrontmatter(content))
}

// firstH1 recovers a title from the body of a memory whose frontmatter cannot be read.
func firstH1(content string) string {
	for _, line := range strings.Split(content, "\n") {
		if strings.HasPrefix(line, "# ") {
			return strings.TrimSpace(strings.TrimPrefix(line, "# "))
		}
	}
	return ""
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

// sameMemoryBody compares two memories by BODY only, ignoring their frontmatter.
//
// It survived the retirement of repair.go because the question it answers is not about repair: two
// revisions of one memory differ in their frontmatter on every write — revision, updated_at,
// previous — so "did the text actually change" needs the frontmatter excluded.
func sameMemoryBody(a, b string) bool {
	return strings.TrimSpace(extractBodyAfterFrontmatter(a)) ==
		strings.TrimSpace(extractBodyAfterFrontmatter(b))
}
