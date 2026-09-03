package wiki

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"path/filepath"
	"reflect"
	"slices"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// The wiki index, on LanceDB.
//
// A WIKI HAS NO GRAPH, which is the whole difference from the AST side: there is one store here,
// not two. Cross-references look like they need a graph and do not — see FindXRefs, whose
// traversal is Go walking one-hop lookups, exactly as the SQLite version's was. What it needs from
// storage is a filterable table of pairs, and that is a column store's easiest case.
//
// FOUR TABLES, replacing five SQLite ones. `chunk_emb` disappears into a column of `chunks`, for
// the same reason the AST index dropped its separate vector table: an embedding that lives beside
// its chunk cannot outlive it, so the failure where a stale vector answers for a page that no
// longer exists stops being expressible rather than being defended against.

// WikiIndexDirName is the index directory inside a wiki.
const WikiIndexDirName = "index.lance"

const (
	lanceChunksTable  = "chunks"
	lanceXRefsTable   = "xrefs"
	lanceSyncLogTable = "sync_log"
	lanceMetaTable    = "meta"

	lanceWikiBody  = "body"
	lanceWikiTerms = "search_terms"

	lanceWikiVector = "embedding"
)

// WikiDB is a wiki's index.
type WikiDB struct {
	store   *lancestore.Store
	chunks  *lancestore.Table
	xrefs   *lancestore.Table
	syncLog *lancestore.Table
	meta    *lancestore.Table

	Logger *slog.Logger
}

func (w *WikiDB) log() *slog.Logger { return slogutil.Resolve(w.Logger) }

// WikiIndexPath is where a wiki's index lives.
func WikiIndexPath(wikiDir string) string { return filepath.Join(wikiDir, WikiIndexDirName) }

// OpenWikiDB opens — creating if needed — the index of a wiki directory.
func OpenWikiDB(ctx context.Context, wikiDir string) (*WikiDB, error) {
	return OpenWikiDBAt(ctx, lancestore.Config{URI: WikiIndexPath(wikiDir)})
}

// OpenWikiDBAt opens an index by URI, which is how a PUBLISHED wiki on S3 is read: the engine
// queries the objects in place and no page file is ever downloaded.
func OpenWikiDBAt(ctx context.Context, cfg lancestore.Config) (*WikiDB, error) {
	st, err := lancestore.Open(ctx, cfg)
	if err != nil {
		return nil, fmt.Errorf("open wiki index: %w", err)
	}
	return &WikiDB{store: st}, nil
}

// Close releases the store.
func (w *WikiDB) Close() error {
	if w == nil || w.store == nil {
		return nil
	}
	return w.store.Close()
}

// DBPath is where the index lives, for callers that report or replicate it.
func (w *WikiDB) DBPath() string {
	if w == nil || w.store == nil {
		return ""
	}
	return w.store.URI()
}

// Compact merges the fragments a burst of writes leaves behind. Nothing depends on it for
// correctness — a write is already durable and complete without it, because Lance produces a new
// immutable version rather than a file plus a log to fold in.
func (w *WikiDB) Compact(ctx context.Context) error {
	if err := w.ensureTables(ctx); err != nil {
		return err
	}
	_, err := w.chunks.Compact(ctx)
	return err
}

func lanceWikiVersionRetention() time.Duration { return config.WikiVersionRetention() }

// Maintain reclaims the disk a rebuild leaves behind: dead rows still occupying their fragments,
// and superseded versions kept for a time travel nobody performs.
//
// Best-effort by design — every failure here costs disk, not correctness — so it logs rather
// than returning. A wiki rebuild replaces the whole chunk set, which is exactly the write that
// leaves a full superseded copy behind.
func (w *WikiDB) Maintain(ctx context.Context) {
	if w.Remote() {
		return
	}
	if err := w.ensureTables(ctx); err != nil {
		w.log().Warn("wiki index maintenance skipped: tables unavailable", "error", err)
		return
	}
	for _, t := range []struct {
		name  string
		table *lancestore.Table
	}{
		{lanceChunksTable, w.chunks},
		{lanceXRefsTable, w.xrefs},
		{lanceSyncLogTable, w.syncLog},
		{lanceMetaTable, w.meta},
	} {
		if t.table == nil {
			continue
		}
		t0 := time.Now()
		if res, err := t.table.Compact(ctx); err != nil {
			w.log().Warn("compacting the wiki index", "table", t.name, "error", err)
		} else if res.FragmentsRemoved > 0 {
			w.log().Info("wiki index compacted", "table", t.name,
				"fragments_removed", res.FragmentsRemoved, "fragments_added", res.FragmentsAdded,
				"duration_ms", time.Since(t0).Milliseconds())
		}
		if res, err := t.table.PruneVersions(ctx, lanceWikiVersionRetention()); err != nil {
			w.log().Warn("pruning superseded wiki index versions", "table", t.name, "error", err)
		} else if res.OldVersions > 0 {
			w.log().Info("superseded wiki index versions pruned", "table", t.name,
				"versions", res.OldVersions, "bytes_reclaimed", res.BytesRemoved)
		}
	}
}

// Remote reports whether this is a published index, which is read-only.
func (w *WikiDB) Remote() bool { return w.store != nil && w.store.Remote() }

func lanceChunksSchema(vectorDim int) lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "slug", Type: lancestore.FieldString},
		{Name: "title", Type: lancestore.FieldString},
		{Name: "summary", Type: lancestore.FieldString, Nullable: true},
		{Name: "body", Type: lancestore.FieldString, Nullable: true},
		{Name: "doc_type", Type: lancestore.FieldString, Nullable: true},
		{Name: "source", Type: lancestore.FieldString, Nullable: true},
		{Name: "breadcrumb", Type: lancestore.FieldString, Nullable: true},
		{Name: "cluster_id", Type: lancestore.FieldInt64},
		{Name: "cluster_name", Type: lancestore.FieldString, Nullable: true},
		{Name: "confidence", Type: lancestore.FieldFloat64},
		{Name: "content_hash", Type: lancestore.FieldString, Nullable: true},
		{Name: "word_count", Type: lancestore.FieldInt64},
		{Name: "updated", Type: lancestore.FieldString, Nullable: true},
		{Name: "important", Type: lancestore.FieldBool},
		{Name: "mandatory", Type: lancestore.FieldBool},
		{Name: "tags_json", Type: lancestore.FieldString, Nullable: true},
		{Name: "entity_id", Type: lancestore.FieldString, Nullable: true},
		{Name: "revision_id", Type: lancestore.FieldString, Nullable: true},
		{Name: "superseded", Type: lancestore.FieldBool},
		{Name: "current_id", Type: lancestore.FieldString, Nullable: true},
		{Name: "revision", Type: lancestore.FieldInt64},
		{Name: "previous", Type: lancestore.FieldString, Nullable: true},
		{Name: "next", Type: lancestore.FieldString, Nullable: true},
		{Name: "created", Type: lancestore.FieldString, Nullable: true},
		{Name: "stale_since", Type: lancestore.FieldString, Nullable: true},
		{Name: "stale_reason", Type: lancestore.FieldString, Nullable: true},
		{Name: lanceWikiTerms, Type: lancestore.FieldString},
		{Name: lanceWikiVector, Type: lancestore.FieldVector, Dim: vectorDim, Nullable: true},
	}}
}

func lanceXRefsSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "source_slug", Type: lancestore.FieldString},
		{Name: "target_slug", Type: lancestore.FieldString},
		{Name: "ref_type", Type: lancestore.FieldString, Nullable: true},
	}}
}

func lanceSyncLogSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "timestamp", Type: lancestore.FieldString},
		{Name: "total_docs", Type: lancestore.FieldInt64},
		{Name: "articles_written", Type: lancestore.FieldInt64},
		{Name: "backlinks_added", Type: lancestore.FieldInt64},
		{Name: "added_json", Type: lancestore.FieldString, Nullable: true},
		{Name: "updated_json", Type: lancestore.FieldString, Nullable: true},
		{Name: "deleted_json", Type: lancestore.FieldString, Nullable: true},
		{Name: "details_json", Type: lancestore.FieldString, Nullable: true},
	}}
}

func lanceMetaSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "key", Type: lancestore.FieldString},
		{Name: "value", Type: lancestore.FieldString, Nullable: true},
	}}
}

const lanceWikiMinRowsForVectorIndex = 256

func lanceChunkIndexes() []lancestore.Index {
	return []lancestore.Index{
		{Column: lanceWikiBody, Kind: lancestore.IndexInvertedText},
		{Column: lanceWikiTerms, Kind: lancestore.IndexInvertedText},
		{Column: lanceWikiVector, Kind: lancestore.IndexVectorIVFPQ},
		{Column: "slug", Kind: lancestore.IndexScalarBTree},
		{Column: "doc_type", Kind: lancestore.IndexScalarBitmap},
		{Column: "superseded", Kind: lancestore.IndexScalarBitmap},
		{Column: "mandatory", Kind: lancestore.IndexScalarBitmap},
		{Column: "entity_id", Kind: lancestore.IndexScalarBTree},
	}
}

func wikiSearchTerms(c WikiChunk) string {
	parts := []string{c.Title, splitWikiSlug(c.Slug), c.Summary, c.Breadcrumb, c.DocType, strings.Join(c.Tags, " ")}
	parts = append(parts, wikiGramBag(c.Title, c.Slug))
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		if strings.TrimSpace(p) != "" {
			out = append(out, p)
		}
	}
	return strings.Join(out, " ")
}

func splitWikiSlug(slug string) string {
	return strings.Join(strings.FieldsFunc(slug, func(r rune) bool {
		return r == '-' || r == '_' || r == '.' || r == '/'
	}), " ")
}

func wikiGramBag(title, slug string) string {
	seen := map[string]bool{}
	var out []string
	for _, src := range []string{title, splitWikiSlug(slug)} {
		for _, w := range strings.Fields(strings.ToLower(src)) {
			r := []rune(w)
			for _, n := range []int{2, 3} {
				for i := 0; i+n <= len(r); i++ {
					g := string(r[i : i+n])
					if !seen[g] {
						seen[g] = true
						out = append(out, g)
					}
				}
			}
		}
	}
	return strings.Join(out, " ")
}

// WikiQueryText expands a query the way the document was expanded, so both sides agree.
func WikiQueryText(query string) string {
	seen := map[string]bool{}
	var out []string
	add := func(s string) {
		if s != "" && !seen[s] {
			seen[s] = true
			out = append(out, s)
		}
	}
	for _, t := range strings.Fields(strings.ToLower(query)) {
		add(t)
		r := []rune(t)
		for _, n := range []int{2, 3} {
			for i := 0; i+n <= len(r); i++ {
				add(string(r[i : i+n]))
			}
		}
	}
	return strings.Join(out, " ")
}

func buildChunkRow(c WikiChunk, vec []float32) lancestore.Row {
	tagsJSON := ""
	if len(c.Tags) > 0 {
		if raw, err := json.Marshal(c.Tags); err == nil {
			tagsJSON = string(raw)
		}
	}
	row := lancestore.Row{
		"slug":          c.Slug,
		"title":         c.Title,
		"summary":       c.Summary,
		"body":          c.Body,
		"doc_type":      c.DocType,
		"source":        c.Source,
		"breadcrumb":    c.Breadcrumb,
		"cluster_id":    int64(c.ClusterID),
		"cluster_name":  c.ClusterName,
		"confidence":    c.Confidence,
		"content_hash":  c.ContentHash,
		"word_count":    int64(c.WordCount),
		"updated":       c.Updated,
		"important":     c.Important,
		"mandatory":     c.Mandatory,
		"tags_json":     tagsJSON,
		"entity_id":     c.EntityID,
		"revision_id":   c.RevisionID,
		"superseded":    c.Superseded,
		"current_id":    c.CurrentID,
		"revision":      int64(c.Revision),
		"previous":      c.Previous,
		"next":          c.Next,
		"created":       c.Created,
		"stale_since":   c.StaleSince,
		"stale_reason":  c.StaleReason,
		lanceWikiTerms:  wikiSearchTerms(c),
		lanceWikiVector: nil,
	}
	if len(vec) > 0 {
		row[lanceWikiVector] = vec
	}
	return row
}

const lanceWikiWriteBatch = 500

// Sync makes the stored wiki equal to the compiled corpus without replacing any table.
//
// Slug is the document key. New and changed rows are upserted, missing slugs are deleted, and
// unchanged rows are not touched. Vectors survive any derived-field update whose content hash is
// unchanged. Cross-references are updated by source slug with the same rule.
func (w *WikiDB) Sync(ctx context.Context, chunks []WikiChunk, xrefs map[string][]string,
	logEntry *SyncLogEntry) error {
	if w.Remote() {
		return lancestore.ErrReadOnly
	}
	t0 := time.Now()
	if err := w.ensureTables(ctx); err != nil {
		return err
	}

	existing, err := w.Chunks(ctx)
	if err != nil {
		return fmt.Errorf("reading current wiki rows: %w", err)
	}
	existingBySlug := make(map[string]WikiChunk, len(existing))
	for _, c := range existing {
		existingBySlug[c.Slug] = c
	}
	desiredBySlug := make(map[string]WikiChunk, len(chunks))
	for _, c := range chunks {
		desiredBySlug[c.Slug] = c
	}

	var deleted []string
	for slug := range existingBySlug {
		if _, ok := desiredBySlug[slug]; !ok {
			deleted = append(deleted, slug)
		}
	}
	if err := w.chunks.DeleteByKey(ctx, "slug", deleted); err != nil {
		return fmt.Errorf("deleting %d wiki rows: %w", len(deleted), err)
	}

	embeddings := make(map[string][]float32)
	if stored, serr := w.StoredEmbeddings(ctx); serr == nil {
		for _, e := range stored {
			if len(e.Vector) > 0 {
				embeddings[e.ContentHash] = e.Vector
			}
		}
	}

	changedRows := make([]lancestore.Row, 0)
	for _, c := range chunks {
		if old, ok := existingBySlug[c.Slug]; ok && wikiChunksEqual(old, c) {
			continue
		}
		changedRows = append(changedRows, buildChunkRow(c, embeddings[c.ContentHash]))
	}
	changedCount := len(changedRows)
	for len(changedRows) > 0 {
		n := min(len(changedRows), lanceWikiWriteBatch)
		if err := w.chunks.Upsert(ctx, "slug", changedRows[:n]); err != nil {
			return fmt.Errorf("upserting %d wiki rows: %w", n, err)
		}
		changedRows = changedRows[n:]
	}

	currentRefs, err := w.AllXRefs(ctx)
	if err != nil {
		return fmt.Errorf("reading current cross-references: %w", err)
	}
	refSources := make(map[string]bool, len(currentRefs)+len(xrefs))
	for src := range currentRefs {
		refSources[src] = true
	}
	for src := range xrefs {
		refSources[src] = true
	}
	changedRefs := make(map[string][]string)
	var changedRefSources []string
	for src := range refSources {
		oldTargets := sortedUnique(currentRefs[src])
		newTargets := sortedUnique(xrefs[src])
		if slices.Equal(oldTargets, newTargets) {
			continue
		}
		changedRefSources = append(changedRefSources, src)
		if len(newTargets) > 0 {
			changedRefs[src] = newTargets
		}
	}
	if err := w.xrefs.DeleteByKey(ctx, "source_slug", changedRefSources); err != nil {
		return fmt.Errorf("deleting changed cross-references: %w", err)
	}
	if err := w.writeXRefs(ctx, changedRefs); err != nil {
		return err
	}

	if logEntry != nil {
		if err := w.appendSyncLog(ctx, *logEntry); err != nil {
			return err
		}
	}

	idx := lanceChunkIndexes()
	embedded, _ := w.EmbeddingStats(ctx)
	if embedded < lanceWikiMinRowsForVectorIndex {
		idx = withoutWikiColumn(idx, lanceWikiVector)
	}
	if err := w.chunks.EnsureIndexes(ctx, idx...); err != nil {
		return err
	}
	if len(deleted) > 0 || changedCount > 0 {
		if err := w.chunks.FoldNewRowsIntoIndexes(ctx); err != nil {
			return err
		}
	}

	w.log().Info("wiki index sync", "desired_chunks", len(chunks), "upserted", changedCount, "deleted", len(deleted),
		"changed_refs", len(changedRefSources), "duration_ms", time.Since(t0).Seconds()*1000)
	w.maintainIfDue(ctx)
	return nil
}

func wikiChunksEqual(a, b WikiChunk) bool {
	return reflect.DeepEqual(a, b)
}

func sortedUnique(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	out := append([]string(nil), values...)
	sort.Strings(out)
	n := 0
	for _, value := range out {
		if n == 0 || out[n-1] != value {
			out[n] = value
			n++
		}
	}
	return out[:n]
}

const wikiMaintenanceInterval = 15 * time.Minute

func (w *WikiDB) maintainIfDue(ctx context.Context) {
	const key = "last_maintenance"
	if w.meta == nil {
		return
	}
	hits, err := w.meta.Search(ctx, lancestore.Query{
		Filter: fmt.Sprintf("key = %s", lanceQuote(key)), Limit: 1,
	})
	if err == nil && len(hits) > 0 {
		if last, parseErr := time.Parse(time.RFC3339, str(hits[0].Row["value"])); parseErr == nil && time.Since(last) < wikiMaintenanceInterval {
			return
		}
	}
	w.Maintain(ctx)
	now := time.Now().UTC().Format(time.RFC3339)
	if err := w.meta.Upsert(ctx, "key", []lancestore.Row{{"key": key, "value": now}}); err != nil {
		w.log().Warn("recording wiki maintenance", "error", err)
	}
}

func withoutWikiColumn(in []lancestore.Index, column string) []lancestore.Index {
	out := make([]lancestore.Index, 0, len(in))
	for _, i := range in {
		if i.Column != column {
			out = append(out, i)
		}
	}
	return out
}

func (w *WikiDB) ensureTables(ctx context.Context) error {
	// A READ MUST NOT GO THROUGH A WRITE PATH, and this is where that was getting violated.
	//
	// EnsureTable CREATES what is missing, and creating is refused on a published store — so a
	// published wiki answered every query with "this store is read-only", which reads as a
	// permission problem and is actually a code path problem. A mounted artifact's tables exist
	// by definition: the publisher wrote them. They are OPENED.
	if w.store.Remote() {
		if err := w.openTables(ctx); err != nil {
			return err
		}
		expected := lanceChunksSchema(ai.ResolveConfiguredEmbeddingDimensions())
		if !w.chunks.Schema().Equal(expected) {
			return fmt.Errorf("published wiki has an incompatible chunks schema; republish it with the current build")
		}
		return nil
	}
	var err error
	expected := lanceChunksSchema(ai.ResolveConfiguredEmbeddingDimensions())
	if w.chunks == nil {
		if w.chunks, err = w.store.EnsureTable(ctx, lanceChunksTable, expected); err != nil {
			return err
		}
	}
	if !w.chunks.Schema().Equal(expected) {
		return w.resetTables(ctx, expected)
	}
	if w.xrefs == nil {
		if w.xrefs, err = w.store.EnsureTable(ctx, lanceXRefsTable, lanceXRefsSchema()); err != nil {
			return err
		}
	}
	if w.syncLog == nil {
		if w.syncLog, err = w.store.EnsureTable(ctx, lanceSyncLogTable, lanceSyncLogSchema()); err != nil {
			return err
		}
	}
	if w.meta == nil {
		if w.meta, err = w.store.EnsureTable(ctx, lanceMetaTable, lanceMetaSchema()); err != nil {
			return err
		}
	}
	return nil
}

func (w *WikiDB) resetTables(ctx context.Context, chunksSchema lancestore.Schema) error {
	for _, table := range []*lancestore.Table{w.chunks, w.xrefs, w.syncLog, w.meta} {
		if table != nil {
			_ = table.Close()
		}
	}
	w.chunks, w.xrefs, w.syncLog, w.meta = nil, nil, nil, nil
	for _, name := range []string{lanceChunksTable, lanceXRefsTable, lanceSyncLogTable, lanceMetaTable} {
		if err := w.store.DropTable(ctx, name); err != nil {
			return err
		}
	}
	var err error
	if w.chunks, err = w.store.CreateTable(ctx, lanceChunksTable, chunksSchema); err != nil {
		return err
	}
	if w.xrefs, err = w.store.CreateTable(ctx, lanceXRefsTable, lanceXRefsSchema()); err != nil {
		return err
	}
	if w.syncLog, err = w.store.CreateTable(ctx, lanceSyncLogTable, lanceSyncLogSchema()); err != nil {
		return err
	}
	w.meta, err = w.store.CreateTable(ctx, lanceMetaTable, lanceMetaSchema())
	return err
}

func (w *WikiDB) openTables(ctx context.Context) error {
	if w.chunks == nil {
		t, err := w.store.OpenTable(ctx, lanceChunksTable)
		if err != nil {
			return fmt.Errorf("opening the published wiki's pages: %w", err)
		}
		w.chunks = t
	}
	for _, spec := range []struct {
		name string
		dst  **lancestore.Table
	}{
		{lanceXRefsTable, &w.xrefs},
		{lanceSyncLogTable, &w.syncLog},
		{lanceMetaTable, &w.meta},
	} {
		if *spec.dst != nil {
			continue
		}
		if t, err := w.store.OpenTable(ctx, spec.name); err == nil {
			*spec.dst = t
		}
	}
	return nil
}

func (w *WikiDB) writeXRefs(ctx context.Context, xrefs map[string][]string) error {
	if len(xrefs) == 0 {
		return nil
	}
	sources := make([]string, 0, len(xrefs))
	for s := range xrefs {
		sources = append(sources, s)
	}
	sort.Strings(sources)

	rows := make([]lancestore.Row, 0, len(xrefs))
	for _, src := range sources {
		targets := append([]string(nil), xrefs[src]...)
		sort.Strings(targets)
		for _, tgt := range targets {
			rows = append(rows, lancestore.Row{
				"source_slug": src, "target_slug": tgt, "ref_type": "wikilink",
			})
			if len(rows) >= lanceWikiWriteBatch {
				if err := w.xrefs.Append(ctx, rows); err != nil {
					return fmt.Errorf("writing cross-references: %w", err)
				}
				rows = rows[:0]
			}
		}
	}
	if len(rows) > 0 {
		if err := w.xrefs.Append(ctx, rows); err != nil {
			return fmt.Errorf("writing cross-references: %w", err)
		}
	}
	return nil
}

// Search runs the keyword half.
//
// A query with no searchable terms returns NO RESULTS AND NO ERROR. "  " is a question with no
// content, not a malformed request, and a caller that shows the user an error for it is reporting
// a fault that does not exist.
func (w *WikiDB) Search(ctx context.Context, query string, topK int) ([]WikiSearchResult, error) {
	return w.SearchWithOptions(ctx, query, topK, WikiSearchOptions{})
}

// WikiSearchOptions controls predicates that must be applied before ranking. Mandatory exclusion is
// used by the second phase of session recall because phase one has already loaded those memories.
type WikiSearchOptions struct {
	ExcludeMandatory bool
}

func (w *WikiDB) SearchWithOptions(ctx context.Context, query string, topK int, opts WikiSearchOptions) ([]WikiSearchResult, error) {
	var err error
	topK, err = w.resolveSearchLimit(ctx, topK)
	if err != nil {
		return nil, err
	}
	text := WikiQueryText(query)
	if text == "" {
		return nil, nil
	}
	limit := wikiCandidateLimit(topK)
	filter := wikiSearchFilter(opts)
	body, err := w.search(ctx, lancestore.Query{Text: text, TextColumn: lanceWikiBody, Filter: filter, Limit: limit})
	if err != nil {
		return nil, err
	}
	terms, err := w.search(ctx, lancestore.Query{Text: text, TextColumn: lanceWikiTerms, Filter: filter, Limit: limit})
	if err != nil {
		return nil, err
	}
	return fuseWikiRankings(topK, body, terms), nil
}

// SemanticSearch runs the vector half.
func (w *WikiDB) SemanticSearch(ctx context.Context, vec []float32, topK int) ([]WikiSearchResult, error) {
	var err error
	topK, err = w.resolveSearchLimit(ctx, topK)
	if err != nil {
		return nil, err
	}
	return w.search(ctx, lancestore.Query{
		Vector: vec, VectorColumn: lanceWikiVector, Limit: topK,
	})
}

// HybridSearch runs both and lets the ENGINE fuse them.
//
// No fusion is computed here. That is the entire reason this engine was chosen: the SQLite version
// had 331 lines of Go doing reciprocal-rank fusion across seven weighted passes, and every one of
// them was a ranking decision made outside the thing that owns ranking.
func (w *WikiDB) HybridSearch(ctx context.Context, query string, vec []float32, topK int) ([]WikiSearchResult, error) {
	var err error
	topK, err = w.resolveSearchLimit(ctx, topK)
	if err != nil {
		return nil, err
	}
	text := WikiQueryText(query)
	if text == "" && len(vec) == 0 {
		return nil, nil
	}
	if text == "" {
		return w.SemanticSearch(ctx, vec, topK)
	}
	if len(vec) == 0 {
		return w.Search(ctx, query, topK)
	}
	limit := wikiCandidateLimit(topK)
	bodyVector, err := w.search(ctx, lancestore.Query{
		Text: text, TextColumn: lanceWikiBody,
		Vector: vec, VectorColumn: lanceWikiVector, Limit: limit,
	})
	if err != nil {
		return nil, err
	}
	terms, err := w.search(ctx, lancestore.Query{Text: text, TextColumn: lanceWikiTerms, Limit: limit})
	if err != nil {
		return nil, err
	}
	return fuseWikiRankings(topK, bodyVector, terms), nil
}

func wikiCandidateLimit(topK int) int {
	maxInt := int(^uint(0) >> 1)
	if topK > maxInt/4 {
		return maxInt
	}
	return topK * 4
}

// resolveSearchLimit makes the public zero value mean what the API promises: all matching rows.
// Lance's own zero means DefaultLimit, so it must never cross this boundary unchanged.
func (w *WikiDB) resolveSearchLimit(ctx context.Context, topK int) (int, error) {
	if topK < 0 {
		return 0, fmt.Errorf("top_k cannot be negative")
	}
	if topK > 0 {
		return topK, nil
	}
	if err := w.ensureTables(ctx); err != nil {
		return 0, err
	}
	n, err := w.chunks.Count(ctx)
	if err != nil {
		return 0, err
	}
	maxInt := int64(^uint(0) >> 1)
	if n > maxInt {
		return int(maxInt), nil
	}
	if n == 0 {
		return 1, nil
	}
	return int(n), nil
}

func wikiSearchFilter(opts WikiSearchOptions) string {
	if opts.ExcludeMandatory {
		return "mandatory = false"
	}
	return ""
}

func fuseWikiRankings(topK int, rankings ...[]WikiSearchResult) []WikiSearchResult {
	const rrfK = 60.0
	type fused struct {
		result WikiSearchResult
		score  float64
	}
	bySlug := make(map[string]*fused)
	for _, ranking := range rankings {
		for rank, result := range ranking {
			entry := bySlug[result.Slug]
			if entry == nil {
				copy := result
				entry = &fused{result: copy}
				bySlug[result.Slug] = entry
			}
			entry.score += 1000 / (rrfK + float64(rank+1))
		}
	}
	out := make([]WikiSearchResult, 0, len(bySlug))
	for _, entry := range bySlug {
		entry.result.Score = entry.score
		out = append(out, entry.result)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Score != out[j].Score {
			return out[i].Score > out[j].Score
		}
		return out[i].Slug < out[j].Slug
	})
	if topK <= 0 {
		topK = lancestore.DefaultLimit
	}
	if len(out) > topK {
		out = out[:topK]
	}
	return out
}

func (w *WikiDB) search(ctx context.Context, q lancestore.Query) ([]WikiSearchResult, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	hits, err := w.chunks.Search(ctx, q)
	if err != nil {
		return nil, err
	}
	out := make([]WikiSearchResult, 0, len(hits))
	for _, h := range hits {
		r := WikiSearchResult{
			Slug:       str(h.Row["slug"]),
			Title:      str(h.Row["title"]),
			Summary:    str(h.Row["summary"]),
			DocType:    str(h.Row["doc_type"]),
			Breadcrumb: str(h.Row["breadcrumb"]),
			Source:     str(h.Row["source"]),
			Score:      h.Score,
			EntityID:   str(h.Row["entity_id"]),
			RevisionID: str(h.Row["revision_id"]),
			Superseded: boolOf(h.Row["superseded"]),
			CurrentID:  str(h.Row["current_id"]),
			Mandatory:  boolOf(h.Row["mandatory"]),
		}
		if r.Snippet == "" {
			r.Snippet = wikiSnippet(str(h.Row["body"]), str(h.Row["summary"]))
		}
		r.CrossRefs = w.directTargets(ctx, r.Slug)
		out = append(out, r)
	}
	return out, nil
}

func wikiSnippet(body, summary string) string {
	if s := strings.TrimSpace(summary); s != "" {
		return s
	}
	const max = 240
	b := strings.TrimSpace(body)
	if len(b) <= max {
		return b
	}
	cut := strings.LastIndex(b[:max], " ")
	if cut < max/2 {
		cut = max
	}
	return b[:cut] + "…"
}

// Browse lists pages by filter, which is a predicate and a limit rather than a search.
func (w *WikiDB) Browse(ctx context.Context, filter BrowseFilter) ([]BrowseEntry, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	var conds []string
	if filter.DocType != "" {
		conds = append(conds, fmt.Sprintf("doc_type = %s", lanceQuote(filter.DocType)))
	}
	if filter.ClusterID >= 0 {
		conds = append(conds, fmt.Sprintf("cluster_id = %d", filter.ClusterID))
	}
	if filter.Important != nil {
		conds = append(conds, fmt.Sprintf("important = %t", *filter.Important))
	}
	if len(conds) == 0 {
		conds = append(conds, "word_count >= 0")
	}
	limit := filter.Limit
	if limit <= 0 {
		limit = 100
	}

	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: strings.Join(conds, " AND "), Limit: limit,
	})
	if err != nil {
		return nil, err
	}
	out := make([]BrowseEntry, 0, len(hits))
	for _, h := range hits {
		out = append(out, BrowseEntry{
			Slug:        str(h.Row["slug"]),
			Title:       str(h.Row["title"]),
			Summary:     str(h.Row["summary"]),
			DocType:     str(h.Row["doc_type"]),
			Breadcrumb:  str(h.Row["breadcrumb"]),
			ClusterName: str(h.Row["cluster_name"]),
			Confidence:  flt(h.Row["confidence"]),
			Important:   boolOf(h.Row["important"]),
			WordCount:   int(i64(h.Row["word_count"])),
		})
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].DocType != out[j].DocType {
			return out[i].DocType < out[j].DocType
		}
		return out[i].Slug < out[j].Slug
	})
	return out, nil
}

// FindXRefs walks the cross-reference graph breadth-first to the given depth.
//
// The traversal is in Go and always was: the storage side answers "what does this slug point at
// and what points at it", one hop at a time, and no engine feature is needed for the walk. This is
// the same shape as the SQLite version, deliberately — it was never the part that needed changing.
func (w *WikiDB) FindXRefs(ctx context.Context, slug string, depth int) ([]XRefResult, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	if depth < 1 {
		depth = 1
	}
	titles, err := w.slugTitles(ctx)
	if err != nil {
		return nil, err
	}

	visited := map[string]bool{slug: true}
	frontier := []string{slug}
	var out []XRefResult

	for hop := 0; hop < depth && len(frontier) > 0; hop++ {
		var next []string
		for _, s := range frontier {
			for _, e := range w.directXRefs(ctx, s) {
				if visited[e.Slug] {
					continue
				}
				visited[e.Slug] = true
				e.Title = titles[e.Slug]
				out = append(out, e)
				next = append(next, e.Slug)
			}
		}
		frontier = next
	}

	sort.Slice(out, func(i, j int) bool {
		if out[i].Direction != out[j].Direction {
			return out[i].Direction < out[j].Direction
		}
		return out[i].Slug < out[j].Slug
	})
	return out, nil
}

func (w *WikiDB) directXRefs(ctx context.Context, slug string) []XRefResult {
	var out []XRefResult
	q := lancestore.Query{
		Filter: fmt.Sprintf("source_slug = %s", lanceQuote(slug)), Limit: 1000,
	}
	if hits, err := w.xrefs.Search(ctx, q); err == nil {
		for _, h := range hits {
			out = append(out, XRefResult{
				Slug: str(h.Row["target_slug"]), RefType: str(h.Row["ref_type"]), Direction: "outbound",
			})
		}
	}
	q.Filter = fmt.Sprintf("target_slug = %s", lanceQuote(slug))
	if hits, err := w.xrefs.Search(ctx, q); err == nil {
		for _, h := range hits {
			out = append(out, XRefResult{
				Slug: str(h.Row["source_slug"]), RefType: str(h.Row["ref_type"]), Direction: "inbound",
			})
		}
	}
	return out
}

func (w *WikiDB) directTargets(ctx context.Context, slug string) []string {
	hits, err := w.xrefs.Search(ctx, lancestore.Query{
		Filter: fmt.Sprintf("source_slug = %s", lanceQuote(slug)), Limit: 100,
	})
	if err != nil {
		return nil
	}
	out := make([]string, 0, len(hits))
	for _, h := range hits {
		out = append(out, str(h.Row["target_slug"]))
	}
	sort.Strings(out)
	return out
}

// PageTitles maps every indexed slug to its title, projected so no body is read.
//
// It is the cheap half of Chunks, for the callers that need the page SET and its labels: the
// cross-reference graph, and anything rendering a list of links.
func (w *WikiDB) PageTitles(ctx context.Context) (map[string]string, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	return w.slugTitles(ctx)
}

func (w *WikiDB) slugTitles(ctx context.Context) (map[string]string, error) {
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: "word_count >= 0", Columns: []string{"slug", "title"}, Limit: 100000,
	})
	if err != nil {
		return nil, err
	}
	out := make(map[string]string, len(hits))
	for _, h := range hits {
		out[str(h.Row["slug"])] = str(h.Row["title"])
	}
	return out, nil
}

const syncLogKeep = 50

// AppendSyncLog records one sync.
func (w *WikiDB) AppendSyncLog(ctx context.Context, entry SyncLogEntry) error {
	if err := w.ensureTables(ctx); err != nil {
		return err
	}
	return w.appendSyncLog(ctx, entry)
}

func (w *WikiDB) appendSyncLog(ctx context.Context, entry SyncLogEntry) error {
	if entry.Timestamp == "" {
		entry.Timestamp = time.Now().UTC().Format(time.RFC3339Nano)
	}
	return w.syncLog.Append(ctx, []lancestore.Row{{
		"timestamp":        entry.Timestamp,
		"total_docs":       int64(entry.TotalDocs),
		"articles_written": int64(entry.ArticlesWritten),
		"backlinks_added":  int64(entry.BacklinksAdded),
		"added_json":       jsonOrEmpty(entry.Added),
		"updated_json":     jsonOrEmpty(entry.Updated),
		"deleted_json":     jsonOrEmpty(entry.Deleted),
		"details_json":     jsonOrEmpty(entry.Details),
	}})
}

// QuerySyncLog returns the most recent entries, newest first.
func (w *WikiDB) QuerySyncLog(ctx context.Context, limit int) ([]SyncLogEntry, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	return w.recentSyncLog(ctx, limit)
}

func (w *WikiDB) recentSyncLog(ctx context.Context, limit int) ([]SyncLogEntry, error) {
	if w.syncLog == nil {
		return nil, nil
	}
	if limit <= 0 {
		limit = 10
	}
	hits, err := w.syncLog.Search(ctx, lancestore.Query{
		Filter: "total_docs >= 0", Limit: syncLogKeep * 2,
	})
	if err != nil {
		return nil, err
	}
	out := make([]SyncLogEntry, 0, len(hits))
	for _, h := range hits {
		e := SyncLogEntry{
			Timestamp:       str(h.Row["timestamp"]),
			TotalDocs:       int(i64(h.Row["total_docs"])),
			ArticlesWritten: int(i64(h.Row["articles_written"])),
			BacklinksAdded:  int(i64(h.Row["backlinks_added"])),
		}
		_ = json.Unmarshal([]byte(str(h.Row["added_json"])), &e.Added)
		_ = json.Unmarshal([]byte(str(h.Row["updated_json"])), &e.Updated)
		_ = json.Unmarshal([]byte(str(h.Row["deleted_json"])), &e.Deleted)
		_ = json.Unmarshal([]byte(str(h.Row["details_json"])), &e.Details)
		out = append(out, e)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Timestamp > out[j].Timestamp })
	if len(out) > limit {
		out = out[:limit]
	}
	return out, nil
}

// PendingEmbeddings lists chunks with no vector yet, which is what the embedder consumes.
func (w *WikiDB) PendingEmbeddings(ctx context.Context) ([]chunkRow, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: lanceWikiVector + " IS NULL", Limit: 100000,
	})
	if err != nil {
		return nil, err
	}
	out := make([]chunkRow, 0, len(hits))
	for _, h := range hits {
		out = append(out, chunkRow{
			Slug:       str(h.Row["slug"]),
			Title:      str(h.Row["title"]),
			Summary:    str(h.Row["summary"]),
			Body:       str(h.Row["body"]),
			DocType:    str(h.Row["doc_type"]),
			Breadcrumb: str(h.Row["breadcrumb"]),
			WordCount:  int(i64(h.Row["word_count"])),
		})
	}
	return out, nil
}

// SetChunkVector attaches an embedding to a chunk, keyed by slug.
//
// Keyed by SLUG and not by a row id: the SQLite version used an integer chunk id, and an id that
// only means something inside one build is exactly what breaks when the index is rebuilt between
// the read and the write.
func (w *WikiDB) SetChunkVector(ctx context.Context, slug string, vec []float32) error {
	if w.Remote() {
		return lancestore.ErrReadOnly
	}
	if err := w.ensureTables(ctx); err != nil {
		return err
	}
	if want := ai.ResolveConfiguredEmbeddingDimensions(); len(vec) != want {
		return fmt.Errorf("wiki embedding for %s has %d dimensions, want %d",
			slug, len(vec), want)
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: fmt.Sprintf("slug = %s", lanceQuote(slug)), Limit: 1,
	})
	if err != nil {
		return err
	}
	if len(hits) == 0 {
		return fmt.Errorf("wiki index has no chunk %q to attach an embedding to", slug)
	}
	row := lancestore.Row{}
	for k, v := range hits[0].Row {
		row[k] = v
	}
	row[lanceWikiVector] = vec
	return w.chunks.Upsert(ctx, "slug", []lancestore.Row{row})
}

// EmbeddingStats reports how much of the wiki carries a vector.
func (w *WikiDB) EmbeddingStats(ctx context.Context) (embedded, total int) {
	if err := w.ensureTables(ctx); err != nil {
		return 0, 0
	}
	n, err := w.chunks.Count(ctx)
	if err != nil {
		return 0, 0
	}
	total = int(n)
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: lanceWikiVector + " IS NOT NULL", Limit: 100000,
	})
	if err != nil {
		return 0, total
	}
	return len(hits), total
}

// StoredEmbeddings returns every vector stored in the wiki table.
func (w *WikiDB) StoredEmbeddings(ctx context.Context) ([]StoredEmbedding, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: lanceWikiVector + " IS NOT NULL", Limit: 100000,
	})
	if err != nil {
		return nil, err
	}
	out := make([]StoredEmbedding, 0, len(hits))
	for _, h := range hits {
		vec, ok := h.Row[lanceWikiVector].([]float32)
		if !ok {
			continue
		}
		out = append(out, StoredEmbedding{
			Source:      str(h.Row["source"]),
			ContentHash: str(h.Row["content_hash"]),
			Vector:      vec,
		})
	}
	return out, nil
}

// VectorsBySource returns one vector per source file.
//
// The vectors are already computed and stored, so anything that needs semantic neighbourhood
// reads them rather than recomputing them — which is the whole point of the memory
// consolidation path that calls this.
//
// A document with several chunks contributes its FIRST vector: callers that want neighbourhood
// want one point per document, and the memory wiki compiles one document per page, so there is
// only ever one.
func (w *WikiDB) VectorsBySource(ctx context.Context) (map[string][]float32, error) {
	stored, err := w.StoredEmbeddings(ctx)
	if err != nil {
		return nil, err
	}
	out := make(map[string][]float32, len(stored))
	for _, s := range stored {
		if s.Source == "" {
			continue
		}
		if _, seen := out[s.Source]; seen {
			continue
		}
		if len(s.Vector) > 0 {
			out[s.Source] = s.Vector
		}
	}
	return out, nil
}

func (w *WikiDB) Chunk(ctx context.Context, slug string) (*WikiChunk, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: fmt.Sprintf("slug = %s", lanceQuote(slug)), Limit: 1,
	})
	if err != nil {
		return nil, fmt.Errorf("reading wiki page %q: %w", slug, err)
	}
	if len(hits) == 0 {
		return nil, fmt.Errorf("%w: %s", ErrPageNotFound, slug)
	}
	c := rowToChunk(hits[0].Row)
	return &c, nil
}

// Chunks returns every page's whole row, sorted by slug.
//
// It carries the bodies, so it is the expensive read of this file and the callers are the two that
// genuinely need every page: the lint that audits them and the markdown export that renders them.
// Anything that only needs names and types wants Browse, and anything that only needs change
// detection wants PageHashes.
func (w *WikiDB) Chunks(ctx context.Context) ([]WikiChunk, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{Filter: "word_count >= 0", Limit: 100000})
	if err != nil {
		return nil, err
	}
	out := make([]WikiChunk, 0, len(hits))
	for _, h := range hits {
		out = append(out, rowToChunk(h.Row))
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Slug < out[j].Slug })
	return out, nil
}

// PageHashes maps every indexed slug to its content hash.
//
// Projected, not selected whole: `Query.Columns` keeps the bodies out of the answer, which matters
// because this is the query a generator runs on every build to decide what was added, changed and
// deleted. It replaced an `os.ReadDir` of the wiki directory plus a frontmatter read per page.
func (w *WikiDB) PageHashes(ctx context.Context) (map[string]string, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter:  "word_count >= 0",
		Columns: []string{"slug", "content_hash"},
		Limit:   100000,
	})
	if err != nil {
		return nil, err
	}
	out := make(map[string]string, len(hits))
	for _, h := range hits {
		if s := str(h.Row["slug"]); s != "" {
			out[s] = str(h.Row["content_hash"])
		}
	}
	return out, nil
}

// AllXRefs returns the whole outbound edge set, source slug to target slugs.
//
// FindXRefs answers one slug's neighbourhood; this answers the graph, which is what a lint or an
// export needs before it can say anything about orphans and broken links.
func (w *WikiDB) AllXRefs(ctx context.Context) (map[string][]string, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	if w.xrefs == nil {
		return map[string][]string{}, nil
	}
	hits, err := w.xrefs.Search(ctx, lancestore.Query{
		Filter: "source_slug IS NOT NULL", Limit: 200000,
	})
	if err != nil {
		return nil, err
	}
	out := make(map[string][]string)
	for _, h := range hits {
		src := str(h.Row["source_slug"])
		tgt := str(h.Row["target_slug"])
		if src == "" || tgt == "" {
			continue
		}
		out[src] = append(out[src], tgt)
	}
	for k := range out {
		sort.Strings(out[k])
	}
	return out, nil
}

func rowToChunk(row map[string]any) WikiChunk {
	c := WikiChunk{
		Slug:        str(row["slug"]),
		Title:       str(row["title"]),
		Body:        str(row["body"]),
		Summary:     str(row["summary"]),
		DocType:     str(row["doc_type"]),
		Source:      str(row["source"]),
		Breadcrumb:  str(row["breadcrumb"]),
		ClusterID:   int(i64(row["cluster_id"])),
		ClusterName: str(row["cluster_name"]),
		Confidence:  flt(row["confidence"]),
		ContentHash: str(row["content_hash"]),
		WordCount:   int(i64(row["word_count"])),
		Updated:     str(row["updated"]),
		Important:   boolOf(row["important"]),
		Mandatory:   boolOf(row["mandatory"]),
		EntityID:    str(row["entity_id"]),
		RevisionID:  str(row["revision_id"]),
		Superseded:  boolOf(row["superseded"]),
		CurrentID:   str(row["current_id"]),
		Revision:    int(i64(row["revision"])),
		Previous:    str(row["previous"]),
		Next:        str(row["next"]),
		Created:     str(row["created"]),
		StaleSince:  str(row["stale_since"]),
		StaleReason: str(row["stale_reason"]),
	}
	if raw := str(row["tags_json"]); raw != "" {
		_ = json.Unmarshal([]byte(raw), &c.Tags)
	}
	return c
}

// Slugs lists every page in the index, sorted.
func (w *WikiDB) Slugs(ctx context.Context) ([]string, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{Filter: "word_count >= 0", Limit: 100000})
	if err != nil {
		return nil, err
	}
	out := make([]string, 0, len(hits))
	for _, h := range hits {
		if s := str(h.Row["slug"]); s != "" {
			out = append(out, s)
		}
	}
	sort.Strings(out)
	return out, nil
}

// HasContent reports whether anything is indexed.
func (w *WikiDB) HasContent(ctx context.Context) bool {
	if err := w.ensureTables(ctx); err != nil {
		return false
	}
	n, err := w.chunks.Count(ctx)
	return err == nil && n > 0
}

// Stats reports the index's size.
func (w *WikiDB) Stats(ctx context.Context) (chunks, slugs, xrefs, logEntries int, err error) {
	if err = w.ensureTables(ctx); err != nil {
		return
	}
	c, err := w.chunks.Count(ctx)
	if err != nil {
		return
	}
	x, err := w.xrefs.Count(ctx)
	if err != nil {
		return
	}
	l, err := w.syncLog.Count(ctx)
	if err != nil {
		return
	}
	return int(c), int(c), int(x), int(l), nil
}

func lanceQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", "''") + "'"
}

func jsonOrEmpty(v any) string {
	if v == nil {
		return ""
	}
	b, err := json.Marshal(v)
	if err != nil {
		return ""
	}
	return string(b)
}

func str(v any) string {
	s, _ := v.(string)
	return s
}

func i64(v any) int64 {
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

func flt(v any) float64 {
	switch n := v.(type) {
	case float64:
		return n
	case float32:
		return float64(n)
	case int64:
		return float64(n)
	}
	return 0
}

func boolOf(v any) bool {
	b, _ := v.(bool)
	return b
}
