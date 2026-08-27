package ast

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"time"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// engineOwnedRelTypes are the relation types the ENGINE routes through a path of its
// own, and which therefore must not also be written as a generic relation edge.
//
// This replaced an allowlist of relation names, which was the wrong shape twice over.
// It was stale in both directions — it admitted CREATES, EXECUTES and TRUNCATES, which
// no shipped grammar declares, while a grammar declaring anything outside it was
// silently dropped at the last step, with its entities extracted, its references
// cached and no edge in the graph and no error anywhere. And it put GRAMMAR vocabulary
// in engine code: adding a relation meant editing Go, which is exactly what the query
// files exist to avoid.
//
// Naming what the engine owns is a closed question — these are engine concepts, not a
// language's — so anything a grammar invents now reaches the graph on its own.
var engineOwnedRelTypes = map[string]bool{
	// Become CallSites in processRelations, and edges through the CALLS group.
	RelCalls: true, "INSTANTIATES": true,
	// Become FieldAccess in ConvertToCache, with their own edge pair.
	RelReadsField: true, RelWritesField: true,
	// Have their own cached records and their own groups.
	RelInherits: true, RelImplements: true, RelImports: true,
	// Consumed while parsing; never a relation between two nodes.
	"DECORATOR": true, "EXPORT": true,
}

func RebuildFromJSON(ctx context.Context, db GraphDB, cache *ShardCache, embCache *ShardEmbCache, cluster, rootPath string, logger *slog.Logger) error {
	return rebuildFromJSON(ctx, db, cache, embCache, cluster, rootPath, logger, nil)
}

// RebuildFromJSONWithSearch is RebuildFromJSON followed by a rebuild of the search sidecar.
//
// AFTER the swap, deliberately, and this is the one ordering that works. The index is a
// separate file that no rename can publish together with the graph, so the only question is
// which of the two is briefly stale. Building it first would leave the index describing a
// corpus the published graph does not yet have — and if the swap then fails, permanently
// so. Building it second leaves search answering from the previous corpus for the length of
// one rebuild, which is the window this arrangement accepts by design.
//
// It also has to be the whole index and not a delta: the caller reached a FULL rebuild
// because the graph could not be updated incrementally, which is exactly the situation in
// which the index's own idea of the corpus cannot be trusted either.
func RebuildFromJSONWithSearch(ctx context.Context, db GraphDB, cache *ShardCache, embCache *ShardEmbCache, cluster, rootPath string, logger *slog.Logger, onProgress func(rows int)) error {
	if err := rebuildFromJSON(ctx, db, cache, embCache, cluster, rootPath, logger, onProgress); err != nil {
		return err
	}
	lb, ok := db.(*LadybugBackend)
	if !ok {
		return nil
	}

	idx, err := OpenSearchIndex(ctx, lb.cfg.DBPath)
	if err != nil {
		return fmt.Errorf("open search index: %w", err)
	}
	defer func() { _ = idx.Close() }()
	idx.Logger = logger

	// Returned, not swallowed: files.source is the only queryable copy of file text, so a
	// failure here costs every `ast source` read in the project, not merely search
	// freshness — and a graph whose text cannot be read looks perfectly healthy.
	if err := idx.RebuildFromCache(ctx, cache, BuildEmbLookup(cache, embCache)); err != nil {
		slogutil.Resolve(logger).Error("search index rebuild failed — search AND source "+
			"reads will fail until it is rebuilt", "error", err)
		return fmt.Errorf("search index rebuild: %w", err)
	}
	return nil
}

// RebuildFromJSONWithProgress is RebuildFromJSON reporting rows as they land.
//
// The write phase is the long one — measured here at 80% of an index run, against
// 16% for the parse — and it used to emit a single progress line before starting
// and then nothing at all. On a large repository that is minutes of silence after
// the last parse line, which reads as a hang; the code already carried a comment
// admitting 424 s of it. onProgress may be nil.
func RebuildFromJSONWithProgress(ctx context.Context, db GraphDB, cache *ShardCache, embCache *ShardEmbCache, cluster, rootPath string, logger *slog.Logger, onProgress func(rows int)) error {
	return rebuildFromJSON(ctx, db, cache, embCache, cluster, rootPath, logger, onProgress)
}

// rebuildFromJSON is the implementation: it builds the graph in a temporary database and
// publishes it with one rename. The search index is not its concern — it is a separate
// file, maintained by the callers above.
func rebuildFromJSON(ctx context.Context, db GraphDB, cache *ShardCache, embCache *ShardEmbCache, cluster, rootPath string, logger *slog.Logger, onProgress func(rows int)) error {
	log := slogutil.Resolve(logger)

	lb, ok := db.(*LadybugBackend)
	if !ok {
		return fmt.Errorf("rebuild requires LadybugBackend")
	}

	entries := make(map[string]*parseCacheEntry, cache.Count())
	cache.StreamEntries(func(relPath string, entry *parseCacheEntry) bool {
		entries[relPath] = entry
		return true
	})

	if len(entries) == 0 {
		return nil
	}
	log.Info("cache loaded", "files", len(entries))

	t1 := time.Now()
	ri := newRebuildIndex(entries, targetRulesFor(rootPath))
	schemaInfo := ri.schemaInfo()

	tempDBPath := lb.cfg.DBPath + "." + shortHex()

	swapped := false
	defer func() {
		if !swapped {
			_ = os.RemoveAll(tempDBPath)
		}
	}()

	tempCfg := LadybugConfig{
		DBPath: tempDBPath,
	}
	tempBackend := NewLadybugDB(tempCfg)

	if err := tempBackend.connect(); err != nil {
		_ = tempBackend.Close()
		log.Error("temp DB connect failed — LadybugDB file will NOT be created",
			"path", tempDBPath, "error", err)
		return fmt.Errorf("temp DB connect: %w", err)
	}

	if err := tempBackend.initSchemaForLabels(schemaInfo); err != nil {
		_ = tempBackend.Close()
		return fmt.Errorf("schema: %w", err)
	}

	_ = tempBackend.execQuery("INSTALL json")
	if err := tempBackend.execQuery("LOAD EXTENSION json"); err != nil {
		_ = tempBackend.Close()
		return fmt.Errorf("load json extension: %w", err)
	}
	log.Info("schema initialized", "duration_s", time.Since(t1).Seconds())

	t2 := time.Now()
	tmpDir, err := os.MkdirTemp("", "graphit-rebuild-*")
	if err != nil {
		_ = tempBackend.Close()
		return fmt.Errorf("tmpdir: %w", err)
	}
	defer func() { _ = os.RemoveAll(tmpDir) }()

	nodeCount, edgeCount := 0, 0
	var copyErrors int64

	// Split so the write phase can be attributed. It is 80% of an index run and
	// runs on one core, and "write" alone does not say whether the time goes to
	// serialising the rows to disk or to the database ingesting them — which are
	// different problems with different fixes.
	var serializeNanos, copyNanos, rowsWritten int64

	writeJSON := func(name string, data []map[string]any) string {
		if len(data) == 0 {
			return ""
		}
		t0 := time.Now()
		p := filepath.Join(tmpDir, name+".json")
		if err := writeJSONFile(p, data); err != nil {
			log.Error("write json file", "name", name, "error", err)
			atomic.AddInt64(&copyErrors, 1)
			return ""
		}
		atomic.AddInt64(&serializeNanos, int64(time.Since(t0)))
		atomic.AddInt64(&rowsWritten, int64(len(data)))
		return p
	}

	// execCopy runs one COPY and accounts for its time apart from serialisation.
	execCopy := func(q string) error {
		t0 := time.Now()
		err := tempBackend.execQuery(q)
		atomic.AddInt64(&copyNanos, int64(time.Since(t0)))
		if onProgress != nil {
			onProgress(int(atomic.LoadInt64(&rowsWritten)))
		}
		return err
	}

	copyNode := func(table string, cols []string, data []map[string]any) {
		for i, batch := range batchRows(data, copyBatchBytes) {
			name := table
			if i > 0 {
				name = fmt.Sprintf("%s.%d", table, i)
			}
			p := writeJSON(name, batch)
			if p == "" {
				return
			}
			q := fmt.Sprintf("COPY `%s`(%s) FROM '%s'", table, strings.Join(cols, ","), p)
			if err := execCopy(q); err != nil {
				log.Error("COPY node", "table", table, "batch", i, "rows", len(batch), "error", err)
				atomic.AddInt64(&copyErrors, 1)
			}
			nodeCount++
		}
	}

	copyEdge := func(relTable, fromLabel, toLabel, fromKey, toKey string, propKeys []string, data []map[string]any) {
		p := writeJSON(fmt.Sprintf("%s_%s_%s", relTable, fromLabel, toLabel), data)
		if p == "" {
			return
		}
		cols := []string{fromKey, toKey}
		cols = append(cols, propKeys...)
		retClause := strings.Join(cols, ", ")
		q := fmt.Sprintf(`COPY `+"`%s`"+` FROM (LOAD FROM '%s' RETURN %s) (from="%s", to="%s")`,
			relTable, p, retClause, fromLabel, toLabel)
		if err := execCopy(q); err != nil {
			log.Error("COPY edge", "rel", relTable, "from", fromLabel, "to", toLabel, "error", err)
			atomic.AddInt64(&copyErrors, 1)
		}
		edgeCount++
	}

	fileCols := []string{"path", "name", "relative_path", "is_dependency", "lang", "cluster"}
	dirCols := []string{"path", "name", "cluster"}
	entCols := []string{"uid", "name", "path", "line_number", "end_line", "docstring", "lang",
		"cyclomatic_complexity", "context", "context_type", "is_dependency", "is_exported",
		"value", "is_stub", "cluster"}
	modCols := []string{"uid", "name", "lang", "full_import_name", "is_stub", "cluster"}
	decCols := []string{"uid", "name", "lang", "is_stub", "cluster"}

	copyNode("File", fileCols, ri.fileNodeJSON())
	copyNode("Directory", dirCols, ri.dirNodeJSON(nil, ""))
	for _, label := range ri.labels {
		if label == "Module" {
			continue
		}
		copyNode(label, entCols, ri.entityJSON(label))
	}
	if ri.hasImports {
		copyNode("Module", modCols, ri.moduleJSON())
	}

	copyNode("Function", entCols, ri.stubFunctionJSON())
	if ri.labelSet["Class"] {
		copyNode("Class", entCols, ri.stubClassJSON())
	}
	if ri.labelSet["Interface"] {
		copyNode("Interface", entCols, ri.stubInterfaceJSON())
	}
	if ri.labelSet["Field"] {
		copyNode("Field", entCols, ri.stubFieldJSON())
	}
	if ri.labelSet["Table"] {
		copyNode("Table", entCols, ri.stubTableJSON())
	}
	for _, kind := range ri.annotationKinds {
		copyNode(kind, decCols, ri.annotationNodeJSON(kind))
	}
	nodeTime := time.Since(t2)

	t3 := time.Now()
	edgeProps := []string{"source_file", "line_number"}

	if ri.hasParams {
		for _, owner := range ri.paramOwnerLabels {
			copyEdge("HAS_PARAMETER", owner, "Parameter", "func_uid", "uid", edgeProps, ri.paramEdgeJSON(owner))
		}
	}

	for _, pt := range ri.labels {
		copyEdge("HAS_FIELD", pt, "Field", "parent_uid", "uid", edgeProps, ri.fieldEdgeJSON(pt))
	}

	for _, kind := range ri.annotationKinds {
		edgeName := "HAS_" + strings.ToUpper(kind)
		for _, ol := range ri.decoratorOwnerLabels {
			if !ri.labelSet[ol] {
				continue
			}
			copyEdge(edgeName, ol, kind, "entity_uid", "annotation_uid", edgeProps, ri.annotationEdgeJSON(kind, ol))
		}
	}

	if ri.hasImports {
		copyEdge("IMPORTS", "File", "Module", "file_uid", "module_uid",
			[]string{"alias", "full_import_name", "imported_name", "line_number", "source_file"},
			ri.importEdgeJSON())
	}

	for _, cl := range ri.callerLabels {
		if !ri.canWriteCallerLabel(cl) {
			continue
		}
		for _, tl := range ri.calleeLabels {
			copyEdge("CALLS", cl, tl, "caller_uid", "callee_uid",
				[]string{"source_file", "line_number", "full_call_name", "receiver_type"},
				ri.callEdgeJSON(cl, tl))
		}
	}

	for _, from := range ri.inheritLabels {
		if !ri.labelSet[from] {
			continue
		}
		for _, to := range ri.inheritLabels {
			if !ri.labelSet[to] {
				continue
			}
			copyEdge("INHERITS", from, to, "child_uid", "parent_uid", edgeProps, ri.inheritEdgeJSON("INHERITS", from, to))
			copyEdge("IMPLEMENTS", from, to, "child_uid", "parent_uid", edgeProps, ri.inheritEdgeJSON("IMPLEMENTS", from, to))
		}
	}

	if ri.labelSet[LabelField] {
		for _, src := range ri.fieldAccessSourceLabels {
			copyEdge("READS_FIELD", src, "Field", "source_uid", "field_uid", edgeProps, ri.fieldAccessEdgeJSON(false, src))
			copyEdge("WRITES_FIELD", src, "Field", "source_uid", "field_uid", edgeProps, ri.fieldAccessEdgeJSON(true, src))
		}
	}

	for _, rt := range ri.dmlTypes {
		if engineOwnedRelTypes[rt] {
			continue
		}
		for _, src := range ri.dmlSourceLabels {
			if !ri.labelSet[src] {
				continue
			}
			// One COPY per target label, matching the pairs the rel table group
			// declares. Sending every edge to Table was only ever right because
			// every target used to be a stub Table.
			for _, tgt := range ri.dmlTargetLabels {
				copyEdge(rt, src, tgt, "source_uid", "target_uid", edgeProps, ri.dmlEdgeJSON(rt, src, tgt))
			}
		}
		// Edges whose source is the FILE, in a step of their own: a statement at the
		// top of a script has no entity around it, and without this the edge was built
		// and discarded. The schema already declared `FROM File TO <target>` — what was
		// missing was someone filling it. File stays out of dmlSourceLabels precisely
		// so the pair is not declared twice.
		for _, tgt := range ri.dmlTargetLabels {
			copyEdge(rt, LabelFile, tgt, "source_uid", "target_uid", edgeProps,
				ri.dmlEdgeJSON(rt, LabelFile, tgt))
		}
	}

	for _, label := range ri.labels {
		copyEdge("CONTAINS", "File", label, "path", "uid", nil, ri.containsFileEntityJSON(label))
	}
	copyEdge("CONTAINS", "Directory", "Directory", "parent_dir", "child_dir", nil, ri.containsDirDirJSON())
	copyEdge("CONTAINS", "Directory", "File", "parent_dir", "file_path", nil, ri.containsDirFileJSON())
	for _, eg := range ri.containsPairs {
		copyEdge("CONTAINS", eg[0], eg[1], "parent_uid", "child_uid", nil, ri.containsEntityJSON(eg[0], eg[1]))
	}

	edgeTime := time.Since(t3)
	log.Info("COPY complete",
		"nodes", nodeCount, "node_duration_s", nodeTime.Seconds(),
		"edges", edgeCount, "edge_duration_s", edgeTime.Seconds(),
		"serialize_s", time.Duration(serializeNanos).Seconds(),
		"copy_s", time.Duration(copyNanos).Seconds(),
		"rows", rowsWritten)

	// A failed COPY used to be logged and then ignored: the rebuild enriched and
	// SWAPPED IN the half-loaded database, so a project could serve queries from a
	// graph missing a whole table with nothing surfaced to the caller. Observed on a
	// 36k-file Oracle export whose File COPY failed: 0 File nodes, therefore no
	// source for any path, no CONTAINS File→entity and no Directory→File — and every
	// File-anchored query answering "empty" rather than "broken". Keeping the old
	// database is strictly better than publishing an incomplete one; the temp DB is
	// removed by the deferred cleanup because swapped stays false.
	if copyErrors > 0 {
		_ = tempBackend.Shutdown()
		_ = tempBackend.Close()
		log.Error("COPY errors — keeping the previous database instead of swapping",
			"count", copyErrors)
		return fmt.Errorf("rebuild aborted: %d COPY operation(s) failed, "+
			"the graph would be incomplete", copyErrors)
	}

	_ = tempBackend.Shutdown()
	_ = tempBackend.Close()

	log.Info("swapping DB", "mode", "atomic")
	if err := lb.AtomicSwapDB(tempDBPath); err != nil {
		return fmt.Errorf("atomic swap: %w", err)
	}
	swapped = true

	return nil
}

func writeJSONFile(path string, data []map[string]any) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer func() { _ = f.Close() }()
	enc := json.NewEncoder(f)
	return enc.Encode(data)
}

// copyBatchBytes caps the payload of one COPY.
//
// The File table carries every file's full source in a column, so a single-shot COPY
// meant one JSON document holding the entire repository. On a 36k-file Oracle export
// that is 2.4 GB, with individual files as large as 133 MB — the COPY failed and the
// File table came out empty. Batching keeps each COPY bounded regardless of how big
// the repository is, and the failure of one batch no longer costs the whole table.
//
// A row larger than the budget still gets its own batch: the point is to bound the
// document, not to reject content.
const copyBatchBytes = 64 << 20

// batchRows splits rows into groups whose estimated JSON payload stays under
// maxBytes. Rows are measured, not counted, because sizes here span six orders of
// magnitude: an entity row is tens of bytes and a File row is its whole source.
func batchRows(data []map[string]any, maxBytes int) [][]map[string]any {
	if len(data) == 0 {
		return nil
	}
	if maxBytes <= 0 {
		return [][]map[string]any{data}
	}

	var batches [][]map[string]any
	start, size := 0, 0
	for i, row := range data {
		rowSize := estimateRowBytes(row)
		if i > start && size+rowSize > maxBytes {
			batches = append(batches, data[start:i])
			start, size = i, 0
		}
		size += rowSize
	}
	return append(batches, data[start:])
}

// estimateRowBytes approximates a row's encoded size. Only strings are measured
// exactly — they are the only values that can be arbitrarily large — and everything
// else is charged a flat width plus per-key overhead for quotes and separators.
func estimateRowBytes(row map[string]any) int {
	n := 2
	for k, v := range row {
		n += len(k) + 6
		switch t := v.(type) {
		case string:
			n += len(t)
		case []float32:
			// An embedding serialises as a list of decimal literals, so it is nothing
			// like the 8 bytes a scalar costs — at 768 dimensions it dominates the row.
			// Counting it as a scalar made the byte budget meaningless for search rows.
			n += len(t) * 12
		default:
			n += 8
		}
	}
	return n
}

func shortHex() string {
	b := make([]byte, 4)
	rand.Read(b)
	return hex.EncodeToString(b)[:7]
}
