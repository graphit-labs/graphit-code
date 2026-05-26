package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
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
}

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
	for _, label := range embeddableLabels {
		rows := e.scanEntities(label)
		for _, row := range rows {
			hash := e.cfg.ParseCache.GetHash(row.Path)
			if hash == "" {
				continue
			}
			if vec := e.cfg.EmbCache.Get(row.Path, row.UID, hash); vec == nil {
				total++
			}
		}
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
}

func (e *Embedder) RunCycle(ctx context.Context) (int, error) {
	if e.cfg.EmbCache == nil || e.cfg.ParseCache == nil {
		return 0, nil
	}

	grandTotal := e.CountPending(ctx)
	if grandTotal == 0 {
		return 0, nil
	}
	if e.cfg.OnProgress != nil {
		e.cfg.OnProgress(0, grandTotal)
	}

	done := 0
	for _, label := range embeddableLabels {
		if ctx.Err() != nil {
			return done, ctx.Err()
		}

		n, err := e.processLabel(ctx, label, &done, grandTotal)
		done += n
		if err != nil {
			fmt.Fprintf(os.Stderr, "[embedder] %s: %v\n", label, err)
		}
	}

	return done, nil
}

func (e *Embedder) processLabel(ctx context.Context, label string, donePtr *int, grandTotal int) (int, error) {

	allRows := e.scanEntities(label)
	if len(allRows) == 0 {
		return 0, nil
	}

	var needRows []entityRow
	for _, row := range allRows {
		hash := e.cfg.ParseCache.GetHash(row.Path)
		if hash == "" {
			continue
		}
		if vec := e.cfg.EmbCache.Get(row.Path, row.UID, hash); vec != nil {
			continue
		}
		needRows = append(needRows, row)
	}

	if len(needRows) == 0 {
		return 0, nil
	}

	return e.processBatch(ctx, label, needRows, donePtr, grandTotal)
}

func (e *Embedder) scanEntities(label string) []entityRow {
	entries := e.cfg.ParseCache.AllEntries()
	if len(entries) == 0 {
		return nil
	}

	labelSet := map[string]bool{label: true}
	var rows []entityRow
	for _, entry := range entries {
		for _, ent := range entry.Entities {
			if !labelSet[ent.Label] {
				continue
			}
			if ent.UID == "" || ent.Name == "" {
				continue
			}
			rows = append(rows, entityRow{
				UID:       ent.UID,
				Label:     ent.Label,
				Name:      ent.Name,
				Docstring: ent.Docstring,
				Path:      ent.Path,
				Line:      ent.Line,
				EndLine:   ent.EndLine,
			})
		}
	}
	return rows
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
			fmt.Fprintf(os.Stderr, "[embedder] cache save: %v\n", err)
		}

		if e.cfg.OnProgress != nil && grandTotal > 0 {
			e.cfg.OnProgress(*donePtr+total, grandTotal)
		}
	}

	return total, nil
}

func (e *Embedder) buildEmbeddingText(row entityRow) string {
	var parts []string

	parts = append(parts, row.Name)

	if row.Docstring != "" {
		parts = append(parts, row.Docstring)
	}

	source := ""
	if row.Path != "" && e.cfg.ParseCache != nil {
		if entry := e.cfg.ParseCache.GetEntry(row.Path); entry != nil {
			source = entry.Source
		}
	}

	if source != "" && row.Line > 0 {
		lines := strings.Split(source, "\n")
		start := row.Line - 1
		end := row.EndLine
		if start < 0 {
			start = 0
		}
		if start < len(lines) {
			if end <= start || end > len(lines) {
				end = len(lines)
			}
			source = strings.Join(lines[start:end], "\n")
		} else {
			source = ""
		}
	}

	if source != "" {
		if len(source) > e.cfg.MaxSourceChars {
			source = source[:e.cfg.MaxSourceChars]
		}
		parts = append(parts, source)
	}

	return strings.Join(parts, "\n")
}

type EmbeddingStats struct {
	Label    string
	Total    int
	Embedded int
	Missing  int
}

func (e *Embedder) Stats(ctx context.Context) ([]EmbeddingStats, error) {
	if e.cfg.ParseCache == nil || e.cfg.EmbCache == nil {
		return nil, nil
	}

	labelCounts := make(map[string]int)
	labelEmbedded := make(map[string]int)

	entries := e.cfg.ParseCache.AllEntries()
	for _, entry := range entries {
		for _, ent := range entry.Entities {
			if ent.UID == "" || ent.Name == "" {
				continue
			}
			labelCounts[ent.Label]++
			hash := e.cfg.ParseCache.GetHash(ent.Path)
			if hash != "" {
				if vec := e.cfg.EmbCache.Get(ent.Path, ent.UID, hash); vec != nil {
					labelEmbedded[ent.Label]++
				}
			}
		}
	}

	var stats []EmbeddingStats
	for _, label := range embeddableLabels {
		total := labelCounts[label]
		if total > 0 {
			embedded := labelEmbedded[label]
			stats = append(stats, EmbeddingStats{
				Label:    label,
				Total:    total,
				Embedded: embedded,
				Missing:  total - embedded,
			})
		}
	}

	return stats, nil
}



func RunEmbeddingLoop(ctx context.Context, interval time.Duration, cacheDir string) error {
	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		fmt.Fprintf(os.Stderr, "[embedder] failed to create embedding client: %v\n", err)
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

	dbPath := filepath.Join(cacheDir, "ladybugdb")

	if n, err := embedder.RunCycle(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "[embedder] initial cycle error: %v\n", err)
	} else if n > 0 {
		fmt.Fprintf(os.Stderr, "[embedder] initial cycle: %d entities embedded\n", n)
		triggerEmbeddingRebuild(ctx, dbPath, cfg.ParseCache, cfg.EmbCache)
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
				fmt.Fprintf(os.Stderr, "[embedder] cycle error: %v\n", err)
			} else if n > 0 {
				fmt.Fprintf(os.Stderr, "[embedder] embedded %d entities\n", n)
				triggerEmbeddingRebuild(ctx, dbPath, cfg.ParseCache, cfg.EmbCache)
			}
		}
	}
}

func triggerEmbeddingRebuild(ctx context.Context, dbPath string, parseCache *ShardCache, embCache *ShardEmbCache) {
	if parseCache == nil || embCache == nil {
		return
	}
	if parseCache.Count() == 0 {
		return
	}

	fmt.Fprintf(os.Stderr, "[embedder] rebuilding DB to inject embeddings…\n")
	t0 := time.Now()

	lb := NewLadybugDB(LadybugConfig{DBPath: dbPath})

	if err := RebuildFromJSON(ctx, lb, parseCache, embCache, "", ""); err != nil {
		fmt.Fprintf(os.Stderr, "[embedder] rebuild error: %v\n", err)
		return
	}

	fmt.Fprintf(os.Stderr, "[embedder] rebuild complete (%.1fs)\n", time.Since(t0).Seconds())
}
