package ast

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/lockfile"
)

const stagedSearchQueueSize = 16

type stagedSearchEntry struct {
	relPath string
	entry   *parseCacheEntry
}

type stagedSearchRebuild struct {
	ctx         context.Context
	cancel      context.CancelFunc
	cache       *ShardCache
	embCache    *ShardEmbCache
	index       *SearchIndex
	writer      *searchRebuildWriter
	storeDir    string
	stagingPath string
	queue       chan stagedSearchEntry
	done        chan struct{}

	mu          sync.Mutex
	err         error
	closed      bool
	adopted     map[string]bool
	timing      SearchRebuildTiming
	fileCount   int
	entityCount int
	vectorCount int
}

func startStagedSearchRebuild(ctx context.Context, storeDir string, cache *ShardCache,
	embCache *ShardEmbCache) (*stagedSearchRebuild, error) {
	if storeDir == "" || cache == nil {
		return nil, fmt.Errorf("start staged search rebuild: missing store or parse cache")
	}
	stageCtx, cancel := context.WithCancel(ctx)
	stagingPath := LanceIndexPath(storeDir) + ".staging." + shortHex()
	if err := os.MkdirAll(filepath.Dir(stagingPath), 0o755); err != nil {
		cancel()
		return nil, fmt.Errorf("create search staging parent: %w", err)
	}
	idx, err := OpenSearchIndexAt(stageCtx, lancestore.Config{URI: stagingPath})
	if err != nil {
		cancel()
		return nil, fmt.Errorf("open staged search index: %w", err)
	}
	writer, err := idx.beginSearchRebuild(stageCtx, BuildEmbLookup(cache, embCache))
	if err != nil {
		_ = idx.Close()
		cancel()
		_ = os.RemoveAll(stagingPath)
		return nil, fmt.Errorf("initialize staged search index: %w", err)
	}

	stage := &stagedSearchRebuild{
		ctx: stageCtx, cancel: cancel, cache: cache, embCache: embCache,
		index: idx, writer: writer, storeDir: storeDir, stagingPath: stagingPath,
		queue: make(chan stagedSearchEntry, stagedSearchQueueSize), done: make(chan struct{}),
		adopted: make(map[string]bool, cache.Count()),
	}
	go stage.run()
	return stage, nil
}

func (s *stagedSearchRebuild) run() {
	defer close(s.done)
	for item := range s.queue {
		if s.ctx.Err() != nil {
			s.setError(s.ctx.Err())
			break
		}
		tSource := time.Now()
		source := s.cache.SourceOf(item.relPath)
		s.writer.timing.Prepare += time.Since(tSource)
		if err := s.writer.Add(item.relPath, source, item.entry); err != nil {
			s.setError(err)
			break
		}
	}
	if s.getError() == nil {
		if err := s.writer.Finish(); err != nil {
			s.setError(err)
		}
	}
	s.timing = s.writer.timing
	s.fileCount = s.writer.fileCount
	s.entityCount = s.writer.entCount
	s.vectorCount = s.writer.vecCount
	if err := s.index.Close(); err != nil && s.getError() == nil {
		s.setError(fmt.Errorf("close staged search index: %w", err))
	}
	if s.embCache != nil {
		if err := s.embCache.Close(); err != nil && s.getError() == nil {
			s.setError(fmt.Errorf("close embedding cache: %w", err))
		}
	}
}

func (s *stagedSearchRebuild) Adopt(relPath string, entry *parseCacheEntry) error {
	if s == nil || relPath == "" || entry == nil {
		return nil
	}
	s.mu.Lock()
	if s.closed || s.adopted[relPath] {
		s.mu.Unlock()
		return s.getError()
	}
	s.mu.Unlock()

	select {
	case s.queue <- stagedSearchEntry{relPath: relPath, entry: entry}:
		s.mu.Lock()
		s.adopted[relPath] = true
		s.mu.Unlock()
		return nil
	case <-s.done:
		return s.getError()
	case <-s.ctx.Done():
		return s.ctx.Err()
	}
}

func (s *stagedSearchRebuild) Complete(entries map[string]*parseCacheEntry) error {
	if s == nil {
		return nil
	}
	for relPath, entry := range entries {
		if err := s.Adopt(relPath, entry); err != nil {
			s.Abort()
			return err
		}
	}
	s.mu.Lock()
	if !s.closed {
		s.closed = true
		close(s.queue)
	}
	s.mu.Unlock()
	return nil
}

func (s *stagedSearchRebuild) Publish(ctx context.Context) (SearchRebuildTiming, error) {
	if s == nil {
		return SearchRebuildTiming{}, fmt.Errorf("publish staged search rebuild: no staging build")
	}
	select {
	case <-s.done:
	case <-ctx.Done():
		s.Abort()
		return SearchRebuildTiming{}, ctx.Err()
	}
	if err := s.getError(); err != nil {
		_ = os.RemoveAll(s.stagingPath)
		return s.timing, err
	}

	tPublish := time.Now()
	if err := publishStagedSearch(ctx, s.storeDir, s.stagingPath,
		int64(s.vectorCount), int64(s.entityCount)); err != nil {
		_ = os.RemoveAll(s.stagingPath)
		return s.timing, err
	}
	s.timing.Publish = time.Since(tPublish)
	s.stagingPath = ""
	return s.timing, nil
}

func (s *stagedSearchRebuild) Abort() {
	if s == nil {
		return
	}
	s.cancel()
	s.mu.Lock()
	if !s.closed {
		s.closed = true
		close(s.queue)
	}
	s.mu.Unlock()
	<-s.done
	if s.stagingPath != "" {
		_ = os.RemoveAll(s.stagingPath)
	}
}

func (s *stagedSearchRebuild) setError(err error) {
	if err == nil {
		return
	}
	s.mu.Lock()
	if s.err == nil {
		s.err = err
	}
	s.mu.Unlock()
}

func (s *stagedSearchRebuild) getError() error {
	if s == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.err
}

func publishStagedSearch(ctx context.Context, storeDir, stagingPath string, vectors, entities int64) error {
	publicationLock, err := acquireVectorPublicationLock(ctx, storeDir)
	if err != nil {
		return err
	}
	defer publicationLock.Release()

	previousStatus := readEmbedsStatus(storeDir)
	generation, err := beginVectorGeneration(storeDir)
	if err != nil {
		return fmt.Errorf("publishing pending vector generation: %w", err)
	}

	activePath := LanceIndexPath(storeDir)
	backupPath := activePath + ".backup." + shortHex()
	hadActive := false
	if _, statErr := os.Stat(activePath); statErr == nil {
		hadActive = true
		if err := os.Rename(activePath, backupPath); err != nil {
			_ = restoreEmbedsStatus(storeDir, generation, previousStatus)
			return fmt.Errorf("back up active search index: %w", err)
		}
	} else if !os.IsNotExist(statErr) {
		_ = restoreEmbedsStatus(storeDir, generation, previousStatus)
		return fmt.Errorf("inspect active search index: %w", statErr)
	}

	if err := os.Rename(stagingPath, activePath); err != nil {
		if hadActive {
			_ = os.Rename(backupPath, activePath)
		}
		_ = restoreEmbedsStatus(storeDir, generation, previousStatus)
		return fmt.Errorf("publish staged search index: %w", err)
	}
	if err := publishPendingVectorCounts(storeDir, generation, vectors, entities); err != nil {
		_ = os.RemoveAll(activePath)
		if hadActive {
			_ = os.Rename(backupPath, activePath)
		}
		_ = restoreEmbedsStatus(storeDir, generation, previousStatus)
		return fmt.Errorf("publish staged vector counts: %w", err)
	}
	if hadActive {
		_ = os.RemoveAll(backupPath)
	}
	return nil
}

func acquireVectorPublicationLock(ctx context.Context, storeDir string) (*lockfile.Lock, error) {
	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()
	path := filepath.Join(storeDir, vectorFinalizeLockFile)
	for {
		lock, err := lockfile.TryAcquire(path)
		if err == nil {
			return lock, nil
		}
		if !errors.Is(err, lockfile.ErrLocked) {
			return nil, fmt.Errorf("lock search publication: %w", err)
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-ticker.C:
		}
	}
}

func restoreEmbedsStatus(storeDir, generation string, previous embedsStatus) error {
	return withEmbedsStatusLock(storeDir, func(current embedsStatus) error {
		if current.Generation != generation {
			return nil
		}
		return writeEmbedsStatusFile(storeDir, previous)
	})
}
