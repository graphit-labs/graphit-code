package ast

import (
	"context"
	"fmt"
	"log/slog"
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

	// Index is where a query reads a vector from: the embedding is a column of the
	// entity row, so an entity that no longer exists cannot leave a vector behind.
	Index *SearchIndex

	// EmbCache is the durable copy, and it is not a second opinion about which vector is
	// current — its entries are keyed on the file's content hash, so a changed file
	// invalidates its own. It exists because a rebuild DROPS the entity table, and
	// recomputing an embedding is expensive where re-reading one is not.
	EmbCache *ShardEmbCache

	ParseCache *ShardCache

	// RepoRoot is the indexed tree, and it is where the snippet in every embedding
	// comes from: a shard no longer carries a copy of the file, so this is the only
	// text there is. It is handed to the parse cache, which owns the hash check that
	// makes reading it safe.
	//
	// This is also what makes `ast.index_source: false` work rather than silently
	// degrade semantic search. That setting says "do not keep a copy of the source",
	// not "do not look at the source": an embedding is a vector, not recoverable
	// text, so it can be computed from the file while the text itself is never
	// persisted. Empty disables the read — an imported context has no tree here — and
	// the embedding falls back to name, docstring and context alone.
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
	if e.cfg.Index == nil || e.cfg.EmbCache == nil || e.cfg.ParseCache == nil {
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

// preparedRow pairs a row with its embedding text so the text is built once and the batching
// can order by its length. See processBatch.
type preparedRow struct {
	row  entityRow
	text string
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
	IsDep     bool
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
	if e.cfg.Index == nil || e.cfg.EmbCache == nil || e.cfg.ParseCache == nil {
		return 0, nil
	}
	generation := e.cfg.Index.VectorGeneration()

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
		if generation == "" {
			return 0, e.cfg.Index.FinalizeVectors(ctx)
		}
		_, err := e.cfg.Index.FinalizeVectorsForGeneration(ctx, generation)
		return 0, err
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

	// The vector index cannot exist before the vectors do — see FinalizeVectors.
	if done > 0 {
		var err error
		if generation == "" {
			err = e.cfg.Index.FinalizeVectors(ctx)
		} else {
			_, err = e.cfg.Index.FinalizeVectorsForGeneration(ctx, generation)
		}
		if err != nil {
			return done, fmt.Errorf("finalizing vector index: %w", err)
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
		// Split ONCE per shard, not once per entity, and resolved lazily so a shard
		// whose entities are all comments never touches the disk.
		var fileLines []string
		sourceResolved := false

		for _, ent := range entry.Entities {
			if ent.UID == "" || ent.Name == "" {
				continue
			}
			rank, embeddable := ranker.rank(ent.Lang, ent.Label)
			if !embeddable {
				continue
			}
			// The CACHE decides what is pending, not the index: it is the copy that
			// survives a rebuild, so right after one the index is empty while the work is
			// genuinely already done.
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
				IsDep:     ent.IsDep,
				Rank:      rank,
			}
			// A comment's snippet is the comment itself, marker syntax included, so
			// slicing it appends the name a second time and pays for the shard read
			// to do it. Every other label's snippet is a body its name does not carry.
			if withSnippet && ent.Label != LabelComment {
				if !sourceResolved {
					fileLines = e.fileLinesFor(relPath)
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

// fileLinesFor resolves the text an entity's snippet is sliced out of.
//
// The parse cache owns the precedence — the working tree when there is one, the shard's
// own copy for an imported context that has no tree — and it owns the hash check that
// makes either safe. See ShardCache.SourceOf.
func (e *Embedder) fileLinesFor(relPath string) []string {
	if e.cfg.ParseCache == nil || relPath == "" {
		return nil
	}
	source := e.cfg.ParseCache.SourceOf(relPath)
	if source == "" {
		return nil
	}
	return strings.Split(source, "\n")
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

// vectorFlushRows bounds how many vectors are held before they are written to the index.
// At 768 float32 a row is ~3 KB, so this is ~12 MB of buffer to cut the number of dataset
// versions a cycle produces by a factor of 32.
const vectorFlushRows = 4096

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

	// Vectors are handed to the index in large groups rather than one group per model batch.
	// An Upsert is a delete plus an append, so each one is a dataset version and a fragment:
	// at one per 128-entity batch a single cycle left 484 versions and 643 fragments behind
	// and grew the index to 858 MB before the compaction at the end could reclaim it. The
	// cost in TIME was measured at 0.2% of the cycle — this is about the disk it churns
	// through, not about speed.
	//
	// The cache is still saved per batch, so a cycle that dies loses no computed vector: the
	// next rebuild replays them from there.
	var pending []cachedEntity
	var pendingVecs [][]float32
	flushVectors := func() error {
		if len(pending) == 0 {
			return nil
		}
		if err := e.cfg.Index.StoreEntityVectors(ctx, pending, pendingVecs); err != nil {
			return fmt.Errorf("store vectors: %w", err)
		}
		pending, pendingVecs = pending[:0], pendingVecs[:0]
		return nil
	}

	// Build every text once and batch the SHORT ones together.
	//
	// The model receives a batchSize x maxLen tensor, where maxLen is the longest text in
	// the batch — so a batch costs its worst member, for all of it. MEASURED on this
	// repository: 40,365 entities with a median of 29 tokens and a maximum of 307, which in
	// arrival order makes the tensor 1.9x the tokens that carry any signal. Grouping texts
	// of similar length brings it to 1.0x and the model's work to 54%.
	//
	// Nothing about a vector changes: an entity is embedded from its own text, so only which
	// entities share a tensor row-length moves.
	prepared := make([]preparedRow, len(rows))
	for i, row := range rows {
		prepared[i] = preparedRow{row: row, text: e.buildEmbeddingText(row)}
	}
	sort.Slice(prepared, func(i, j int) bool {
		return len(prepared[i].text) < len(prepared[j].text)
	})

	for i := 0; i < len(prepared); i += e.cfg.BatchSize {
		if ctx.Err() != nil {
			return total, ctx.Err()
		}

		end := i + e.cfg.BatchSize
		if end > len(prepared) {
			end = len(prepared)
		}
		group := prepared[i:end]

		batch := make([]entityRow, len(group))
		texts := make([]string, len(group))
		for j, p := range group {
			batch[j] = p.row
			texts[j] = p.text
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

			if dim := e.client.Dimensions(); len(vec) != dim {
				vec = fitVectorWidth(vec, dim)
			}

			if row.Path != "" {
				if hash := e.cfg.ParseCache.GetHash(row.Path); hash != "" {
					e.cfg.EmbCache.Set(row.Path, row.UID, hash, vec)
				}
			}
			pending = append(pending, row.entity())
			pendingVecs = append(pendingVecs, vec)
			total++
		}

		if len(pending) >= vectorFlushRows {
			if err := flushVectors(); err != nil {
				return total, err
			}
		}
		if err := e.cfg.EmbCache.Save(); err != nil {
			e.log().Warn("embedding cache save", "error", err)
		}

		if e.cfg.OnProgress != nil && grandTotal > 0 {
			e.cfg.OnProgress(*donePtr+total, grandTotal)
		}
	}

	if err := flushVectors(); err != nil {
		return total, err
	}

	return total, nil
}

// entity rebuilds the cached entity a row came from, so the search row can be composed by
// buildEntityRow — the single constructor — instead of a second, drifting one.
func (r entityRow) entity() cachedEntity {
	return cachedEntity{
		UID:       r.UID,
		Name:      r.Name,
		Label:     r.Label,
		Path:      r.Path,
		Line:      r.Line,
		Docstring: r.Docstring,
		IsDep:     r.IsDep,
	}
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
			jc.SetRoot(cfg.RepoRoot)
			cfg.ParseCache = jc
			defer func() { _ = jc.Close() }()

			if ec, ecErr := NewShardEmbCache(cacheDir, jc); ecErr == nil {
				cfg.EmbCache = ec
				defer func() { _ = ec.Close() }()
			}

		}
	}

	idx, err := OpenSearchIndex(ctx, cacheDir)
	if err != nil {
		log.Error("open search index", "error", err)
		return fmt.Errorf("open search index: %w", err)
	}
	defer func() { _ = idx.Close() }()
	cfg.Index = idx

	embedder := NewEmbedder(client, cfg)
	embedder.Logger = logger

	// One cycle, holding the process-wide heavy-work slot for its whole duration.
	// The cycle drives an ONNX session sized to the entire CPU budget, which is
	// precisely the work the gate exists to serialize. The graph itself is not
	// touched: a vector lives in the search index, and the icebug bundle never
	// carried one.
	// The daemon runs one of these loops per active project inside a single process.
	cycle := func(label string, reload bool) {
		// The gate has ONE slot for the whole machine, shared with every project's reindex
		// and every other project's embedding cycle. A cycle that waits on it looks
		// identical to a cycle that is working, from outside and from the log — which is
		// part of what "embedding is slow" turns out to be. Say so when the wait is long
		// enough to matter, as the sync module already does for its own.
		askedAt := time.Now()
		release, err := sysutil.AcquireHeavy(ctx)
		if err != nil {
			return
		}
		defer release()
		if waited := time.Since(askedAt); waited > time.Second {
			log.Info(label+" waited for the indexing slot",
				"waited", waited.Round(time.Millisecond))
		}

		if reload && cfg.ParseCache != nil {
			cfg.ParseCache.Reload()
		}

		if n, err := embedder.RunCycle(ctx); err != nil {
			log.Warn(label+" error", "error", err)
		} else if n > 0 {
			log.Info(label+" complete", "entities_embedded", n)
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
