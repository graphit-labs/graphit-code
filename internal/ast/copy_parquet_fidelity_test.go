package ast

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

// TestCopyMappingAndFidelity asks the two questions that decide whether a staging format can
// be swapped without losing data.
//
//  1. Does COPY map columns by NAME or by POSITION? JSON maps by name — a Go map has no
//     order, and TestCopyJSONCarriesVectorColumn shows the values landing in the right
//     columns anyway. If Parquet maps by position instead, an encoder that emits columns in
//     any other order than the DDL would corrupt every row silently.
//
//  2. Does the round trip preserve bytes that are not valid UTF-8? Source files are not
//     guaranteed to be UTF-8, and Go's encoding/json replaces invalid bytes with U+FFFD
//     without reporting anything — so this is a question about the CURRENT design as much
//     as about any replacement.
//
// GRAPHIT_COPY_ROWSIZE=1 go test -run TestCopyMappingAndFidelity ./internal/ast/ -v
func TestCopyMappingAndFidelity(t *testing.T) {
	if os.Getenv("GRAPHIT_COPY_ROWSIZE") == "" {
		t.Skip("set GRAPHIT_COPY_ROWSIZE=1")
	}
	dir := t.TempDir()
	st, err := openProbeStoreWithJSON(t, filepath.Join(dir, "db"))
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer func() { _ = st.Close() }()

	if err := st.Exec(`CREATE NODE TABLE Src(path STRING, name STRING, lang STRING,
		line INT64, dep BOOLEAN, PRIMARY KEY(path))`, nil); err != nil {
		t.Fatalf("ddl: %v", err)
	}
	if err := st.Exec(`CREATE (:Src {path: 'a.go', name: 'Alpha', lang: 'go',
		line: 7, dep: true})`, nil); err != nil {
		t.Fatalf("seed: %v", err)
	}

	shuffled := filepath.Join(dir, "shuffled.parquet")
	if err := st.Exec(fmt.Sprintf(
		`COPY (MATCH (s:Src) RETURN s.lang, s.name, s.dep, s.path, s.line) TO '%s'`,
		shuffled), nil); err != nil {
		t.Fatalf("export shuffled: %v", err)
	}
	if err := st.Exec(`CREATE NODE TABLE DstShuffled(path STRING, name STRING, lang STRING,
		line INT64, dep BOOLEAN, PRIMARY KEY(path))`, nil); err != nil {
		t.Fatalf("ddl dst: %v", err)
	}
	err = st.Exec(fmt.Sprintf("COPY DstShuffled FROM '%s'", shuffled), nil)
	rows, _ := st.Query(`MATCH (n:DstShuffled) RETURN n.path AS path, n.name AS name,
		n.lang AS lang, n.line AS line`, nil)
	if err != nil {
		t.Logf("MAPPING: COPY rejected the reordered file -> %v", err)
	} else if len(rows) == 1 {
		p := ladybugstore.Str(rows[0]["path"])
		n := ladybugstore.Str(rows[0]["name"])
		l := ladybugstore.Str(rows[0]["lang"])
		switch {
		case p == "a.go" && n == "Alpha" && l == "go":
			t.Logf("MAPPING: BY NAME — reordered columns landed correctly")
		default:
			t.Logf("MAPPING: BY POSITION — path=%q name=%q lang=%q line=%d "+
				"(the encoder must emit DDL order or every row is silently wrong)",
				p, n, l, ladybugstore.Int64(rows[0]["line"]))
		}
	}

	invalid := "package main // \xff\xfe raw bytes \x00 and a lone surrogate \xed\xa0\x80\n"
	if err := st.Exec(`CREATE NODE TABLE Bytes(path STRING, source STRING,
		PRIMARY KEY(path))`, nil); err != nil {
		t.Fatalf("ddl bytes: %v", err)
	}
	stage := filepath.Join(dir, "bytes.json")
	if err := writeJSONFile(stage, []map[string]any{
		{"path": "b.go", "source": invalid},
	}); err != nil {
		t.Fatalf("stage bytes: %v", err)
	}
	if err := st.Exec(fmt.Sprintf("COPY Bytes FROM '%s'", stage), nil); err != nil {
		t.Logf("FIDELITY(json): COPY refused the row -> %v", err)
	} else {
		got, _ := st.Query(`MATCH (n:Bytes) WHERE n.path = 'b.go'
			RETURN n.source AS s, size(n.source) AS n`, nil)
		if len(got) == 1 {
			stored := ladybugstore.Str(got[0]["s"])
			t.Logf("FIDELITY(json): wrote %d bytes, stored %d, identical=%v",
				len(invalid), len(stored), stored == invalid)
			t.Logf("   stored = %q", stored)
		}
	}

	raw, rerr := os.ReadFile(stage)
	t.Logf("FIDELITY(stage file): %d bytes on disk (err=%v)", len(raw), rerr)
	t.Logf("   on disk = %q", string(raw))

	if err := st.Exec(`UNWIND $b AS r CREATE (:Bytes {path: r.path, source: r.source})`,
		map[string]any{"b": []any{
			map[string]any{"path": "c.go", "source": invalid},
		}}); err != nil {
		t.Logf("FIDELITY(unwind): refused -> %v", err)
	} else {
		got, _ := st.Query(`MATCH (n:Bytes) WHERE n.path = 'c.go' RETURN n.source AS s`, nil)
		if len(got) == 1 {
			stored := ladybugstore.Str(got[0]["s"])
			t.Logf("FIDELITY(unwind): wrote %d bytes, stored %d, identical=%v",
				len(invalid), len(stored), stored == invalid)
		}
	}
}
