package ast

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io/fs"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/netutil"
	graphitui "github.com/graphit-labs/graphit-code/internal/ui"
)

type Server struct {
	db       GraphDB
	jobs     *JobManager
	aiClient ai.Client
	repoPath string
	port     int
	ln       net.Listener
	mux      *http.ServeMux
}

func NewServer(db GraphDB, jobs *JobManager, repoPath string) (*Server, error) {
	ln, port, err := netutil.ListenOnFreePort(8000)
	if err != nil {
		return nil, fmt.Errorf("no free port available: %w", err)
	}

	aiClient, _ := ai.NewClientFromConfig()

	s := &Server{
		db:       db,
		jobs:     jobs,
		aiClient: aiClient,
		repoPath: repoPath,
		port:     port,
		ln:       ln,
		mux:      http.NewServeMux(),
	}
	s.registerRoutes()
	return s, nil
}

func NewServerOnPort(db GraphDB, jobs *JobManager, repoPath string, port int) (*Server, error) {

	aiClient, _ := ai.NewClientFromConfig()

	s := &Server{
		db:       db,
		jobs:     jobs,
		aiClient: aiClient,
		repoPath: repoPath,
		port:     port,

		mux: http.NewServeMux(),
	}

	return s, nil
}

func (s *Server) Port() int { return s.port }

func (s *Server) Start(ctx context.Context) error {
	addr := fmt.Sprintf(":%d", s.port)
	srv := &http.Server{
		Addr:    addr,
		Handler: corsMiddleware(s.mux),
	}

	errCh := make(chan error, 1)
	go func() {
		errCh <- srv.Serve(s.ln)
	}()

	<-ctx.Done()
	shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_ = srv.Shutdown(shutdownCtx)

	select {
	case err := <-errCh:
		if err != nil && err != http.ErrServerClosed {
			return err
		}
	default:
	}
	return nil
}

func (s *Server) RegisterAPIRoutes(mux *http.ServeMux) {

	mux.HandleFunc("GET /api/query", s.handleQuery)
	mux.HandleFunc("POST /api/query", s.handleQuery)
	mux.HandleFunc("GET /api/schema", s.handleSchema)

	mux.HandleFunc("GET /api/graph", s.handleGraph)
	mux.HandleFunc("GET /api/file", s.handleFile)
	mux.HandleFunc("GET /api/contexts", s.handleContexts)
	mux.HandleFunc("POST /api/generate-cypher", s.handleGenerateCypher)

	mux.HandleFunc("GET /api/search", s.handleSearch)
	mux.HandleFunc("GET /api/find_code", s.handleFindCode)
	mux.HandleFunc("GET /api/dead_code", s.handleDeadCode)
	mux.HandleFunc("GET /api/relationships", s.handleRelationships)
	mux.HandleFunc("GET /api/complexity", s.handleComplexity)
	mux.HandleFunc("GET /api/most_complex", s.handleMostComplex)
	mux.HandleFunc("GET /api/fts", s.handleFTS)

	mux.HandleFunc("GET /api/jobs", s.handleListJobs)
	mux.HandleFunc("GET /api/jobs/{id}", s.handleGetJob)

	mux.HandleFunc("GET /api/repositories", s.handleListRepositories)
	mux.HandleFunc("GET /api/stats", s.handleRepoStats)

	mux.HandleFunc("POST /api/watch", s.handleWatch)
	mux.HandleFunc("DELETE /api/watch", s.handleUnwatch)
	mux.HandleFunc("GET /api/watched", s.handleListWatched)

	mux.HandleFunc("GET /api/status", s.handleStatus)
	mux.HandleFunc("GET /api/ping", s.handlePing)

	mux.HandleFunc("GET /api/parsers/status", s.handleParsersStatus)

	mux.HandleFunc("POST /api/export/obsidian", s.handleObsidianExport)
	mux.HandleFunc("POST /api/export/bundle", s.handleExportBundle)

	mux.HandleFunc("DELETE /api/context/{name}", s.handleDeleteContext)
}

func (s *Server) registerRoutes() {
	s.RegisterAPIRoutes(s.mux)

	s.mux.HandleFunc("/", s.handleUI)
}

type emptyGraphDB struct{}

func (e *emptyGraphDB) Query(_ context.Context, _ string, _ map[string]any) (*QueryResult, error) {
	return &QueryResult{}, nil
}
func (e *emptyGraphDB) Execute(_ context.Context, _ string, _ map[string]any) (*QueryResult, error) {
	return &QueryResult{}, nil
}
func (e *emptyGraphDB) ExecuteBatch(_ context.Context, _ []BatchQuery) error { return nil }
func (e *emptyGraphDB) Ping(_ context.Context) error                         { return nil }
func (e *emptyGraphDB) BackendType() string                                  { return "empty" }
func (e *emptyGraphDB) Close() error                                         { return nil }

func (s *Server) dbForContext(r *http.Request) (GraphDB, bool) {
	ctxName := r.URL.Query().Get("context")

	requestedDir := r.URL.Query().Get("project_dir")
	root := s.repoPath
	if root == "" {
		root, _ = os.Getwd()
	}
	if requestedDir != "" && requestedDir != root {

		if ctxName != "" && ctxName != "__project__" {

			importDBPath := filepath.Join(requestedDir, brand.DotDir(), "ast", ctxName, "ladybugdb")
			if resolved, err := filepath.EvalSymlinks(importDBPath); err == nil {
				importDBPath = resolved
			}
			if _, err := os.Stat(importDBPath); err == nil {
				db := NewLadybugDBReadOnly(LadybugConfig{DBPath: importDBPath, ReadOnly: true})
				return db, true
			}
			return &emptyGraphDB{}, false
		}

		otherDBPath := filepath.Join(requestedDir, brand.DotDir(), "ast", "project", "ladybugdb")
		if _, err := os.Stat(otherDBPath); err == nil {
			db := NewLadybugDBReadOnly(LadybugConfig{DBPath: otherDBPath, ReadOnly: true})
			return db, true
		}

		return &emptyGraphDB{}, false
	}

	if ctxName == "" || ctxName == "__project__" {
		return s.db, false
	}

	cfg := LadybugConfigForContext(ctxName)
	db := NewLadybugDB(cfg)
	return db, true
}

func (s *Server) handleQuery(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Cypher     string         `json:"cypher"`
		Params     map[string]any `json:"params"`
		Context    string         `json:"context"`
		ProjectDir string         `json:"project_dir"`
	}

	if r.Method == http.MethodGet {
		body.Cypher = r.URL.Query().Get("cypher")
		body.Context = r.URL.Query().Get("context")
	} else {
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			writeError(w, http.StatusBadRequest, "invalid request body")
			return
		}
	}

	if body.Cypher == "" {
		writeError(w, http.StatusBadRequest, "cypher query is required")
		return
	}

	if body.Context != "" {
		q := r.URL.Query()
		q.Set("context", body.Context)
		r.URL.RawQuery = q.Encode()
	}
	if body.ProjectDir != "" {
		q := r.URL.Query()
		q.Set("project_dir", body.ProjectDir)
		r.URL.RawQuery = q.Encode()
	}

	db, shouldClose := s.dbForContext(r)
	if shouldClose {
		defer db.Close()
	}

	ctx := r.Context()
	result, err := db.Query(ctx, body.Cypher, body.Params)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	writeJSON(w, map[string]any{
		"records": result.Records,
		"stats": map[string]any{
			"nodes_created":         result.NodesCreated,
			"relationships_created": result.RelationshipsCreated,
			"properties_set":        result.PropertiesSet,
		},
	})
}

func (s *Server) handleSchema(w http.ResponseWriter, r *http.Request) {
	db, shouldClose := s.dbForContext(r)
	if shouldClose {
		defer db.Close()
	}

	ctx := r.Context()

	labelsQ := `MATCH (n) RETURN DISTINCT label(n) as label, count(n) as count ORDER BY count DESC`
	labelsRes, err := db.Query(ctx, labelsQ, nil)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	nodeStats := make([]map[string]any, 0, len(labelsRes.Records))
	for _, r := range labelsRes.Records {
		nodeStats = append(nodeStats, map[string]any{
			"label": r["label"],
			"count": r["count"],
		})
	}

	edgesQ := `MATCH ()-[r]->() RETURN DISTINCT label(r) as type, count(r) as count ORDER BY count DESC LIMIT 20`
	edgesRes, _ := db.Query(ctx, edgesQ, nil)

	edgeStats := make([]map[string]any, 0)
	for _, r := range edgesRes.Records {
		edgeStats = append(edgeStats, map[string]any{
			"type":  r["type"],
			"count": r["count"],
		})
	}

	nodeLabels := make([]string, 0, len(nodeStats))
	for _, ns := range nodeStats {
		if l, ok := ns["label"].(string); ok {
			nodeLabels = append(nodeLabels, l)
		}
	}
	edgeTypes := make([]string, 0, len(edgeStats))
	for _, es := range edgeStats {
		if t, ok := es["type"].(string); ok {
			edgeTypes = append(edgeTypes, t)
		}
	}

	writeJSON(w, map[string]any{
		"nodes":       nodeStats,
		"edges":       edgeStats,
		"node_labels": nodeLabels,
		"edge_types":  edgeTypes,
		"backend":     db.BackendType(),
	})
}

func (s *Server) handleSearch(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	term := q.Get("q")
	if term == "" {
		writeError(w, http.StatusBadRequest, "q param required")
		return
	}

	fuzzy := q.Get("fuzzy") == "true"
	editDist := 2
	if d, err := strconv.Atoi(q.Get("edit_distance")); err == nil {
		editDist = d
	}

	db, shouldClose := s.dbForContext(r)
	if shouldClose {
		defer db.Close()
	}

	qs := NewQueryService(db)
	result, err := qs.FindRelatedCode(r.Context(), term, fuzzy, editDist, s.repoPath)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	writeJSON(w, result)
}

func (s *Server) handleFTS(w http.ResponseWriter, r *http.Request) {
	qp := r.URL.Query()
	term := qp.Get("q")
	if term == "" {
		writeError(w, http.StatusBadRequest, "q param required")
		return
	}
	topK := 20
	if v, err := strconv.Atoi(qp.Get("top")); err == nil && v > 0 {
		topK = v
	}

	db, shouldClose := s.dbForContext(r)
	if shouldClose {
		defer db.Close()
	}

	qs := NewQueryService(db)
	results, err := qs.FullTextSearch(r.Context(), term, topK)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	writeJSON(w, results)
}

func (s *Server) handleListJobs(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, s.jobs.List())
}

func (s *Server) handleGetJob(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	job := s.jobs.Get(id)
	if job == nil {
		writeError(w, http.StatusNotFound, "job not found")
		return
	}
	writeJSON(w, job)
}

func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	err := s.db.Ping(ctx)
	connected := err == nil

	writeJSON(w, map[string]any{
		"status":    "ok",
		"backend":   s.db.BackendType(),
		"connected": connected,
		"port":      s.port,
		"repo":      s.repoPath,
	})
}

func (s *Server) handlePing(w http.ResponseWriter, _ *http.Request) {
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write([]byte("pong"))
}

func (s *Server) handleObsidianExport(w http.ResponseWriter, r *http.Request) {
	var body struct {
		OutputDir string `json:"output_dir"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.OutputDir == "" {
		body.OutputDir = filepath.Join(s.repoPath, brand.DotDir(), "ast", "export")
	}

	exporter := NewObsidianExporter(s.db, s.repoPath)
	if err := exporter.Export(r.Context(), body.OutputDir); err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	writeJSON(w, map[string]any{
		"output_dir": body.OutputDir,
		"status":     "exported",
	})
}

func (s *Server) handleUI(w http.ResponseWriter, r *http.Request) {
	if graphitui.ServeStatic(w, r) {
		return
	}
	data, err := fs.ReadFile(graphitui.DistFS, "dist/index.html")
	if err != nil {
		writeError(w, http.StatusInternalServerError, "UI not found: "+err.Error())
		return
	}
	apiBase := fmt.Sprintf("http://localhost:%d", s.port)
	injection := fmt.Sprintf(`<script>
  window.__API_BASE__ = %q;
  window.__APP_MODE__ = "ast";
</script>`, apiBase)
	data = bytes.Replace(data, []byte("</head>"), []byte(injection+"</head>"), 1)
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	w.Write(data)
}

func (s *Server) handleGraph(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	repoPath := q.Get("repo_path")
	cypherQuery := q.Get("cypher_query")

	db, shouldClose := s.dbForContext(r)
	if shouldClose {
		defer db.Close()
	}

	ctx := r.Context()

	const defaultGraphQuery = `
		MATCH (n)
		OPTIONAL MATCH (n)-[r]->(m)
		RETURN
			CAST(id(n) AS STRING) AS src_id,
			label(n) AS src_label,
			n.name AS src_name,
			n.path AS src_path,
			n.cluster AS src_cluster,
			n.lang AS src_lang,
			CAST(id(m) AS STRING) AS dst_id,
			label(m) AS dst_label,
			m.name AS dst_name,
			m.path AS dst_path,
			m.cluster AS dst_cluster,
			m.lang AS dst_lang,
			label(r) AS rel_type
		LIMIT 300`

	cypher, isUserQuery := resolveGraphQuery(cypherQuery, repoPath, defaultGraphQuery)
	if isUserQuery {
		if err := validateReadOnlyQuery(cypher); err != nil {
			writeError(w, http.StatusBadRequest, err.Error())
			return
		}
	}

	result, err := db.Query(ctx, cypher, nil)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	nodesMap := map[string]map[string]any{}
	var edges []map[string]any
	var tabCols []string
	var tabRows [][]any

	for i, rec := range result.Records {
		if isUserQuery {
			tabCols, tabRows = collectTabularRow(rec, tabCols, tabRows, i)
			extractUserQueryGraph(rec, nodesMap, &edges)
		} else {
			extractBuiltinQueryGraph(rec, nodesMap, &edges)
		}
	}

	writeGraphResponse(w, nodesMap, edges, tabCols, tabRows, isUserQuery)
}

func resolveGraphQuery(cypherQuery, repoPath, defaultQuery string) (cypher string, isUserQuery bool) {
	switch {
	case cypherQuery != "":
		return cypherQuery, true
	case repoPath != "":
		return defaultQuery, false
	default:
		return defaultQuery, false
	}
}

func validateReadOnlyQuery(cypher string) error {
	upper := strings.ToUpper(strings.TrimSpace(cypher))
	for _, kw := range []string{"CREATE", "DELETE", "SET ", "REMOVE", "MERGE", "DROP", "DETACH"} {
		if strings.Contains(upper, kw) {
			return fmt.Errorf("Write operations are not allowed from the visualizer")
		}
	}
	return nil
}

func collectTabularRow(rec map[string]any, tabCols []string, tabRows [][]any, index int) ([]string, [][]any) {
	if index == 0 {
		for k := range rec {
			tabCols = append(tabCols, k)
		}
	}
	row := make([]any, len(tabCols))
	for j, col := range tabCols {
		row[j] = rec[col]
	}
	return tabCols, append(tabRows, row)
}

func extractUserQueryGraph(rec map[string]any, nodesMap map[string]map[string]any, edges *[]map[string]any) {

	for _, key := range []string{"n", "m", "a", "b", "src", "dst", "source", "target"} {
		if node, ok := rec[key]; ok && node != nil {
			if nm, ok := node.(map[string]any); ok {
				id := extractLadybugNodeID(nm)
				if id == "" {
					continue
				}
				if _, exists := nodesMap[id]; !exists {
					label := "Other"
					if l, ok := nm["Label"].(string); ok && l != "" {
						label = l
					}
					props, _ := nm["Properties"].(map[string]any)
					if props == nil {
						props = map[string]any{}
					}
					name := getStr(props, "name", getStr(props, "path", "Unknown"))
					nodesMap[id] = map[string]any{
						"id": id, "name": name, "label": name,
						"type": label, "file": getStr(props, "path", ""),
						"properties": props,
					}
				}
			}
		}
	}

	for _, key := range []string{"rel", "r", "e", "edge"} {
		if rel, ok := rec[key]; ok && rel != nil {
			if rm, ok := rel.(map[string]any); ok {
				relLabel := "RELATED"
				if l, ok := rm["Label"].(string); ok && l != "" {
					relLabel = l
				}
				srcID := ""
				dstID := ""
				if src, ok := rm["SourceID"].(map[string]any); ok {
					srcID = fmt.Sprintf("%v:%v", src["TableID"], src["Offset"])
				}
				if dst, ok := rm["DestinationID"].(map[string]any); ok {
					dstID = fmt.Sprintf("%v:%v", dst["TableID"], dst["Offset"])
				}
				if srcID != "" && dstID != "" {
					*edges = append(*edges, map[string]any{
						"source": srcID, "target": dstID, "type": strings.ToUpper(relLabel),
					})
				}
			}
		}
	}
}

func extractBuiltinQueryGraph(rec map[string]any, nodesMap map[string]map[string]any, edges *[]map[string]any) {

	srcID := ladybugIDStr(rec["src_id"])
	srcLabel := safeStr(rec["src_label"])
	srcName := safeStr(rec["src_name"])
	srcPath := safeStr(rec["src_path"])
	srcCluster := safeStr(rec["src_cluster"])
	srcLang := safeStr(rec["src_lang"])
	dstID := ladybugIDStr(rec["dst_id"])
	dstLabel := safeStr(rec["dst_label"])
	dstName := safeStr(rec["dst_name"])
	dstPath := safeStr(rec["dst_path"])
	dstCluster := safeStr(rec["dst_cluster"])
	dstLang := safeStr(rec["dst_lang"])
	relType := safeStr(rec["rel_type"])

	if _, exists := nodesMap[srcID]; !exists {
		nodesMap[srcID] = buildGraphNode(srcID, srcLabel, srcName, srcPath, srcCluster, srcLang)
	}
	if _, exists := nodesMap[dstID]; !exists {
		nodesMap[dstID] = buildGraphNode(dstID, dstLabel, dstName, dstPath, dstCluster, dstLang)
	}

	*edges = append(*edges, map[string]any{
		"source": srcID, "target": dstID, "type": strings.ToUpper(relType),
	})
}

func writeGraphResponse(w http.ResponseWriter, nodesMap map[string]map[string]any, edges []map[string]any, tabCols []string, tabRows [][]any, isUserQuery bool) {

	if isUserQuery && len(nodesMap) == 0 && len(tabRows) > 0 {
		writeJSON(w, map[string]any{
			"nodes": []any{}, "links": []any{},
			"files": []any{}, "fileContents": map[string]string{},
			"tabular": map[string]any{"columns": tabCols, "rows": tabRows},
		})
		return
	}

	nodes := make([]map[string]any, 0, len(nodesMap))
	fileSet := map[string]bool{}
	for _, n := range nodesMap {
		nodes = append(nodes, n)
		if strings.EqualFold(fmt.Sprintf("%v", n["type"]), "file") {
			if f := fmt.Sprintf("%v", n["file"]); f != "" {
				fileSet[f] = true
			}
		}
	}
	var files []string
	for f := range fileSet {
		files = append(files, f)
	}

	if edges == nil {
		edges = []map[string]any{}
	}
	if files == nil {
		files = []string{}
	}

	writeJSON(w, map[string]any{
		"nodes": nodes, "links": edges,
		"files": files, "fileContents": map[string]string{},
	})
}

func (s *Server) handleFile(w http.ResponseWriter, r *http.Request) {
	path := r.URL.Query().Get("path")
	if path == "" {
		writeError(w, http.StatusBadRequest, "path param required")
		return
	}

	contextName := r.URL.Query().Get("context")
	db, shouldClose := s.db, false
	if contextName != "" && contextName != "__project__" {
		cfg := LadybugConfigForContext(contextName)
		db = NewLadybugDB(cfg)
		shouldClose = true
	}
	if shouldClose {
		defer db.Close()
	}

	ctx := r.Context()
	res, err := db.Query(ctx, `MATCH (f:File {path: $path}) RETURN f.source AS source`, map[string]any{"path": path})
	if err == nil && len(res.Records) > 0 {
		if src, ok := res.Records[0]["source"].(string); ok && src != "" {
			writeJSON(w, map[string]string{"content": src, "source": "indexed"})
			return
		}
	}

	writeError(w, http.StatusNotFound, "File source not found")
}

func (s *Server) handleContexts(w http.ResponseWriter, r *http.Request) {
	root := s.repoPath
	if root == "" {
		root, _ = os.Getwd()
	}

	requestedDir := r.URL.Query().Get("project_dir")
	isDifferentProject := requestedDir != "" && requestedDir != root

	targetDir := root
	if isDifferentProject {
		targetDir = requestedDir
	}
	projectName := filepath.Base(targetDir)
	lockPath := filepath.Join(targetDir, brand.LockFileName())
	if data, err := os.ReadFile(lockPath); err == nil {
		var lock map[string]any
		if json.Unmarshal(data, &lock) == nil {
			if proj, ok := lock["project"].(map[string]any); ok {
				if name, ok := proj["name"].(string); ok && name != "" {
					projectName = name
				}
			}
		}
	}

	projectIDNames := loadProjectIDNames()

	contexts := []map[string]any{}
	ctx := r.Context()

	projectDBPath := filepath.Join(targetDir, brand.DotDir(), "ast", "project", "ladybugdb")
	if _, err := os.Stat(projectDBPath); err == nil {
		nodeCount, edgeCount := 0, 0
		if !isDifferentProject {

			if res, err := s.db.Query(ctx, "MATCH (n) RETURN count(n) AS c", nil); err == nil && len(res.Records) > 0 {
				if c, ok := res.Records[0]["c"]; ok {
					nodeCount = toInt(c)
				}
			}
			if res, err := s.db.Query(ctx, "MATCH ()-[r]->() RETURN count(r) AS c", nil); err == nil && len(res.Records) > 0 {
				if c, ok := res.Records[0]["c"]; ok {
					edgeCount = toInt(c)
				}
			}
		} else {

			otherDB := NewLadybugDBReadOnly(LadybugConfig{DBPath: projectDBPath, ReadOnly: true})
			if res, err := otherDB.Query(ctx, "MATCH (n) RETURN count(n) AS c", nil); err == nil && len(res.Records) > 0 {
				if c, ok := res.Records[0]["c"]; ok {
					nodeCount = toInt(c)
				}
			}
			if res, err := otherDB.Query(ctx, "MATCH ()-[r]->() RETURN count(r) AS c", nil); err == nil && len(res.Records) > 0 {
				if c, ok := res.Records[0]["c"]; ok {
					edgeCount = toInt(c)
				}
			}
			otherDB.Close()
		}
		backend := "ladybug"
		if !isDifferentProject {
			backend = s.db.BackendType()
		}
		contexts = append(contexts, map[string]any{
			"id": "__project__", "name": projectName, "type": "project",
			"database": backend, "node_count": nodeCount,
			"edge_count": edgeCount, "path": targetDir,
			"db_path": projectDBPath,
		})
	}

	seenIDs := map[string]bool{"__project__": true}
	astBaseDir := filepath.Join(targetDir, brand.DotDir(), "ast")
	if entries, err := os.ReadDir(astBaseDir); err == nil {
		for _, entry := range entries {
			name := entry.Name()
			if name == "project" || name == "imports" || name == "export" || strings.HasPrefix(name, ".") || name == "config.yaml" {
				continue
			}

			fullPath := filepath.Join(astBaseDir, name)
			info, err := os.Stat(fullPath)
			if err != nil || !info.IsDir() {
				continue
			}

			dbPath := filepath.Join(fullPath, "ladybugdb")
			if _, err := os.Stat(dbPath); err != nil {
				continue
			}

			seenIDs[name] = true
			displayName := name
			if readable, ok := projectIDNames[name]; ok {
				displayName = readable
			}
			ic := map[string]any{
				"id": name, "name": displayName, "type": "import",
				"database": "ladybugdb", "db_path": dbPath,
			}

			ctxDB := NewLadybugDBReadOnly(LadybugConfig{DBPath: dbPath, ReadOnly: true})
			if nRes, err := ctxDB.Query(ctx, "MATCH (n) RETURN count(n) AS c", nil); err == nil && len(nRes.Records) > 0 {
				ic["node_count"] = toInt(nRes.Records[0]["c"])
			}
			if eRes, err := ctxDB.Query(ctx, "MATCH ()-[r]->() RETURN count(r) AS c", nil); err == nil && len(eRes.Records) > 0 {
				ic["edge_count"] = toInt(eRes.Records[0]["c"])
			}
			ctxDB.Close()

			contexts = append(contexts, ic)
		}
	}

	if !isDifferentProject {
		for key, ictx := range ListImportedContexts() {
			if seenIDs[key] {
				continue
			}
			seenIDs[key] = true
			ic := map[string]any{
				"id": key, "name": ictx.Name, "type": "import",
				"database": "ladybugdb", "path": ictx.SourcePath,
				"imported_at": ictx.ImportedAt, "db_path": ictx.DBPath,
			}

			ctxDB := NewLadybugDB(LadybugConfigForContext(key))
			if nRes, err := ctxDB.Query(ctx, "MATCH (n) RETURN count(n) AS c", nil); err == nil && len(nRes.Records) > 0 {
				ic["node_count"] = toInt(nRes.Records[0]["c"])
			}
			if eRes, err := ctxDB.Query(ctx, "MATCH ()-[r]->() RETURN count(r) AS c", nil); err == nil && len(eRes.Records) > 0 {
				ic["edge_count"] = toInt(eRes.Records[0]["c"])
			}
			ctxDB.Close()

			contexts = append(contexts, ic)
		}
	}

	importsDir := filepath.Join(targetDir, brand.DotDir(), "ast", "imports")
	if entries, err := os.ReadDir(importsDir); err == nil {
		for _, entry := range entries {
			if !entry.IsDir() {
				continue
			}
			name := entry.Name()
			if seenIDs[name] {
				continue
			}
			meta := map[string]any{}
			metaPath := filepath.Join(importsDir, name, "import_meta.json")
			if data, err := os.ReadFile(metaPath); err == nil {
				_ = json.Unmarshal(data, &meta)
			}
			displayName := name
			if n, ok := meta["name"].(string); ok && n != "" {
				displayName = n
			}
			contexts = append(contexts, map[string]any{
				"id": name, "name": displayName, "type": "import",
				"node_count": toInt(meta["node_count"]),
				"edge_count": toInt(meta["edge_count"]),
				"path":       filepath.Join(importsDir, name),
			})
		}
	}

	writeJSON(w, map[string]any{"contexts": contexts, "project_root": targetDir, "project_name": projectName})
}

func (s *Server) handleDeleteContext(w http.ResponseWriter, r *http.Request) {
	name := r.PathValue("name")
	if name == "" || name == "__project__" {
		writeError(w, http.StatusBadRequest, "cannot delete the project context")
		return
	}

	if err := RemoveImportedContext(name); err != nil {
		writeError(w, http.StatusNotFound, err.Error())
		return
	}

	writeJSON(w, map[string]any{"status": "deleted", "context": name})
}

func (s *Server) handleGenerateCypher(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Query      string `json:"query"`
		Context    string `json:"context"`
		ProjectDir string `json:"project_dir"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Query == "" {
		writeError(w, http.StatusBadRequest, "No query provided")
		return
	}

	if s.aiClient == nil {
		writeError(w, http.StatusServiceUnavailable,
			"AI CLI not configured. Run:\n"+
				"  "+brand.BinName()+" config --global ai.cli <gemini|claude|opencode|codex|cursor-agent>\n"+
				"Then make sure the CLI is installed and authenticated on your system.")
		return
	}

	if body.Context != "" {
		q := r.URL.Query()
		q.Set("context", body.Context)
		r.URL.RawQuery = q.Encode()
	}
	if body.ProjectDir != "" {
		q := r.URL.Query()
		q.Set("project_dir", body.ProjectDir)
		r.URL.RawQuery = q.Encode()
	}

	db, shouldClose := s.dbForContext(r)
	if shouldClose {
		defer db.Close()
	}

	resp, err := GenerateAICypher(r.Context(), db, s.aiClient, AICypherRequest{
		UserQuery:  body.Query,
		RepoPath:   s.repoPath,
		MaxResults: 25,
		Backend:    db.BackendType(),
	})
	if err != nil {
		writeError(w, http.StatusBadGateway, err.Error())
		return
	}

	result := map[string]any{
		"cypher":         resp.Cypher,
		"original_query": body.Query,
		"keywords":       resp.Keywords,
		"entities":       resp.PreSearchEntities,
	}
	if resp.Error != "" {
		result["execution_error"] = resp.Error
	}
	writeJSON(w, result)
}

func getStr(m map[string]any, key, def string) string {
	if v, ok := m[key].(string); ok && v != "" {
		return v
	}
	return def
}

func safeStr(v any) string {
	if v == nil {
		return ""
	}
	s := fmt.Sprintf("%v", v)
	if s == "<nil>" || s == "" {
		return ""
	}
	return s
}

func buildGraphNode(id, label, name, path, cluster, lang string) map[string]any {
	displayName := name
	if displayName == "" {
		displayName = path
	}
	if displayName == "" {
		displayName = label
	}

	filePath := path
	if filePath == "" && (label == "File" || label == "Directory") {
		filePath = name
	}
	props := map[string]any{}
	if name != "" {
		props["name"] = name
	}
	if path != "" {
		props["path"] = path
	}
	if cluster != "" {
		props["cluster"] = cluster
	}
	if lang != "" {
		props["lang"] = lang
	}
	return map[string]any{
		"id": id, "name": displayName, "label": displayName,
		"type": label, "file": filePath,
		"properties": props,
	}
}

func ladybugIDStr(v any) string {
	if v == nil {
		return ""
	}

	if s, ok := v.(string); ok {
		return s
	}

	if m, ok := v.(map[string]any); ok {
		return fmt.Sprintf("%v:%v", m["TableID"], m["Offset"])
	}

	s := fmt.Sprintf("%v", v)

	s = strings.TrimPrefix(s, "{")
	s = strings.TrimSuffix(s, "}")
	parts := strings.Fields(s)
	if len(parts) == 2 {
		return parts[0] + ":" + parts[1]
	}
	return s
}

func extractLadybugNodeID(nm map[string]any) string {
	if idMap, ok := nm["ID"].(map[string]any); ok {
		return fmt.Sprintf("%v:%v", idMap["TableID"], idMap["Offset"])
	}

	if id, ok := nm["_id"]; ok {
		return ladybugIDStr(id)
	}
	return ""
}

func (s *Server) handleFindCode(w http.ResponseWriter, r *http.Request) {
	name := r.URL.Query().Get("name")
	if name == "" {
		writeError(w, 400, "name parameter required")
		return
	}
	entityType := r.URL.Query().Get("type")
	fuzzy := r.URL.Query().Get("fuzzy") == "true"
	repoPath := r.URL.Query().Get("repo")

	finder := NewCodeFinder(s.db)

	if entityType == "" {
		results, err := finder.FindRelatedCode(r.Context(), name, fuzzy, repoPath)
		if err != nil {
			writeError(w, 500, err.Error())
			return
		}
		writeJSON(w, results)
		return
	}

	var results []FindCodeResult
	var err error
	switch entityType {
	case "function":
		results, err = finder.FindByFunctionName(r.Context(), name, fuzzy, repoPath)
	case "class":
		results, err = finder.FindByClassName(r.Context(), name, fuzzy, repoPath)
	case "variable":
		results, err = finder.FindByVariableName(r.Context(), name, repoPath)
	default:
		relResults, relErr := finder.FindRelatedCode(r.Context(), name, fuzzy, repoPath)
		if relErr != nil {
			writeError(w, 500, relErr.Error())
			return
		}
		writeJSON(w, relResults)
		return
	}

	if err != nil {
		writeError(w, 500, err.Error())
		return
	}
	writeJSON(w, map[string]any{"results": results, "count": len(results)})
}

func (s *Server) handleDeadCode(w http.ResponseWriter, r *http.Request) {
	repoPath := r.URL.Query().Get("repo")
	excludeStr := r.URL.Query().Get("exclude_decorators")
	var exclude []string
	if excludeStr != "" {
		exclude = strings.Split(excludeStr, ",")
	}

	finder := NewCodeFinder(s.db)
	results, err := finder.FindDeadCode(r.Context(), exclude, repoPath)
	if err != nil {
		writeError(w, 500, err.Error())
		return
	}
	writeJSON(w, map[string]any{"dead_functions": results, "count": len(results)})
}

func (s *Server) handleRelationships(w http.ResponseWriter, r *http.Request) {
	name := r.URL.Query().Get("name")
	if name == "" {
		writeError(w, 400, "name parameter required")
		return
	}
	path := r.URL.Query().Get("path")
	repoPath := r.URL.Query().Get("repo")

	finder := NewCodeFinder(s.db)
	result, err := finder.AnalyzeRelationships(r.Context(), name, path, repoPath)
	if err != nil {
		writeError(w, 500, err.Error())
		return
	}
	writeJSON(w, result)
}

func (s *Server) handleComplexity(w http.ResponseWriter, r *http.Request) {
	name := r.URL.Query().Get("name")
	if name == "" {
		writeError(w, 400, "name parameter required")
		return
	}
	path := r.URL.Query().Get("path")
	repoPath := r.URL.Query().Get("repo")

	finder := NewCodeFinder(s.db)
	result, err := finder.GetCyclomaticComplexity(r.Context(), name, path, repoPath)
	if err != nil {
		writeError(w, 500, err.Error())
		return
	}
	writeJSON(w, result)
}

func (s *Server) handleMostComplex(w http.ResponseWriter, r *http.Request) {
	repoPath := r.URL.Query().Get("repo")
	limit := 10
	if l := r.URL.Query().Get("limit"); l != "" {
		if n, err := strconv.Atoi(l); err == nil && n > 0 {
			limit = n
		}
	}

	finder := NewCodeFinder(s.db)
	results, err := finder.FindMostComplexFunctions(r.Context(), limit, repoPath)
	if err != nil {
		writeError(w, 500, err.Error())
		return
	}
	writeJSON(w, map[string]any{"functions": results, "count": len(results)})
}

func (s *Server) handleListRepositories(w http.ResponseWriter, r *http.Request) {
	finder := NewCodeFinder(s.db)
	repos, err := finder.ListIndexedRepositories(r.Context())
	if err != nil {
		writeError(w, 500, err.Error())
		return
	}
	writeJSON(w, map[string]any{"repositories": repos, "count": len(repos)})
}

func (s *Server) handleRepoStats(w http.ResponseWriter, r *http.Request) {
	repoPath := r.URL.Query().Get("repo")
	if repoPath == "" {
		repoPath = s.repoPath
	}

	finder := NewCodeFinder(s.db)
	stats, err := finder.GetRepositoryStats(r.Context(), repoPath)
	if err != nil {
		writeError(w, 500, err.Error())
		return
	}
	writeJSON(w, stats)
}

type watcherEntry struct {
	watcher *Watcher
	cancel  context.CancelFunc
}

var (
	watcherRegistry   = make(map[string]*watcherEntry)
	watcherRegistryMu sync.Mutex
)

func (s *Server) handleWatch(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Path string `json:"path"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil || body.Path == "" {
		writeError(w, 400, "path parameter required")
		return
	}

	watcherRegistryMu.Lock()
	defer watcherRegistryMu.Unlock()

	if _, exists := watcherRegistry[body.Path]; exists {
		writeJSON(w, map[string]string{"status": "already_watching", "path": body.Path})
		return
	}

	watcher, err := NewWatcher(s.db, body.Path, s.repoPath, DefaultWatcherConfig(), s.jobs)
	if err != nil {
		writeError(w, 500, err.Error())
		return
	}

	ctx, cancel := context.WithCancel(context.Background())
	go watcher.Start(ctx)

	watcherRegistry[body.Path] = &watcherEntry{watcher: watcher, cancel: cancel}
	writeJSON(w, map[string]string{"status": "watching", "path": body.Path})
}

func (s *Server) handleUnwatch(w http.ResponseWriter, r *http.Request) {
	path := r.URL.Query().Get("path")
	if path == "" {
		writeError(w, 400, "path parameter required")
		return
	}

	watcherRegistryMu.Lock()
	defer watcherRegistryMu.Unlock()

	entry, exists := watcherRegistry[path]
	if !exists {
		writeError(w, 404, "path not being watched")
		return
	}

	entry.cancel()
	delete(watcherRegistry, path)
	writeJSON(w, map[string]string{"status": "unwatched", "path": path})
}

func (s *Server) handleListWatched(w http.ResponseWriter, _ *http.Request) {
	watcherRegistryMu.Lock()
	defer watcherRegistryMu.Unlock()

	var paths []string
	for p := range watcherRegistry {
		paths = append(paths, p)
	}
	writeJSON(w, map[string]any{"watched_paths": paths, "count": len(paths)})
}

func (s *Server) handleExportBundle(w http.ResponseWriter, r *http.Request) {
	var body struct {
		RepoPath   string `json:"repo_path"`
		OutputPath string `json:"output_path"`
	}
	if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
		writeError(w, 400, "invalid request body")
		return
	}
	if body.RepoPath == "" {
		body.RepoPath = s.repoPath
	}
	if body.OutputPath == "" {
		body.OutputPath = body.RepoPath + ".ast"
	}

	if err := ExportBundle(r.Context(), s.db, body.RepoPath, body.OutputPath); err != nil {
		writeError(w, 500, err.Error())
		return
	}
	writeJSON(w, map[string]string{"status": "exported", "path": body.OutputPath})
}

func (s *Server) handleParsersStatus(w http.ResponseWriter, _ *http.Request) {
	exts := TreeSitterSupportedExtensions()
	writeJSON(w, map[string]any{
		"engine":               "tree-sitter",
		"supported_extensions": exts,
		"count":                len(exts),
	})
}

func corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization")
		if r.Method == http.MethodOptions {
			w.WriteHeader(http.StatusNoContent)
			return
		}
		next.ServeHTTP(w, r)
	})
}

func writeJSON(w http.ResponseWriter, v any) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, msg string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(map[string]string{"error": msg})
}

func loadProjectIDNames() map[string]string {
	names := map[string]string{}
	registryPath := filepath.Join(brand.GlobalDir(), "hub.registry.json")
	data, err := os.ReadFile(registryPath)
	if err != nil {
		return names
	}
	var cache struct {
		Projects map[string]struct {
			Name string `json:"name"`
		} `json:"projects"`
	}
	if json.Unmarshal(data, &cache) == nil {
		for id, proj := range cache.Projects {
			if proj.Name != "" {
				names[id] = proj.Name
			}
		}
	}
	return names
}
