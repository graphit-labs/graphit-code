package ast

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"
	"unicode/utf8"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugBulkInsertStringIntegrity checks that a string survives a bulk insert unchanged.
//
// On the real corpus, indexing intermittently fails with
//
//	CALL CREATE_FTS_INDEX('SearchFile', 'sf_source', ['source'])
//	  -> Runtime exception: Failed calling LOWER: Invalid UTF-8.
//
// and scanning the table afterwards found 4 rows out of 35358 holding invalid UTF-8 — while
// every one of those files is valid UTF-8 on disk, verified byte by byte. Re-running the
// identical test then produced zero corrupt rows and no failure, so this is not bad data and
// not a size limit: it is intermittent corruption somewhere between a Go string and what the
// database stores or returns.
//
// The candidates it distinguishes: corruption on write versus on read, and dependence on
// value size, batch size or repetition. Everything here is generated, so a failure needs no
// corpus to reproduce.
func TestLadybugBulkInsertStringIntegrity(t *testing.T) {
	if testing.Short() {
		t.Skip("writes several MiB per iteration")
	}

	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "integrity"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("conn: %v", err)
	}
	defer conn.Close()

	run := func(q string) error {
		r, e := conn.Query(q)
		if e != nil {
			return e
		}
		r.Close()
		return nil
	}
	if err := run("CREATE NODE TABLE S(uid STRING, body STRING, PRIMARY KEY(uid))"); err != nil {
		t.Fatalf("schema: %v", err)
	}

	// Accented, multi-byte content of varied length, so a truncation lands mid-character
	// often rather than rarely.
	unit := "criação de índice não padrão para pedido em açaí — linha "
	body := func(i int) string {
		var b strings.Builder
		n := 200 + (i*37)%4000
		for j := 0; j < n; j++ {
			fmt.Fprintf(&b, "%s%d\n", unit, j)
		}
		return b.String()
	}

	const rows = 400
	const batchSize = 16

	want := make(map[string]string, rows)
	stmt, err := conn.Prepare("UNWIND $batch AS row CREATE (:S {uid: row.uid, body: row.body})")
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	defer stmt.Close()

	batch := make([]map[string]any, 0, batchSize)
	flush := func() {
		if len(batch) == 0 {
			return
		}
		res, err := conn.Execute(stmt, map[string]any{"batch": batch})
		if err != nil {
			t.Fatalf("bulk insert: %v", err)
		}
		res.Close()
		batch = batch[:0]
	}
	for i := 0; i < rows; i++ {
		uid := fmt.Sprintf("r%04d", i)
		v := body(i)
		want[uid] = v
		batch = append(batch, map[string]any{"uid": uid, "body": v})
		if len(batch) == batchSize {
			flush()
		}
	}
	flush()

	res, err := conn.Query("MATCH (s:S) RETURN s.uid AS uid, s.body AS body")
	if err != nil {
		t.Fatalf("read back: %v", err)
	}
	defer res.Close()

	var scanned, mismatched, invalid int
	var examples []string
	for res.HasNext() {
		tup, e := res.Next()
		if e != nil {
			break
		}
		uv, _ := tup.GetValue(0)
		bv, _ := tup.GetValue(1)
		uid, _ := uv.(string)
		got, _ := bv.(string)
		scanned++

		if !utf8.ValidString(got) {
			invalid++
		}
		if exp, ok := want[uid]; ok && exp != got {
			mismatched++
			if len(examples) < 3 {
				examples = append(examples, fmt.Sprintf("%s: wrote %d bytes, read %d bytes, valid=%v",
					uid, len(exp), len(got), utf8.ValidString(got)))
			}
		}
	}

	t.Logf("rows written=%d read=%d mismatched=%d invalid-utf8=%d", rows, scanned, mismatched, invalid)
	for _, e := range examples {
		t.Logf("  %s", e)
	}

	if scanned != rows {
		t.Errorf("read back %d rows, wrote %d", scanned, rows)
	}
	if mismatched > 0 {
		t.Errorf("%d of %d rows did not survive the round trip — bulk insert or read corrupts "+
			"string values", mismatched, rows)
	}
	if invalid > 0 {
		t.Errorf("%d rows came back as invalid UTF-8 although every input was valid", invalid)
	}
}
