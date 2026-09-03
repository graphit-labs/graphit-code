package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// `MATCH (n:Import)` was an error in a Go repository with 2838 imports, because
// ConvertToCache built the entity and threw it away: the imports branch ended in
// `continue`. The IMPORTS edge to the canonical Module was the only thing kept, so
// nothing recorded where a statement actually sits.
//
// Both survive now, and they answer different questions — the edge says what the file
// depends on, the entity says where the import is written.
func TestConvertToCacheStoresImportsAsEntitiesAndEdges(t *testing.T) {
	t.Parallel()

	pd := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	pf := parseFixture(t, pd, "main.go", `package main

import (
	"fmt"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func main() { fmt.Println(brand.Name) }
`)

	entry := ConvertToCache(pf, pd, false, "")

	if len(entry.Imports) < 2 {
		t.Fatalf("expected the IMPORTS records to survive, got %d", len(entry.Imports))
	}

	byName := map[string]cachedEntity{}
	for _, e := range entry.Entities {
		if e.Label == LabelImport {
			byName[e.Name] = e
		}
	}
	if len(byName) == 0 {
		t.Fatal("no Import entity reached the cache — the entity is still being dropped")
	}

	for _, want := range []string{"fmt", "github.com/graphit-labs/graphit-code/internal/brand"} {
		e, ok := byName[want]
		if !ok {
			t.Errorf("no Import entity for %q; got %v", want, importNames(byName))
			continue
		}
		if e.Line <= 0 {
			t.Errorf("Import %q has no line number", want)
		}
		if e.Path == "" {
			t.Errorf("Import %q has no path", want)
		}
	}

	for _, e := range entry.Entities {
		if e.Label == LabelModule {
			t.Errorf("an import became a Module entity, which collides with the canonical module node: %+v", e)
		}
	}
}

func TestConvertToCacheDropsNoDeclaredEntity(t *testing.T) {
	t.Parallel()

	pf := &ParsedFile{
		Path:     "x.go",
		Language: "go",
		Entities: map[string][]Entity{
			"functions": {{Name: "Run", GraphLabel: "Function", Line: 1}},
			"imports":   {{Name: "fmt", GraphLabel: "Import", Line: 2}},
			"structs":   {{Name: "Cfg", GraphLabel: "Struct", Line: 3}},
			"comments":  {{Name: "// note", GraphLabel: "Comment", Line: 4}},
			"fields":    {{Name: "Timeout", GraphLabel: "Field", Line: 5, Context: "Cfg", ContextType: "struct"}},
			"parameters": {
				{Name: "ctx", GraphLabel: "Parameter", Line: 1, Context: "Run", ContextType: "function"},
				{Name: "orphan", GraphLabel: "Parameter", Line: 9},
			},
		},
	}

	entry := ConvertToCache(pf, ".", false, "")

	got := map[string]bool{}
	for _, e := range entry.Entities {
		got[e.Label+"/"+e.Name] = true
	}

	for _, want := range []string{
		"Function/Run",
		"Import/fmt",
		"Struct/Cfg",
		"Comment/// note",
		"Field/Timeout",
		"Parameter/ctx",
	} {
		if !got[want] {
			t.Errorf("declared entity %q never reached the cache", want)
		}
	}

	if got["Parameter/orphan"] {
		t.Error("a parameter with no owner should not be stored — it has nothing to attach to")
	}
}

func importNames(m map[string]cachedEntity) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}

// End to end, against a real graph: the cache-level tests above prove the entity is
// built, this proves it reaches the database and is reachable from its File — which is
// the query that was an error.
func TestPipelineWritesImportNodesToTheGraph(t *testing.T) {
	work := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")

	src := `package sample

import (
	"fmt"
	"strings"
)

func Shout(s string) string { return strings.ToUpper(fmt.Sprint(s)) }
`
	if err := os.WriteFile(filepath.Join(work, "sample.go"), []byte(src), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	tmp := t.TempDir()
	dbPath := filepath.Join(tmp, "ladybugdb")
	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	if _, err := RunPipeline(context.Background(), db, work, PipelineOptions{
		CacheDir: filepath.Join(tmp, "cache"),
	}); err != nil {
		_ = db.Close()
		t.Fatalf("pipeline: %v", err)
	}
	_ = db.Close()

	graph := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	defer func() { _ = graph.Close() }()

	ctx := context.Background()

	res, err := graph.Query(ctx, "MATCH (i:Import) RETURN i.name AS name, i.line_number AS line ORDER BY name", nil)
	if err != nil {
		t.Fatalf("MATCH (i:Import) must not be an error any more: %v", err)
	}
	if len(res.Records) != 2 {
		t.Fatalf("expected two Import nodes, got %d: %v", len(res.Records), res.Records)
	}

	contained, err := graph.Query(ctx,
		"MATCH (f:File {path: 'sample.go'})-[:CONTAINS]->(i:Import) RETURN DISTINCT i.name", nil)
	if err != nil {
		t.Fatalf("File-CONTAINS->Import: %v", err)
	}
	if len(contained.Records) != 2 {
		t.Errorf("Import nodes are orphans, not contained by their file: %v", contained.Records)
	}

	mods, err := graph.Query(ctx, "MATCH (f:File {path: 'sample.go'})-[:IMPORTS]->(m:Module) RETURN count(DISTINCT m.uid) AS n", nil)
	if err != nil {
		t.Fatalf("IMPORTS edges: %v", err)
	}
	if len(mods.Records) == 0 || fmt.Sprint(mods.Records[0]["n"]) == "0" {
		t.Errorf("the IMPORTS edges were lost: %v", mods.Records)
	}
}

// The declarations drifted three ways on one data_key: 22 import queries said Module,
// 10 said Import, one said "" and 7 said nothing — while the pipeline used none of it,
// because the branch that built the entity threw it away. Module was the worst of the
// three: the entity uid is per file, so honouring it would fabricate a second node for
// a module that already has one under its canonical uid.
//
// The rule is not "every imports query says Import". A query declared `type: relation`
// produces an edge, and this repository's own validator (TestVerifyAllDefaultQueries)
// requires those to carry NO graph_label — five grammars declare their imports that
// way. So each kind is checked against its own rule, and the label is forced in code
// regardless, which is what makes an Import node appear for all of them.
func TestImportQueriesDeclareTheRightLabelForTheirKind(t *testing.T) {
	t.Parallel()

	entries, err := os.ReadDir("queries")
	if err != nil {
		t.Fatalf("read grammar dir: %v", err)
	}

	reLabel := regexp.MustCompile(`(?m)^\s*graph_label:\s*(.*)$`)
	entityQueries, relationQueries := 0, 0

	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".yaml" {
			continue
		}
		data, err := os.ReadFile(filepath.Join("queries", e.Name()))
		if err != nil {
			t.Fatalf("read %s: %v", e.Name(), err)
		}

		for _, block := range regexp.MustCompile(`(?m)^\s*- `).Split(string(data), -1) {
			if !strings.HasPrefix(strings.TrimSpace(block), "data_key: imports") {
				continue
			}
			label := ""
			if m := reLabel.FindStringSubmatch(block); m != nil {
				label = strings.Trim(strings.TrimSpace(m[1]), `"'`)
			}

			if strings.Contains(block, "type: relation") {
				relationQueries++
				if label != "" {
					t.Errorf("%s: a type=relation imports query declares graph_label %q — relations carry no label",
						e.Name(), label)
				}
				continue
			}

			entityQueries++
			switch {
			case strings.Contains(block, "preproc_include"):
				if label != LabelInclude {
					t.Errorf("%s: a #include query declares %q, want %q", e.Name(), label, LabelInclude)
				}
			case strings.Contains(block, "export_statement"):
				if label != LabelExport {
					t.Errorf("%s: an export-from query declares %q, want %q", e.Name(), label, LabelExport)
				}
			default:
				if label != LabelImport {
					t.Errorf("%s: an entity imports query declares %q, want %q", e.Name(), label, LabelImport)
				}
			}
		}
	}

	if entityQueries < 20 || relationQueries < 5 {
		t.Fatalf("expected both kinds across the grammars, got %d entity and %d relation",
			entityQueries, relationQueries)
	}
}

// The label is forced in code, so an imports query that declares nothing — which is
// what a type=relation query must do — still produces an Import node. Without this the
// five grammars declaring imports as relations would silently have no Import nodes.
func TestConvertToCacheForcesTheImportLabelWhenTheQueryDeclaresNone(t *testing.T) {
	t.Parallel()

	pf := &ParsedFile{
		Path:     "Main.hs",
		Language: "haskell",
		Entities: map[string][]Entity{
			"imports": {{Name: "Data.List", Line: 3}},
		},
	}

	entry := ConvertToCache(pf, ".", false, "")

	if len(entry.Entities) != 1 || entry.Entities[0].Label != LabelImport {
		t.Fatalf("expected one Import entity, got %+v", entry.Entities)
	}
	if len(entry.Imports) != 1 {
		t.Errorf("the IMPORTS edge record must still be produced, got %+v", entry.Imports)
	}
	if entry.Entities[0].Label == "Imports" {
		t.Error("the label fell back to the plural data_key")
	}
}

// The label is honoured for the import family and replaced for anything else. Module is
// the case that matters: 22 grammars declared it, and it would fabricate a second
// Module node under a per-file uid beside the canonical one.
func TestImportEntityLabelHonoursTheFamilyAndReplacesTheRest(t *testing.T) {
	t.Parallel()

	for declared, want := range map[string]string{
		LabelImport:  LabelImport,
		LabelInclude: LabelInclude,
		LabelExport:  LabelExport,
		LabelModule:  LabelImport,
		"":           LabelImport,
		"Whatever":   LabelImport,
	} {
		if got := importEntityLabel(declared); got != want {
			t.Errorf("importEntityLabel(%q) = %q, want %q", declared, got, want)
		}
	}
}

// End to end for the split: a C #include lands as Include, and still produces the
// IMPORTS edge, because it does pull a module in.
//
// The grammar is staged into the work directory on purpose. Query files are resolved at
// runtime from three directories, and the installed copy is whatever the last sync put
// there — so a test that does not stage measures the installed YAML instead of the one
// in this repository, and passes while the repository's file says something else.
func TestPipelineWritesIncludeNodesForC(t *testing.T) {
	work := stageGrammar(t, "c", "tree-sitter-c", ".c", "c.yaml")

	src := "#include <stdio.h>\n#include \"local.h\"\n\nint main(void) { return 0; }\n"
	if err := os.WriteFile(filepath.Join(work, "main.c"), []byte(src), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	tmp := t.TempDir()
	dbPath := filepath.Join(tmp, "ladybugdb")
	db := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	if _, err := RunPipeline(context.Background(), db, work, PipelineOptions{
		CacheDir: filepath.Join(tmp, "cache"),
	}); err != nil {
		_ = db.Close()
		t.Fatalf("pipeline: %v", err)
	}
	_ = db.Close()

	graph := NewLadybugDB(LadybugConfig{StoreDir: filepath.Dir(dbPath), IcebugDir: filepath.Join(filepath.Dir(dbPath), "graph.icebug")})
	defer func() { _ = graph.Close() }()

	res, err := graph.Query(context.Background(),
		"MATCH (i:Include) RETURN i.name AS name ORDER BY name", nil)
	if err != nil {
		t.Fatalf("MATCH (i:Include): %v", err)
	}
	if len(res.Records) != 2 {
		t.Errorf("expected two Include nodes, got %d: %v", len(res.Records), res.Records)
	}

	if _, err := graph.Query(context.Background(), "MATCH (i:Import) RETURN count(i) AS n", nil); err == nil {
		t.Error("a C file produced Import nodes; #include must land as Include")
	}

	mods, err := graph.Query(context.Background(), "MATCH (f:File {path: 'main.c'})-[:IMPORTS]->(m:Module) RETURN count(DISTINCT m.uid) AS n", nil)
	if err != nil {
		t.Fatalf("IMPORTS edges: %v", err)
	}
	if len(mods.Records) == 0 || fmt.Sprint(mods.Records[0]["n"]) == "0" {
		t.Errorf("an Include must still produce its IMPORTS edge: %v", mods.Records)
	}
}
