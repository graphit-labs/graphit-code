package ast

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// errReflinkUnsupported signals that a copy-on-write clone is unavailable
// (non-Linux, non-reflink filesystem, or a non-regular file), so CopyDBDir must
// fall back to a portable byte copy.
var errReflinkUnsupported = errors.New("reflink clone unsupported")

// CopyDBDir copies the LadybugDB store (a single file on liblbug 0.18.2; a
// directory tree is also handled) from src to dst. It prefers a copy-on-write
// reflink clone on Linux (near-instant on btrfs/XFS) and otherwise does a
// portable native byte copy. Implemented in pure Go — no `cp` subprocess — so it
// works on Windows and macOS, where the previous `cp` shell-out did not exist /
// did not clone and (on Windows) silently forced a full rebuild every run.
func CopyDBDir(src, dst string) error {
	if err := os.RemoveAll(dst); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("copy db: clean dst: %w", err)
	}
	if err := reflinkClone(src, dst); err == nil {
		return nil
	}
	return copyPath(src, dst)
}

func copyPath(src, dst string) error {
	info, err := os.Stat(src)
	if err != nil {
		return fmt.Errorf("copy db: stat src: %w", err)
	}
	if info.IsDir() {
		return copyDir(src, dst, info)
	}
	return copyFile(src, dst, info)
}

func copyDir(src, dst string, info os.FileInfo) error {
	if err := os.MkdirAll(dst, info.Mode().Perm()|0o700); err != nil {
		return fmt.Errorf("copy db: mkdir %s: %w", dst, err)
	}
	entries, err := os.ReadDir(src)
	if err != nil {
		return fmt.Errorf("copy db: read dir %s: %w", src, err)
	}
	for _, e := range entries {
		s := filepath.Join(src, e.Name())
		d := filepath.Join(dst, e.Name())
		fi, err := e.Info()
		if err != nil {
			return err
		}
		if fi.IsDir() {
			if err := copyDir(s, d, fi); err != nil {
				return err
			}
			continue
		}
		if err := copyFile(s, d, fi); err != nil {
			return err
		}
	}
	return nil
}

func copyFile(src, dst string, info os.FileInfo) error {
	in, err := os.Open(src)
	if err != nil {
		return fmt.Errorf("copy db: open %s: %w", src, err)
	}
	defer func() { _ = in.Close() }()
	out, err := os.OpenFile(dst, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, info.Mode().Perm())
	if err != nil {
		return fmt.Errorf("copy db: create %s: %w", dst, err)
	}
	if _, err := io.Copy(out, in); err != nil {
		_ = out.Close()
		return fmt.Errorf("copy db: copy %s: %w", src, err)
	}
	return out.Close()
}

// IncrementalRebuild applies a delta to the store by copying production, mutating the
// copy, and publishing it with rename(2).
//
// There is no in-place variant. Writing to production while readers hold it was measured
// against one fresh read-only open per subprocess — the shape of a stdio MCP call —
// against a writer behaving like the daemon:
//
//	writer model                                     reads ok  open failed  crashed
//	in place, commit + CHECKPOINT                      43/60        11         6
//	copy+swap, production never held by the writer     60/60         0         0
//
// No run produced torn rows; the failures are opens, not reads. They are not the writer's
// lock either — an idle read-write holder is harmless. They are the CHECKPOINT, which
// rewrites pages under a reader that is opening the same file. The engine reports that as
// an opaque status code and sometimes faults inside the open itself, and a SIGSEGV in cgo
// cannot be retried away: it takes the reader process with it.
//
// The search tables ride along, because they live in the same database. That is what lets
// the FTS indexes be dropped and recreated on every write — O(corpus) work the engine
// forces, since it does not maintain an FTS index on insert — without any of it touching
// the file readers have open.
func IncrementalRebuild(ctx context.Context, lb *LadybugBackend, cache *ShardCache,
	embCache *ShardEmbCache, changedFiles []string, deletedFiles []string,
	cluster, rootPath string, logger *slog.Logger) error {

	log := slogutil.Resolve(logger)

	if len(changedFiles) == 0 && len(deletedFiles) == 0 {
		return nil
	}

	allAffected := make([]string, 0, len(changedFiles)+len(deletedFiles))
	allAffected = append(allAffected, changedFiles...)
	allAffected = append(allAffected, deletedFiles...)

	log.Info("incremental rebuild",
		"changed", len(changedFiles), "deleted", len(deletedFiles), "total", cache.Count())

	prodPath := lb.cfg.DBPath

	if _, err := os.Stat(prodPath); os.IsNotExist(err) {
		log.Warn("no production DB, falling back to full rebuild")
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, logger)
	}

	cmsThreshold := cache.Count() * 20 / 100
	if cmsThreshold < 5 {
		cmsThreshold = 5
	}
	if len(allAffected) > cmsThreshold {
		log.Info("threshold exceeded, using full rebuild",
			"affected", len(allAffected), "threshold", cmsThreshold)
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, logger)
	}

	totalStart := time.Now()

	t1 := time.Now()
	workingPath := prodPath + "." + shortHex()
	if err := CopyDBDir(prodPath, workingPath); err != nil {
		log.Warn("copy prod failed, falling back to full rebuild")
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, logger)
	}
	copyTime := time.Since(t1)

	swapped := false
	defer func() {
		if !swapped {
			_ = os.RemoveAll(workingPath)
		}
	}()

	t2 := time.Now()
	workingBackend := NewLadybugDB(LadybugConfig{
		DBPath: workingPath,
	})
	if err := workingBackend.connect(); err != nil {
		_ = workingBackend.Close()
		log.Warn("open working DB failed, falling back to full rebuild")
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, logger)
	}
	_ = workingBackend.execQuery("INSTALL json")
	_ = workingBackend.execQuery("LOAD EXTENSION json")
	openTime := time.Since(t2)

	t3 := time.Now()
	deleteFileData(ctx, workingBackend, allAffected)
	deleteTime := time.Since(t3)

	t4 := time.Now()
	var insertErrors int64
	if len(changedFiles) > 0 {
		insertErrors = insertChangedFiles(ctx, workingBackend, cache, embCache, changedFiles, cluster, rootPath, logger)
	}
	insertTime := time.Since(t4)

	// Never publish a partial graph. Inserts can fail (notably a liblbug crash
	// on some UNWIND ... CREATE batches, which the generic ExecuteBatch path
	// works around per-row); previously the failure was only counted and the
	// working copy was swapped in anyway, silently dropping nodes and edges. A
	// full rebuild is slower but correct by construction.
	if insertErrors > 0 {
		log.Warn("incremental insert failed; falling back to full rebuild instead of publishing a partial graph",
			"errors", insertErrors, "changed", len(changedFiles))
		_ = workingBackend.Shutdown()
		_ = workingBackend.Close()
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, logger)
	}

	// The search index is a separate SQLite file, updated IN PLACE — not copied into the
	// working directory and not published by the rename below.
	//
	// That is the trade this whole arrangement is built on, and it is worth stating where
	// it is paid rather than where it is enjoyed. Riding the swap would mean copying the
	// index too, which is O(corpus) per edit; and the alternative that avoided the copy —
	// keeping the index inside the graph database — cost O(corpus) anyway, because that
	// engine does not maintain a full-text index on insert and every write had to rebuild
	// all seven. Measured there: 1,178 s for a one-file incremental at 39,429 files, worse
	// than rebuilding the whole thing. SQLite maintains its index through the triggers, so
	// the same delta is milliseconds.
	//
	// What that buys is bounded and known: for the width of this update, a reader can see
	// the new index against the old graph, or — if the swap below fails — the new index
	// against a graph that never changed. Neither is corruption; both are corrected by the
	// next incremental. Ordered before the swap rather than after only because a failure
	// here should still reach the full-rebuild fallback with the old graph intact.
	t5 := time.Now()
	searchIdx, err := OpenSearchIndex(ctx, lb.cfg.DBPath)
	if err != nil {
		log.Error("opening the search index failed; falling back to full rebuild", "error", err)
		_ = workingBackend.Shutdown()
		_ = workingBackend.Close()
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, logger)
	}
	searchIdx.Logger = logger
	updateErr := searchIdx.UpdateIncremental(ctx, cache, changedFiles, deletedFiles,
		BuildEmbLookup(cache, embCache))
	_ = searchIdx.Close()
	if updateErr != nil {
		// Fatal for the incremental, not merely logged: files.source is the only
		// queryable copy of file text, so an index that failed to update leaves search
		// answering from stale rows AND `ast source` unable to read the changed files —
		// both indistinguishable from working.
		log.Error("search index update failed; falling back to full rebuild", "error", updateErr)
		_ = workingBackend.Shutdown()
		_ = workingBackend.Close()
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, logger)
	}
	searchTime := time.Since(t5)

	t6 := time.Now()
	// Closing the mutated working copy is the dominant and most variable cost of
	// an incremental rebuild: measured 215 ms – 5.0 s on a 35k-file repository
	// while every other phase stays under ~110 ms. Dropping the explicit
	// CHECKPOINT was measured and does NOT help — Close() performs the same
	// flush internally (TestCloseWithoutCheckpointPersists shows the WAL is gone
	// and the data durable after Close alone). Removing this cost requires not
	// opening/closing a second database per incremental at all.
	_ = workingBackend.Shutdown()
	_ = workingBackend.Close()
	shutdownTime := time.Since(t6)

	t7 := time.Now()
	if err := lb.AtomicSwapDB(workingPath); err != nil {
		return fmt.Errorf("atomic swap prod: %w", err)
	}
	swapped = true
	swapTime := time.Since(t7)

	mutateTime := time.Since(totalStart)
	log.Info("production updated",
		"total_s", mutateTime.Seconds(),
		"copy_ms", copyTime.Seconds()*1000,
		"open_ms", openTime.Seconds()*1000,
		"delete_ms", deleteTime.Seconds()*1000,
		"insert_ms", insertTime.Seconds()*1000,
		"search_ms", searchTime.Seconds()*1000,
		"shutdown_ms", shutdownTime.Seconds()*1000,
		"swap_ms", swapTime.Seconds()*1000,
		"errors", insertErrors)

	log.Info("total incremental", "duration_s", time.Since(totalStart).Seconds())
	return nil
}

func fullRebuildWithSearch(ctx context.Context, lb *LadybugBackend, cache *ShardCache,
	embCache *ShardEmbCache, cluster, rootPath string, logger *slog.Logger) error {

	// The search tables are built INSIDE the rebuild's temporary database, so one rename
	// publishes the graph and the index together. Building them afterwards, through the
	// production handle, leaves the live store without an FTS index for the length of the
	// build — and every search answering empty while it lasts.
	//
	// Returned, not swallowed: SearchFile.source is the only queryable copy of file text,
	// so a failure here costs every ast source read in the project, not just search
	// freshness.
	return RebuildFromJSONWithSearch(ctx, lb, cache, embCache, cluster, rootPath, logger, nil)
}

func BuildEmbLookup(cache *ShardCache, embCache *ShardEmbCache) func(relPath, uid string) []float32 {
	if embCache == nil {
		return nil
	}
	return func(relPath, uid string) []float32 {
		hash := cache.GetHash(relPath)
		if hash == "" {
			return nil
		}
		return embCache.Get(relPath, uid, hash)
	}
}

func deleteFileData(ctx context.Context, db GraphDB, paths []string) {
	if len(paths) == 0 {
		return
	}

	params := map[string]any{"paths": paths}

	_, _ = db.Execute(ctx, `UNWIND $paths AS p MATCH (f:File {path: p})-[:CONTAINS]->(e) DETACH DELETE e`, params)

	_, _ = db.Execute(ctx, `UNWIND $paths AS p MATCH (f:File {path: p}) DETACH DELETE f`, params)
}

func insertChangedFiles(ctx context.Context, db GraphDB, cache *ShardCache,
	embCache *ShardEmbCache, changedFiles []string, cluster, rootPath string, logger *slog.Logger) int64 {

	log := slogutil.Resolve(logger)

	changedEntries := make(map[string]*parseCacheEntry, len(changedFiles))
	for _, p := range changedFiles {
		if e := cache.GetEntry(p); e != nil {
			changedEntries[p] = e
		}
	}
	if len(changedEntries) == 0 {
		return 0
	}

	ri := newRebuildIndex(changedEntries, targetRulesFor(rootPath))
	lb := db.(*LadybugBackend)

	var insertErrors int64

	insertNodes := func(table string, data []map[string]any) {
		if len(data) == 0 {
			return
		}

		keys := make([]string, 0, len(data[0]))
		for k := range data[0] {
			keys = append(keys, k)
		}
		sort.Strings(keys)

		props := make([]string, len(keys))
		for i, k := range keys {
			props[i] = fmt.Sprintf("`%s`: row.`%s`", k, k)
		}
		q := fmt.Sprintf("UNWIND $batch AS row CREATE (n:`%s` {%s})", table, strings.Join(props, ", "))
		// Bounded the same way as the full rebuild's COPY: a File row carries the
		// file's entire source, and a single large file is enough to make one
		// UNWIND parameter hundreds of megabytes.
		for _, batch := range batchRows(data, copyBatchBytes) {
			if _, err := db.Execute(ctx, q, map[string]any{"batch": batch}); err != nil {
				log.Error("insert node", "table", table, "rows", len(batch), "error", err)
				insertErrors++
			}
		}
	}

	tmpDir, _ := os.MkdirTemp("", "graphit-incr-*")
	defer func() { _ = os.RemoveAll(tmpDir) }()

	const edgeUnwindThreshold = 200

	insertEdges := func(relTable, fromLabel, toLabel, fromKey, toKey string, propKeys []string, data []map[string]any) {
		if len(data) == 0 {
			return
		}

		if len(data) <= edgeUnwindThreshold {

			setProps := make([]string, 0, len(propKeys))
			for _, pk := range propKeys {
				setProps = append(setProps, fmt.Sprintf("r.`%s` = row.`%s`", pk, pk))
			}
			setClause := ""
			if len(setProps) > 0 {
				setClause = " SET " + strings.Join(setProps, ", ")
			}
			q := fmt.Sprintf(
				"UNWIND $batch AS row "+
					"MATCH (a:`%s` {%s: row.`%s`}), (b:`%s` {%s: row.`%s`}) "+
					"CREATE (a)-[r:`%s`]->(b)%s",
				fromLabel, primaryKeyFor(fromLabel), fromKey,
				toLabel, primaryKeyFor(toLabel), toKey,
				relTable, setClause,
			)
			if _, err := db.Execute(ctx, q, map[string]any{"batch": data}); err != nil {
				log.Error("insert edge", "rel", relTable, "from", fromLabel, "to", toLabel, "error", err)
				insertErrors++
			}
			return
		}

		p := filepath.Join(tmpDir, fmt.Sprintf("%s_%s_%s.json", relTable, fromLabel, toLabel))
		if err := writeJSONFile(p, data); err != nil {
			insertErrors++
			return
		}
		cols := []string{fromKey, toKey}
		cols = append(cols, propKeys...)
		retClause := strings.Join(cols, ", ")
		q := fmt.Sprintf("COPY `%s` FROM (LOAD FROM '%s' RETURN %s) (from=\"%s\", to=\"%s\")",
			relTable, p, retClause, fromLabel, toLabel)
		if err := lb.execQuery(q); err != nil {
			log.Error("insert edge", "rel", relTable, "from", fromLabel, "to", toLabel, "error", err)
			insertErrors++
		}
	}

	edgeProps := []string{"source_file", "line_number"}

	insertNodes("File", ri.fileNodeJSON())
	for _, label := range ri.labels {
		if label == "Module" {
			continue
		}
		insertNodes(label, ri.entityJSON(label))
	}

	if ri.hasParams {
		for _, cl := range ri.callerLabels {
			insertEdges("HAS_PARAMETER", cl, "Parameter", "func_uid", "uid", edgeProps, ri.paramEdgeJSON(cl))
		}
	}
	for _, pt := range ri.labels {
		insertEdges("HAS_FIELD", pt, "Field", "parent_uid", "uid", edgeProps, ri.fieldEdgeJSON(pt))
	}
	// The incremental path wrote no field access at all, not even where the full one
	// did: a file reindexed by the watcher lost its READS_FIELD/WRITES_FIELD.
	for _, src := range ri.fieldAccessSourceLabels {
		insertEdges("READS_FIELD", src, "Field", "source_uid", "field_uid", edgeProps, ri.fieldAccessEdgeJSON(false, src))
		insertEdges("WRITES_FIELD", src, "Field", "source_uid", "field_uid", edgeProps, ri.fieldAccessEdgeJSON(true, src))
	}
	for _, kind := range ri.annotationKinds {
		edgeName := "HAS_" + strings.ToUpper(kind)
		for _, ol := range ri.decoratorOwnerLabels {
			if !ri.labelSet[ol] {
				continue
			}
			insertEdges(edgeName, ol, kind, "entity_uid", "annotation_uid", edgeProps, ri.annotationEdgeJSON(kind, ol))
		}
	}
	if ri.hasImports {
		insertEdges("IMPORTS", "File", "Module", "file_uid", "module_uid",
			[]string{"alias", "full_import_name", "imported_name", "line_number", "source_file"},
			ri.importEdgeJSON())
	}
	for _, cl := range ri.callerLabels {
		if !ri.labelSet[cl] {
			continue
		}
		for _, tl := range ri.calleeLabels {
			insertEdges("CALLS", cl, tl, "caller_uid", "callee_uid",
				[]string{"source_file", "line_number", "full_call_name", "receiver_type"},
				ri.callEdgeJSON(cl, tl))
		}
	}
	for _, label := range ri.labels {
		insertEdges("CONTAINS", "File", label, "path", "uid", nil, ri.containsFileEntityJSON(label))
	}
	for _, eg := range ri.containsPairs {
		insertEdges("CONTAINS", eg[0], eg[1], "parent_uid", "child_uid", nil, ri.containsEntityJSON(eg[0], eg[1]))
	}

	return insertErrors
}

func primaryKeyFor(label string) string {
	switch label {
	case "File":
		return "path"
	case "Directory":
		return "path"
	case "Module":
		return "uid"
	default:
		return "uid"
	}
}
