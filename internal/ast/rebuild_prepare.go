package ast

import (
	"sync"
	"time"
)

type rebuildEntryPreparation struct {
	mu       sync.Mutex
	entries  map[string]*parseCacheEntry
	done     chan struct{}
	duration time.Duration
}

func startRebuildEntryPreparation(cache *ShardCache, reparsed map[string]bool) *rebuildEntryPreparation {
	p := &rebuildEntryPreparation{
		entries: make(map[string]*parseCacheEntry, cache.Count()),
		done:    make(chan struct{}),
	}
	go func() {
		started := time.Now()
		cache.streamEntriesExcept(reparsed, func(relPath string, entry *parseCacheEntry) bool {
			p.adopt(relPath, entry)
			return true
		})
		p.mu.Lock()
		p.duration = time.Since(started)
		p.mu.Unlock()
		close(p.done)
	}()
	return p
}

func (p *rebuildEntryPreparation) adopt(relPath string, entry *parseCacheEntry) {
	if p == nil || relPath == "" || entry == nil {
		return
	}
	p.mu.Lock()
	p.entries[relPath] = entry
	p.mu.Unlock()
}

func (p *rebuildEntryPreparation) finish(cache *ShardCache) (map[string]*parseCacheEntry, time.Duration) {
	if p == nil {
		return nil, 0
	}
	<-p.done

	// A reparsed file can fail before Store. Preserve the old cache-backed entry
	// in that case, matching the rebuild's previous source-of-truth behavior.
	for _, relPath := range cache.AllPaths() {
		p.mu.Lock()
		_, exists := p.entries[relPath]
		p.mu.Unlock()
		if exists {
			continue
		}
		if entry := cache.GetEntry(relPath); entry != nil {
			p.adopt(relPath, entry)
		}
	}

	p.mu.Lock()
	entries, duration := p.entries, p.duration
	p.entries = nil
	p.mu.Unlock()
	return entries, duration
}
