package ast

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/store"
)

type cachedDB struct {
	db       GraphDB
	lastUsed time.Time
}

type Server struct {
	db       GraphDB
	aiClient ai.Client
	repoPath string
	port     int
	mux      *http.ServeMux

	dbCacheMu sync.Mutex
	dbCache   map[string]*cachedDB
}

func NewServerOnPort(db GraphDB, repoPath string, port int) (*Server, error) {

	// The agent client is not even constructed when the module is off. Building it would shell out
	// to exec.LookPath for nothing, and leaving it nil is what makes the handler's existing
	// nil-check the single place that refuses the request.
	var aiClient ai.Client
	if config.AgentFeaturesEnabled(nil, config.LoadProjectConfig(repoPath)) {
		aiClient, _ = ai.NewClientFromConfig()
	}

	s := &Server{
		db:       db,
		aiClient: aiClient,
		repoPath: repoPath,
		port:     port,

		mux:     http.NewServeMux(),
		dbCache: make(map[string]*cachedDB),
	}

	return s, nil
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

	mux.HandleFunc("GET /api/repositories", s.handleListRepositories)
	mux.HandleFunc("GET /api/stats", s.handleRepoStats)

	mux.HandleFunc("GET /api/status", s.handleStatus)
	mux.HandleFunc("GET /api/ping", s.handlePing)

	mux.HandleFunc("GET /api/parsers/status", s.handleParsersStatus)

	mux.HandleFunc("POST /api/export/obsidian", s.handleObsidianExport)
	mux.HandleFunc("POST /api/export/bundle", s.handleExportBundle)

	mux.HandleFunc("DELETE /api/context/{name}", s.handleDeleteContext)
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

func (s *Server) getOrCreateCachedDB(projectDir, storeDir string, readOnly bool) GraphDB {
	key := projectDir + "\x00" + storeDir
	s.dbCacheMu.Lock()
	defer s.dbCacheMu.Unlock()

	if cached, ok := s.dbCache[key]; ok {
		cached.lastUsed = time.Now()
		return cached.db
	}

	var db GraphDB
	cfg := LadybugConfig{StoreDir: storeDir, IcebugDir: filepath.Join(storeDir, "graph.icebug"), ReadOnly: readOnly}
	if readOnly {
		db = NewLadybugDBReadOnly(cfg)
	} else {
		db = NewLadybugDB(cfg)
	}
	s.dbCache[key] = &cachedDB{db: db, lastUsed: time.Now()}
	return db
}

// requestedRoot reports which project root the request is about, and whether
// that is a different project than the one this server was started in.
//
// The UI's project switcher sends project_dir on every call, so this decision —
// "answer from my own store, or from that project's store" — has to be taken the
// same way by every handler. Taking it in one place is what keeps them agreeing.
func (s *Server) requestedRoot(r *http.Request) (root string, otherProject bool) {
	root = s.repoPath
	if root == "" {
		root, _ = os.Getwd()
	}
	if requested := r.URL.Query().Get("project_dir"); requested != "" && requested != root {
		return requested, true
	}
	return root, false
}

func (s *Server) dbForContext(r *http.Request) GraphDB {
	ctxName := r.URL.Query().Get("context")

	root, otherProject := s.requestedRoot(r)
	if otherProject {

		if ctxName != "" && ctxName != "__project__" {

			importDir := store.ASTContextDirIn(root, ctxName)
			if _, err := os.Stat(importDir); err == nil {
				return s.getOrCreateCachedDB(root, importDir, true)
			}
			return &emptyGraphDB{}
		}

		otherDir := store.ASTProjectDir(root)
		if _, err := os.Stat(otherDir); err == nil {
			return s.getOrCreateCachedDB(root, otherDir, true)
		}

		return &emptyGraphDB{}
	}

	if ctxName == "" || ctxName == "__project__" {
		return s.db
	}

	cfg := LadybugConfigForContextIn(root, ctxName)
	return s.getOrCreateCachedDB(root, cfg.StoreDir, false)
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

	db := s.dbForContext(r)

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
	db := s.dbForContext(r)

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

	edgeStats := schemaEdgeStats(ctx, db)

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
		"langs":       schemaLangGroups(ctx, db),
		"node_labels": nodeLabels,
		"edge_types":  edgeTypes,
		"backend":     db.BackendType(),
	})
}

func schemaEdgeStats(ctx context.Context, db GraphDB) []map[string]any {
	if provider, ok := db.(relationshipStatsProvider); ok {
		if stats, canonical := provider.logicalRelationshipStats(); canonical {
			out := make([]map[string]any, 0, len(stats))
			for _, stat := range stats {
				out = append(out, map[string]any{"type": stat.Type, "count": stat.Count})
			}
			return out
		}
	}

	edgesQ := `MATCH ()-[r]->() RETURN DISTINCT label(r) as type, count(r) as count ORDER BY count DESC LIMIT 20`
	edgesRes, _ := db.Query(ctx, edgesQ, nil)
	edgeStats := make([]map[string]any, 0, len(edgesRes.Records))
	for _, record := range edgesRes.Records {
		edgeStats = append(edgeStats, map[string]any{
			"type":  record["type"],
			"count": record["count"],
		})
	}
	return edgeStats
}

// schemaLangGroups renders SchemaLangGroups as the JSON the explorer's Schema panel
// reads. The grouping itself — including the rule that keeps the language-less group
// last — lives in SchemaLangGroups, so the panel and `ast schema` never disagree.
//
// The empty `lang` is passed through as-is rather than as NoLangGroup: the client names
// that group itself, because the same string is also its hide-filter key.
func schemaLangGroups(ctx context.Context, db GraphDB) []map[string]any {
	groups := SchemaLangGroups(ctx, db)

	out := make([]map[string]any, 0, len(groups))
	for _, g := range groups {
		labels := make([]map[string]any, 0, len(g.Labels))
		for _, l := range g.Labels {
			labels = append(labels, map[string]any{"label": l.Label, "count": l.Count})
		}
		out = append(out, map[string]any{
			"lang":   g.Lang,
			"count":  g.Count,
			"labels": labels,
		})
	}
	return out
}

func (s *Server) handleSearch(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	term := q.Get("q")
	if term == "" {
		writeError(w, http.StatusBadRequest, "q param required")
		return
	}

	topK := 20
	if v, err := strconv.Atoi(q.Get("top")); err == nil && v > 0 {
		topK = v
	}

	db := s.dbForContext(r)

	qs := NewQueryService(db)
	defer qs.Close()
	embClient, err := ai.NewEmbeddingClientFromConfig()
	if err == nil {
		qs.SetEmbeddingClient(embClient)
	}

	result, err := qs.HybridSearch(r.Context(), term, topK)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	writeJSON(w, result)
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

// How much of the graph the explorer draws when it opens. Both bind the SCAN, not
// the projection: on a graph with millions of nodes a LIMIT after an expansion
// materialises the intermediate result first and exhausts the buffer pool.
//
// The edge budget is the larger one because edges are what make the picture, and
// because raising it is nearly free — measured on a 2M-node graph, the edge sample
// costs ~0.19s at 300 rows and ~0.19s at 1000. Its cost is fan-out over the graph's
// tables, paid once, not per row.
const (
	graphSampleNodes = 300
	graphSampleEdges = 1000
)

// The node sample answers one question — what is IN this graph — and does not
// expand. That is a performance decision, taken against measurements rather than
// intuition, and it is worth stating because the obvious shape is the expensive one.
//
// It used to expand into `OPTIONAL MATCH (n)-[r]->(m) WHERE id(m) IN sample_ids`,
// to draw the edges among the sampled nodes. That expansion cost 0.45s for 300
// nodes, and none of it was data: the same query on a graph 34x smaller took the
// same 0.35s. The cost is the unlabelled pattern fanning out across every node and
// relationship table, plus an IN filter over a 300-element list evaluated per row —
// neither of which an index can help with, because nothing is being looked up by
// value. Without the expansion the same sample costs 0.01s.
//
// What it bought was 293 Directory-CONTAINS->File edges: the directory tree, which
// the explorer's file panel already shows. defaultGraphEdgeQuery is where the
// picture's connectivity comes from, and its budget was raised to more than cover
// what this stopped returning.
func graphNodeSampleQuery(withLine bool) string {
	return fmt.Sprintf(`
	MATCH (n)
	WITH n LIMIT %d
	RETURN
		%s`, graphSampleNodes, graphSideColumns("n", "src", withLine))
}

var defaultGraphQuery = graphNodeSampleQuery(true)

// graphEdgeSampleQuery samples the graph the other way round — edges first, with
// the nodes on both ends — and is the only source of links in the default view.
//
// Sampling nodes alone draws a field of dots: the first nodes of a repository-shaped
// graph are Files and Directories, which point at entities that fall outside any
// node sample. This scan starts from the edges instead, so every row is a link with
// both of its endpoints.
//
// The two stay separate rather than merged into one query: each remains a simple
// bounded scan, either one coming back empty is harmless, and a graph with no edges
// at all is still drawn through the node sample.
func graphEdgeSampleQuery(withLine bool) string {
	return fmt.Sprintf(`
	MATCH (n)-[r]->(m)
	WITH n, r, m LIMIT %d
	RETURN
		%s,
		%s,
		label(r) AS rel_type`, graphSampleEdges,
		graphSideColumns("n", "src", withLine), graphSideColumns("m", "dst", withLine))
}

var defaultGraphEdgeQuery = graphEdgeSampleQuery(true)

// graphSideColumns projects one end of a row. Both sample queries name their columns
// the same way, which is what lets one reader — graphNodeSideFrom — serve both ends.
//
// withLine is not a preference, it is a fallback. `n` is unlabelled, and a property
// binds only if SOME label in the graph carries it: a graph holding nothing but
// files and directories has no table with line_number, and asking for it there is
// not an empty column but a Binder exception that fails the whole query. That graph
// has no entity to jump to anyway, so the explorer drops the column and still draws.
func graphSideColumns(v, prefix string, withLine bool) string {
	cols := fmt.Sprintf(`CAST(id(%[1]s) AS STRING) AS %[2]s_id,
		label(%[1]s) AS %[2]s_label,
		%[1]s.name AS %[2]s_name,
		%[1]s.path AS %[2]s_path,
		%[1]s.cluster AS %[2]s_cluster,
		%[1]s.lang AS %[2]s_lang`, v, prefix)
	if withLine {
		cols += fmt.Sprintf(`,
		%s.line_number AS %s_line`, v, prefix)
	}
	return cols
}

// querySample runs a sample that asks for line_number and retries without it when
// this particular graph has no table carrying that property. Only a binder error on
// that exact column is retried — anything else is the caller's error to report.
func querySample(ctx context.Context, db GraphDB, withLine, withoutLine string) (*QueryResult, error) {
	res, err := db.Query(ctx, withLine, nil)
	if err == nil || !strings.Contains(err.Error(), "Cannot find property line_number") {
		return res, err
	}
	return db.Query(ctx, withoutLine, nil)
}

// defaultGraphQueryText exposes the query so its shape can be asserted.
func defaultGraphQueryText() string { return defaultGraphQuery }

// defaultGraphEdgeQueryText exposes the edge sample for the same reason.
func defaultGraphEdgeQueryText() string { return defaultGraphEdgeQuery }

func (s *Server) handleGraph(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	repoPath := q.Get("repo_path")
	cypherQuery := q.Get("cypher_query")

	db := s.dbForContext(r)

	ctx := r.Context()

	cypher, isUserQuery := resolveGraphQuery(cypherQuery, repoPath, defaultGraphQuery)
	if isUserQuery {
		if err := validateReadOnlyQuery(cypher); err != nil {
			writeError(w, http.StatusBadRequest, err.Error())
			return
		}
	}

	var result *QueryResult
	var err error
	if isUserQuery {
		result, err = db.Query(ctx, cypher, nil)
	} else {
		result, err = querySample(ctx, db, defaultGraphQuery, graphNodeSampleQuery(false))
	}
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

	// The node sample says what is in the graph; the edge sample says how it is
	// connected. Neither alone is a picture: sampling nodes on a repository-shaped
	// graph returns Files only, which have no edges between them. A failure here is
	// not fatal — the nodes already collected are still worth drawing.
	if !isUserQuery {
		if edgeResult, edgeErr := querySample(ctx, db, defaultGraphEdgeQuery, graphEdgeSampleQuery(false)); edgeErr == nil {
			for _, rec := range edgeResult.Records {
				extractBuiltinQueryGraph(rec, nodesMap, &edges)
			}
		}
	}

	normalizeGraphEdgeTypes(db, edges)
	writeGraphResponse(w, nodesMap, edges, tabCols, tabRows, isUserQuery)
}

func normalizeGraphEdgeTypes(db GraphDB, edges []map[string]any) {
	namer, ok := db.(relationshipTypeNamer)
	if !ok {
		return
	}
	for _, edge := range edges {
		if physical, ok := edge["type"].(string); ok {
			edge["type"] = namer.logicalRelationshipType(physical)
		}
	}
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
			return fmt.Errorf("write operations are not allowed from the visualizer")
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
					node := map[string]any{
						"id": id, "name": name, "label": label,
						"type": label, "file": getStr(props, "path", ""),
						"properties": props,
					}
					// A node returned by a typed query carries its raw properties, so
					// the line is already there — it just has to be lifted to where
					// the explorer looks for it, the same place the sample queries
					// put it.
					if line := toInt(props["line_number"]); line > 0 {
						node["line"] = line
					}
					nodesMap[id] = node
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

// graphNodeSide is one end of a row from either sample query. Both queries name
// their columns with the same src_/dst_ prefixes, so one reader serves both ends —
// which is also what keeps a new column from having to be threaded through a
// growing list of positional string arguments.
type graphNodeSide struct {
	id      string
	label   string
	name    string
	path    string
	cluster string
	lang    string
	line    int
}

func graphNodeSideFrom(rec map[string]any, prefix string) graphNodeSide {
	return graphNodeSide{
		id:      ladybugIDStr(rec[prefix+"_id"]),
		label:   safeStr(rec[prefix+"_label"]),
		name:    safeStr(rec[prefix+"_name"]),
		path:    safeStr(rec[prefix+"_path"]),
		cluster: safeStr(rec[prefix+"_cluster"]),
		lang:    safeStr(rec[prefix+"_lang"]),
		line:    toInt(rec[prefix+"_line"]),
	}
}

func extractBuiltinQueryGraph(rec map[string]any, nodesMap map[string]map[string]any, edges *[]map[string]any) {

	src := graphNodeSideFrom(rec, "src")
	if src.id == "" {
		return
	}
	dst := graphNodeSideFrom(rec, "dst")
	relType := safeStr(rec["rel_type"])

	if _, exists := nodesMap[src.id]; !exists {
		nodesMap[src.id] = buildGraphNode(src)
	}
	if dst.id != "" {
		if _, exists := nodesMap[dst.id]; !exists {
			nodesMap[dst.id] = buildGraphNode(dst)
		}
		*edges = append(*edges, map[string]any{
			"source": src.id, "target": dst.id, "type": strings.ToUpper(relType),
		})
	}
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

	if src, ok := FileSourceAt(r.Context(), s.storePathForRequest(r), path); ok {
		writeJSON(w, map[string]string{"content": src, "source": "indexed"})
		return
	}

	writeError(w, http.StatusNotFound, "File source not found")
}

// storePathForRequest resolves the store dir this request is asking about — the same
// answer dbForContext gives, as a path.
//
// The file handler needs the path and not the handle, because file text lives in
// the search index beside the icebug bundle rather than in the graph. It used to ask
// storePathFor, which only ever knows about the project this server was started
// in: with the UI pointed at another project, /api/graph answered from that
// project's store and /api/file answered 404 from this one, for a file that is
// indexed — just not here.
func (s *Server) storePathForRequest(r *http.Request) string {
	ctxName := r.URL.Query().Get("context")

	root, otherProject := s.requestedRoot(r)
	if otherProject {
		if ctxName != "" && ctxName != "__project__" {
			return store.ASTContextDirIn(root, ctxName)
		}
		return store.ASTProjectDir(root)
	}

	dir := s.storePathFor(root, ctxName)
	if dir != "" && !filepath.IsAbs(dir) {
		// A backend built by hand with a relative StoreDir resolves against the
		// request's project root — the same guard the DBPath version had, with
		// the same reason: the server may be serving another project.
		dir = filepath.Join(root, dir)
	}
	return dir
}

func (s *Server) storePathFor(projectDir, contextName string) string {
	if contextName != "" && contextName != "__project__" {
		return LadybugConfigForContextIn(projectDir, contextName).StoreDir
	}
	if lb, ok := s.db.(*LadybugBackend); ok {
		return lb.StoreDir()
	}
	return ""
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

	projectDir := store.ASTProjectDir(targetDir)
	if _, err := os.Stat(projectDir); err == nil {
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

			otherDB := s.getOrCreateCachedDB(targetDir, projectDir, true)
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
		}
		backend := "ladybug"
		if !isDifferentProject {
			backend = s.db.BackendType()
		}
		contexts = append(contexts, map[string]any{
			"id": "__project__", "name": projectName, "type": "project",
			"database": backend, "node_count": nodeCount,
			"edge_count": edgeCount, "path": targetDir,
			"store_path": projectDir,
		})
	}

	seenIDs := map[string]bool{"__project__": true}

	// ListImportedContextsIn answers for targetDir whether or not that is the
	// project this server was started in, and it reads targetDir's own records —
	// its lockfile for Hub contexts and its context registry for locally imported
	// ones. Every store is global, so walking the project would find nothing.
	for key, ictx := range ListImportedContextsIn(targetDir) {
		if seenIDs[key] {
			continue
		}
		seenIDs[key] = true

		displayName := ictx.Name
		if readable, ok := projectIDNames[key]; ok {
			displayName = readable
		}

		ic := map[string]any{
			"id": key, "name": displayName, "type": "import",
			"database": "ladybug", "path": ictx.SourcePath,
			"imported_at": ictx.ImportedAt, "store_path": store.ASTContextDirIn(targetDir, key),
		}

		ctxDB := s.getOrCreateCachedDB(targetDir, store.ASTContextDirIn(targetDir, key), true)
		if nRes, err := ctxDB.Query(ctx, "MATCH (n) RETURN count(n) AS c", nil); err == nil && len(nRes.Records) > 0 {
			ic["node_count"] = toInt(nRes.Records[0]["c"])
		}
		if eRes, err := ctxDB.Query(ctx, "MATCH ()-[r]->() RETURN count(r) AS c", nil); err == nil && len(eRes.Records) > 0 {
			ic["edge_count"] = toInt(eRes.Records[0]["c"])
		}

		contexts = append(contexts, ic)
	}

	writeJSON(w, map[string]any{"contexts": contexts, "project_root": targetDir, "project_name": projectName})
}

func (s *Server) handleDeleteContext(w http.ResponseWriter, r *http.Request) {
	name := r.PathValue("name")
	if name == "" || name == "__project__" {
		writeError(w, http.StatusBadRequest, "cannot delete the project context")
		return
	}

	root, _ := s.requestedRoot(r)
	if err := RemoveImportedContext(root, name); err != nil {
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

	db := s.dbForContext(r)

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

// buildGraphNode shapes one node for the visualizer.
//
// `name` is what the user reads (the entity name, or its path when it has none);
// `label` and `type` both carry the GRAPH label — `File`, `Function`, `Struct`.
// The distinction is not cosmetic: the explorer keys its per-label colours, its
// node radii and the sidebar's hide toggles off `label`, and the schema panel
// gets those labels from /api/schema, which returns `label(n)`. Emitting the
// display name as `label` — as this did — silently breaks all three, because a
// label like `Function` never equals a name like `handleFile`.
func buildGraphNode(s graphNodeSide) map[string]any {
	displayName := s.name
	if displayName == "" {
		displayName = s.path
	}
	if displayName == "" {
		displayName = s.label
	}

	filePath := s.path
	if filePath == "" && (s.label == "File" || s.label == "Directory") {
		filePath = s.name
	}
	props := map[string]any{}
	if s.name != "" {
		props["name"] = s.name
	}
	if s.path != "" {
		props["path"] = s.path
	}
	if s.cluster != "" {
		props["cluster"] = s.cluster
	}
	if s.lang != "" {
		props["lang"] = s.lang
	}
	// Line 0 is the call-target stub's placeholder, not a location: those nodes are
	// created by a call site and have no declaration to open. Omitting it is what
	// lets the explorer tell "no line to jump to" from "line 1".
	if s.line > 0 {
		props["line_number"] = s.line
	}
	node := map[string]any{
		"id": s.id, "name": displayName, "label": s.label,
		"type": s.label, "file": filePath,
		"properties": props,
	}
	if s.line > 0 {
		node["line"] = s.line
	}
	return node
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

func (s *Server) handleExportBundle(w http.ResponseWriter, r *http.Request) {
	var body struct {
		RepoPath   string `json:"repo_path"`
		OutputPath string `json:"output_path"`
		NoSources  bool   `json:"no_sources"`
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

	opts := BundleOptions{StorePath: s.storePathFor(body.RepoPath, ""), NoSources: body.NoSources}
	if err := ExportBundle(r.Context(), s.db, body.RepoPath, body.OutputPath, opts, nil); err != nil {
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
