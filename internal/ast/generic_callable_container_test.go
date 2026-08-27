package ast

import "testing"

// These three grammars declare a callable from a GENERIC node — clojure's `list_lit`,
// julia's `assignment`, r's `binary_operator` — which is why
// TestEveryCallableContainerIsDeclaredAsAContext exempts them instead of demanding
// they be contexts: making a bare list or a bare assignment a container would name the
// wrong owner for everything nested in one.
//
// The exemptions each claim that nothing is lost anyway. These tests are that claim,
// checked against the real grammars rather than asserted in a comment.

// r resolves a parameter through function_definition, which IS a declared context,
// even though the Function entity is named from the binary_operator around it.
func TestRParametersBelongToTheirFunction(t *testing.T) {
	projectDir := stageGrammar(t, "r", "tree-sitter-r", ".R", "r.yaml")
	pf := parseFixture(t, projectDir, "charge.R", `charge_card <- function(amount, currency) {
  authorize(amount)
}
`)

	fns := entitiesOfLabel(pf, "Function")
	if len(fns) == 0 {
		t.Fatal("no Function extracted — the fixture or the grammar changed")
	}
	if fns[0].Name != "charge_card" {
		t.Errorf("Function named %q, want charge_card", fns[0].Name)
	}

	params := entitiesOfLabel(pf, "Parameter")
	if len(params) == 0 {
		t.Fatal("no Parameter extracted — nothing to attribute, and the exemption's " +
			"premise no longer holds")
	}
	for _, p := range params {
		if p.Context != "charge_card" || p.ContextType != "Function" {
			t.Errorf("%s owned by %s %q, want Function %q",
				p.Name, p.ContextType, p.Context, "charge_card")
		}
		if p.Context == "function" {
			t.Errorf("%s is owned by the `function` KEYWORD — the ancestor walk is "+
				"naming function_definition after its own keyword", p.Name)
		}
	}

	assertNoDanglingContains(t, pf, projectDir)
}

// An anonymous function has no name of its own to state, so its parameters are attributed
// to the named function whose body they sit in — not the lambda, which r never names, but
// where the code actually lives.
//
// This is what `binary_operator: Function` with a `lhs` name path buys: the lhs is the very
// node the Function entity is named from, so the owner always exists. Before it, r had no
// usable context at all: function_definition has no "name" field and the lookup returned
// the `function` KEYWORD, so everything came out owned by a Function named "function".
func TestRAnonymousFunctionParametersBelongToTheEnclosingFunction(t *testing.T) {
	projectDir := stageGrammar(t, "r", "tree-sitter-r", ".R", "r.yaml")
	pf := parseFixture(t, projectDir, "anon.R", `charge_card <- function(amount) {
  sapply(items, function(item) item * 2)
}
`)

	owners := map[string]string{}
	for _, p := range entitiesOfLabel(pf, "Parameter") {
		owners[p.Name] = p.Context
		if p.Context == "function" {
			t.Errorf("parameter %q is owned by the `function` KEYWORD", p.Name)
		}
	}
	if owners["item"] != "charge_card" {
		t.Errorf("the lambda's parameter is owned by %q, want the enclosing function %q",
			owners["item"], "charge_card")
	}
	if owners["amount"] != "charge_card" {
		t.Errorf("the named function's parameter is owned by %q, want %q",
			owners["amount"], "charge_card")
	}
	assertNoDanglingContains(t, pf, projectDir)
}

// julia keeps a function's name inside its signature, so the ancestor walk found
// function_definition, could not name it, and left the parameters with an EMPTY context
// — which ConvertToCache discards. Containment is stated by the pattern now.
func TestJuliaParametersBelongToTheirFunction(t *testing.T) {
	projectDir := stageGrammar(t, "julia", "tree-sitter-julia", ".jl", "julia.yaml")
	pf := parseFixture(t, projectDir, "charge.jl", `function charge_card(amount, currency)
    authorize(amount)
end
`)

	fns := entitiesOfLabel(pf, "Function")
	if len(fns) == 0 {
		t.Fatal("no Function extracted — the fixture or the grammar changed")
	}

	params := entitiesOfLabel(pf, "Parameter")
	if len(params) == 0 {
		t.Fatal("no Parameter extracted — nothing to attribute, and the exemption's " +
			"premise no longer holds")
	}
	for _, p := range params {
		if p.Context != "charge_card" || p.ContextType != "Function" {
			t.Errorf("%s owned by %s %q, want Function %q",
				p.Name, p.ContextType, p.Context, "charge_card")
		}
	}

	assertNoDanglingContains(t, pf, projectDir)
}

// julia's SHORT form, `f(x) = ...`, is where the exemption's reasoning actually bites:
// the Parameter query requires a `signature` node, which the short form has none of, so
// its arguments are not extracted at all. Nothing is misattributed and nothing is
// dropped by ConvertToCache — there is simply no Parameter to place. Pinned here so the
// difference is a known property rather than a surprise.
func TestJuliaShortFormFunctionHasNoParameters(t *testing.T) {
	projectDir := stageGrammar(t, "julia", "tree-sitter-julia", ".jl", "julia.yaml")
	pf := parseFixture(t, projectDir, "short.jl", `charge(amount) = authorize(amount)
`)

	var found bool
	for _, f := range entitiesOfLabel(pf, "Function") {
		if f.Name == "charge" {
			found = true
		}
	}
	if !found {
		t.Error("the short form should still yield the Function entity")
	}

	for _, p := range entitiesOfLabel(pf, "Parameter") {
		if p.ContextType == "" || p.Context == "" {
			t.Errorf("parameter %q came out with no owner — ConvertToCache would drop it",
				p.Name)
		}
	}
}

// clojure declares no Parameter query at all, so the question of attributing one does
// not arise, and the grammar is listed in flatLanguages for that reason. What it DOES
// extract must still land.
func TestClojureExtractsTopLevelFormsWithoutParameters(t *testing.T) {
	projectDir := stageGrammar(t, "clojure", "tree-sitter-clojure", ".clj", "clojure.yaml")
	pf := parseFixture(t, projectDir, "core.clj", `(ns payments.core)

(defn charge-card [amount currency]
  (authorize amount))

(defrecord Card [number holder])
`)

	names := map[string]bool{}
	for _, label := range []string{"Function", "Namespace", "Record"} {
		for _, e := range entitiesOfLabel(pf, label) {
			names[e.Name] = true
		}
	}
	for _, want := range []string{"charge-card", "payments.core", "Card"} {
		if !names[want] {
			t.Errorf("%q was not extracted", want)
		}
	}

	if params := entitiesOfLabel(pf, "Parameter"); len(params) != 0 {
		t.Errorf("clojure.yaml declares no Parameter query, yet %d were extracted — "+
			"the flatLanguages exemption needs revisiting", len(params))
	}
}

// TestTomlPairsBelongToTheirTable is the same failure html.yaml had, found by auditing
// non-callable containers: `table` was a declared context with no context_name_paths, so
// tree-sitter-toml's nameless `table` node was transparent and every pair fell back to
// the File.
func TestTomlPairsBelongToTheirTable(t *testing.T) {
	projectDir := stageGrammar(t, "toml", "tree-sitter-toml", ".toml", "toml.yaml")
	pf := parseFixture(t, projectDir, "cfg.toml", `[database]
host = "localhost"
port = 5432
`)

	pairs := entitiesOfLabel(pf, "Pair")
	if len(pairs) == 0 {
		t.Fatal("no Pair extracted — the fixture or the grammar changed")
	}
	for _, p := range pairs {
		if p.Context != "database" || p.ContextType != "Table" {
			t.Errorf("pair %q owned by %s %q, want Table %q",
				p.Name, p.ContextType, p.Context, "database")
		}
	}

	assertNoDanglingContains(t, pf, projectDir)
}

// A table is keyed either way — [database] is a bare_key, [server.http] a dotted_key — and a
// name path now declares both as ordered alternatives. Before that, only bare keys resolved
// and every pair under a dotted table fell back to the File.
func TestTomlDottedTablePairsBelongToTheirTable(t *testing.T) {
	projectDir := stageGrammar(t, "toml", "tree-sitter-toml", ".toml", "toml.yaml")
	pf := parseFixture(t, projectDir, "dotted.toml", `[server.http]
port = 8080
`)

	pairs := entitiesOfLabel(pf, "Pair")
	if len(pairs) == 0 {
		t.Fatal("no Pair extracted — the fixture or the grammar changed")
	}
	for _, p := range pairs {
		if p.Context != "server.http" || p.ContextType != "Table" {
			t.Errorf("pair %q owned by %s %q, want Table %q",
				p.Name, p.ContextType, p.Context, "server.http")
		}
	}
	assertNoDanglingContains(t, pf, projectDir)
}

// The five grammars below shipped with every context INERT: the grammar defines no field
// called "name", the query file declared no context_name_paths, so nameNodeOf returned nil
// for every container and each one was skipped. Every entity fell back to the File.
//
// They were found by TestNamelessContextsDeclareANamePath, which infers it from the
// grammar rather than from a fixture: no "name" field in the grammar means no node can
// have one. These tests are the confirmation, and the regression net.

func TestProtobufMembersBelongToTheirContainer(t *testing.T) {
	projectDir := stageGrammar(t, "protobuf", "tree-sitter-proto", ".proto", "protobuf.yaml")
	pf := parseFixture(t, projectDir, "pay.proto", `message Charge {
  string card_id = 1;
}

service Payments {
  rpc DoCharge(Charge) returns (Charge);
}
`)

	want := map[string]struct{ label, context, contextType string }{
		"card_id":  {"MessageField", "Charge", "Message"},
		"DoCharge": {"RPC", "Payments", "Service"},
	}
	got := map[string]Entity{}
	for _, l := range []string{"MessageField", "RPC"} {
		for _, e := range entitiesOfLabel(pf, l) {
			got[e.Name] = e
		}
	}
	for name, exp := range want {
		e, ok := got[name]
		if !ok {
			t.Errorf("%s was not extracted", name)
			continue
		}
		if e.Context != exp.context || e.ContextType != exp.contextType {
			t.Errorf("%s owned by %s %q, want %s %q",
				name, e.ContextType, e.Context, exp.contextType, exp.context)
		}
	}

	assertNoDanglingContains(t, pf, projectDir)
}

func TestGraphQLFieldsBelongToTheirType(t *testing.T) {
	projectDir := stageGrammar(t, "graphql", "tree-sitter-graphql", ".graphql", "graphql.yaml")
	pf := parseFixture(t, projectDir, "schema.graphql", `type Charge {
  id: ID!
  amount: Int!
}
`)

	fields := entitiesOfLabel(pf, "GraphQLField")
	if len(fields) == 0 {
		t.Fatal("no GraphQLField extracted — the fixture or the grammar changed")
	}
	for _, f := range fields {
		if f.Context != "Charge" || f.ContextType != "ObjectType" {
			t.Errorf("field %q owned by %s %q, want ObjectType %q",
				f.Name, f.ContextType, f.Context, "Charge")
		}
	}

	assertNoDanglingContains(t, pf, projectDir)
}

// The framework ships no markdown query file — documents are the knowledge wiki's,
// and in the code graph they were a Heading node per section. The grammar stays
// registered, so a project that does want markdown structure gets it by writing
// this file into ast.queries_dir, and that is what is exercised here: the query
// file below is the project's, not one read out of queries/.
//
// A grammar that is registered but reachable by nothing is indistinguishable from
// one that was removed, and it would rot without noticing. This is the test that
// notices.
func TestMarkdownContentBelongsToItsHeadingWhenAProjectOptsIn(t *testing.T) {
	const queries = `language: markdown
grammar: tree-sitter-markdown
extensions: [".md"]
queries:
  - data_key: headings
    graph_label: Heading
    pattern: '(atx_heading heading_content: (inline) @name)'
  - data_key: code_blocks
    graph_label: CodeBlock
    pattern: '(fenced_code_block (info_string) @name)'
exports:
  strategy: none
self_keywords: []
context_types:
  section: Heading
context_name_paths:
  section: atx_heading/heading_content
anon_func_types: []
declaration_types:
  - atx_heading
  - setext_heading
comment_types:
  - html_comment
`

	projectDir := stageGrammarWithQueries(t, "markdown", "tree-sitter-markdown", ".md",
		"markdown.yaml", queries)
	pf := parseFixture(t, projectDir, "doc.md", "# Title\n\n```go\nx := 1\n```\n")

	blocks := entitiesOfLabel(pf, "CodeBlock")
	if len(blocks) == 0 {
		t.Fatal("no CodeBlock extracted — the project's opt-in query file no longer " +
			"resolves the registered markdown grammar")
	}
	for _, b := range blocks {
		if b.Context != "Title" || b.ContextType != "Heading" {
			t.Errorf("code block %q owned by %s %q, want Heading %q",
				b.Name, b.ContextType, b.Context, "Title")
		}
	}

	assertNoDanglingContains(t, pf, projectDir)
}

// elixir needed both halves: a name path, because every container is a `call` and only a
// defmodule call has an alias to be named by; and parent_capture on the Parameter query,
// because a `def` call cannot be named that way and its parameters would otherwise be
// attributed to the module instead of the function.
func TestElixirFunctionAndParameterOwners(t *testing.T) {
	projectDir := stageGrammar(t, "elixir", "tree-sitter-elixir", ".ex", "elixir.yaml")
	pf := parseFixture(t, projectDir, "pay.ex", `defmodule Pay do
  def charge(amount, currency) do
    amount
  end
end
`)

	var sawFn bool
	for _, f := range entitiesOfLabel(pf, "Function") {
		if f.Name != "charge" {
			continue
		}
		sawFn = true
		if f.Context != "Pay" || f.ContextType != "Module" {
			t.Errorf("function charge owned by %s %q, want Module %q",
				f.ContextType, f.Context, "Pay")
		}
	}
	if !sawFn {
		t.Error("the function was not extracted")
	}

	params := entitiesOfLabel(pf, "Parameter")
	if len(params) == 0 {
		t.Fatal("no Parameter extracted")
	}
	for _, p := range params {
		if p.Context != "charge" || p.ContextType != "Function" {
			t.Errorf("parameter %q owned by %s %q, want Function %q — a parameter belongs "+
				"to its function, not to the module", p.Name, p.ContextType, p.Context, "charge")
		}
	}

	assertNoDanglingContains(t, pf, projectDir)
}

// assertNoDanglingContains is the check that would have caught two of my own fixes.
//
// ConvertToCache does not verify that a context names something real: when it cannot find
// the parent among the entities it SYNTHESIZES a UID (entityUID(relPath, e.Context, "")) and
// emits the edge anyway. The result is a CONTAINS edge to a node that is never created —
// the same shape as the historical "Table does not exist" failure that took a whole rebuild
// down, and now more dangerous, because a failed COPY aborts the rebuild instead of being
// logged.
//
// So a context_name_paths entry is only correct if the name it resolves EQUALS the name of
// an entity the queries actually produce.
func assertNoDanglingContains(t *testing.T, pf *ParsedFile, projectDir string) {
	t.Helper()

	entry := ConvertToCache(pf, projectDir, true, "")
	if entry == nil {
		t.Fatal("ConvertToCache returned nothing")
	}
	uids := make(map[string]bool, len(entry.Entities))
	for _, e := range entry.Entities {
		uids[e.UID] = true
	}
	for _, ce := range entry.ContainsEdges {
		if !uids[ce.ParentUID] {
			t.Errorf("CONTAINS %s(%s) -> %s(%s): the parent does not exist. A context is "+
				"naming something the queries never produce, so the edge points at a "+
				"synthesized UID.",
				ce.ParentLabel, ce.ParentUID, ce.ChildLabel, ce.ChildUID)
		}
	}
}

// hcl needed the indexed segment to be correct at all: `resource "aws_s3_bucket" "logs"` is
// a block with TWO string_lit labels and the entity is the SECOND. Naming the context after
// the first gave the block TYPE, which is deliberately not an entity, so every CONTAINS edge
// pointed at a synthesized parent UID that no node ever gets — caught by
// assertNoDanglingContains, which is why that helper exists.
//
// The second alternative covers one-label blocks, whose only label IS the name.
func TestHCLAttributesBelongToTheirBlock(t *testing.T) {
	projectDir := stageGrammar(t, "hcl", "tree-sitter-hcl", ".tf", "hcl.yaml")
	pf := parseFixture(t, projectDir, "main.tf", `resource "aws_s3_bucket" "logs" {
  bucket = "my-logs"
}

variable "region" {
  default = "us-east-1"
}
`)

	owners := map[string]string{}
	for _, a := range entitiesOfLabel(pf, "Attribute") {
		owners[a.Name] = a.Context
	}
	if owners["bucket"] != "logs" {
		t.Errorf("two-label block: attribute owned by %q, want the INSTANCE %q — %q would "+
			"be the block type, which is not an entity",
			owners["bucket"], "logs", "aws_s3_bucket")
	}
	if owners["default"] != "region" {
		t.Errorf("one-label block: attribute owned by %q, want %q", owners["default"], "region")
	}
	assertNoDanglingContains(t, pf, projectDir)
}

// Every declaration in elixir is a `call`, so a pattern without a predicate on the target
// matches ordinary calls too: `def charge(amount, currency)` produced Function entities named
// `amount` and `currency` — the arguments of the inner call — and `alias Other.Helper`
// produced a Module. Pre-existing noise in the graph of any elixir project.
func TestElixirDoesNotTurnArgumentsIntoFunctions(t *testing.T) {
	projectDir := stageGrammar(t, "elixir", "tree-sitter-elixir", ".ex", "elixir.yaml")
	pf := parseFixture(t, projectDir, "pay.ex", `defmodule Pay do
  alias Other.Helper

  def charge(amount, currency) do
    Helper.run(amount, currency)
  end
end
`)

	fns := map[string]bool{}
	for _, f := range entitiesOfLabel(pf, "Function") {
		fns[f.Name] = true
	}
	if !fns["charge"] {
		t.Error("the function itself was not extracted")
	}
	for _, arg := range []string{"amount", "currency"} {
		if fns[arg] {
			t.Errorf("the argument %q became a Function — the def/defp predicate is missing", arg)
		}
	}

	mods := map[string]bool{}
	for _, m := range entitiesOfLabel(pf, "Module") {
		mods[m.Name] = true
	}
	if !mods["Pay"] {
		t.Error("the module itself was not extracted")
	}
	if mods["Other.Helper"] {
		t.Error("an alias became a Module — the defmodule predicate is missing")
	}

	assertNoDanglingContains(t, pf, projectDir)
}

// parsePathSegment is the syntax half of the two extensions a name path needed: an
// occurrence index, and ordered alternatives. Unit-tested because a malformed segment fails
// OPEN — it degrades to "the first child of that kind", which makes the context resolve to
// the wrong node instead of to nothing, and a wrong owner is the failure mode this whole
// area keeps producing.
func TestParsePathSegment(t *testing.T) {
	cases := []struct {
		in       string
		wantKind string
		wantIdx  int
	}{
		{"string_lit", "string_lit", -1},
		{"string_lit[0]", "string_lit", 0},
		{"string_lit[1]", "string_lit", 1},
		{"string_lit[10]", "string_lit", 10},
		// Malformed, and each must degrade to "no index" rather than to a wrong one.
		{"string_lit[", "string_lit[", -1},
		{"string_lit]", "string_lit]", -1},
		{"string_lit[]", "string_lit[]", -1},
		{"string_lit[x]", "string_lit[x]", -1},
		{"string_lit[-1]", "string_lit[-1]", -1},
		{"[0]", "[0]", -1},
		{"", "", -1},
	}
	for _, c := range cases {
		kind, idx := parsePathSegment(c.in)
		if kind != c.wantKind || idx != c.wantIdx {
			t.Errorf("parsePathSegment(%q) = (%q, %d), want (%q, %d)",
				c.in, kind, idx, c.wantKind, c.wantIdx)
		}
	}
}
