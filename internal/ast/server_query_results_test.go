package ast

import (
	"encoding/json"
	"net/http/httptest"
	"testing"
)

func TestVisualizerReadOnlyIgnoresQuotedText(t *testing.T) {
	for _, q := range []string{

		"MATCH (n:Import) RETURN n LIMIT 20",
		"MATCH (import:Import) RETURN DISTINCT import",
		"MATCH (import:Import) WITH DISTINCT import RETURN import",
		"MATCH (import:Import) RETURN 2 * import.count",
		"MATCH (set:Function) WHERE set.name = 'x' RETURN set.name AS import ORDER BY import",
		"MATCH (n) RETURN n.set, n.import, {delete: n.name}",
		"MATCH (n) WHERE n.name = $import RETURN n.name AS create",
		"MATCH (n:Function) WHERE n.name CONTAINS 'Create' RETURN n.name LIMIT 2",
		"MATCH (n) RETURN n.`SET`", "MATCH (n) RETURN 'can\\'t DELETE'",
		"MATCH (n) /* DELETE n */ RETURN n // CREATE",
		"MATCH (n) RETURN \"SET\"",
	} {
		if err := validateReadOnlyQuery(q); err != nil {
			t.Errorf("%s: %v", q, err)
		}
	}
	for _, q := range []string{"MATCH (n) DELETE n", "MATCH (n) SET n.name='x'", "/* safe */ CREATE (n)", "RETURN 'safe'; DROP TABLE Function", "MATCH (n) // safe\nDELETE n", "WITH 'safe' DELETE n", "MATCH (n) WITH n AS set SET set.name = 'x' RETURN set", "CALL { CREATE (n) } RETURN n", "FOREACH (n IN [] | SET n.name = 'x')", "MATCH (n) RETURN n; IMPORT DATABASE 'x'", "MATCH (n) WITH * DELETE n", "MATCH (n) WITH * SET n.name = 'x'", "MATCH (n) WITH * CREATE (m)", "MATCH (n) WITH DISTINCT * DELETE n", "MATCH (n) WITH *, n AS alias DELETE alias", "MATCH (n) WITH * /* comment */ DELETE n"} {
		if validateReadOnlyQuery(q) == nil {
			t.Errorf("allowed mutation: %s", q)
		}
	}
}
func TestVisualizerPreservesAliasesAndMixedTable(t *testing.T) {
	rec := map[string]any{"arbitraryAlias": map[string]any{"ID": map[string]any{"TableID": 0, "Offset": 7}, "Label": "Function", "Properties": map[string]any{"name": "Create"}}, "name": "Create"}
	nodes := map[string]map[string]any{}
	var edges []map[string]any
	extractUserQueryGraph(rec, nodes, &edges)
	if len(nodes) != 1 {
		t.Fatalf("nodes: %v", nodes)
	}
	cols, rows := collectTabularRow(rec, nil, nil, 0)
	w := httptest.NewRecorder()
	writeGraphResponse(w, nodes, edges, cols, rows, true)
	var response struct {
		Nodes   []any `json:"nodes"`
		Tabular struct {
			Columns []string `json:"columns"`
			Rows    [][]any  `json:"rows"`
		} `json:"tabular"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &response); err != nil {
		t.Fatal(err)
	}
	if len(response.Nodes) != 1 || len(response.Tabular.Columns) != 2 || len(response.Tabular.Rows) != 1 {
		t.Fatal(w.Body.String())
	}
	w = httptest.NewRecorder()
	writeGraphResponse(w, nil, nil, nil, nil, true)
	var empty map[string]any
	json.Unmarshal(w.Body.Bytes(), &empty)
	table, ok := empty["tabular"].(map[string]any)
	if !ok || table["rows"] == nil || table["columns"] == nil {
		t.Fatal(w.Body.String())
	}
}
