package ast

import (
	"database/sql"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	sqlite_vec "github.com/asg017/sqlite-vec-go-bindings/cgo"
	_ "github.com/mattn/go-sqlite3"
)

func init() {
	sqlite_vec.Auto()
}

type SearchIndex struct {
	db   *sql.DB
	path string
	mu   sync.RWMutex
}

func OpenSearchIndex(dbPath string) (*SearchIndex, error) {
	if err := os.MkdirAll(filepath.Dir(dbPath), 0o755); err != nil {
		return nil, fmt.Errorf("search index dir: %w", err)
	}

	db, err := sql.Open("sqlite3", dbPath+"?_journal_mode=WAL&_synchronous=NORMAL&_busy_timeout=5000")
	if err != nil {
		return nil, fmt.Errorf("search index open: %w", err)
	}

	ddl := []string{

		`CREATE VIRTUAL TABLE IF NOT EXISTS file_fts USING fts5(
			path, name, source,
			tokenize='unicode61'
		)`,
		`CREATE VIRTUAL TABLE IF NOT EXISTS entity_fts USING fts5(
			uid, name, docstring, entity_type, path, line_number UNINDEXED,
			tokenize='unicode61'
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
			_ = db.Close()
			return nil, fmt.Errorf("search index schema %q: %w", q[:min(60, len(q))], err)
		}
	}

	return &SearchIndex{db: db, path: dbPath}, nil
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
	_, _ = tx.Exec("DELETE FROM entity_vec")
	_, _ = tx.Exec("DELETE FROM entity_vec_map")

	fileStmt, _ := tx.Prepare("INSERT INTO file_fts(path, name, source) VALUES (?, ?, ?)")
	defer func() { _ = fileStmt.Close() }()

	entityFTSStmt, _ := tx.Prepare("INSERT INTO entity_fts(uid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?)")
	defer func() { _ = entityFTSStmt.Close() }()

	vecStmt, _ := tx.Prepare("INSERT INTO entity_vec(rowid, embedding) VALUES (?, ?)")
	defer func() { _ = vecStmt.Close() }()

	mapStmt, _ := tx.Prepare("INSERT INTO entity_vec_map(uid, vec_rowid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?, ?)")
	defer func() { _ = mapStmt.Close() }()

	fileCount, entityCount, vecCount := 0, 0, 0
	rowID := int64(1)

	for relPath, entry := range entries {
		_, _ = fileStmt.Exec(relPath, filepath.Base(relPath), entry.Source)
		fileCount++

		for _, e := range entry.Entities {
			_, _ = entityFTSStmt.Exec(e.UID, e.Name, e.Docstring, e.Label, e.Path, e.Line)
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

	fmt.Fprintf(os.Stderr, "    Search index rebuild: %d files, %d entities, %d vectors in %.0fms\n",
		fileCount, entityCount, vecCount, time.Since(t0).Seconds()*1000)
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

		_, _ = tx.Exec("INSERT INTO file_fts(path, name, source) VALUES (?, ?, ?)",
			p, filepath.Base(p), entry.Source)
		fileCount++

		for _, e := range entry.Entities {
			_, _ = tx.Exec("INSERT INTO entity_fts(uid, name, docstring, entity_type, path, line_number) VALUES (?, ?, ?, ?, ?, ?)",
				e.UID, e.Name, e.Docstring, e.Label, e.Path, e.Line)
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

	fmt.Fprintf(os.Stderr, "    Search index update: %d files, %d entities, %d vectors in %.0fms\n",
		fileCount, entityCount, vecCount, time.Since(t0).Seconds()*1000)
	return nil
}

func (s *SearchIndex) SearchFiles(query string, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	limit := topK
	if limit <= 0 {
		limit = -1
	}

	rows, err := s.db.Query(
		`SELECT path, name, bm25(file_fts) AS score
		 FROM file_fts WHERE file_fts MATCH ?
		 ORDER BY score LIMIT ?`,
		escapeFTSSQLite(query), limit)
	if err != nil {
		return nil, nil
	}
	defer func() { _ = rows.Close() }()

	var results []SearchResult
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
	return results, nil
}

func (s *SearchIndex) SearchEntities(query string, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	limit := topK
	if limit <= 0 {
		limit = -1
	}

	rows, err := s.db.Query(
		`SELECT uid, name, docstring, entity_type, path, line_number, bm25(entity_fts) AS score
		 FROM entity_fts WHERE entity_fts MATCH ?
		 ORDER BY score LIMIT ?`,
		escapeFTSSQLite(query), limit)
	if err != nil {
		return nil, nil
	}
	defer func() { _ = rows.Close() }()

	var results []SearchResult
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
	return results, nil
}

func (s *SearchIndex) SemanticSearch(queryVec []float32, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

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
		WHERE v.embedding MATCH ?
		ORDER BY v.distance
		LIMIT ?`, blob, limit)
	if err != nil {
		return nil, nil
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
		results = append(results, SearchResult{
			Type: strings.ToLower(entityType), SearchType: "semantic",
			Name: name, Path: path, Line: line, Docstring: docstring,
			RelevanceScore: 1.0 - math.Min(distance, 1.0),
		})
	}
	return results, nil
}

func escapeFTSSQLite(query string) string {
	terms := strings.Fields(query)
	for i, t := range terms {
		t = strings.ReplaceAll(t, "\"", "")
		if t != "" {
			terms[i] = "\"" + t + "\""
		}
	}
	return strings.Join(terms, " ")
}
