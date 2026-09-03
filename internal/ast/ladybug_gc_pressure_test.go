package ast

import (
	"fmt"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"sync"
	"testing"
	"unicode/utf8"

	lbug "github.com/LadybugDB/go-ladybug"
)

// Another attempt at the intermittent string corruption: on the real corpus,
// indexing sometimes failed with
//
//	Runtime exception: Failed calling LOWER: Invalid UTF-8.
//
// and a scan found 4 rows out of 35358 holding invalid UTF-8, from files that are
// valid UTF-8 byte for byte on disk. A rerun produced none.
//
// Every earlier probe varied the shape of the data — control characters, value
// size, batch size, invalid UTF-8 at the source — and none reproduced it. This one
// varies the runtime instead, because the symptom fits a Go string handed to C
// whose backing array is not kept alive across the call: rare, timing dependent,
// invisible to single-threaded runs, and dependent on when a collection lands
// rather than on what the bytes are.
//
// Two knobs the earlier attempts left untouched:
//
//   - a collector configured to run constantly (SetGCPercent(1)) plus explicit
//     collections from a separate goroutine while writes are in flight;
//   - a reader querying the same table throughout, which the engine does allow.
//
// Concurrent *writers* were tried first and are not a valid experiment here: the
// engine refuses them outright with "Only one write transaction at a time is
// allowed in the system", so the real indexer cannot be writing concurrently
// either. That rules the concurrent write path out as the cause rather than
// leaving it untested.
//
// Building with GOEXPERIMENT=cgocheck2 makes the same test check every pointer
// crossing into C, which would name the offending call directly:
//
//	GOEXPERIMENT=cgocheck2 go test -tags lancedb -run TestLadybugStringIntegrityUnderGCPressure ./internal/ast/
func TestLadybugStringIntegrityUnderGCPressure(t *testing.T) {
	if testing.Short() {
		t.Skip("hammers the allocator while writing to disk")
	}

	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "gcpressure"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()

	setup, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("conn: %v", err)
	}
	r, err := setup.Query("CREATE NODE TABLE S(uid STRING, body STRING, PRIMARY KEY(uid))")
	if err != nil {
		setup.Close()
		t.Fatalf("schema: %v", err)
	}
	r.Close()
	setup.Close()

	unit := "criação de índice não padrão para pedido em açaí — linha "
	body := func(w, i int) string {
		s := fmt.Sprintf("w%d-r%d|", w, i)
		for j := 0; j < 40+(i*7)%160; j++ {
			s += fmt.Sprintf("%s%d\n", unit, j)
		}
		return s
	}

	const rows = 3000
	const batchSize = 32

	oldGC := debug.SetGCPercent(1)
	defer debug.SetGCPercent(oldGC)

	stopGC := make(chan struct{})
	var gcWG sync.WaitGroup
	gcWG.Add(1)
	go func() {
		defer gcWG.Done()
		for {
			select {
			case <-stopGC:
				return
			default:
				runtime.GC()
			}
		}
	}()

	stopRead := make(chan struct{})
	var readWG sync.WaitGroup
	readWG.Add(1)
	go func() {
		defer readWG.Done()
		rc, err := lbug.OpenConnection(db)
		if err != nil {
			return
		}
		defer rc.Close()
		for {
			select {
			case <-stopRead:
				return
			default:
			}
			res, err := rc.Query("MATCH (s:S) RETURN s.body LIMIT 50")
			if err != nil {
				continue
			}
			for res.HasNext() {
				if _, err := res.Next(); err != nil {
					break
				}
			}
			res.Close()
		}
	}()

	writer, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("writer conn: %v", err)
	}
	defer writer.Close()
	stmt, err := writer.Prepare("UNWIND $batch AS row CREATE (:S {uid: row.uid, body: row.body})")
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	defer stmt.Close()

	expected := make(map[string]string, rows)
	batch := make([]map[string]any, 0, batchSize)
	flush := func() {
		if len(batch) == 0 {
			return
		}
		res, err := writer.Execute(stmt, map[string]any{"batch": batch})
		if err != nil {
			t.Fatalf("bulk insert: %v", err)
		}
		res.Close()
		batch = batch[:0]
	}
	for i := 0; i < rows; i++ {
		uid := fmt.Sprintf("r%05d", i)
		v := body(i%13, i)
		expected[uid] = v
		batch = append(batch, map[string]any{"uid": uid, "body": v})
		if len(batch) == batchSize {
			flush()
		}
	}
	flush()

	close(stopRead)
	readWG.Wait()
	close(stopGC)
	gcWG.Wait()

	readConn, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("read conn: %v", err)
	}
	defer readConn.Close()

	res, err := readConn.Query("MATCH (s:S) RETURN s.uid, s.body")
	if err != nil {
		t.Fatalf("read back: %v", err)
	}
	defer res.Close()

	seen, invalid, mismatched := 0, 0, 0
	for res.HasNext() {
		tuple, err := res.Next()
		if err != nil {
			t.Fatalf("next: %v", err)
		}
		vals, err := tuple.GetAsSlice()
		if err != nil || len(vals) < 2 {
			t.Fatalf("row shape: %v", err)
		}
		uid, _ := vals[0].(string)
		got, _ := vals[1].(string)
		seen++

		if !utf8.ValidString(got) {
			invalid++
			if invalid <= 3 {
				t.Errorf("CORRUPTION: %s came back as invalid UTF-8 (%d bytes)", uid, len(got))
			}
			continue
		}
		if w, ok := expected[uid]; ok && w != got {
			mismatched++
			if mismatched <= 3 {
				t.Errorf("CORRUPTION: %s differs — %d bytes stored, %d expected",
					uid, len(got), len(w))
			}
		}
	}

	t.Logf("rows written %d, read back %d", len(expected), seen)
	t.Logf("invalid UTF-8: %d | valid but different: %d", invalid, mismatched)
	if seen != len(expected) {
		t.Errorf("row count: read %d, wrote %d", seen, len(expected))
	}
	if invalid == 0 && mismatched == 0 {
		t.Logf("no corruption under this configuration — a constantly running collector, "+
			"%d rows in batches of %d, and a concurrent reader do not reproduce it either",
			rows, batchSize)
	}
}
