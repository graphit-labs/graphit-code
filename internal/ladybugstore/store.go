// Package ladybugstore is the thin layer every LadybugDB store in this project sits on.
//
// It exists because two packages need the same primitives and neither may import the
// other: internal/ast holds the code graph, internal/wiki holds the documentation and
// memory indexes, and internal/wiki must not depend on internal/ast. Before the search
// indexes moved off SQLite there was nothing to share — the AST spoke Cypher and the wiki
// spoke SQL. Now they speak the same language, and the open/query/close mechanics, the
// extension loading, and the value coercion out of a graph result were about to be written
// twice.
//
// It is deliberately small. Anything that knows what a node MEANS belongs to the package
// that owns that node, not here.
package ladybugstore

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"

	lbug "github.com/LadybugDB/go-ladybug"
)

// Store is one open LadybugDB database.
//
// The engine allows exactly ONE read-write handle per database. A second Open on the same
// path fails rather than blocking, so a process that already holds a store must pass it
// around instead of opening another. Read-only opens are not subject to that and may run
// alongside a writer.
type Store struct {
	db   *lbug.Database
	conn *lbug.Connection
	path string

	mu    sync.Mutex
	stmts map[string]*lbug.PreparedStatement
}

// Open opens or creates a store read-write.
func Open(path string) (*Store, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, fmt.Errorf("store dir: %w", err)
	}
	return open(path, lbug.DefaultSystemConfig())
}

// OpenReadOnly opens an existing store for reading. It does not take the write slot, so it
// is what any probe or lookup must use — a read that opened read-write would lock out the
// process trying to reindex.
func OpenReadOnly(path string) (*Store, error) {
	cfg := lbug.DefaultSystemConfig()
	cfg.ReadOnly = true
	return open(path, cfg)
}

func open(path string, cfg lbug.SystemConfig) (*Store, error) {
	db, err := lbug.OpenDatabase(path, cfg)
	if err != nil {
		return nil, fmt.Errorf("open %s: %w", filepath.Base(path), err)
	}
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		db.Close()
		return nil, fmt.Errorf("connect %s: %w", filepath.Base(path), err)
	}
	return &Store{db: db, conn: conn, path: path, stmts: map[string]*lbug.PreparedStatement{}}, nil
}

func (s *Store) Path() string { return s.path }

func (s *Store) Close() error {
	if s == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, st := range s.stmts {
		st.Close()
	}
	s.stmts = map[string]*lbug.PreparedStatement{}
	if s.conn != nil {
		s.conn.Close()
		s.conn = nil
	}
	if s.db != nil {
		s.db.Close()
		s.db = nil
	}
	return nil
}

// Checkpoint flushes the write-ahead log into the store file.
func (s *Store) Checkpoint() {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.conn == nil {
		return
	}
	if res, err := s.conn.Query("CHECKPOINT"); err == nil {
		res.Close()
	}
}

// Exec runs a statement and discards its result.
func (s *Store) Exec(cypher string, params map[string]any) error {
	res, err := s.run(cypher, params)
	if err != nil {
		return err
	}
	res.Close()
	return nil
}

// Query runs a statement and materialises its rows.
//
// Rows are read fully rather than streamed: LadybugDB result sets are tied to the
// connection, and a caller holding one open across other calls on the same connection is
// how a query ends up reading from a cursor another statement has already invalidated.
// Callers that must not materialise a whole corpus — the file-text walk, for one — page by
// key instead.
func (s *Store) Query(cypher string, params map[string]any) ([]map[string]any, error) {
	res, err := s.run(cypher, params)
	if err != nil {
		return nil, err
	}
	defer res.Close()

	cols := res.GetColumnNames()
	var out []map[string]any
	for res.HasNext() {
		tup, err := res.Next()
		if err != nil {
			break
		}
		row := make(map[string]any, len(cols))
		for i, name := range cols {
			v, err := tup.GetValue(uint64(i))
			if err != nil {
				continue
			}
			row[name] = v
		}
		out = append(out, row)
	}
	return out, nil
}

// CountQuery runs a statement whose first column of its first row is a count.
func (s *Store) CountQuery(cypher string, params map[string]any) (int, error) {
	recs, err := s.Query(cypher, params)
	if err != nil {
		return 0, err
	}
	if len(recs) == 0 {
		return 0, nil
	}
	for _, v := range recs[0] {
		return int(Int64(v)), nil
	}
	return 0, nil
}

func (s *Store) run(cypher string, params map[string]any) (*lbug.QueryResult, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.conn == nil {
		return nil, fmt.Errorf("store is closed")
	}
	if len(params) == 0 {
		return s.conn.Query(cypher)
	}
	stmt, ok := s.stmts[cypher]
	if !ok {
		var err error
		stmt, err = s.conn.Prepare(cypher)
		if err != nil {
			return nil, fmt.Errorf("prepare: %w", err)
		}
		s.stmts[cypher] = stmt
	}
	return s.conn.Execute(stmt, params)
}

// IsAlreadyExists reports whether an error is the engine complaining that something being
// created is already there, which every idempotent schema step has to tolerate.
func IsAlreadyExists(err error) bool {
	return err != nil && strings.Contains(strings.ToLower(err.Error()), "already exists")
}

// Str, Int64 and Float64 coerce a value out of a graph result.
//
// They exist because the C API returns concrete Go types that vary with the column's
// declared type, and a caller that type-asserts the one it expects gets a zero value on any
// other — silently. Coercing in one place makes a surprising type a wrong number rather
// than a missing field.
func Str(v any) string {
	if v == nil {
		return ""
	}
	if s, ok := v.(string); ok {
		return s
	}
	return fmt.Sprint(v)
}

func Int64(v any) int64 {
	switch n := v.(type) {
	case int64:
		return n
	case int:
		return int64(n)
	case int32:
		return int64(n)
	case float64:
		return int64(n)
	case bool:
		if n {
			return 1
		}
		return 0
	default:
		return 0
	}
}

func Float64(v any) float64 {
	switch n := v.(type) {
	case float64:
		return n
	case float32:
		return float64(n)
	case int64:
		return float64(n)
	case int:
		return float64(n)
	default:
		return 0
	}
}

// Float32Slice coerces a stored FLOAT[N] column back into a vector.
func Float32Slice(v any) []float32 {
	switch s := v.(type) {
	case []float32:
		return s
	case []float64:
		out := make([]float32, len(s))
		for i, f := range s {
			out[i] = float32(f)
		}
		return out
	case []any:
		out := make([]float32, len(s))
		for i, e := range s {
			out[i] = float32(Float64(e))
		}
		return out
	default:
		return nil
	}
}
