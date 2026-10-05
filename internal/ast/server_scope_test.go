package ast

import (
	"context"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/store"
)

type selectedScopeDB struct {
	emptyGraphDB
	queries int
}

func (db *selectedScopeDB) Query(context.Context, string, map[string]any) (*QueryResult, error) {
	db.queries++
	return &QueryResult{}, nil
}

func (db *selectedScopeDB) Execute(ctx context.Context, query string, params map[string]any) (*QueryResult, error) {
	return db.Query(ctx, query, params)
}

func TestGlobalASTUsesSelectedProject(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	project := t.TempDir()
	if err := os.WriteFile(filepath.Join(project, brand.LockFileName()), []byte(`{"project":{"id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	dir := store.ASTProjectDir(project)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	db := &selectedScopeDB{}
	s := &Server{db: NewEmptyGraphDB(), repoPath: brand.GlobalDir(), dbCache: map[string]*cachedDB{
		project + "\x00" + dir: {db: db, lastUsed: time.Now()},
	}}
	mux := http.NewServeMux()
	s.RegisterAPIRoutes(mux)
	for _, path := range []string{"find_code?name=f", "dead_code?", "relationships?name=f", "complexity?name=f", "most_complex?", "repositories?", "stats?"} {
		t.Run(path, func(t *testing.T) {
			db.queries = 0
			rec := httptest.NewRecorder()
			mux.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/api/"+path+"&project_dir="+url.QueryEscape(project), nil))
			if db.queries == 0 {
				t.Fatalf("selected database was ignored: status=%d body=%s", rec.Code, rec.Body.String())
			}
		})
	}
	db.queries = 0
	rec := httptest.NewRecorder()
	mux.ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/api/export/obsidian?project_dir="+url.QueryEscape(project), strings.NewReader(`{}`)))
	if rec.Code != http.StatusOK || db.queries == 0 {
		t.Fatalf("export ignored selected database: %d %s", rec.Code, rec.Body.String())
	}
	index, err := os.ReadFile(filepath.Join(project, brand.DotDir(), "ast", "export", "index.md"))
	if err != nil || !strings.Contains(string(index), "repo: "+project) {
		t.Fatalf("export did not use selected project root: %v", err)
	}
}
