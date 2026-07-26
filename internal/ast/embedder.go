package ast

import (
	"context"
	"fmt"
	"log/slog"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

type EmbeddingConfig struct {
	BatchSize int

	MaxSourceChars int

	OnProgress func(done, total int)

	EmbCache *ShardEmbCache

	ParseCache *ShardCache
}

func DefaultEmbeddingConfig() EmbeddingConfig {
	return EmbeddingConfig{
		BatchSize:      128,
		MaxSourceChars: 500,
	}
}

type Embedder struct {
	client ai.EmbeddingClient
	cfg    EmbeddingConfig
	Logger *slog.Logger
}

func (e *Embedder) log() *slog.Logger { return slogutil.Resolve(e.Logger) }

func NewEmbedder(client ai.EmbeddingClient, cfg EmbeddingConfig) *Embedder {
	return &Embedder{
		client: client,
		cfg:    cfg,
	}
}

func (e *Embedder) CountPending(ctx context.Context) int {
	if e.cfg.EmbCache == nil || e.cfg.ParseCache == nil {
		return 0
	}
	total := 0
	for _, rows := range e.scanPending(false) {
		total += len(rows)
	}
	return total
}

var embeddableLabels = []string{
	"Function", "Class", "Procedure", "Method", "Package", "Trigger",
	"Struct", "Enum", "Type", "Interface", "Module", "Trait",
	"Variable", "Constant", "Table", "View",
}

type entityRow struct {
	UID       string
	Label     string
	Name      string
	Docstring string
	Path      string
	Line      int
	EndLine   int
	Context   string
	// Source is the entity's source snippet, precomputed during the single
	// streaming scan (while the shard is loaded) so the embedding phase does not
	// reload/pin shards to fetch it. Empty when source indexing is off.
	Source string
}

func (e *Embedder) RunCycle(ctx context.Context) (int, error) {
	if e.cfg.EmbCache == nil || e.cfg.ParseCache == nil {
		return 0, nil
	}

	// Single streaming pass: collect only rows that still need embedding,
	// bucketed by label, with their source snippet precomputed. Replaces the old
	// per-label AllEntries() scans (which pinned every shard in RAM and re-walked
	// the full cache ~2x per label).
	buckets := e.scanPending(true)
	grandTotal := 0
	for _, rows := range buckets {
		grandTotal += len(rows)
	}
	if grandTotal == 0 {
		return 0, nil
	}
	if e.cfg.OnProgress != nil {
		e.cfg.OnProgress(0, grandTotal)
	}

	done := 0
	for _, label := range embeddableLabels {
		rows := buckets[label]
		if len(rows) == 0 {
			continue
		}
		if ctx.Err() != nil {
			return done, ctx.Err()
		}

		n, err := e.processBatch(ctx, label, rows, &done, grandTotal)
		done += n
		if err != nil {
			e.log().Warn("process label", "label", label, "error", err)
		}
	}

	return done, nil
}

// scanPending performs ONE streaming pass over the parse cache (StreamEntries
// loads and evicts one shard at a time, bounding memory to O(1) shard), keeping
// only entities that still need embedding, bucketed by label. When withSnippet
// is true it also precomputes each row's source snippet from the loaded shard so
// the embedding phase never reloads a shard just to fetch source text.
func (e *Embedder) scanPending(withSnippet bool) map[string][]entityRow {
	labelSet := make(map[string]bool, len(embeddableLabels))
	for _, l := range embeddableLabels {
		labelSet[l] = true
	}

	buckets := make(map[string][]entityRow)
	e.cfg.ParseCache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		for _, ent := range entry.Entities {
			if !labelSet[ent.Label] || ent.UID == "" || ent.Name == "" {
				continue
			}
			hash := e.cfg.ParseCache.GetHash(ent.Path)
			if hash == "" {
				continue
			}
			if vec := e.cfg.EmbCache.Get(ent.Path, ent.UID, hash); vec != nil {
				continue
			}
			row := entityRow{
				UID:       ent.UID,
				Label:     ent.Label,
				Name:      ent.Name,
				Docstring: ent.Docstring,
				Path:      ent.Path,
				Line:      ent.Line,
				EndLine:   ent.EndLine,
				Context:   ent.Context,
			}
			if withSnippet {
				row.Source = embedSourceSnippet(entry.Source, ent.Line, ent.EndLine)
			}
			buckets[ent.Label] = append(buckets[ent.Label], row)
		}
		return true
	})

	// Deduplicate (Path, UID) across labels, keeping the label that appears
	// earliest in embeddableLabels order. The embedding cache is keyed on
	// (relPath, UID) WITHOUT the label, but the embedding text includes the
	// label, so when one UID appears under two embeddable labels (e.g. a TS
	// `class Foo` + `interface Foo`, or a same-named Table + View) both variants
	// collide on one cache key. The previous per-label interleaved Get/Set flow
	// let the FIRST embeddable label win (its Set made later labels skip); this
	// restores that, instead of letting the last-processed label overwrite.
	seen := make(map[string]struct{})
	for _, label := range embeddableLabels {
		rows := buckets[label]
		if len(rows) == 0 {
			continue
		}
		kept := rows[:0]
		for _, r := range rows {
			key := r.Path + "\x00" + r.UID
			if _, dup := seen[key]; dup {
				continue
			}
			seen[key] = struct{}{}
			kept = append(kept, r)
		}
		if len(kept) == 0 {
			delete(buckets, label)
		} else {
			buckets[label] = kept
		}
	}
	return buckets
}

// embedSourceSnippet extracts the [line, endLine] slice of a file's source used
// for the embedding text. It mirrors the original inline slicing exactly so the
// embedding text (and therefore the produced vectors) is byte-identical.
func embedSourceSnippet(fileSource string, line, endLine int) string {
	if fileSource == "" || line <= 0 {
		return fileSource
	}
	lines := strings.Split(fileSource, "\n")
	start := line - 1
	end := endLine
	if start < 0 {
		start = 0
	}
	if start >= len(lines) {
		return ""
	}
	if end <= start || end > len(lines) {
		end = len(lines)
	}
	return strings.Join(lines[start:end], "\n")
}

func (e *Embedder) processBatch(ctx context.Context, label string, rows []entityRow, donePtr *int, grandTotal int) (int, error) {
	total := 0

	for i := 0; i < len(rows); i += e.cfg.BatchSize {
		if ctx.Err() != nil {
			return total, ctx.Err()
		}

		end := i + e.cfg.BatchSize
		if end > len(rows) {
			end = len(rows)
		}
		batch := rows[i:end]

		texts := make([]string, len(batch))
		for j, row := range batch {
			texts[j] = e.buildEmbeddingText(row)
		}

		vectors, err := e.client.EmbedBatch(ctx, texts)
		if err != nil {
			return total, fmt.Errorf("embed batch: %w", err)
		}
		if len(vectors) != len(batch) {
			return total, fmt.Errorf("expected %d vectors, got %d", len(batch), len(vectors))
		}

		for j, row := range batch {
			vec := vectors[j]
			if len(vec) == 0 {
				continue
			}

			if len(vec) < ai.EmbeddingDimensions {
				padded := make([]float32, ai.EmbeddingDimensions)
				copy(padded, vec)
				vec = padded
			}

			if row.Path != "" {
				hash := e.cfg.ParseCache.GetHash(row.Path)
				if hash != "" {
					e.cfg.EmbCache.Set(row.Path, row.UID, hash, vec)
				}
			}
			total++
		}

		if err := e.cfg.EmbCache.Save(); err != nil {
			e.log().Warn("embedding cache save", "error", err)
		}

		if e.cfg.OnProgress != nil && grandTotal > 0 {
			e.cfg.OnProgress(*donePtr+total, grandTotal)
		}
	}

	return total, nil
}

func (e *Embedder) buildEmbeddingText(row entityRow) string {
	var parts []string

	parts = append(parts, "["+row.Label+"] "+row.Path)

	if row.Context != "" {
		parts = append(parts, "context: "+row.Context)
	}

	parts = append(parts, row.Name)

	if row.Docstring != "" {
		parts = append(parts, row.Docstring)
	}

	// row.Source is the line-sliced snippet precomputed during scanPending (from
	// the same file source and line range the old inline logic used); apply only
	// the MaxSourceChars cap here so the embedding text stays byte-identical.
	source := row.Source
	if source != "" {
		if len(source) > e.cfg.MaxSourceChars {
			source = source[:e.cfg.MaxSourceChars]
		}
		parts = append(parts, source)
	}

	return strings.Join(parts, "\n")
}

func RunEmbeddingLoop(ctx context.Context, interval time.Duration, cacheDir string, logger *slog.Logger) error {
	log := slogutil.Resolve(logger)

	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		log.Error("failed to create embedding client", "error", err)
		return fmt.Errorf("embedding client: %w", err)
	}

	cfg := DefaultEmbeddingConfig()

	if cacheDir != "" {
		if jc, err := NewShardCache(cacheDir); err == nil {
			cfg.ParseCache = jc
			defer func() { _ = jc.Close() }()

			if ec, err := NewShardEmbCache(cacheDir, jc); err == nil {
				cfg.EmbCache = ec
			}
		}
	}

	embedder := NewEmbedder(client, cfg)
	embedder.Logger = logger

	dbPath := filepath.Join(cacheDir, "ladybugdb")

	if n, err := embedder.RunCycle(ctx); err != nil {
		log.Warn("initial cycle error", "error", err)
	} else if n > 0 {
		log.Info("initial cycle complete", "entities_embedded", n)
		triggerEmbeddingRebuild(ctx, dbPath, cfg.ParseCache, cfg.EmbCache, logger)
	}

	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:

			if cfg.ParseCache != nil {
				cfg.ParseCache.Reload()
			}

			if n, err := embedder.RunCycle(ctx); err != nil {
				log.Warn("cycle error", "error", err)
			} else if n > 0 {
				log.Info("embedding cycle complete", "entities_embedded", n)
				triggerEmbeddingRebuild(ctx, dbPath, cfg.ParseCache, cfg.EmbCache, logger)
			}
		}
	}
}

func triggerEmbeddingRebuild(ctx context.Context, dbPath string, parseCache *ShardCache, embCache *ShardEmbCache, logger *slog.Logger) {
	log := slogutil.Resolve(logger)

	if parseCache == nil || embCache == nil {
		return
	}
	if parseCache.Count() == 0 {
		return
	}

	log.Info("rebuilding DB to inject embeddings")
	t0 := time.Now()

	lb := NewLadybugDB(LadybugConfig{DBPath: dbPath})

	if err := RebuildFromJSON(ctx, lb, parseCache, embCache, "", "", logger); err != nil {
		log.Error("rebuild error", "error", err)
		return
	}

	idxPath := dbPath + searchIndexSuffix
	searchIdx, err := OpenSearchIndex(idxPath)
	if err != nil {
		log.Error("open search index for embedding rebuild", "error", err)
	} else {
		embLookup := BuildEmbLookup(parseCache, embCache)
		if err := searchIdx.RebuildFromCache(parseCache, embLookup); err != nil {
			log.Error("search index rebuild after embeddings", "error", err)
		}
		_ = searchIdx.Close()
	}

	log.Info("rebuild complete", "duration_s", time.Since(t0).Seconds())
}
