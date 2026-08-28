package ast

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// The AST search index, on LanceDB.
//
// WHY THIS IS NOT A SECOND SEARCH BACKEND. It replaces the SQLite sidecar rather than joining it:
// a published graph on S3 carries no SQLite file and no source text, so the only search that can
// answer against it is one the engine can run over object storage. Two backends would also mean
// two relevance behaviours, and the one users hit would depend on where the graph came from.
//
// WHAT IS THE SAME AS THE SQLITE INDEX, deliberately, so the port is a port and not a redesign:
//
//   - two tables, files and entities, because a file match and an entity match are different
//     answers and ranking them in one pile buries the entities;
//   - the row is built in ONE place for both write paths — see buildEntityRow. The SQLite index
//     learned this the hard way: its incremental INSERT listed every column except name_tri, so
//     any file touched by an incremental silently lost its trigrams until the next full rebuild;
//   - the full rebuild indexes AFTER the bulk load, not during it.
//
// WHAT IS DIFFERENT, because the engine is:
//
//   - no integer row ids. SQLite needed them to join external-content FTS tables to their content;
//     here the uid is the key and nothing has to be numbered;
//   - no triggers. Lance keeps appended rows searchable by scanning the fragments that are not in
//     the index yet, so the incremental does not maintain an index per row — it folds afterwards,
//     once, for latency. Measured: lancestore.TestFoldIsAboutLatencyNotVisibility;
//   - no separate vector table and no manual compaction of dead vectors. The embedding is a column
//     of the entity, so deleting the entity deletes its vector, and the whole class of bug where a
//     stale vector answers for an entity that no longer exists cannot be expressed.

// LanceIndexDirName is the search index's directory, a sibling of the graph store.
const LanceIndexDirName = "search.lance"

// The two tables and the columns that carry the search.
const (
	lanceFilesTable    = "files"
	lanceEntitiesTable = "entities"

	// lanceBodyColumn is the single text column the full-text index is built on.
	//
	// ONE COLUMN AND NOT SEVEN. The SQLite index queried seven weighted fields separately
	// (entity name_split at 10.0, docstring at 3.0, etype at 2.0, path at 1.0, and three more for
	// files) and fused the passes in Go. That is not portable to an engine whose full-text query
	// takes one column, and reproducing the fusion in Go would be exactly the Go-side search this
	// project ruled out. So the fields are concatenated into one document and the engine ranks it
	// with BM25, which already weights by term rarity — the thing the manual weights approximated.
	lanceBodyColumn = "body"

	// lanceVectorColumn holds the entity embedding, for the vector half of the hybrid query.
	lanceVectorColumn = "embedding"
)

// SearchIndex is the search sidecar of a graph store.
type SearchIndex struct {
	store    *lancestore.Store
	files    *lancestore.Table
	entities *lancestore.Table

	// vectorCount is how many entity rows carry an embedding, read from the embeds
	// status file beside the index. When it is zero, hybrid and semantic queries
	// cannot answer — the engine's RRF rank needs a _distance column no row can
	// produce — so they degrade to keywords instead of failing.
	vectorCount int64
	// storeDir is the local store this index was opened from, when there is one.
	// Remote indexes (S3) have none, and hold vectorCount zero by construction.
	storeDir string

	Logger *slog.Logger
}

func (s *SearchIndex) log() *slog.Logger { return slogutil.Resolve(s.Logger) }

// LanceIndexPath is where the search index of a store directory lives: a sibling
// of graph.icebug under the same store dir.
func LanceIndexPath(storeDir string) string {
	return filepath.Join(storeDir, LanceIndexDirName)
}

// embedsStatusFile is the sidecar that records how many entity rows carry an embedding,
// written by every rebuild. It is the only thing that tells a hybrid/semantic query whether
// the vector channel can answer at all — the engine's RRF rank needs a `_distance` column
// that no row can produce when every embedding is NULL, and the resulting error is one word
// out of a C++ query planner rather than an actionable message.
const embedsStatusFile = "embeds.json"

type embedsStatus struct {
	Vectors  int64 `json:"vectors"`
	Entities int64 `json:"entities"`
}

func embedsStatusPath(storeDir string) string {
	return filepath.Join(storeDir, embedsStatusFile)
}

func readEmbedsStatus(storeDir string) embedsStatus {
	var st embedsStatus
	raw, err := os.ReadFile(embedsStatusPath(storeDir))
	if err != nil {
		return st
	}
	_ = json.Unmarshal(raw, &st)
	return st
}

func writeEmbedsStatus(storeDir string, vectors, entities int64) error {
	raw, err := json.MarshalIndent(embedsStatus{Vectors: vectors, Entities: entities}, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(embedsStatusPath(storeDir), raw, 0o644)
}

// OpenSearchIndex opens the search index of a graph store.
//
// It resolves to object storage when the store was MOUNTED and to a local directory otherwise, and
// the caller does not have to know which — see searchConfigFor. That is what lets a query service
// built the same way serve a local project and a Hub context.
func OpenSearchIndex(ctx context.Context, storeDir string) (*SearchIndex, error) {
	si, err := openLanceIndex(ctx, searchConfigFor(storeDir))
	if err != nil {
		return nil, err
	}
	si.storeDir = storeDir
	si.vectorCount = readEmbedsStatus(storeDir).Vectors
	return si, nil
}

// OpenSearchIndexForDir is OpenSearchIndex under its honest name: the argument is the
// store directory, never a database file.
func OpenSearchIndexForDir(ctx context.Context, storeDir string) (*SearchIndex, error) {
	return OpenSearchIndex(ctx, storeDir)
}

// OpenSearchIndexAt opens an index by its own URI, which is how a PUBLISHED index on S3 is
// read: the caller passes `s3://bucket/prefix` and the engine queries it in place.
func OpenSearchIndexAt(ctx context.Context, cfg lancestore.Config) (*SearchIndex, error) {
	return openLanceIndex(ctx, cfg)
}

func openLanceIndex(ctx context.Context, cfg lancestore.Config) (*SearchIndex, error) {
	st, err := lancestore.Open(ctx, cfg)
	if err != nil {
		return nil, fmt.Errorf("open search index: %w", err)
	}
	return &SearchIndex{store: st}, nil
}

// Close releases the store.
func (s *SearchIndex) Close() error {
	if s == nil || s.store == nil {
		return nil
	}
	return s.store.Close()
}

// Remote reports whether this index is a published one, which is read-only.
func (s *SearchIndex) Remote() bool { return s.store != nil && s.store.Remote() }

// ---------- schema ----------

func lanceFilesSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "path", Type: lancestore.FieldString},
		{Name: "name", Type: lancestore.FieldString},
		{Name: "source", Type: lancestore.FieldString, Nullable: true},
		{Name: lanceBodyColumn, Type: lancestore.FieldString},
	}}
}

func lanceEntitiesSchema() lancestore.Schema {
	return lancestore.Schema{Fields: []lancestore.Field{
		{Name: "uid", Type: lancestore.FieldString},
		{Name: "name", Type: lancestore.FieldString},
		{Name: "etype", Type: lancestore.FieldString},
		{Name: "path", Type: lancestore.FieldString},
		{Name: "line", Type: lancestore.FieldInt64},
		{Name: "docstring", Type: lancestore.FieldString, Nullable: true},
		{Name: "is_dep", Type: lancestore.FieldBool},
		{Name: lanceBodyColumn, Type: lancestore.FieldString},
		// Nullable: an entity with no embedding is still searchable by keyword, and refusing to
		// store it would make the whole index depend on the embedder having run.
		{Name: lanceVectorColumn, Type: lancestore.FieldVector, Dim: ai.EmbeddingDimensions, Nullable: true},
	}}
}

// lanceIndexes is what has to exist for search to answer.
//
// The inverted index is NOT optional: a full-text query without one matches nothing at all, so
// this is not a performance step that can be deferred to an idle moment.
func lanceEntityIndexes() []lancestore.Index {
	return []lancestore.Index{
		{Column: lanceBodyColumn, Kind: lancestore.IndexInvertedText},
		{Column: lanceVectorColumn, Kind: lancestore.IndexVectorIVFPQ},
		// etype has few distinct values across many rows, which is the bitmap's case.
		{Column: "etype", Kind: lancestore.IndexScalarBitmap},
		{Column: "path", Kind: lancestore.IndexScalarBTree},
	}
}

func lanceFileIndexes() []lancestore.Index {
	return []lancestore.Index{
		{Column: lanceBodyColumn, Kind: lancestore.IndexInvertedText},
		{Column: "path", Kind: lancestore.IndexScalarBTree},
	}
}

// ---------- the document ----------

// entityBody renders an entity as the one document the full-text index reads.
//
// The composition is the one measured in lancestore's tuning sweep — identifier, split form,
// lowercased variants, docstring, type, and a 2+3-gram bag — and the grams are the one place Go
// does work the engine could: the sweep measured the engine's own n-gram tokenizer against this
// and this won. See lancestore.TestSearchTuningSweep for the table.
//
// The path is deliberately LAST and unexpanded: it is the weakest evidence of relevance, and
// expanding it into grams floods the document with directory names that match everything.
func entityBody(e cachedEntity) string {
	split := splitCodeIdentifier(e.Name)
	parts := []string{e.Name}
	if split != e.Name {
		parts = append(parts, split)
	}
	// The lowercased copies are part of the composition the tuning sweep MEASURED, so they stay
	// even though the engine's default tokenizer lowercases at index time and they are therefore
	// probably redundant. Dropping them is a change to a measured configuration, which means
	// re-running the sweep — not reasoning about it here. Recorded in the backlog.
	if lower := strings.ToLower(e.Name); lower != e.Name {
		parts = append(parts, lower)
	}
	if lowerSplit := strings.ToLower(split); lowerSplit != split && lowerSplit != e.Name {
		parts = append(parts, lowerSplit)
	}
	if e.Label != "" {
		parts = append(parts, e.Label)
	}
	if e.Docstring != "" {
		parts = append(parts, e.Docstring)
	}
	parts = append(parts, gramsFor(e.Name, split), e.Path)
	return strings.Join(nonEmpty(parts), " ")
}

// fileBody is the same idea for a file: its name carries the signal, its text is corroboration.
func fileBody(relPath, source string) string {
	base := filepath.Base(relPath)
	split := splitCodeIdentifier(base)
	parts := []string{base}
	if split != base {
		parts = append(parts, split)
	}
	parts = append(parts, gramsFor(base, split), relPath, source)
	return strings.Join(nonEmpty(parts), " ")
}

// gramsFor is the 2+3-gram bag of an identifier and of each of its split words.
//
// It exists so a truncated query ("resolv") reaches the identifier it was cut from
// ("resolveHubArtifact"), which BM25 over whole terms cannot do.
func gramsFor(name, split string) string {
	seen := make(map[string]bool)
	var out []string
	add := func(s string) {
		for _, g := range identifierGrams(s) {
			if !seen[g] {
				seen[g] = true
				out = append(out, g)
			}
		}
	}
	add(name)
	for _, w := range strings.Fields(split) {
		add(w)
	}
	return strings.Join(out, " ")
}

// identifierGrams renders an identifier as the bag of its overlapping two- and
// three-character grams.
//
// The bigrams are what let a two-character abbreviation land at all; the sweep measured 2+3 above
// 3 alone and above the engine's 2-4, which over-matched.
func identifierGrams(name string) []string {
	norm := []rune(normalizeForTrigrams(name))
	var out []string
	for _, n := range []int{2, 3} {
		for i := 0; i+n <= len(norm); i++ {
			out = append(out, string(norm[i:i+n]))
		}
	}
	return out
}

// LanceQueryText expands a query the same way the document was expanded.
//
// THE TWO SIDES MUST MATCH. A gram bag in the document that the query never produces is dead
// weight that only dilutes BM25's term statistics, and a query gram the document lacks matches
// nothing. The requirement is not new — the SQLite index had to keep the same pair in step.
func LanceQueryText(query string) string {
	tokens := tokenizeQuery(query)
	seen := make(map[string]bool)
	var out []string
	add := func(s string) {
		if s != "" && !seen[s] {
			seen[s] = true
			out = append(out, s)
		}
	}
	for _, t := range tokens {
		add(strings.ToLower(t))
	}
	for _, t := range tokens {
		for _, g := range identifierGrams(t) {
			add(g)
		}
	}
	return strings.Join(out, " ")
}

func nonEmpty(in []string) []string {
	out := in[:0]
	for _, s := range in {
		if strings.TrimSpace(s) != "" {
			out = append(out, s)
		}
	}
	return out
}

// ---------- the rows ----------

// buildEntityRow is the ONLY place an entity row is composed.
//
// Both write paths go through it on purpose. The SQLite index had two INSERT statements listing
// their columns separately, and the incremental one omitted name_tri — so trigram recall silently
// died for every file an incremental touched. A single constructor makes that unexpressible.
func buildEntityRow(e cachedEntity, emb []float32) lancestore.Row {
	row := lancestore.Row{
		"uid":             e.UID,
		"name":            e.Name,
		"etype":           e.Label,
		"path":            e.Path,
		"line":            int64(e.Line),
		"docstring":       e.Docstring,
		"is_dep":          e.IsDep,
		lanceBodyColumn:   entityBody(e),
		lanceVectorColumn: nil,
	}
	if len(emb) == ai.EmbeddingDimensions {
		row[lanceVectorColumn] = emb
	}
	return row
}

// buildFileRow is the same, for a file.
func buildFileRow(relPath, source string) lancestore.Row {
	return lancestore.Row{
		"path":          relPath,
		"name":          filepath.Base(relPath),
		"source":        source,
		lanceBodyColumn: fileBody(relPath, source),
	}
}

// ---------- rebuild ----------

// lanceMinRowsForVectorIndex is the engine's own floor for training an IVF-PQ index.
//
// MEASURED, not guessed: below it the index build fails with
//
//	Unprocessable: Not enough rows to train PQ. Requires 256 rows but only 2 available
//
// and that failure took the WHOLE rebuild down. Which means a project with fewer than 256 indexed
// entities — a new repository, a small service, most test fixtures — could not build a search
// index at all. Skipping the index below the floor is correct rather than a workaround: a vector
// query over an unindexed column is answered by scanning, which at this size is what an index
// would degrade into anyway.
const lanceMinRowsForVectorIndex = 256

// lanceWriteBatch bounds how many rows are held before they are handed to the engine.
//
// The corpus does not fit: file text alone reached 2.4 GB on a 36k-file export, which is what
// made the SQLite rebuild stream instead of collecting. The same constraint applies here, and a
// batch is the unit that keeps peak memory flat while still giving the engine enough rows per
// call to be worth the crossing into Rust.
const lanceWriteBatch = 2000

// RebuildFromCache replaces the whole index from the parse shards.
//
// The tables are DROPPED and recreated rather than emptied: a rebuild is defined as "the shards
// are the truth", and dropping is the only way to be sure nothing survives from a schema that has
// since changed. Then the rows stream in, and the indexes are built LAST — building them first
// would pay index maintenance on every batch of a bulk load, which is the same reason the SQLite
// version dropped its triggers and rebuilt afterwards.
func (s *SearchIndex) RebuildFromCache(ctx context.Context, cache *ShardCache,
	embLookup func(relPath, uid string) []float32) error {
	if cache == nil {
		return fmt.Errorf("rebuild search index: no parse cache")
	}
	if s.Remote() {
		return lancestore.ErrReadOnly
	}
	t0 := time.Now()

	for _, name := range []string{lanceFilesTable, lanceEntitiesTable} {
		if err := s.store.DropTable(ctx, name); err != nil {
			return fmt.Errorf("clearing %s: %w", name, err)
		}
	}
	files, err := s.store.CreateTable(ctx, lanceFilesTable, lanceFilesSchema())
	if err != nil {
		return err
	}
	entities, err := s.store.CreateTable(ctx, lanceEntitiesTable, lanceEntitiesSchema())
	if err != nil {
		return err
	}
	s.files, s.entities = files, entities

	var (
		fileRows, entRows             []lancestore.Row
		fileCount, entCount, vecCount int
		walkErr                       error
	)
	flushFiles := func() bool {
		if len(fileRows) == 0 {
			return true
		}
		if err := files.Append(ctx, fileRows); err != nil {
			walkErr = fmt.Errorf("writing files: %w", err)
			return false
		}
		fileRows = fileRows[:0]
		return true
	}
	flushEntities := func() bool {
		if len(entRows) == 0 {
			return true
		}
		if err := entities.Append(ctx, entRows); err != nil {
			walkErr = fmt.Errorf("writing entities: %w", err)
			return false
		}
		entRows = entRows[:0]
		return true
	}

	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		fileRows = append(fileRows, buildFileRow(relPath, entry.Source))
		fileCount++
		if len(fileRows) >= lanceWriteBatch && !flushFiles() {
			return false
		}

		for _, e := range entry.Entities {
			var emb []float32
			if embLookup != nil {
				emb = embLookup(relPath, e.UID)
			}
			row := buildEntityRow(e, emb)
			if row[lanceVectorColumn] != nil {
				vecCount++
			}
			entRows = append(entRows, row)
			entCount++
			if len(entRows) >= lanceWriteBatch && !flushEntities() {
				return false
			}
		}
		return true
	})
	if walkErr != nil {
		return walkErr
	}
	if !flushFiles() || !flushEntities() {
		return walkErr
	}

	if err := files.EnsureIndexes(ctx, lanceFileIndexes()...); err != nil {
		return err
	}
	// The vector index needs enough vectors to train on — see lanceMinRowsForVectorIndex. Below
	// the floor it is skipped, and semantic search still answers by scanning.
	idx := lanceEntityIndexes()
	if vecCount < lanceMinRowsForVectorIndex {
		idx = withoutColumn(idx, lanceVectorColumn)
		s.log().Info("vector index not built: too few embeddings to train on",
			"vectors", vecCount, "required", lanceMinRowsForVectorIndex,
			"impact", "semantic search still answers, by scanning instead of by index")
	}
	if err := entities.EnsureIndexes(ctx, idx...); err != nil {
		return err
	}

	s.log().Info("search index rebuild",
		"files", fileCount, "entities", entCount, "vectors", vecCount,
		"duration_ms", time.Since(t0).Seconds()*1000)

	if s.storeDir != "" && entCount > 0 {
		if err := writeEmbedsStatus(s.storeDir, int64(vecCount), int64(entCount)); err != nil {
			return fmt.Errorf("writing embeds status: %w", err)
		}
	}
	s.vectorCount = int64(vecCount)
	return nil
}

func withoutColumn(in []lancestore.Index, column string) []lancestore.Index {
	out := make([]lancestore.Index, 0, len(in))
	for _, i := range in {
		if i.Column != column {
			out = append(out, i)
		}
	}
	return out
}

// ---------- incremental ----------

// UpdateIncremental applies a delta: every affected path is removed and every changed one
// reinserted.
//
// This runs IN PLACE, against the index readers are using. Deleting by path and appending is the
// whole operation — there is no index to maintain per row, because the engine keeps appended rows
// findable by scanning the fragments it has not folded in yet.
//
// The fold at the end is for LATENCY, not correctness: without it those fragments accumulate and
// the scanned part of every later query grows. Measured in
// lancestore.TestFoldIsAboutLatencyNotVisibility — the intuition that an unfolded row is
// invisible is wrong, and building the delete-then-read ordering around it would have been.
func (s *SearchIndex) UpdateIncremental(ctx context.Context, cache *ShardCache,
	changedFiles, deletedFiles []string, embLookup func(relPath, uid string) []float32) error {
	if s.Remote() {
		return lancestore.ErrReadOnly
	}
	affected := make([]string, 0, len(changedFiles)+len(deletedFiles))
	affected = append(affected, changedFiles...)
	affected = append(affected, deletedFiles...)
	if len(affected) == 0 {
		return nil
	}
	if err := s.ensureTables(ctx); err != nil {
		return err
	}
	t0 := time.Now()

	// Deleted first, and for BOTH lists: a changed file's old rows have to go before its new ones
	// arrive, or the index answers with two versions of the same entity.
	if err := s.entities.DeleteByKey(ctx, "path", affected); err != nil {
		return fmt.Errorf("removing entities for %d paths: %w", len(affected), err)
	}
	if err := s.files.DeleteByKey(ctx, "path", affected); err != nil {
		return fmt.Errorf("removing files for %d paths: %w", len(affected), err)
	}

	var fileRows, entRows []lancestore.Row
	var vecCount int
	for _, p := range changedFiles {
		entry := cache.GetEntry(p)
		if entry == nil {
			continue
		}
		fileRows = append(fileRows, buildFileRow(p, entry.Source))
		for _, e := range entry.Entities {
			var emb []float32
			if embLookup != nil {
				emb = embLookup(p, e.UID)
			}
			row := buildEntityRow(e, emb)
			if row[lanceVectorColumn] != nil {
				vecCount++
			}
			entRows = append(entRows, row)
		}
	}

	if len(fileRows) > 0 {
		if err := s.files.Append(ctx, fileRows); err != nil {
			return fmt.Errorf("writing %d files: %w", len(fileRows), err)
		}
	}
	if len(entRows) > 0 {
		if err := s.entities.Append(ctx, entRows); err != nil {
			return fmt.Errorf("writing %d entities: %w", len(entRows), err)
		}
	}

	// Best-effort: a fold that fails leaves the index CORRECT and merely slower, so it must not
	// fail the reindex that just succeeded.
	for _, t := range []*lancestore.Table{s.files, s.entities} {
		if err := t.FoldNewRowsIntoIndexes(ctx); err != nil {
			s.log().Warn("could not fold new rows into the search index",
				"table", t.Name(), "error", err,
				"impact", "queries stay correct and get slower until the next rebuild")
		}
	}

	s.log().Info("search index incremental",
		"changed", len(changedFiles), "deleted", len(deletedFiles),
		"entities", len(entRows), "vectors", vecCount,
		"duration_ms", time.Since(t0).Seconds()*1000)

	if s.storeDir != "" {
		prev := readEmbedsStatus(s.storeDir)
		vectors, entities := prev.Vectors, prev.Entities
		switch {
		case int64(vecCount) > 0:
			// The delta carried vectors, so the population is now definitely
			// non-empty; keep the larger count as an estimate.
			if int64(vecCount) > vectors {
				vectors = int64(vecCount)
			}
			if int64(len(entRows)) > entities {
				entities = int64(len(entRows))
			}
		case len(deletedFiles) == 0:
			// Nothing changed in the population: the new rows carry no vectors and
			// nothing was removed, so the previous status still holds.
		default:
			// Deletes with no new vectors are the only case that can flip the
			// binary question, so answer it with one cheap probe instead of a scan.
			if !hasVectorRows(ctx, s.entities) {
				vectors = 0
			}
		}
		if err := writeEmbedsStatus(s.storeDir, vectors, entities); err != nil {
			return fmt.Errorf("writing embeds status: %w", err)
		}
		s.vectorCount = vectors
	}
	return nil
}

// hasVectorRows reports whether any entity row carries an embedding, answering with
// a filtered probe: WHERE embedding IS NOT NULL LIMIT 1 over a NULL-bearing column
// is a scan the engine can abort at the first hit and which the null bitmap makes
// cheap on the non-matching side.
func hasVectorRows(ctx context.Context, entities *lancestore.Table) bool {
	hits, err := entities.Search(ctx, lancestore.Query{
		Filter: "embedding IS NOT NULL", Limit: 1,
	})
	if err != nil || len(hits) == 0 {
		return false
	}
	_, ok := hits[0].Row[lanceVectorColumn]
	return ok
}

// ensureTables opens the two tables, creating them if this is a first write.
func (s *SearchIndex) ensureTables(ctx context.Context) error {
	if s.files == nil {
		t, err := s.store.EnsureTable(ctx, lanceFilesTable, lanceFilesSchema())
		if err != nil {
			return err
		}
		s.files = t
	}
	if s.entities == nil {
		t, err := s.store.EnsureTable(ctx, lanceEntitiesTable, lanceEntitiesSchema())
		if err != nil {
			return err
		}
		s.entities = t
	}
	return nil
}

// ---------- search ----------

// Search runs the keyword half over both tables.
//
// Entities and files are searched SEPARATELY and concatenated with entities first, which is the
// behaviour the SQLite index had and the reason it had two tables: a file whose name happens to
// share a word with the query would otherwise outrank the function the user was looking for, and
// there is no single relevance scale on which "this file" and "this function" compare.
func (s *SearchIndex) Search(ctx context.Context, query string, topK int) ([]SearchResult, error) {
	return s.search(ctx, lancestore.Query{
		Text: LanceQueryText(query), TextColumn: lanceBodyColumn, Limit: topK,
	}, topK)
}

// SemanticSearch runs the vector half. Entities only: a file has no embedding of its own.
func (s *SearchIndex) SemanticSearch(ctx context.Context, vec []float32, topK int) ([]SearchResult, error) {
	if s.vectorCount == 0 {
		s.log().Info("semantic search returned nothing: the index holds no embedding rows",
			"hint", "run `graphit ast embed` to enable the semantic channel")
		return nil, nil
	}
	if err := s.ensureTables(ctx); err != nil {
		return nil, err
	}
	hits, err := s.entities.Search(ctx, lancestore.Query{
		Vector: vec, VectorColumn: lanceVectorColumn, Limit: topK,
	})
	if err != nil {
		return nil, err
	}
	out := entityHitsToResults(hits, "semantic")

	// The engine gives this query a DISTANCE and no score, so the relevance field arrives empty
	// and has to be derived before anything reads it — including the confidence floor below,
	// which is expressed as a cosine. See cosineFromSquaredL2 for the measurement and for what
	// this cost while it was missing.
	for i := range out {
		out[i].RelevanceScore = cosineFromSquaredL2(out[i].Distance)
	}
	return confidentSemanticResults(out), nil
}

// HybridSearch runs both channels and lets the ENGINE fuse them.
//
// With no query vector it degrades to the keyword half rather than failing: a project whose
// embeddings have not been generated yet still has to be searchable.
func (s *SearchIndex) HybridSearch(ctx context.Context, query string, vec []float32, topK int) ([]SearchResult, error) {
	if len(vec) == 0 || s.vectorCount == 0 {
		if len(vec) > 0 && s.vectorCount == 0 {
			s.log().Info("hybrid search degraded to keywords: the index holds no embedding rows",
				"hint", "run `graphit ast embed` to enable the semantic channel")
		}
		return s.Search(ctx, query, topK)
	}
	return s.search(ctx, lancestore.Query{
		Text: LanceQueryText(query), TextColumn: lanceBodyColumn,
		Vector: vec, VectorColumn: lanceVectorColumn, Limit: topK,
	}, topK)
}

// search queries entities, then files, and puts entities first.
//
// THE TWO PASSES ARE ORDERED SEPARATELY AND CONCATENATED — never sorted together. Sorting the
// concatenation is what this function used to do, and it is a defect measured on the real index of
// this project (61,446 entities, 770 files):
//
//	query "evictOldestStaged"
//	  entity pass, keyword only : 156.4  (the method)
//	  file pass,   keyword only :  29.6  (the file declaring it)
//	  entity pass, WITH a vector:   1.0  (the engine's FUSED score, normalised to [0,1])
//
// So raw BM25 was never the problem — on that channel the entity wins by 5x, which is why every
// keyword-mode gate passed while the CLI returned nothing but files. The problem is that a hybrid
// entity pass returns a FUSED score in [0,1] while the file pass, which drops the vector channel
// because the files table has no embedding column, returns raw BM25 in the tens. One sort over
// both puts every file above every entity by a factor of twenty to eighty.
//
// Normalising the two into one scale is not an option: it would be exactly the weighted fusion in
// Go that this project deleted along with the SQLite index. And it is not needed — a file match
// and an entity match are different answers, so precedence between them is the honest rule.
//
// The reranker is a THIRD scale, which is why it is propagated rather than dropped: a
// cross-encoder judges the (query, candidate) pair, so it produces scores that ARE comparable
// across the two lists — but only if both lists reach it. Passing it to one pass only would rank
// half the answer with a model and half with BM25, which is this same defect wearing a different
// number.
//
// Each list IS sorted by score internally, and that is safe because the score is now the right
// column. It was not: a hybrid row carries both _score and _relevance_score, lancestore collapsed
// them into one field from inside a map range, and the winner was whichever Go visited last — so
// the entity a query named by name landed anywhere in the list. See the comment on the hit
// assembly in lancestore/store_lancedb.go. With the columns separated, the fused score is monotone
// with the engine's own order, so sorting by it agrees with the engine and adds a deterministic
// tie-break the engine does not give.
func (s *SearchIndex) search(ctx context.Context, q lancestore.Query, topK int) ([]SearchResult, error) {
	if err := s.ensureTables(ctx); err != nil {
		return nil, err
	}
	entHits, err := s.entities.Search(ctx, q)
	if err != nil {
		return nil, err
	}
	out := entityHitsToResults(entHits, q.Mode())

	sortResultsDeterministic(out)

	if len(out) < topK || topK <= 0 {
		fq := lancestore.Query{
			Text: q.Text, TextColumn: lanceBodyColumn, Limit: topK,
			Filter: q.Filter, Rerank: q.Rerank,
		}
		if fq.Text != "" {
			fileHits, ferr := s.files.Search(ctx, fq)
			if ferr == nil {
				files := fileHitsToResults(fileHits)
				sortResultsDeterministic(files)
				out = append(out, files...)
			}
		}
	}

	if topK > 0 && len(out) > topK {
		out = out[:topK]
	}
	return out, nil
}

func entityHitsToResults(hits []lancestore.Hit, mode string) []SearchResult {
	out := make([]SearchResult, 0, len(hits))
	for _, h := range hits {
		name, _ := h.Row["name"].(string)
		etype, _ := h.Row["etype"].(string)
		path, _ := h.Row["path"].(string)
		doc, _ := h.Row["docstring"].(string)
		isDep, _ := h.Row["is_dep"].(bool)
		var line int
		switch n := h.Row["line"].(type) {
		case int64:
			line = int(n)
		case float64:
			line = int(n)
		}
		out = append(out, SearchResult{
			Type: etype, Name: name, Path: path, Line: line,
			Docstring: doc, IsDepend: isDep,
			SearchType: mode, RelevanceScore: h.Score, Distance: h.Distance,
		})
	}
	return out
}

func fileHitsToResults(hits []lancestore.Hit) []SearchResult {
	out := make([]SearchResult, 0, len(hits))
	for _, h := range hits {
		path, _ := h.Row["path"].(string)
		name, _ := h.Row["name"].(string)
		src, _ := h.Row["source"].(string)
		out = append(out, SearchResult{
			Type: LabelFile, Name: name, Path: path, Source: src,
			SearchType: "fts", RelevanceScore: h.Score,
		})
	}
	return out
}

// Counts reports how many entities and how many of them carry an embedding.
//
// Exposed because a caller has to be able to verify that embeddings reached the store, and the old
// way of doing it — counting rows in two tables with raw SQL — is gone with the tables.
func (s *SearchIndex) Counts(ctx context.Context) (entities, withVector int64, err error) {
	if err = s.ensureTables(ctx); err != nil {
		return 0, 0, err
	}
	if entities, err = s.entities.Count(ctx); err != nil {
		return 0, 0, err
	}
	hits, err := s.entities.Search(ctx, lancestore.Query{
		Filter: lanceVectorColumn + " IS NOT NULL", Limit: 1_000_000,
	})
	if err != nil {
		return entities, 0, err
	}
	return entities, int64(len(hits)), nil
}

// CountForPath reports how many entities and files the index holds for one path, which is what a
// reindex test needs to prove a delete actually took.
func (s *SearchIndex) CountForPath(ctx context.Context, relPath string) (entities, files int) {
	if err := s.ensureTables(ctx); err != nil {
		return 0, 0
	}
	filter := fmt.Sprintf("path = %s", astQuote(relPath))
	if hits, err := s.entities.Search(ctx, lancestore.Query{Filter: filter, Limit: 100000}); err == nil {
		entities = len(hits)
	}
	if hits, err := s.files.Search(ctx, lancestore.Query{Filter: filter, Limit: 10}); err == nil {
		files = len(hits)
	}
	return entities, files
}

// ---------- file text ----------

// The index is the ONLY queryable copy of a file's text.
//
// The graph's File table stopped carrying source, because holding it in both places put the same
// bytes on disk twice and made the graph the larger of the two. So these are not conveniences:
// `ast source` reads through them, and an index that failed to build takes source reads down with
// it — which is why the rebuild path returns that error instead of logging it.

// SearchIndexBuilt reports whether an index exists and has files in it.
//
// Populated, not merely present: opening creates, so a store whose build died after the graph has
// an index that exists and answers nothing.
func SearchIndexBuilt(ctx context.Context, dbPath string) bool {
	if dbPath == "" {
		return false
	}
	if info, err := os.Stat(LanceIndexPath(dbPath)); err != nil || !info.IsDir() {
		return false
	}
	idx, err := OpenSearchIndex(ctx, dbPath)
	if err != nil {
		return false
	}
	defer func() { _ = idx.Close() }()
	if err := idx.ensureTables(ctx); err != nil {
		return false
	}
	n, err := idx.files.Count(ctx)
	return err == nil && n > 0
}

// FileSourceAt reads one file's text out of the index.
func FileSourceAt(ctx context.Context, dbPath, relPath string) (string, bool) {
	if dbPath == "" || relPath == "" {
		return "", false
	}
	if info, err := os.Stat(LanceIndexPath(dbPath)); err != nil || !info.IsDir() {
		return "", false
	}
	idx, err := OpenSearchIndex(ctx, dbPath)
	if err != nil {
		return "", false
	}
	defer func() { _ = idx.Close() }()
	return idx.FileSource(ctx, relPath)
}

// FileSource reads one file's text from an already-open index.
func (s *SearchIndex) FileSource(ctx context.Context, relPath string) (string, bool) {
	if err := s.ensureTables(ctx); err != nil {
		return "", false
	}
	hits, err := s.files.Search(ctx, lancestore.Query{
		Filter: fmt.Sprintf("path = %s", astQuote(relPath)), Limit: 1,
	})
	if err != nil || len(hits) == 0 {
		return "", false
	}
	src, _ := hits[0].Row["source"].(string)
	return src, src != ""
}

// eachFileSourceBatch bounds how many rows are held at once while walking every file's text.
//
// The whole corpus is on the other side of this walk — 2.4 GB on a 36k-file export — and a caller
// writing into an archive needs one file at a time, not all of them in memory. The SQLite version
// got this from a streaming cursor; here it is paged explicitly, because the engine returns a
// materialised batch and asking for the whole table would be the 2.4 GB.
const eachFileSourceBatch = 200

// EachFileSource calls fn for every indexed file that has text, one at a time and in path order.
//
// fn returning an error stops the walk.
//
// A MISSING INDEX IS AN ERROR, NOT AN EMPTY WALK, and that distinction has to be made here rather
// than left to the open. OpenSearchIndex CREATES what it opens, so without this check a store with
// no index walks cleanly over zero files — and a caller writing an artifact would publish it as
// complete. The SQLite version got this for free from a read-only open that failed on a missing
// file; the directory-based store has to be asked.
func EachFileSource(ctx context.Context, dbPath string, fn func(relPath, source string) error) error {
	if dbPath == "" {
		return fmt.Errorf("no search index path")
	}
	idxPath := LanceIndexPath(dbPath)
	if info, err := os.Stat(idxPath); err != nil || !info.IsDir() {
		return fmt.Errorf("no search index at %s: the store has no indexed file text", idxPath)
	}
	idx, err := OpenSearchIndex(ctx, dbPath)
	if err != nil {
		return err
	}
	defer func() { _ = idx.Close() }()
	return idx.EachFileSource(ctx, fn)
}

// EachFileSource is the walk on an already-open index.
//
// Paged by PATH rather than by offset: the engine's filter has no stable row ordering to offset
// into, so the cursor is the last path seen. That also makes the walk resumable and immune to a
// concurrent write shifting rows under it, which an offset would not be.
func (s *SearchIndex) EachFileSource(ctx context.Context, fn func(relPath, source string) error) error {
	if err := s.ensureTables(ctx); err != nil {
		return err
	}
	// One pass to collect the paths, then the text one page at a time. The paths of even a large
	// corpus are a few megabytes; the text is gigabytes, and that is the whole distinction.
	pathHits, err := s.files.Search(ctx, lancestore.Query{
		Filter: "path IS NOT NULL", Limit: 1_000_000,
	})
	if err != nil {
		return fmt.Errorf("read file paths: %w", err)
	}
	paths := make([]string, 0, len(pathHits))
	for _, h := range pathHits {
		if p, _ := h.Row["path"].(string); p != "" {
			paths = append(paths, p)
		}
	}
	sort.Strings(paths)

	for start := 0; start < len(paths); start += eachFileSourceBatch {
		end := start + eachFileSourceBatch
		if end > len(paths) {
			end = len(paths)
		}
		page := paths[start:end]

		quoted := make([]string, 0, len(page))
		for _, p := range page {
			quoted = append(quoted, astQuote(p))
		}
		hits, err := s.files.Search(ctx, lancestore.Query{
			Filter: fmt.Sprintf("path IN (%s)", strings.Join(quoted, ", ")),
			Limit:  len(page),
		})
		if err != nil {
			return fmt.Errorf("read file sources: %w", err)
		}
		byPath := make(map[string]string, len(hits))
		for _, h := range hits {
			p, _ := h.Row["path"].(string)
			src, _ := h.Row["source"].(string)
			byPath[p] = src
		}
		// Emitted in path order, not in whatever order the engine returned the page.
		for _, p := range page {
			src := byPath[p]
			if src == "" {
				continue
			}
			if err := fn(p, src); err != nil {
				return err
			}
		}
	}
	return nil
}

// astQuote renders a string literal for the engine's filter dialect.
//
// SAFETY: single quotes, doubled to escape. A path can contain an apostrophe, and an unescaped one
// does not fail the query — it changes which rows match.
func astQuote(s string) string {
	return "'" + strings.ReplaceAll(s, "'", "''") + "'"
}

// BuildSearchIndexFor builds a store's search half from the shards it was parsed from.
//
// Used when installing a context whose artifact carried no search index: the shards live in the
// Hub download rather than beside the store, so nothing else can serve them and the index has to
// be built before they go away. An installed context without it can be traversed but neither
// searched nor read.
func BuildSearchIndexFor(ctx context.Context, dbPath string, cache *ShardCache, embCache *ShardEmbCache) error {
	if cache == nil {
		return fmt.Errorf("build search index: no parse cache")
	}
	idx, err := OpenSearchIndex(ctx, dbPath)
	if err != nil {
		return fmt.Errorf("open search index: %w", err)
	}
	defer func() { _ = idx.Close() }()
	return idx.RebuildFromCache(ctx, cache, BuildEmbLookup(cache, embCache))
}

// ---------- a mounted store's search index ----------

// SearchMountFile records that this store's search index lives on object storage.
//
// SAME SHAPE AS THE GRAPH, deliberately: mounting a graph writes a local catalog whose tables name
// a remote location, and this is the search half of exactly that idea — a few bytes of local
// metadata pointing at data nobody downloaded. Without it a mounted context would open a local
// index directory that does not exist and answer every search with nothing, which is
// indistinguishable from a corpus that genuinely has no match.
const SearchMountFile = "search.uri"

// WriteSearchMount records where a mounted store reads its search index from.
//
// ONLY THE URI IS STORED. Region and endpoint are resolved from the ambient configuration when the
// index is opened, and that is not an oversight: writing them here would freeze them, so pointing
// the framework at a different endpoint would leave every installed context reaching for the old
// one. The bucket is part of the location and so belongs in the URI; the rest is how you connect.
func WriteSearchMount(storeDir, uri string) error {
	if strings.TrimSpace(uri) == "" {
		return fmt.Errorf("recording the search mount: no URI")
	}
	if err := os.MkdirAll(storeDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(storeDir, SearchMountFile), []byte(uri+"\n"), 0o644)
}

// searchMountURI reads the recorded URI, or "" when this store's index is local.
func searchMountURI(storeDir string) string {
	data, err := os.ReadFile(filepath.Join(storeDir, SearchMountFile))
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(data))
}

// searchConfigFor decides where a store's search index is, local or mounted.
func searchConfigFor(storeDir string) lancestore.Config {
	if uri := searchMountURI(storeDir); uri != "" {
		return lancestore.Config{URI: uri, S3: config.HubS3Config()}
	}
	return lancestore.Config{URI: LanceIndexPath(storeDir)}
}
