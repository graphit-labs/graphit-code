package memory

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The skill used to document memory_gc backwards: it presented the bare call as a
// dry run and `dry_run: false` as the destructive one. The tool's DryRun field is
// a bool that defaults to false, so the bare call is what deletes — an agent
// following the old text destroyed memories while believing it was scanning.
func TestMemoryRuleContentDoesNotInvertTheGCDryRun(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	gc := brand.MCPToolName("memory", "gc")

	if strings.Contains(content, gc+"(project_dir: \"/path/to/project\", dry_run: false)") {
		t.Error("skill still presents dry_run:false as a distinct destructive call; the bare call already deletes")
	}
	if !strings.Contains(content, gc+"(project_dir: \"/path/to/project\", dry_run: true)") {
		t.Error("skill does not show the dry_run:true form, which is the only non-destructive one")
	}
	if !strings.Contains(content, "deletes by default") {
		t.Error("skill does not warn that the bare gc call deletes")
	}
}

// Two statements in the same skill disagreed about what memory_search reads: one
// said raw .md files, another said a pre-compiled index. It searches the compiled
// wiki through FTS5 — which is also why a memory written seconds ago may not
// surface yet, a symptom the agent needs the right model to explain.
func TestMemoryRuleContentDescribesSearchAccurately(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	if strings.Contains(content, "searches raw `.md` memory files") {
		t.Error("skill still claims memory_search scans raw .md files")
	}
	if !strings.Contains(content, "FTS5") {
		t.Error("skill does not say memory_search runs over the compiled wiki via FTS5")
	}
}

// The retrieval steps named the browse tool as a literal string instead of
// building it from the brand, so a rebranded build rendered a tool the agent does
// not have. It also omitted `wiki: "memory"`, which defaults to the project wiki —
// browsing the wrong one from inside the memory skill.
func TestMemoryRuleContentBuildsTheBrowseToolFromTheBrand(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	if !strings.Contains(content, brand.MCPToolRef("wiki", "browse")+" with `wiki: \"memory\"`") {
		t.Error("retrieval steps do not point at the memory scope of the browse tool")
	}
}
