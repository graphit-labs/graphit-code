package memory

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// memory_search reads the compiled wiki, which is rebuilt after the write. The
// skill already said a fresh memory "may not surface yet"; what it did not say is
// what to do when the answer has to be certain — list instead of search, and call
// the index or the sync rather than assuming the rebuild already happened.
func TestMemoryRuleContentSaysTheRecompileLandsAfterTheWrite(t *testing.T) {
	t.Parallel()
	content := RuleContent(nil)

	for _, want := range []string{
		"automatic, but not instant",
		"means eventually, not immediately",
		"reads the store, not the wiki",
		brand.MCPToolRef("sync"),
	} {
		if !strings.Contains(content, want) {
			t.Errorf("memory skill does not explain the recompile lag or how to get certainty: missing %q", want)
		}
	}
}
