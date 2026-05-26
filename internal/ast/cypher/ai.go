package cypher

import (
	"context"
	"fmt"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
)

type AIClient interface {
	Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}

type QueryRequest struct {
	UserQuery string

	RepoPath string

	MaxResults int

	Backend string
}

type QueryResponse struct {
	Cypher string

	Keywords []string

	PreSearchEntities []string

	Result *ast.QueryResult

	Error string
}

type Generator struct {
	db      ast.GraphDB
	ai      AIClient
	backend string
}

func NewGenerator(db ast.GraphDB, ai AIClient) *Generator {
	return &Generator{db: db, ai: ai, backend: db.BackendType()}
}

func init() {
	ast.RegisterAICypherGenerator(func(
		ctx context.Context, db ast.GraphDB, aiClient ast.AIClient, req ast.AICypherRequest,
	) (*ast.AICypherResponse, error) {
		gen := NewGenerator(db, aiClient)
		resp, err := gen.Generate(ctx, QueryRequest{
			UserQuery:  req.UserQuery,
			RepoPath:   req.RepoPath,
			MaxResults: req.MaxResults,
			Backend:    req.Backend,
		})
		if err != nil {
			return nil, err
		}
		return &ast.AICypherResponse{
			Cypher:            resp.Cypher,
			Keywords:          resp.Keywords,
			PreSearchEntities: resp.PreSearchEntities,
			Result:            resp.Result,
			Error:             resp.Error,
		}, nil
	})
}

func (g *Generator) Generate(ctx context.Context, req QueryRequest) (*QueryResponse, error) {
	if req.MaxResults == 0 {
		req.MaxResults = 25
	}

	keywords, err := g.expandKeywords(ctx, req.UserQuery)
	if err != nil {

		keywords = []string{req.UserQuery}
	}

	entities := g.preSearch(ctx, keywords, req.RepoPath)

	schema, err := g.buildSchemaPrompt(ctx)
	if err != nil {
		return nil, fmt.Errorf("schema generation: %w", err)
	}
	cypher, err := g.generateCypher(ctx, req.UserQuery, schema, keywords, entities, req.RepoPath, req.MaxResults)
	if err != nil {
		return nil, fmt.Errorf("cypher generation: %w", err)
	}

	cypher = sanitizeQuery(cypher)

	resp := &QueryResponse{
		Cypher:            cypher,
		Keywords:          keywords,
		PreSearchEntities: entities,
	}

	result, execErr := g.db.Query(ctx, cypher, map[string]any{})
	if execErr != nil {
		resp.Error = execErr.Error()
	} else {
		resp.Result = result
	}

	return resp, nil
}

const keywordSystemPrompt = `You are a code search assistant that expands user queries into relevant keywords.

For the given query, identify:
1. The technical terms (function names, class names, module names)
2. Domain-specific synonyms (e.g., "CPF" → also "CNPJ", "CGC", "tax_id")
3. Common abbreviations used in codebases

Return ONLY a JSON array of strings with no explanation.
Example: ["payment", "charge", "billing", "invoice"]`

func (g *Generator) expandKeywords(ctx context.Context, query string) ([]string, error) {
	response, err := g.ai.Complete(ctx, keywordSystemPrompt, query)
	if err != nil {
		return nil, err
	}

	response = strings.TrimSpace(response)
	if !strings.HasPrefix(response, "[") {

		terms := strings.Split(query, " ")
		return terms, nil
	}

	response = strings.Trim(response, "[]")
	parts := strings.Split(response, ",")
	keywords := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.TrimSpace(p)
		p = strings.Trim(p, `"'`)
		if p != "" {
			keywords = append(keywords, p)
		}
	}
	return keywords, nil
}

func (g *Generator) preSearch(ctx context.Context, keywords []string, repoPath string) []string {
	seen := map[string]bool{}
	var entities []string

	for _, kw := range keywords {
		if kw == "" {
			continue
		}

		repoFilter := ""
		params := map[string]any{"kw": strings.ToLower(kw)}
		if repoPath != "" {
			repoFilter = "AND n.path STARTS WITH $repo_path"
			params["repo_path"] = repoPath
		}

		cypher := fmt.Sprintf(`
			MATCH (n)
			WHERE toLower(n.name) CONTAINS $kw %s
			RETURN DISTINCT n.name as name, label(n) as label`, repoFilter)

		res, err := g.db.Query(ctx, cypher, params)
		if err != nil {
			continue
		}

		for _, r := range res.Records {
			name, _ := r["name"].(string)
			lbl, _ := r["label"].(string)
			if name == "" {
				continue
			}
			entry := name
			if lbl != "" {
				entry = lbl + ":" + name
			}
			if !seen[entry] {
				seen[entry] = true
				entities = append(entities, entry)
			}
		}
	}

	return entities
}

func (g *Generator) generateCypher(
	ctx context.Context,
	userQuery, schema string,
	keywords, entities []string,
	repoPath string,
	maxResults int,
) (string, error) {
	dialectNote := ""
	if g.backend == "ladybug" {
		dialectNote = `
LadybugDB dialect notes:
- Use label(n) instead of labels(n)[0]
- No ON CREATE SET / ON MATCH SET
- WHERE n:Label → label(n) = 'Label'`
	}

	groundingNote := ""
	if len(entities) > 0 {
		groundingNote = fmt.Sprintf(`
Known entity names in the graph (use these in MATCH clauses when relevant):
%s`, strings.Join(entities, ", "))
	}

	repoNote := ""
	if repoPath != "" {
		repoNote = fmt.Sprintf("\nThe current repository root is: %s (use this only if strictly necessary to filter by path)", repoPath)
	}

	systemPrompt := fmt.Sprintf(`You are a Cypher query expert for a code knowledge graph.

Graph Schema:
%s
%s%s%s

Rules:
1. ALWAYS use n-[:REL]-m patterns (never label-less MATCH) and return the full nodes and relationships (e.g., RETURN n, r, m) to support visualization.
2. DO NOT include a LIMIT clause unless the user explicitly asks for it.
3. Only reference node labels and properties that exist in the schema.
4. LadybugDB strict typing: DO NOT access properties (like n.source or n.value) unless you explicitly MATCH the label that contains them (e.g., (n:Function)). If a property is not shared by ALL possible labels in a pattern, LadybugDB will crash!
5. When searching by name, use: toLower(n.name) CONTAINS toLower('term')
6. Never hallucinate node/property names — only use what is in the schema.
7. ALWAYS write the entire query on a SINGLE LINE without any newline characters (\n). Use spaces before WHERE, RETURN, and LIMIT keywords.
8. Do not make the query overly complex with OR conditions unless explicitly asked.
9. ALWAYS wrap your final Cypher query inside <cypher>...</cypher> tags. Do not add any other text.`,
		schema, dialectNote, groundingNote, repoNote)

	userPrompt := fmt.Sprintf(`User question: %s

Keywords to consider: %s

Generate a single Cypher query.`, userQuery, strings.Join(keywords, ", "))

	response, err := g.ai.Complete(ctx, systemPrompt, userPrompt)
	if err != nil {
		return "", err
	}

	startIdx := strings.Index(response, "<cypher>")
	endIdx := strings.LastIndex(response, "</cypher>")
	if startIdx == -1 || endIdx == -1 || startIdx >= endIdx {
		return "", fmt.Errorf("AI response did not contain a valid <cypher> wrapper. Response was: %s", response)
	}

	response = response[startIdx+len("<cypher>") : endIdx]
	response = strings.TrimSpace(response)

	response = stripCodeFence(response)

	return response, nil
}

func (g *Generator) buildSchemaPrompt(ctx context.Context) (string, error) {
	return ast.SchemaText(ctx, g.db)
}

func sanitizeQuery(q string) string {
	q = strings.TrimSpace(q)
	q = stripCodeFence(q)

	q = strings.TrimRight(q, ";")

	q = strings.ReplaceAll(q, "\n", " ")
	q = strings.ReplaceAll(q, "\r", "")
	return strings.TrimSpace(q)
}

func stripCodeFence(s string) string {
	if strings.HasPrefix(s, "```") {
		lines := strings.Split(s, "\n")
		if len(lines) > 2 {

			s = strings.Join(lines[1:len(lines)-1], "\n")
		}
	}
	return strings.TrimSpace(s)
}
