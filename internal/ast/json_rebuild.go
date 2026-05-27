package ast

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"time"
)

var validDMLEdgeTypes = map[string]bool{
	"SELECTS": true, "INSERTS": true, "UPDATES": true,
	"DELETES": true, "CALLS": true, "EXECUTES": true,
	"CREATES": true, "DROPS": true, "ALTERS": true,
	"TRUNCATES": true, "REFERENCES": true,
}

func RebuildFromJSON(ctx context.Context, db GraphDB, cache *ShardCache, embCache *ShardEmbCache, cluster, rootPath string) error {
	lb, ok := db.(*LadybugBackend)
	if !ok {
		return fmt.Errorf("rebuild requires LadybugBackend")
	}

	entries := cache.AllEntries()
	if len(entries) == 0 {
		return nil
	}
	fmt.Fprintf(os.Stderr, "\n  › Cache: %d files\n", len(entries))

	t1 := time.Now()
	ri := newRebuildIndex(entries)
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
	fmt.Fprintf(os.Stderr, "  › Schema: %.1fs\n", time.Since(t1).Seconds())

	t2 := time.Now()
	tmpDir, err := os.MkdirTemp("", "graphit-rebuild-*")
	if err != nil {
		_ = tempBackend.Close()
		return fmt.Errorf("tmpdir: %w", err)
	}
	defer func() { _ = os.RemoveAll(tmpDir) }()

	nodeCount, edgeCount := 0, 0
	var copyErrors int64

	writeJSON := func(name string, data []map[string]any) string {
		if len(data) == 0 {
			return ""
		}
		p := filepath.Join(tmpDir, name+".json")
		if err := writeJSONFile(p, data); err != nil {
			fmt.Fprintf(os.Stderr, "  [error] write %s.json: %v\n", name, err)
			atomic.AddInt64(&copyErrors, 1)
			return ""
		}
		return p
	}

	copyNode := func(table string, cols []string, data []map[string]any) {
		p := writeJSON(table, data)
		if p == "" {
			return
		}
		q := fmt.Sprintf("COPY `%s`(%s) FROM '%s'", table, strings.Join(cols, ","), p)
		if err := tempBackend.execQuery(q); err != nil {
			fmt.Fprintf(os.Stderr, "  [error] COPY %s: %v\n", table, err)
			atomic.AddInt64(&copyErrors, 1)
		}
		nodeCount++
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
		if err := tempBackend.execQuery(q); err != nil {
			fmt.Fprintf(os.Stderr, "  [error] COPY %s(%s→%s): %v\n", relTable, fromLabel, toLabel, err)
			atomic.AddInt64(&copyErrors, 1)
		}
		edgeCount++
	}

	fileCols := []string{"path", "name", "relative_path", "is_dependency", "lang", "cluster", "source"}
	dirCols := []string{"path", "name", "cluster"}
	entCols := []string{"uid", "name", "path", "line_number", "end_line", "docstring", "lang",
		"cyclomatic_complexity", "context", "context_type", "is_dependency", "is_exported",
		"value", "is_stub", "entry_point_score"}
	modCols := []string{"uid", "name", "lang", "full_import_name", "is_stub"}
	decCols := []string{"uid", "name", "lang", "is_stub"}

	copyNode("File", fileCols, ri.fileNodeJSON(cluster))
	copyNode("Directory", dirCols, ri.dirNodeJSON(cluster))
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
		for _, cl := range ri.callerLabels {
			copyEdge("HAS_PARAMETER", cl, "Parameter", "func_uid", "uid", edgeProps, ri.paramEdgeJSON(cl))
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
		copyEdge("CALLS", cl, "Function", "caller_uid", "callee_uid",
			[]string{"source_file", "line_number", "full_call_name", "receiver_type"},
			ri.callEdgeJSON(cl))
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

	if ri.labelSet["Field"] {
		copyEdge("READS_FIELD", "Function", "Field", "source_uid", "field_uid", edgeProps, ri.fieldAccessEdgeJSON(false))
		copyEdge("WRITES_FIELD", "Function", "Field", "source_uid", "field_uid", edgeProps, ri.fieldAccessEdgeJSON(true))
	}

	for _, rt := range ri.dmlTypes {
		if !validDMLEdgeTypes[rt] {
			continue
		}
		for _, src := range ri.dmlSourceLabels {
			if !ri.labelSet[src] {
				continue
			}
			copyEdge(rt, src, "Table", "source_uid", "target_uid", edgeProps, ri.dmlEdgeJSON(rt, src))
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
	fmt.Fprintf(os.Stderr, "  › COPY nodes: %d (%.1fs) | edges: %d (%.1fs)\n",
		nodeCount, nodeTime.Seconds(), edgeCount, edgeTime.Seconds())
	if copyErrors > 0 {
		fmt.Fprintf(os.Stderr, "  [error] %d COPY errors\n", copyErrors)
	}

	t4 := time.Now()
	RunEnrichment(ctx, tempBackend, rootPath)

	fmt.Fprintf(os.Stderr, "  › Post-processing (enrichment): %.1fs\n", time.Since(t4).Seconds())

	_ = tempBackend.Shutdown()
	_ = tempBackend.Close()

	fmt.Fprintf(os.Stderr, "  › Swapping DB (atomic)…\n")
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

func shortHex() string {
	b := make([]byte, 4)
	rand.Read(b)
	return hex.EncodeToString(b)[:7]
}
