package ast

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The daemon reindexes the source tree incrementally on file change, so the block
// demanding a sync after every edit ordered the agent to redo work already done —
// and to wait on a full rebuild of AST, wiki, memory and Hub for an incremental
// result it was getting anyway.
func TestASTRuleContentDoesNotDemandSyncAfterEveryEdit(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	for _, gone := range []string{
		"MANDATORY: Sync After Every File Modification",
		"Forgetting to call sync is a framework integrity violation",
	} {
		if strings.Contains(content, gone) {
			t.Errorf("obsolete sync mandate still present: %q", gone)
		}
	}
	if !strings.Contains(content, "Reindexing is automatic") {
		t.Error("skill does not explain that the daemon reindexes on its own")
	}
}

// Missing vectors are a job for ast_embed. Sending the agent to the global sync
// rebuilds the AST graph, both wikis and the Hub to fix one set of embeddings.
func TestASTRuleContentSendsMissingEmbeddingsToEmbed(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	if !strings.Contains(content, "If semantic results are empty, call "+brand.MCPToolRef("ast", "embed")) {
		t.Error("skill does not route empty semantic results to ast_embed")
	}
}

// Comments became first-class entities, so the concession sending the agent to
// grep for them is obsolete — and it conceded the one thing grep looked good at.
func TestASTRuleContentTreatsCommentsAsQueryable(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	if strings.Contains(content, "Searching inside string literals or comments | grep/ripgrep") {
		t.Error("skill still sends comment searches to grep")
	}
	for _, want := range []string{
		"MATCH (c:Comment)",
		"`name` is the comment text itself",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not teach comments as graph entities: missing %q", want)
		}
	}
}

// "You already know the path, just read the file" was accepted without the
// harness having been tried. ast_source reads the indexed copy, slices by entity
// or line range in one call, and is the only option for an imported context whose
// files are not in this checkout. The native read stays correct for a file that is
// not in the graph — stated as a named exception, with the reason.
func TestASTRuleContentPrefersSourceToolOverNativeRead(t *testing.T) {
	t.Parallel()
	content := ASTRuleContent()

	if strings.Contains(content, "faster and simpler") {
		t.Error("skill still recommends the native file read as faster and simpler")
	}
	if !strings.Contains(content, "not in the graph") {
		t.Error("skill does not name the one case where reading from disk is correct")
	}
}
