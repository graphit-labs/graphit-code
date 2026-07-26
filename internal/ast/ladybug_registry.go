package ast

import (
	"sync"

	lbug "github.com/LadybugDB/go-ladybug"
)

// Shared database handles.
//
// LadybugDB's concurrency guarantee is scoped to connections opened on the SAME
// *lbug.Database object: a reader on a sibling connection keeps serving the
// pre-commit snapshot while a writer mutates inside a transaction, and flips to
// the new data at COMMIT (verified by TestLadybugSharedDatabaseMVCC — the reader
// answered in 859µs mid-transaction with no block and no torn state).
//
// That property is what makes in-place incremental writes possible instead of
// copying the whole database, mutating the copy and swapping it in — a model
// whose close-the-copy step alone costs 0.2–5.0 s. To benefit, every backend in
// the process that talks to a given path must share one handle, which is what
// this registry provides.
//
// Policy: a read-only request reuses an existing read-write handle when one is
// already open for that path (the daemon case: writer plus in-process readers),
// because sharing is exactly what buys snapshot isolation. Otherwise it opens
// its own read-only handle.
var dbRegistry = struct {
	mu sync.Mutex
	m  map[string]*sharedDatabase
}{m: make(map[string]*sharedDatabase)}

type sharedDatabase struct {
	db       *lbug.Database
	readOnly bool
	refs     int
}

// acquireDatabase returns a shared *lbug.Database for path, opening one if
// needed. Every successful call must be paired with releaseDatabase.
func acquireDatabase(path string, cfg lbug.SystemConfig) (*lbug.Database, error) {
	dbRegistry.mu.Lock()
	defer dbRegistry.mu.Unlock()

	if sh, ok := dbRegistry.m[path]; ok {
		// Share when the existing handle is at least as capable as requested: a
		// read-only caller is happy with a read-write handle (and gains MVCC with
		// the writer), but a writer cannot use a read-only handle.
		if !sh.readOnly || cfg.ReadOnly {
			sh.refs++
			return sh.db, nil
		}
		// Writer needs read-write while only a read-only handle exists: fall
		// through and open a private handle rather than disturbing readers.
		db, err := lbug.OpenDatabase(path, cfg)
		if err != nil {
			return nil, err
		}
		return db, nil
	}

	db, err := lbug.OpenDatabase(path, cfg)
	if err != nil {
		return nil, err
	}
	dbRegistry.m[path] = &sharedDatabase{db: db, readOnly: cfg.ReadOnly, refs: 1}
	return db, nil
}

// releaseDatabase drops one reference to path's shared handle, closing it when
// the last user goes away. Handles opened privately (not in the registry, or a
// different object than the registered one) are closed by the caller instead.
func releaseDatabase(path string, db *lbug.Database) {
	dbRegistry.mu.Lock()
	sh, ok := dbRegistry.m[path]
	if !ok || sh.db != db {
		dbRegistry.mu.Unlock()
		if db != nil {
			db.Close()
		}
		return
	}
	sh.refs--
	if sh.refs > 0 {
		dbRegistry.mu.Unlock()
		return
	}
	delete(dbRegistry.m, path)
	dbRegistry.mu.Unlock()
	sh.db.Close()
}

// sharedDatabaseOpen reports whether a read-write handle for path is already
// open in this process. In-place writes are only safe to prefer when the
// readers that matter share that handle.
func sharedDatabaseOpen(path string) bool {
	dbRegistry.mu.Lock()
	defer dbRegistry.mu.Unlock()
	sh, ok := dbRegistry.m[path]
	return ok && !sh.readOnly
}
