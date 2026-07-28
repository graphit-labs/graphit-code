package improvements

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The improvements mandate has always fired on "you noticed something worth
// fixing that is outside the current change", and the skill it pointed at
// offered no way to act on it — so the finding either bloated the diff or was
// dropped. dream_subject_add is the third exit, and it belongs in this skill
// because this is the skill the agent is already reading when it finds one.
func TestImprovementsRuleContentTeachesDreamSubjects(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	for _, action := range []string{
		"subject_add", "subject_list", "subject_remove", "status", "reports",
	} {
		if !strings.Contains(content, brand.MCPToolName("dream", action)) {
			t.Errorf("improvements skill never mentions %s", brand.MCPToolName("dream", action))
		}
	}
}

// Two facts that make the difference between a subject that works and one that
// silently never runs: the dream agent inherits no conversation, and the module
// is opt-in, so a queued subject may have nothing to pick it up.
func TestImprovementsRuleContentWarnsAboutDreamPreconditions(t *testing.T) {
	t.Parallel()
	content := ImprovementsRuleContent()

	if !strings.Contains(content, "does not\ninherit this session") {
		t.Error("improvements skill does not warn that the dream agent has no conversation history")
	}
	if !strings.Contains(content, "opt-in") {
		t.Error("improvements skill does not warn that dream is opt-in and may never run")
	}
	// dream_reports advances the last-seen marker, so a second call looks empty.
	if !strings.Contains(content, "Reading marks as read") {
		t.Error("improvements skill does not warn that dream_reports marks reports as seen")
	}
}
