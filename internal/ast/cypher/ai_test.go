package cypher

import (
	"context"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/ast"
)

type mockGraphDB struct {
	queryFunc func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error)
}

func (m *mockGraphDB) Query(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
	if m.queryFunc != nil {
		return m.queryFunc(ctx, cypher, params)
	}
	return &ast.QueryResult{}, nil
}

func (m *mockGraphDB) Execute(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
	return &ast.QueryResult{}, nil
}

func (m *mockGraphDB) ExecuteBatch(ctx context.Context, queries []ast.BatchQuery) error {
	return nil
}

func (m *mockGraphDB) Ping(ctx context.Context) error {
	return nil
}

func (m *mockGraphDB) BackendType() string {
	return "ladybug"
}

func (m *mockGraphDB) Close() error {
	return nil
}

type mockAIClient struct {
	completeFunc func(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}

func (m *mockAIClient) Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	if m.completeFunc != nil {
		return m.completeFunc(ctx, systemPrompt, userPrompt)
	}
	return "mocked response", nil
}

func TestSanitizeAndFences(t *testing.T) {
	// stripCodeFence
	s := "```cypher\nMATCH (n) RETURN n\n```"
	got := stripCodeFence(s)
	if got != "MATCH (n) RETURN n" {
		t.Errorf("expected 'MATCH (n) RETURN n', got %q", got)
	}

	// sanitizeQuery
	q := "```cypher\nMATCH (n)\nRETURN n;\n```"
	gotSanitized := sanitizeQuery(q)
	if gotSanitized != "MATCH (n) RETURN n" {
		t.Errorf("expected 'MATCH (n) RETURN n', got %q", gotSanitized)
	}
}

func TestExpandKeywords(t *testing.T) {
	db := &mockGraphDB{}
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			return `["payment", "billing"]`, nil
		},
	}
	gen := NewGenerator(db, ai)

	// JSON response
	ctx := context.Background()
	kws, err := gen.expandKeywords(ctx, "payment billing")
	if err != nil {
		t.Fatalf("expandKeywords failed: %v", err)
	}
	if len(kws) != 2 || kws[0] != "payment" || kws[1] != "billing" {
		t.Errorf("unexpected keywords: %v", kws)
	}

	// Plain text fallback
	ai.completeFunc = func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
		return "payment billing", nil
	}
	kwsFallback, err := gen.expandKeywords(ctx, "payment billing")
	if err != nil {
		t.Fatalf("expandKeywords failed: %v", err)
	}
	if len(kwsFallback) != 2 || kwsFallback[0] != "payment" || kwsFallback[1] != "billing" {
		t.Errorf("unexpected fallback keywords: %v", kwsFallback)
	}
}

func TestGeneratorGenerate(t *testing.T) {
	db := &mockGraphDB{
		queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
			if strings.Contains(cypher, "CONTAINS") {
				// preSearch query
				return &ast.QueryResult{
					Records: []ast.QueryRecord{
						{"name": "my_function", "label": "Function"},
					},
				}, nil
			}
			return &ast.QueryResult{}, nil
		},
	}

	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			if strings.Contains(systemPrompt, "expand user queries") {
				return `["payment"]`, nil
			}
			return "<cypher>MATCH (n:Function) RETURN n</cypher>", nil
		},
	}

	gen := NewGenerator(db, ai)
	ctx := context.Background()
	resp, err := gen.Generate(ctx, QueryRequest{
		UserQuery:  "find payment function",
		RepoPath:   "/my/repo",
		MaxResults: 10,
	})
	if err != nil {
		t.Fatalf("Generate failed: %v", err)
	}

	if resp.Cypher != "MATCH (n:Function) RETURN n" {
		t.Errorf("unexpected Cypher output: %q", resp.Cypher)
	}
	if len(resp.PreSearchEntities) != 1 || resp.PreSearchEntities[0] != "Function:my_function" {
		t.Errorf("unexpected PreSearchEntities: %v", resp.PreSearchEntities)
	}

	// Test error in ai complete (missing cypher tag)
	ai.completeFunc = func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
		if strings.Contains(systemPrompt, "expand user queries") {
			return `["payment"]`, nil
		}
		return "no tag here", nil
	}
	_, err = gen.Generate(ctx, QueryRequest{UserQuery: "find payment"})
	if err == nil {
		t.Error("expected error due to missing <cypher> tag")
	}
}
