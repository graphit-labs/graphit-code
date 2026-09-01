package ide

import (
	"regexp"
	"strings"
	"testing"
)

// Every module block below the preamble pushes the agent towards reading a skill, and
// nothing used to push back — so agents opened all of them on turn one and spent the
// session's context before the first tool call. The preamble has to carry the opposite
// obligation explicitly: one skill, at the moment its domain is entered.
func TestMandatePreambleRequiresSkillsToBeReadJustInTime(t *testing.T) {
	t.Parallel()
	preamble := mandatePreamble()

	if !strings.Contains(preamble, "ONE SKILL, AT THE MOMENT YOU NEED IT") {
		t.Fatal("mandate preamble does not state the lazy skill-loading rule: agents will keep reading every skill at session start")
	}

	for _, want := range []string{
		"ABOUT TO ACT in its domain, and not before",
		"Do not read these skills at the start of a session",
		"No trigger fired means the skill stays closed",
		"A skill you already read this session stays read",
		"A need you can imagine is not a trigger",
	} {
		if !strings.Contains(preamble, want) {
			t.Errorf("mandate preamble is missing a corollary of the lazy-loading rule: %q", want)
		}
	}
}

// "If you are unsure whether one applies, it applies" is load-bearing — it stops an
// agent from reclassifying a structural question as a quick grep — but unscoped it also
// reads as a licence to preload everything, since nobody can be sure a domain will not
// come up later. It has to be bound to the action currently in hand.
func TestMandatePreambleScopesTheUnsureClauseToTheActionInHand(t *testing.T) {
	t.Parallel()
	preamble := mandatePreamble()

	for _, want := range []string{
		"THE ACTION YOU ARE ABOUT TO TAKE",
		"not a reason to open a skill for work you have not started",
	} {
		if !strings.Contains(preamble, want) {
			t.Errorf("the unsure-clause is not scoped to the current action: missing %q", want)
		}
	}
}

// The lazy-loading prose is inside the mandate block, which parseTriggers scans for
// <word> tags. A pseudo-tag anywhere in it would be reassembled as a phantom trigger.
func TestLazySkillSectionCarriesNoPseudoTag(t *testing.T) {
	t.Parallel()

	section := mandatePreamble()
	start := strings.Index(section, "## ONE SKILL")
	if start < 0 {
		t.Fatal("the lazy-loading section is not in the preamble")
	}
	end := strings.Index(section[start:], "## MCP-FIRST")
	if end < 0 {
		t.Fatal("the lazy-loading section is not followed by the MCP-FIRST section")
	}

	if m := regexp.MustCompile(`<(\w+)>`).FindStringSubmatch(section[start : start+end]); m != nil {
		t.Errorf("the lazy-loading section contains the pseudo-tag %q, which parseTriggers would read as a trigger block", m[0])
	}
}
