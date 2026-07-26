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

// searchIndexSuffix names the search index file, a sibling of the graph store.
//
// A constant rather than a literal repeated at five call sites: CleanupInterruptedSwap
// deletes stray "<dbPath>.*" siblings and has to skip this one, and having the two places
// disagree would silently delete the index.
const searchIndexSuffix = ".search.sqlite"

// SearchIndexSuffix is searchIndexSuffix for callers outside this package.
const SearchIndexSuffix = searchIndexSuffix

const ftsSchemaVersion = 4

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
		// Trigrams are precomputed at write time into name_tri and indexed with a
		// WORD tokenizer, so a query's trigrams OR-match a name's trigrams as an
		// unordered bag scored by BM25.
		//
		// This is deliberately not tokenize='trigram'. FTS5's trigram tokenizer
		// matches the query's trigrams as an ordered phrase, which is substring
		// containment: "conf" reaches coreConf, but "config" cannot, because
		// coreConf has no nfi/fig. Bag scoring keeps partial overlap rankable, so
		// the spelled-out query reaches the abbreviated identifier
		// (TestAbbreviationRecallByNameAlone, 2/4 -> 4/4). Cross-boundary substrings
		// survive because the whole name is trigrammed, separators removed, rather
		// than trigramming each split word.
		`CREATE VIRTUAL TABLE IF NOT EXISTS entity_trigram USING fts5(
			uid UNINDEXED, name UNINDEXED, name_tri, entity_type UNINDEXED, path UNINDEXED, line_number UNINDEXED,
			tokenize='unicode61 remove_diacritics 2'
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

	// Atomic rebuild via write-to-temp + rename.
	//
	// sqlite-vec (vec0) allocates fixed 1024-slot chunks and never reclaims disk space
	// when rows are deleted — slots are only marked invalid in the validity bitmap.
	// The only way to reclaim that space is to start from a fresh file.
	//
	// Strategy: write all data to a sibling temp file (.new), then atomically replace
	// the live file with os.Rename (rename(2) syscall — atomic on Linux/macOS).
	// Concurrent readers always see either the complete old file or the complete new file,
	// never a partial state. If the process crashes mid-write, the old file is untouched.
	tmpPath := s.path + ".new"
	_ = os.Remove(tmpPath)
	_ = os.Remove(tmpPath + "-wal")
	_ = os.Remove(tmpPath + "-shm")

	tmpDB, err := sql.Open("sqlite3", tmpPath+"?_journal_mode=WAL&_synchronous=NORMAL&_busy_timeout=5000")
	if err != nil {
		return fmt.Errorf("open temp search index: %w", err)
	}
	cleanupTmp := func() {
		_ = tmpDB.Close()
		_ = os.Remove(tmpPath)
		_ = os.Remove(tmpPath + "-wal")
		_ = os.Remove(tmpPath + "-shm")
	}

	if err := migrateSearchSchema(tmpDB); err != nil {
		cleanupTmp()
		return fmt.Errorf("search schema (temp): %w", err)
	}

	tx, err := tmpDB.Begin()
	if err != nil {
		cleanupTmp()
		return fmt.Errorf("search begin: %w", err)
	}
	defer func() { _ = tx.Rollback() }()

	fileStmt, err := tx.Prepare("INSERT INTO file_fts(path, name, source) VALUES (?, ?, ?)")
	if err != nil {
		cleanupTmp()
		return fmt.Errorf("prepare file_fts: %w", err)
	}
	defer func() { _ = fileStmt.Close() }()

	entityFTSStmt, err := tx.Prepare("INSERT INTO entity_fts(uid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?)")
	if err != nil {
		cleanupTmp()
		return fmt.Errorf("prepare entity_fts: %w", err)
	}
	defer func() { _ = entityFTSStmt.Close() }()

	trigramStmt, err := tx.Prepare("INSERT INTO entity_trigram(uid, name, name_tri, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?)")
	if err != nil {
		cleanupTmp()
		return fmt.Errorf("prepare entity_trigram: %w", err)
	}
	defer func() { _ = trigramStmt.Close() }()

	vecStmt, err := tx.Prepare("INSERT INTO entity_vec(rowid, embedding) VALUES (?, ?)")
	if err != nil {
		cleanupTmp()
		return fmt.Errorf("prepare entity_vec: %w", err)
	}
	defer func() { _ = vecStmt.Close() }()

	mapStmt, err := tx.Prepare("INSERT INTO entity_vec_map(uid, vec_rowid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?, ?)")
	if err != nil {
		cleanupTmp()
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
			_, _ = trigramStmt.Exec(e.UID, e.Name, identifierTrigrams(e.Name), e.Label, e.Path, e.Line)
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
		cleanupTmp()
		return fmt.Errorf("search commit: %w", err)
	}

	// Checkpoint the WAL so the .new file is self-contained before the rename.
	_, _ = tmpDB.Exec(`PRAGMA wal_checkpoint(TRUNCATE)`)
	_ = tmpDB.Close()

	// Atomically replace the live file. Concurrent readers are unaffected:
	// they hold open file descriptors to the old inode, which stays alive
	// until all readers close it.
	_ = s.db.Close()
	_ = os.Remove(s.path + "-wal")
	_ = os.Remove(s.path + "-shm")
	if err := os.Rename(tmpPath, s.path); err != nil {
		// Rename failed (e.g. cross-device) — fall back to cleaning up temp.
		_ = os.Remove(tmpPath)
		return fmt.Errorf("atomic rename search index: %w", err)
	}
	// Remove any leftover WAL/SHM from the .new file (already checkpointed).
	_ = os.Remove(tmpPath + "-wal")
	_ = os.Remove(tmpPath + "-shm")

	// Reopen the now-live file.
	newDB, err := sql.Open("sqlite3", s.path+"?_journal_mode=WAL&_synchronous=NORMAL&_busy_timeout=5000")
	if err != nil {
		return fmt.Errorf("reopen search index after rename: %w", err)
	}
	s.db = newDB

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

	sortResultsDeterministic(results)

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

	sortResultsDeterministic(results)
	return results
}

// queryTrigram runs the recall pass: the query is reduced to the same bag of
// trigrams the index stores, and the terms are OR'd so a name that shares only
// part of the query still matches. BM25 does the discrimination — a name covering
// more of the query's trigrams outranks one sharing a single common gram, and IDF
// already discounts grams like "con" that appear everywhere.
//
// Ordering matters beyond presentation: the caller fuses passes by RRF, which
// scores on rank POSITION, so an unordered result set would inject arbitrary
// positions into the fusion. The ORDER BY was missing entirely.
func (s *SearchIndex) queryTrigram(tokens []string, limit int) []SearchResult {
	if len(tokens) == 0 {
		return nil
	}

	seen := make(map[string]bool)
	var grams []string
	for _, t := range tokens {
		// A token whose normalised form is shorter than a trigram has no gram to
		// match against — the index stores only 3-grams.
		if len([]rune(normalizeForTrigrams(t))) < 3 {
			continue
		}
		for _, g := range strings.Fields(identifierTrigrams(t)) {
			if !seen[g] {
				seen[g] = true
				grams = append(grams, g)
			}
		}
	}
	if len(grams) == 0 {
		return nil
	}

	parts := make([]string, len(grams))
	for i, g := range grams {
		parts[i] = quoteToken(g)
	}
	trigramQuery := strings.Join(parts, " OR ")

	rows, err := s.db.Query(
		`SELECT uid, name, entity_type, path, line_number, rank
		 FROM entity_trigram WHERE entity_trigram MATCH ?
		 ORDER BY rank LIMIT ?`,
		trigramQuery, limit)
	if err != nil {
		return nil
	}
	defer func() { _ = rows.Close() }()

	var results []SearchResult
	for rows.Next() {
		var uid, name, entityType, path string
		var line int
		var score float64
		if err := rows.Scan(&uid, &name, &entityType, &path, &line, &score); err != nil {
			continue
		}
		results = append(results, SearchResult{
			Type: strings.ToLower(entityType), SearchType: "fts",
			Name: name, Path: path, Line: line,
			RelevanceScore: -score,
		})
	}
	sortResultsDeterministic(results)
	return results
}

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

	sortResultsDeterministic(results)

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
// Deterministic ordering
// ---------------------------------------------------------------------------

// sortResultsDeterministic orders results by descending relevance with a total order
// on ties.
//
// Without it, ranking depends on the order rows were inserted. FTS5 breaks equal BM25
// scores by rowid, and RebuildFromCache inserts while iterating a map, so two builds of
// the same corpus produce different rank POSITIONS — which the RRF fusion turns into
// different scores, not merely a different tie order. Measured before the fix: the query
// "valid" returned PKG_VALIDACAO_PAGAMENTO or validateSchema at top-1 depending on the
// build (TestSearchOrderIsDeterministic).
//
// Applied to each pass before fusion, not only to the fused list, because the pass rank
// position is what feeds the RRF score.
//
// Residual nondeterminism, deliberately not addressed: WHICH tied rows fall inside a
// pass's LIMIT window is still SQLite's choice, so a tie exactly at the fetch boundary
// can vary. Forcing that too would mean a secondary ORDER BY in SQL, giving up FTS5's
// top-N path on every query to stabilise the least significant row of an over-fetched
// window.
func sortResultsDeterministic(results []SearchResult) {
	sort.Slice(results, func(i, j int) bool {
		if results[i].RelevanceScore != results[j].RelevanceScore {
			return results[i].RelevanceScore > results[j].RelevanceScore
		}
		return deduplicationKey(results[i]) < deduplicationKey(results[j])
	})
}

// ---------------------------------------------------------------------------
// Trigram bags
// ---------------------------------------------------------------------------

// normalizeForTrigrams folds an identifier to the alphabet trigrams are built over:
// lowercase alphanumerics, separators dropped. Dropping rather than splitting on
// separators keeps substrings that straddle a word boundary findable — "reconf" still
// occurs in coreconf.
func normalizeForTrigrams(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range strings.ToLower(s) {
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(r)
		}
	}
	return b.String()
}

// identifierTrigrams renders an identifier as the space-separated bag of its overlapping
// 3-grams ("coreConf" -> "cor ore rec eco con onf"), which a word tokenizer then indexes
// as ordinary terms. Inputs too short to yield a trigram are emitted whole so they are at
// least present in the column.
func identifierTrigrams(name string) string {
	norm := []rune(normalizeForTrigrams(name))
	if len(norm) < 3 {
		return string(norm)
	}
	grams := make([]string, 0, len(norm)-2)
	for i := 0; i+3 <= len(norm); i++ {
		grams = append(grams, string(norm[i:i+3]))
	}
	return strings.Join(grams, " ")
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
