package memory

import (
	"context"
	"encoding/json"
	"fmt"
	"net/url"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

const memoryTableName = "memories"

var memoryReadPageSize = 1024

// MemoryRecord is one memory, live or archived, with every field the markdown file carried.
//
// The field list is the complete persistence contract; authored metadata must not depend on the
// compiled wiki projection.
type MemoryRecord struct {
	// ID is the chain identity: a live memory and every archived revision of it share one.
	ID string
	// RevisionID is an archived revision's own address, empty on a live memory. That emptiness is
	// the definition of the head of a chain.
	RevisionID string
	// Superseded marks an archived revision explicitly.
	Superseded bool

	Title string
	Body  string
	Type  string
	Tags  []string

	Important bool
	Mandatory bool
	CreatedAt string
	UpdatedAt string
	Revision  int
	UpdatedBy string

	// Previous and Next are logical addresses in the two-way revision chain.
	Previous string
	Next     string

	Scope     string
	ScopeID   string
	ProjectID string

	ContentHash string

	// Embedding is the memory's vector, carried in the store so it TRAVELS.
	//
	// It travels with the authoritative table; no embedding sidecar exists.
	Embedding []float32
}

func (r MemoryRecord) Key() string {
	if r.RevisionID == "" {
		return r.ID
	}
	return r.ID + "/" + r.RevisionID
}

// Markdown renders the record back into the file format.
//
// It is the presentation form returned by memory reads. The store persists the fields as columns.
func (r MemoryRecord) Markdown() string {
	return renderMemoryFile(MemoryFrontmatter{
		ID:         r.ID,
		Title:      r.Title,
		Scope:      r.Scope,
		ScopeID:    r.ScopeID,
		ProjectID:  r.ProjectID,
		Type:       r.Type,
		Important:  r.Important,
		Mandatory:  r.Mandatory,
		CreatedAt:  r.CreatedAt,
		UpdatedAt:  r.UpdatedAt,
		Revision:   r.Revision,
		UpdatedBy:  r.UpdatedBy,
		Previous:   r.Previous,
		Next:       r.Next,
		RevisionID: r.RevisionID,
		Tags:       r.Tags,
	}, r.Body)
}

func memoryTableSchema(vectorDim int) lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString},
		{Name: "id", Type: lancestore.FieldString},
		{Name: "revision_id", Type: lancestore.FieldString, Nullable: true},
		{Name: "superseded", Type: lancestore.FieldBool},
		{Name: "title", Type: lancestore.FieldString, Nullable: true},
		{Name: "body", Type: lancestore.FieldString, Nullable: true},
		{Name: "type", Type: lancestore.FieldString, Nullable: true},
		{Name: "tags_json", Type: lancestore.FieldString, Nullable: true},
		{Name: "important", Type: lancestore.FieldBool},
		{Name: "mandatory", Type: lancestore.FieldBool},
		{Name: "created_at", Type: lancestore.FieldString, Nullable: true},
		{Name: "updated_at", Type: lancestore.FieldString, Nullable: true},
		{Name: "revision", Type: lancestore.FieldInt64},
		{Name: "updated_by", Type: lancestore.FieldString, Nullable: true},
		{Name: "previous", Type: lancestore.FieldString, Nullable: true},
		{Name: "next", Type: lancestore.FieldString, Nullable: true},
		{Name: "scope", Type: lancestore.FieldString, Nullable: true},
		{Name: "scope_id", Type: lancestore.FieldString, Nullable: true},
		{Name: "project_id", Type: lancestore.FieldString, Nullable: true},
		{Name: "content_hash", Type: lancestore.FieldString, Nullable: true},
		{Name: "embedding", Type: lancestore.FieldVector, Dim: vectorDim, Nullable: true},
	}}
}

func memoryTableIndexes() []lancestore.Index {
	return []lancestore.Index{
		{Column: "key", Kind: lancestore.IndexScalarBTree},
		{Column: "id", Kind: lancestore.IndexScalarBTree},
		{Column: "superseded", Kind: lancestore.IndexScalarBitmap},
		{Column: "important", Kind: lancestore.IndexScalarBitmap},
		{Column: "mandatory", Kind: lancestore.IndexScalarBitmap},
	}
}

// MemoryTable is one scope's store.
type MemoryTable struct {
	store *lancestore.Store
	table *lancestore.Table
}

// OpenMemoryTable opens — creating if needed — the store of one scope.
//
// `uri` is a local directory or an `s3://bucket/prefix`. A remote URI is opened WRITABLE, which is
// the whole point: a memory scope in a bucket is extended by every unit that shares it, and it is
// not a published artifact somebody else owns. That distinction is why `lancestore.Config.Writable`
// exists rather than the permission being derived from the scheme.
func OpenMemoryTable(ctx context.Context, uri string) (*MemoryTable, error) {
	if err := authorizeMemoryURI(ctx, uri); err != nil {
		return nil, err
	}
	st, err := lancestore.Open(ctx, lancestore.Config{
		URI:      uri,
		S3:       config.HubS3Config(),
		Writable: true,
	})
	if err != nil {
		return nil, fmt.Errorf("opening the memory store at %s: %w", uri, err)
	}
	expected := memoryTableSchema(ai.ResolveConfiguredEmbeddingDimensions())
	tbl, err := st.EnsureTable(ctx, memoryTableName, expected)
	if err != nil {
		_ = st.Close()
		return nil, fmt.Errorf("opening the memories table at %s: %w", uri, err)
	}
	if !tbl.Schema().Equal(expected) {
		_ = tbl.Close()
		if err := st.DropTable(ctx, memoryTableName); err != nil {
			_ = st.Close()
			return nil, fmt.Errorf("resetting incompatible memories table at %s: %w", uri, err)
		}
		tbl, err = st.CreateTable(ctx, memoryTableName, expected)
		if err != nil {
			_ = st.Close()
			return nil, fmt.Errorf("recreating memories table at %s: %w", uri, err)
		}
	}
	return &MemoryTable{store: st, table: tbl}, nil
}

func authorizeMemoryURI(ctx context.Context, uri string) error {
	if !strings.HasPrefix(uri, "s3://") {
		return nil
	}
	cfg := config.HubS3Config()
	parsed, err := url.Parse(uri)
	if err != nil || parsed.Scheme != "s3" || parsed.Host != cfg.Bucket || !cfg.Configured() {
		return fmt.Errorf("remote memory URI is outside the configured Hub")
	}
	key := strings.TrimPrefix(parsed.Path, "/")
	objects, err := s3store.New(ctx, cfg)
	if err != nil {
		return err
	}
	projectRoot := s3store.JoinKey(cfg.Prefix, hubaccess.VersionPrefix, "projects") + "/"
	if strings.HasPrefix(key, projectRoot) {
		projectID := strings.SplitN(strings.TrimPrefix(key, projectRoot), "/", 2)[0]
		if key != s3store.JoinKey(cfg.Prefix, hubaccess.ProjectMemoryPrefix(projectID)) {
			return fmt.Errorf("remote memory URI is not a project memory prefix")
		}
		return hubaccess.AuthorizeProject(ctx, objects, projectID)
	}
	userRoot := s3store.JoinKey(cfg.Prefix, hubaccess.VersionPrefix, "users") + "/"
	if strings.HasPrefix(key, userRoot) {
		userID := strings.SplitN(strings.TrimPrefix(key, userRoot), "/", 2)[0]
		if key != s3store.JoinKey(cfg.Prefix, hubaccess.UserMemoryPrefix(userID)) {
			return fmt.Errorf("remote memory URI is not a user memory prefix")
		}
		subject, err := hubaccess.TrustedSubject(ctx)
		if err != nil {
			return err
		}
		if subject.UserID != userID {
			return fmt.Errorf("%w: user memory %s", hubaccess.ErrDenied, userID)
		}
		return nil
	}
	return fmt.Errorf("remote memory URI is outside Hub v2 memory namespaces")
}

// Close releases the store.
func (t *MemoryTable) Close() error {
	if t == nil || t.store == nil {
		return nil
	}
	return t.store.Close()
}

// Count is how many records the store holds, live and archived.
func (t *MemoryTable) Count(ctx context.Context) (int64, error) {
	return t.table.Count(ctx)
}

// Put writes records, replacing any that share a key.
//
// `Upsert` is one atomic merge, so a rewrite cannot expose a missing head between two commits.
func (t *MemoryTable) Put(ctx context.Context, records ...MemoryRecord) error {
	if len(records) == 0 {
		return nil
	}
	rows := make([]lancestore.Row, 0, len(records))
	for _, r := range records {
		row, err := memoryRow(r)
		if err != nil {
			return err
		}
		rows = append(rows, row)
	}
	if err := t.table.Upsert(ctx, "key", rows); err != nil {
		return fmt.Errorf("writing %d memory record(s): %w", len(rows), err)
	}
	return nil
}

// Delete removes records by key. A key that is not there is not an error: the caller's intent is
// that it must not be there.
func (t *MemoryTable) Delete(ctx context.Context, keys ...string) error {
	if len(keys) == 0 {
		return nil
	}
	if err := t.table.DeleteByKey(ctx, "key", keys); err != nil {
		return fmt.Errorf("deleting %d memory record(s): %w", len(keys), err)
	}
	return nil
}

// Get reads one record by key.
func (t *MemoryTable) Get(ctx context.Context, key string) (MemoryRecord, bool, error) {
	hits, err := t.table.Search(ctx, lancestore.Query{
		Filter: fmt.Sprintf("key = %s", sqlQuoteMemory(key)), Limit: 1,
	})
	if err != nil {
		return MemoryRecord{}, false, fmt.Errorf("reading the memory %q: %w", key, err)
	}
	if len(hits) == 0 {
		return MemoryRecord{}, false, nil
	}
	return recordFromRow(hits[0].Row), true, nil
}

// List reads every record, live memories first and then archived revisions, each group ordered by
// key so the answer is stable across calls.
func (t *MemoryTable) List(ctx context.Context) ([]MemoryRecord, error) {
	out, err := t.readAll(ctx, "revision >= 0")
	if err != nil {
		return nil, fmt.Errorf("listing the memory store: %w", err)
	}
	sortMemoryRecords(out)
	return out, nil
}

// Live reads the current memories, excluding archived revisions — the catalogue of what this scope
// knows, which is what every listing surface wants.
func (t *MemoryTable) Live(ctx context.Context) ([]MemoryRecord, error) {
	out, err := t.readAll(ctx, "superseded = false")
	if err != nil {
		return nil, fmt.Errorf("listing the live memories: %w", err)
	}
	sortMemoryRecords(out)
	return out, nil
}

// Mandatory reads the unconditional session-start set directly from the authoritative table.
func (t *MemoryTable) Mandatory(ctx context.Context) ([]MemoryRecord, error) {
	out, err := t.readAll(ctx, "superseded = false AND mandatory = true")
	if err != nil {
		return nil, fmt.Errorf("listing mandatory memories: %w", err)
	}
	sortMemoryRecords(out)
	return out, nil
}

func (t *MemoryTable) Revisions(ctx context.Context, id string) ([]MemoryRecord, error) {
	out, err := t.readAll(ctx, fmt.Sprintf("id = %s AND superseded = true", sqlQuoteMemory(id)))
	if err != nil {
		return nil, fmt.Errorf("reading the revisions of %q: %w", id, err)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].RevisionID < out[j].RevisionID })
	return out, nil
}

func (t *MemoryTable) readAll(ctx context.Context, filter string) ([]MemoryRecord, error) {
	pageSize := memoryReadPageSize
	if pageSize < 1 {
		pageSize = 1024
	}
	out := make([]MemoryRecord, 0, pageSize)
	for offset := 0; ; offset += pageSize {
		hits, err := t.table.Search(ctx, lancestore.Query{Filter: filter, Limit: pageSize, Offset: offset})
		if err != nil {
			return nil, err
		}
		for _, h := range hits {
			out = append(out, recordFromRow(h.Row))
		}
		if len(hits) < pageSize {
			return out, nil
		}
	}
}

func (t *MemoryTable) Maintain(ctx context.Context) error {
	if err := t.table.FoldNewRowsIntoIndexes(ctx); err != nil {
		return err
	}
	if _, err := t.table.Compact(ctx); err != nil {
		return err
	}
	_, err := t.table.PruneVersions(ctx, config.MemoryVersionRetention())
	return err
}

// EnsureIndexes builds the scalar indexes. It is separate from opening because a fresh store has no
// rows to index.
func (t *MemoryTable) EnsureIndexes(ctx context.Context) error {
	return t.table.EnsureIndexes(ctx, memoryTableIndexes()...)
}

func sortMemoryRecords(rs []MemoryRecord) {
	sort.Slice(rs, func(i, j int) bool {
		if rs[i].Superseded != rs[j].Superseded {
			return rs[j].Superseded
		}
		return rs[i].Key() < rs[j].Key()
	})
}

func memoryRow(r MemoryRecord) (lancestore.Row, error) {
	if r.ID == "" {
		return nil, fmt.Errorf("a memory record needs an id")
	}
	tags := ""
	if len(r.Tags) > 0 {
		raw, err := json.Marshal(r.Tags)
		if err != nil {
			return nil, fmt.Errorf("encoding the tags of %q: %w", r.Key(), err)
		}
		tags = string(raw)
	}
	revision := r.Revision
	if revision < 1 {
		revision = 1
	}
	row := lancestore.Row{
		"key":          r.Key(),
		"id":           r.ID,
		"revision_id":  r.RevisionID,
		"superseded":   r.Superseded,
		"title":        r.Title,
		"body":         r.Body,
		"type":         r.Type,
		"tags_json":    tags,
		"important":    r.Important,
		"mandatory":    r.Mandatory,
		"created_at":   r.CreatedAt,
		"updated_at":   r.UpdatedAt,
		"revision":     int64(revision),
		"updated_by":   r.UpdatedBy,
		"previous":     r.Previous,
		"next":         r.Next,
		"scope":        r.Scope,
		"scope_id":     r.ScopeID,
		"project_id":   r.ProjectID,
		"content_hash": r.ContentHash,
		"embedding":    nil,
	}
	if len(r.Embedding) > 0 {
		row["embedding"] = r.Embedding
	}
	return row, nil
}

func recordFromRow(row map[string]any) MemoryRecord {
	r := MemoryRecord{
		ID:          memStr(row["id"]),
		RevisionID:  memStr(row["revision_id"]),
		Superseded:  memBool(row["superseded"]),
		Title:       memStr(row["title"]),
		Body:        memStr(row["body"]),
		Type:        memStr(row["type"]),
		Important:   memBool(row["important"]),
		Mandatory:   memBool(row["mandatory"]),
		CreatedAt:   memStr(row["created_at"]),
		UpdatedAt:   memStr(row["updated_at"]),
		Revision:    int(memI64(row["revision"])),
		UpdatedBy:   memStr(row["updated_by"]),
		Previous:    memStr(row["previous"]),
		Next:        memStr(row["next"]),
		Scope:       memStr(row["scope"]),
		ScopeID:     memStr(row["scope_id"]),
		ProjectID:   memStr(row["project_id"]),
		ContentHash: memStr(row["content_hash"]),
		Embedding:   memVector(row["embedding"]),
	}
	if raw := memStr(row["tags_json"]); raw != "" {
		_ = json.Unmarshal([]byte(raw), &r.Tags)
	}
	return r
}

func recordFromMarkdown(rel string, data []byte) (MemoryRecord, bool) {
	content := string(data)
	fm, parsed := ParseMemoryFrontmatterOK(content)
	if !parsed {
		return MemoryRecord{}, false
	}

	superseded := isHistorySource(rel) || fm.IsArchivedRevision()

	id := fm.ID
	if id == "" {
		if superseded {
			id = chainIDFromHistoryPath(rel)
		} else {
			id = MemoryIDFromFileName(rel)
		}
	}
	revisionID := fm.RevisionID
	if superseded && revisionID == "" {
		revisionID = RevisionIDFromHistoryPath(rel)
	}

	return MemoryRecord{
		ID:          id,
		RevisionID:  revisionID,
		Superseded:  superseded,
		Title:       fm.Title,
		Body:        extractBodyAfterFrontmatter(content),
		Type:        fm.Type,
		Tags:        fm.Tags,
		Important:   fm.Important,
		Mandatory:   fm.Mandatory,
		CreatedAt:   fm.CreatedAt,
		UpdatedAt:   fm.UpdatedAt,
		Revision:    fm.Revision,
		UpdatedBy:   fm.UpdatedBy,
		Previous:    fm.Previous,
		Next:        fm.Next,
		Scope:       fm.Scope,
		ScopeID:     fm.ScopeID,
		ProjectID:   fm.ProjectID,
		ContentHash: wiki.ContentHash(data),
	}, id != ""
}

// chainIDFromHistoryPath is the chain a revision belongs to, taken from its directory.
//
// For an archive the id fallback is the chain DIRECTORY, never the file's own name: the name is the
// revision's address, and using it as the memory id is precisely the mistake that forked 184
// memories into twins.
func chainIDFromHistoryPath(rel string) string {
	parts := strings.Split(strings.Trim(strings.ReplaceAll(rel, "\\", "/"), "/"), "/")
	if len(parts) < 2 {
		return ""
	}
	return parts[len(parts)-2]
}

func sqlQuoteMemory(s string) string {
	return "'" + strings.ReplaceAll(s, "'", "''") + "'"
}

func memStr(v any) string {
	s, _ := v.(string)
	return s
}

func memBool(v any) bool {
	b, _ := v.(bool)
	return b
}

func memI64(v any) int64 {
	switch n := v.(type) {
	case int64:
		return n
	case int:
		return int64(n)
	case float64:
		return int64(n)
	}
	return 0
}

func memVector(v any) []float32 {
	switch vec := v.(type) {
	case []float32:
		if len(vec) == 0 {
			return nil
		}
		return vec
	case []float64:
		out := make([]float32, len(vec))
		for i, f := range vec {
			out[i] = float32(f)
		}
		return out
	}
	return nil
}
