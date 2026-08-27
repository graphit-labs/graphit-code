package ast

import (
	"os"
	"path/filepath"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// The engine's own sidecar names, from liblbug storage_utils.h. All of them are
// "<dbPath>.<suffix>", which is why an allowlist over "<dbPath>.*" cannot work:
// every one of these was deleted by the old CleanupInterruptedSwap except the
// first, and ".wal.checkpoint" slipped through because the exemption compared
// for equality with ".wal".
var engineSidecars = []string{
	".wal",
	".wal.checkpoint",
	".shadow",
	".tmp",
	".checkpoint.intent.lock",
	".checkpoint.apply.lock",
}

// ourSidecars are files this project owns beside the store and that cleanup must
// spare. There are none: the search index used to be one — "<dbPath>.search.sqlite"
// plus the -wal and -shm SQLite kept next to it — and it now lives in tables inside
// the store itself. The variable is kept, empty, because the property it guards is
// still the one that matters. Anything this project starts writing beside the store
// belongs here on the day it is written, not after cleanup has eaten it once.
var ourSidecars []string

func TestCleanupInterruptedSwapSparesEngineSidecars(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "ladybugdb")

	write := func(p string) {
		if err := os.WriteFile(p, []byte("x"), 0o644); err != nil {
			t.Fatalf("seed %s: %v", p, err)
		}
	}

	write(dbPath)
	for _, s := range engineSidecars {
		write(dbPath + s)
	}
	for _, s := range ourSidecars {
		write(dbPath + s)
	}

	// What cleanup exists for: a swap that died halfway. ".a3f91c2" is a working
	// copy (shortHex is 7 hex characters) and may have brought its own sidecars.
	leftovers := []string{".old", ".a3f91c2", ".a3f91c2.wal", ".a3f91c2.shadow", ".staging"}
	for _, s := range leftovers {
		write(dbPath + s)
	}

	CleanupInterruptedSwap(dbPath)

	if _, err := os.Stat(dbPath); err != nil {
		t.Errorf("the database itself was removed: %v", err)
	}
	for _, s := range engineSidecars {
		if _, err := os.Stat(dbPath + s); err != nil {
			t.Errorf("engine sidecar %q was deleted — this is what tore the checkpoint "+
				"state out from under the writer", s)
		}
	}
	for _, s := range ourSidecars {
		if _, err := os.Stat(dbPath + s); err != nil {
			t.Errorf("search index file %q was deleted", s)
		}
	}
	for _, s := range leftovers {
		if _, err := os.Stat(dbPath + s); err == nil {
			t.Errorf("swap leftover %q survived — cleanup did not do its job", s)
		}
	}
}

// A reader must leave the filesystem exactly as it found it. Before the fix,
// connect() ran CleanupInterruptedSwap on EVERY open, read-only included, so a
// read-only MCP call deleted the writer's live sidecars and its working copy.
//
// The writer here stays attached and idle, which is the arrangement the engine
// tolerates (an idle read-write holder does not disturb read-only openers) and
// which leaves a genuine ".wal" on disk — planting a fake one would just make
// the open fail, since the engine tries to recover from it.
func TestReadOnlyConnectDeletesNothing(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "ladybugdb")

	db, err := lbug.OpenDatabase(dbPath, lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("seed conn: %v", err)
	}
	defer conn.Close()
	run := func(q string) {
		res, qErr := conn.Query(q)
		if qErr != nil {
			t.Fatalf("seed %q: %v", q, qErr)
		}
		res.Close()
	}
	run("CREATE NODE TABLE T(id INT64, PRIMARY KEY(id))")
	for i := 0; i < 50; i++ {
		run("CREATE (:T {id: " + itoa(i) + "})")
	}

	// Leftovers a writer legitimately owns while mid-swap. These are inert files
	// the engine never opens, so their content does not matter — only whether a
	// reader dares to remove them.
	for _, s := range []string{".old", ".b7c4e01", ".b7c4e01.wal"} {
		if err := os.WriteFile(dbPath+s, []byte("x"), 0o644); err != nil {
			t.Fatalf("plant %s: %v", s, err)
		}
	}

	before, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("list before: %v", err)
	}

	ro := NewLadybugDBReadOnly(LadybugConfig{DBPath: dbPath})
	if err := ro.connect(); err != nil {
		t.Fatalf("read-only connect failed while an idle writer held the database: %v", err)
	}
	_ = ro.Close()

	for _, e := range before {
		if _, err := os.Stat(filepath.Join(dir, e.Name())); err != nil {
			t.Errorf("read-only connect deleted %q", e.Name())
		}
	}
}

// A read-only open of a path that is not there must fail fast, with a message
// that names the problem, and must not create anything.
func TestReadOnlyConnectOnMissingDatabaseFailsWithoutCreating(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "nested", "ladybugdb")

	ro := NewLadybugDBReadOnly(LadybugConfig{DBPath: dbPath})
	err := ro.connect()
	if err == nil {
		_ = ro.Close()
		t.Fatal("expected a read-only open of a missing database to fail")
	}
	if _, statErr := os.Stat(filepath.Dir(dbPath)); statErr == nil {
		t.Error("a read-only open created the parent directory")
	}
}

// connect() must not remember a failure. It used to run inside a sync.Once that
// stored the error in a field, so one open landing in a writer's checkpoint
// window poisoned the backend permanently: every later call returned the stale
// error without retrying.
func TestConnectRetriesAfterAFailedAttempt(t *testing.T) {
	dir := t.TempDir()
	dbPath := filepath.Join(dir, "ladybugdb")

	ro := NewLadybugDBReadOnly(LadybugConfig{DBPath: dbPath})
	if err := ro.connect(); err == nil {
		_ = ro.Close()
		t.Fatal("expected the first connect to fail: no database exists yet")
	}

	// The database appears — as it does when the daemon finishes its first index.
	db, err := lbug.OpenDatabase(dbPath, lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		db.Close()
		t.Fatalf("seed conn: %v", err)
	}
	res, err := conn.Query("CREATE NODE TABLE T(id INT64, PRIMARY KEY(id))")
	if err != nil {
		conn.Close()
		db.Close()
		t.Fatalf("seed schema: %v", err)
	}
	res.Close()
	conn.Close()
	db.Close()

	if err := ro.connect(); err != nil {
		t.Fatalf("the same backend refused to retry after its first failure: %v", err)
	}
	_ = ro.Close()
}
