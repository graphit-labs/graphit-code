//go:build lancedb

package ast

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/fswatch"
	scip "github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
)

func TestSCIPTypeScriptSelectionIncremental(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	project := stageGrammar(t, "javascript", "tree-sitter-javascript", ".js", "javascript.yaml")
	write := func(rel, body string) {
		t.Helper()
		path := filepath.Join(project, rel)
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("a.js", "export const a = 1;\n")
	write("b.js", "export const b = 1;\n")
	write("nested/Keep.js", "export const keep = 1;\n")
	write("nested/Skip.js", "export const skip = 1;\n")
	write(".gitignore", "nested/*.js\n!nested/Keep.js\n")
	write("nested/.astignore", "*.js\n!Keep.js\n")
	var docs []*scip.Document
	for _, rel := range []string{"a.js", "b.js", "nested/Keep.js", "nested/Skip.js"} {
		symbol := "scip-typescript npm demo 1 " + rel + "/value."
		docs = append(docs, &scip.Document{RelativePath: rel, Language: "javascript", Symbols: []*scip.SymbolInformation{{Symbol: symbol, DisplayName: "value", Kind: scip.SymbolInformation_Variable}}, Occurrences: []*scip.Occurrence{{Symbol: symbol, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 13, 18}}}})
	}
	data, err := proto.Marshal(&scip.Index{Documents: docs})
	if err != nil {
		t.Fatal(err)
	}
	old := runSCIPImage
	defer func() { runSCIPImage = old }()
	calls := 0
	var selected map[string]bool
	runSCIPImage = func(_ context.Context, _, _, family string, allow ...map[string]bool) ([]byte, error) {
		if family != "typescript" || len(allow) != 1 {
			t.Fatalf("unexpected SCIP call: family=%s allow=%v", family, allow)
		}
		calls++
		selected = allow[0]
		return data, nil
	}
	cache := filepath.Join(t.TempDir(), "cache")
	graphDir := t.TempDir()
	db := NewLadybugDB(LadybugConfig{StoreDir: graphDir, IcebugDir: filepath.Join(graphDir, "graph.icebug")})
	defer func() { _ = db.Close() }()
	ctx := context.Background()
	run := func(changed, deleted []string, wantCalls int, wantAllowed ...string) *PipelineResult {
		t.Helper()
		var result *PipelineResult
		var err error
		if changed == nil && deleted == nil {
			result, err = RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
		} else {
			result, err = RunPipelineForPaths(ctx, db, project, changed, deleted, PipelineOptions{CacheDir: cache})
		}
		if err != nil || calls != wantCalls {
			t.Fatalf("pipeline calls=%d want=%d result=%+v err=%v", calls, wantCalls, result, err)
		}
		if calls == wantCalls && len(wantAllowed) > 0 {
			if len(selected) != len(wantAllowed) {
				t.Fatalf("allowed=%v want=%v", selected, wantAllowed)
			}
			for _, rel := range wantAllowed {
				if !selected[rel] {
					t.Fatalf("missing allowed %s in %v", rel, selected)
				}
			}
		}
		return result
	}
	run(nil, nil, 1, "a.js", "b.js", "nested/Keep.js")
	write(".gitignore", "b.js\nnested/*.js\n!nested/Keep.js\n")
	run([]string{filepath.Join(project, "b.js")}, nil, 2, "a.js", "nested/Keep.js")
	rows, err := db.Query(ctx, "MATCH (f:File) RETURN f.path AS path", nil)
	if err != nil {
		t.Fatal(err)
	}
	for _, row := range rows.Records {
		if strings.HasSuffix(fmt.Sprint(row["path"]), "b.js") {
			t.Fatalf("ignored scoped file remains in graph: %+v", rows.Records)
		}
	}
	run([]string{filepath.Join(project, ".gitignore")}, nil, 2)
	write("b.js", "export const b = 2;\n")
	ignored := run([]string{filepath.Join(project, "b.js")}, nil, 2)
	if ignored.ParsedFiles != 0 || ignored.EngineStats["tree-sitter:javascript"] != 0 {
		t.Fatalf("ignored scoped edit was parsed: %+v", ignored)
	}
	write("nested/.astignore", "*.js\n")
	run([]string{filepath.Join(project, "nested", ".astignore")}, nil, 3, "a.js")
	write("tsconfig.json", `{"compilerOptions":{"allowJs":true},"files":["a.js"]}`)
	run([]string{filepath.Join(project, "tsconfig.json")}, nil, 4, "a.js")
	if err := os.Remove(filepath.Join(project, "a.js")); err != nil {
		t.Fatal(err)
	}
	output := filepath.Join(scipOutputDir(project, cache, "typescript"), "index.scip")
	if err := os.MkdirAll(filepath.Dir(output), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(output, []byte("stale"), 0o644); err != nil {
		t.Fatal(err)
	}
	run(nil, []string{"a.js"}, 4)
	if _, err := os.Stat(output); !os.IsNotExist(err) {
		t.Fatalf("stale raw SCIP index remains: %v", err)
	}
	shards, err := NewShardCache(cache)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = shards.Close() }()
	for _, rel := range shards.AllPaths() {
		if strings.HasSuffix(rel, ".js") {
			t.Fatalf("ignored or deleted JS shard remains: %s", rel)
		}
	}
}

func TestSCIPTypeScriptConfigDropsDocumentToSyntax(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	project := stageGrammar(t, "javascript", "tree-sitter-javascript", ".js", "javascript.yaml")
	for _, rel := range []string{"a.js", "b.js"} {
		if err := os.WriteFile(filepath.Join(project, rel), []byte("export const value = 1;\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.WriteFile(filepath.Join(project, "tsconfig.base.json"), []byte(`{"compilerOptions":{"allowJs":true}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	makeIndex := func(paths ...string) []byte {
		t.Helper()
		var docs []*scip.Document
		for _, rel := range paths {
			symbol := "scip-typescript npm demo 1 " + rel + "/value."
			docs = append(docs, &scip.Document{RelativePath: rel, Language: "javascript", Symbols: []*scip.SymbolInformation{{Symbol: symbol, DisplayName: "value", Kind: scip.SymbolInformation_Variable}}, Occurrences: []*scip.Occurrence{{Symbol: symbol, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 13, 18}}}})
		}
		data, err := proto.Marshal(&scip.Index{Documents: docs})
		if err != nil {
			t.Fatal(err)
		}
		return data
	}
	before, after := makeIndex("a.js", "b.js"), makeIndex("a.js")
	old := runSCIPImage
	defer func() { runSCIPImage = old }()
	calls := 0
	runSCIPImage = func(context.Context, string, string, string, ...map[string]bool) ([]byte, error) {
		calls++
		if calls == 1 {
			return before, nil
		}
		return after, nil
	}
	cache := filepath.Join(t.TempDir(), "cache")
	graphDir := t.TempDir()
	db := NewLadybugDB(LadybugConfig{StoreDir: graphDir, IcebugDir: filepath.Join(graphDir, "graph.icebug")})
	defer func() { _ = db.Close() }()
	ctx := context.Background()
	first, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 1 || first.EngineStats["scip:javascript"] != 2 {
		t.Fatalf("initial SCIP: calls=%d result=%+v err=%v", calls, first, err)
	}
	if err := os.WriteFile(filepath.Join(project, "tsconfig.json"), []byte(`{"extends":"./tsconfig.base.json","files":["a.js"]}`), 0o644); err != nil {
		t.Fatal(err)
	}
	second, err := RunPipelineForPaths(ctx, db, project, []string{"tsconfig.json"}, nil, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 2 || second.EngineStats["scip:javascript"] != 1 || second.EngineStats["tree-sitter:javascript"] != 1 {
		t.Fatalf("tsconfig shrink did not reparse omitted document: calls=%d result=%+v err=%v", calls, second, err)
	}
	if err := os.WriteFile(filepath.Join(project, "tsconfig.base.json"), []byte(`{"compilerOptions":{"allowJs":true,"strict":true}}`), 0o644); err != nil {
		t.Fatal(err)
	}
	third, err := RunPipelineForPaths(ctx, db, project, []string{"tsconfig.base.json"}, nil, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 3 || third.EngineStats["scip:javascript"] != 1 || third.EngineStats["tree-sitter:javascript"] != 1 {
		t.Fatalf("extended tsconfig edit did not refresh family: calls=%d result=%+v err=%v", calls, third, err)
	}
	noOp, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 3 || noOp.ParsedFiles != 0 {
		t.Fatalf("unchanged selector did not stay cached: calls=%d result=%+v err=%v", calls, noOp, err)
	}
}

func TestSCIPClangSelectionIncremental(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	project := stageGrammar(t, "c", "tree-sitter-c", ".c", "c.yaml")
	for _, rel := range []string{"a.c", "b.c"} {
		if err := os.WriteFile(filepath.Join(project, rel), []byte("int value(void) { return 1; }\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	writeCompdb := func(files ...string) {
		t.Helper()
		var commands []map[string]any
		for _, rel := range files {
			commands = append(commands, map[string]any{"directory": project, "file": filepath.Join(project, rel), "arguments": []string{"clang", "-c", filepath.Join(project, rel)}})
		}
		data, err := json.Marshal(commands)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(project, "compile_commands.json"), data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	makeIndex := func(paths ...string) []byte {
		t.Helper()
		var docs []*scip.Document
		for _, rel := range paths {
			symbol := "scip-clang . . . " + rel + "/value."
			docs = append(docs, &scip.Document{RelativePath: rel, Language: "c", Symbols: []*scip.SymbolInformation{{Symbol: symbol, DisplayName: "value", Kind: scip.SymbolInformation_Function}}, Occurrences: []*scip.Occurrence{{Symbol: symbol, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 4, 9}}}})
		}
		data, err := proto.Marshal(&scip.Index{Documents: docs})
		if err != nil {
			t.Fatal(err)
		}
		return data
	}
	writeCompdb("a.c", "b.c")
	both, onlyA := makeIndex("a.c", "b.c"), makeIndex("a.c")
	old := runSCIPImage
	defer func() { runSCIPImage = old }()
	calls := 0
	runSCIPImage = func(_ context.Context, _, _, family string, allow ...map[string]bool) ([]byte, error) {
		if family != "clang" || len(allow) != 1 {
			t.Fatalf("unexpected SCIP family=%s allow=%v", family, allow)
		}
		calls++
		if calls == 1 {
			return both, nil
		}
		return onlyA, nil
	}
	cache := filepath.Join(t.TempDir(), "cache")
	graphDir := t.TempDir()
	db := NewLadybugDB(LadybugConfig{StoreDir: graphDir, IcebugDir: filepath.Join(graphDir, "graph.icebug")})
	defer func() { _ = db.Close() }()
	ctx := context.Background()
	first, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 1 || first.EngineStats["scip:c"] != 2 {
		t.Fatalf("initial Clang: calls=%d result=%+v err=%v", calls, first, err)
	}
	writeCompdb("a.c")
	second, err := RunPipelineForPaths(ctx, db, project, []string{"compile_commands.json"}, nil, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 2 || second.EngineStats["scip:c"] != 1 || second.EngineStats["tree-sitter:c"] != 1 {
		t.Fatalf("compdb shrink left stale SCIP: calls=%d result=%+v err=%v", calls, second, err)
	}
	if err := os.WriteFile(filepath.Join(project, ".gitignore"), []byte("b.c\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	third, err := RunPipelineForPaths(ctx, db, project, []string{".gitignore"}, nil, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 3 || third.EngineStats["scip:c"] != 1 {
		t.Fatalf("Clang ignore edit did not refresh: calls=%d result=%+v err=%v", calls, third, err)
	}
	if err := os.Remove(filepath.Join(project, "a.c")); err != nil {
		t.Fatal(err)
	}
	output := filepath.Join(scipOutputDir(project, cache, "clang"), "index.scip")
	if err := os.MkdirAll(filepath.Dir(output), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(output, []byte("stale"), 0o644); err != nil {
		t.Fatal(err)
	}
	last, err := RunPipelineForPaths(ctx, db, project, nil, []string{"a.c"}, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 3 {
		t.Fatalf("last Clang TU removal: calls=%d result=%+v err=%v", calls, last, err)
	}
	if _, err := os.Stat(output); !os.IsNotExist(err) {
		t.Fatalf("Clang raw index remains with no source: %v", err)
	}
}

func TestSCIPPipelineFullIncrementalAndFallback(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "false")
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	project := stageGrammar(t, "go", "tree-sitter-go", ".go", "go.yaml")
	for _, name := range []string{"a.go", "b.go"} {
		if err := os.WriteFile(filepath.Join(project, name), []byte("package demo\nfunc Foo() {}\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	aSymbol := "scip-go gomod example.com/demo v1 demo/A()."
	bSymbol := "scip-go gomod example.com/demo v1 demo/B()."
	external := "scip-go gomod example.com/dependency v2 dep/External#."
	index := &scip.Index{Documents: []*scip.Document{
		{RelativePath: "a.go", Language: "go", Symbols: []*scip.SymbolInformation{{Symbol: aSymbol, DisplayName: "Foo", Kind: scip.SymbolInformation_Function}},
			Occurrences: []*scip.Occurrence{{Symbol: aSymbol, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{1, 5, 8}}}},
		{RelativePath: "b.go", Language: "go", Symbols: []*scip.SymbolInformation{{Symbol: bSymbol, DisplayName: "Foo", Kind: scip.SymbolInformation_Function,
			Relationships: []*scip.Relationship{{Symbol: external, IsReference: true, IsImplementation: true}}}},
			Occurrences: []*scip.Occurrence{{Symbol: bSymbol, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{1, 5, 8}}, {Symbol: aSymbol, Range: []int32{1, 9, 12}}}},
	}}
	data, err := proto.Marshal(index)
	if err != nil {
		t.Fatal(err)
	}
	old := runSCIPImage
	defer func() { runSCIPImage = old }()
	calls := 0
	runSCIPImage = func(_ context.Context, _, _, family string, _ ...map[string]bool) ([]byte, error) {
		if family != "go" {
			t.Fatalf("unexpected indexer family %q", family)
		}
		calls++
		return data, nil
	}
	cache := filepath.Join(t.TempDir(), "cache")
	graphDir := t.TempDir()
	db := NewLadybugDB(LadybugConfig{StoreDir: graphDir, IcebugDir: filepath.Join(graphDir, "graph.icebug")})
	defer func() { _ = db.Close() }()
	ctx := context.Background()
	syntax, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil {
		t.Fatal(err)
	}
	if calls != 0 || syntax.EngineStats["tree-sitter:go"] != 2 {
		t.Fatalf("initial syntax index: calls=%d stats=%v", calls, syntax.EngineStats)
	}
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	full, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil {
		t.Fatal(err)
	}
	if calls != 1 || full.EngineStats["scip:go"] != 2 {
		t.Fatalf("full: calls=%d stats=%v", calls, full.EngineStats)
	}
	rows, err := db.Query(ctx, "MATCH (f:Function) RETURN f.uid AS uid, f.name AS name", nil)
	if err != nil {
		t.Fatal(err)
	}
	uids := make(map[string]bool)
	for _, row := range rows.Records {
		uids[row["uid"].(string)] = true
	}
	if len(uids) != 2 || !uids[scipUID("a.go", aSymbol)] || !uids[scipUID("b.go", bSymbol)] {
		t.Fatalf("canonical same-name symbols were merged: %+v", rows.Records)
	}
	stubQuery := fmt.Sprintf("MATCH (f:Function)-[:REFERENCES]->(s:Symbol) WHERE f.uid = '%s' RETURN DISTINCT s.uid AS target, s.is_stub AS stub", scipUID("b.go", bSymbol))
	stubRows, err := db.Query(ctx, stubQuery, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(stubRows.Records) != 1 || stubRows.Records[0]["target"] != scipUID("b.go", external) || stubRows.Records[0]["stub"] != true {
		t.Fatalf("external canonical relationship missing: %+v", stubRows.Records)
	}
	implementationRows, err := db.Query(ctx, fmt.Sprintf("MATCH (f:Function)-[:IMPLEMENTS]->(s:Symbol) WHERE f.uid = '%s' RETURN DISTINCT s.uid AS target", scipUID("b.go", bSymbol)), nil)
	if err != nil || len(implementationRows.Records) != 1 || implementationRows.Records[0]["target"] != scipUID("b.go", external) {
		t.Fatalf("SCIP implementation relationship missing: rows=%v err=%v", implementationRows, err)
	}
	changed := filepath.Join(project, "a.go")
	if err := os.WriteFile(changed, []byte("package demo\nfunc Foo() { _ = 1 }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	incremental, err := RunPipelineForPaths(ctx, db, project, []string{changed}, nil, PipelineOptions{CacheDir: cache})
	if err != nil {
		t.Fatal(err)
	}
	if calls != 2 || incremental.EngineStats["scip:go"] != 2 {
		t.Fatalf("incremental: calls=%d stats=%v", calls, incremental.EngineStats)
	}
	runSCIPImage = func(context.Context, string, string, string, ...map[string]bool) ([]byte, error) {
		return nil, errors.New("indexer failed")
	}
	if err := os.WriteFile(changed, []byte("package demo\nfunc Foo() { _ = 2 }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	fallback, err := RunPipelineForPaths(ctx, db, project, []string{changed}, nil, PipelineOptions{CacheDir: cache})
	if err != nil {
		t.Fatal(err)
	}
	if fallback.EngineStats["tree-sitter:go"] != 1 {
		t.Fatalf("fallback did not parse with syntax grammar: %v", fallback.EngineStats)
	}
	if _, err := os.Stat(filepath.Join(cache, "scip-configuration")); !os.IsNotExist(err) {
		t.Fatalf("failed SCIP run left success marker: %v", err)
	}
	runSCIPImage = func(_ context.Context, _, _, family string, _ ...map[string]bool) ([]byte, error) {
		calls++
		return data, nil
	}
	recovered, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil {
		t.Fatal(err)
	}
	if calls != 3 || recovered.EngineStats["scip:go"] != 2 {
		t.Fatalf("unchanged files did not retry SCIP: calls=%d stats=%v", calls, recovered.EngineStats)
	}
	watcher, err := NewWatcher(db, project, WatcherConfig{StoreDir: cache, Workers: 1})
	if err != nil {
		t.Fatal(err)
	}
	watchChanged := filepath.Join(project, "b.go")
	if err := os.WriteFile(watchChanged, []byte("package demo\nfunc Foo() { _ = 3 }\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	watcher.reindex(ctx, fswatch.Batch{Changed: []string{watchChanged}})
	if calls != 4 {
		t.Fatalf("watcher did not route a changed file through SCIP: calls=%d", calls)
	}
	noOp, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 4 || noOp.ParsedFiles != 0 {
		t.Fatalf("unchanged project relaunched SCIP: calls=%d result=%+v err=%v", calls, noOp, err)
	}
	profilePath := filepath.Join(projectQueriesDir(project), "scip", "scip-go.yaml")
	if err := os.MkdirAll(filepath.Dir(profilePath), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(profilePath, []byte("merge: true\nentities:\n  labels:\n    Function: Routine\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	reprofiled, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 5 || reprofiled.EngineStats["scip:go"] != 2 {
		t.Fatalf("profile edit did not reindex unchanged sources: calls=%d result=%+v err=%v", calls, reprofiled, err)
	}
	routines, err := db.Query(ctx, "MATCH (f:Routine) RETURN f.uid AS uid", nil)
	if err != nil || len(routines.Records) != 2 {
		t.Fatalf("profile graph label not applied: rows=%+v err=%v", routines, err)
	}
	if err := os.WriteFile(profilePath, []byte("merge: true\nrelations: [CALLS]\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	invalid, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 5 || invalid.EngineStats["tree-sitter:go"] != 2 {
		t.Fatalf("invalid profile did not fall back without Docker: calls=%d result=%+v err=%v", calls, invalid, err)
	}
	if _, err := os.Stat(filepath.Join(cache, "scip-configuration")); !os.IsNotExist(err) {
		t.Fatalf("invalid profile left success marker: %v", err)
	}
	if err := os.WriteFile(profilePath, []byte("merge: true\nentities:\n  labels:\n    Function: Routine\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	retried, err := RunPipeline(ctx, db, project, PipelineOptions{CacheDir: cache})
	if err != nil || calls != 6 || retried.EngineStats["scip:go"] != 2 {
		t.Fatalf("repaired profile did not retry SCIP: calls=%d result=%+v err=%v", calls, retried, err)
	}
}
