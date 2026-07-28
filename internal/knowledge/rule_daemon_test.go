package knowledge

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The staleness exception table opens with "the daemon is not running", which is
// a question the skill gave the agent no way to answer. daemon_status is that
// answer, and it lives here because this is where the question gets asked.
func TestKnowledgeRuleContentTeachesDaemonStatus(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	for _, action := range []string{"status", "stop"} {
		if !strings.Contains(content, brand.MCPToolName("daemon", action)) {
			t.Errorf("knowledge skill never mentions %s", brand.MCPToolName("daemon", action))
		}
	}
}

// A read that lands while the daemon holds the write lock fails with a message
// naming the database, which reads like "there is no index here". An agent that
// believes it falls back to grep — abandoning the graph precisely because it was
// busy building itself.
func TestKnowledgeRuleContentExplainsTheTransientLock(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	if !strings.Contains(content, "failed to open database with status 1") {
		t.Error("knowledge skill does not name the lock error an agent will actually see")
	}
	if !strings.Contains(content, "lock, not an absence") {
		t.Error("knowledge skill does not say the lock error is not a missing index")
	}
}
