package ast

import (
	"regexp"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// An agent read the skill, went straight to Cypher, and crashed twice in a row on
// properties that do not exist — `n.type` (the node type is a label) and `n.line`
// (it is `line_number`). The skill had a property table, but nothing told it to
// call the schema tool BEFORE writing a query, so the table only got read after
// the failure. The instruction has to be an up-front step, not a reference page.
func TestASTRuleContentDemandsSchemaBeforeFirstQuery(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	schemaTool := brand.MCPToolName("ast", "schema")
	queryTool := brand.MCPToolName("ast", "query")

	// The mandate must be positional: schema first, query second.
	if !strings.Contains(content, "the first AST call you make is") {
		t.Error("skill does not state that the schema tool is the FIRST call, before any query")
	}
	if !strings.Contains(content, "Schema before Cypher") {
		t.Error("Cypher Guidelines do not carry the schema-first rule")
	}
	if !strings.Contains(content, "Before you write Cypher — not after it fails") {
		t.Error("skill does not rule out calling the schema tool only as failure recovery")
	}

	// And it must land before the first query example, or an agent that reads
	// top-down has already written a query by the time it hears the rule.
	schemaAt := strings.Index(content, "Phase 1: Know the schema — call the schema tool BEFORE your first query")
	if schemaAt < 0 {
		t.Fatal("Phase 1 no longer announces the schema call in its heading")
	}
	if phase3At := strings.Index(content, "Phase 3: Precise Graph Query"); phase3At > 0 && schemaAt > phase3At {
		t.Error("the schema-first instruction comes after the query phase")
	}

	for _, want := range []string{schemaTool, queryTool} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not name %q", want)
		}
	}
}

// The two failures were guesses of a specific kind: a plausible synonym for a real
// property. Naming the wrong ones with their replacements is what makes the rule
// actionable — "don't guess" alone leaves the agent with nothing to write.
func TestASTRuleContentNamesTheInventedProperties(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	if !strings.Contains(content, "Properties that do NOT exist") {
		t.Error("skill has no list of non-existent properties")
	}

	// The two that actually crashed, plus the families around them.
	for _, want := range []string{
		"`n.type`", "`n.kind`", "`n.line`", "`n.complexity`",
		"`n.body`", "`n.is_test`", "`n.params`", "`n.is_public`",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not warn about the invented property %s", want)
		}
	}

	// Recovery: the exception is a schema lookup, not a cue to try another name.
	if !strings.Contains(content, "Binder exception: Cannot find property") {
		t.Error("skill does not show the error the agent will actually see")
	}
	if !strings.Contains(content, "Do not guess a second name") {
		t.Error("skill does not forbid guessing again after a binder failure")
	}
	// The skill used to state the binding rule backwards — that an unlabeled match may
	// only touch what ALL labels share. It binds when ANY candidate label has it, so
	// `MATCH (n) RETURN n.line_number` is fine on a complete graph, and the guidance sent
	// agents rewriting correct queries.
	if strings.Contains(content, "you may ONLY access properties shared by ALL labels") {
		t.Error("skill still claims an unlabeled match is limited to properties every label shares")
	}
	if !strings.Contains(content, "binds when ANY candidate label has it") {
		t.Error("skill does not state the actual binding rule")
	}
	if !strings.Contains(content, "does not fix a binding error") {
		t.Error("skill does not warn that WHERE label(n) IN [...] comes too late to fix binding")
	}

	// The cause that makes a correct query look wrong: mid-rebuild, the property is on no
	// table at all. It reads exactly like a misspelled property.
	for _, want := range []string{"mid-rebuild", "a rebuild, not a typo"} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not cover the partial-schema cause: missing %q", want)
		}
	}
}

// Guarding the skill against itself: every runnable query it ships must use real
// property names. A copy-pasteable example with an invented property teaches the
// exact mistake this rule exists to prevent.
func TestASTRuleContentRunnableQueriesUseRealProperties(t *testing.T) {
	t.Parallel()

	// The union of node and relationship properties, from graphit_ast_schema.
	valid := map[string]bool{}
	for _, p := range []string{
		// nodes
		"uid", "name", "path", "line_number", "end_line", "docstring", "lang",
		"cyclomatic_complexity", "context", "context_type", "class_context",
		"is_dependency", "is_exported", "value", "is_stub",
		"cluster", "relative_path", "full_import_name",
		// relationships
		"source_file", "full_call_name", "receiver_type", "alias", "imported_name",
	} {
		valid[p] = true
	}

	literal := regexp.MustCompile(`'[^']*'`)
	access := regexp.MustCompile(`\b([A-Za-z_][A-Za-z0-9_]*)\.([A-Za-z_][A-Za-z0-9_]*)`)

	for _, query := range runnableQueries(ASTRuleContent()) {
		// String literals hold paths like 'helpers.go' — not property access.
		for _, m := range access.FindAllStringSubmatch(literal.ReplaceAllString(query, "''"), -1) {
			if !valid[m[2]] {
				t.Errorf("runnable example accesses non-existent property %q:\n  %s", m[0], query)
			}
		}
	}
}
