package ide

import (
	"regexp"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// An interruption or a correction is where the mandate was silently dropped: the
// agent read the skills on turn one, the user asked for a fix on turn six, and the
// resumed work went back to grep. The preamble now says a resume re-applies
// everything, so the rule is stated where every module inherits it.
func TestMandatePreambleReAppliesAfterAnInterruption(t *testing.T) {
	t.Parallel()
	preamble := mandatePreamble()

	for _, want := range []string{
		"AN INTERRUPTION IS NOT AN EXEMPTION",
		"it re-applies all of it",
		"ahead of your native ones exactly as on the first turn",
	} {
		if !strings.Contains(preamble, want) {
			t.Errorf("mandate preamble does not tell the agent to re-apply the mandate on resume: missing %q", want)
		}
	}

	lower := strings.ToLower(preamble)
	for _, want := range []string{"corrected", "redirected"} {
		if !strings.Contains(lower, want) {
			t.Errorf("mandate preamble never names the situation that triggers a resume: missing %q", want)
		}
	}
}

// Reindexing is automatic but lands after the write, so a tool called inside that
// window answers from the previous state with no warning. The preamble has to say
// both halves: that the lag exists, and that sync is what removes the doubt.
func TestMandatePreambleSaysIndexingLagsAndNamesSync(t *testing.T) {
	t.Parallel()
	preamble := mandatePreamble()

	if !strings.Contains(preamble, "AUTOMATIC INDEXING LAGS THE CHANGE") {
		t.Error("mandate preamble does not warn that the automatic reindex lags the change")
	}
	if !strings.Contains(preamble, brand.MCPToolRef("sync")) {
		t.Errorf("mandate preamble never names %s as the way to reach certainty", brand.MCPToolName("sync"))
	}
	for _, want := range []string{"knowledge index", "memory index", "AST index"} {
		if !strings.Contains(preamble, want) {
			t.Errorf("mandate preamble does not say which indexes sync brings into step: missing %q", want)
		}
	}
	if !strings.Contains(preamble, "NOT mean is a sync after every edit") {
		t.Error("mandate preamble does not rule out a sync after every edit, contradicting the module skills")
	}
}

// The mandate's inner content is parsed by parseTriggers, which treats any <word>
// as a module trigger tag. Prose containing a pseudo-tag would be reassembled as a
// phantom trigger, so the preamble must never carry one.
func TestMandatePreambleHasNoPseudoTags(t *testing.T) {
	t.Parallel()

	if m := regexp.MustCompile(`<(\w+)>`).FindStringSubmatch(mandatePreamble()); m != nil {
		t.Errorf("mandate preamble contains the pseudo-tag %q, which parseTriggers would read as a trigger block", m[0])
	}
}
