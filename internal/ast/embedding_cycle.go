package ast

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"sort"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
)

// RunEmbeddingCycleOnce waits for an active AST publication, then holds the
// lifecycle lock only while it has store handles open. Model inference uses a
// snapshot of text and runs without the lock.
func RunEmbeddingCycleOnce(ctx context.Context, cacheDir, repoRoot string, newClient func() (ai.EmbeddingClient, error), logger *slog.Logger) (int, bool, error) {
	return runEmbeddingCycleOnce(ctx, cacheDir, repoRoot, newClient, logger, false)
}

func runEmbeddingCycleOnce(ctx context.Context, cacheDir, repoRoot string, newClient func() (ai.EmbeddingClient, error), logger *slog.Logger, deferIfLocked bool) (int, bool, error) {
	if cacheDir == "" {
		return 0, false, nil
	}
	var lockedCtx context.Context
	var guard *storelifecycle.Guard
	var err error
	if deferIfLocked {
		lockedCtx, guard, err = storelifecycle.TryAcquire(ctx, cacheDir)
		if errors.Is(err, storelifecycle.ErrLocked) {
			return 0, true, nil
		}
	} else {
		lockedCtx, guard, err = storelifecycle.Acquire(ctx, cacheDir)
	}
	if err != nil {
		return 0, false, fmt.Errorf("lock AST store lifecycle: %w", err)
	}
	buckets, generation, err := snapshotEmbeddingRows(lockedCtx, cacheDir, repoRoot, logger)
	guard.Release()
	if err != nil {
		return 0, false, err
	}
	rows := prepareEmbeddingRows(buckets)

	if len(rows) == 0 {
		return 0, false, finalizeEmbeddingGeneration(ctx, cacheDir, generation)
	}
	client, err := newClient()
	if err != nil {
		return 0, false, fmt.Errorf("embedding client: %w", err)
	}
	if client == nil {
		return 0, false, errors.New("embedding client is nil")
	}

	batchSize := DefaultEmbeddingConfig().BatchSize
	stored := 0
	for i := 0; i < len(rows); i += batchSize {
		if err := ctx.Err(); err != nil {
			return stored, false, err
		}
		end := i + batchSize
		if end > len(rows) {
			end = len(rows)
		}
		batch := rows[i:end]
		texts := make([]string, len(batch))
		for j := range batch {
			texts[j] = batch[j].text
		}
		// No cache or index handle, and no lifecycle lock, survives this call.
		vectors, err := client.EmbedBatch(ctx, texts)
		if err != nil {
			return stored, false, fmt.Errorf("embed batch: %w", err)
		}
		if len(vectors) != len(batch) {
			return stored, false, fmt.Errorf("expected %d vectors, got %d", len(batch), len(vectors))
		}
		n, current, err := publishEmbeddingBatch(ctx, cacheDir, repoRoot, generation, batch, vectors, client.Dimensions())
		stored += n
		if err != nil {
			return stored, false, err
		}
		if !current {
			// A rebuild won the lock during inference. Its next embedding cycle
			// will scan the new generation; no old vector may be upserted into it.
			return stored, false, nil
		}
	}
	return stored, false, finalizeEmbeddingGeneration(ctx, cacheDir, generation)
}

func snapshotEmbeddingRows(ctx context.Context, cacheDir, repoRoot string, logger *slog.Logger) (map[string][]entityRow, string, error) {
	store, err := openEmbeddingStore(ctx, cacheDir, repoRoot)
	if err != nil {
		return nil, "", err
	}
	defer store.close()
	cfg := DefaultEmbeddingConfig()
	cfg.RepoRoot, cfg.ProjectDir = repoRoot, repoRoot
	cfg.ParseCache, cfg.EmbCache, cfg.Index = store.parse, store.emb, store.index
	embedder := NewEmbedder(nil, cfg)
	embedder.Logger = logger
	buckets := embedder.scanPending(true)
	return buckets, store.index.VectorGeneration(), nil
}

// Text construction and sorting use only copied rows, so neither needs a
// store handle or the lifecycle lock.
func prepareEmbeddingRows(buckets map[string][]entityRow) []preparedRow {
	embedder := NewEmbedder(nil, DefaultEmbeddingConfig())
	rows := make([]preparedRow, 0)
	for _, label := range labelOrder(buckets) {
		group := make([]preparedRow, 0, len(buckets[label]))
		for _, row := range buckets[label] {
			group = append(group, preparedRow{row: row, text: embedder.buildEmbeddingText(row)})
		}
		sort.Slice(group, func(i, j int) bool { return len(group[i].text) < len(group[j].text) })
		rows = append(rows, group...)
	}
	return rows
}

func publishEmbeddingBatch(ctx context.Context, cacheDir, repoRoot, generation string, rows []preparedRow, vectors [][]float32, dimensions int) (int, bool, error) {
	lockedCtx, guard, err := storelifecycle.Acquire(ctx, cacheDir)
	if err != nil {
		return 0, false, fmt.Errorf("lock AST store lifecycle: %w", err)
	}
	defer guard.Release()
	store, err := openEmbeddingStore(lockedCtx, cacheDir, repoRoot)
	if err != nil {
		return 0, false, err
	}
	defer store.close()
	if store.index.VectorGeneration() != generation {
		return 0, false, nil
	}
	ents := make([]cachedEntity, 0, len(rows))
	vecs := make([][]float32, 0, len(rows))
	for i, prepared := range rows {
		row, vec := prepared.row, vectors[i]
		if len(vec) == 0 || row.Hash == "" || store.parse.GetHash(row.Path) != row.Hash {
			continue
		}
		entry := store.parse.GetEntry(row.Path)
		found := false
		if entry != nil {
			for _, current := range entry.Entities {
				if current.UID == row.UID && current.Label == row.Label {
					found = true
					break
				}
			}
		}
		if !found {
			continue
		}
		if len(vec) != dimensions {
			vec = fitVectorWidth(vec, dimensions)
		}
		ents = append(ents, row.entity())
		vecs = append(vecs, vec)
	}
	if err := store.index.StoreEntityVectors(lockedCtx, ents, vecs); err != nil {
		return 0, true, fmt.Errorf("store vectors: %w", err)
	}
	for i, ent := range ents {
		store.emb.Set(ent.Path, ent.UID, store.parse.GetHash(ent.Path), vecs[i])
	}
	if err := store.emb.Save(); err != nil {
		return len(ents), true, fmt.Errorf("save embedding cache: %w", err)
	}
	return len(ents), true, nil
}

func finalizeEmbeddingGeneration(ctx context.Context, cacheDir, generation string) error {
	lockedCtx, guard, err := storelifecycle.Acquire(ctx, cacheDir)
	if err != nil {
		return fmt.Errorf("lock AST store lifecycle: %w", err)
	}
	defer guard.Release()
	index, err := OpenSearchIndex(lockedCtx, cacheDir)
	if err != nil {
		return fmt.Errorf("open search index: %w", err)
	}
	defer func() { _ = index.Close() }()
	if index.VectorGeneration() != generation {
		return nil
	}
	if generation == "" {
		return index.FinalizeVectors(lockedCtx)
	}
	_, err = index.FinalizeVectorsForGeneration(lockedCtx, generation)
	return err
}

type embeddingStore struct {
	parse *ShardCache
	emb   *ShardEmbCache
	index *SearchIndex
}

func openEmbeddingStore(ctx context.Context, cacheDir, repoRoot string) (*embeddingStore, error) {
	store := &embeddingStore{}
	var err error
	store.parse, err = NewShardCache(cacheDir)
	if err != nil {
		return nil, fmt.Errorf("open parse cache: %w", err)
	}
	store.parse.SetRoot(repoRoot)
	store.emb, err = NewShardEmbCache(cacheDir, store.parse)
	if err == nil {
		store.index, err = OpenSearchIndex(ctx, cacheDir)
	}
	if err != nil {
		store.close()
		return nil, fmt.Errorf("open embedding store: %w", err)
	}
	return store, nil
}

func (s *embeddingStore) close() {
	if s.index != nil {
		_ = s.index.Close()
	}
	if s.emb != nil {
		_ = s.emb.Close()
	}
	if s.parse != nil {
		_ = s.parse.Close()
	}
}
