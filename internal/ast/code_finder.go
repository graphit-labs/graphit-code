package ast

import (
	"context"
	"fmt"
	"math"
	"sort"
	"strings"
)

type CodeFinder struct {
	db GraphDB
}

func NewCodeFinder(db GraphDB) *CodeFinder {
	return &CodeFinder{db: db}
}

func levenshteinDistance(a, b string) int {
	if len(a) < len(b) {
		return levenshteinDistance(b, a)
	}
	if len(b) == 0 {
		return len(a)
	}
	prev := make([]int, len(b)+1)
	for j := range prev {
		prev[j] = j
	}
	for i := 0; i < len(a); i++ {
		curr := make([]int, len(b)+1)
		curr[0] = i + 1
		for j := 0; j < len(b); j++ {
			cost := 0
			if a[i] != b[j] {
				cost = 1
			}
			curr[j+1] = minOf3(prev[j+1]+1, curr[j]+1, prev[j]+cost)
		}
		prev = curr
	}
	return prev[len(b)]
}

func minOf3(a, b, c int) int {
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

type FindCodeResult struct {
	Name      string  `json:"name"`
	Path      string  `json:"path"`
	Line      int     `json:"line_number"`
	EndLine   int     `json:"end_line,omitempty"`
	Source    string  `json:"source,omitempty"`
	Docstring string  `json:"docstring,omitempty"`
	Type      string  `json:"type"`
	Lang      string  `json:"lang,omitempty"`
	Score     float64 `json:"score,omitempty"`
}

func (cf *CodeFinder) FindByFunctionName(ctx context.Context, name string, fuzzy bool, repoPath string) ([]FindCodeResult, error) {
	return cf.findByName(ctx, "Function", name, fuzzy, repoPath)
}

func (cf *CodeFinder) FindByClassName(ctx context.Context, name string, fuzzy bool, repoPath string) ([]FindCodeResult, error) {
	return cf.findByName(ctx, "Class", name, fuzzy, repoPath)
}

func (cf *CodeFinder) FindByVariableName(ctx context.Context, name string, repoPath string) ([]FindCodeResult, error) {
	return cf.findByName(ctx, "Variable", name, false, repoPath)
}

func (cf *CodeFinder) findByName(ctx context.Context, label, name string, fuzzy bool, repoPath string) ([]FindCodeResult, error) {
	if !fuzzy {

		q := fmt.Sprintf(
			`MATCH (n:%s) WHERE toLower(n.name) CONTAINS toLower($name) RETURN n.name AS name, n.path AS path, n.line_number AS line, n.end_line AS end_line, n.docstring AS doc, n.lang AS lang LIMIT 50`,
			label)
		res, err := cf.db.Execute(ctx, q, map[string]any{"name": name})
		if err != nil {
			return nil, err
		}
		return cf.recordsToResults(res.Records, strings.ToLower(label)), nil
	}

	q := fmt.Sprintf(`MATCH (n:%s) RETURN n.name AS name, n.path AS path, n.line_number AS line, n.end_line AS end_line, n.docstring AS doc, n.lang AS lang`, label)
	if repoPath != "" {
		q = fmt.Sprintf(`MATCH (n:%s) WHERE n.path STARTS WITH $repo RETURN n.name AS name, n.path AS path, n.line_number AS line, n.end_line AS end_line, n.docstring AS doc, n.lang AS lang`, label)
	}
	params := map[string]any{}
	if repoPath != "" {
		params["repo"] = repoPath
	}
	res, err := cf.db.Execute(ctx, q, params)
	if err != nil {
		return nil, err
	}

	searchLower := strings.ToLower(name)
	maxDist := int(math.Max(float64(len(name))/3, 2))

	var results []FindCodeResult
	for _, rec := range res.Records {
		n := fmt.Sprint(rec["name"])
		dist := levenshteinDistance(strings.ToLower(n), searchLower)
		if dist <= maxDist || strings.Contains(strings.ToLower(n), searchLower) {
			r := cf.recordToResult(rec, strings.ToLower(label))
			r.Score = float64(dist)
			results = append(results, r)
		}
	}

	sort.Slice(results, func(i, j int) bool {
		return results[i].Score < results[j].Score
	})
	if len(results) > 50 {
		results = results[:50]
	}
	return results, nil
}

func (cf *CodeFinder) FindRelatedCode(ctx context.Context, query string, fuzzy bool, repoPath string) (map[string][]FindCodeResult, error) {
	result := make(map[string][]FindCodeResult)

	fns, _ := cf.FindByFunctionName(ctx, query, fuzzy, repoPath)
	if len(fns) > 0 {
		result["functions"] = fns
	}

	cls, _ := cf.FindByClassName(ctx, query, fuzzy, repoPath)
	if len(cls) > 0 {
		result["classes"] = cls
	}

	vars, _ := cf.FindByVariableName(ctx, query, repoPath)
	if len(vars) > 0 {
		result["variables"] = vars
	}

	return result, nil
}

type DeadCodeResult struct {
	Name       string   `json:"name"`
	Path       string   `json:"path"`
	Line       int      `json:"line_number"`
	Decorators []string `json:"decorators,omitempty"`
	Lang       string   `json:"lang,omitempty"`
}

func (cf *CodeFinder) FindDeadCode(ctx context.Context, excludeDecorators []string, repoPath string) ([]DeadCodeResult, error) {
	q := `MATCH (f:Function)
		  WHERE NOT ()-[:CALLS]->(f)
		  AND NOT f.is_dependency = true`
	if repoPath != "" {
		q += ` AND f.path STARTS WITH $repo`
	}
	q += ` RETURN f.name AS name, f.path AS path, f.line_number AS line, f.decorators AS decs, f.lang AS lang
		   ORDER BY f.path, f.line_number
		   LIMIT 200`

	params := map[string]any{}
	if repoPath != "" {
		params["repo"] = repoPath
	}

	res, err := cf.db.Execute(ctx, q, params)
	if err != nil {
		return nil, err
	}

	excludeSet := make(map[string]bool)
	for _, d := range excludeDecorators {
		excludeSet[strings.ToLower(d)] = true
	}

	var results []DeadCodeResult
	for _, rec := range res.Records {

		if len(excludeSet) > 0 {
			if decs, ok := rec["decs"].([]any); ok {
				skip := false
				for _, d := range decs {
					if excludeSet[strings.ToLower(fmt.Sprint(d))] {
						skip = true
						break
					}
				}
				if skip {
					continue
				}
			}
		}

		results = append(results, DeadCodeResult{
			Name: fmt.Sprint(rec["name"]),
			Path: fmt.Sprint(rec["path"]),
			Line: cfToInt(rec["line"]),
			Lang: fmt.Sprint(rec["lang"]),
		})
	}

	return results, nil
}

type RelationshipResult struct {
	Callers      []FindCodeResult `json:"callers,omitempty"`
	Callees      []FindCodeResult `json:"callees,omitempty"`
	Parents      []FindCodeResult `json:"parents,omitempty"`
	Children     []FindCodeResult `json:"children,omitempty"`
	Implementors []FindCodeResult `json:"implementors,omitempty"`
}

func (cf *CodeFinder) AnalyzeRelationships(ctx context.Context, name, path, repoPath string) (*RelationshipResult, error) {
	result := &RelationshipResult{}

	q := `MATCH (caller)-[:CALLS]->(target {name: $name})
		  RETURN caller.name AS name, caller.path AS path, caller.line_number AS line, labels(caller)[0] AS type`
	if path != "" {
		q = `MATCH (caller)-[:CALLS]->(target {name: $name, path: $path})
			 RETURN caller.name AS name, caller.path AS path, caller.line_number AS line, labels(caller)[0] AS type`
	}
	params := map[string]any{"name": name}
	if path != "" {
		params["path"] = path
	}
	if res, err := cf.db.Execute(ctx, q, params); err == nil {
		result.Callers = cf.recordsToResults(res.Records, "function")
	}

	q2 := `MATCH (source {name: $name})-[:CALLS]->(target)
		   RETURN target.name AS name, target.path AS path, target.line_number AS line, labels(target)[0] AS type`
	if path != "" {
		q2 = `MATCH (source {name: $name, path: $path})-[:CALLS]->(target)
			  RETURN target.name AS name, target.path AS path, target.line_number AS line, labels(target)[0] AS type`
	}
	if res, err := cf.db.Execute(ctx, q2, params); err == nil {
		result.Callees = cf.recordsToResults(res.Records, "function")
	}

	q3 := `MATCH (child {name: $name})-[:INHERITS]->(parent)
		   RETURN parent.name AS name, parent.path AS path, parent.line_number AS line, labels(parent)[0] AS type`
	if res, err := cf.db.Execute(ctx, q3, params); err == nil {
		result.Parents = cf.recordsToResults(res.Records, "class")
	}

	q4 := `MATCH (child)-[:INHERITS]->(parent {name: $name})
		   RETURN child.name AS name, child.path AS path, child.line_number AS line, labels(child)[0] AS type`
	if res, err := cf.db.Execute(ctx, q4, params); err == nil {
		result.Children = cf.recordsToResults(res.Records, "class")
	}

	q5 := `MATCH (impl)-[:IMPLEMENTS]->(iface {name: $name})
		   RETURN impl.name AS name, impl.path AS path, impl.line_number AS line, labels(impl)[0] AS type`
	if res, err := cf.db.Execute(ctx, q5, params); err == nil {
		result.Implementors = cf.recordsToResults(res.Records, "class")
	}

	return result, nil
}

type ComplexityResult struct {
	Name       string `json:"name"`
	Path       string `json:"path"`
	Line       int    `json:"line_number"`
	Complexity int    `json:"cyclomatic_complexity"`
	Lang       string `json:"lang,omitempty"`
}

func (cf *CodeFinder) GetCyclomaticComplexity(ctx context.Context, name, path, repoPath string) (*ComplexityResult, error) {
	q := `MATCH (f:Function {name: $name})
		  RETURN f.name AS name, f.path AS path, f.line_number AS line, f.cyclomatic_complexity AS cc, f.lang AS lang
		  LIMIT 1`
	params := map[string]any{"name": name}
	if path != "" {
		q = `MATCH (f:Function {name: $name, path: $path})
			 RETURN f.name AS name, f.path AS path, f.line_number AS line, f.cyclomatic_complexity AS cc, f.lang AS lang
			 LIMIT 1`
		params["path"] = path
	}

	res, err := cf.db.Execute(ctx, q, params)
	if err != nil {
		return nil, err
	}
	if len(res.Records) == 0 {
		return nil, fmt.Errorf("function %q not found", name)
	}

	rec := res.Records[0]
	return &ComplexityResult{
		Name:       fmt.Sprint(rec["name"]),
		Path:       fmt.Sprint(rec["path"]),
		Line:       cfToInt(rec["line"]),
		Complexity: cfToInt(rec["cc"]),
		Lang:       fmt.Sprint(rec["lang"]),
	}, nil
}

func (cf *CodeFinder) FindMostComplexFunctions(ctx context.Context, limit int, repoPath string) ([]ComplexityResult, error) {
	q := `MATCH (f:Function)
		  WHERE f.cyclomatic_complexity IS NOT NULL AND f.cyclomatic_complexity > 1
		  RETURN f.name AS name, f.path AS path, f.line_number AS line, f.cyclomatic_complexity AS cc, f.lang AS lang
		  ORDER BY f.cyclomatic_complexity DESC
		  LIMIT $limit`
	params := map[string]any{"limit": limit}
	if repoPath != "" {
		q = `MATCH (f:Function)
			 WHERE f.cyclomatic_complexity IS NOT NULL AND f.cyclomatic_complexity > 1 AND f.path STARTS WITH $repo
			 RETURN f.name AS name, f.path AS path, f.line_number AS line, f.cyclomatic_complexity AS cc, f.lang AS lang
			 ORDER BY f.cyclomatic_complexity DESC
			 LIMIT $limit`
		params["repo"] = repoPath
	}

	res, err := cf.db.Execute(ctx, q, params)
	if err != nil {
		return nil, err
	}

	var results []ComplexityResult
	for _, rec := range res.Records {
		results = append(results, ComplexityResult{
			Name:       fmt.Sprint(rec["name"]),
			Path:       fmt.Sprint(rec["path"]),
			Line:       cfToInt(rec["line"]),
			Complexity: cfToInt(rec["cc"]),
			Lang:       fmt.Sprint(rec["lang"]),
		})
	}
	return results, nil
}

type RepoInfo struct {
	Path      string `json:"path"`
	Name      string `json:"name"`
	FileCount int    `json:"file_count"`
}

func (cf *CodeFinder) ListIndexedRepositories(ctx context.Context) ([]RepoInfo, error) {
	q := `MATCH (r:Directory) WHERE NOT ()-[:CONTAINS]->(r)
		  OPTIONAL MATCH (r)-[:CONTAINS*1..]->(f:File)
		  RETURN r.path AS path, r.name AS name, count(f) AS files
		  ORDER BY r.name`
	res, err := cf.db.Execute(ctx, q, nil)
	if err != nil {
		return nil, err
	}

	var repos []RepoInfo
	for _, rec := range res.Records {
		repos = append(repos, RepoInfo{
			Path:      fmt.Sprint(rec["path"]),
			Name:      fmt.Sprint(rec["name"]),
			FileCount: cfToInt(rec["files"]),
		})
	}
	return repos, nil
}

type RepoStats struct {
	Path           string `json:"path"`
	Files          int    `json:"files"`
	Functions      int    `json:"functions"`
	Classes        int    `json:"classes"`
	Variables      int    `json:"variables"`
	Modules        int    `json:"modules"`
	CallEdges      int    `json:"call_edges"`
	InheritEdges   int    `json:"inherit_edges"`
	ImplementEdges int    `json:"implement_edges"`
}

func (cf *CodeFinder) GetRepositoryStats(ctx context.Context, repoPath string) (*RepoStats, error) {
	stats := &RepoStats{Path: repoPath}

	counts := []struct {
		label string
		field *int
	}{
		{"File", &stats.Files},
		{"Function", &stats.Functions},
		{"Class", &stats.Classes},
		{"Variable", &stats.Variables},
		{"Module", &stats.Modules},
	}

	for _, c := range counts {
		q := fmt.Sprintf(`MATCH (n:%s) WHERE n.path STARTS WITH $repo RETURN count(n) AS cnt`, c.label)
		if c.label == "Module" {
			q = `MATCH (n:Module) RETURN count(n) AS cnt`
		}
		if res, err := cf.db.Execute(ctx, q, map[string]any{"repo": repoPath}); err == nil && len(res.Records) > 0 {
			*c.field = cfToInt(res.Records[0]["cnt"])
		}
	}

	edgeCounts := []struct {
		relType string
		field   *int
	}{
		{"CALLS", &stats.CallEdges},
		{"INHERITS", &stats.InheritEdges},
		{"IMPLEMENTS", &stats.ImplementEdges},
	}

	for _, ec := range edgeCounts {
		q := fmt.Sprintf(`MATCH (a)-[r:%s]->(b) WHERE a.path STARTS WITH $repo RETURN count(r) AS cnt`, ec.relType)
		if res, err := cf.db.Execute(ctx, q, map[string]any{"repo": repoPath}); err == nil && len(res.Records) > 0 {
			*ec.field = cfToInt(res.Records[0]["cnt"])
		}
	}

	return stats, nil
}

func (cf *CodeFinder) recordsToResults(records []QueryRecord, entityType string) []FindCodeResult {
	var results []FindCodeResult
	for _, rec := range records {
		results = append(results, cf.recordToResult(rec, entityType))
	}
	return results
}

func (cf *CodeFinder) recordToResult(rec QueryRecord, entityType string) FindCodeResult {
	return FindCodeResult{
		Name:      fmt.Sprint(rec["name"]),
		Path:      fmt.Sprint(rec["path"]),
		Line:      cfToInt(rec["line"]),
		EndLine:   cfToInt(rec["end_line"]),
		Source:    fmt.Sprint(rec["source"]),
		Docstring: fmt.Sprint(rec["doc"]),
		Type:      entityType,
		Lang:      fmt.Sprint(rec["lang"]),
	}
}

func cfToInt(v any) int {
	switch n := v.(type) {
	case int:
		return n
	case int64:
		return int(n)
	case float64:
		return int(n)
	case nil:
		return 0
	default:
		return 0
	}
}
