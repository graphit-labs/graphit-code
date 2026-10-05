package ast

import (
	"context"
	"fmt"
	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

type resourceTestDB struct {
	emptyGraphDB
	closed atomic.Int32
}

func (db *resourceTestDB) Close() error { db.closed.Add(1); return nil }

func TestUIDatabasesWaitForHandlersBeforeEviction(t *testing.T) {
	db := &resourceTestDB{}
	s := &Server{db: &emptyGraphDB{}, dbCache: map[string]*cachedDB{"context": {db: db, lastUsed: time.Now().Add(-time.Hour)}}}
	mux := http.NewServeMux()
	borrowed, release := make(chan struct{}), make(chan struct{})
	s.registerDBRoute(mux, "GET /query", func(w http.ResponseWriter, r *http.Request) {
		// A reference can be borrowed before an operation starts. Close must wait
		// for the whole handler, not just the backend's individual Query mutex.
		s.dbCacheMu.Lock()
		actual := s.dbCache["context"].db
		s.dbCacheMu.Unlock()
		close(borrowed)
		<-release
		_, _ = actual.Query(r.Context(), "RETURN 1", nil)
	})
	handlerDone := make(chan struct{})
	go func() {
		mux.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest("GET", "/query", nil))
		close(handlerDone)
	}()
	<-borrowed
	sweepDone := make(chan struct{})
	go func() { s.releaseDatabasesBefore(time.Now()); close(sweepDone) }()
	select {
	case <-sweepDone:
		t.Fatal("evicted a borrowed backend while handler still active")
	case <-time.After(20 * time.Millisecond):
	}
	if db.closed.Load() != 0 {
		t.Fatal("closed a borrowed backend")
	}
	close(release)
	<-handlerDone
	<-sweepDone
	if db.closed.Load() != 1 || len(s.dbCache) != 0 {
		t.Fatal("idle backend was not closed and removed")
	}
	// A later acquisition creates a fresh lazy backend, rather than returning the removed one.
	next := s.getOrCreateCachedDB("project", t.TempDir(), true)
	if next == db {
		t.Fatal("returned an evicted backend")
	}
	s.CloseDatabases()
}

func TestUIDatabasesRetainRecentEntriesAndParkBase(t *testing.T) {
	base := &resourceTestDB{}
	s := &Server{db: base, dbCache: make(map[string]*cachedDB), baseLastUsed: time.Now().Add(-time.Hour)}
	now := time.Now()
	dbs := make([]*resourceTestDB, 20)
	for i := range dbs {
		dbs[i] = &resourceTestDB{}
		s.dbCache[fmt.Sprint(i)] = &cachedDB{db: dbs[i], lastUsed: now.Add(time.Duration(i) * time.Millisecond)}
	}
	s.releaseDatabasesBefore(now.Add(-time.Minute))
	if len(s.dbCache) != uiDBCacheLimit || base.closed.Load() != 1 {
		t.Fatalf("cache=%d base closes=%d", len(s.dbCache), base.closed.Load())
	}
	for i, db := range dbs {
		want := int32(0)
		if i < 4 {
			want = 1
		}
		if db.closed.Load() != want {
			t.Fatalf("entry %d closed=%d want=%d", i, db.closed.Load(), want)
		}
	}
	if s.baseDB() != base {
		t.Fatal("base backend identity changed")
	}
	s.releaseDatabasesBefore(now.Add(-time.Minute))
	if base.closed.Load() != 1 {
		t.Fatal("recent base use was closed")
	}
	s.CloseDatabases()
	mux := http.NewServeMux()
	s.registerDBRoute(mux, "GET /query", func(http.ResponseWriter, *http.Request) { t.Error("query started after shutdown") })
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest("GET", "/query", nil))
	if rec.Code != http.StatusServiceUnavailable {
		t.Fatalf("after shutdown status=%d", rec.Code)
	}
	if len(s.dbCache) != 0 {
		t.Fatal("shutdown retained contexts")
	}
}

func TestUIDatabasesMaintenanceStopsWithContext(t *testing.T) {
	s := &Server{}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan struct{})
	go func() { s.MaintainDatabases(ctx); close(done) }()
	cancel()
	select {
	case <-done:
	case <-time.After(time.Second):
		t.Fatal("maintenance did not stop")
	}
}

func TestLadybugCloseReleasesCanonicalManifest(t *testing.T) {
	db := NewLadybugDBReadOnly(LadybugConfig{})
	db.canonical = &ladybug.CanonicalManifest{}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}
	if db.canonical != nil {
		t.Fatal("closed backend retained canonical manifest")
	}
}

func TestUIDatabasesSlowHandlerRefreshesActualAcquisition(t *testing.T) {
	base := &resourceTestDB{}
	s := &Server{db: base, dbCache: make(map[string]*cachedDB)}
	mux := http.NewServeMux()
	acquired, release := make(chan struct{}), make(chan struct{})
	var contextDB GraphDB
	s.registerDBRoute(mux, "GET /query", func(http.ResponseWriter, *http.Request) {
		_ = s.baseDB()
		contextDB = s.getOrCreateCachedDB("project", "context-store", true)
		close(acquired)
		<-release
	})
	done := make(chan struct{})
	go func() { mux.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest("GET", "/query", nil)); close(done) }()
	<-acquired
	// The handler completes later than both acquisitions.
	finishedAfter := time.Now()
	close(release)
	<-done
	if s.baseLastUsed.Before(finishedAfter) || s.dbCache["project\x00context-store"].lastUsed.Before(finishedAfter) {
		t.Fatal("idle interval started before handler finished")
	}
	s.releaseDatabasesBefore(finishedAfter)
	if base.closed.Load() != 0 || len(s.dbCache) != 1 {
		t.Fatal("just-used backend released")
	}
	s.releaseDatabasesBefore(time.Now().Add(time.Second))
	if base.closed.Load() != 1 || len(s.dbCache) != 0 {
		t.Fatal("later idle cleanup retained backend")
	}
	if s.getOrCreateCachedDB("project", "context-store", true) == contextDB {
		t.Fatal("later request reused removed instance")
	}
	s.CloseDatabases()
}

func TestUIDatabasesShutdownWaitsForActiveHandler(t *testing.T) {
	base := &resourceTestDB{}
	s := &Server{db: base, dbCache: make(map[string]*cachedDB)}
	mux := http.NewServeMux()
	borrowed, release := make(chan struct{}), make(chan struct{})
	s.registerDBRoute(mux, "GET /query", func(http.ResponseWriter, *http.Request) { _ = s.baseDB(); close(borrowed); <-release })
	done := make(chan struct{})
	go func() { mux.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest("GET", "/query", nil)); close(done) }()
	<-borrowed
	closed := make(chan struct{})
	go func() { s.CloseDatabases(); close(closed) }()
	select {
	case <-closed:
		t.Fatal("shutdown closed active backend")
	case <-time.After(20 * time.Millisecond):
	}
	if base.closed.Load() != 0 {
		t.Fatal("active backend closed")
	}
	close(release)
	<-done
	<-closed
	if base.closed.Load() != 1 {
		t.Fatal("shutdown did not close drained backend")
	}
}
