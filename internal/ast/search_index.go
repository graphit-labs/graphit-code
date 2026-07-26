package ast

import (
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	lbug "github.com/LadybugDB/go-ladybug"
	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// Search index backed by LadybugDB, replacing the SQLite (FTS5 + sqlite-vec) index.
//
// The design is the one measured before it was written, rather than assumed:
//
//   - Per-field FTS indexes instead of FTS5's bm25(...) column weights, which have no
//     equivalent; the weights are applied by the caller when fusing passes
//     (TestLadybugFTSFeatureParity).
//   - Identifier splitting at write time ("parseConfig" -> "parseConfig parse Config"),
//     which is mandatory, not an optimisation: without it "config" does not reach
//     parseConfig at all (TestCamelCaseSearchStrategies, 0/6 -> 6/6).
//   - A precomputed trigram bag as a low-weight recall pass. Not optional either:
//     spelled-out queries must reach abbreviated identifiers ("config" -> coreConf,
//     CONF_MGR), which exact-token FTS cannot do in either direction
//     (TestAbbreviatedIdentifierSearch, 3/9 -> 8/9).
//   - A CONTAINS fallback for queries shorter than a trigram. This is the ONLY
//     capability FTS5's prefix index provided that has no indexed equivalent here —
//     there is no wildcard operator ('conf*' matches nothing). Measured as the single
//     divergence across eleven truncation probes, and closed by the fallback
//     (TestPrefixIndexGap, 11/11).
//   - One table for entities, vectors included, rather than SQLite's separate
//     entity_vec_map. Rows with a NULL vector are accepted and skipped by
//     QUERY_VECTOR_INDEX, and the index can be created on an empty table
//     (TestLadybugVectorSchemaConstraints).
//
// The vector half is the reason to consolidate at all. sqlite-vec's vec0 allocates
// fixed 1024-row chunks and never reclaims space on delete, which is what forces the
// whole-file rebuild in the SQLite implementation. Ladybug reflects both DELETE and
// INSERT without a rebuild (TestLadybugVectorIndex), so incremental updates mutate
// in place.

// searchIndexSuffix names the search database, a sibling of the graph store. Exported
// through SearchIndexSuffix for callers outside the package.
//
// CleanupInterruptedSwap deletes stray "<dbPath>.*" siblings, so anything matching this
// prefix must stay on its skip list — the search index is not swap debris.
const searchIndexSuffix = ".search"

// SearchIndexSuffix is searchIndexSuffix for callers outside this package.
const SearchIndexSuffix = searchIndexSuffix

const ladybugSearchSchemaVersion = 1

// Pass weights. They mirror the intent of the SQLite index's bm25(0,10,3,2,1) for
// entities and bm25(2,8,1) for files: the name dominates, prose supports it, metadata
// is a tiebreak, and the trigram bag is a recall pass kept deliberately low so its
// looser matching lands beneath exact hits.
const (
	weightEntityName     = 3.0
	weightEntityDoc      = 1.5
	weightEntityMeta     = 0.8
	weightEntityTrigram  = 0.7
	weightFileName       = 2.5
	weightFileSource     = 1.0
	weightFileTrigram    = 0.6
	weightSubTrigramScan = 0.5
	weightSemantic       = 2.0
)

// Batch sizes for bulk insert. Files carry whole file sources, so their batches stay
// small to bound peak memory per statement; entity rows are small.
const (
	fileInsertBatch   = 16
	entityInsertBatch = 512
)

type SearchIndex struct {
	path string
	db   *lbug.Database
	conn *lbug.Connection

	mu     sync.RWMutex
	Logger *slog.Logger

	hasVector bool
}

func (s *SearchIndex) log() *slog.Logger { return slogutil.Resolve(s.Logger) }

// OpenSearchIndex opens (creating if needed) the search database at dbPath.
func OpenSearchIndex(dbPath string) (*SearchIndex, error) {
	if err := os.MkdirAll(filepath.Dir(dbPath), 0o755); err != nil {
		return nil, fmt.Errorf("search index dir: %w", err)
	}

	s := &SearchIndex{path: dbPath}
	if err := s.open(dbPath, true); err != nil {
		return nil, err
	}
	return s, nil
}

// open connects and ensures the tables exist. withIndexes also creates the FTS and
// vector indexes, which a fresh rebuild defers until after its bulk load — see
// createIndexes for why.
func (s *SearchIndex) open(dbPath string, withIndexes bool) error {
	sysCfg := lbug.DefaultSystemConfig()
	// Same reasoning as the graph store: liblbug's default buffer pool is ~80% of
	// physical RAM, ignoring any cgroup limit, and this database is open alongside
	// the graph. The pool is a lazily-grown ceiling, so bounding it is close to free.
	sysCfg.BufferPoolSize = boundedDBBufferPool(sysCfg.BufferPoolSize)
	sysCfg.MaxNumThreads = boundedDBThreads(sysCfg.MaxNumThreads)

	// Share the handle per path so an in-process reader gets snapshot isolation
	// against the indexer writing (see ladybug_registry.go).
	db, err := acquireDatabase(dbPath, sysCfg)
	if err != nil {
		return fmt.Errorf("search index open: %w", err)
	}
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		releaseDatabase(dbPath, db)
		return fmt.Errorf("search index connection: %w", err)
	}

	s.db, s.conn = db, conn

	if err := s.initSchema(); err != nil {
		_ = s.closeLocked()
		return err
	}
	if withIndexes {
		if err := s.createIndexes(); err != nil {
			_ = s.closeLocked()
			return err
		}
	}
	return nil
}

func (s *SearchIndex) exec(q string) error {
	res, err := s.conn.Query(q)
	if err != nil {
		return err
	}
	res.Close()
	return nil
}

// execTolerant runs DDL that is expected to fail when the object already exists.
func (s *SearchIndex) execTolerant(q string) error {
	if err := s.exec(q); err != nil {
		if strings.Contains(strings.ToLower(err.Error()), "already exists") {
			return nil
		}
		return fmt.Errorf("%q: %w", truncateForError(q), err)
	}
	return nil
}

func truncateForError(q string) string {
	if len(q) > 60 {
		return q[:60] + "…"
	}
	return q
}

func (s *SearchIndex) initSchema() error {
	if err := s.exec("INSTALL fts"); err != nil {
		// Already installed is fine; a genuine failure surfaces on LOAD.
		s.log().Debug("search index: INSTALL fts", "error", err)
	}
	if err := s.exec("LOAD EXTENSION fts"); err != nil {
		return fmt.Errorf("search index: fts extension unavailable: %w", err)
	}

	// Vector search is optional: full-text still works without it, and degrading is
	// better than refusing to index. HybridSearch reports the reduced capability.
	if err := s.exec("INSTALL vector"); err != nil {
		s.log().Debug("search index: INSTALL vector", "error", err)
	}
	if err := s.exec("LOAD EXTENSION vector"); err != nil {
		s.log().Warn("search index: vector extension unavailable, semantic search disabled", "error", err)
	} else {
		s.hasVector = true
	}

	ddl := []string{
		"CREATE NODE TABLE IF NOT EXISTS SearchMeta(k STRING, v STRING, PRIMARY KEY(k))",
		// name is what a result displays; name_split is what the index matches.
		// Keeping them apart is why results carry a clean identifier — the SQLite
		// index stored the split into the same column and so returned
		// "config.go config go" as the name.
		"CREATE NODE TABLE IF NOT EXISTS SearchFile(" +
			"path STRING, name STRING, name_split STRING, name_tri STRING, source STRING, PRIMARY KEY(path))",
		fmt.Sprintf("CREATE NODE TABLE IF NOT EXISTS SearchEntity("+
			"uid STRING, name STRING, name_split STRING, name_tri STRING, docstring STRING, "+
			"etype STRING, path STRING, line INT64, emb FLOAT[%d], PRIMARY KEY(uid))",
			ai.EmbeddingDimensions),
	}
	for _, q := range ddl {
		if err := s.execTolerant(q); err != nil {
			return fmt.Errorf("search index schema: %w", err)
		}
	}

	_ = s.exec(fmt.Sprintf(
		"MERGE (m:SearchMeta {k: 'schema_version'}) SET m.v = '%d'", ladybugSearchSchemaVersion))

	return nil
}

// ftsIndexes is the set of per-field FTS indexes, which is how per-field weighting is
// reconstructed — FTS5's bm25(...) column weights have no equivalent.
var ftsIndexes = []struct{ table, name, fields string }{
	{"SearchEntity", "se_split", "['name_split']"},
	{"SearchEntity", "se_tri", "['name_tri']"},
	{"SearchEntity", "se_doc", "['docstring']"},
	{"SearchEntity", "se_meta", "['etype', 'path']"},
	{"SearchFile", "sf_name", "['name_split']"},
	{"SearchFile", "sf_tri", "['name_tri']"},
	{"SearchFile", "sf_source", "['source']"},
}

// createIndexes builds the FTS and vector indexes.
//
// Order matters and was established by measurement, not preference: an FTS index that
// exists during a bulk load ends up populated only intermittently — a rebuild would
// answer every lexical query correctly on one run and return nothing on the next, while
// the rows themselves were always present and a single-row write into a live index always
// registered. liblbug's own diagnostic points the same way ("FTS index ... is
// inconsistent ... Drop and recreate the FTS index"). Building the indexes after the data
// is deterministic (TestSearchIndexRebuildIsDeterministic).
//
// The vector index does not share the problem — it reflects inserts and deletes in place
// (TestLadybugVectorIndex) — but it is created here too so a rebuild has one ordering.
func (s *SearchIndex) createIndexes() error {
	if err := s.createFTSIndexes(); err != nil {
		return err
	}

	if s.hasVector {
		if err := s.execTolerant(
			"CALL CREATE_VECTOR_INDEX('SearchEntity', 'se_vec', 'emb')"); err != nil {
			// Not fatal: lexical search remains usable.
			s.log().Warn("search index: vector index unavailable, semantic search disabled", "error", err)
			s.hasVector = false
		}
	}
	return nil
}

// dropFTSIndexes removes the FTS indexes. A missing index is not an error: this runs before
// writes that may precede any creation.
func (s *SearchIndex) dropFTSIndexes() {
	for _, idx := range ftsIndexes {
		if err := s.exec(fmt.Sprintf("CALL DROP_FTS_INDEX('%s', '%s')", idx.table, idx.name)); err != nil {
			s.log().Debug("search index: drop fts index", "index", idx.name, "error", err)
		}
	}
}

// createFTSIndexes builds the FTS indexes over whatever rows currently exist.
func (s *SearchIndex) createFTSIndexes() error {
	for _, idx := range ftsIndexes {
		q := fmt.Sprintf("CALL CREATE_FTS_INDEX('%s', '%s', %s)", idx.table, idx.name, idx.fields)
		if err := s.execTolerant(q); err != nil {
			return fmt.Errorf("search index fts: %w", err)
		}
	}
	return nil
}

func (s *SearchIndex) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.closeLocked()
}

func (s *SearchIndex) closeLocked() error {
	if s.conn != nil {
		s.conn.Close()
		s.conn = nil
	}
	if s.db != nil {
		releaseDatabase(s.path, s.db)
		s.db = nil
	}
	return nil
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

// searchRows is the flattened form of a cache entry, ready for bulk insert.
type searchRows struct {
	files           []map[string]any
	entities        []map[string]any
	entitiesWithEmb []map[string]any
}

// rowsForEntry flattens one parsed file. Entities carrying a vector go into a
// separate slice: UNWIND infers one struct type per batch, so a batch cannot mix
// rows that have the property with rows that do not.
func rowsForEntry(relPath string, entry *parseCacheEntry,
	embLookup func(relPath, uid string) []float32, seenUID map[string]bool) searchRows {

	var out searchRows

	baseName := filepath.Base(relPath)
	out.files = append(out.files, map[string]any{
		"path":       relPath,
		"name":       baseName,
		"name_split": baseName + " " + splitCodeIdentifier(baseName),
		"name_tri":   identifierTrigrams(baseName),
		"source":     entry.Source,
	})

	for _, e := range entry.Entities {
		// uid is the primary key here, unlike the SQLite index which had no key and
		// silently kept duplicates. Dropping the later duplicate preserves
		// first-wins, which is the semantics the embedder's UID map also relies on.
		if seenUID != nil {
			if seenUID[e.UID] {
				continue
			}
			seenUID[e.UID] = true
		}

		row := map[string]any{
			"uid":        e.UID,
			"name":       e.Name,
			"name_split": e.Name + " " + splitCodeIdentifier(e.Name),
			"name_tri":   identifierTrigrams(e.Name),
			"docstring":  e.Docstring,
			"etype":      e.Label,
			"path":       e.Path,
			"line":       int64(e.Line),
		}

		if embLookup != nil {
			if vec := embLookup(relPath, e.UID); len(vec) == ai.EmbeddingDimensions {
				row["emb"] = vec
				out.entitiesWithEmb = append(out.entitiesWithEmb, row)
				continue
			}
		}
		out.entities = append(out.entities, row)
	}

	return out
}

const (
	insertFileCypher = "UNWIND $batch AS row CREATE (:SearchFile {" +
		"path: row.path, name: row.name, name_split: row.name_split, " +
		"name_tri: row.name_tri, source: row.source})"

	insertEntityCypher = "UNWIND $batch AS row CREATE (:SearchEntity {" +
		"uid: row.uid, name: row.name, name_split: row.name_split, name_tri: row.name_tri, " +
		"docstring: row.docstring, etype: row.etype, path: row.path, line: row.line})"

	insertEntityVecCypher = "UNWIND $batch AS row CREATE (:SearchEntity {" +
		"uid: row.uid, name: row.name, name_split: row.name_split, name_tri: row.name_tri, " +
		"docstring: row.docstring, etype: row.etype, path: row.path, line: row.line, emb: row.emb})"
)

// flushBatch inserts rows and returns the number of rows it failed to write.
// Errors are returned rather than logged-and-ignored: a partially written search
// index is a silently degraded one, which is the failure mode that let a partial
// graph be published before.
func (s *SearchIndex) flushBatch(cypher string, rows []map[string]any) error {
	if len(rows) == 0 {
		return nil
	}
	stmt, err := s.conn.Prepare(cypher)
	if err != nil {
		return fmt.Errorf("prepare %q: %w", truncateForError(cypher), err)
	}
	defer stmt.Close()

	res, err := s.conn.Execute(stmt, map[string]any{"batch": rows})
	if err != nil {
		return fmt.Errorf("insert %d rows: %w", len(rows), err)
	}
	res.Close()
	return nil
}

// RebuildFromCache rebuilds the whole index from the parse cache.
//
// Built into a sibling path and renamed into place, mirroring the SQLite
// implementation: readers either see the previous database or the new one, never a
// half-populated index, and a crash mid-build leaves the old one untouched.
func (s *SearchIndex) RebuildFromCache(cache *ShardCache,
	embLookup func(relPath, uid string) []float32) error {

	s.mu.Lock()
	defer s.mu.Unlock()

	t0 := time.Now()

	tmpPath := s.path + ".new"
	cleanup := func() {
		_ = os.RemoveAll(tmpPath)
		_ = os.RemoveAll(tmpPath + ".wal")
	}
	cleanup()

	// Tables only: the FTS indexes are created after the bulk load, deliberately.
	tmp := &SearchIndex{path: tmpPath, Logger: s.Logger}
	if err := tmp.open(tmpPath, false); err != nil {
		cleanup()
		return fmt.Errorf("search index (temp): %w", err)
	}

	var fileCount, entityCount, vecCount int
	var pending searchRows
	seenUID := make(map[string]bool)
	var streamErr error

	flush := func(force bool) bool {
		if force || len(pending.files) >= fileInsertBatch {
			if err := tmp.flushBatch(insertFileCypher, pending.files); err != nil {
				streamErr = err
				return false
			}
			fileCount += len(pending.files)
			pending.files = nil
		}
		if force || len(pending.entities) >= entityInsertBatch {
			if err := tmp.flushBatch(insertEntityCypher, pending.entities); err != nil {
				streamErr = err
				return false
			}
			entityCount += len(pending.entities)
			pending.entities = nil
		}
		if force || len(pending.entitiesWithEmb) >= entityInsertBatch {
			if err := tmp.flushBatch(insertEntityVecCypher, pending.entitiesWithEmb); err != nil {
				streamErr = err
				return false
			}
			entityCount += len(pending.entitiesWithEmb)
			vecCount += len(pending.entitiesWithEmb)
			pending.entitiesWithEmb = nil
		}
		return true
	}

	// StreamEntries rather than AllEntries: the whole point is to keep peak memory
	// proportional to a batch, not to the corpus.
	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		if entry == nil {
			return true
		}
		r := rowsForEntry(relPath, entry, embLookup, seenUID)
		pending.files = append(pending.files, r.files...)
		pending.entities = append(pending.entities, r.entities...)
		pending.entitiesWithEmb = append(pending.entitiesWithEmb, r.entitiesWithEmb...)
		return flush(false)
	})

	if streamErr == nil {
		flush(true)
	}
	if streamErr != nil {
		_ = tmp.Close()
		cleanup()
		return fmt.Errorf("search index rebuild: %w", streamErr)
	}

	if err := tmp.createIndexes(); err != nil {
		_ = tmp.Close()
		cleanup()
		return fmt.Errorf("search index rebuild: %w", err)
	}

	if err := tmp.exec("CHECKPOINT"); err != nil {
		s.log().Debug("search index checkpoint before swap", "error", err)
	}
	_ = tmp.Close()

	// Replace the live database. Our own handle must go first: it is shared through
	// the registry, so leaving it open would keep serving the old inode.
	_ = s.closeLocked()
	_ = os.RemoveAll(s.path)
	_ = os.RemoveAll(s.path + ".wal")

	if err := os.Rename(tmpPath, s.path); err != nil {
		cleanup()
		// Reopen whatever remains so the index is not left unusable.
		if openErr := s.open(s.path, true); openErr != nil {
			return fmt.Errorf("rename search index: %w (reopen also failed: %v)", err, openErr)
		}
		return fmt.Errorf("rename search index: %w", err)
	}
	_ = os.Rename(tmpPath+".wal", s.path+".wal")

	if err := s.open(s.path, true); err != nil {
		return fmt.Errorf("reopen search index after swap: %w", err)
	}

	s.log().Info("search index rebuild",
		"files", fileCount, "entities", entityCount, "vectors", vecCount,
		"duration_ms", time.Since(t0).Seconds()*1000)
	return nil
}

// UpdateIncremental applies changes for the given files in place.
//
// In place is possible here precisely because Ladybug's FTS and vector indexes
// reflect DELETE and INSERT without a rebuild (TestLadybugVectorIndex). The SQLite
// implementation could not do this for vectors, which is why it rebuilt the file.
func (s *SearchIndex) UpdateIncremental(cache *ShardCache, changedFiles, deletedFiles []string,
	embLookup func(relPath, uid string) []float32) error {

	s.mu.Lock()
	defer s.mu.Unlock()

	if len(changedFiles) == 0 && len(deletedFiles) == 0 {
		return nil
	}

	t0 := time.Now()

	affected := make([]string, 0, len(changedFiles)+len(deletedFiles))
	affected = append(affected, changedFiles...)
	affected = append(affected, deletedFiles...)

	// The FTS indexes come down BEFORE the mutation and go back up after it.
	//
	// They have to be rebuilt anyway — rows written while an index exists are only
	// intermittently indexed (see createIndexes) — so keeping them live through the delete
	// buys nothing and costs correctness: deleting a node the index has no document for
	// fails outright,
	//
	//	FTS index 'sf_source' is inconsistent: document for node offset 3002 is missing
	//	during delete. Drop and recreate the FTS index.
	//
	// which aborted the update and left the index stale. It surfaced from the fourth
	// consecutive update on a 3000-file corpus, and only because the call site stopped
	// discarding the error; the benchmark had been reading the failing no-op as a speed-up.
	s.dropFTSIndexes()

	delStmt, err := s.conn.Prepare(
		"UNWIND $paths AS p MATCH (e:SearchEntity {path: p}) DELETE e")
	if err != nil {
		return fmt.Errorf("prepare entity delete: %w", err)
	}
	defer delStmt.Close()
	delFileStmt, err := s.conn.Prepare(
		"UNWIND $paths AS p MATCH (f:SearchFile {path: p}) DELETE f")
	if err != nil {
		return fmt.Errorf("prepare file delete: %w", err)
	}
	defer delFileStmt.Close()

	params := map[string]any{"paths": affected}
	if res, err := s.conn.Execute(delStmt, params); err != nil {
		return fmt.Errorf("delete entities for changed files: %w", err)
	} else {
		res.Close()
	}
	if res, err := s.conn.Execute(delFileStmt, params); err != nil {
		return fmt.Errorf("delete files: %w", err)
	} else {
		res.Close()
	}

	var fileCount, entityCount, vecCount int
	seenUID := make(map[string]bool)
	for _, p := range changedFiles {
		entry := cache.GetEntry(p)
		if entry == nil {
			continue
		}
		r := rowsForEntry(p, entry, embLookup, seenUID)
		if err := s.flushBatch(insertFileCypher, r.files); err != nil {
			return fmt.Errorf("insert file %s: %w", p, err)
		}
		if err := s.flushBatch(insertEntityCypher, r.entities); err != nil {
			return fmt.Errorf("insert entities for %s: %w", p, err)
		}
		if err := s.flushBatch(insertEntityVecCypher, r.entitiesWithEmb); err != nil {
			return fmt.Errorf("insert embedded entities for %s: %w", p, err)
		}
		fileCount += len(r.files)
		entityCount += len(r.entities) + len(r.entitiesWithEmb)
		vecCount += len(r.entitiesWithEmb)
	}

	// Indexes back up over the mutated rows. The vector index is untouched throughout: it
	// genuinely does update in place, which is what makes the whole consolidation
	// worthwhile.
	ftsStart := time.Now()
	if err := s.createFTSIndexes(); err != nil {
		return err
	}
	ftsMS := time.Since(ftsStart).Seconds() * 1000

	s.log().Info("search index update",
		"files", fileCount, "entities", entityCount, "vectors", vecCount,
		"fts_rebuild_ms", ftsMS,
		"duration_ms", time.Since(t0).Seconds()*1000)
	return nil
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

// ftsPass runs one weighted FTS pass and folds it into scores by RRF on rank
// position.
func (s *SearchIndex) ftsPass(table, index, query string, weight float64, limit int,
	scores map[string]*rankedSearchDoc, isFile bool) {

	if strings.TrimSpace(query) == "" {
		return
	}

	var ret string
	if isFile {
		ret = "RETURN node.path AS path, node.name AS name, '' AS docstring, 'file' AS etype, 0 AS line"
	} else {
		ret = "RETURN node.path AS path, node.name AS name, node.docstring AS docstring, " +
			"node.etype AS etype, node.line AS line"
	}

	cypher := fmt.Sprintf(
		"CALL QUERY_FTS_INDEX('%s', '%s', $q) %s, score ORDER BY score DESC LIMIT %d",
		table, index, ret, limit)

	stmt, err := s.conn.Prepare(cypher)
	if err != nil {
		s.log().Debug("search pass prepare failed", "index", index, "error", err)
		return
	}
	defer stmt.Close()

	res, err := s.conn.Execute(stmt, map[string]any{"q": query})
	if err != nil {
		s.log().Debug("search pass failed", "index", index, "error", err)
		return
	}
	defer res.Close()

	rank := 0
	for res.HasNext() {
		tup, e := res.Next()
		if e != nil {
			break
		}
		r, ok := searchResultFromTuple(tup, isFile)
		if !ok {
			continue
		}
		foldRRF(scores, r, weight, rank)
		rank++
	}
}

type rankedSearchDoc struct {
	result SearchResult
	score  float64
}

func foldRRF(scores map[string]*rankedSearchDoc, r SearchResult, weight float64, rank int) {
	key := deduplicationKey(r)
	contribution := weight / float64(rrfK+rank+1)
	if existing, ok := scores[key]; ok {
		existing.score += contribution
		// Keep whichever view of the document carries a distance (only the semantic
		// pass sets one) and the richer docstring.
		if r.Distance != 0 && existing.result.Distance == 0 {
			existing.result.Distance = r.Distance
		}
		if existing.result.Docstring == "" && r.Docstring != "" {
			existing.result.Docstring = r.Docstring
		}
		return
	}
	scores[key] = &rankedSearchDoc{result: r, score: contribution}
}

func searchResultFromTuple(tup *lbug.FlatTuple, isFile bool) (SearchResult, bool) {
	get := func(i uint64) any {
		v, err := tup.GetValue(i)
		if err != nil {
			return nil
		}
		return v
	}
	asString := func(v any) string {
		if v == nil {
			return ""
		}
		if s, ok := v.(string); ok {
			return s
		}
		return fmt.Sprint(v)
	}

	path := asString(get(0))
	name := asString(get(1))
	if path == "" && name == "" {
		return SearchResult{}, false
	}

	line := 0
	switch v := get(4).(type) {
	case int64:
		line = int(v)
	case int:
		line = v
	}

	typ := "file"
	if !isFile {
		typ = strings.ToLower(asString(get(3)))
	}

	return SearchResult{
		Type:       typ,
		SearchType: "fts",
		Name:       name,
		Path:       path,
		Line:       line,
		Docstring:  asString(get(2)),
	}, true
}

// subTrigramScan is the fallback for queries too short to produce a trigram. It is a
// scan, not an index lookup, and is affordable only because it fires for 1–2
// character queries and is bounded by LIMIT — see TestPrefixIndexGap for why it is
// needed at all (Ladybug has no wildcard operator).
func (s *SearchIndex) subTrigramScan(tokens []string, limit int,
	scores map[string]*rankedSearchDoc) {

	var needles []string
	for _, t := range tokens {
		if n := normalizeForTrigrams(t); n != "" {
			needles = append(needles, n)
		}
	}
	if len(needles) == 0 {
		return
	}

	conds := make([]string, len(needles))
	args := make(map[string]any, len(needles))
	for i, n := range needles {
		key := fmt.Sprintf("n%d", i)
		conds[i] = fmt.Sprintf("lower(e.name) CONTAINS $%s", key)
		args[key] = n
	}

	cypher := fmt.Sprintf(
		"MATCH (e:SearchEntity) WHERE %s "+
			"RETURN e.path AS path, e.name AS name, e.docstring AS docstring, "+
			"e.etype AS etype, e.line AS line LIMIT %d",
		strings.Join(conds, " OR "), limit)

	stmt, err := s.conn.Prepare(cypher)
	if err != nil {
		s.log().Debug("sub-trigram scan prepare failed", "error", err)
		return
	}
	defer stmt.Close()

	res, err := s.conn.Execute(stmt, args)
	if err != nil {
		s.log().Debug("sub-trigram scan failed", "error", err)
		return
	}
	defer res.Close()

	rank := 0
	for res.HasNext() {
		tup, e := res.Next()
		if e != nil {
			break
		}
		r, ok := searchResultFromTuple(tup, false)
		if !ok {
			continue
		}
		foldRRF(scores, r, weightSubTrigramScan, rank)
		rank++
	}
}

// queryTrigramBag reduces a query to the trigram bag the index stores.
func queryTrigramBag(tokens []string) string {
	seen := make(map[string]bool)
	var grams []string
	for _, t := range tokens {
		if len([]rune(normalizeForTrigrams(t))) < 3 {
			continue
		}
		for _, g := range strings.Fields(identifierTrigrams(t)) {
			if !seen[g] {
				seen[g] = true
				grams = append(grams, g)
			}
		}
	}
	return strings.Join(grams, " ")
}

func (s *SearchIndex) lexicalPasses(tokens []string, limit int, scores map[string]*rankedSearchDoc) {
	joined := strings.Join(tokens, " ")

	s.ftsPass("SearchEntity", "se_split", joined, weightEntityName, limit, scores, false)
	s.ftsPass("SearchEntity", "se_doc", joined, weightEntityDoc, limit, scores, false)
	s.ftsPass("SearchEntity", "se_meta", joined, weightEntityMeta, limit, scores, false)
	s.ftsPass("SearchFile", "sf_name", joined, weightFileName, limit, scores, true)
	s.ftsPass("SearchFile", "sf_source", joined, weightFileSource, limit, scores, true)

	bag := queryTrigramBag(tokens)
	if bag == "" {
		// Nothing was long enough to trigram; only a scan can serve this query.
		s.subTrigramScan(tokens, limit, scores)
		return
	}
	s.ftsPass("SearchEntity", "se_tri", bag, weightEntityTrigram, limit, scores, false)
	s.ftsPass("SearchFile", "sf_tri", bag, weightFileTrigram, limit, scores, true)
}

func rankedToResults(scores map[string]*rankedSearchDoc, searchType string, limit int) []SearchResult {
	results := make([]SearchResult, 0, len(scores))
	for _, rd := range scores {
		rd.result.RelevanceScore = rd.score
		if searchType != "" {
			rd.result.SearchType = searchType
		}
		results = append(results, rd.result)
	}
	sortResultsDeterministic(results)
	if limit > 0 && len(results) > limit {
		results = results[:limit]
	}
	return results
}

func resolveLimit(topK int) (limit, fetch int) {
	limit = topK
	if limit <= 0 {
		limit = 50
	}
	return limit, limit * 3
}

func (s *SearchIndex) Search(query string, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	tokens := tokenizeQuery(query)
	if len(tokens) == 0 {
		return nil, nil
	}

	limit, fetch := resolveLimit(topK)
	scores := make(map[string]*rankedSearchDoc)
	s.lexicalPasses(tokens, fetch, scores)

	return rankedToResults(scores, "fts", limit), nil
}

func (s *SearchIndex) SemanticSearch(queryVec []float32, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.semanticSearchLocked(queryVec, topK)
}

func (s *SearchIndex) semanticSearchLocked(queryVec []float32, topK int) ([]SearchResult, error) {
	if !s.hasVector {
		return nil, nil
	}
	if len(queryVec) != ai.EmbeddingDimensions {
		return nil, fmt.Errorf("semantic search: query vector has %d dimensions, want %d",
			len(queryVec), ai.EmbeddingDimensions)
	}

	limit := topK
	if limit <= 0 {
		limit = 50
	}

	cypher := fmt.Sprintf(
		"CALL QUERY_VECTOR_INDEX('SearchEntity', 'se_vec', $q, %d) "+
			"RETURN node.path AS path, node.name AS name, node.docstring AS docstring, "+
			"node.etype AS etype, node.line AS line, distance ORDER BY distance", limit)

	stmt, err := s.conn.Prepare(cypher)
	if err != nil {
		return nil, fmt.Errorf("semantic search prepare: %w", err)
	}
	defer stmt.Close()

	res, err := s.conn.Execute(stmt, map[string]any{"q": queryVec})
	if err != nil {
		return nil, fmt.Errorf("semantic query failed: %w", err)
	}
	defer res.Close()

	var results []SearchResult
	for res.HasNext() {
		tup, e := res.Next()
		if e != nil {
			break
		}
		r, ok := searchResultFromTuple(tup, false)
		if !ok {
			continue
		}
		var distance float64
		if v, err := tup.GetValue(5); err == nil {
			switch d := v.(type) {
			case float64:
				distance = d
			case float32:
				distance = float64(d)
			}
		}
		r.SearchType = "semantic"
		r.Distance = distance
		// Same convention as the SQLite implementation: report similarity, derived
		// from the L2 distance between unit vectors.
		r.RelevanceScore = 1.0 - (distance*distance)/2.0
		results = append(results, r)
	}

	sortResultsDeterministic(results)
	return results, nil
}

func (s *SearchIndex) HybridSearch(query string, queryVec []float32, topK int) ([]SearchResult, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	tokens := tokenizeQuery(query)
	if len(tokens) == 0 && queryVec == nil {
		return nil, nil
	}

	limit, fetch := resolveLimit(topK)
	scores := make(map[string]*rankedSearchDoc)

	if len(tokens) > 0 {
		s.lexicalPasses(tokens, fetch, scores)
	}

	if queryVec != nil {
		semantic, err := s.semanticSearchLocked(queryVec, fetch)
		if err != nil {
			s.log().Debug("hybrid search: semantic pass unavailable", "error", err)
		}
		for rank, r := range semantic {
			foldRRF(scores, r, weightSemantic, rank)
		}
	}

	return rankedToResults(scores, "hybrid", limit), nil
}
