package improvements

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
)

// The improvements mandate has always fired on "you noticed something worth
// fixing that is outside the current change", and the skill it pointed at
// offered no way to act on it — so the finding either bloated the diff or was
// dropped. The improvement backlog is the third exit, and it belongs in this
// skill because this is the skill the agent is already reading when it finds one.
func TestImprovementsRuleContentTeachesTheBacklog(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	for _, action := range []string{"backlog_add", "backlog_list", "backlog_remove"} {
		if !strings.Contains(content, brand.MCPToolName("improvements", action)) {
			t.Errorf("improvements skill never mentions %s", brand.MCPToolName("improvements", action))
		}
	}

	// Reading what an autonomous session did is still the dream module's job.
	for _, action := range []string{"status", "reports"} {
		if !strings.Contains(content, brand.MCPToolName("dream", action)) {
			t.Errorf("improvements skill never mentions %s", brand.MCPToolName("dream", action))
		}
	}
}

// The skill must not send agents to the tool names this refactor retired, or
// every call it teaches fails with "unknown tool".
func TestImprovementsRuleContentDropsTheOldSubjectTools(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	for _, action := range []string{"subject_add", "subject_list", "subject_remove"} {
		if strings.Contains(content, brand.MCPToolName("dream", action)) {
			t.Errorf("improvements skill still mentions the retired tool %s", brand.MCPToolName("dream", action))
		}
	}
}

// An agent that does not know where the backlog lives cannot tell the user, and
// cannot find an item a previous session left behind.
func TestImprovementsRuleContentNamesTheBacklogLocation(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	if !strings.Contains(content, config.DefaultBacklogDir(nil, nil)) {
		t.Errorf("improvements skill never names the default backlog dir %q", config.DefaultBacklogDir(nil, nil))
	}
	if !strings.Contains(content, "improvements.backlog_dir") {
		t.Error("improvements skill never names the improvements.backlog_dir override")
	}
}

// Two facts that make the difference between an item that gets worked and one
// that silently never runs: whoever picks it up inherits no conversation, and
// the dream module is opt-in, so a queued item may have nothing to action it.
func TestImprovementsRuleContentWarnsAboutPreconditions(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	if !strings.Contains(content, "does\nnot inherit this session") {
		t.Error("improvements skill does not warn that the next agent has no conversation history")
	}
	if !strings.Contains(content, "opt-in") {
		t.Error("improvements skill does not warn that dream is opt-in and may never run")
	}
	// dream_reports advances the last-seen marker, so a second call looks empty.
	if !strings.Contains(content, "Reading marks as read") {
		t.Error("improvements skill does not warn that dream_reports marks reports as seen")
	}
}
