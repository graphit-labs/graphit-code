package ast

import (
	"path/filepath"
	"testing"
	"time"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugSharedDatabaseMVCC is the gate for in-place incremental writes.
//
// Today every incremental copies the whole database, mutates the copy and
// atomically swaps it in, which is what makes reads lock-free — and closing that
// mutated copy costs 0.2–5.0 s. Writing in place instead would remove the copy,
// the second open and the close entirely, but only if LadybugDB lets a reader
// connection keep serving consistent data while a sibling connection on the SAME
// Database object writes. liblbug's header scopes its concurrency guarantee to
// "multiple connections … to the same Database instance", so this test checks
// exactly that:
//
//  1. a reader does not block (or error) while a write transaction is open;
//  2. the reader sees the pre-commit snapshot, not a torn intermediate state;
//  3. after COMMIT the reader observes the new data.
func TestLadybugSharedDatabaseMVCC(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "mvcc"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()

	writer, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("writer conn: %v", err)
	}
	defer writer.Close()
	reader, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("reader conn (second connection on same Database): %v", err)
	}
	defer reader.Close()

	exec := func(c *lbug.Connection, q string) error {
		res, err := c.Query(q)
		if err != nil {
			return err
		}
		res.Close()
		return nil
	}
	count := func(c *lbug.Connection) (int64, time.Duration, error) {
		t0 := time.Now()
		res, err := c.Query("MATCH (n:T) RETURN count(n) AS c")
		if err != nil {
			return 0, time.Since(t0), err
		}
		defer res.Close()
		var n int64
		if res.HasNext() {
			tup, tErr := res.Next()
			if tErr == nil {
				if v, gErr := tup.GetValue(0); gErr == nil {
					switch x := v.(type) {
					case int64:
						n = x
					case int:
						n = int64(x)
					}
				}
			}
		}
		return n, time.Since(t0), nil
	}

	if err := exec(writer, "CREATE NODE TABLE T(id INT64, PRIMARY KEY(id))"); err != nil {
		t.Fatalf("create: %v", err)
	}
	for i := 1; i <= 10; i++ {
		if err := exec(writer, "CREATE (:T {id: "+itoa(i)+"})"); err != nil {
			t.Fatalf("seed %d: %v", i, err)
		}
	}

	base, _, err := count(reader)
	if err != nil {
		t.Fatalf("baseline read: %v", err)
	}
	t.Logf("baseline rows visible to reader: %d", base)

	if err := exec(writer, "BEGIN TRANSACTION"); err != nil {
		t.Fatalf("BEGIN: %v", err)
	}
	for i := 11; i <= 20; i++ {
		if err := exec(writer, "CREATE (:T {id: "+itoa(i)+"})"); err != nil {
			t.Fatalf("in-tx insert %d: %v", i, err)
		}
	}

	mid, dur, rErr := count(reader)
	switch {
	case rErr != nil:
		t.Logf(">>> reader ERRORED during open write transaction after %v: %v", dur, rErr)
		t.Errorf("reader cannot serve queries during a write transaction — in-place writes would break lock-free reads")
	case dur > 2*time.Second:
		t.Errorf("reader BLOCKED for %v during a write transaction — in-place writes would stall readers", dur)
	case mid != base:
		t.Errorf(">>> reader saw UNCOMMITTED state: %d rows (baseline %d) — no snapshot isolation", mid, base)
	default:
		t.Logf(">>> reader served the pre-commit snapshot (%d rows) in %v — snapshot isolation holds", mid, dur)
	}

	if err := exec(writer, "COMMIT"); err != nil {
		t.Fatalf("COMMIT: %v", err)
	}

	after, dur2, err := count(reader)
	if err != nil {
		t.Fatalf("post-commit read: %v", err)
	}
	t.Logf(">>> after COMMIT reader sees %d rows in %v (want 20)", after, dur2)
	if after != 20 {
		t.Errorf("reader did not observe committed data: got %d, want 20", after)
	}
}

func itoa(i int) string {
	if i == 0 {
		return "0"
	}
	var b [20]byte
	pos := len(b)
	for i > 0 {
		pos--
		b[pos] = byte('0' + i%10)
		i /= 10
	}
	return string(b[pos:])
}

// TestLadybugSeparateHandleDuringWrite probes the OTHER topology: a reader that
// opens its own Database handle on the same path (what the stdio MCP server does
// per request, in a different process) while a writer has the file open and is
// mutating it. The same-process MVCC guarantee does NOT extend here, so this
// determines whether in-place writes can be enabled globally or only where the
// reader shares the writer's process.
func TestLadybugSeparateHandleDuringWrite(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sep")

	wdb, err := lbug.OpenDatabase(path, lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer wdb.Close()
	w, err := lbug.OpenConnection(wdb)
	if err != nil {
		t.Fatalf("writer conn: %v", err)
	}
	defer w.Close()

	run := func(c *lbug.Connection, q string) error {
		res, err := c.Query(q)
		if err != nil {
			return err
		}
		res.Close()
		return nil
	}
	if err := run(w, "CREATE NODE TABLE T(id INT64, PRIMARY KEY(id))"); err != nil {
		t.Fatalf("create: %v", err)
	}
	if err := run(w, "CREATE (:T {id: 1})"); err != nil {
		t.Fatalf("seed: %v", err)
	}

	roCfg := lbug.DefaultSystemConfig()
	roCfg.ReadOnly = true
	rdb, err := lbug.OpenDatabase(path, roCfg)
	if err != nil {
		t.Logf(">>> second read-only Database handle REFUSED while writer holds the file: %v", err)
		t.Logf(">>> in-place writes would break any reader that opens its own handle (e.g. the stdio MCP process)")
		return
	}
	defer rdb.Close()
	r, err := lbug.OpenConnection(rdb)
	if err != nil {
		t.Logf(">>> second handle opened but connection refused: %v", err)
		return
	}
	defer r.Close()
	t.Logf(">>> second read-only Database handle opened while writer holds the file")

	if err := run(w, "CREATE (:T {id: 2})"); err != nil {
		t.Fatalf("write after reader attached: %v", err)
	}
	res, qErr := r.Query("MATCH (n:T) RETURN count(n) AS c")
	if qErr != nil {
		t.Logf(">>> independent reader ERRORED after an in-place write: %v", qErr)
		return
	}
	var n int64
	if res.HasNext() {
		if tup, e := res.Next(); e == nil {
			if v, e2 := tup.GetValue(0); e2 == nil {
				switch x := v.(type) {
				case int64:
					n = x
				case int:
					n = int64(x)
				}
			}
		}
	}
	res.Close()
	t.Logf(">>> independent reader saw %d rows after an in-place write (writer wrote 2)", n)
}

// TestLadybugTransactionWithPreparedStatement checks whether a parameterized
// (prepared-statement) execution participates in an explicit transaction. The
// in-place incremental path issues BEGIN, then runs its delete/insert through
// prepared statements, then COMMIT — and COMMIT fails with "No active
// transaction", while the same sequence using plain Query works.
func TestLadybugTransactionWithPreparedStatement(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "tx"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()
	c, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("conn: %v", err)
	}
	defer c.Close()

	run := func(q string) error {
		res, err := c.Query(q)
		if err != nil {
			return err
		}
		res.Close()
		return nil
	}
	if err := run("CREATE NODE TABLE T(id INT64, PRIMARY KEY(id))"); err != nil {
		t.Fatalf("create: %v", err)
	}

	if err := run("BEGIN TRANSACTION"); err != nil {
		t.Fatalf("begin: %v", err)
	}

	stmt, err := c.Prepare("CREATE (:T {id: $id})")
	if err != nil {
		t.Fatalf("prepare: %v", err)
	}
	defer stmt.Close()
	res, err := c.Execute(stmt, map[string]any{"id": int64(1)})
	if err != nil {
		t.Fatalf("execute prepared inside tx: %v", err)
	}
	res.Close()

	if err := run("COMMIT"); err != nil {
		t.Errorf(">>> COMMIT after a prepared-statement write FAILED: %v", err)
		t.Logf(">>> prepared/parameterized execution does not join the explicit transaction — the in-place path must avoid it or drive the transaction differently")
	} else {
		t.Logf(">>> COMMIT succeeded after a prepared-statement write — prepared statements DO join the transaction")
	}
}
