package ast

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

func CopyDBDir(src, dst string) error {
	_ = os.RemoveAll(dst)

	if runtime.GOOS == "linux" {
		cmd := exec.Command("cp", "-a", "--reflink=auto", src, dst)
		if err := cmd.Run(); err == nil {
			return nil
		}
	}

	cmd := exec.Command("cp", "-a", src, dst)
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("copy db dir: %w", err)
	}
	return nil
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

	t5 := time.Now()
	RunEnrichment(ctx, workingBackend, rootPath, logger)
	enrichTime := time.Since(t5)

	_ = workingBackend.Shutdown()
	_ = workingBackend.Close()

	if err := lb.AtomicSwapDB(workingPath); err != nil {
		return fmt.Errorf("atomic swap prod: %w", err)
	}
	swapped = true

	mutateTime := time.Since(totalStart)
	log.Info("production updated",
		"total_s", mutateTime.Seconds(),
		"copy_ms", copyTime.Seconds()*1000,
		"open_ms", openTime.Seconds()*1000,
		"delete_ms", deleteTime.Seconds()*1000,
		"insert_ms", insertTime.Seconds()*1000,
		"enrich_ms", enrichTime.Seconds()*1000,
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
