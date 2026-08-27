package ast

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// Every callable exists twice in the graph: the declaration, reached by CONTAINS
// from its File, and a call-target stub keyed by bare name, which is what CALLS
// points at. So `NOT ()-[:CALLS]->(f)` is true for EVERY declaration — the skill
// used it in three places to find dead code, and it reports live code as dead.
// Verified against this repository: Apply has 13 callers and the old form listed
// it as uncalled.
func TestASTRuleContentHasNoAlwaysTrueDeadCodeQuery(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	// The broken forms may still appear in prose — they are named there as the
	// thing not to write. What must not survive is a runnable example: a line the
	// agent can lift straight into the query tool.
	for _, line := range runnableQueries(content) {
		if strings.Contains(line, "NOT ()-[:CALLS]->(") {
			t.Errorf("runnable example uses NOT ()-[:CALLS]->(f), true for every declaration:\n  %s", line)
		}
		if strings.Contains(line, "OPTIONAL MATCH (caller)-[:CALLS]->(f)") {
			t.Errorf("runnable example counts callers on a declaration, which always yields zero:\n  %s", line)
		}
	}
	// The working form collects the called names and excludes declarations by name.
	if !strings.Contains(content, "WITH collect(DISTINCT s.name) AS called") {
		t.Error("skill does not show the name-based form that actually finds uncalled functions")
	}
}

// A call is joined to the declaration it names, so CALLS normally lands on the same
// node CONTAINS reaches and the two compose. A stub survives only where there is
// nothing here to join to — a builtin, a dependency, or a name with homonyms — and an
// agent that does not know which case it is looking at either distrusts a traversal
// that works or trusts a location that is empty.
func TestASTRuleContentExplainsTheStubDuality(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	for _, want := range []string{
		"points at the real declaration",
		"is_stub",
		"resolves to **nothing**",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not explain when a call target is the declaration and when it is a stub: missing %q", want)
		}
	}

	// The superseded model is worse than silence: it tells the agent to replace a
	// traversal that now works with a name-join, and to treat every declaration as
	// uncalled.
	for _, gone := range []string{"exists TWICE", "never points at a declaration"} {
		if strings.Contains(content, gone) {
			t.Errorf("skill still asserts the superseded two-node model: %q", gone)
		}
	}
}

// The skill recommended hybrid search as the "RECOMMENDED default", and an agent
// reading that stops there and answers from a ranked text guess. Search finds
// names; it never traversed an edge, so it cannot answer anything about
// relationships, impact or complexity.
func TestASTRuleContentFramesSearchAsGroundingNotAnswer(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	if strings.Contains(content, "RECOMMENDED default for text-based discovery") {
		t.Error("skill still presents search as the recommended default, which is why queries go unused")
	}
	if !strings.Contains(content, "never the last") {
		t.Error("skill does not say a search result is not the answer")
	}
	// The database and its language have to be named for the agent to reach for them.
	for _, want := range []string{"LadybugDB", "Cypher"} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not name %q where it introduces the graph", want)
		}
	}
}

// The five families the graph answers and text search cannot. Each is here
// because it is a question an agent routinely answers by guessing instead.
func TestASTRuleContentCoversTheQueryOnlyQuestions(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	for _, want := range []string{
		"Relationships between entities",
		"Find usage",
		"Refactoring",
		"Complexity and risk",
		"Understanding a system you have never read",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill is missing the %q family of queries", want)
		}
	}

	// Aggregation is what turns a survey into one query.
	if !strings.Contains(content, "cyclomatic_complexity") {
		t.Error("skill does not show the precomputed risk property")
	}
	if !strings.Contains(content, brand.MCPToolName("ast", "query")) {
		t.Error("skill does not name the query tool")
	}
}

// runnableQueries returns the lines an agent could lift straight into the query
// tool: a bare Cypher statement on its own line. Prose that merely quotes a query
// — to name it as an anti-pattern, for instance — is not one of them, which is the
// distinction the dead-code check depends on.
func runnableQueries(content string) []string {
	var out []string
	for _, line := range strings.Split(content, "\n") {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "MATCH ") {
			out = append(out, trimmed)
		}
	}
	return out
}
