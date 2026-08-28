package ast

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

type EmbeddingConfig struct {
	BatchSize int

	MaxSourceChars int

	OnProgress func(done, total int)

	EmbCache *ShardEmbCache

	ParseCache *ShardCache

	// RepoRoot is the indexed tree, read only when the parse cache has no text for
	// a file — which is what `ast.index_source: false` produces.
	//
	// That setting says "do not keep a copy of the source", not "do not look at the
	// source": an embedding is a vector, not recoverable text, so it can be computed
	// from the file and persisted while the text itself never is. Without this the
	// flag silently degraded semantic search, since the snippet is the only part of
	// the embedded text that describes what an entity DOES rather than what it is
	// called. Empty disables the read, and the embedding falls back to name,
	// docstring and context alone.
	RepoRoot string

	// ProjectDir is the project whose grammars decide which labels get a vector —
	// `embed_labels`, resolved project-then-user-then-runtime.
	//
	// It is the same directory as RepoRoot in every caller, and separate from it on
	// purpose: RepoRoot is a place to READ SOURCE FROM, and blanking it is a
	// supported state that means "do not read the tree". Blanking the answer to
	// "which labels does this language embed" is a different thing entirely — it
	// embeds nothing — and one field cannot mean both. Empty here falls back to the
	// user and runtime grammar levels, which is what a caller outside any project
	// should get.
	ProjectDir string
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

// embedRanker answers "does this (lang, label) get a vector, and at what
// priority" from the grammars themselves — `embed_labels` in each language's query
// YAML. See ExternalQueryFile.EmbedLabels for why the answer cannot live here.
//
// Rank is the label's index in its own language's declared list, and it is what
// resolves a (path, uid) carried by two labels: lower rank wins. Comparing ranks
// across languages is meaningless and never happens — a collision is one entity in
// one file, so both sides come from the same grammar.
type embedRanker struct {
	projectDir string
	byLang     map[string]map[string]int
}

func newEmbedRanker(projectDir string) *embedRanker {
	return &embedRanker{projectDir: projectDir, byLang: make(map[string]map[string]int)}
}

// rank reports the label's priority for its language, and whether the language
// asked for it to be embedded at all.
func (r *embedRanker) rank(lang, label string) (int, bool) {
	ranks, ok := r.byLang[lang]
	if !ok {
		declared := EmbedLabelsForLang(r.projectDir, lang)
		ranks = make(map[string]int, len(declared))
		for i, l := range declared {
			if _, dup := ranks[l]; !dup {
				ranks[l] = i
			}
		}
		r.byLang[lang] = ranks
	}
	i, ok := ranks[label]
	return i, ok
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
	// Rank is this label's index in its language's embed_labels, carried on the
	// row because the row is the only place that still knows which language
	// answered for it. See embedRanker.
	Rank int
	// Source is the entity's source snippet, precomputed during the single
	// streaming scan (while the shard is loaded) so the embedding phase does not
	// reload/pin shards to fetch it. Empty when source indexing is off.
	Source string
}

// labelOrder puts the buckets in a deterministic, grammar-declared order: by the
// best rank any of a label's rows carries, then by name so two labels that tie
// never swap between runs.
//
// The batch loop and the dedup both walk this, and they must walk the SAME order:
// the dedup decides which label keeps a (path, uid), and it decides it by being
// there first.
func labelOrder(buckets map[string][]entityRow) []string {
	labels := make([]string, 0, len(buckets))
	best := make(map[string]int, len(buckets))
	for label, rows := range buckets {
		labels = append(labels, label)
		r := rows[0].Rank
		for _, row := range rows[1:] {
			if row.Rank < r {
				r = row.Rank
			}
		}
		best[label] = r
	}
	sort.Slice(labels, func(i, j int) bool {
		if best[labels[i]] != best[labels[j]] {
			return best[labels[i]] < best[labels[j]]
		}
		return labels[i] < labels[j]
	})
	return labels
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
	for _, label := range labelOrder(buckets) {
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
	// Which labels are embeddable is a per-LANGUAGE answer, read from the grammar
	// that produced the entity — so it is resolved per row, not once for the scan.
	// The ranker memoises each language the first time it is asked.
	ranker := newEmbedRanker(e.cfg.ProjectDir)

	// Shards with no text and no readable file: their entities get embedded by name
	// alone. Counted so the degradation is reported instead of being silent — the
	// case is a context installed from elsewhere whose origin indexed without source,
	// where there is no tree here to read and nothing the operator can do locally,
	// but plenty they would want to know.
	textless := 0

	buckets := make(map[string][]entityRow)
	e.cfg.ParseCache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		// Split ONCE per shard, not once per entity, and resolved lazily so a cache
		// that DOES carry text never touches the disk.
		var fileLines []string
		sourceResolved := false
		if entry.Source != "" {
			fileLines = strings.Split(entry.Source, "\n")
			sourceResolved = true
		}

		for _, ent := range entry.Entities {
			if ent.UID == "" || ent.Name == "" {
				continue
			}
			rank, embeddable := ranker.rank(ent.Lang, ent.Label)
			if !embeddable {
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
				Rank:      rank,
			}
			// A comment's snippet is the comment itself, marker syntax included, so
			// slicing it appends the name a second time and pays for the shard read
			// to do it. Every other label's snippet is a body its name does not carry.
			if withSnippet && ent.Label != LabelComment {
				if !sourceResolved {
					fileLines = e.sourceFromDisk(relPath, hash)
					sourceResolved = true
					if len(fileLines) == 0 {
						textless++
					}
				}
				row.Source = sliceLines(fileLines, ent.Line, ent.EndLine)
			}
			buckets[ent.Label] = append(buckets[ent.Label], row)
		}
		return true
	})

	// Deduplicate (Path, UID) across labels, keeping the label its own grammar
	// ranked first in embed_labels. The embedding cache is keyed on (relPath, UID)
	// WITHOUT the label, but the embedding text includes the label, so when one UID
	// appears under two embeddable labels (e.g. a TS `class Foo` + `interface Foo`,
	// or a same-named Table + View) both variants collide on one cache key. The
	// per-label interleaved Get/Set flow this replaced let the FIRST embeddable
	// label win (its Set made later labels skip); ranked order restores that, with
	// the grammar rather than a Go slice saying which label comes first.
	seen := make(map[string]struct{})
	for _, label := range labelOrder(buckets) {
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

	if textless > 0 {
		e.log().Warn("embedding without source text: these entities are described by name, "+
			"docstring and context only, so semantic search over what they DO is weaker",
			"files", textless, "repo_root_set", e.cfg.RepoRoot != "")
	}
	return buckets
}

// sourceFromDisk reads the indexed file so an entity can still be embedded with its
// body when the parse cache holds no text.
//
// The whole file is read because the hash covers the whole file — there is no way to
// verify it from a line range. Memory is at parity with the default path, where the
// shard being streamed carries the same text; the extra cost is one read per shard.
//
// SAFETY: the content hash must match the one the shard was built from. The embedding
// cache is keyed on that hash, so embedding newer text under an older key would cache
// a vector describing code that the graph does not contain — and it would survive
// until the file changed again. A mismatch means the shard is stale and about to be
// reparsed, so no snippet is better than the wrong one.
func (e *Embedder) sourceFromDisk(relPath, shardHash string) []string {
	if e.cfg.RepoRoot == "" || relPath == "" || shardHash == "" {
		return nil
	}
	data, err := os.ReadFile(filepath.Join(e.cfg.RepoRoot, relPath))
	if err != nil {
		return nil
	}
	if contentHashOf(data) != shardHash {
		return nil
	}
	return strings.Split(string(data), "\n")
}

// sliceLines returns the [line, endLine] slice of an already-split file.
//
// Splitting belongs to the caller and happens ONCE per shard. It used to happen inside
// embedSourceSnippet, which runs once per ENTITY: a file with 500 embeddable entities
// was split into lines 500 times, allocating a slice of every line in the file each
// time.
func sliceLines(lines []string, line, endLine int) string {
	if len(lines) == 0 || line <= 0 {
		return ""
	}
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

// embedSourceSnippet extracts the [line, endLine] slice of a file's source used for
// the embedding text, mirroring the original inline slicing exactly so the embedding
// text — and therefore the produced vectors — stays byte-identical.
func embedSourceSnippet(fileSource string, line, endLine int) string {
	if fileSource == "" || line <= 0 {
		return fileSource
	}
	return sliceLines(strings.Split(fileSource, "\n"), line, endLine)
}

// maxSkippedSample caps how many unembeddable entities are named in the warning.
// The count is the signal; the sample is what makes it actionable.
const maxSkippedSample = 5

func (e *Embedder) processBatch(ctx context.Context, label string, rows []entityRow, donePtr *int, grandTotal int) (int, error) {
	total := 0
	skipped := 0
	var skippedSample []string

	defer func() {
		if skipped > 0 {
			e.log().Warn("entities skipped: the embedding client returned no vector, "+
				"so they stay out of semantic search until their text changes",
				"label", label, "skipped", skipped, "sample", skippedSample)
		}
	}()

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
				// The client could not embed this one — today that means its text
				// could not be tokenized. Named, because the alternative is an
				// entity that silently never becomes searchable, and nothing says
				// which one.
				skipped++
				if len(skippedSample) < maxSkippedSample {
					skippedSample = append(skippedSample, row.Path+"::"+row.UID)
				}
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

func RunEmbeddingLoop(ctx context.Context, interval time.Duration, cacheDir, repoRoot string, logger *slog.Logger) error {
	log := slogutil.Resolve(logger)

	client, err := ai.NewEmbeddingClientFromConfig()
	if err != nil {
		log.Error("failed to create embedding client", "error", err)
		return fmt.Errorf("embedding client: %w", err)
	}

	cfg := DefaultEmbeddingConfig()
	cfg.RepoRoot = repoRoot
	cfg.ProjectDir = repoRoot

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

	// One cycle, holding the process-wide heavy-work slot for its whole duration.
	// The cycle drives an ONNX session sized to the entire CPU budget, and the
	// search-index rebuild that follows a productive one is heavy too — so this is
	// precisely the work the gate exists to serialize. The graph itself is not
	// touched: vectors live in the search index + embedding shards, and the
	// icebug bundle never carried them.
	// The daemon runs one of these loops per active project inside a single process.
	cycle := func(label string, reload bool) {
		release, err := sysutil.AcquireHeavy(ctx)
		if err != nil {
			return
		}
		defer release()

		if reload && cfg.ParseCache != nil {
			cfg.ParseCache.Reload()
		}

		if n, err := embedder.RunCycle(ctx); err != nil {
			log.Warn(label+" error", "error", err)
		} else if n > 0 {
			log.Info(label+" complete", "entities_embedded", n)
			rebuildSearchIndexForEmbeddings(ctx, cacheDir, cfg.ParseCache, cfg.EmbCache, logger)
		}
	}

	cycle("initial cycle", false)

	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return nil
		case <-ticker.C:
			cycle("embedding cycle", true)
		}
	}
}

// rebuildSearchIndexForEmbeddings rewrites the search index from the shards after an
// embedding cycle produced new vectors. The icebug graph never holds vectors, so only
// the LanceDB sidecar is rebuilt.
func rebuildSearchIndexForEmbeddings(ctx context.Context, storeDir string, parseCache *ShardCache, embCache *ShardEmbCache, logger *slog.Logger) {
	log := slogutil.Resolve(logger)
	if parseCache == nil || embCache == nil || parseCache.Count() == 0 {
		return
	}
	log.Info("rebuilding search index to inject embeddings")
	t0 := time.Now()

	idx, err := OpenSearchIndex(ctx, storeDir)
	if err != nil {
		log.Error("open search index", "error", err)
		return
	}
	defer func() { _ = idx.Close() }()
	if err := idx.RebuildFromCache(ctx, parseCache, BuildEmbLookup(parseCache, embCache)); err != nil {
		log.Error("search index rebuild", "error", err)
		return
	}
	log.Info("search index rebuild complete", "duration_s", time.Since(t0).Seconds())
}
