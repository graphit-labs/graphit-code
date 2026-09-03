package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugFTSSurvivesReopenAndRename isolates the last step the earlier probes never
// exercised, and the one the implementation depends on.
//
// RebuildFromCache builds into a sibling path and renames it into place, so that readers
// see either the old index or the new one and never a half-populated one. Diagnostics
// showed the rows arriving intact through that cycle while every FTS query came back
// empty on the reopened database — so the question is which of the three steps loses the
// index: CHECKPOINT, close/reopen, or the rename.
//
// Each step is asserted separately: a single "it does not work" would not say which part
// of the swap to redesign.
func TestLadybugFTSSurvivesReopenAndRename(t *testing.T) {
	dir := t.TempDir()
	pathA := filepath.Join(dir, "one")
	pathB := filepath.Join(dir, "two")

	type session struct {
		db   *lbug.Database
		conn *lbug.Connection
	}
	openAt := func(p string) *session {
		db, err := lbug.OpenDatabase(p, lbug.DefaultSystemConfig())
		if err != nil {
			t.Skipf("ladybug unavailable: %v", err)
		}
		conn, err := lbug.OpenConnection(db)
		if err != nil {
			t.Fatalf("conn: %v", err)
		}
		run := func(q string) error {
			r, e := conn.Query(q)
			if e != nil {
				return e
			}
			r.Close()
			return nil
		}
		_ = run("INSTALL fts")
		if err := run("LOAD EXTENSION fts"); err != nil {
			t.Skipf("fts unavailable: %v", err)
		}
		return &session{db: db, conn: conn}
	}
	closeSession := func(s *session) {
		s.conn.Close()
		s.db.Close()
	}
	hits := func(s *session, q string) []string {
		res, err := s.conn.Query(fmt.Sprintf(
			"CALL QUERY_FTS_INDEX('T','t_idx','%s') RETURN node.name AS n ORDER BY score DESC LIMIT 10", q))
		if err != nil {
			t.Logf("    query error: %v", err)
			return nil
		}
		defer res.Close()
		var out []string
		for res.HasNext() {
			tup, e := res.Next()
			if e != nil {
				break
			}
			if v, e2 := tup.GetValue(0); e2 == nil {
				out = append(out, fmt.Sprint(v))
			}
		}
		return out
	}

	s1 := openAt(pathA)
	for _, q := range []string{
		"CREATE NODE TABLE T(uid STRING, name STRING, PRIMARY KEY(uid))",
		"CALL CREATE_FTS_INDEX('T','t_idx',['name'])",
	} {
		r, err := s1.conn.Query(q)
		if err != nil {
			t.Fatalf("%q: %v", q, err)
		}
		r.Close()
	}
	stmt, err := s1.conn.Prepare("UNWIND $batch AS row CREATE (:T {uid: row.uid, name: row.name})")
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	res, err := s1.conn.Execute(stmt, map[string]any{"batch": []map[string]any{
		{"uid": "x1", "name": "alpha_token"},
		{"uid": "x2", "name": "beta_token"},
	}})
	if err != nil {
		t.Fatalf("bulk insert: %v", err)
	}
	res.Close()
	stmt.Close()

	inSession := hits(s1, "alpha")
	t.Logf("(1) same session, after bulk insert            -> %v", inSession)
	if len(inSession) == 0 {
		t.Fatal("index empty in the same session — contradicts TestLadybugFTSBulkInsertMaintainsIndex")
	}

	if r, err := s1.conn.Query("CHECKPOINT"); err != nil {
		t.Logf("    CHECKPOINT error: %v", err)
	} else {
		r.Close()
	}
	closeSession(s1)

	s2 := openAt(pathA)
	afterReopen := hits(s2, "alpha")
	t.Logf("(2) after CHECKPOINT + close + reopen same path -> %v", afterReopen)
	survivesReopen := len(afterReopen) > 0

	closeSession(s2)
	if err := os.Rename(pathA, pathB); err != nil {
		t.Fatalf("rename db: %v", err)
	}
	if _, err := os.Stat(pathA + ".wal"); err == nil {
		if err := os.Rename(pathA+".wal", pathB+".wal"); err != nil {
			t.Logf("    rename wal: %v", err)
		}
	} else {
		t.Logf("    no .wal alongside the database after CHECKPOINT (%v)", err)
	}

	s3 := openAt(pathB)
	afterRename := hits(s3, "alpha")
	t.Logf("(3) after rename to a new path                  -> %v", afterRename)
	survivesRename := len(afterRename) > 0
	closeSession(s3)

	t.Logf("SUMMARY: survives reopen=%v, survives rename=%v", survivesReopen, survivesRename)

	if !survivesReopen {
		t.Error("an FTS index does not survive closing and reopening the database — a build-then-swap " +
			"rebuild cannot carry FTS state, and indexes must be created after reopening")
	}
	if survivesReopen && !survivesRename {
		t.Error("an FTS index survives reopen but not a rename of the database file — the extension " +
			"stores index data keyed to the database path, so the atomic-swap rebuild needs a different " +
			"mechanism than renaming a sibling file")
	}
}
