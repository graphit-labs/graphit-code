package ast

import (
	"strings"
	"testing"
)

func TestASTSkillCompactContract(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()
	for _, want := range []string{
		"Graphit AST precedes native search", "Before the first Cypher", "pair one exact", "Before editing an entity",
		"required Graphit tool is unavailable", "Native tools cannot read an imported context",
		"graphit_ast_search", "graphit_ast_query", "graphit_ast_schema", "graphit_ast_source", "graphit_ast_list",
		"graphit_ast_index", "graphit_ast_embed", "graphit_ast_export", "graphit_ast_install", "graphit_ast_remove",
		"graphit_cluster_projects", "graphit_daemon_status", "graphit_sync",
	} {
		if !strings.Contains(content, want) {
			t.Fatalf("compact AST skill missing %q", want)
		}
	}
	if len(content) > 6000 {
		t.Fatalf("AST skill exceeded its token budget: %d bytes", len(content))
	}
}

func TestASTMandateContainsOnlyRouting(t *testing.T) {
	t.Parallel()
	content := MandateTrigger()
	for _, want := range []string{"locating or understanding code", "editing an entity", "another repository", "graphit_ast_search"} {
		if !strings.Contains(content, want) {
			t.Fatalf("AST mandate missing %q", want)
		}
	}
	if len(content) > 1800 {
		t.Fatalf("AST mandate too large: %d bytes", len(content))
	}
}
