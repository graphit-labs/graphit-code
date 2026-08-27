package ast

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// verifyFixture stages a real file and a graph that describes it, and returns the
// project root plus an open database.
//
// The graph is built by hand rather than by indexing, because the subject of these
// tests is a database that returns text no parser ever produced: LadybugDB's silent
// corruption hands back the valid text of ANOTHER row. Indexing can only produce
// correct rows, so it cannot stage the case at all.
func verifyFixture(t *testing.T) (string, *LadybugBackend) {
	t.Helper()

	root := t.TempDir()
	write := func(rel, body string) {
		p := filepath.Join(root, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		if err := os.WriteFile(p, []byte(body), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}

	write("engine/store.go", `package engine

// The buffer is flushed on close, never per write.
func Flush() {}
`)
	write("ui/panel.tsx", `export function Panel() {
  // Renders the sidebar and nothing else.
  return null
}
`)

	db := NewLadybugDB(LadybugConfig{DBPath: filepath.Join(t.TempDir(), "ladybugdb")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	t.Cleanup(func() { _ = db.Close() })

	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Comment", "Function"}}); err != nil {
		t.Fatalf("schema: %v", err)
	}
	return root, db
}

// The recheck pause is the probe's discriminator in production, and dead weight in a
// test whose database nobody is writing to.
func init() { verifyRecheckDelay = 0 }

func seed(t *testing.T, db *LadybugBackend, q string) {
	t.Helper()
	if _, err := db.Execute(context.Background(), q, nil); err != nil {
		t.Fatalf("seed %q: %v", q, err)
	}
}

// A graph that agrees with disk must report NOTHING. This half matters as much as the
// detection half: a probe that reports on a healthy corpus is a probe that gets turned
// off, and then the one real divergence has nowhere to appear.
func TestVerifyReportsNoDivergenceOnAGraphThatMatchesDisk(t *testing.T) {
	root, db := verifyFixture(t)

	seed(t, db, `CREATE (:Comment {uid:'c1', name:'The buffer is flushed on close, never per write.',
		path:'engine/store.go', line_number:3, end_line:3, is_stub:false, is_dependency:false})`)
	seed(t, db, `CREATE (:Comment {uid:'c2', name:'Renders the sidebar and nothing else.',
		path:'ui/panel.tsx', line_number:2, end_line:2, is_stub:false, is_dependency:false})`)
	// A declaration too: its name has to appear on the line the graph claims.
	seed(t, db, `CREATE (:Function {uid:'f1', name:'Flush', path:'engine/store.go',
		line_number:4, end_line:4, is_stub:false, is_dependency:false})`)

	report, err := VerifyGraphAgainstDisk(context.Background(), db, root)
	if err != nil {
		t.Fatalf("verify: %v", err)
	}
	if !report.Clean() {
		t.Fatalf("healthy graph reported %d divergence(s):\n%s",
			len(report.Divergences), FormatVerifyReport(report))
	}
	if report.Checked != 3 {
		t.Errorf("checked %d nodes, want 3 — a probe that checks nothing is also clean", report.Checked)
	}
}

// The failure this exists for, staged exactly as it occurs: a row keeps its own path
// and line, and its TEXT is the valid text of another row. Every column agrees with
// every other, the bytes are valid UTF-8, and the value is simply not what is in that
// file.
//
// This is the case the existing check cannot see. `count(toLower(n.name))` per label
// only finds invalid UTF-8, and this text is perfectly well-formed — it just belongs to
// a different file.
func TestVerifyCatchesTextBorrowedFromAnotherFile(t *testing.T) {
	root, db := verifyFixture(t)

	seed(t, db, `CREATE (:Comment {uid:'c1', name:'The buffer is flushed on close, never per write.',
		path:'engine/store.go', line_number:3, end_line:3, is_stub:false, is_dependency:false})`)
	// The corrupted row: a .tsx line carrying a comment from the Go file.
	seed(t, db, `CREATE (:Comment {uid:'c2', name:'The buffer is flushed on close, never per write.',
		path:'ui/panel.tsx', line_number:2, end_line:2, is_stub:false, is_dependency:false})`)

	report, err := VerifyGraphAgainstDisk(context.Background(), db, root)
	if err != nil {
		t.Fatalf("verify: %v", err)
	}
	if len(report.Divergences) != 1 {
		t.Fatalf("got %d divergence(s), want exactly 1:\n%s",
			len(report.Divergences), FormatVerifyReport(report))
	}

	d := report.Divergences[0]
	if d.Path != "ui/panel.tsx" {
		t.Errorf("blamed %s, want the row whose file does not contain the text", d.Path)
	}
	// The report has to show BOTH sides. The whole difficulty of this corruption is
	// that the graph's answer is plausible read alone; only the comparison exposes it.
	out := FormatVerifyReport(report)
	if !strings.Contains(out, "buffer is flushed") || !strings.Contains(out, "Renders the sidebar") {
		t.Errorf("report does not show graph and file side by side:\n%s", out)
	}
}

// A trailing comment shares its line with code, and a declaration's name is one token
// among many. Equality would flag every one of them on a healthy corpus — which is the
// specific way this probe could become noise, so it gets its own case.
func TestVerifyAcceptsTextThatSharesItsLineWithCode(t *testing.T) {
	root := t.TempDir()
	src := "package p\n\nvar Limit = 10 // hard cap, raised twice already\n"
	if err := os.WriteFile(filepath.Join(root, "cap.go"), []byte(src), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}

	db := NewLadybugDB(LadybugConfig{DBPath: filepath.Join(t.TempDir(), "ladybugdb")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()
	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Comment", "Variable"}}); err != nil {
		t.Fatalf("schema: %v", err)
	}

	seed(t, db, `CREATE (:Comment {uid:'c1', name:'hard cap, raised twice already',
		path:'cap.go', line_number:3, end_line:3, is_stub:false, is_dependency:false})`)
	seed(t, db, `CREATE (:Variable {uid:'v1', name:'Limit', path:'cap.go',
		line_number:3, end_line:3, is_stub:false, is_dependency:false})`)

	report, err := VerifyGraphAgainstDisk(context.Background(), db, root)
	if err != nil {
		t.Fatalf("verify: %v", err)
	}
	if !report.Clean() {
		t.Errorf("a trailing comment and an inline declaration were reported:\n%s",
			FormatVerifyReport(report))
	}
}

// A multi-line comment is ONE node spanning several lines, and its stored text has been
// through cleanDocstring — markers stripped, blank lines dropped, the rest joined. The
// probe has to accept that shape, or every block comment in the corpus is a divergence.
func TestVerifyAcceptsAMultiLineCommentAfterMarkerStripping(t *testing.T) {
	root := t.TempDir()
	src := "package p\n\n// First line of the note.\n//\n// Third line, after a blank one.\nfunc F() {}\n"
	if err := os.WriteFile(filepath.Join(root, "note.go"), []byte(src), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}

	db := NewLadybugDB(LadybugConfig{DBPath: filepath.Join(t.TempDir(), "ladybugdb")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()
	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Comment"}}); err != nil {
		t.Fatalf("schema: %v", err)
	}

	// Exactly what cleanDocstring produces for those three lines.
	stored := cleanDocstring("// First line of the note.\n//\n// Third line, after a blank one.")
	seed(t, db, `CREATE (:Comment {uid:'c1', name:'`+stored+`',
		path:'note.go', line_number:3, end_line:5, is_stub:false, is_dependency:false})`)

	report, err := VerifyGraphAgainstDisk(context.Background(), db, root)
	if err != nil {
		t.Fatalf("verify: %v", err)
	}
	if !report.Clean() {
		t.Errorf("a normal block comment was reported as a divergence:\n%s",
			FormatVerifyReport(report))
	}
}

// A file the graph knows and disk does not is a STALE index, not a corrupt string.
// Reporting it here would drown the signal this probe exists for, so it is skipped and
// counted as such rather than silently ignored.
func TestVerifySkipsNodesWhoseFileIsGone(t *testing.T) {
	root, db := verifyFixture(t)

	seed(t, db, `CREATE (:Comment {uid:'c1', name:'anything at all',
		path:'deleted/gone.go', line_number:1, end_line:1, is_stub:false, is_dependency:false})`)

	report, err := VerifyGraphAgainstDisk(context.Background(), db, root)
	if err != nil {
		t.Fatalf("verify: %v", err)
	}
	if !report.Clean() {
		t.Errorf("a missing file was reported as corruption:\n%s", FormatVerifyReport(report))
	}
	if report.Skipped == 0 {
		t.Error("the missing file was not counted as skipped; the totals would claim coverage it does not have")
	}
}

// The second pass is what makes this probe usable, and it needs its own case.
//
// The same corruption has a transient form — a read landing inside the daemon's write
// window returns another row's text and agrees with disk moments later. Measured while
// building this: a pass right after a sync reported 1915 divergences over a graph that
// was intact, and the next pass reported none. Without the recheck the probe would
// shout on that, get switched off, and the durable case would have nowhere to appear.
func TestVerifyTreatsARecoveringNodeAsTransientNotCorruption(t *testing.T) {
	root, db := verifyFixture(t)

	// The node in the database is CORRECT. This is the situation after a transient
	// bad read: the first pass saw garbage, and asking again returns the truth.
	seed(t, db, `CREATE (:Comment {uid:'c1', name:'Renders the sidebar and nothing else.',
		path:'ui/panel.tsx', line_number:2, end_line:2, is_stub:false, is_dependency:false})`)

	candidate := Divergence{
		Label: "Comment", Path: "ui/panel.tsx", LineNumber: 2, EndLine: 2,
		Graph: "text from some entirely different row", Window: "",
	}
	confirmed, transient, err := reverify(context.Background(), db, root, []Divergence{candidate})
	if err != nil {
		t.Fatalf("reverify: %v", err)
	}
	if len(confirmed) != 0 {
		t.Errorf("a node that agrees on the second read was reported as corruption: %+v", confirmed)
	}
	if transient != 1 {
		t.Errorf("transient count = %d, want 1 — it has to be counted, not silently dropped", transient)
	}
}

// And the mirror: a node that disagrees on BOTH reads is the durable case, and must
// survive the recheck. Without this half, the recheck could swallow everything and the
// probe would report clean forever.
func TestVerifyKeepsANodeThatDisagreesOnBothReads(t *testing.T) {
	root, db := verifyFixture(t)

	seed(t, db, `CREATE (:Comment {uid:'c1', name:'The buffer is flushed on close, never per write.',
		path:'ui/panel.tsx', line_number:2, end_line:2, is_stub:false, is_dependency:false})`)

	candidate := Divergence{
		Label: "Comment", Path: "ui/panel.tsx", LineNumber: 2, EndLine: 2,
	}
	confirmed, transient, err := reverify(context.Background(), db, root, []Divergence{candidate})
	if err != nil {
		t.Fatalf("reverify: %v", err)
	}
	if len(confirmed) != 1 {
		t.Fatalf("the durable case did not survive the recheck: confirmed=%d transient=%d",
			len(confirmed), transient)
	}
	if !strings.Contains(confirmed[0].Window, "Renders the sidebar") {
		t.Errorf("the confirmed divergence lost the file side: %+v", confirmed[0])
	}
}
