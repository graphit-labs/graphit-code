package ast

import (
	"context"
	"net/http"
	"sort"
	"time"
)

const uiDBIdleTimeout = 2 * time.Minute
const uiDBSweepInterval = 30 * time.Second
const uiDBCacheLimit = 16

// A handler retains the actual backend under a shared lease. Maintenance takes
// an exclusive lease, so no pointer borrowed by a request can reopen an evicted DB.
func (s *Server) registerDBRoute(mux *http.ServeMux, pattern string, handler http.HandlerFunc) {
	mux.HandleFunc(pattern, func(w http.ResponseWriter, r *http.Request) {
		s.dbRequests.RLock()
		defer s.dbRequests.RUnlock()
		started := time.Now()
		defer s.finishDatabaseUse(started)
		if s.dbClosed {
			http.Error(w, "AST server is stopping", http.StatusServiceUnavailable)
			return
		}
		handler(w, r)
	})
}

// Last use ends with the handler, so a slow query gets a full idle interval.
func (s *Server) finishDatabaseUse(started time.Time) {
	s.dbCacheMu.Lock()
	defer s.dbCacheMu.Unlock()
	now := time.Now()
	if !s.baseLastUsed.Before(started) {
		s.baseLastUsed = now
	}
	for _, cached := range s.dbCache {
		if !cached.lastUsed.Before(started) {
			cached.lastUsed = now
		}
	}
}

func (s *Server) baseDB() GraphDB {
	s.dbCacheMu.Lock()
	s.baseLastUsed = time.Now()
	s.dbCacheMu.Unlock()
	return s.db
}

// MaintainDatabases parks the base backend and removes unused context backends.
// The base backend remains lazy and reopens on the next query.
func (s *Server) MaintainDatabases(ctx context.Context) {
	ticker := time.NewTicker(uiDBSweepInterval)
	defer ticker.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case now := <-ticker.C:
			s.releaseDatabasesBefore(now.Add(-uiDBIdleTimeout))
		}
	}
}

func (s *Server) releaseDatabasesBefore(before time.Time) {
	s.dbRequests.Lock()
	defer s.dbRequests.Unlock()
	s.dbCacheMu.Lock()
	defer s.dbCacheMu.Unlock()
	if s.dbClosed {
		return
	}
	if !s.baseLastUsed.IsZero() && s.baseLastUsed.Before(before) {
		_ = s.db.Close()
		s.baseLastUsed = time.Time{}
	}
	type entry struct {
		key  string
		used time.Time
	}
	entries := make([]entry, 0, len(s.dbCache))
	for key, cached := range s.dbCache {
		if cached.lastUsed.Before(before) {
			_ = cached.db.Close()
			delete(s.dbCache, key)
		} else {
			entries = append(entries, entry{key, cached.lastUsed})
		}
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].used.After(entries[j].used) })
	for _, e := range entries[min(len(entries), uiDBCacheLimit):] {
		_ = s.dbCache[e.key].db.Close()
		delete(s.dbCache, e.key)
	}
}

// CloseDatabases waits for AST handlers before closing, then rejects new ones.
func (s *Server) CloseDatabases() {
	s.dbRequests.Lock()
	defer s.dbRequests.Unlock()
	s.dbCacheMu.Lock()
	defer s.dbCacheMu.Unlock()
	if s.dbClosed {
		return
	}
	s.dbClosed = true
	for key, cached := range s.dbCache {
		_ = cached.db.Close()
		delete(s.dbCache, key)
	}
	if s.db != nil {
		_ = s.db.Close()
	}
	s.baseLastUsed = time.Time{}
}
