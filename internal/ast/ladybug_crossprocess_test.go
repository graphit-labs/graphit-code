package ast

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	lbug "github.com/LadybugDB/go-ladybug"
)

const xprocReaderEnv = "GRAPHIT_XPROC_READER"

// TestCrossProcessReaderHelper is not a test. It is the reader subprocess: it
// opens the database at $GRAPHIT_XPROC_READER read-only and queries in a loop
// until told to stop, printing one line per anomaly it sees.
func TestCrossProcessReaderHelper(t *testing.T) {
	path := os.Getenv(xprocReaderEnv)
	if path == "" {
		t.Skip("not the reader subprocess")
	}

	cfg := lbug.DefaultSystemConfig()
	cfg.ReadOnly = os.Getenv("GRAPHIT_XPROC_RW") == ""
	db, err := lbug.OpenDatabase(path, cfg)
	if err != nil {
		fmt.Printf("OPEN_FAILED %v\n", err)
		os.Exit(3)
	}
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		fmt.Printf("CONN_FAILED %v\n", err)
		db.Close()
		os.Exit(4)
	}

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
	conn.Close()
	db.Close()
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
