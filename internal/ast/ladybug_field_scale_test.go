package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"unicode/utf8"

	lbug "github.com/LadybugDB/go-ladybug"
)

// The last untried dimension of the intermittent string corruption.
//
// The field failure wrote 35358 rows whose values were whole source files and
// produced 4 rows of invalid UTF-8 — about one in nine thousand. Every probe so
// far has been three orders of magnitude smaller in row count and two in value
// size, and each ruled out a hypothesis about the *shape* of the data rather
// than about volume. If the cause is a buffer or arena that only misbehaves
// after a certain amount of traffic, no small probe can see it.
//
// This one matches the field run: same row count, same order of value size, and
// the same FTS index build that raised the original exception.
//
//	go test -tags lancedb -run TestLadybugFieldScaleStringIntegrity -timeout 60m ./internal/ast/
//
// It is skipped unless GRAPHIT_FIELD_SCALE is set, because it writes on the
// order of a gigabyte and takes minutes.
func TestLadybugFieldScaleStringIntegrity(t *testing.T) {
	if testing.Short() {
		t.Skip("writes ~1 GB")
	}
	if !envSet("GRAPHIT_FIELD_SCALE") {
		t.Skip("set GRAPHIT_FIELD_SCALE=1 to run the field-scale probe (minutes, ~1 GB)")
	}

	const rows = 35358
	const batchSize = 64

	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "fieldscale"), lbug.DefaultSystemConfig())
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
	_ = run("INSTALL fts")
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts extension unavailable: %v", err)
	}
	if err := run("CREATE NODE TABLE SearchFile(uid STRING, source STRING, PRIMARY KEY(uid))"); err != nil {
		t.Fatalf("schema: %v", err)
	}

	// Values sized like source files, with accented multi-byte text so any
	// mangling lands mid-character and shows as invalid UTF-8 rather than as a
	// plausible substitution. The C1 control characters that the real corpus
	// carries (CP1252 read as Latin-1) are included, since they are legal UTF-8
	// and were present in the field data.
	// \u0083 and \u0087 are the C1 control characters the real corpus
	// carries, from CP1252 text read as Latin-1. They are legal UTF-8 and
	// were present in the field data, so they belong here — as escapes,
	// because invisible bytes in a literal are a trap for whoever edits
	// this next.
	unit := "cria\u00e7\u00e3o de \u00edndice n\u00e3o padr\u00e3o \u0083 para pedido em a\u00e7a\u00ed \u0087 \u2014 linha "
	body := func(i int) string {
		var b strings.Builder
		lines := 40 + (i*17)%600 // roughly 2 KB to 30 KB, like a source tree
		b.Grow(lines * (len(unit) + 6))
		for j := 0; j < lines; j++ {
			fmt.Fprintf(&b, "%s%d\n", unit, j)
		}
		return b.String()
	}

	stmt, err := conn.Prepare("UNWIND $batch AS row CREATE (:SearchFile {uid: row.uid, source: row.source})")
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	defer stmt.Close()

	lengths := make(map[string]int, rows)
	batch := make([]map[string]any, 0, batchSize)
	totalBytes := 0
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
		uid := fmt.Sprintf("f%05d", i)
		v := body(i)
		lengths[uid] = len(v)
		totalBytes += len(v)
		batch = append(batch, map[string]any{"uid": uid, "source": v})
		if len(batch) == batchSize {
			flush()
		}
		if i%5000 == 0 && i > 0 {
			t.Logf("  written %d/%d rows (%d MB)", i, rows, totalBytes/(1<<20))
		}
	}
	flush()
	t.Logf("wrote %d rows, %d MB", rows, totalBytes/(1<<20))

	// The original failure surfaced here: LOWER refusing the stored bytes while
	// the index was being built. Building it is part of the probe, not scaffolding.
	ftsErr := run("CALL CREATE_FTS_INDEX('SearchFile','sf_source',['source'])")
	if ftsErr != nil {
		t.Errorf("REPRODUCED at index build: %v", ftsErr)
	}

	// Independently of the index, scan every row back.
	res, err := conn.Query("MATCH (s:SearchFile) RETURN s.uid, s.source")
	if err != nil {
		t.Fatalf("scan: %v", err)
	}
	defer res.Close()

	seen, invalid, wrongLen := 0, 0, 0
	for res.HasNext() {
		tuple, err := res.Next()
		if err != nil {
			t.Fatalf("next at row %d: %v", seen, err)
		}
		vals, err := tuple.GetAsSlice()
		if err != nil || len(vals) < 2 {
			t.Fatalf("row shape at %d: %v", seen, err)
		}
		uid, _ := vals[0].(string)
		got, _ := vals[1].(string)
		seen++

		if !utf8.ValidString(got) {
			invalid++
			if invalid <= 5 {
				t.Errorf("REPRODUCED: %s came back as invalid UTF-8 (%d bytes stored, %d expected)",
					uid, len(got), lengths[uid])
			}
			continue
		}
		if want, ok := lengths[uid]; ok && want != len(got) {
			wrongLen++
			if wrongLen <= 5 {
				t.Errorf("REPRODUCED: %s is valid UTF-8 but %d bytes, expected %d",
					uid, len(got), want)
			}
		}
	}

	t.Logf("rows read back: %d/%d", seen, rows)
	t.Logf("invalid UTF-8: %d | valid but wrong length: %d", invalid, wrongLen)
	if seen != rows {
		t.Errorf("read back %d rows, wrote %d", seen, rows)
	}
	if invalid == 0 && wrongLen == 0 && ftsErr == nil {
		t.Logf("NOT REPRODUCED at field scale: %d rows and %d MB of accented text, "+
			"including C1 control characters, survived intact and the FTS index built "+
			"cleanly. Volume plus value size is now ruled out along with the rest.",
			rows, totalBytes/(1<<20))
	}
}

func envSet(k string) bool {
	v := os.Getenv(k)
	return v != "" && v != "0"
}
