package ast

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Data formats are key/value languages, and until now only the key was indexed.
//
// An XML attribute produced one node named "env"; the "prod" it was set to lived
// nowhere. A JSON member produced a node named `"host"` — quotes included, because
// the capture spanned the string literal — and never the host itself. Meanwhile
// every capture in a pattern became an entity carrying the query's graph_label, so
// HCL's `resource "aws_instance" "web"` produced three Resource nodes: `resource`,
// `aws_instance` and `web`.
//
// Both halves of a pair are now nodes, the pair survives as a CONTAINS edge from
// key to value, and the value's name IS the value — which is what puts it in the
// search index, since that index reads name and docstring and never value.

// stageGrammar is stageLang for languages whose grammar name is not derivable
// from the language name — yaml_lang uses tree-sitter-yaml.
func stageGrammar(t *testing.T, langName, grammar, ext, queryFile string) string {
	t.Helper()
	body, err := os.ReadFile(filepath.Join("queries", queryFile))
	if err != nil {
		t.Skipf("no %s: %v", queryFile, err)
	}
	return stageGrammarWithQueries(t, langName, grammar, ext, queryFile, string(body))
}

// stageGrammarWithQueries stages a query file the framework does not ship, which
// is how a project opts a registered grammar back in through ast.queries_dir.
// Reading from queries/ would skip instead of fail for these, and a skip is not
// how an opt-in that stopped working should report itself.
func stageGrammarWithQueries(t *testing.T, langName, grammar, ext, queryFile, queries string) string {
	t.Helper()
	body := []byte(queries)
	if lang, err := resolveTreeSitterLang(langName, grammar); err != nil || lang == nil {
		t.Skipf("%s grammar unavailable: %v", langName, err)
	}

	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(qdir, queryFile), body, 0o644); err != nil {
		t.Fatal(err)
	}

	restore, had := tsExtMap[ext]
	extTablesMu.Lock()
	tsExtMap[ext] = &tsLangConfig{
		Language: langName, Grammar: grammar, Extensions: []string{ext},
	}
	extTablesMu.Unlock()
	t.Cleanup(func() {
		extTablesMu.Lock()
		if had {
			tsExtMap[ext] = restore
		} else {
			delete(tsExtMap, ext)
		}
		extTablesMu.Unlock()
	})
	return projectDir
}

type kvNode struct {
	label   string
	name    string
	parent  string // Context
	pLabel  string // ContextType
	value   string // Properties["value"]
	present bool
}

func nodesOf(pf *ParsedFile) map[[2]string]kvNode {
	got := make(map[[2]string]kvNode)
	for _, ents := range pf.Entities {
		for _, e := range ents {
			got[[2]string{e.GraphLabel, e.Name}] = kvNode{
				label: e.GraphLabel, name: e.Name,
				parent: e.Context, pLabel: e.ContextType,
				value: e.Properties["value"], present: true,
			}
		}
	}
	return got
}

func parseFixture(t *testing.T, projectDir, name, source string) *ParsedFile {
	t.Helper()
	srcPath := filepath.Join(projectDir, name)
	if err := os.WriteFile(srcPath, []byte(source), 0o644); err != nil {
		t.Fatal(err)
	}
	pf, err := NewCompositeParser(projectDir, nil).Parse(srcPath, false, ParseOptions{})
	if err != nil {
		t.Fatalf("parse %s: %v", name, err)
	}
	return pf
}

// wantNode asserts a node exists with the given label and name, and that it is
// contained by the expected parent.
func wantNode(t *testing.T, got map[[2]string]kvNode, label, name, parent, pLabel string) {
	t.Helper()
	n, ok := got[[2]string{label, name}]
	if !ok {
		t.Errorf("no %s node named %q", label, name)
		return
	}
	if n.parent != parent || n.pLabel != pLabel {
		t.Errorf("%s %q is contained by %s %q, want %s %q",
			label, name, n.pLabel, n.parent, pLabel, parent)
	}
}

func TestXMLKeysAndValuesAreBothNodes(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	got := nodesOf(parseFixture(t, projectDir, "a.xml", `<?xml version="1.0"?>
<config env="prod">
  <database host="db.example.com">
    <port>5432</port>
  </database>
  <empty flag="true"/>
</config>
`))

	wantNode(t, got, "Element", "config", "", "")
	wantNode(t, got, "Attribute", "env", "config", "Element")
	wantNode(t, got, "AttributeValue", "prod", "env", "Attribute")
	wantNode(t, got, "Attribute", "host", "database", "Element")
	wantNode(t, got, "AttributeValue", "db.example.com", "host", "Attribute")
	// Self-closing tags carry attributes too.
	wantNode(t, got, "Attribute", "flag", "empty", "Element")
	wantNode(t, got, "AttributeValue", "true", "flag", "Attribute")
	// Character data is the element's value.
	wantNode(t, got, "Text", "5432", "port", "Element")

	// The quotes tree-sitter-xml leaves on AttValue are not part of the value.
	for key := range got {
		if key[0] == "AttributeValue" && (key[1][0] == '"' || key[1][0] == '\'') {
			t.Errorf("AttributeValue %q kept its delimiters", key[1])
		}
	}
	// Whitespace between tags is not a Text node.
	for key := range got {
		if key[0] == "Text" && key[1] != "5432" {
			t.Errorf("unexpected Text node %q", key[1])
		}
	}
	// The value is also on the key, so a single node answers "what is env set to".
	if v := got[[2]string{"Attribute", "env"}].value; v != "prod" {
		t.Errorf("Attribute env has value %q, want %q", v, "prod")
	}
}

func TestJSONKeysAndValuesAreBothNodes(t *testing.T) {
	projectDir := stageGrammar(t, "json", "tree-sitter-json", ".json", "json.yaml")
	got := nodesOf(parseFixture(t, projectDir, "a.json", `{
  "name": "graphit",
  "port": 5432,
  "debug": true,
  "database": { "host": "db.example.com" },
  "tags": ["alpha", "beta"]
}
`))

	wantNode(t, got, "Pair", "name", "", "")
	wantNode(t, got, "Value", "graphit", "name", "Pair")
	wantNode(t, got, "Value", "5432", "port", "Pair")
	wantNode(t, got, "Value", "true", "debug", "Pair")
	// A nested member is still a Pair with its own value, and it is contained
	// by the member that encloses it rather than by the file.
	wantNode(t, got, "Pair", "host", "database", "Pair")
	wantNode(t, got, "Value", "db.example.com", "host", "Pair")
	// Array elements belong to the key that names the array.
	wantNode(t, got, "Value", "alpha", "tags", "Pair")
	wantNode(t, got, "Value", "beta", "tags", "Pair")

	// The key used to be captured as the whole string literal, quotes included.
	for key := range got {
		if key[1][0] == '"' {
			t.Errorf("%s node %q kept its quotes", key[0], key[1])
		}
	}
}

func TestYAMLKeysAndValuesAreBothNodes(t *testing.T) {
	projectDir := stageGrammar(t, "yaml_lang", "tree-sitter-yaml", ".yaml", "yaml_lang.yaml")
	got := nodesOf(parseFixture(t, projectDir, "a.yaml", `name: graphit
port: 5432
database:
  host: db.example.com
tags:
  - alpha
  - beta
`))

	wantNode(t, got, "Mapping", "name", "", "")
	wantNode(t, got, "Value", "graphit", "name", "Mapping")
	wantNode(t, got, "Value", "5432", "port", "Mapping")
	wantNode(t, got, "Mapping", "host", "database", "Mapping")
	wantNode(t, got, "Value", "db.example.com", "host", "Mapping")
	wantNode(t, got, "Value", "alpha", "tags", "Mapping")
	wantNode(t, got, "Value", "beta", "tags", "Mapping")
}

func TestTOMLKeysAndValuesAreBothNodes(t *testing.T) {
	projectDir := stageGrammar(t, "toml", "tree-sitter-toml", ".toml", "toml.yaml")
	got := nodesOf(parseFixture(t, projectDir, "a.toml", `name = "graphit"
port = 5432
[database]
host = "db.example.com"
tags = ["alpha"]
`))

	wantNode(t, got, "Pair", "name", "", "")
	wantNode(t, got, "Value", "graphit", "name", "Pair")
	wantNode(t, got, "Value", "5432", "port", "Pair")
	wantNode(t, got, "Value", "db.example.com", "host", "Pair")
	wantNode(t, got, "Value", "alpha", "tags", "Pair")

	for key := range got {
		if key[1][0] == '"' {
			t.Errorf("%s node %q kept its quotes", key[0], key[1])
		}
	}
}

func TestHTMLAttributesAreKeyValueNodes(t *testing.T) {
	projectDir := stageGrammar(t, "html", "tree-sitter-html", ".html", "html.yaml")
	pf := parseFixture(t, projectDir, "a.html", `<div id="main">
  <a href="/orders">Orders</a>
  <img src="/logo.png"/>
</div>
`)
	got := nodesOf(pf)

	wantNode(t, got, "Attribute", "id", "div", "Element")
	wantNode(t, got, "AttributeValue", "main", "id", "Attribute")
	wantNode(t, got, "Attribute", "href", "a", "Element")
	wantNode(t, got, "AttributeValue", "/orders", "href", "Attribute")
	wantNode(t, got, "Attribute", "src", "img", "Element")
	wantNode(t, got, "AttributeValue", "/logo.png", "src", "Attribute")
	wantNode(t, got, "Text", "Orders", "a", "Element")

	// The `@_attr` capture only exists for the predicate to test. It used to be
	// emitted as a reference target of its own, so `id="main"` produced a
	// REFERENCES edge to the literal name `id` next to the one to `main`.
	for _, r := range pf.References {
		switch r.TargetName {
		case "id", "class", "href", "src", "action", "name", "for", "role":
			t.Errorf("attribute name %q became a REFERENCES target", r.TargetName)
		}
	}
	// The references themselves survive: containment says a value belongs to a
	// key, not that it points somewhere.
	var sawHref bool
	for _, r := range pf.References {
		if r.TargetName == "/orders" && r.RelType == "REFERENCES" {
			sawHref = true
		}
	}
	if !sawHref {
		t.Error("href value lost its REFERENCES edge")
	}
}

func TestHCLBlockLabelsAreNotAllEntities(t *testing.T) {
	projectDir := stageGrammar(t, "hcl", "tree-sitter-hcl", ".tf", "hcl.yaml")
	got := nodesOf(parseFixture(t, projectDir, "a.tf", `resource "aws_instance" "web" {
  ami = "ami-123"
  count = 2
}

variable "region" {
  default = "us-east-1"
}
`))

	// Only the block name is the entity; the block type and the resource type are
	// there for the predicate to test.
	wantNode(t, got, "Resource", "web", "", "")
	if _, bad := got[[2]string{"Resource", "resource"}]; bad {
		t.Error("the block-type keyword became a Resource node")
	}
	if _, bad := got[[2]string{"Resource", "aws_instance"}]; bad {
		t.Error("the resource type became a Resource node")
	}
	// A `variable` block is not also an Output, a Module and a Provider.
	wantNode(t, got, "Variable", "region", "", "")
	for _, label := range []string{"Output", "Module", "Provider"} {
		if _, bad := got[[2]string{label, "region"}]; bad {
			t.Errorf("variable region also became a %s", label)
		}
	}

	// An attribute belongs to the block INSTANCE — "web", not the type "aws_instance",
	// which is not an entity at all. Reaching the instance needs the indexed name-path
	// segment (`string_lit[1]`); the first label would name a node that never exists.
	wantNode(t, got, "Attribute", "ami", "web", "Resource")
	wantNode(t, got, "Value", "ami-123", "ami", "Attribute")
	wantNode(t, got, "Value", "2", "count", "Attribute")
}

// A value that is a document, not a name, is not indexed as one: its text would
// become a UID, an FTS row and a bag of trigrams, and none of those is what a
// name is for.
func TestOversizedAndMultilineValuesAreNotNodes(t *testing.T) {
	long := make([]byte, maxDataValueLen+1)
	for i := range long {
		long[i] = 'x'
	}
	if got := dataText(string(long)); got != "" {
		t.Errorf("a %d-char value was accepted", len(long))
	}
	if got := dataText("linha um\nlinha dois"); got != "" {
		t.Errorf("a multi-line value was accepted: %q", got)
	}
	if got := dataText("   "); got != "" {
		t.Errorf("a blank value was accepted: %q", got)
	}
	if got := dataText(`"prod"`); got != "prod" {
		t.Errorf("dataText(%q) = %q, want %q", `"prod"`, got, "prod")
	}
	if got := dataText(`'prod'`); got != "prod" {
		t.Errorf("dataText(%q) = %q, want %q", `'prod'`, got, "prod")
	}
	// A lone delimiter is not a pair of them.
	if got := dataText(`it's`); got != "it's" {
		t.Errorf("dataText(%q) = %q, want %q", `it's`, got, "it's")
	}
}

// The key/value pair has to survive into the graph as an edge, not just as two
// unrelated nodes.
func TestKeyContainsValueEdgeReachesTheGraph(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "a.xml", `<config env="prod"/>
`)
	entry := ConvertToCache(pf, projectDir, false, "")
	if entry == nil {
		t.Fatal("no cache entry")
	}

	uidOf := map[[2]string]string{}
	for _, e := range entry.Entities {
		uidOf[[2]string{e.Label, e.Name}] = e.UID
	}
	keyUID := uidOf[[2]string{"Attribute", "env"}]
	valUID := uidOf[[2]string{"AttributeValue", "prod"}]
	if keyUID == "" || valUID == "" {
		t.Fatalf("missing nodes: key=%q value=%q", keyUID, valUID)
	}

	var found bool
	for _, ce := range entry.ContainsEdges {
		if ce.ParentUID == keyUID && ce.ChildUID == valUID &&
			ce.ParentLabel == "Attribute" && ce.ChildLabel == "AttributeValue" {
			found = true
		}
	}
	if !found {
		t.Errorf("no Attribute→AttributeValue CONTAINS edge; got %+v", entry.ContainsEdges)
	}
}

// The point of making the value a node is that the search index reads a node's
// name and docstring and never its value. A value stored only as a property is
// unreachable by the tool people actually use to find things.
func TestDataValuesAreReachableByFullTextSearch(t *testing.T) {
	projectDir := stageGrammar(t, "xml", "tree-sitter-xml", ".xml", "xml.yaml")
	pf := parseFixture(t, projectDir, "beans.xml", `<beans>
  <bean class="com.acme.OrderRepository" scope="singleton">
    <property name="datasource" value="jdbc:postgresql://reporting-db:5432/orders"/>
  </bean>
</beans>
`)
	entry := ConvertToCache(pf, projectDir, false, "")
	if entry == nil {
		t.Fatal("no cache entry")
	}

	dir := t.TempDir()
	cache, err := NewShardCache(dir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	if err := cache.Store("beans.xml", "h1", entry); err != nil {
		t.Fatalf("store: %v", err)
	}
	if err := cache.FlushDirty(); err != nil {
		t.Fatal(err)
	}
	si := buildSearchIndex(t, dir, cache, nil)

	// Each of these is a value, not a key. None of them was findable before.
	for _, probe := range []string{"OrderRepository", "singleton", "reporting-db"} {
		res, err := si.Search(context.Background(), probe, 20)
		if err != nil {
			t.Fatalf("search %q: %v", probe, err)
		}
		var hit bool
		for _, r := range res {
			if r.Type != "file" && strings.Contains(r.Name, probe) {
				hit = true
				break
			}
		}
		if !hit {
			t.Errorf("value %q is not reachable by search; got %d results", probe, len(res))
		}
	}
}

// stageDataFormats stages every data-format grammar into one project so a single
// index run covers them all.
func stageDataFormats(t *testing.T) string {
	t.Helper()
	projectDir := t.TempDir()
	qdir := filepath.Join(projectDir, brand.DotDir(), "ast", "queries")
	if err := os.MkdirAll(qdir, 0o755); err != nil {
		t.Fatal(err)
	}
	for _, g := range []struct{ lang, grammar, ext, file string }{
		{"xml", "tree-sitter-xml", ".xml", "xml.yaml"},
		{"json", "tree-sitter-json", ".json", "json.yaml"},
		{"yaml_lang", "tree-sitter-yaml", ".yaml", "yaml_lang.yaml"},
		{"toml", "tree-sitter-toml", ".toml", "toml.yaml"},
	} {
		body, err := os.ReadFile(filepath.Join("queries", g.file))
		if err != nil {
			t.Skipf("no %s: %v", g.file, err)
		}
		if lang, err := resolveTreeSitterLang(g.lang, g.grammar); err != nil || lang == nil {
			t.Skipf("%s grammar unavailable: %v", g.lang, err)
		}
		if err := os.WriteFile(filepath.Join(qdir, g.file), body, 0o644); err != nil {
			t.Fatal(err)
		}

		ext, cfg := g.ext, &tsLangConfig{
			Language: g.lang, Grammar: g.grammar, Extensions: []string{g.ext},
		}
		restore, had := tsExtMap[ext]
		extTablesMu.Lock()
		tsExtMap[ext] = cfg
		extTablesMu.Unlock()
		t.Cleanup(func() {
			extTablesMu.Lock()
			if had {
				tsExtMap[ext] = restore
			} else {
				delete(tsExtMap, ext)
			}
			extTablesMu.Unlock()
		})
	}
	return projectDir
}

// The graph has to answer the question the change exists for: given a key, what
// is it set to — as a traversal, in Cypher, against a real database.
func TestDataFormatGraphIsQueryable(t *testing.T) {
	proj := stageDataFormats(t)

	sources := []struct{ name, src string }{
		{"web.xml", `<web-app>
  <servlet name="orders" class="com.acme.OrderServlet">
    <timeout>30</timeout>
  </servlet>
</web-app>
`},
		{"package.json", `{"name": "acme-web", "version": "2.1.0"}`},
		{"compose.yaml", "services:\n  api:\n    image: acme/api:2.1.0\n"},
		{"config.toml", "[server]\nlisten = \"0.0.0.0:8080\"\n"},
	}

	cacheDir := t.TempDir()
	cache, err := NewShardCache(cacheDir)
	if err != nil {
		t.Fatalf("shard cache: %v", err)
	}
	defer func() { _ = cache.Close() }()

	comp := NewCompositeParser(proj, nil)
	for _, s := range sources {
		p := filepath.Join(proj, s.name)
		if err := os.WriteFile(p, []byte(s.src), 0o644); err != nil {
			t.Fatal(err)
		}
		pf, err := comp.Parse(p, false, ParseOptions{IndexSource: true})
		if err != nil {
			t.Fatalf("parse %s: %v", s.name, err)
		}
		entry := ConvertToCache(pf, proj, true, "")
		if entry == nil || len(entry.Entities) == 0 {
			t.Fatalf("%s: nothing cached", s.name)
		}
		if err := cache.Store(entry.RelPath, "h-"+entry.RelPath, entry); err != nil {
			t.Fatalf("store %s: %v", s.name, err)
		}
	}

	dbPath := filepath.Join(t.TempDir(), "ladybugdb")
	writer := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := writer.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	ctx := context.Background()
	if err := RebuildFromJSON(ctx, writer, cache, nil, "", proj, nil); err != nil {
		_ = writer.Close()
		t.Fatalf("rebuild: %v", err)
	}
	_ = writer.Close()

	db := NewLadybugDB(LadybugConfig{DBPath: dbPath})
	if err := db.connect(); err != nil {
		t.Fatalf("reopen after swap: %v", err)
	}
	defer func() { _ = db.Close() }()

	probes := []struct {
		what, query, wantKey, wantValue string
	}{
		{
			what:      "an XML attribute and its value",
			query:     "MATCH (a:`Attribute`)-[:CONTAINS]->(v:`AttributeValue`) WHERE a.name = 'class' RETURN a.name AS k, v.name AS v",
			wantKey:   "class",
			wantValue: "com.acme.OrderServlet",
		},
		{
			what:      "an XML element and its text",
			query:     "MATCH (e:`Element`)-[:CONTAINS]->(t:`Text`) WHERE e.name = 'timeout' RETURN e.name AS k, t.name AS v",
			wantKey:   "timeout",
			wantValue: "30",
		},
		{
			what:      "a JSON member and its value",
			query:     "MATCH (p:`Pair`)-[:CONTAINS]->(v:`Value`) WHERE p.name = 'version' RETURN p.name AS k, v.name AS v",
			wantKey:   "version",
			wantValue: "2.1.0",
		},
		{
			what:      "a YAML mapping and its value",
			query:     "MATCH (m:`Mapping`)-[:CONTAINS]->(v:`Value`) WHERE m.name = 'image' RETURN m.name AS k, v.name AS v",
			wantKey:   "image",
			wantValue: "acme/api:2.1.0",
		},
		{
			what:      "a TOML pair and its value",
			query:     "MATCH (p:`Pair`)-[:CONTAINS]->(v:`Value`) WHERE p.name = 'listen' RETURN p.name AS k, v.name AS v",
			wantKey:   "listen",
			wantValue: "0.0.0.0:8080",
		},
	}

	for _, probe := range probes {
		rows, err := db.Query(ctx, probe.query, nil)
		if err != nil {
			t.Errorf("%s: query failed: %v", probe.what, err)
			continue
		}
		if len(rows.Records) == 0 {
			t.Errorf("%s: no CONTAINS edge from the key to its value", probe.what)
			continue
		}
		rec := rows.Records[0]
		if rec["k"] != probe.wantKey || rec["v"] != probe.wantValue {
			t.Errorf("%s: got %v -> %v, want %q -> %q",
				probe.what, rec["k"], rec["v"], probe.wantKey, probe.wantValue)
		}
	}

	// The value also sits on the key, so the one-node answer works too.
	rows, err := db.Query(ctx,
		"MATCH (p:`Pair`) WHERE p.name = 'version' RETURN p.value AS v", nil)
	if err != nil {
		t.Fatalf("query value property: %v", err)
	}
	if len(rows.Records) == 0 || rows.Records[0]["v"] != "2.1.0" {
		t.Errorf("Pair version has value %v, want 2.1.0", rows.Records)
	}
}

// Helper captures are not entities in any language. The `@_` prefix is a
// convention the query files use throughout, and it only means anything if the
// executor honours name_capture — which it did not, so the keyword a predicate
// tests became a node of its own carrying the query's label.
func TestHelperCapturesAreNotEntities(t *testing.T) {
	cases := []struct {
		lang, ext, queryFile, file, source string
		// wantNodes and rejectNodes are {label, name}: the same text can be a
		// legitimate entity under one label and a helper capture under another.
		// `defn` really is a call in Clojure — every list's head symbol is —
		// but it is not a Function.
		wantNodes   [][2]string
		rejectNodes [][2]string
		// rejectRefs are {relation_type, target}: a keyword must not be the
		// thing an edge points at.
		rejectRefs [][2]string
	}{
		{
			lang: "ruby", ext: ".rb", queryFile: "ruby.yaml", file: "a.rb",
			source: `require "json"
class Pedido
  include Comparable
end
`,
			wantNodes:  [][2]string{{"Class", "Pedido"}},
			rejectRefs: [][2]string{{"IMPORTS", "require"}, {"INCLUDES", "include"}},
		},
		{
			lang: "clojure", ext: ".clj", queryFile: "clojure.yaml", file: "a.clj",
			source: `(ns acme.pedido)
(defn faturar [p] p)
`,
			wantNodes:   [][2]string{{"Namespace", "acme.pedido"}, {"Function", "faturar"}},
			rejectNodes: [][2]string{{"Namespace", "ns"}, {"Function", "defn"}},
			rejectRefs:  [][2]string{{"IMPORTS", "require"}},
		},
		{
			lang: "graphql", ext: ".graphql", queryFile: "graphql.yaml", file: "a.graphql",
			source: `query GetOrders { orders { id } }
mutation CreateOrder { createOrder { id } }
`,
			wantNodes: [][2]string{{"Query", "GetOrders"}, {"Mutation", "CreateOrder"}},
			rejectNodes: [][2]string{
				{"Query", "query"}, {"Query", "mutation"},
				{"Mutation", "mutation"}, {"Mutation", "query"},
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.lang, func(t *testing.T) {
			projectDir := stageGrammar(t, tc.lang, "tree-sitter-"+tc.lang, tc.ext, tc.queryFile)
			pf := parseFixture(t, projectDir, tc.file, tc.source)
			got := nodesOf(pf)

			for _, want := range tc.wantNodes {
				if !got[want].present {
					t.Errorf("no %s node named %q", want[0], want[1])
				}
			}
			for _, reject := range tc.rejectNodes {
				if got[reject].present {
					t.Errorf("helper capture %q became a %s node", reject[1], reject[0])
				}
			}
			for _, reject := range tc.rejectRefs {
				for _, r := range pf.References {
					if r.RelType == reject[0] && r.TargetName == reject[1] {
						t.Errorf("helper capture %q became a %s target", reject[1], reject[0])
					}
				}
			}
		})
	}
}
