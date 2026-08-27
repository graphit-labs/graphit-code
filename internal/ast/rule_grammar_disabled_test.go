package ast

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// An absent language is indistinguishable, from the agent's side, from a stale
// index — and the reflex it triggers is the expensive one: abandon the graph and
// grep. The skill has to name the configuration that can cause it, because the
// agent cannot deduce a key it has never been told about.
func TestASTRuleContentNamesTheGrammarDisableKeys(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	for _, needed := range []string{
		"ast.grammars_blacklist",
		"ast.grammars_whitelist",
		brand.MCPToolRef("config", "get"),
	} {
		if !strings.Contains(content, needed) {
			t.Errorf("skill does not mention %q", needed)
		}
	}
	if !strings.Contains(content, "deliberately inert") {
		t.Error("skill does not warn that an unmatched name disables nothing silently")
	}
}
