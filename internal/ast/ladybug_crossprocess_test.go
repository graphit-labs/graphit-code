package ast

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugSeparateHandleDuringWrite already covers a second handle inside one
// process. Production does not look like that: the MCP server is its own process,
// started separately and outliving individual indexing runs, and it reads the
// database while the daemon writes to it in place. Nothing exercised that, so
// "in-place writes do not break lock-free reads" was an in-process claim being
// applied to a cross-process arrangement.
//
// The readers here are real subprocesses. The test binary re-executes itself
// with GRAPHIT_XPROC_READER set, which is the cheapest way to get a separate
// process that already links the database driver.

const xprocReaderEnv = "GRAPHIT_XPROC_READER"

// TestCrossProcessReaderHelper is not a test. It is the reader subprocess: it
// opens the database at $GRAPHIT_XPROC_READER read-only and queries in a loop
// until told to stop, printing one line per anomaly it sees.
func TestCrossProcessReaderHelper(t *testing.T) {
	path := os.Getenv(xprocReaderEnv)
	if path == "" {
		t.Skip("not the reader subprocess")
	}

	// Read-only, which is what NewLadybugDBReadOnly gives the MCP server. A
	// reader that opens read-write takes the single writer slot and locks the
	// indexer out — that is a property of the engine, not of this test, and the
	// separate test below pins it.
	cfg := lbug.DefaultSystemConfig()
	cfg.ReadOnly = os.Getenv("GRAPHIT_XPROC_RW") == ""
	db, err := lbug.OpenDatabase(path, cfg)
	if err != nil {
		fmt.Printf("OPEN_FAILED %v\n", err)
		os.Exit(3)
	}
	defer db.Close()
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		fmt.Printf("CONN_FAILED %v\n", err)
		os.Exit(4)
	}
	defer conn.Close()

	deadline := time.Now().Add(20 * time.Second)
	reads, anomalies := 0, 0
	for time.Now().Before(deadline) {
		res, err := conn.Query("MATCH (t:T) RETURN t.id, t.body")
		if err != nil {
			fmt.Printf("QUERY_ERROR %v\n", err)
			anomalies++
			continue
		}
		for res.HasNext() {
			tuple, err := res.Next()
			if err != nil {
				fmt.Printf("NEXT_ERROR %v\n", err)
				anomalies++
				break
			}
			vals, err := tuple.GetAsSlice()
			if err != nil || len(vals) < 2 {
				fmt.Printf("SHAPE_ERROR %v\n", err)
				anomalies++
				break
			}
			// Every row is written as body = strings.Repeat(marker, n), so a
			// torn read shows up as a body that is not a clean repetition.
			body, _ := vals[1].(string)
			if body != "" && !isCleanRepetition(body, "abcdefgh") {
				fmt.Printf("TORN_ROW len=%d prefix=%q\n", len(body), safePrefix(body))
				anomalies++
			}
			reads++
		}
		res.Close()
		if os.Getenv("GRAPHIT_XPROC_STOP") != "" {
			break
		}
	}
	fmt.Printf("READS %d ANOMALIES %d\n", reads, anomalies)
	if anomalies > 0 {
		os.Exit(5)
	}
}

func isCleanRepetition(s, unit string) bool {
	if len(s)%len(unit) != 0 {
		return false
	}
	for i := 0; i < len(s); i += len(unit) {
		if s[i:i+len(unit)] != unit {
			return false
		}
	}
	return true
}

func safePrefix(s string) string {
	if len(s) > 24 {
		return s[:24]
	}
	return s
}

func TestLadybugInPlaceWritesUnderCrossProcessReaders(t *testing.T) {
	if testing.Short() {
		t.Skip("spawns reader subprocesses and writes continuously")
	}

	dir := t.TempDir()
	path := filepath.Join(dir, "xproc")

	wdb, err := lbug.OpenDatabase(path, lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	w, err := lbug.OpenConnection(wdb)
	if err != nil {
		wdb.Close()
		t.Fatalf("writer conn: %v", err)
	}

	run := func(q string) error {
		res, err := w.Query(q)
		if err != nil {
			return err
		}
		res.Close()
		return nil
	}
	if err := run("CREATE NODE TABLE T(id INT64, body STRING, PRIMARY KEY(id))"); err != nil {
		w.Close()
		wdb.Close()
		t.Fatalf("schema: %v", err)
	}
	unit := "abcdefgh"
	for i := 0; i < 200; i++ {
		if err := run(fmt.Sprintf("CREATE (:T {id: %d, body: '%s'})",
			i, strings.Repeat(unit, 4+i%12))); err != nil {
			w.Close()
			wdb.Close()
			t.Fatalf("seed %d: %v", i, err)
		}
	}

	// The writer's handle is released before the readers start, then reacquired
	// once they are attached, so the write happens with readers already inside.
	w.Close()
	wdb.Close()

	const readers = 3
	procs := make([]*exec.Cmd, 0, readers)
	outs := make([]*strings.Builder, 0, readers)
	for i := 0; i < readers; i++ {
		cmd := exec.Command(os.Args[0], "-test.run", "TestCrossProcessReaderHelper", "-test.v")
		cmd.Env = append(os.Environ(), xprocReaderEnv+"="+path)
		var sb strings.Builder
		cmd.Stdout = &sb
		cmd.Stderr = &sb
		if err := cmd.Start(); err != nil {
			t.Fatalf("start reader %d: %v", i, err)
		}
		procs = append(procs, cmd)
		outs = append(outs, &sb)
	}
	t.Cleanup(func() {
		for _, c := range procs {
			if c.Process != nil {
				_ = c.Process.Kill()
			}
		}
	})

	// Give the readers time to open the database before writing starts.
	time.Sleep(2 * time.Second)

	wdb2, err := lbug.OpenDatabase(path, lbug.DefaultSystemConfig())
	if err != nil {
		t.Fatalf("a writer cannot attach while read-only readers hold the database: %v", err)
	}
	defer wdb2.Close()
	w2, err := lbug.OpenConnection(wdb2)
	if err != nil {
		t.Fatalf("writer reconnect: %v", err)
	}
	defer w2.Close()

	// In-place updates while the readers are mid-scan.
	writes, writeErrs := 0, 0
	deadline := time.Now().Add(10 * time.Second)
	for i := 200; time.Now().Before(deadline); i++ {
		q := fmt.Sprintf("CREATE (:T {id: %d, body: '%s'})", i, strings.Repeat(unit, 4+i%12))
		res, err := w2.Query(q)
		if err != nil {
			writeErrs++
			if writeErrs <= 3 {
				t.Logf("write %d: %v", i, err)
			}
			continue
		}
		res.Close()
		writes++
	}

	var wg sync.WaitGroup
	results := make([]string, readers)
	codes := make([]int, readers)
	for i, c := range procs {
		wg.Add(1)
		go func(i int, c *exec.Cmd) {
			defer wg.Done()
			err := c.Wait()
			results[i] = outs[i].String()
			if ee, ok := err.(*exec.ExitError); ok {
				codes[i] = ee.ExitCode()
			} else if err != nil {
				codes[i] = -1
			}
		}(i, c)
	}
	wg.Wait()

	t.Logf("writer: %d writes, %d errors", writes, writeErrs)
	if writes == 0 {
		t.Error("no write succeeded while readers were attached")
	}

	for i, out := range results {
		summary := "(no summary line)"
		for _, line := range strings.Split(out, "\n") {
			if strings.HasPrefix(line, "READS ") {
				summary = line
			}
			for _, bad := range []string{"TORN_ROW", "QUERY_ERROR", "NEXT_ERROR", "SHAPE_ERROR"} {
				if strings.HasPrefix(line, bad) {
					t.Errorf("reader %d: %s", i, line)
				}
			}
			if strings.HasPrefix(line, "OPEN_FAILED") || strings.HasPrefix(line, "CONN_FAILED") {
				t.Errorf("reader %d could not attach to a database being written: %s", i, line)
			}
		}
		t.Logf("reader %d: exit=%d %s", i, codes[i], summary)
		if codes[i] != 0 {
			t.Errorf("reader %d exited %d", i, codes[i])
		}
	}
}

// A reader that opens read-write does not merely share badly — it takes the
// single writer slot and the indexer cannot attach at all. This is why the MCP
// path must go through NewLadybugDBReadOnly, and it is worth an explicit test
// because the failure appears at the indexer, far from the process that caused
// it, as an opaque "failed to open database".
func TestLadybugReadWriteOpenerLocksOutTheIndexer(t *testing.T) {
	if testing.Short() {
		t.Skip("spawns a subprocess")
	}

	dir := t.TempDir()
	path := filepath.Join(dir, "excl")

	seed, err := lbug.OpenDatabase(path, lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	c, err := lbug.OpenConnection(seed)
	if err != nil {
		seed.Close()
		t.Fatalf("conn: %v", err)
	}
	res, err := c.Query("CREATE NODE TABLE T(id INT64, body STRING, PRIMARY KEY(id))")
	if err != nil {
		c.Close()
		seed.Close()
		t.Fatalf("schema: %v", err)
	}
	res.Close()
	c.Close()
	seed.Close()

	for _, tc := range []struct {
		name       string
		readOnly   bool
		wantAttach bool
	}{
		{"holder is read-only: the indexer attaches", true, true},
		{"holder is read-write: the indexer is locked out", false, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			cmd := exec.Command(os.Args[0], "-test.run", "TestCrossProcessReaderHelper")
			env := append(os.Environ(), xprocReaderEnv+"="+path)
			if !tc.readOnly {
				env = append(env, "GRAPHIT_XPROC_RW=1")
			}
			cmd.Env = env
			var sb strings.Builder
			cmd.Stdout, cmd.Stderr = &sb, &sb
			if err := cmd.Start(); err != nil {
				t.Fatalf("start holder: %v", err)
			}
			defer func() { _ = cmd.Process.Kill(); _ = cmd.Wait() }()
			time.Sleep(2 * time.Second)

			db, err := lbug.OpenDatabase(path, lbug.DefaultSystemConfig())
			attached := err == nil
			if attached {
				db.Close()
			}
			if attached != tc.wantAttach {
				t.Errorf("indexer attached = %v, want %v (err=%v)\nholder output: %s",
					attached, tc.wantAttach, err, sb.String())
			}
		})
	}
}
