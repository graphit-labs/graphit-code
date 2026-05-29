package ast

import (
	"context"
	"fmt"

	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

type SearchResult struct {
	Type           string
	Name           string
	Path           string
	Line           int
	Source         string
	Docstring      string
	IsDepend       bool
	SearchType     string
	RelevanceScore float64
	Distance       float64
}

type CallChainResult struct {
	FunctionChain []map[string]any
	CallDetails   []map[string]any
	ChainLength   int
}

type ClassHierarchy struct {
	ClassName string
	Parents   []map[string]any
	Children  []map[string]any
	Methods   []map[string]any
}

type QueryService struct {
	db              GraphDB
	dbPath          string
	lacksFulltext   bool
	embeddingClient ai.EmbeddingClient
	searchIndex     *SearchIndex
}

func NewQueryService(db GraphDB) *QueryService {
	qs := &QueryService{
		db:            db,
		lacksFulltext: db.BackendType() != "neo4j",
	}
	if lb, ok := db.(*LadybugBackend); ok {
		qs.dbPath = lb.cfg.DBPath
		idxPath := lb.cfg.DBPath + ".search.sqlite"
		if si, err := OpenSearchIndex(idxPath); err == nil {
			qs.searchIndex = si
		}
	}
	return qs
}

func (q *QueryService) Close() {
	if q.searchIndex != nil {
		_ = q.searchIndex.Close()
		q.searchIndex = nil
	}
}

func (q *QueryService) SetEmbeddingClient(client ai.EmbeddingClient) {
	q.embeddingClient = client
}

func (q *QueryService) SetSearchIndex(si *SearchIndex) {
	q.searchIndex = si
}

func (q *QueryService) FindByFunctionName(
	ctx context.Context,
	name string,
	fuzzy bool,
	repoPath string,
	editDistance int,
) ([]SearchResult, error) {
	if !fuzzy || q.lacksFulltext {
		cypher := buildNodeNameQuery("Function", fuzzy, repoPath)
		params := map[string]any{"name": name}
		if repoPath != "" {
			params["repo_path"] = repoPath
		}
		if fuzzy {
			params["search_term"] = name
		}
		res, err := q.db.Query(ctx, cypher, params)
		if err != nil {
			return nil, fmt.Errorf("find function: %w", err)
		}
		if q.lacksFulltext && fuzzy {
			return q.fuzzyFilter(res.Records, name, editDistance, "function"), nil
		}
		return recordsToResults(res.Records, "function"), nil
	}
	return q.fullTextSearch(ctx, "Function", name, editDistance, repoPath, "function")
}

func (q *QueryService) FindByClassName(
	ctx context.Context,
	name string,
	fuzzy bool,
	repoPath string,
	editDistance int,
) ([]SearchResult, error) {
	if !fuzzy || q.lacksFulltext {
		cypher := buildNodeNameQuery("Class", fuzzy, repoPath)
		params := map[string]any{"name": name}
		if repoPath != "" {
			params["repo_path"] = repoPath
		}
		if fuzzy {
			params["search_term"] = name
		}
		res, err := q.db.Query(ctx, cypher, params)
		if err != nil {
			return nil, fmt.Errorf("find class: %w", err)
		}
		if q.lacksFulltext && fuzzy {
			return q.fuzzyFilter(res.Records, name, editDistance, "class"), nil
		}
		return recordsToResults(res.Records, "class"), nil
	}
	return q.fullTextSearch(ctx, "Class", name, editDistance, repoPath, "class")
}

func (q *QueryService) FindByVariableName(ctx context.Context, name, repoPath string) ([]SearchResult, error) {
	repoFilter := ""
	params := map[string]any{"search_term": name}
	if repoPath != "" {
		repoFilter = "AND v.path STARTS WITH $repo_path"
		params["repo_path"] = repoPath
	}
	cypher := fmt.Sprintf(`
		MATCH (v:Variable)
		WHERE v.name CONTAINS $search_term %s
		RETURN v.name as name, v.path as path, v.line_number as line_number,
		       v.value as value, v.context as context, v.is_dependency as is_dependency
		ORDER BY v.is_dependency ASC, v.name
		LIMIT 20`, repoFilter)

	res, err := q.db.Query(ctx, cypher, params)
	if err != nil {
		return nil, err
	}
	return recordsToResults(res.Records, "variable"), nil
}

func (q *QueryService) FindByContent(ctx context.Context, term, repoPath string) ([]SearchResult, error) {
	repoFilter := ""
	params := map[string]any{"search_term": term}
	if repoPath != "" {
		repoFilter = "AND node.path STARTS WITH $repo_path"
		params["repo_path"] = repoPath
	}

	var results []SearchResult
	for _, label := range []string{"Function", "Class"} {
		typeName := strings.ToLower(label)
		cypher := fmt.Sprintf(`
			MATCH (node:%s)
			WHERE (toLower(node.name) CONTAINS toLower($search_term)
			    OR (node.docstring IS NOT NULL AND toLower(node.docstring) CONTAINS toLower($search_term)))
			    %s
			RETURN node.name as name, node.path as path, node.line_number as line_number,
			       node.docstring as docstring, node.is_dependency as is_dependency
			ORDER BY node.is_dependency ASC, node.name
			LIMIT 20`, label, repoFilter)

		res, err := q.db.Query(ctx, cypher, params)
		if err != nil {
			continue
		}
		results = append(results, recordsToResults(res.Records, typeName)...)
	}

	if len(results) > 20 {
		results = results[:20]
	}
	return results, nil
}

func (q *QueryService) WhoCallsFunction(ctx context.Context, funcName, path, repoPath string) ([]map[string]any, error) {
	repoFilter := ""
	params := map[string]any{"function_name": funcName}
	if repoPath != "" {
		repoFilter = "AND caller.path STARTS WITH $repo_path"
		params["repo_path"] = repoPath
	}
	if path != "" {
		params["path"] = path
	}

	cypher := fmt.Sprintf(`
		MATCH (caller)-[call:CALLS]->(target:Function {name: $function_name})
		WHERE (caller:Function OR caller:Class OR caller:File) %s
		RETURN caller.name as caller_function, caller.path as caller_file_path,
		       caller.line_number as caller_line_number, caller.is_dependency as caller_is_dependency,
		       call.line_number as call_line_number
		ORDER BY caller.is_dependency ASC, caller.path, caller.line_number
		LIMIT 20`, repoFilter)

	res, err := q.db.Query(ctx, cypher, params)
	if err != nil {
		return nil, err
	}
	return recordsToMaps(res.Records), nil
}

func (q *QueryService) WhatDoesFunctionCall(ctx context.Context, funcName, path, repoPath string) ([]map[string]any, error) {
	params := map[string]any{"function_name": funcName}
	repoFilter := "OR $repo_path IS NULL"
	if repoPath != "" {
		params["repo_path"] = repoPath
		repoFilter = "OR called.path STARTS WITH $repo_path"
	}
	if path != "" {
		params["path"] = path
	}

	cypher := fmt.Sprintf(`
		MATCH (caller:Function {name: $function_name})-[call:CALLS]->(called:Function)
		WHERE called.path STARTS WITH $repo_path %s
		RETURN called.name as called_function, called.path as called_file_path,
		       called.line_number as called_line_number, called.is_dependency as called_is_dependency,
		       call.line_number as call_line_number
		ORDER BY called.is_dependency ASC, called_function
		LIMIT 20`, repoFilter)

	res, err := q.db.Query(ctx, cypher, params)
	if err != nil {
		return nil, err
	}
	return recordsToMaps(res.Records), nil
}

func (q *QueryService) FindClassHierarchy(ctx context.Context, className, path, repoPath string) (*ClassHierarchy, error) {
	params := map[string]any{"class_name": className}
	if path != "" {
		params["path"] = path
	}
	if repoPath != "" {
		params["repo_path"] = repoPath
	}

	cypher := `
		MATCH (child:Class {name: $class_name})
		MATCH (child)-[:INHERITS]->(parent:Class)
		RETURN parent.name as parent_class, parent.path as parent_file_path,
		       parent.line_number as parent_line_number, parent.is_dependency as parent_is_dependency
		ORDER BY parent.is_dependency ASC
		LIMIT 20`

	parentsRes, _ := q.db.Query(ctx, cypher, params)

	cypher2 := `
		MATCH (child:Class {name: $class_name})
		MATCH (grandchild:Class)-[:INHERITS]->(child)
		RETURN grandchild.name as child_class, grandchild.path as child_file_path,
		       grandchild.line_number as child_line_number, grandchild.is_dependency as child_is_dependency
		ORDER BY grandchild.is_dependency ASC
		LIMIT 20`
	childrenRes, _ := q.db.Query(ctx, cypher2, params)

	cypher3 := `
		MATCH (child:Class {name: $class_name})-[:CONTAINS]->(method:Function)
		RETURN method.name as method_name, method.path as method_file_path,
		       method.line_number as method_line_number, method.is_dependency as method_is_dependency
		ORDER BY method.is_dependency ASC, method.line_number
		LIMIT 20`
	methodsRes, _ := q.db.Query(ctx, cypher3, params)

	return &ClassHierarchy{
		ClassName: className,
		Parents:   recordsToMaps(parentsRes.Records),
		Children:  recordsToMaps(childrenRes.Records),
		Methods:   recordsToMaps(methodsRes.Records),
	}, nil
}

func (q *QueryService) ExecuteCypher(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error) {
	return q.db.Query(ctx, cypher, params)
}

func (q *QueryService) fuzzyFilter(records []QueryRecord, term string, editDistance int, typeName string) []SearchResult {
	term = strings.ToLower(term)
	type scored struct {
		result SearchResult
		dist   int
	}
	var candidates []scored

	for _, r := range records {
		name, _ := r["name"].(string)
		if name == "" {
			continue
		}
		d := levenshtein(term, strings.ToLower(name))
		if d <= editDistance {
			sr := recordToResult(r, typeName)
			candidates = append(candidates, scored{sr, d})
		}
	}

	sort.Slice(candidates, func(i, j int) bool { return candidates[i].dist < candidates[j].dist })

	results := make([]SearchResult, 0, len(candidates))
	for _, c := range candidates {
		if len(results) >= 20 {
			break
		}
		results = append(results, c.result)
	}
	return results
}

func (q *QueryService) fullTextSearch(ctx context.Context, label, term string, editDistance int, repoPath, typeName string) ([]SearchResult, error) {
	cypher := buildNodeNameQuery(label, true, repoPath)
	params := map[string]any{"search_term": term}
	if repoPath != "" {
		params["repo_path"] = repoPath
	}
	res, err := q.db.Query(ctx, cypher, params)
	if err != nil {
		return nil, err
	}
	return recordsToResults(res.Records, typeName), nil
}

func buildNodeNameQuery(label string, fuzzy bool, repoPath string) string {
	repoFilter := ""
	if repoPath != "" {
		repoFilter = "AND node.path STARTS WITH $repo_path"
	}
	nameFilter := "node.name = $name"
	if fuzzy {
		nameFilter = "toLower(node.name) CONTAINS toLower($search_term)"
	}
	return fmt.Sprintf(`
		MATCH (node:%s)
		WHERE %s %s
		RETURN node.name as name, node.path as path, node.line_number as line_number,
		       node.docstring as docstring, node.is_dependency as is_dependency
		ORDER BY node.is_dependency ASC, node.name
		LIMIT 20`, label, nameFilter, repoFilter)
}

func recordsToResults(records []QueryRecord, typeName string) []SearchResult {
	results := make([]SearchResult, 0, len(records))
	for _, r := range records {
		results = append(results, recordToResult(r, typeName))
	}
	return results
}

func recordToResult(r QueryRecord, typeName string) SearchResult {
	sr := SearchResult{Type: typeName}
	if v, ok := r["name"].(string); ok {
		sr.Name = v
	}
	if v, ok := r["path"].(string); ok {
		sr.Path = v
	}
	if v, ok := r["line_number"]; ok {
		switch vv := v.(type) {
		case int:
			sr.Line = vv
		case int64:
			sr.Line = int(vv)
		case float64:
			sr.Line = int(vv)
		}
	}
	if v, ok := r["source"].(string); ok {
		sr.Source = v
	}
	if v, ok := r["docstring"].(string); ok {
		sr.Docstring = v
	}
	if v, ok := r["is_dependency"].(bool); ok {
		sr.IsDepend = v
	}
	return sr
}

func recordsToMaps(records []QueryRecord) []map[string]any {
	result := make([]map[string]any, 0, len(records))
	for _, r := range records {
		m := make(map[string]any, len(r))
		for k, v := range r {
			m[k] = v
		}
		result = append(result, m)
	}
	return result
}

func levenshtein(a, b string) int {
	if len(a) < len(b) {
		a, b = b, a
	}
	if b == "" {
		return len(a)
	}
	prev := make([]int, len(b)+1)
	for i := range prev {
		prev[i] = i
	}
	for i, c1 := range a {
		curr := make([]int, len(b)+1)
		curr[0] = i + 1
		for j, c2 := range b {
			insert := prev[j+1] + 1
			delete_ := curr[j] + 1
			replace := prev[j]
			if c1 != c2 {
				replace++
			}
			curr[j+1] = min3(insert, delete_, replace)
		}
		prev = curr
	}
	return prev[len(b)]
}

func min3(a, b, c int) int {
	if a < b {
		if a < c {
			return a
		}
		return c
	}
	if b < c {
		return b
	}
	return c
}

func (q *QueryService) FullTextSearch(ctx context.Context, query string, topK int) ([]SearchResult, error) {
	if q.searchIndex == nil {
		return nil, nil
	}
	return q.searchIndex.Search(query, topK)
}

func (q *QueryService) SemanticSearch(ctx context.Context, query string, topK int, cluster string) ([]SearchResult, error) {
	if q.embeddingClient == nil || q.searchIndex == nil {
		return nil, nil
	}

	var vec []float32
	var err error
	if qe, ok := q.embeddingClient.(ai.QueryEmbedder); ok {
		vec, err = qe.EmbedQuery(ctx, query)
	} else {
		vec, err = q.embeddingClient.Embed(ctx, query)
	}
	if err != nil {
		return nil, fmt.Errorf("embed query: %w", err)
	}

	if len(vec) < ai.EmbeddingDimensions {
		padded := make([]float32, ai.EmbeddingDimensions)
		copy(padded, vec)
		vec = padded
	} else if len(vec) > ai.EmbeddingDimensions {
		vec = vec[:ai.EmbeddingDimensions]
	}

	results, err := q.searchIndex.SemanticSearch(vec, topK)
	if err != nil {
		return nil, err
	}

	if topK > 0 && len(results) > topK {
		results = results[:topK]
	}

	return results, nil
}

func (q *QueryService) HybridSearch(ctx context.Context, query string, topK int) ([]SearchResult, error) {
	if q.searchIndex == nil {
		return nil, nil
	}

	var queryVec []float32
	if q.embeddingClient != nil {
		var err error
		if qe, ok := q.embeddingClient.(ai.QueryEmbedder); ok {
			queryVec, err = qe.EmbedQuery(ctx, query)
		} else {
			queryVec, err = q.embeddingClient.Embed(ctx, query)
		}
		if err != nil {
			queryVec = nil
		}

		if queryVec != nil {
			if len(queryVec) < ai.EmbeddingDimensions {
				padded := make([]float32, ai.EmbeddingDimensions)
				copy(padded, queryVec)
				queryVec = padded
			} else if len(queryVec) > ai.EmbeddingDimensions {
				queryVec = queryVec[:ai.EmbeddingDimensions]
			}
		}
	}

	return q.searchIndex.HybridSearch(query, queryVec, topK)
}

type AIClient interface {
	Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}

type AICypherRequest struct {
	UserQuery  string
	RepoPath   string
	MaxResults int
	Backend    string
}

type AICypherResponse struct {
	Cypher            string
	Keywords          []string
	PreSearchEntities []string
	Result            *QueryResult
	Error             string
}

func GenerateAICypher(ctx context.Context, db GraphDB, aiClient AIClient, req AICypherRequest) (*AICypherResponse, error) {

	gen := newCypherGenerator(db, aiClient)
	return gen.generate(ctx, req)
}

type cypherGen struct {
	db GraphDB
	ai AIClient
}

func newCypherGenerator(db GraphDB, aiClient AIClient) *cypherGen {
	return &cypherGen{db: db, ai: aiClient}
}

func (g *cypherGen) generate(ctx context.Context, req AICypherRequest) (*AICypherResponse, error) {

	if generateFunc == nil {
		return nil, fmt.Errorf("AI Cypher generator not registered — this is a bug")
	}
	return generateFunc(ctx, g.db, g.ai, req)
}

type GenerateAICypherFunc func(ctx context.Context, db GraphDB, ai AIClient, req AICypherRequest) (*AICypherResponse, error)

var generateFunc GenerateAICypherFunc

func RegisterAICypherGenerator(fn GenerateAICypherFunc) {
	generateFunc = fn
}
