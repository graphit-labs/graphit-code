package ast

import (
	"os"
	"path/filepath"
	"testing"

	scip "github.com/scip-code/scip/bindings/go/scip"
	"google.golang.org/protobuf/proto"
)

func TestDecodeSCIPCanonicalSymbolsAndSelectedDocuments(t *testing.T) {
	root := t.TempDir()
	for _, rel := range []string{"a.go", "b.go", "ignored.go"} {
		if err := os.WriteFile(filepath.Join(root, rel), []byte("package demo\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	global := "scip-go gomod example v1 pkg/Foo()."
	local := "local 0"
	idx := &scip.Index{Documents: []*scip.Document{
		{RelativePath: "a.go", Language: "go", Symbols: []*scip.SymbolInformation{
			{Symbol: global, DisplayName: "Foo", Kind: scip.SymbolInformation_Function},
			{Symbol: local, DisplayName: "Foo", Kind: scip.SymbolInformation_Variable},
		}, Occurrences: []*scip.Occurrence{
			{Symbol: global, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{1, 0, 3}},
			{Symbol: local, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{2, 0, 3}},
		}},
		{RelativePath: "b.go", Language: "go", Occurrences: []*scip.Occurrence{
			{Symbol: global, Range: []int32{1, 0, 3}},
		}},
		{RelativePath: "ignored.go", Language: "go", Occurrences: []*scip.Occurrence{
			{Symbol: global, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{1, 0, 3}},
		}},
	}}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	entries, err := decodeSCIP(data, root, map[string]bool{"a.go": true, "b.go": true})
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 2 {
		t.Fatalf("selected docs = %d, want 2", len(entries))
	}
	a := entries[filepath.Join(root, "a.go")]
	b := entries[filepath.Join(root, "b.go")]
	if a == nil || b == nil || len(a.Entities) != 2 || len(b.References) != 1 {
		t.Fatalf("unexpected SCIP graph rows: a=%+v b=%+v", a, b)
	}
	if a.Entities[0].UID != scipUID("a.go", global) || a.Entities[1].UID != scipUID("a.go", local) || a.Entities[0].UID == a.Entities[1].UID {
		t.Fatalf("canonical identities lost: %+v", a.Entities)
	}
	if b.References[0].TargetUID != a.Entities[0].UID {
		t.Fatalf("reference target = %q, want %q", b.References[0].TargetUID, a.Entities[0].UID)
	}
}

func TestDecodeSCIPAppliesDeclarativeEntityAndRelationRules(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	root := t.TempDir()
	file := filepath.Join(root, "a.go")
	if err := os.WriteFile(file, []byte("package demo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	profilePath := filepath.Join(projectQueriesDir(root), "scip", "scip-go.yaml")
	if err := os.MkdirAll(filepath.Dir(profilePath), 0o755); err != nil {
		t.Fatal(err)
	}
	profile := "merge: true\nentities:\n  kinds: [Function]\n  labels:\n    Function: Routine\nrelations: [REFERENCES]\ndocumentation: false\nexternal_symbols: false\n"
	if err := os.WriteFile(profilePath, []byte(profile), 0o644); err != nil {
		t.Fatal(err)
	}
	fn := "scip-go gomod demo v1 pkg/Foo()."
	variable := "scip-go gomod demo v1 pkg/value."
	target := "scip-go gomod dep v1 pkg/Bar()."
	idx := &scip.Index{ExternalSymbols: []*scip.SymbolInformation{{Symbol: target, Kind: scip.SymbolInformation_Function}},
		Documents: []*scip.Document{{RelativePath: "a.go", Language: "go",
			Symbols: []*scip.SymbolInformation{
				{Symbol: fn, Kind: scip.SymbolInformation_Function, Documentation: []string{"hidden"},
					Relationships: []*scip.Relationship{{Symbol: target, IsReference: true, IsImplementation: true}}},
				{Symbol: variable, Kind: scip.SymbolInformation_Variable},
			},
			Occurrences: []*scip.Occurrence{
				{Symbol: fn, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 0, 1}},
				{Symbol: variable, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 2, 3}},
			}}}}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	entries, err := decodeSCIP(data, root, map[string]bool{"a.go": true})
	if err != nil {
		t.Fatal(err)
	}
	entry := entries[file]
	if entry == nil || len(entry.Entities) != 1 || entry.Entities[0].Label != "Routine" ||
		entry.Entities[0].UID != scipUID("a.go", fn) || entry.Entities[0].Docstring != "" {
		t.Fatalf("entity rules not applied: %+v", entry)
	}
	if len(entry.References) != 1 || entry.References[0].TargetUID != scipUID("a.go", target) || len(entry.Inheritance) != 0 {
		t.Fatalf("relation rules not applied: refs=%+v inheritance=%+v", entry.References, entry.Inheritance)
	}
}

func TestDecodeSCIPRejectsInvalidAndUnselectedPaths(t *testing.T) {
	root := t.TempDir()
	idx := &scip.Index{Documents: []*scip.Document{{RelativePath: "../outside.go"}, {RelativePath: "x/../bad.go"}, {RelativePath: "missing.go"}}}
	data, _ := proto.Marshal(idx)
	entries, err := decodeSCIP(data, root, map[string]bool{"../outside.go": true, "x/../bad.go": true, "missing.go": true})
	if err != nil || len(entries) != 0 {
		t.Fatalf("invalid documents accepted: %v, err %v", entries, err)
	}
}

func TestDecodeSCIPMalformedDefinitionFallsBackPerDocument(t *testing.T) {
	root := t.TempDir()
	file := filepath.Join(root, "broken.go")
	if err := os.WriteFile(file, []byte("package demo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	idx := &scip.Index{Documents: []*scip.Document{{RelativePath: "broken.go", Occurrences: []*scip.Occurrence{{
		Symbol: "scip-go gomod example v1 pkg/Broken().", SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{2},
	}}}}}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	entries, err := decodeSCIP(data, root, map[string]bool{"broken.go": true})
	if err != nil || entries[file] != nil {
		t.Fatalf("malformed SCIP definition suppressed syntax fallback: entries=%v err=%v", entries, err)
	}
}

func TestSCIPGlobalMemberUsesCanonicalEnclosingSymbol(t *testing.T) {
	parent := "scip-go gomod example v1 pkg/Thing#"
	child := parent + "Run()."
	got := scipEnclosingSymbol(&scip.SymbolInformation{Symbol: child})
	if got != parent {
		t.Fatalf("enclosing symbol = %q, want %q", got, parent)
	}
}

func TestDecodeSCIPCrossDocumentContainmentKeepsParentKind(t *testing.T) {
	root := t.TempDir()
	for _, name := range []string{"parent.go", "child.go"} {
		if err := os.WriteFile(filepath.Join(root, name), []byte("package demo\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	parent := "scip-go gomod example v1 pkg/Thing#"
	child := parent + "Run()."
	idx := &scip.Index{Documents: []*scip.Document{
		{RelativePath: "parent.go", Symbols: []*scip.SymbolInformation{{Symbol: parent, Kind: scip.SymbolInformation_Class}},
			Occurrences: []*scip.Occurrence{{Symbol: parent, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 0, 1}}}},
		{RelativePath: "child.go", Symbols: []*scip.SymbolInformation{{Symbol: child, Kind: scip.SymbolInformation_Method}},
			Occurrences: []*scip.Occurrence{{Symbol: child, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 0, 1}}}},
	}}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	entries, err := decodeSCIP(data, root, map[string]bool{"parent.go": true, "child.go": true})
	if err != nil {
		t.Fatal(err)
	}
	edges := entries[filepath.Join(root, "child.go")].ContainsEdges
	if len(edges) != 1 || edges[0].ParentUID != scipUID("parent.go", parent) || edges[0].ParentLabel != "Class" || edges[0].ChildUID != scipUID("child.go", child) {
		t.Fatalf("cross-document containment lost: %+v", edges)
	}
}

func TestDecodeSCIPPreservesIndependentRelationshipFlagsAndSignature(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "a.go"), []byte("package demo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	symbol := "scip-go gomod example v1 pkg/Child#Method()."
	target := "scip-go gomod example v1 pkg/Parent#Method()."
	idx := &scip.Index{Documents: []*scip.Document{{RelativePath: "a.go", Language: "go",
		Symbols: []*scip.SymbolInformation{{Symbol: symbol, DisplayName: "Method", Kind: scip.SymbolInformation_Method,
			SignatureDocumentation: &scip.Signature{Language: "go", Text: "func Method()"},
			Documentation:          []string{"method docs"},
			Relationships: []*scip.Relationship{{Symbol: target, IsReference: true, IsImplementation: true,
				IsTypeDefinition: true, IsDefinition: true}, {Symbol: "scip-go gomod example v1 pkg/Unused#"}}}},
		Occurrences: []*scip.Occurrence{{Symbol: symbol, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 0, 1}}},
	}}}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	entries, err := decodeSCIP(data, root, map[string]bool{"a.go": true})
	if err != nil {
		t.Fatal(err)
	}
	entry := entries[filepath.Join(root, "a.go")]
	if entry == nil || len(entry.Entities) != 1 || entry.Entities[0].Docstring != "func Method()\n\nmethod docs" {
		t.Fatalf("signature/documentation lost: %+v", entry)
	}
	if len(entry.References) != 3 || len(entry.Inheritance) != 1 || entry.Inheritance[0].RelType != "IMPLEMENTS" ||
		entry.Inheritance[0].ChildUID != scipUID("a.go", symbol) || entry.Inheritance[0].ParentUID != scipUID("a.go", target) {
		t.Fatalf("relationship flags lost or invented: %+v", entry.References)
	}
	for i, kind := range []string{"REFERENCES", "TYPE_DEFINITION", "DEFINITION"} {
		ref := entry.References[i]
		if ref.RelType != kind || ref.SourceUID != scipUID("a.go", symbol) || ref.TargetUID != scipUID("a.go", target) {
			t.Fatalf("relationship %d = %+v, want %s", i, ref, kind)
		}
	}
}

func TestDecodeSCIPUsesExternalSymbolMetadataForCanonicalStub(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "a.go"), []byte("package demo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	external := "scip-go gomod dependency v2 pkg/External#"
	idx := &scip.Index{Documents: []*scip.Document{{RelativePath: "a.go", Language: "go",
		Occurrences: []*scip.Occurrence{{Symbol: external, Range: []int32{0, 0, 1}}}}},
		ExternalSymbols: []*scip.SymbolInformation{{Symbol: external, DisplayName: "External",
			Kind: scip.SymbolInformation_Class, Documentation: []string{"external docs"}}}}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	entries, err := decodeSCIP(data, root, map[string]bool{"a.go": true})
	if err != nil {
		t.Fatal(err)
	}
	entry := entries[filepath.Join(root, "a.go")]
	if entry == nil || len(entry.Entities) != 1 {
		t.Fatalf("external metadata missing: %+v", entry)
	}
	stub := entry.Entities[0]
	if stub.UID != scipUID("a.go", external) || stub.Label != "Class" || stub.Name != "External" ||
		stub.Docstring != "external docs" || !stub.IsDep || !stub.IsStub || stub.Path != "" {
		t.Fatalf("external metadata not preserved: %+v", stub)
	}
}

func TestDecodeSCIPReferenceUsesMostSpecificSameLineOwner(t *testing.T) {
	root := t.TempDir()
	if err := os.WriteFile(filepath.Join(root, "nested.go"), []byte("package demo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	outer := "scip-go gomod example v1 pkg/Outer#."
	inner := "scip-go gomod example v1 pkg/Outer#Inner()."
	target := "scip-go gomod dependency v1 pkg/Target#."
	idx := &scip.Index{Documents: []*scip.Document{{RelativePath: "nested.go", Language: "go",
		Occurrences: []*scip.Occurrence{
			{Symbol: outer, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 0, 1}, EnclosingRange: []int32{0, 0, 3, 0}},
			{Symbol: inner, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 5, 10}, EnclosingRange: []int32{0, 5, 2, 0}},
			{Symbol: target, Range: []int32{1, 2, 5}},
		},
	}}}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 20; i++ {
		entries, err := decodeSCIP(data, root, map[string]bool{"nested.go": true})
		if err != nil {
			t.Fatal(err)
		}
		refs := entries[filepath.Join(root, "nested.go")].References
		if len(refs) != 1 || refs[0].SourceUID != scipUID("nested.go", inner) {
			t.Fatalf("iteration %d chose wrong enclosing symbol: %+v", i, refs)
		}
	}
}

func TestDecodeSCIPPreservesSymbolWithoutDefinitionOccurrence(t *testing.T) {
	root := t.TempDir()
	for _, name := range []string{"alias.go", "definition.go"} {
		if err := os.WriteFile(filepath.Join(root, name), []byte("package demo\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	alias := "scip-go gomod example v1 pkg/Alias#."
	target := "scip-go gomod example v1 pkg/Target#."
	idx := &scip.Index{Documents: []*scip.Document{
		{RelativePath: "alias.go", Language: "go", Symbols: []*scip.SymbolInformation{
			{Symbol: alias, DisplayName: "Alias", Kind: scip.SymbolInformation_TypeAlias,
				Relationships: []*scip.Relationship{{Symbol: target, IsTypeDefinition: true}}},
			{Symbol: target, DisplayName: "Target", Kind: scip.SymbolInformation_Class},
		}},
		{RelativePath: "definition.go", Language: "go", Symbols: []*scip.SymbolInformation{{Symbol: target,
			DisplayName: "Target", Kind: scip.SymbolInformation_Class}},
			Occurrences: []*scip.Occurrence{{Symbol: target, SymbolRoles: int32(scip.SymbolRole_Definition), Range: []int32{0, 0, 1}}}},
	}}
	data, err := proto.Marshal(idx)
	if err != nil {
		t.Fatal(err)
	}
	entries, err := decodeSCIP(data, root, map[string]bool{"alias.go": true, "definition.go": true})
	if err != nil {
		t.Fatal(err)
	}
	aliasEntry := entries[filepath.Join(root, "alias.go")]
	if len(aliasEntry.Entities) != 1 || aliasEntry.Entities[0].UID != scipUID("alias.go", alias) || !aliasEntry.Entities[0].IsStub {
		t.Fatalf("definitionless symbol or duplicate target: %+v", aliasEntry.Entities)
	}
	if len(aliasEntry.References) != 1 || aliasEntry.References[0].TargetUID != scipUID("alias.go", target) || aliasEntry.References[0].RelType != "TYPE_DEFINITION" {
		t.Fatalf("definitionless symbol lost its relationship: %+v", aliasEntry.References)
	}
}

func TestSCIPTargetsResolveByCanonicalUID(t *testing.T) {
	uidA := scipUID("a.go", "scip-go gomod example v1 pkg/A#Handle().")
	uidB := scipUID("b.go", "scip-go gomod example v1 pkg/B#Handle().")
	ri := &rebuildIndex{entityUIDs: map[string]string{uidA: "Method", uidB: "Method"}}
	if uid, label := ri.resolveCallee(uidA, "go"); uid != uidA || label != "Method" {
		t.Fatalf("call resolved by display name: %s %s", uid, label)
	}
	ref := cachedReference{TargetUID: uidB, RelType: "REFERENCES"}
	if uid, label := ri.resolveRefTarget(ref, "go"); uid != uidB || label != "Method" {
		t.Fatalf("reference resolved by display name: %s %s", uid, label)
	}
}

func TestSCIPExternalReferenceKeepsCanonicalStub(t *testing.T) {
	external := "scip:scip-go gomod example v1 dep/External#."
	entry := &parseCacheEntry{RelPath: "a.go", Language: "go", References: []cachedReference{{
		SourceUID: "a.go", TargetUID: external, RelType: "REFERENCES", Path: "a.go", Lang: "go",
	}}}
	ri := newRebuildIndex(map[string]*parseCacheEntry{"a.go": entry}, nil)
	stubs := ri.entityJSON("Symbol")
	if len(stubs) != 1 || stubs[0]["uid"] != external || stubs[0]["is_stub"] != true {
		t.Fatalf("canonical external stub missing: %+v", stubs)
	}
	if uid, label := ri.resolveRefTarget(entry.References[0], "go"); uid != external || label != "Symbol" {
		t.Fatalf("external target changed: %q %q", uid, label)
	}
}

func TestCompositeParserUsesSCIPBeforeSyntaxParsers(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	root := t.TempDir()
	path := filepath.Join(root, "bad.go")
	if err := os.WriteFile(path, []byte("this is not valid Go"), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("GRAPHIT_AST_GRAMMAR", ".go=unavailable-grammar")
	if !HasParserForExtensionIn(root, ".go") {
		t.Fatal("explicit syntax override hid opt-in SCIP from file discovery")
	}
	entry := &parseCacheEntry{RelPath: "bad.go", Language: "go", Entities: []cachedEntity{{UID: "scip:exact", Label: "Function", Name: "Exact"}}}
	parser := NewCompositeParser(root, map[string]string{".go": "unavailable-grammar"})
	parser.SetSCIPEntries(map[string]*parseCacheEntry{path: entry})
	parsed, err := parser.Parse(path, false, ParseOptions{})
	if err != nil || parsed.Parser != "scip" || parsed.SCIPEntry == entry || parsed.SCIPEntry.Entities[0].UID != "scip:exact" {
		t.Fatalf("SCIP did not win: parsed=%+v err=%v", parsed, err)
	}
	if cached := ConvertToCache(parsed, root, false, ""); cached != parsed.SCIPEntry {
		t.Fatalf("canonical SCIP rows passed through name converter: %+v", cached)
	}
	dependent, err := parser.Parse(path, true, ParseOptions{})
	if err != nil || dependent == nil || dependent.SCIPEntry == nil || !dependent.SCIPEntry.IsDepend || parsed.SCIPEntry.IsDepend || entry.IsDepend {
		t.Fatalf("per-parse SCIP metadata was shared across workers: parsed=%+v dependent=%+v original=%+v err=%v", parsed.SCIPEntry, dependent.SCIPEntry, entry, err)
	}
	empty := &parseCacheEntry{RelPath: "bad.go", Language: "go"}
	parser.SetSCIPEntries(map[string]*parseCacheEntry{path: empty})
	parsed, err = parser.Parse(path, false, ParseOptions{})
	if err != nil || parsed.Parser != "scip" || parsed.SCIPEntry == empty || parsed.SCIPEntry.RelPath != empty.RelPath {
		t.Fatalf("empty SCIP document fell back to syntax parser: parsed=%+v err=%v", parsed, err)
	}
	parser.SetSCIPEntries(nil)
	if _, err := parser.Parse(path, false, ParseOptions{}); err == nil {
		t.Fatal("missing SCIP document bypassed explicit syntax grammar fallback")
	}
}

func TestSCIPRespectsLanguageGrammarFilter(t *testing.T) {
	t.Setenv("GRAPHIT_AST_SCIP_ENABLED", "true")
	t.Setenv("GRAPHIT_AST_GRAMMARS_BLACKLIST", "go")
	root := t.TempDir()
	if family := scipFamilyFor(root, ".go"); family != "" {
		t.Fatalf("blacklisted Go language selected SCIP: %s", family)
	}
	if family := scipFamilyFor(root, ".js"); family != "typescript" {
		t.Fatalf("unblocked JavaScript SCIP family = %q", family)
	}
}
