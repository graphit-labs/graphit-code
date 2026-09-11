package ast

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func canonicalMetadataManifest() *ladybug.CanonicalManifest {
	return &ladybug.CanonicalManifest{
		Version: ladybug.CanonicalManifestVersion, Format: "icebug-canonical", Finished: true,
		NodeTables: []ladybug.CanonicalNodeTable{
			{Label: "Function", Rows: 7, LangCounts: []ladybug.CanonicalLangCount{{Lang: "go", Rows: 5}, {Lang: "tsx", Rows: 2}}},
			{Label: "File", Rows: 3, LangCounts: []ladybug.CanonicalLangCount{{Lang: "go", Rows: 2}, {Lang: "tsx", Rows: 1}}},
			{Label: "Directory", Rows: 2, LangCounts: []ladybug.CanonicalLangCount{{Lang: "", Rows: 2}}},
		},
		EdgeCount: 12,
		RelGroups: []ladybug.CanonicalRelGroup{
			{Type: "CALLS", Members: []ladybug.CanonicalMember{{Rows: 8}}, ReverseMembers: []ladybug.CanonicalMember{{Rows: 8}}},
			{Type: "CONTAINS", Members: []ladybug.CanonicalMember{{Rows: 4}}, ReverseMembers: []ladybug.CanonicalMember{{Rows: 4}}},
		},
	}
}

func TestCanonicalStatsUseManifestWithoutReverseMirrors(t *testing.T) {
	stats := canonicalStatsFromManifest(canonicalMetadataManifest())
	if stats.NodeCount != 12 || stats.EdgeCount != 12 {
		t.Fatalf("totals = nodes %d edges %d, want 12 and 12", stats.NodeCount, stats.EdgeCount)
	}
	if len(stats.Nodes) != 3 || stats.Nodes[0] != (nodeTypeStat{Label: "Function", Count: 7}) {
		t.Fatalf("node stats = %#v", stats.Nodes)
	}
	if len(stats.Relationships) != 2 || stats.Relationships[0] != (relationshipTypeStat{Type: "CALLS", Count: 8}) {
		t.Fatalf("relationship stats = %#v", stats.Relationships)
	}
	if !stats.LangStatsComplete || len(stats.Langs) != 3 {
		t.Fatalf("language stats incomplete: %#v", stats.Langs)
	}
	if stats.Langs[0].Lang != "go" || stats.Langs[0].Count != 7 || stats.Langs[2].Lang != "" {
		t.Fatalf("language ordering/totals = %#v", stats.Langs)
	}
}

func TestCanonicalMetadataQueriesDoNotOpenLadybug(t *testing.T) {
	for _, tc := range []struct {
		name, query, column string
		want                any
	}{
		{"nodes variable", `MATCH (n) RETURN count(n) AS c`, "c", int64(12)},
		{"nodes star case whitespace", "  match (node)\nRETURN COUNT( * ) as Total ; ", "Total", int64(12)},
		{"typed nodes", `MATCH (f:Function) RETURN count(f) AS functions`, "functions", int64(7)},
		{"typed nodes star", "MATCH (f:`File`) RETURN count(*) AS files", "files", int64(3)},
		{"relationships", `MATCH ()-[edge]->() RETURN count(edge) AS edges`, "edges", int64(12)},
		{"relationships star", `MATCH ()-[r]->() RETURN count(*) AS c`, "c", int64(12)},
	} {
		t.Run(tc.name, func(t *testing.T) {
			db := &LadybugBackend{canonical: canonicalMetadataManifest()}
			result, err := db.Query(context.Background(), tc.query, nil)
			if err != nil {
				t.Fatal(err)
			}
			if db.conn != nil {
				t.Fatal("metadata query opened LadybugDB")
			}
			if got := result.Records[0][tc.column]; got != tc.want {
				t.Fatalf("%s = %#v, want %#v", tc.column, got, tc.want)
			}
		})
	}
}

func TestCanonicalMetadataGroupedQueriesPreserveAliases(t *testing.T) {
	for _, tc := range []struct {
		query, key, countKey string
		rows                 int
		first                string
	}{
		{`MATCH (n) RETURN DISTINCT label(n) AS kind, count(n) AS total ORDER BY total DESC`, "kind", "total", 3, "Function"},
		{`MATCH ()-[r]->() RETURN DISTINCT label(r) AS relation, count(r) AS amount ORDER BY amount DESC LIMIT 1`, "relation", "amount", 1, "CALLS"},
	} {
		result, handled := canonicalMetadataQuery(canonicalMetadataManifest(), tc.query)
		if !handled || len(result.Records) != tc.rows {
			t.Fatalf("query not handled or rows = %d: %#v", len(result.Records), result)
		}
		if result.Records[0][tc.key] != tc.first {
			t.Fatalf("first %s = %#v, want %q", tc.key, result.Records[0][tc.key], tc.first)
		}
		if _, ok := result.Records[0][tc.countKey]; !ok {
			t.Fatalf("count alias %q missing from %#v", tc.countKey, result.Records[0])
		}
	}
}

func TestCanonicalMetadataPlannerRejectsAmbiguousQueries(t *testing.T) {
	queries := []string{
		`MATCH (n) WHERE n.lang = 'go' RETURN count(n) AS c`,
		`MATCH (n) RETURN count(DISTINCT n) AS c`,
		`MATCH (n:Function) WHERE n.lang = 'go' RETURN count(n) AS c`,
		`MATCH (n:Class) RETURN count(n) AS classes`,
		`MATCH (n) RETURN count(n) AS c, n.lang AS lang`,
		`MATCH (n) RETURN count(n) AS c LIMIT 1`,
		`MATCH (n) RETURN count(n) AS c UNION MATCH (m) RETURN count(m) AS c`,
		`MATCH (n) SET n.seen = true RETURN count(n) AS c`,
		`MATCH (n) RETURN count(other) AS c`,
		`MATCH (node) RETURN count(Node) AS c`,
		`MATCH (n:function) RETURN count(n) AS c`,
		`MATCH ()-[r:CALLS]->() RETURN count(r) AS c`,
	}
	for _, query := range queries {
		if result, handled := canonicalMetadataQuery(canonicalMetadataManifest(), query); handled {
			t.Errorf("unsafe query was handled: %q => %#v", query, result)
		}
	}
}

type statsOnlyGraphDB struct {
	emptyGraphDB
	stats   canonicalStats
	queries int
}

func (d *statsOnlyGraphDB) Query(context.Context, string, map[string]any) (*QueryResult, error) {
	d.queries++
	return nil, errors.New("query must not be called")
}

func (d *statsOnlyGraphDB) canonicalGraphStats() (canonicalStats, bool) { return d.stats, true }

func TestContextsUsesCanonicalStatsWithoutQuery(t *testing.T) {
	root := t.TempDir()
	projectStore, err := store.EnsureASTProjectDir(root)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(projectStore, 0o755); err != nil {
		t.Fatal(err)
	}
	db := &statsOnlyGraphDB{stats: canonicalStats{NodeCount: 7_368_281, EdgeCount: 18_559_493}}
	s := &Server{db: db, repoPath: root, dbCache: map[string]*cachedDB{}}
	rec := httptest.NewRecorder()
	s.handleContexts(rec, httptest.NewRequest(http.MethodGet, "/api/contexts", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status %d: %s", rec.Code, rec.Body.String())
	}
	if db.queries != 0 {
		t.Fatalf("handler executed %d graph queries", db.queries)
	}
	if body := rec.Body.String(); !containsAll(body, `"node_count":7368281`, `"edge_count":18559493`) {
		t.Fatalf("manifest totals missing: %s", body)
	}
}

func TestSchemaUsesCanonicalStatsWithoutQuery(t *testing.T) {
	db := &statsOnlyGraphDB{stats: canonicalStatsFromManifest(canonicalMetadataManifest())}
	s := &Server{db: db, repoPath: t.TempDir()}
	rec := httptest.NewRecorder()
	s.handleSchema(rec, httptest.NewRequest(http.MethodGet, "/api/schema", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status %d: %s", rec.Code, rec.Body.String())
	}
	if db.queries != 0 {
		t.Fatalf("schema executed %d graph queries", db.queries)
	}
	if body := rec.Body.String(); !containsAll(body, `"label":"Function"`, `"type":"CALLS"`, `"lang":"go"`) {
		t.Fatalf("canonical schema stats missing: %s", body)
	}
}

type countFallbackDB struct {
	emptyGraphDB
	queries []string
}

func (d *countFallbackDB) Query(_ context.Context, query string, _ map[string]any) (*QueryResult, error) {
	d.queries = append(d.queries, query)
	count := int64(21)
	if len(d.queries) == 2 {
		count = 34
	}
	return &QueryResult{Records: []QueryRecord{{"c": count}}}, nil
}

func TestGraphCountsFallsBackForNonCanonicalBackend(t *testing.T) {
	db := &countFallbackDB{}
	nodes, edges := graphCounts(context.Background(), db)
	if nodes != 21 || edges != 34 || len(db.queries) != 2 {
		t.Fatalf("fallback = nodes %d edges %d queries %#v", nodes, edges, db.queries)
	}
}

func containsAll(value string, fragments ...string) bool {
	for _, fragment := range fragments {
		if !strings.Contains(value, fragment) {
			return false
		}
	}
	return true
}
