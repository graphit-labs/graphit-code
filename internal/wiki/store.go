package wiki

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
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

	// lanceWikiBody is the single column the full-text index reads.
	//
	// The SQLite wiki queried title, summary and body as separate weighted FTS columns and fused
	// the passes in Go. One document and BM25 instead, for the same reason as the AST index: the
	// engine's full-text query takes one column, and rebuilding the fusion in Go is the Go-side
	// search this project ruled out.
	lanceWikiBody = "search_body"

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

// Checkpoint used to fold SQLite's write-ahead log into the database file, because replication
// copies the file and deliberately not its log — so skipping it shipped chunks without their
// embeddings.
//
// IT IS A NO-OP NOW, and deliberately kept rather than deleted at the call sites: Lance has no
// separate log to fold. A write produces a new immutable version of the dataset, so what is on
// disk after a write is already the whole truth and a copy of the directory is complete. Removing
// the call would be correct too; keeping it is what tells the next reader that the concern was
// considered rather than forgotten.
func (w *WikiDB) Checkpoint() {}

// Compact merges the fragments a burst of writes leaves behind. Unlike Checkpoint this does
// something, but nothing depends on it for correctness.
func (w *WikiDB) Compact(ctx context.Context) error {
	if err := w.ensureTables(ctx); err != nil {
		return err
	}
	_, err := w.chunks.Compact(ctx)
	return err
}

// lanceWikiVersionRetention is how long a superseded dataset version survives before pruning.
//
// NOT zero. Lance is MVCC: a reader answers from the snapshot it opened, so compaction never
// takes a query down, but pruning a version a live reader still holds does. Nothing here uses
// time travel, so retention beyond a margin for in-flight reads buys nothing and costs the whole
// superseded copy.
const lanceWikiVersionRetention = 15 * time.Minute

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
		if res, err := t.table.PruneVersions(ctx, lanceWikiVersionRetention); err != nil {
			w.log().Warn("pruning superseded wiki index versions", "table", t.name, "error", err)
		} else if res.OldVersions > 0 {
			w.log().Info("superseded wiki index versions pruned", "table", t.name,
				"versions", res.OldVersions, "bytes_reclaimed", res.BytesRemoved)
		}
	}
}

// Remote reports whether this is a published index, which is read-only.
func (w *WikiDB) Remote() bool { return w.store != nil && w.store.Remote() }

// ---------- schema ----------

// lanceChunksSchema takes the vector width as a parameter rather than assuming
// ai.EmbeddingDimensions — see the identical note on lanceEntitiesSchema in
// internal/ast/search_lance.go, which this mirrors.
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
		{Name: lanceWikiBody, Type: lancestore.FieldString},
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
		// The three slug lists and the per-document details are JSON in one column each.
		//
		// A column store has no natural place for a variable-length list of strings that is only
		// ever read whole, and modelling them as rows would make a log entry a join. They are
		// written and read as a unit, so they are stored as one.
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

// lanceWikiMinRowsForVectorIndex is the engine's IVF-PQ training floor, the same 256 the AST index
// hit. A wiki with fewer pages than that keeps answering semantic queries by scanning.
const lanceWikiMinRowsForVectorIndex = 256

func lanceChunkIndexes() []lancestore.Index {
	return []lancestore.Index{
		{Column: lanceWikiBody, Kind: lancestore.IndexInvertedText},
		{Column: lanceWikiVector, Kind: lancestore.IndexVectorIVFPQ},
		{Column: "slug", Kind: lancestore.IndexScalarBTree},
		{Column: "doc_type", Kind: lancestore.IndexScalarBitmap},
	}
}

// ---------- the document ----------

// wikiSearchBody renders a chunk as the one document the full-text index reads.
//
// Title first and twice-present through its split form, because a page's title is the strongest
// evidence of what it is about; then the summary, then the body. The gram bag covers the slug and
// the title so a truncated query reaches them, and it deliberately does NOT cover the body: a
// page's whole text expanded into two- and three-character grams is tens of thousands of tokens
// that match everything, which is the opposite of recall.
func wikiSearchBody(c WikiChunk) string {
	parts := []string{c.Title, splitWikiSlug(c.Slug), c.Summary, c.Breadcrumb, c.DocType, c.Body}
	parts = append(parts, wikiGramBag(c.Title, c.Slug))
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		if strings.TrimSpace(p) != "" {
			out = append(out, p)
		}
	}
	return strings.Join(out, " ")
}

// splitWikiSlug turns a slug into words, since a slug carries the title of a page that may have
// been written before the title was.
func splitWikiSlug(slug string) string {
	return strings.Join(strings.FieldsFunc(slug, func(r rune) bool {
		return r == '-' || r == '_' || r == '.' || r == '/'
	}), " ")
}

// wikiGramBag is the 2+3-gram bag of the title and the slug, deduplicated.
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

// ---------- rows ----------

// buildChunkRow is the ONLY place a chunk row is composed, for the same reason the AST index has
// one: two constructors drift, and the drift is silent.
func buildChunkRow(c WikiChunk, vec []float32) lancestore.Row {
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
		lanceWikiBody:   wikiSearchBody(c),
		lanceWikiVector: nil,
	}
	// See the identical comment on buildEntityRow in internal/ast/search_lance.go: a failed
	// embed reports as empty, which is the only case to tell apart from a real vector — not a
	// fixed width, or every provider except the local 768-dim one loses its vectors silently.
	if len(vec) > 0 {
		row[lanceWikiVector] = vec
	}
	return row
}

// CheckAllHashesMatch reports whether every given chunk is already indexed with the same content
// hash, which is how a caller skips a rebuild that would change nothing.
//
// Not merely an optimisation: a wiki rebuild drops and rewrites four tables, and doing that when
// nothing changed churns the published artifact — every object re-uploaded for an identical
// result.
func (w *WikiDB) CheckAllHashesMatch(ctx context.Context, chunks []WikiChunk) bool {
	if len(chunks) == 0 {
		return false
	}
	if err := w.ensureTables(ctx); err != nil {
		return false
	}
	n, err := w.chunks.Count(ctx)
	if err != nil || int(n) != len(chunks) {
		return false
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: "word_count >= 0", Limit: len(chunks) + 1,
	})
	if err != nil {
		return false
	}
	indexed := make(map[string]string, len(hits))
	for _, h := range hits {
		indexed[str(h.Row["slug"])] = str(h.Row["content_hash"])
	}
	for _, c := range chunks {
		if c.ContentHash == "" || indexed[c.Slug] != c.ContentHash {
			return false
		}
	}
	return true
}

// ---------- rebuild ----------

const lanceWikiWriteBatch = 500

// Rebuild replaces the whole index from the compiled chunks.
//
// A wiki rebuild is all-or-nothing by nature: the pages are recompiled from their sources and the
// index is derived from them, so there is no delta to apply and nothing to preserve. Tables are
// dropped and rewritten, and the indexes are built last.
func (w *WikiDB) Rebuild(ctx context.Context, chunks []WikiChunk, xrefs map[string][]string,
	logEntry *SyncLogEntry, embCache EmbeddingCache) error {
	if w.Remote() {
		return lancestore.ErrReadOnly
	}
	t0 := time.Now()

	// The sync log is the ONE table that survives a rebuild: it is the history of rebuilds, so
	// clearing it on every rebuild would leave it permanently one entry long.
	previousLog, err := w.recentSyncLog(ctx, syncLogKeep)
	if err != nil {
		w.log().Warn("could not read the existing sync log before rebuilding", "error", err)
	}

	for _, name := range []string{lanceChunksTable, lanceXRefsTable, lanceSyncLogTable, lanceMetaTable} {
		if err := w.store.DropTable(ctx, name); err != nil {
			return fmt.Errorf("clearing %s: %w", name, err)
		}
	}
	if err := w.createTables(ctx); err != nil {
		return err
	}

	var (
		batch    []lancestore.Row
		vecCount int
	)
	flush := func() error {
		if len(batch) == 0 {
			return nil
		}
		if err := w.chunks.Append(ctx, batch); err != nil {
			return fmt.Errorf("writing %d chunks: %w", len(batch), err)
		}
		batch = batch[:0]
		return nil
	}
	for _, c := range chunks {
		var vec []float32
		if embCache != nil {
			vec = embCache[c.ContentHash]
		}
		row := buildChunkRow(c, vec)
		if row[lanceWikiVector] != nil {
			vecCount++
		}
		batch = append(batch, row)
		if len(batch) >= lanceWikiWriteBatch {
			if err := flush(); err != nil {
				return err
			}
		}
	}
	if err := flush(); err != nil {
		return err
	}

	if err := w.writeXRefs(ctx, xrefs); err != nil {
		return err
	}

	// The preserved history goes back first, so the new entry is the most recent row.
	for i := len(previousLog) - 1; i >= 0; i-- {
		if err := w.appendSyncLog(ctx, previousLog[i]); err != nil {
			w.log().Warn("could not restore a sync log entry", "error", err)
		}
	}
	if logEntry != nil {
		if err := w.appendSyncLog(ctx, *logEntry); err != nil {
			return err
		}
	}

	idx := lanceChunkIndexes()
	if vecCount < lanceWikiMinRowsForVectorIndex {
		idx = withoutWikiColumn(idx, lanceWikiVector)
		w.log().Info("wiki vector index not built: too few embeddings to train on",
			"vectors", vecCount, "required", lanceWikiMinRowsForVectorIndex,
			"impact", "semantic search still answers, by scanning instead of by index")
	}
	if err := w.chunks.EnsureIndexes(ctx, idx...); err != nil {
		return err
	}

	w.log().Info("wiki index rebuild", "chunks", len(chunks), "vectors", vecCount,
		"duration_ms", time.Since(t0).Seconds()*1000)

	// A rebuild replaces the whole chunk set, leaving the superseded copy and the fragments
	// behind it on disk. Nothing else reclaims them.
	w.Maintain(ctx)
	return nil
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

func (w *WikiDB) createTables(ctx context.Context) error {
	var err error
	if w.chunks, err = w.store.CreateTable(ctx, lanceChunksTable, lanceChunksSchema(ai.ResolveConfiguredEmbeddingDimensions())); err != nil {
		return err
	}
	if w.xrefs, err = w.store.CreateTable(ctx, lanceXRefsTable, lanceXRefsSchema()); err != nil {
		return err
	}
	if w.syncLog, err = w.store.CreateTable(ctx, lanceSyncLogTable, lanceSyncLogSchema()); err != nil {
		return err
	}
	if w.meta, err = w.store.CreateTable(ctx, lanceMetaTable, lanceMetaSchema()); err != nil {
		return err
	}
	return nil
}

func (w *WikiDB) ensureTables(ctx context.Context) error {
	// A READ MUST NOT GO THROUGH A WRITE PATH, and this is where that was getting violated.
	//
	// EnsureTable CREATES what is missing, and creating is refused on a published store — so a
	// published wiki answered every query with "this store is read-only", which reads as a
	// permission problem and is actually a code path problem. A mounted artifact's tables exist
	// by definition: the publisher wrote them. They are OPENED.
	if w.store.Remote() {
		return w.openTables(ctx)
	}
	var err error
	if w.chunks == nil {
		if w.chunks, err = w.store.EnsureTable(ctx, lanceChunksTable, lanceChunksSchema(ai.ResolveConfiguredEmbeddingDimensions())); err != nil {
			return err
		}
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

// openTables opens what a published artifact already contains.
//
// A missing table is TOLERATED here, unlike locally: an artifact published by an older build may
// have no sync log, and refusing to open the whole wiki over that would make one absent table cost
// the pages. Only `chunks` is required — without it there is no wiki.
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
	// Sorted, so the table is byte-identical for identical input. A published index that differs
	// only in row order would re-upload every object for no change.
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

// ---------- search ----------

// Search runs the keyword half.
//
// A query with no searchable terms returns NO RESULTS AND NO ERROR. "  " is a question with no
// content, not a malformed request, and a caller that shows the user an error for it is reporting
// a fault that does not exist.
func (w *WikiDB) Search(ctx context.Context, query string, topK int) ([]WikiSearchResult, error) {
	text := WikiQueryText(query)
	if text == "" {
		return nil, nil
	}
	return w.search(ctx, lancestore.Query{
		Text: text, TextColumn: lanceWikiBody, Limit: topK,
	})
}

// SemanticSearch runs the vector half.
func (w *WikiDB) SemanticSearch(ctx context.Context, vec []float32, topK int) ([]WikiSearchResult, error) {
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
	text := WikiQueryText(query)
	if text == "" && len(vec) == 0 {
		// Neither channel has anything to search with. Empty, not an error — see Search.
		return nil, nil
	}
	return w.search(ctx, lancestore.Query{
		Text: text, TextColumn: lanceWikiBody,
		Vector: vec, VectorColumn: lanceWikiVector, Limit: topK,
	})
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
		}
		if r.Snippet == "" {
			r.Snippet = wikiSnippet(str(h.Row["body"]), str(h.Row["summary"]))
		}
		r.CrossRefs = w.directTargets(ctx, r.Slug)
		out = append(out, r)
	}
	return out, nil
}

// wikiSnippet is the summary when there is one, and the head of the body otherwise.
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

// ---------- browse ----------

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
	// A filter-only query needs SOME predicate, so an unfiltered browse gets one that is always
	// true rather than a special code path.
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
	// Ordered here rather than by the engine: a filter query has no ranking channel, so the rows
	// come back in storage order and the caller expects a stable list.
	sort.Slice(out, func(i, j int) bool {
		if out[i].DocType != out[j].DocType {
			return out[i].DocType < out[j].DocType
		}
		return out[i].Slug < out[j].Slug
	})
	return out, nil
}

// ---------- cross-references ----------

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

func (w *WikiDB) slugTitles(ctx context.Context) (map[string]string, error) {
	hits, err := w.chunks.Search(ctx, lancestore.Query{Filter: "word_count >= 0", Limit: 100000})
	if err != nil {
		return nil, err
	}
	out := make(map[string]string, len(hits))
	for _, h := range hits {
		out[str(h.Row["slug"])] = str(h.Row["title"])
	}
	return out, nil
}

// ---------- sync log ----------

// syncLogKeep bounds the retained history. The SQLite version kept the table unbounded and read
// the tail; a column store has no cheap "delete all but the newest N", so the bound is applied
// where the rows are written instead.
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
	// Newest first. The timestamp is RFC3339Nano, which sorts lexicographically in time order.
	sort.Slice(out, func(i, j int) bool { return out[i].Timestamp > out[j].Timestamp })
	if len(out) > limit {
		out = out[:limit]
	}
	return out, nil
}

// ---------- embeddings ----------

// PendingEmbeddings lists chunks with no vector yet, which is what the embedder consumes.
func (w *WikiDB) PendingEmbeddings(ctx context.Context) ([]chunkRow, error) {
	if err := w.ensureTables(ctx); err != nil {
		return nil, err
	}
	// `embedding IS NULL` is the predicate that matters; it is expressed by the engine's filter
	// rather than by fetching everything and checking in Go.
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

// StoredEmbeddings returns every vector, so a caller can carry them into the shard cache and have
// them survive the next rebuild without re-running the model.
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

// PageBody returns one page's text and title from the index.
//
// ErrPageNotFound when the slug is absent, so a caller can offer the list of what is there —
// exactly as the file-backed path does.
func (w *WikiDB) PageBody(ctx context.Context, slug string) (body, title string, err error) {
	if err := w.ensureTables(ctx); err != nil {
		return "", "", err
	}
	hits, err := w.chunks.Search(ctx, lancestore.Query{
		Filter: fmt.Sprintf("slug = %s", lanceQuote(slug)), Limit: 1,
	})
	if err != nil {
		return "", "", fmt.Errorf("reading wiki page %q: %w", slug, err)
	}
	if len(hits) == 0 {
		return "", "", fmt.Errorf("%w: %s", ErrPageNotFound, slug)
	}
	return str(hits[0].Row["body"]), str(hits[0].Row["title"]), nil
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

// ---------- stats ----------

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
	// One chunk per page here: the wiki compiles a page into a single row, so slugs and chunks are
	// the same count. It stays a separate return value because the callers print both.
	return int(c), int(c), int(x), int(l), nil
}

// ---------- small helpers ----------

// lanceQuote renders a string literal for the engine's filter dialect.
//
// SAFETY: single quotes, doubled to escape. A slug comes from a file name and can contain one, and
// an unescaped quote there does not fail — it changes which rows match.
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
