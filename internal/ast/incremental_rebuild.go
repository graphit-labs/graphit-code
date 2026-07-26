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
	"sync"
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

func IncrementalRebuild(ctx context.Context, lb *LadybugBackend, cache *ShardCache,
	embCache *ShardEmbCache, changedFiles []string, deletedFiles []string,
	cluster, rootPath string, searchIdx *SearchIndex, logger *slog.Logger) error {

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
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, searchIdx, logger)
	}

	cmsThreshold := cache.Count() * 20 / 100
	if cmsThreshold < 5 {
		cmsThreshold = 5
	}
	if len(allAffected) > cmsThreshold {
		log.Info("threshold exceeded, using full rebuild",
			"affected", len(allAffected), "threshold", cmsThreshold)
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, searchIdx, logger)
	}

	totalStart := time.Now()

	// In-place path: mutate the live database inside a transaction instead of
	// copying it, mutating the copy and swapping it in. Readers on sibling
	// connections of the same *lbug.Database keep serving the pre-commit snapshot
	// and flip atomically at COMMIT (TestLadybugSharedDatabaseMVCC), so the
	// lock-free-read property the swap provided is preserved for in-process
	// readers. This removes the copy, the second database open and — dominant and
	// wildly variable at 0.2–5.0 s — closing the mutated copy.
	if inPlaceIncrementalEnabled() {
		if err := incrementalInPlace(ctx, lb, cache, embCache, changedFiles, deletedFiles,
			allAffected, cluster, rootPath, searchIdx, totalStart, logger); err == nil {
			return nil
		} else {
			log.Warn("in-place incremental failed, falling back to copy+swap", "error", err)
		}
	}

	t1 := time.Now()
	workingPath := prodPath + "." + shortHex()
	if err := CopyDBDir(prodPath, workingPath); err != nil {
		log.Warn("copy prod failed, falling back to full rebuild")
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, searchIdx, logger)
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
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, searchIdx, logger)
	}
	_ = workingBackend.execQuery("INSTALL json")
	_ = workingBackend.execQuery("LOAD EXTENSION json")
	openTime := time.Since(t2)

	var searchWg sync.WaitGroup
	if searchIdx != nil {
		embLookup := BuildEmbLookup(cache, embCache)
		searchWg.Add(1)
		go func() {
			defer searchWg.Done()
			_ = searchIdx.UpdateIncremental(cache, changedFiles, deletedFiles, embLookup)
		}()
	}

	t3 := time.Now()
	deleteFileData(ctx, workingBackend, allAffected)
	deleteTime := time.Since(t3)

	t4 := time.Now()
	var insertErrors int64
	if len(changedFiles) > 0 {
		insertErrors = insertChangedFiles(ctx, workingBackend, cache, embCache, changedFiles, cluster, logger)
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
		searchWg.Wait() // the incremental search update must finish before it is rebuilt
		return fullRebuildWithSearch(ctx, lb, cache, embCache, cluster, rootPath, searchIdx, logger)
	}

	t5 := time.Now()
	RunEnrichment(ctx, workingBackend, rootPath, changedFiles, logger)
	enrichTime := time.Since(t5)

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
		"enrich_ms", enrichTime.Seconds()*1000,
		"shutdown_ms", shutdownTime.Seconds()*1000,
		"swap_ms", swapTime.Seconds()*1000,
		"errors", insertErrors)

	searchWg.Wait()

	log.Info("total incremental", "duration_s", time.Since(totalStart).Seconds())

	return nil
}

func fullRebuildWithSearch(ctx context.Context, lb *LadybugBackend, cache *ShardCache,
	embCache *ShardEmbCache, cluster, rootPath string, searchIdx *SearchIndex, logger *slog.Logger) error {

	if err := RebuildFromJSON(ctx, lb, cache, embCache, cluster, rootPath, logger); err != nil {
		return err
	}
	if searchIdx != nil {
		embLookup := BuildEmbLookup(cache, embCache)
		_ = searchIdx.RebuildFromCache(cache, embLookup)
	}
	return nil
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
	embCache *ShardEmbCache, changedFiles []string, cluster string, logger *slog.Logger) int64 {

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

	ri := newRebuildIndex(changedEntries)
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
		if _, err := db.Execute(ctx, q, map[string]any{"batch": data}); err != nil {
			log.Error("insert node", "table", table, "error", err)
			insertErrors++
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

	insertNodes("File", ri.fileNodeJSON(cluster))
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
		insertEdges("CALLS", cl, "Function", "caller_uid", "callee_uid",
			[]string{"source_file", "line_number", "full_call_name", "receiver_type"},
			ri.callEdgeJSON(cl))
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

// inPlaceIncrementalEnabled reports whether the in-place incremental write path
// should be used. It is opt-in for now (GRAPHIT_INPLACE_INCREMENTAL=1) because
// it changes the durability/visibility model: readers that open their OWN
// database handle — notably the stdio MCP server, a separate process — are
// outside LadybugDB's same-Database MVCC guarantee and observe a stale but
// consistent snapshot rather than the swap's whole-file flip.
func inPlaceIncrementalEnabled() bool {
	return os.Getenv("GRAPHIT_INPLACE_INCREMENTAL") == "1"
}

// incrementalInPlace applies the delta directly to the production database
// inside a single transaction. On any error it rolls back and returns, leaving
// the database untouched so the caller can fall back to copy+swap.
func incrementalInPlace(ctx context.Context, lb *LadybugBackend, cache *ShardCache,
	embCache *ShardEmbCache, changedFiles, deletedFiles, allAffected []string,
	cluster, rootPath string, searchIdx *SearchIndex, totalStart time.Time,
	logger *slog.Logger) error {

	log := slogutil.Resolve(logger)

	if err := lb.ensureConnectedLocked(); err != nil {
		return fmt.Errorf("connect: %w", err)
	}

	var searchWg sync.WaitGroup
	if searchIdx != nil {
		embLookup := BuildEmbLookup(cache, embCache)
		searchWg.Add(1)
		go func() {
			defer searchWg.Done()
			_ = searchIdx.UpdateIncremental(cache, changedFiles, deletedFiles, embLookup)
		}()
	}
	defer searchWg.Wait()

	t0 := time.Now()
	if err := lb.execQuery("BEGIN TRANSACTION"); err != nil {
		return fmt.Errorf("begin: %w", err)
	}
	committed := false
	defer func() {
		if !committed {
			// Readers never saw the partial state; drop it.
			_ = lb.execQuery("ROLLBACK")
		}
	}()

	t1 := time.Now()
	// Inside a transaction an ignored error is fatal: LadybugDB aborts the
	// transaction, so the later COMMIT fails with "No active transaction".
	// Surface delete errors instead of swallowing them as the copy+swap path can.
	if err := deleteFileDataChecked(ctx, lb, allAffected); err != nil {
		return fmt.Errorf("delete: %w", err)
	}
	deleteTime := time.Since(t1)

	t2 := time.Now()
	var insertErrors int64
	if len(changedFiles) > 0 {
		insertErrors = insertChangedFiles(ctx, lb, cache, embCache, changedFiles, cluster, logger)
	}
	insertTime := time.Since(t2)
	if insertErrors > 0 {
		// Roll back rather than commit a partial graph; the caller falls back.
		return fmt.Errorf("insert reported %d errors", insertErrors)
	}

	t4 := time.Now()
	if err := lb.execQuery("COMMIT"); err != nil {
		return fmt.Errorf("commit: %w", err)
	}
	committed = true
	commitTime := time.Since(t4)

	// Enrichment runs AFTER the commit, deliberately. It recomputes derived
	// properties (entry-point scores, detected frameworks) and is idempotent, so
	// it does not need to be atomic with the delta; running it inside the
	// transaction aborts that transaction, and keeping the transaction to just
	// delete+insert also shortens the window readers spend on the old snapshot.
	t3 := time.Now()
	RunEnrichment(ctx, lb, rootPath, changedFiles, logger)
	enrichTime := time.Since(t3)

	log.Info("production updated in place",
		"total_s", time.Since(totalStart).Seconds(),
		"begin_ms", t1.Sub(t0).Seconds()*1000,
		"delete_ms", deleteTime.Seconds()*1000,
		"insert_ms", insertTime.Seconds()*1000,
		"enrich_ms", enrichTime.Seconds()*1000,
		"commit_ms", commitTime.Seconds()*1000)
	return nil
}

// deleteFileDataChecked removes each affected file's subgraph and returns the
// first error. deleteFileData deliberately ignores errors, which is safe when
// mutating a throwaway copy but not inside a transaction, where any error aborts
// it and makes the subsequent COMMIT fail.
func deleteFileDataChecked(ctx context.Context, db GraphDB, paths []string) error {
	if len(paths) == 0 {
		return nil
	}
	params := map[string]any{"paths": paths}
	if _, err := db.Execute(ctx, `UNWIND $paths AS p MATCH (f:File {path: p})-[:CONTAINS]->(e) DETACH DELETE e`, params); err != nil {
		return fmt.Errorf("delete contained entities: %w", err)
	}
	if _, err := db.Execute(ctx, `UNWIND $paths AS p MATCH (f:File {path: p}) DETACH DELETE f`, params); err != nil {
		return fmt.Errorf("delete file nodes: %w", err)
	}
	return nil
}
