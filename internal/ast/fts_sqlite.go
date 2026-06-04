package ast

import (
	"database/sql"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"
	"unicode"

	sqlite_vec "github.com/asg017/sqlite-vec-go-bindings/cgo"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	_ "github.com/mattn/go-sqlite3"
)

func init() {
	sqlite_vec.Auto()
}

const ftsSchemaVersion = 3

type SearchIndex struct {
	db     *sql.DB
	path   string
	mu     sync.RWMutex
	Logger *slog.Logger
}

func (s *SearchIndex) log() *slog.Logger { return slogutil.Resolve(s.Logger) }

func OpenSearchIndex(dbPath string) (*SearchIndex, error) {
	if err := os.MkdirAll(filepath.Dir(dbPath), 0o755); err != nil {
		return nil, fmt.Errorf("search index dir: %w", err)
	}

	db, err := sql.Open("sqlite3", dbPath+"?_journal_mode=WAL&_synchronous=NORMAL&_busy_timeout=5000")
	if err != nil {
		return nil, fmt.Errorf("search index open: %w", err)
	}

	if err := migrateSearchSchema(db); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("search schema migrate: %w", err)
	}

	return &SearchIndex{db: db, path: dbPath}, nil
}

func migrateSearchSchema(db *sql.DB) error {
	_, _ = db.Exec(`CREATE TABLE IF NOT EXISTS search_meta (key TEXT PRIMARY KEY, value TEXT)`)

	var version int
	row := db.QueryRow(`SELECT CAST(value AS INTEGER) FROM search_meta WHERE key = 'schema_version'`)
	_ = row.Scan(&version)

	if version == ftsSchemaVersion {
		return nil
	}

	dropTables := []string{
		`DROP TABLE IF EXISTS file_fts`,
		`DROP TABLE IF EXISTS entity_fts`,
		`DROP TABLE IF EXISTS entity_trigram`,
	}
	for _, q := range dropTables {
		_, _ = db.Exec(q)
	}

	ddl := []string{
		`CREATE VIRTUAL TABLE IF NOT EXISTS file_fts USING fts5(
			path, name, source,
			tokenize='unicode61 remove_diacritics 2',
			prefix='2 3 4'
		)`,
		`CREATE VIRTUAL TABLE IF NOT EXISTS entity_fts USING fts5(
			uid, name, docstring, entity_type, path, line_number UNINDEXED,
			tokenize='unicode61 remove_diacritics 2',
			prefix='2 3 4'
		)`,
		`CREATE VIRTUAL TABLE IF NOT EXISTS entity_trigram USING fts5(
			uid UNINDEXED, name, entity_type UNINDEXED, path UNINDEXED, line_number UNINDEXED,
			tokenize='trigram case_sensitive 0'
		)`,

		`CREATE VIRTUAL TABLE IF NOT EXISTS entity_vec USING vec0(
			embedding float[768]
		)`,
		`CREATE TABLE IF NOT EXISTS entity_vec_map(
			uid TEXT PRIMARY KEY,
			vec_rowid INTEGER NOT NULL,
			name TEXT,
			docstring TEXT,
			entity_type TEXT,
			path TEXT,
			line_number INTEGER
		)`,
	}
	for _, q := range ddl {
		if _, err := db.Exec(q); err != nil {
			return fmt.Errorf("search index schema %q: %w", q[:min(60, len(q))], err)
		}
	}

	rankSetup := []string{
		`INSERT INTO file_fts(file_fts, rank) VALUES('rank', 'bm25(2.0, 8.0, 1.0)')`,
		`INSERT INTO entity_fts(entity_fts, rank) VALUES('rank', 'bm25(0.0, 10.0, 3.0, 2.0, 1.0)')`,
	}
	for _, q := range rankSetup {
		if _, err := db.Exec(q); err != nil {
			return fmt.Errorf("search rank setup: %w", err)
		}
	}

	_, _ = db.Exec(`INSERT OR REPLACE INTO search_meta (key, value) VALUES ('schema_version', ?)`,
		fmt.Sprintf("%d", ftsSchemaVersion))

	return nil
}

func (s *SearchIndex) Close() error {
	if s.db != nil {
		return s.db.Close()
	}
	return nil
}

func (s *SearchIndex) RebuildFromCache(cache *ShardCache, embLookup func(relPath, uid string) []float32) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	t0 := time.Now()
	entries := cache.AllEntries()

	tx, err := s.db.Begin()
	if err != nil {
		return fmt.Errorf("search begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	_, _ = tx.Exec("DELETE FROM file_fts")
	_, _ = tx.Exec("DELETE FROM entity_fts")
	_, _ = tx.Exec("DELETE FROM entity_trigram")
	_, _ = tx.Exec("DELETE FROM entity_vec")
	_, _ = tx.Exec("DELETE FROM entity_vec_map")

	fileStmt, err := tx.Prepare("INSERT INTO file_fts(path, name, source) VALUES (?, ?, ?)")
	if err != nil {
		return fmt.Errorf("prepare file_fts: %w", err)
	}
	defer func() { _ = fileStmt.Close() }()

	entityFTSStmt, err := tx.Prepare("INSERT INTO entity_fts(uid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?)")
	if err != nil {
		return fmt.Errorf("prepare entity_fts: %w", err)
	}
	defer func() { _ = entityFTSStmt.Close() }()

	trigramStmt, err := tx.Prepare("INSERT INTO entity_trigram(uid, name, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?)")
	if err != nil {
		return fmt.Errorf("prepare entity_trigram: %w", err)
	}
	defer func() { _ = trigramStmt.Close() }()

	vecStmt, err := tx.Prepare("INSERT INTO entity_vec(rowid, embedding) VALUES (?, ?)")
	if err != nil {
		return fmt.Errorf("prepare entity_vec: %w", err)
	}
	defer func() { _ = vecStmt.Close() }()

	mapStmt, err := tx.Prepare("INSERT INTO entity_vec_map(uid, vec_rowid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?, ?)")
	if err != nil {
		return fmt.Errorf("prepare entity_vec_map: %w", err)
	}
	defer func() { _ = mapStmt.Close() }()

	fileCount, entityCount, vecCount := 0, 0, 0
	rowID := int64(1)

	for relPath, entry := range entries {
		baseName := filepath.Base(relPath)
		splitBaseName := baseName + " " + splitCodeIdentifier(baseName)
		_, _ = fileStmt.Exec(relPath, splitBaseName, entry.Source)
		fileCount++

		for _, e := range entry.Entities {
			splitName := e.Name + " " + splitCodeIdentifier(e.Name)
			_, _ = entityFTSStmt.Exec(e.UID, splitName, e.Docstring, e.Label, e.Path, e.Line)
			_, _ = trigramStmt.Exec(e.UID, e.Name, e.Label, e.Path, e.Line)
			entityCount++

			if embLookup != nil {
				if vec := embLookup(relPath, e.UID); vec != nil {
					blob, err := sqlite_vec.SerializeFloat32(vec)
					if err == nil {
						_, _ = vecStmt.Exec(rowID, blob)
						_, _ = mapStmt.Exec(e.UID, rowID, e.Name, e.Docstring, e.Label, e.Path, e.Line)
						vecCount++
						rowID++
					}
				}
			}
		}
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("search commit: %w", err)
	}

	s.optimizeTables()

	s.log().Info("search index rebuild",
		"files", fileCount, "entities", entityCount, "vectors", vecCount,
		"duration_ms", time.Since(t0).Seconds()*1000)
	return nil
}

func (s *SearchIndex) UpdateIncremental(cache *ShardCache, changedFiles, deletedFiles []string,
	embLookup func(relPath, uid string) []float32) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	t0 := time.Now()
	allAffected := make([]string, 0, len(changedFiles)+len(deletedFiles))
	allAffected = append(allAffected, changedFiles...)
	allAffected = append(allAffected, deletedFiles...)

	if len(allAffected) == 0 {
		return nil
	}

	tx, err := s.db.Begin()
	if err != nil {
		return fmt.Errorf("search begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	for _, p := range allAffected {
		_, _ = tx.Exec("DELETE FROM file_fts WHERE path = ?", p)
		_, _ = tx.Exec("DELETE FROM entity_fts WHERE path = ?", p)
		_, _ = tx.Exec("DELETE FROM entity_trigram WHERE path = ?", p)

		rows, err := tx.Query("SELECT vec_rowid FROM entity_vec_map WHERE path = ?", p)
		if err == nil {
			var rowids []int64
			for rows.Next() {
				var rid int64
				if rows.Scan(&rid) == nil {
					rowids = append(rowids, rid)
				}
			}
			_ = rows.Close()
			for _, rid := range rowids {
				_, _ = tx.Exec("DELETE FROM entity_vec WHERE rowid = ?", rid)
			}
		}
		_, _ = tx.Exec("DELETE FROM entity_vec_map WHERE path = ?", p)
	}

	var maxRowID int64
	_ = tx.QueryRow("SELECT COALESCE(MAX(vec_rowid), 0) FROM entity_vec_map").Scan(&maxRowID)
	nextRowID := maxRowID + 1

	fileCount, entityCount, vecCount := 0, 0, 0
	for _, p := range changedFiles {
		entry := cache.GetEntry(p)
		if entry == nil {
			continue
		}

		baseName := filepath.Base(p)
		splitBaseName := baseName + " " + splitCodeIdentifier(baseName)
		_, _ = tx.Exec("INSERT INTO file_fts(path, name, source) VALUES (?, ?, ?)",
			p, splitBaseName, entry.Source)
		fileCount++

		for _, e := range entry.Entities {
			splitName := e.Name + " " + splitCodeIdentifier(e.Name)
			_, _ = tx.Exec("INSERT INTO entity_fts(uid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?)",
				e.UID, splitName, e.Docstring, e.Label, e.Path, e.Line)
			_, _ = tx.Exec("INSERT INTO entity_trigram(uid, name, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?)",
				e.UID, e.Name, e.Label, e.Path, e.Line)
			entityCount++

			if embLookup != nil {
				if vec := embLookup(p, e.UID); vec != nil {
					blob, err := sqlite_vec.SerializeFloat32(vec)
					if err == nil {
						_, _ = tx.Exec("INSERT INTO entity_vec(rowid, embedding) VALUES (?, ?)", nextRowID, blob)
						_, _ = tx.Exec("INSERT INTO entity_vec_map(uid, vec_rowid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?, ?)",
							e.UID, nextRowID, e.Name, e.Docstring, e.Label, e.Path, e.Line)
						vecCount++
						nextRowID++
					}
				}
			}
		}
	}

	if err := tx.Commit(); err != nil {
		return fmt.Errorf("search commit: %w", err)
	}

	s.log().Info("search index update",
		"files", fileCount, "entities", entityCount, "vectors", vecCount,
		"duration_ms", time.Since(t0).Seconds()*1000)
	return nil
}

func (s *SearchIndex) optimizeTables() {
	_, _ = s.db.Exec(`INSERT INTO file_fts(file_fts) VALUES('optimize')`)
	_, _ = s.db.Exec(`INSERT INTO entity_fts(entity_fts) VALUES('optimize')`)
	_, _ = s.db.Exec(`INSERT INTO entity_trigram(entity_trigram) VALUES('optimize')`)
}

// ---------------------------------------------------------------------------
// Public Search API — kept for backward compatibility
// ---------------------------------------------------------------------------

func (s *SearchIndex) SearchFiles(query string, topK int) ([]SearchResult, error) {
	res, err := s.Search(query, topK)
	if err != nil {
		return nil, err
	}
	var files []SearchResult
	for _, r := range res {
		if r.Type == "file" {
			files = append(files, r)
		}
	}
	return files, nil
}

func (s *SearchIndex) SearchEntities(query string, topK int) ([]SearchResult, error) {
	res, err := s.Search(query, topK)
	if err != nil {
		return nil, err
	}
	var entities []SearchResult
	for _, r := range res {
		if r.Type != "file" {
			entities = append(entities, r)
		}
	}
	return entities, nil
}

// ---------------------------------------------------------------------------
// Multi-pass search engine
// ---------------------------------------------------------------------------

type searchPass struct {
	name   string
	weight float64
	query  string
}

const rrfK = 60

func (s *SearchIndex) Search(query string, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	tokens := tokenizeQuery(query)
	if len(tokens) == 0 {
		return nil, nil
	}

	limit := topK
	if limit <= 0 {
		limit = 50
	}
	fetchLimit := limit * 3

	passes := buildSearchPasses(tokens, query)

	type rankedDoc struct {
		result SearchResult
		key    string
		score  float64
	}

	docScores := make(map[string]*rankedDoc)

	for _, pass := range passes {
		if pass.query == "" {
			continue
		}

		var passResults []SearchResult

		fileRes := s.queryFTS("file_fts", pass.query, fetchLimit)
		entityRes := s.queryFTS("entity_fts", pass.query, fetchLimit)
		passResults = append(passResults, fileRes...)
		passResults = append(passResults, entityRes...)

		for rank, r := range passResults {
			key := deduplicationKey(r)
			rrfScore := pass.weight / float64(rrfK+rank+1)

			if existing, ok := docScores[key]; ok {
				existing.score += rrfScore
				if r.RelevanceScore > existing.result.RelevanceScore {
					existing.result = r
				}
			} else {
				docScores[key] = &rankedDoc{
					result: r,
					key:    key,
					score:  rrfScore,
				}
			}
		}
	}

	trigramResults := s.queryTrigram(tokens, fetchLimit)
	trigramWeight := 0.7
	for rank, r := range trigramResults {
		key := deduplicationKey(r)
		rrfScore := trigramWeight / float64(rrfK+rank+1)

		if existing, ok := docScores[key]; ok {
			existing.score += rrfScore
			if r.RelevanceScore > existing.result.RelevanceScore {
				existing.result = r
			}
		} else {
			docScores[key] = &rankedDoc{
				result: r,
				key:    key,
				score:  rrfScore,
			}
		}
	}

	results := make([]SearchResult, 0, len(docScores))
	for _, rd := range docScores {
		rd.result.RelevanceScore = rd.score
		results = append(results, rd.result)
	}

	sort.Slice(results, func(i, j int) bool {
		return results[i].RelevanceScore > results[j].RelevanceScore
	})

	if limit > 0 && len(results) > limit {
		results = results[:limit]
	}

	return results, nil
}

func buildSearchPasses(tokens []string, rawQuery string) []searchPass {
	var passes []searchPass

	if len(tokens) > 1 {
		passes = append(passes, searchPass{
			name:   "phrase",
			weight: 3.0,
			query:  buildPhraseQuery(rawQuery),
		})

		passes = append(passes, searchPass{
			name:   "and",
			weight: 2.5,
			query:  buildANDQuery(tokens),
		})
	}

	passes = append(passes, searchPass{
		name:   "or",
		weight: 1.5,
		query:  buildORQuery(tokens),
	})

	passes = append(passes, searchPass{
		name:   "prefix",
		weight: 1.0,
		query:  buildPrefixQuery(tokens),
	})

	return passes
}

// ---------------------------------------------------------------------------
// FTS5 query execution helpers
// ---------------------------------------------------------------------------

func (s *SearchIndex) queryFTS(table, ftsQuery string, limit int) []SearchResult {
	var query string
	switch table {
	case "file_fts":
		query = `SELECT path, name, rank FROM file_fts WHERE file_fts MATCH ? ORDER BY rank LIMIT ?`
	case "entity_fts":
		query = `SELECT uid, name, docstring, entity_type, path, line_number, rank
		         FROM entity_fts WHERE entity_fts MATCH ? ORDER BY rank LIMIT ?`
	default:
		return nil
	}

	rows, err := s.db.Query(query, ftsQuery, limit)
	if err != nil {
		return nil
	}
	defer func() { _ = rows.Close() }()

	var results []SearchResult

	switch table {
	case "file_fts":
		for rows.Next() {
			var path, name string
			var score float64
			if err := rows.Scan(&path, &name, &score); err != nil {
				continue
			}
			results = append(results, SearchResult{
				Type: "file", SearchType: "fts",
				Name: name, Path: path,
				RelevanceScore: -score,
			})
		}
	case "entity_fts":
		for rows.Next() {
			var uid, name, docstring, entityType, path string
			var line int
			var score float64
			if err := rows.Scan(&uid, &name, &docstring, &entityType, &path, &line, &score); err != nil {
				continue
			}
			results = append(results, SearchResult{
				Type: strings.ToLower(entityType), SearchType: "fts",
				Name: name, Path: path, Line: line, Docstring: docstring,
				RelevanceScore: -score,
			})
		}
	}

	return results
}

func (s *SearchIndex) queryTrigram(tokens []string, limit int) []SearchResult {
	if len(tokens) == 0 {
		return nil
	}

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

	rows, err := s.db.Query(
		`SELECT uid, name, entity_type, path, line_number
		 FROM entity_trigram WHERE entity_trigram MATCH ?
		 LIMIT ?`,
		trigramQuery, limit)
	if err != nil {
		return nil
	}
	defer func() { _ = rows.Close() }()

	var results []SearchResult
	for rows.Next() {
		var uid, name, entityType, path string
		var line int
		if err := rows.Scan(&uid, &name, &entityType, &path, &line); err != nil {
			continue
		}
		results = append(results, SearchResult{
			Type: strings.ToLower(entityType), SearchType: "fts",
			Name: name, Path: path, Line: line,
			RelevanceScore: 0.1,
		})
	}
	return results
}

// ---------------------------------------------------------------------------
// Semantic search (unchanged)
// ---------------------------------------------------------------------------

func (s *SearchIndex) SemanticSearch(queryVec []float32, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.semanticSearchLocked(queryVec, topK)
}

func (s *SearchIndex) semanticSearchLocked(queryVec []float32, topK int) ([]SearchResult, error) {
	blob, err := sqlite_vec.SerializeFloat32(queryVec)
	if err != nil {
		return nil, fmt.Errorf("serialize query vec: %w", err)
	}

	limit := topK
	if limit <= 0 {
		limit = -1
	}

	rows, err := s.db.Query(`
		SELECT v.rowid, v.distance, m.name, m.docstring, m.entity_type, m.path, m.line_number
		FROM entity_vec v
		JOIN entity_vec_map m ON m.vec_rowid = v.rowid
		WHERE v.embedding MATCH ? AND k = ?
		ORDER BY v.distance`, blob, limit)
	if err != nil {
		return nil, fmt.Errorf("semantic query failed: %w", err)
	}
	defer func() { _ = rows.Close() }()

	var results []SearchResult
	for rows.Next() {
		var rowid int64
		var distance float64
		var name, docstring, entityType, path string
		var line int
		if err := rows.Scan(&rowid, &distance, &name, &docstring, &entityType, &path, &line); err != nil {
			continue
		}
		cosineSim := 1.0 - (distance*distance)/2.0
		results = append(results, SearchResult{
			Type: strings.ToLower(entityType), SearchType: "semantic",
			Name: name, Path: path, Line: line, Docstring: docstring,
			RelevanceScore: cosineSim,
			Distance:       distance,
		})
	}
	return results, nil
}

// HybridSearch combines BM25 full-text search with semantic vector search
// using Reciprocal Rank Fusion (RRF, k=60) to produce a unified ranking.
func (s *SearchIndex) HybridSearch(query string, queryVec []float32, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	tokens := tokenizeQuery(query)
	if len(tokens) == 0 && queryVec == nil {
		return nil, nil
	}

	limit := topK
	if limit <= 0 {
		limit = 50
	}
	fetchLimit := limit * 3

	type rankedDoc struct {
		result SearchResult
		key    string
		score  float64
	}

	docScores := make(map[string]*rankedDoc)

	addResults := func(results []SearchResult, weight float64) {
		for rank, r := range results {
			key := deduplicationKey(r)
			rrfScore := weight / float64(rrfK+rank+1)
			if existing, ok := docScores[key]; ok {
				existing.score += rrfScore

				// Preserve distance from semantic search (FTS distance is 0)
				bestDist := existing.result.Distance
				if r.Distance != 0 {
					bestDist = r.Distance
				}

				if r.RelevanceScore > existing.result.RelevanceScore {
					existing.result = r
				}

				existing.result.Distance = bestDist
			} else {
				docScores[key] = &rankedDoc{
					result: r,
					key:    key,
					score:  rrfScore,
				}
			}
		}
	}

	if len(tokens) > 0 {
		passes := buildSearchPasses(tokens, query)
		for _, pass := range passes {
			if pass.query == "" {
				continue
			}
			var passResults []SearchResult
			fileRes := s.queryFTS("file_fts", pass.query, fetchLimit)
			entityRes := s.queryFTS("entity_fts", pass.query, fetchLimit)
			passResults = append(passResults, fileRes...)
			passResults = append(passResults, entityRes...)
			addResults(passResults, pass.weight)
		}

		trigramResults := s.queryTrigram(tokens, fetchLimit)
		addResults(trigramResults, 0.7)
	}

	if queryVec != nil {
		semanticResults, err := s.semanticSearchLocked(queryVec, fetchLimit)
		if err == nil && len(semanticResults) > 0 {
			for i := range semanticResults {
				semanticResults[i].SearchType = "hybrid"
			}
			addResults(semanticResults, 2.0)
		}
	}

	results := make([]SearchResult, 0, len(docScores))
	for _, rd := range docScores {
		rd.result.RelevanceScore = rd.score
		rd.result.SearchType = "hybrid"
		results = append(results, rd.result)
	}

	sort.Slice(results, func(i, j int) bool {
		return results[i].RelevanceScore > results[j].RelevanceScore
	})

	if limit > 0 && len(results) > limit {
		results = results[:limit]
	}

	return results, nil
}

// ---------------------------------------------------------------------------
// Query building — FTS5 syntax generators
// ---------------------------------------------------------------------------

var fts5Reserved = map[string]bool{
	"AND": true, "OR": true, "NOT": true, "NEAR": true,
}

func tokenizeQuery(query string) []string {
	raw := strings.Fields(query)
	tokens := make([]string, 0, len(raw)*2)
	for _, t := range raw {
		t = stripFTSSpecialChars(t)
		if t == "" || fts5Reserved[strings.ToUpper(t)] {
			continue
		}
		tokens = append(tokens, t)
		if splits := splitCodeIdentifier(t); splits != t {
			for _, s := range strings.Fields(splits) {
				s = stripFTSSpecialChars(s)
				if s != "" && !fts5Reserved[strings.ToUpper(s)] {
					tokens = append(tokens, s)
				}
			}
		}
	}
	return dedupTokens(tokens)
}

func stripFTSSpecialChars(s string) string {
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

func quoteToken(t string) string {
	return `"` + strings.ReplaceAll(t, `"`, ``) + `"`
}

func buildPhraseQuery(raw string) string {
	clean := strings.ReplaceAll(raw, `"`, ``)
	clean = strings.TrimSpace(clean)
	if clean == "" {
		return ""
	}
	return `"` + clean + `"`
}

func buildANDQuery(tokens []string) string {
	if len(tokens) == 0 {
		return ""
	}
	parts := make([]string, len(tokens))
	for i, t := range tokens {
		parts[i] = quoteToken(t)
	}
	return strings.Join(parts, " ")
}

func buildORQuery(tokens []string) string {
	if len(tokens) == 0 {
		return ""
	}
	parts := make([]string, len(tokens))
	for i, t := range tokens {
		parts[i] = quoteToken(t)
	}
	return strings.Join(parts, " OR ")
}

func buildPrefixQuery(tokens []string) string {
	if len(tokens) == 0 {
		return ""
	}
	parts := make([]string, len(tokens))
	for i, t := range tokens {
		parts[i] = quoteToken(t) + "*"
	}
	return strings.Join(parts, " OR ")
}

// ---------------------------------------------------------------------------
// Deduplication
// ---------------------------------------------------------------------------

func deduplicationKey(r SearchResult) string {
	return r.Path + "\x00" + r.Name + "\x00" + fmt.Sprintf("%d", r.Line)
}

func dedupTokens(tokens []string) []string {
	seen := make(map[string]bool, len(tokens))
	out := make([]string, 0, len(tokens))
	for _, t := range tokens {
		low := strings.ToLower(t)
		if !seen[low] {
			seen[low] = true
			out = append(out, t)
		}
	}
	return out
}

// ---------------------------------------------------------------------------
// Code identifier splitting
// ---------------------------------------------------------------------------

// splitCodeIdentifier splits camelCase, PascalCase, snake_case, and dot.notation
// identifiers into space-separated tokens. Returns the original string unchanged
// if no splitting occurred.
func splitCodeIdentifier(name string) string {
	if name == "" {
		return name
	}

	normalized := strings.NewReplacer("_", " ", ".", " ", "-", " ").Replace(name)

	var parts []string
	for _, word := range strings.Fields(normalized) {
		parts = append(parts, splitCamelCase(word)...)
	}

	result := strings.Join(parts, " ")
	if result == name {
		return name
	}
	return result
}

func splitCamelCase(s string) []string {
	if s == "" {
		return nil
	}

	runes := []rune(s)
	var parts []string
	start := 0

	for i := 1; i < len(runes); i++ {
		prevUpper := unicode.IsUpper(runes[i-1])
		currUpper := unicode.IsUpper(runes[i])
		currLower := unicode.IsLower(runes[i])

		if !prevUpper && currUpper {
			parts = append(parts, string(runes[start:i]))
			start = i
		} else if prevUpper && currLower && i-start > 1 {
			parts = append(parts, string(runes[start:i-1]))
			start = i - 1
		}
	}
	parts = append(parts, string(runes[start:]))

	var filtered []string
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			filtered = append(filtered, p)
		}
	}
	return filtered
}
