package ast

import "testing"

func TestGraphNodeLabelIsTheGraphLabel(t *testing.T) {
	for _, tc := range []struct {
		name                              string
		label, entity, path               string
		wantName, wantLabel, wantFilePath string
	}{
		{
			name:  "named entity",
			label: "Function", entity: "handleFile", path: "internal/ast/server.go",
			wantName: "handleFile", wantLabel: "Function", wantFilePath: "internal/ast/server.go",
		},
		{
			name:  "file node",
			label: "File", entity: "server.go", path: "internal/ast/server.go",
			wantName: "server.go", wantLabel: "File", wantFilePath: "internal/ast/server.go",
		},
		{
			name:  "file node without a path falls back to its name",
			label: "File", entity: "server.go", path: "",
			wantName: "server.go", wantLabel: "File", wantFilePath: "server.go",
		},
		{
			name:  "nameless node displays its path",
			label: "Directory", entity: "", path: "internal/ast",
			wantName: "internal/ast", wantLabel: "Directory", wantFilePath: "internal/ast",
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			n := buildGraphNode(graphNodeSide{
				id: "0:1", label: tc.label, name: tc.entity, path: tc.path,
				cluster: "backend", lang: "go",
			})

			if got := n["label"]; got != tc.wantLabel {
				t.Errorf("label = %v, want %v — the graph label, not the display name", got, tc.wantLabel)
			}
			if got := n["type"]; got != tc.wantLabel {
				t.Errorf("type = %v, want %v", got, tc.wantLabel)
			}
			if got := n["name"]; got != tc.wantName {
				t.Errorf("name = %v, want %v", got, tc.wantName)
			}
			if got := n["file"]; got != tc.wantFilePath {
				t.Errorf("file = %v, want %v", got, tc.wantFilePath)
			}

			props, ok := n["properties"].(map[string]any)
			if !ok {
				t.Fatalf("properties = %T, want map", n["properties"])
			}
			if props["cluster"] != "backend" || props["lang"] != "go" {
				t.Errorf("properties = %v, want cluster=backend lang=go", props)
			}
		})
	}
}

// TestUserQueryGraphNodeLabelIsTheGraphLabel pins the same contract on the other
// builder — the one that shapes nodes returned by a Cypher query typed into the
// query bar. It had the identical inversion.
func TestUserQueryGraphNodeLabelIsTheGraphLabel(t *testing.T) {
	rec := map[string]any{
		"n": map[string]any{
			"ID":         map[string]any{"TableID": 0, "Offset": 7},
			"Label":      "Function",
			"Properties": map[string]any{"name": "handleFile", "path": "internal/ast/server.go"},
		},
	}

	nodes := map[string]map[string]any{}
	var edges []map[string]any
	extractUserQueryGraph(rec, nodes, &edges)

	n, ok := nodes["0:7"]
	if !ok {
		t.Fatalf("node 0:7 not extracted, got %v", nodes)
	}
	if n["label"] != "Function" {
		t.Errorf("label = %v, want Function — the graph label, not the display name", n["label"])
	}
	if n["type"] != "Function" {
		t.Errorf("type = %v, want Function", n["type"])
	}
	if n["name"] != "handleFile" {
		t.Errorf("name = %v, want handleFile", n["name"])
	}
}

// Clicking an entity opens its file AT its declaration, so the line has to reach the
// explorer. It travels as a top-level `line`, which is where the panel reads it —
// leaving it buried in `properties` is the same as not sending it.
//
// Line 0 must NOT be sent: that is the call-target stub's placeholder, not a
// location. A node built from one has no declaration to open, and a 0 would be
// indistinguishable from "no line" only by accident.
func TestGraphNodeCarriesItsDeclarationLine(t *testing.T) {
	t.Run("entity with a line", func(t *testing.T) {
		n := buildGraphNode(graphNodeSide{
			id: "0:1", label: "Function", name: "handleFile",
			path: "internal/ast/server.go", line: 441,
		})

		if n["line"] != 441 {
			t.Errorf("line = %v, want 441 — without it the file opens at the top and "+
				"the user has to hunt for the entity", n["line"])
		}
		props, _ := n["properties"].(map[string]any)
		if props["line_number"] != 441 {
			t.Errorf("properties.line_number = %v, want 441 — the details panel reads "+
				"the raw properties", props["line_number"])
		}
	})

	t.Run("stub and file nodes carry no line", func(t *testing.T) {
		for _, side := range []graphNodeSide{
			{id: "0:2", label: "Function", name: "Apply", line: 0},
			{id: "0:3", label: "File", name: "server.go", path: "internal/ast/server.go"},
		} {
			n := buildGraphNode(side)
			if _, has := n["line"]; has {
				t.Errorf("%s node carries line %v: the explorer would jump to line 0 "+
					"and mark a line that means nothing", side.label, n["line"])
			}
		}
	})
}

// The line has to survive the row reader too, on both ends of an edge — the far
// endpoint is an entity the user clicks just as often as the near one.
func TestBothEndsOfARowKeepTheirLine(t *testing.T) {
	nodes := map[string]map[string]any{}
	var edges []map[string]any

	extractBuiltinQueryGraph(map[string]any{
		"src_id": "0:1", "src_label": "File", "src_name": "server.go",
		"src_path": "internal/ast/server.go", "src_line": int64(0),
		"dst_id": "0:2", "dst_label": "Function", "dst_name": "handleGraph",
		"dst_path": "internal/ast/server.go", "dst_line": int64(441),
		"rel_type": "CONTAINS",
	}, nodes, &edges)

	if _, has := nodes["0:1"]["line"]; has {
		t.Errorf("the file node got a line: %v", nodes["0:1"]["line"])
	}
	if nodes["0:2"]["line"] != 441 {
		t.Errorf("the far endpoint lost its line (%v): clicking it would open the "+
			"file at the top", nodes["0:2"]["line"])
	}
}

// A node returned by a typed Cypher query arrives with raw properties instead of
// prefixed columns, and gets the same treatment — otherwise the jump works from the
// default view and silently stops working the moment the user runs their own query.
func TestUserQueryNodeCarriesItsLine(t *testing.T) {
	nodes := map[string]map[string]any{}
	var edges []map[string]any

	extractUserQueryGraph(map[string]any{
		"n": map[string]any{
			"ID":    map[string]any{"TableID": 0, "Offset": 7},
			"Label": "Function",
			"Properties": map[string]any{
				"name": "handleFile", "path": "internal/ast/server.go", "line_number": int64(441),
			},
		},
	}, nodes, &edges)

	if nodes["0:7"]["line"] != 441 {
		t.Errorf("line = %v, want 441 — the property was there and was not lifted",
			nodes["0:7"]["line"])
	}
}
