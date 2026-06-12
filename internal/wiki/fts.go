package wiki

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"unicode"

	sqlite_vec "github.com/asg017/sqlite-vec-go-bindings/cgo"
	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	_ "github.com/mattn/go-sqlite3"
)

func init() {
	sqlite_vec.Auto()
}

const wikiDBSchemaVersion = 2
const wikiDBFilename = "wiki.db"

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

// WikiDB wraps a SQLite database with FTS5 for wiki document chunks.
type WikiDB struct {
	db     *sql.DB
	path   string
	mu     sync.RWMutex
	Logger *slog.Logger
}

func (w *WikiDB) log() *slog.Logger { return slogutil.Resolve(w.Logger) }

// WikiChunk represents a single wiki document chunk stored in the database.
type WikiChunk struct {
	Slug        string
	Title       string
	Body        string
	Summary     string
	DocType     string
	Source      string
	Breadcrumb  string
	ParentSlug  string
	ClusterID   int
	ClusterName string
	Confidence  float64
	ContentHash string
	WordCount   int
	Updated     string
	Important   bool
}

// WikiSearchResult is a single result from a wiki FTS search.
type WikiSearchResult struct {
	Slug       string
	Title      string
	Summary    string
	DocType    string
	Breadcrumb string
	Score      float64
	Snippet    string
	CrossRefs  []string
	Source     string
}

// BrowseFilter controls Browse() output.
type BrowseFilter struct {
	DocType   string
	ClusterID int // -1 = any
	Important *bool
	Limit     int
}

// BrowseEntry is a single entry returned by Browse.
type BrowseEntry struct {
	Slug        string
	Title       string
	Summary     string
	DocType     string
	Breadcrumb  string
	ClusterName string
	Confidence  float64
	Important   bool
	WordCount   int
}

// XRefResult is a cross-reference returned by FindXRefs.
type XRefResult struct {
	Slug      string
	Title     string
	RefType   string
	Direction string // "outbound" or "inbound"
}

// SyncLogEntry records one wiki sync operation.
type SyncLogEntry struct {
	ID              int
	Timestamp       string
	TotalDocs       int
	ArticlesWritten int
	BacklinksAdded  int
	Added           []string // JSON array
	Updated         []string
	Deleted         []string
	Details         map[string]LogDocDetails
}

// LogDocDetails holds per-document metadata in sync log entries.
type LogDocDetails struct {
	Title   string `json:"title"`
	Summary string `json:"summary"`
}

// ---------------------------------------------------------------------------
// Open / Close
// ---------------------------------------------------------------------------

// OpenWikiDB opens (or creates) the wiki FTS database in wikiDir.
func OpenWikiDB(wikiDir string) (*WikiDB, error) {
	if err := os.MkdirAll(wikiDir, 0o755); err != nil {
		return nil, fmt.Errorf("wiki db dir: %w", err)
	}

	// Ensure wiki.db is excluded from git (it's a derived artifact).
	// Cache files (manifest.json, shards/*.wiki.json, shards/*.emb.json) are kept.
	gitignorePath := filepath.Join(wikiDir, ".gitignore")
	if _, err := os.Stat(gitignorePath); os.IsNotExist(err) {
		_ = os.WriteFile(gitignorePath, []byte("wiki.db\nwiki.db-wal\nwiki.db-shm\n"), 0o644)
	}

	dbPath := filepath.Join(wikiDir, wikiDBFilename)

	db, err := sql.Open("sqlite3", dbPath+"?_journal_mode=WAL&_synchronous=NORMAL&_busy_timeout=5000")
	if err != nil {
		return nil, fmt.Errorf("wiki db open: %w", err)
	}

	if err := migrateWikiSchema(db); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("wiki schema migrate: %w", err)
	}

	return &WikiDB{db: db, path: dbPath}, nil
}

// Close closes the underlying database.
func (w *WikiDB) Close() error {
	if w.db != nil {
		return w.db.Close()
	}
	return nil
}

// DBPath returns the filesystem path to the SQLite database.
func (w *WikiDB) DBPath() string { return w.path }

// ---------------------------------------------------------------------------
// Schema migration
// ---------------------------------------------------------------------------

func migrateWikiSchema(db *sql.DB) error {
	_, _ = db.Exec(`CREATE TABLE IF NOT EXISTS wiki_meta (key TEXT PRIMARY KEY, value TEXT)`)

	var version int
	row := db.QueryRow(`SELECT CAST(value AS INTEGER) FROM wiki_meta WHERE key = 'schema_version'`)
	_ = row.Scan(&version)

	if version == wikiDBSchemaVersion {
		return nil
	}

	// Drop old tables for clean migration.
	dropTables := []string{
		`DROP TABLE IF EXISTS chunks_fts`,
		`DROP TABLE IF EXISTS chunks_trigram`,
		`DROP TABLE IF EXISTS chunks`,
		`DROP TABLE IF EXISTS chunks_vec`,
		`DROP TABLE IF EXISTS chunks_vec_map`,
		`DROP TABLE IF EXISTS xrefs`,
		`DROP TABLE IF EXISTS sync_log`,
		`DROP INDEX IF EXISTS idx_chunks_slug`,
		`DROP INDEX IF EXISTS idx_chunks_parent`,
		`DROP INDEX IF EXISTS idx_chunks_cluster`,
		`DROP INDEX IF EXISTS idx_xrefs_source`,
		`DROP INDEX IF EXISTS idx_xrefs_target`,
	}
	for _, q := range dropTables {
		_, _ = db.Exec(q)
	}

	ddl := []string{
		`CREATE TABLE chunks (
			id INTEGER PRIMARY KEY,
			slug TEXT NOT NULL,
			title TEXT NOT NULL,
			body TEXT NOT NULL,
			summary TEXT DEFAULT '',
			doc_type TEXT DEFAULT '',
			source TEXT DEFAULT '',
			breadcrumb TEXT DEFAULT '',
			parent_slug TEXT DEFAULT '',
			cluster_id INTEGER DEFAULT -1,
			cluster_name TEXT DEFAULT '',
			confidence REAL DEFAULT 0.0,
			content_hash TEXT DEFAULT '',
			word_count INTEGER DEFAULT 0,
			updated TEXT DEFAULT '',
			important INTEGER DEFAULT 0
		)`,

		`CREATE VIRTUAL TABLE chunks_fts USING fts5(
			title, body, summary, breadcrumb, doc_type,
			content='chunks', content_rowid='id',
			tokenize='porter unicode61 remove_diacritics 2',
			prefix='2 3'
		)`,

		`CREATE VIRTUAL TABLE chunks_trigram USING fts5(
			title, body,
			content='chunks', content_rowid='id',
			tokenize='trigram case_sensitive 0'
		)`,

		`CREATE TABLE xrefs (
			source_slug TEXT NOT NULL,
			target_slug TEXT NOT NULL,
			ref_type TEXT DEFAULT 'reference',
			PRIMARY KEY (source_slug, target_slug, ref_type)
		)`,

		`CREATE TABLE sync_log (
			id INTEGER PRIMARY KEY,
			timestamp TEXT NOT NULL,
			total_docs INTEGER DEFAULT 0,
			articles_written INTEGER DEFAULT 0,
			backlinks_added INTEGER DEFAULT 0,
			added TEXT DEFAULT '[]',
			updated TEXT DEFAULT '[]',
			deleted TEXT DEFAULT '[]',
			details TEXT DEFAULT '{}'
		)`,

		`CREATE INDEX IF NOT EXISTS idx_chunks_slug ON chunks(slug)`,
		`CREATE INDEX IF NOT EXISTS idx_chunks_parent ON chunks(parent_slug)`,
		`CREATE INDEX IF NOT EXISTS idx_chunks_cluster ON chunks(cluster_id)`,
		`CREATE INDEX IF NOT EXISTS idx_xrefs_source ON xrefs(source_slug)`,
		`CREATE INDEX IF NOT EXISTS idx_xrefs_target ON xrefs(target_slug)`,

		// sqlite-vec: vector storage for semantic search.
		fmt.Sprintf(`CREATE VIRTUAL TABLE IF NOT EXISTS chunks_vec USING vec0(
			embedding float[%d]
		)`, ai.EmbeddingDimensions),
		`CREATE TABLE IF NOT EXISTS chunks_vec_map (
			chunk_id INTEGER PRIMARY KEY,
			vec_rowid INTEGER NOT NULL,
			slug TEXT,
			title TEXT,
			summary TEXT
		)`,
	}
	for _, q := range ddl {
		if _, err := db.Exec(q); err != nil {
			return fmt.Errorf("wiki schema %q: %w", q[:min(60, len(q))], err)
		}
	}

	// BM25 rank weights: title=10, body=1, summary=5, breadcrumb=2, doc_type=3
	if _, err := db.Exec(`INSERT INTO chunks_fts(chunks_fts, rank) VALUES('rank', 'bm25(10.0, 1.0, 5.0, 2.0, 3.0)')`); err != nil {
		return fmt.Errorf("wiki rank setup: %w", err)
	}

	_, _ = db.Exec(`INSERT OR REPLACE INTO wiki_meta (key, value) VALUES ('schema_version', ?)`,
		fmt.Sprintf("%d", wikiDBSchemaVersion))

	return nil
}

// ---------------------------------------------------------------------------
// Rebuild — wipe and re-create all data from caches
// ---------------------------------------------------------------------------

// EmbeddingCache maps content_hash → raw embedding blob (float32 vector).
// Loaded from per-file .emb.json shards via WikiProcessCache.LoadAllEmbeddings(),
// with legacy wiki_emb_cache.json as fallback. wiki.db is always rebuilt from caches.
type EmbeddingCache map[string][]byte

// restoreEmbeddingsFromCache re-inserts cached embedding vectors for chunks
// whose content_hash matches an entry in the cache. Returns how many were restored.
func (w *WikiDB) restoreEmbeddingsFromCache(cache EmbeddingCache) int {
	if len(cache) == 0 {
		return 0
	}

	rows, err := w.db.Query(`
		SELECT c.id, c.slug, c.title, c.summary, c.content_hash
		FROM chunks c
		LEFT JOIN chunks_vec_map m ON m.chunk_id = c.id
		WHERE m.chunk_id IS NULL AND c.content_hash != '' AND c.word_count >= 10`)
	if err != nil {
		return 0
	}
	defer func() { _ = rows.Close() }()

	type restoreRow struct {
		id      int
		slug    string
		title   string
		summary string
		hash    string
	}
	var pending []restoreRow
	for rows.Next() {
		var r restoreRow
		if err := rows.Scan(&r.id, &r.slug, &r.title, &r.summary, &r.hash); err != nil {
			continue
		}
		if _, ok := cache[r.hash]; ok {
			pending = append(pending, r)
		}
	}

	restored := 0
	for _, r := range pending {
		blob := cache[r.hash]
		res, err := w.db.Exec(`INSERT INTO chunks_vec(embedding) VALUES (?)`, blob)
		if err != nil {
			continue
		}
		rowID, _ := res.LastInsertId()
		_, err = w.db.Exec(`INSERT OR REPLACE INTO chunks_vec_map(chunk_id, vec_rowid, slug, title, summary) VALUES (?, ?, ?, ?, ?)`,
			r.id, rowID, r.slug, r.title, r.summary)
		if err != nil {
			continue
		}
		restored++
	}
	return restored
}

// Rebuild deletes all existing data and re-inserts the provided chunks,
// cross-references, and optionally a sync log entry — all in one transaction.
// If embCache is non-nil (loaded from per-file .emb.json shards or legacy
// wiki_emb_cache.json), embedding vectors for unchanged chunks are restored from it.
func (w *WikiDB) Rebuild(chunks []WikiChunk, xrefs map[string][]string, logEntry *SyncLogEntry, embCache EmbeddingCache) error {
	w.mu.Lock()
	defer w.mu.Unlock()

	tx, err := w.db.Begin()
	if err != nil {
		return fmt.Errorf("wiki rebuild begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	// 1. Clear data (keep sync_log).
	_, _ = tx.Exec("DELETE FROM chunks")
	_, _ = tx.Exec("DELETE FROM chunks_fts")
	_, _ = tx.Exec("DELETE FROM chunks_trigram")
	_, _ = tx.Exec("DELETE FROM chunks_vec")
	_, _ = tx.Exec("DELETE FROM chunks_vec_map")
	_, _ = tx.Exec("DELETE FROM xrefs")

	// 2. Insert all chunks.
	chunkStmt, err := tx.Prepare(`INSERT INTO chunks
		(slug, title, body, summary, doc_type, source, breadcrumb, parent_slug,
		 cluster_id, cluster_name, confidence, content_hash, word_count, updated, important)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return fmt.Errorf("prepare chunks: %w", err)
	}
	defer func() { _ = chunkStmt.Close() }()

	for _, c := range chunks {
		imp := 0
		if c.Important {
			imp = 1
		}
		_, err = chunkStmt.Exec(
			c.Slug, c.Title, c.Body, c.Summary, c.DocType, c.Source,
			c.Breadcrumb, c.ParentSlug, c.ClusterID, c.ClusterName,
			c.Confidence, c.ContentHash, c.WordCount, c.Updated, imp,
		)
		if err != nil {
			return fmt.Errorf("insert chunk %q: %w", c.Slug, err)
		}
	}

	// 3. Sync FTS tables from content table.
	if _, err := tx.Exec(`INSERT INTO chunks_fts(chunks_fts) VALUES('rebuild')`); err != nil {
		return fmt.Errorf("fts rebuild: %w", err)
	}
	if _, err := tx.Exec(`INSERT INTO chunks_trigram(chunks_trigram) VALUES('rebuild')`); err != nil {
		return fmt.Errorf("trigram rebuild: %w", err)
	}

	// 4. Insert cross-references.
	xrefStmt, err := tx.Prepare(`INSERT OR IGNORE INTO xrefs (source_slug, target_slug, ref_type) VALUES (?, ?, 'reference')`)
	if err != nil {
		return fmt.Errorf("prepare xrefs: %w", err)
	}
	defer func() { _ = xrefStmt.Close() }()

	for src, targets := range xrefs {
		for _, tgt := range targets {
			_, _ = xrefStmt.Exec(src, tgt)
		}
	}

	// 5. Append sync_log entry if provided.
	if logEntry != nil {
		if err := appendSyncLogTx(tx, *logEntry); err != nil {
			return fmt.Errorf("sync log: %w", err)
		}
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("wiki rebuild commit: %w", err)
	}

	// 6. Restore embeddings from JSON cache (source of truth).
	restored := w.restoreEmbeddingsFromCache(embCache)

	// 7. Optimize FTS tables.
	w.optimizeTables()

	w.log().Info("wiki db rebuild", "chunks", len(chunks), "xrefs", len(xrefs), "embeddings_restored", restored)
	return nil
}

func (w *WikiDB) optimizeTables() {
	_, _ = w.db.Exec(`INSERT INTO chunks_fts(chunks_fts) VALUES('optimize')`)
	_, _ = w.db.Exec(`INSERT INTO chunks_trigram(chunks_trigram) VALUES('optimize')`)
}

// ---------------------------------------------------------------------------
// Multi-pass search with RRF
// ---------------------------------------------------------------------------

type wikiSearchPass struct {
	name   string
	weight float64
	query  string
}

const wikiRRFK = 60

// Search performs multi-pass FTS5 search with Reciprocal Rank Fusion.
func (w *WikiDB) Search(query string, topK int) ([]WikiSearchResult, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	tokens := tokenizeWikiQuery(query)
	if len(tokens) == 0 {
		return nil, nil
	}

	limit := topK
	if limit <= 0 {
		limit = 50
	}
	fetchLimit := limit * 3

	passes := buildWikiSearchPasses(tokens, query)

	type rankedDoc struct {
		result WikiSearchResult
		key    string
		score  float64
	}
	docScores := make(map[string]*rankedDoc)

	// FTS5 passes on chunks_fts.
	for _, pass := range passes {
		if pass.query == "" {
			continue
		}
		results := w.queryChunksFTS(pass.query, fetchLimit)
		for rank, r := range results {
			key := r.Slug
			rrfScore := pass.weight / float64(wikiRRFK+rank+1)

			if existing, ok := docScores[key]; ok {
				existing.score += rrfScore
				if r.Score > existing.result.Score {
					existing.result = r
				}
			} else {
				docScores[key] = &rankedDoc{result: r, key: key, score: rrfScore}
			}
		}
	}

	// Trigram pass on chunks_trigram.
	trigramResults := w.queryChunksTrigram(tokens, fetchLimit)
	trigramWeight := 0.7
	for rank, r := range trigramResults {
		key := r.Slug
		rrfScore := trigramWeight / float64(wikiRRFK+rank+1)

		if existing, ok := docScores[key]; ok {
			existing.score += rrfScore
			if r.Score > existing.result.Score {
				existing.result = r
			}
		} else {
			docScores[key] = &rankedDoc{result: r, key: key, score: rrfScore}
		}
	}

	// Flatten and sort.
	results := make([]WikiSearchResult, 0, len(docScores))
	for _, rd := range docScores {
		rd.result.Score = rd.score
		results = append(results, rd.result)
	}
	sort.Slice(results, func(i, j int) bool {
		return results[i].Score > results[j].Score
	})
	if limit > 0 && len(results) > limit {
		results = results[:limit]
	}

	// Attach cross-refs for each result.
	for i := range results {
		results[i].CrossRefs = w.lookupXRefs(results[i].Slug)
	}

	return results, nil
}

func (w *WikiDB) queryChunksFTS(ftsQuery string, limit int) []WikiSearchResult {
	rows, err := w.db.Query(`
		SELECT c.slug, c.title, c.summary, c.doc_type, c.breadcrumb, c.body, c.source, chunks_fts.rank
		FROM chunks_fts
		JOIN chunks c ON c.id = chunks_fts.rowid
		WHERE chunks_fts MATCH ?
		ORDER BY chunks_fts.rank
		LIMIT ?`, ftsQuery, limit)
	if err != nil {
		return nil
	}
	defer func() { _ = rows.Close() }()

	var results []WikiSearchResult
	for rows.Next() {
		var slug, title, summary, docType, breadcrumb, body, source string
		var score float64
		if err := rows.Scan(&slug, &title, &summary, &docType, &breadcrumb, &body, &source, &score); err != nil {
			continue
		}
		results = append(results, WikiSearchResult{
			Slug:       slug,
			Title:      title,
			Summary:    summary,
			DocType:    docType,
			Breadcrumb: breadcrumb,
			Score:      -score, // FTS5 rank is negative (lower = better)
			Snippet:    truncateSnippet(body, 200),
			Source:     source,
		})
	}
	return results
}

func (w *WikiDB) queryChunksTrigram(tokens []string, limit int) []WikiSearchResult {
	var longTokens []string
	for _, t := range tokens {
		if len(t) >= 3 {
			longTokens = append(longTokens, t)
		}
	}
	if len(longTokens) == 0 {
		return nil
	}

	parts := make([]string, len(longTokens))
	for i, t := range longTokens {
		parts[i] = `"` + strings.ReplaceAll(t, `"`, ``) + `"`
	}
	trigramQuery := strings.Join(parts, " OR ")

	rows, err := w.db.Query(`
		SELECT c.slug, c.title, c.summary, c.doc_type, c.breadcrumb, c.body, c.source
		FROM chunks_trigram
		JOIN chunks c ON c.id = chunks_trigram.rowid
		WHERE chunks_trigram MATCH ?
		LIMIT ?`, trigramQuery, limit)
	if err != nil {
		return nil
	}
	defer func() { _ = rows.Close() }()

	var results []WikiSearchResult
	for rows.Next() {
		var slug, title, summary, docType, breadcrumb, body, source string
		if err := rows.Scan(&slug, &title, &summary, &docType, &breadcrumb, &body, &source); err != nil {
			continue
		}
		results = append(results, WikiSearchResult{
			Slug:       slug,
			Title:      title,
			Summary:    summary,
			DocType:    docType,
			Breadcrumb: breadcrumb,
			Score:      0.1,
			Snippet:    truncateSnippet(body, 200),
			Source:     source,
		})
	}
	return results
}

// lookupXRefs returns outbound xref slugs for the given slug.
func (w *WikiDB) lookupXRefs(slug string) []string {
	rows, err := w.db.Query(`SELECT target_slug FROM xrefs WHERE source_slug = ?`, slug)
	if err != nil {
		return nil
	}
	defer func() { _ = rows.Close() }()

	var refs []string
	for rows.Next() {
		var target string
		if err := rows.Scan(&target); err != nil {
			continue
		}
		refs = append(refs, target)
	}
	return refs
}

// ---------------------------------------------------------------------------
// Search pass builders
// ---------------------------------------------------------------------------

func buildWikiSearchPasses(tokens []string, rawQuery string) []wikiSearchPass {
	var passes []wikiSearchPass

	if len(tokens) > 1 {
		passes = append(passes, wikiSearchPass{
			name:   "phrase",
			weight: 3.0,
			query:  buildWikiPhraseQuery(rawQuery),
		})
		passes = append(passes, wikiSearchPass{
			name:   "and",
			weight: 2.5,
			query:  buildWikiANDQuery(tokens),
		})
	}

	passes = append(passes, wikiSearchPass{
		name:   "or",
		weight: 1.5,
		query:  buildWikiORQuery(tokens),
	})
	passes = append(passes, wikiSearchPass{
		name:   "prefix",
		weight: 1.0,
		query:  buildWikiPrefixQuery(tokens),
	})

	return passes
}

func buildWikiPhraseQuery(raw string) string {
	clean := strings.ReplaceAll(raw, `"`, ``)
	clean = strings.TrimSpace(clean)
	if clean == "" {
		return ""
	}
	return `"` + clean + `"`
}

func buildWikiANDQuery(tokens []string) string {
	if len(tokens) == 0 {
		return ""
	}
	parts := make([]string, len(tokens))
	for i, t := range tokens {
		parts[i] = quoteWikiToken(t)
	}
	return strings.Join(parts, " ")
}

func buildWikiORQuery(tokens []string) string {
	if len(tokens) == 0 {
		return ""
	}
	parts := make([]string, len(tokens))
	for i, t := range tokens {
		parts[i] = quoteWikiToken(t)
	}
	return strings.Join(parts, " OR ")
}

func buildWikiPrefixQuery(tokens []string) string {
	if len(tokens) == 0 {
		return ""
	}
	parts := make([]string, len(tokens))
	for i, t := range tokens {
		parts[i] = quoteWikiToken(t) + "*"
	}
	return strings.Join(parts, " OR ")
}

func quoteWikiToken(t string) string {
	return `"` + strings.ReplaceAll(t, `"`, ``) + `"`
}

// ---------------------------------------------------------------------------
// Wiki query tokenizer
// ---------------------------------------------------------------------------

var wikiFTS5Reserved = map[string]bool{
	"AND": true, "OR": true, "NOT": true, "NEAR": true,
}

// wikiStopwords is a comprehensive stopword list for wiki search.
var wikiStopwords = map[string]bool{
	"a": true, "an": true, "and": true, "are": true, "as": true, "at": true,
	"be": true, "but": true, "by": true, "do": true, "for": true, "from": true,
	"had": true, "has": true, "have": true, "he": true, "her": true, "his": true,
	"how": true, "i": true, "if": true, "in": true, "into": true, "is": true,
	"it": true, "its": true, "may": true, "my": true, "no": true, "not": true,
	"of": true, "on": true, "or": true, "our": true, "own": true, "say": true,
	"she": true, "so": true, "some": true, "than": true, "that": true,
	"the": true, "their": true, "them": true, "then": true, "there": true,
	"these": true, "they": true, "this": true, "to": true, "up": true,
	"us": true, "very": true, "was": true, "we": true, "were": true,
	"what": true, "when": true, "which": true, "who": true, "whom": true,
	"why": true, "will": true, "with": true, "would": true, "you": true,
	"your": true, "also": true, "been": true, "can": true, "could": true,
	"did": true, "does": true, "each": true, "get": true, "got": true,
	"just": true, "like": true, "made": true, "make": true, "more": true,
	"much": true, "must": true, "need": true, "only": true, "other": true,
	"over": true, "shall": true, "should": true, "still": true, "such": true,
	"take": true, "tell": true, "those": true, "through": true, "too": true,
	"under": true, "use": true, "used": true, "using": true, "well": true,
	"where": true, "while": true, "about": true, "after": true, "again": true,
	"all": true, "am": true, "any": true, "because": true, "before": true,
	"being": true, "below": true, "between": true, "both": true, "down": true,
	"during": true, "few": true, "further": true, "here": true, "itself": true,
	"me": true, "most": true, "nor": true, "off": true, "once": true, "out": true,
	"same": true, "since": true, "until": true, "won": true,
}

func tokenizeWikiQuery(query string) []string {
	raw := strings.Fields(query)
	tokens := make([]string, 0, len(raw))
	for _, t := range raw {
		t = stripWikiFTSSpecialChars(t)
		t = strings.ToLower(t)
		if t == "" || wikiFTS5Reserved[strings.ToUpper(t)] || wikiStopwords[t] {
			continue
		}
		tokens = append(tokens, t)
	}
	return dedupWikiTokens(tokens)
}

func stripWikiFTSSpecialChars(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range s {
		if r == '"' || r == '*' || r == '^' || r == '(' || r == ')' || r == '{' || r == '}' || r == ':' {
			continue
		}
		if unicode.IsControl(r) {
			continue
		}
		b.WriteRune(r)
	}
	return strings.TrimSpace(b.String())
}

func dedupWikiTokens(tokens []string) []string {
	seen := make(map[string]bool, len(tokens))
	out := make([]string, 0, len(tokens))
	for _, t := range tokens {
		if !seen[t] {
			seen[t] = true
			out = append(out, t)
		}
	}
	return out
}

// ---------------------------------------------------------------------------
// Snippet helper
// ---------------------------------------------------------------------------

func truncateSnippet(body string, maxLen int) string {
	body = strings.TrimSpace(body)
	if len(body) <= maxLen {
		return body
	}
	// Try to break at a word boundary.
	cut := maxLen
	if idx := strings.LastIndex(body[:maxLen], " "); idx > maxLen/2 {
		cut = idx
	}
	return body[:cut] + "…"
}

// ---------------------------------------------------------------------------
// Browse
// ---------------------------------------------------------------------------

// Browse returns wiki entries matching the given filter.
// Each slug is returned once, using the chunk with the shortest breadcrumb
// as the representative.
func (w *WikiDB) Browse(filter BrowseFilter) ([]BrowseEntry, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	limit := filter.Limit
	if limit <= 0 {
		limit = 100
	}

	var where []string
	var args []interface{}

	if filter.DocType != "" {
		where = append(where, "doc_type = ?")
		args = append(args, filter.DocType)
	}
	if filter.ClusterID >= 0 {
		where = append(where, "cluster_id = ?")
		args = append(args, filter.ClusterID)
	}
	if filter.Important != nil {
		imp := 0
		if *filter.Important {
			imp = 1
		}
		where = append(where, "important = ?")
		args = append(args, imp)
	}

	whereClause := ""
	if len(where) > 0 {
		whereClause = "WHERE " + strings.Join(where, " AND ")
	}

	// Group by slug, pick the representative with the shortest breadcrumb.
	q := fmt.Sprintf(`
		SELECT slug, title, summary, doc_type, breadcrumb, cluster_name, confidence, important, word_count
		FROM chunks
		%s
		GROUP BY slug
		HAVING LENGTH(breadcrumb) = MIN(LENGTH(breadcrumb))
		ORDER BY title COLLATE NOCASE
		LIMIT ?`, whereClause)
	args = append(args, limit)

	rows, err := w.db.Query(q, args...)
	if err != nil {
		return nil, fmt.Errorf("wiki browse: %w", err)
	}
	defer func() { _ = rows.Close() }()

	var entries []BrowseEntry
	for rows.Next() {
		var e BrowseEntry
		var imp int
		if err := rows.Scan(&e.Slug, &e.Title, &e.Summary, &e.DocType,
			&e.Breadcrumb, &e.ClusterName, &e.Confidence, &imp, &e.WordCount); err != nil {
			continue
		}
		e.Important = imp != 0
		entries = append(entries, e)
	}
	return entries, nil
}

// ---------------------------------------------------------------------------
// Cross-references
// ---------------------------------------------------------------------------

// FindXRefs returns cross-references for slug.
// depth=1 returns direct in/outbound refs.
// depth>1 walks the graph recursively with a CTE.
func (w *WikiDB) FindXRefs(slug string, depth int) ([]XRefResult, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	if depth <= 0 {
		depth = 1
	}

	if depth == 1 {
		return w.findXRefsDirect(slug)
	}
	return w.findXRefsRecursive(slug, depth)
}

func (w *WikiDB) findXRefsDirect(slug string) ([]XRefResult, error) {
	var results []XRefResult

	// Outbound.
	rows, err := w.db.Query(`
		SELECT x.target_slug, x.ref_type, COALESCE(c.title, x.target_slug)
		FROM xrefs x
		LEFT JOIN chunks c ON c.slug = x.target_slug
		WHERE x.source_slug = ?
		GROUP BY x.target_slug, x.ref_type`, slug)
	if err != nil {
		return nil, fmt.Errorf("xrefs outbound: %w", err)
	}
	defer func() { _ = rows.Close() }()

	for rows.Next() {
		var r XRefResult
		if err := rows.Scan(&r.Slug, &r.RefType, &r.Title); err != nil {
			continue
		}
		r.Direction = "outbound"
		results = append(results, r)
	}

	// Inbound.
	rows2, err := w.db.Query(`
		SELECT x.source_slug, x.ref_type, COALESCE(c.title, x.source_slug)
		FROM xrefs x
		LEFT JOIN chunks c ON c.slug = x.source_slug
		WHERE x.target_slug = ?
		GROUP BY x.source_slug, x.ref_type`, slug)
	if err != nil {
		return nil, fmt.Errorf("xrefs inbound: %w", err)
	}
	defer func() { _ = rows2.Close() }()

	for rows2.Next() {
		var r XRefResult
		if err := rows2.Scan(&r.Slug, &r.RefType, &r.Title); err != nil {
			continue
		}
		r.Direction = "inbound"
		results = append(results, r)
	}

	return results, nil
}

func (w *WikiDB) findXRefsRecursive(slug string, depth int) ([]XRefResult, error) {
	var results []XRefResult

	// Outbound recursive CTE.
	outRows, err := w.db.Query(`
		WITH RECURSIVE walk(slug, ref_type, depth) AS (
			SELECT target_slug, ref_type, 1
			FROM xrefs WHERE source_slug = ?
			UNION
			SELECT x.target_slug, x.ref_type, w.depth + 1
			FROM xrefs x
			JOIN walk w ON x.source_slug = w.slug
			WHERE w.depth < ?
		)
		SELECT DISTINCT w.slug, w.ref_type, COALESCE(c.title, w.slug)
		FROM walk w
		LEFT JOIN chunks c ON c.slug = w.slug`, slug, depth)
	if err != nil {
		return nil, fmt.Errorf("xrefs recursive outbound: %w", err)
	}
	defer func() { _ = outRows.Close() }()

	for outRows.Next() {
		var r XRefResult
		if err := outRows.Scan(&r.Slug, &r.RefType, &r.Title); err != nil {
			continue
		}
		r.Direction = "outbound"
		results = append(results, r)
	}

	// Inbound recursive CTE.
	inRows, err := w.db.Query(`
		WITH RECURSIVE walk(slug, ref_type, depth) AS (
			SELECT source_slug, ref_type, 1
			FROM xrefs WHERE target_slug = ?
			UNION
			SELECT x.source_slug, x.ref_type, w.depth + 1
			FROM xrefs x
			JOIN walk w ON x.target_slug = w.slug
			WHERE w.depth < ?
		)
		SELECT DISTINCT w.slug, w.ref_type, COALESCE(c.title, w.slug)
		FROM walk w
		LEFT JOIN chunks c ON c.slug = w.slug`, slug, depth)
	if err != nil {
		return nil, fmt.Errorf("xrefs recursive inbound: %w", err)
	}
	defer func() { _ = inRows.Close() }()

	for inRows.Next() {
		var r XRefResult
		if err := inRows.Scan(&r.Slug, &r.RefType, &r.Title); err != nil {
			continue
		}
		r.Direction = "inbound"
		results = append(results, r)
	}

	return results, nil
}

// ---------------------------------------------------------------------------
// Sync log
// ---------------------------------------------------------------------------

// AppendSyncLog appends a sync log entry.
func (w *WikiDB) AppendSyncLog(entry SyncLogEntry) error {
	w.mu.Lock()
	defer w.mu.Unlock()

	tx, err := w.db.Begin()
	if err != nil {
		return fmt.Errorf("sync log begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	if err := appendSyncLogTx(tx, entry); err != nil {
		return err
	}
	return tx.Commit()
}

func appendSyncLogTx(tx *sql.Tx, entry SyncLogEntry) error {
	added, _ := json.Marshal(nullSlice(entry.Added))
	updated, _ := json.Marshal(nullSlice(entry.Updated))
	deleted, _ := json.Marshal(nullSlice(entry.Deleted))

	details := entry.Details
	if details == nil {
		details = make(map[string]LogDocDetails)
	}
	detailsJSON, _ := json.Marshal(details)

	_, err := tx.Exec(`INSERT INTO sync_log
		(timestamp, total_docs, articles_written, backlinks_added, added, updated, deleted, details)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
		entry.Timestamp, entry.TotalDocs, entry.ArticlesWritten, entry.BacklinksAdded,
		string(added), string(updated), string(deleted), string(detailsJSON))
	if err != nil {
		return fmt.Errorf("insert sync log: %w", err)
	}
	return nil
}

// nullSlice ensures a nil slice marshals to "[]" instead of "null".
func nullSlice(s []string) []string {
	if s == nil {
		return []string{}
	}
	return s
}

// QuerySyncLog returns the most recent sync log entries, ordered newest first.
func (w *WikiDB) QuerySyncLog(limit int) ([]SyncLogEntry, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	if limit <= 0 {
		limit = 20
	}

	rows, err := w.db.Query(`
		SELECT id, timestamp, total_docs, articles_written, backlinks_added,
		       added, updated, deleted, details
		FROM sync_log
		ORDER BY id DESC
		LIMIT ?`, limit)
	if err != nil {
		return nil, fmt.Errorf("query sync log: %w", err)
	}
	defer func() { _ = rows.Close() }()

	var entries []SyncLogEntry
	for rows.Next() {
		var e SyncLogEntry
		var addedJSON, updatedJSON, deletedJSON, detailsJSON string
		if err := rows.Scan(&e.ID, &e.Timestamp, &e.TotalDocs, &e.ArticlesWritten,
			&e.BacklinksAdded, &addedJSON, &updatedJSON, &deletedJSON, &detailsJSON); err != nil {
			continue
		}
		_ = json.Unmarshal([]byte(addedJSON), &e.Added)
		_ = json.Unmarshal([]byte(updatedJSON), &e.Updated)
		_ = json.Unmarshal([]byte(deletedJSON), &e.Deleted)
		e.Details = make(map[string]LogDocDetails)
		_ = json.Unmarshal([]byte(detailsJSON), &e.Details)
		entries = append(entries, e)
	}
	return entries, nil
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

// Stats returns aggregate counts from the wiki database.
func (w *WikiDB) Stats() (totalChunks, totalSlugs, totalXRefs, totalLogEntries int, err error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	_ = w.db.QueryRow(`SELECT COUNT(*) FROM chunks`).Scan(&totalChunks)
	_ = w.db.QueryRow(`SELECT COUNT(DISTINCT slug) FROM chunks`).Scan(&totalSlugs)
	_ = w.db.QueryRow(`SELECT COUNT(*) FROM xrefs`).Scan(&totalXRefs)
	_ = w.db.QueryRow(`SELECT COUNT(*) FROM sync_log`).Scan(&totalLogEntries)

	return totalChunks, totalSlugs, totalXRefs, totalLogEntries, nil
}

// ---------------------------------------------------------------------------
// Semantic search (vector KNN)
// ---------------------------------------------------------------------------

// SemanticSearch performs pure vector similarity search using sqlite-vec.
func (w *WikiDB) SemanticSearch(queryVec []float32, topK int) ([]WikiSearchResult, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()
	return w.semanticSearchLocked(queryVec, topK)
}

func (w *WikiDB) semanticSearchLocked(queryVec []float32, topK int) ([]WikiSearchResult, error) {
	blob, err := sqlite_vec.SerializeFloat32(queryVec)
	if err != nil {
		return nil, fmt.Errorf("serialize query vec: %w", err)
	}

	limit := topK
	if limit <= 0 {
		limit = 50
	}

	rows, err := w.db.Query(`
		SELECT v.rowid, v.distance, m.slug, m.title, m.summary
		FROM chunks_vec v
		JOIN chunks_vec_map m ON m.vec_rowid = v.rowid
		WHERE v.embedding MATCH ? AND k = ?
		ORDER BY v.distance`, blob, limit)
	if err != nil {
		return nil, fmt.Errorf("semantic query: %w", err)
	}
	defer func() { _ = rows.Close() }()

	var results []WikiSearchResult
	for rows.Next() {
		var rowid int64
		var distance float64
		var slug, title, summary string
		if err := rows.Scan(&rowid, &distance, &slug, &title, &summary); err != nil {
			continue
		}
		// Convert L2 distance to cosine similarity estimate.
		cosineSim := 1.0 - (distance*distance)/2.0
		results = append(results, WikiSearchResult{
			Slug:    slug,
			Title:   title,
			Summary: summary,
			Score:   cosineSim,
		})
	}

	// Attach cross-refs.
	for i := range results {
		results[i].CrossRefs = w.lookupXRefs(results[i].Slug)
	}

	return results, nil
}

// ---------------------------------------------------------------------------
// Hybrid search (FTS5 + semantic via RRF)
// ---------------------------------------------------------------------------

// HybridSearch combines BM25 full-text search with semantic vector search
// using Reciprocal Rank Fusion (RRF, k=60).
func (w *WikiDB) HybridSearch(query string, queryVec []float32, topK int) ([]WikiSearchResult, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	tokens := tokenizeWikiQuery(query)
	if len(tokens) == 0 && queryVec == nil {
		return nil, nil
	}

	limit := topK
	if limit <= 0 {
		limit = 50
	}
	fetchLimit := limit * 3

	type rankedDoc struct {
		result WikiSearchResult
		key    string
		score  float64
	}
	docScores := make(map[string]*rankedDoc)

	addResults := func(results []WikiSearchResult, weight float64) {
		for rank, r := range results {
			key := r.Slug
			rrfScore := weight / float64(wikiRRFK+rank+1)
			if existing, ok := docScores[key]; ok {
				existing.score += rrfScore
				if r.Score > existing.result.Score {
					existing.result = r
				}
			} else {
				docScores[key] = &rankedDoc{result: r, key: key, score: rrfScore}
			}
		}
	}

	// FTS5 passes.
	if len(tokens) > 0 {
		passes := buildWikiSearchPasses(tokens, query)
		for _, pass := range passes {
			if pass.query == "" {
				continue
			}
			results := w.queryChunksFTS(pass.query, fetchLimit)
			addResults(results, pass.weight)
		}

		trigramResults := w.queryChunksTrigram(tokens, fetchLimit)
		addResults(trigramResults, 0.7)
	}

	// Semantic pass.
	if queryVec != nil {
		semanticResults, err := w.semanticSearchLocked(queryVec, fetchLimit)
		if err == nil && len(semanticResults) > 0 {
			addResults(semanticResults, 2.0)
		}
	}

	// Flatten, sort, and trim.
	results := make([]WikiSearchResult, 0, len(docScores))
	for _, rd := range docScores {
		rd.result.Score = rd.score
		results = append(results, rd.result)
	}
	sort.Slice(results, func(i, j int) bool {
		return results[i].Score > results[j].Score
	})
	if limit > 0 && len(results) > limit {
		results = results[:limit]
	}

	// Attach cross-refs.
	for i := range results {
		results[i].CrossRefs = w.lookupXRefs(results[i].Slug)
	}

	return results, nil
}

// ---------------------------------------------------------------------------
// Embedding support — PendingEmbeddings, InsertChunkVector, EmbeddingStats
// ---------------------------------------------------------------------------

// PendingEmbeddings returns chunks that have not yet been embedded
// (i.e., no entry in chunks_vec_map). Skips very short chunks (< 10 words).
func (w *WikiDB) PendingEmbeddings() ([]chunkRow, error) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	rows, err := w.db.Query(`
		SELECT c.id, c.slug, c.title, c.summary, c.body, c.doc_type, c.breadcrumb, c.word_count
		FROM chunks c
		LEFT JOIN chunks_vec_map m ON m.chunk_id = c.id
		WHERE m.chunk_id IS NULL AND c.word_count >= 10
		ORDER BY c.id`)
	if err != nil {
		return nil, fmt.Errorf("pending embeddings: %w", err)
	}
	defer func() { _ = rows.Close() }()

	var result []chunkRow
	for rows.Next() {
		var r chunkRow
		if err := rows.Scan(&r.ID, &r.Slug, &r.Title, &r.Summary, &r.Body,
			&r.DocType, &r.Breadcrumb, &r.WordCount); err != nil {
			continue
		}
		result = append(result, r)
	}
	return result, nil
}

// InsertChunkVector stores a single chunk's embedding in the vec0 table.
func (w *WikiDB) InsertChunkVector(chunkID int, slug, title, summary string, vecBlob []byte) error {
	w.mu.Lock()
	defer w.mu.Unlock()

	tx, err := w.db.Begin()
	if err != nil {
		return fmt.Errorf("insert vec begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	// Insert into vec0 table.
	res, err := tx.Exec(`INSERT INTO chunks_vec(embedding) VALUES (?)`, vecBlob)
	if err != nil {
		return fmt.Errorf("insert chunks_vec: %w", err)
	}
	rowID, _ := res.LastInsertId()

	// Insert map entry.
	_, err = tx.Exec(`INSERT OR REPLACE INTO chunks_vec_map(chunk_id, vec_rowid, slug, title, summary) VALUES (?, ?, ?, ?, ?)`,
		chunkID, rowID, slug, title, summary)
	if err != nil {
		return fmt.Errorf("insert chunks_vec_map: %w", err)
	}

	return tx.Commit()
}

// EmbeddingStats returns the count of embedded and total chunks.
func (w *WikiDB) EmbeddingStats() (embedded, total int) {
	w.mu.RLock()
	defer w.mu.RUnlock()

	_ = w.db.QueryRow(`SELECT COUNT(*) FROM chunks_vec_map`).Scan(&embedded)
	_ = w.db.QueryRow(`SELECT COUNT(*) FROM chunks WHERE word_count >= 10`).Scan(&total)
	return embedded, total
}

