package knowledge

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The skill contained both instructions at once: a four-step workflow ordering a
// sync after writing docs, and a later section explaining that the daemon makes
// that unnecessary. An agent reading top to bottom obeys the first one.
func TestKnowledgeRuleContentWorkflowHasNoSyncStep(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	if strings.Contains(content, "3. **Sync the wiki**") {
		t.Error("the completion workflow still orders a sync, contradicting the section that says reindexing is automatic")
	}
	if !strings.Contains(content, "There is no reindex step") {
		t.Error("the completion workflow does not say the reindex step is gone")
	}
}

// hub_list filters by type only — it has no name filter and no project_dir. The
// integration protocol told the agent to filter it "by name/type" and to pass a
// project_dir, so the one mandatory pre-flight step in this skill was a call that
// cannot do what it was asked to do.
func TestKnowledgeRuleContentUsesHubSearchForIntegrations(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	if strings.Contains(content, brand.MCPToolName("hub", "list")+`(project_dir`) {
		t.Error("skill still passes project_dir to hub_list, which has no such parameter")
	}
	if strings.Contains(content, "filtering by name/type") {
		t.Error("skill still claims hub_list can filter by name")
	}
	if !strings.Contains(content, brand.MCPToolName("hub", "search")+`(query:`) {
		t.Error("skill does not show hub_search as the pre-flight call")
	}
}

// Leaving the wiki should hand the agent to the AST graph, not to grep: the two
// indexes cover different things and text search is below both.
func TestKnowledgeRuleContentRoutesCodeQuestionsToTheGraph(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	if strings.Contains(content, "| Reading/editing actual source code (.go, .ts, etc.) | Normal file tools |") {
		t.Error("skill still sends source-code reading to plain file tools")
	}
	if !strings.Contains(content, brand.MCPToolName("ast", "source")) {
		t.Error("skill never points at ast_source for reading code")
	}
}
