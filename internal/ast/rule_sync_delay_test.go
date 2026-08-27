package ast

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// "Reindexing is automatic" reads as "the graph is always current", which is false
// for the window between the write and the debounced rebuild. Inside it a query
// answers from the previous state with the confidence of a current answer, so the
// skill has to name the lag and the criterion for calling sync mid-session.
func TestASTRuleContentExplainsTheIndexingLag(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	for _, want := range []string{
		"Mid-session, when you need CERTAINTY",
		"automatic but not instantaneous",
		"I am about to decide something on the basis of what this returns",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("AST skill does not explain the indexing lag: missing %q", want)
		}
	}

	if !strings.Contains(content, brand.MCPToolRef("ast", "index")+" with a `path`") {
		t.Errorf("AST skill does not offer %s as the narrower call when only the graph is in doubt", brand.MCPToolName("ast", "index"))
	}
}
