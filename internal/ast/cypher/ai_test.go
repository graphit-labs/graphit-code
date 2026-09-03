package cypher

import (
	"context"
	"fmt"
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
	s := "```cypher\nMATCH (n) RETURN n\n```"
	got := stripCodeFence(s)
	if got != "MATCH (n) RETURN n" {
		t.Errorf("expected 'MATCH (n) RETURN n', got %q", got)
	}

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

	ctx := context.Background()
	kws, err := gen.expandKeywords(ctx, "payment billing")
	if err != nil {
		t.Fatalf("expandKeywords failed: %v", err)
	}
	if len(kws) != 2 || kws[0] != "payment" || kws[1] != "billing" {
		t.Errorf("unexpected keywords: %v", kws)
	}

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

func TestExpandKeywordsError(t *testing.T) {
	db := &mockGraphDB{}
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			return "", fmt.Errorf("ai unavailable")
		},
	}
	gen := NewGenerator(db, ai)
	_, err := gen.expandKeywords(context.Background(), "test")
	if err == nil {
		t.Error("expected error from AI client")
	}
}

func TestExpandKeywordsEmptyParts(t *testing.T) {
	db := &mockGraphDB{}
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			return `["foo", "", "bar", ""]`, nil
		},
	}
	gen := NewGenerator(db, ai)
	kws, err := gen.expandKeywords(context.Background(), "test")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(kws) != 2 || kws[0] != "foo" || kws[1] != "bar" {
		t.Errorf("expected [foo bar], got %v", kws)
	}
}

func TestPreSearchEdgeCases(t *testing.T) {
	ctx := context.Background()

	t.Run("empty keyword skipped", func(t *testing.T) {
		db := &mockGraphDB{}
		gen := NewGenerator(db, &mockAIClient{})
		entities := gen.preSearch(ctx, []string{""}, "/repo")
		if len(entities) != 0 {
			t.Errorf("expected no entities for empty keyword, got %v", entities)
		}
	})

	t.Run("DB query error skipped", func(t *testing.T) {
		db := &mockGraphDB{
			queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
				return nil, fmt.Errorf("db error")
			},
		}
		gen := NewGenerator(db, &mockAIClient{})
		entities := gen.preSearch(ctx, []string{"foo"}, "/repo")
		if len(entities) != 0 {
			t.Errorf("expected no entities on DB error, got %v", entities)
		}
	})

	t.Run("empty name in record skipped", func(t *testing.T) {
		db := &mockGraphDB{
			queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
				return &ast.QueryResult{
					Records: []ast.QueryRecord{
						{"name": "", "label": "Function"},
						{"name": "valid", "label": ""},
					},
				}, nil
			},
		}
		gen := NewGenerator(db, &mockAIClient{})
		entities := gen.preSearch(ctx, []string{"test"}, "")
		if len(entities) != 1 || entities[0] != "valid" {
			t.Errorf("expected [valid], got %v", entities)
		}
	})

	t.Run("no repoPath filter", func(t *testing.T) {
		db := &mockGraphDB{
			queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
				if strings.Contains(cypher, "STARTS WITH") {
					t.Error("should not contain repo filter when repoPath is empty")
				}
				return &ast.QueryResult{
					Records: []ast.QueryRecord{
						{"name": "fn1", "label": "Function"},
					},
				}, nil
			},
		}
		gen := NewGenerator(db, &mockAIClient{})
		entities := gen.preSearch(ctx, []string{"fn"}, "")
		if len(entities) != 1 {
			t.Errorf("expected 1 entity, got %v", entities)
		}
	})

	t.Run("dedup entities", func(t *testing.T) {
		db := &mockGraphDB{
			queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
				return &ast.QueryResult{
					Records: []ast.QueryRecord{
						{"name": "fn1", "label": "Function"},
					},
				}, nil
			},
		}
		gen := NewGenerator(db, &mockAIClient{})
		entities := gen.preSearch(ctx, []string{"fn", "fn"}, "")
		if len(entities) != 1 {
			t.Errorf("expected 1 deduped entity, got %v", entities)
		}
	})
}

func TestGenerateCypherVariations(t *testing.T) {
	ctx := context.Background()

	t.Run("non-ladybug backend", func(t *testing.T) {
		db := &mockGraphDB{
			queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
				return &ast.QueryResult{}, nil
			},
		}
		ai := &mockAIClient{
			completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
				if strings.Contains(systemPrompt, "LadybugDB dialect") {
					t.Error("should not contain LadybugDB notes for non-ladybug backend")
				}
				return "<cypher>MATCH (n) RETURN n</cypher>", nil
			},
		}
		gen := &Generator{db: db, ai: ai, backend: "neo4j"}
		result, err := gen.generateCypher(ctx, "find all", "schema", []string{"all"}, nil, "", 10)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if result != "MATCH (n) RETURN n" {
			t.Errorf("unexpected result: %q", result)
		}
	})

	t.Run("with entities and repoPath", func(t *testing.T) {
		db := &mockGraphDB{}
		ai := &mockAIClient{
			completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
				if !strings.Contains(systemPrompt, "Known entity names") {
					t.Error("expected grounding note with entities")
				}
				if !strings.Contains(systemPrompt, "current repository root") {
					t.Error("expected repo note")
				}
				return "<cypher>MATCH (n) RETURN n</cypher>", nil
			},
		}
		gen := &Generator{db: db, ai: ai, backend: "ladybug"}
		_, err := gen.generateCypher(ctx, "find", "schema", []string{"x"}, []string{"Function:foo"}, "/repo", 10)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
	})

	t.Run("AI error", func(t *testing.T) {
		db := &mockGraphDB{}
		ai := &mockAIClient{
			completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
				return "", fmt.Errorf("ai down")
			},
		}
		gen := &Generator{db: db, ai: ai, backend: "ladybug"}
		_, err := gen.generateCypher(ctx, "find", "schema", nil, nil, "", 10)
		if err == nil {
			t.Error("expected error from AI")
		}
	})

	t.Run("code fence in response", func(t *testing.T) {
		db := &mockGraphDB{}
		ai := &mockAIClient{
			completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
				return "<cypher>```cypher\nMATCH (n) RETURN n\n```</cypher>", nil
			},
		}
		gen := &Generator{db: db, ai: ai, backend: "ladybug"}
		result, err := gen.generateCypher(ctx, "find", "schema", nil, nil, "", 10)
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if result != "MATCH (n) RETURN n" {
			t.Errorf("expected stripped code fence, got %q", result)
		}
	})
}

func TestGenerateDefaultMaxResults(t *testing.T) {
	db := &mockGraphDB{
		queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
			return &ast.QueryResult{}, nil
		},
	}
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			if strings.Contains(systemPrompt, "expand user queries") {
				return `["test"]`, nil
			}
			return "<cypher>MATCH (n) RETURN n</cypher>", nil
		},
	}
	gen := NewGenerator(db, ai)
	resp, err := gen.Generate(context.Background(), QueryRequest{UserQuery: "test", MaxResults: 0})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.Cypher != "MATCH (n) RETURN n" {
		t.Errorf("unexpected cypher: %q", resp.Cypher)
	}
}

func TestGenerateWithQueryError(t *testing.T) {
	queryPhase := 0
	db := &mockGraphDB{
		queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
			if strings.Contains(cypher, "CONTAINS") {
				return &ast.QueryResult{}, nil
			}
			if strings.Contains(cypher, "show_tables") || strings.Contains(cypher, "table_info") {
				return &ast.QueryResult{}, nil
			}
			queryPhase++
			return nil, fmt.Errorf("query exec error")
		},
	}
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			if strings.Contains(systemPrompt, "expand user queries") {
				return `["test"]`, nil
			}
			return "<cypher>MATCH (n) RETURN n</cypher>", nil
		},
	}
	gen := NewGenerator(db, ai)
	resp, err := gen.Generate(context.Background(), QueryRequest{UserQuery: "test", MaxResults: 5})
	if err != nil {
		t.Fatalf("Generate should not error on query exec failure, got: %v", err)
	}
	if resp.Error != "query exec error" {
		t.Errorf("expected error in response.Error, got %q", resp.Error)
	}
}

func TestGenerateSchemaError(t *testing.T) {
	db := &mockGraphDB{
		queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
			if strings.Contains(cypher, "CONTAINS") {
				return &ast.QueryResult{}, nil
			}
			if strings.Contains(cypher, "show_tables") {
				return nil, fmt.Errorf("schema query error")
			}
			return &ast.QueryResult{}, nil
		},
	}
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			if strings.Contains(systemPrompt, "expand user queries") {
				return `["test"]`, nil
			}
			return "<cypher>MATCH (n) RETURN n</cypher>", nil
		},
	}
	gen := NewGenerator(db, ai)
	_, err := gen.Generate(context.Background(), QueryRequest{UserQuery: "test", MaxResults: 5})
	if err == nil {
		t.Error("expected error from schema generation")
	}
}

func TestStripCodeFenceShort(t *testing.T) {
	s := "```cypher\nMATCH (n)\n```"
	got := stripCodeFence(s)
	if got != "MATCH (n)" {
		t.Errorf("expected 'MATCH (n)', got %q", got)
	}

	s2 := "```\n```"
	got2 := stripCodeFence(s2)
	if got2 != "```\n```" {
		t.Errorf("expected no stripping for 2-line fence, got %q", got2)
	}

	s3 := "plain text"
	got3 := stripCodeFence(s3)
	if got3 != "plain text" {
		t.Errorf("expected 'plain text', got %q", got3)
	}
}

func TestSanitizeQueryEdgeCases(t *testing.T) {
	tests := []struct {
		name  string
		input string
		want  string
	}{
		{"trailing semicolons", "MATCH (n) RETURN n;;;", "MATCH (n) RETURN n"},
		{"carriage returns", "MATCH (n)\r\nRETURN n", "MATCH (n) RETURN n"},
		{"whitespace only", "  \n\t  ", ""},
		{"empty", "", ""},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			got := sanitizeQuery(tc.input)
			if got != tc.want {
				t.Errorf("sanitizeQuery(%q) = %q, want %q", tc.input, got, tc.want)
			}
		})
	}
}

func TestInitRegistration(t *testing.T) {
	db := &mockGraphDB{
		queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
			return &ast.QueryResult{}, nil
		},
	}
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			if strings.Contains(systemPrompt, "expand user queries") {
				return `["test"]`, nil
			}
			return "<cypher>MATCH (n) RETURN n</cypher>", nil
		},
	}

	resp, err := ast.GenerateAICypher(context.Background(), db, ai, ast.AICypherRequest{
		UserQuery:  "test",
		MaxResults: 5,
	})
	if err != nil {
		t.Fatalf("GenerateAICypher failed: %v", err)
	}
	if resp.Cypher != "MATCH (n) RETURN n" {
		t.Errorf("unexpected cypher: %q", resp.Cypher)
	}

	dbFail := &mockGraphDB{
		queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
			if strings.Contains(cypher, "show_tables") {
				return nil, fmt.Errorf("init error path")
			}
			return &ast.QueryResult{}, nil
		},
	}
	_, errInit := ast.GenerateAICypher(context.Background(), dbFail, ai, ast.AICypherRequest{
		UserQuery: "test",
	})
	if errInit == nil {
		t.Error("expected error from registered generator")
	}
}

func TestGenerateExpandKeywordsFallback(t *testing.T) {
	db := &mockGraphDB{
		queryFunc: func(ctx context.Context, cypher string, params map[string]any) (*ast.QueryResult, error) {
			return &ast.QueryResult{}, nil
		},
	}
	callNum := 0
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			callNum++
			if callNum == 1 {
				return "", fmt.Errorf("keyword expansion failed")
			}
			return "<cypher>MATCH (n) RETURN n</cypher>", nil
		},
	}
	gen := NewGenerator(db, ai)
	resp, err := gen.Generate(context.Background(), QueryRequest{UserQuery: "find payment", MaxResults: 5})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(resp.Keywords) != 1 {
		t.Errorf("expected 1 fallback keyword, got %d: %q", len(resp.Keywords), resp.Keywords)
	} else if resp.Keywords[0] != "find payment" {
		t.Errorf("expected keyword 'find payment', got %q", resp.Keywords[0])
	}
}
