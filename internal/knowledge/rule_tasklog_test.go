package knowledge

import (
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// The skill used to describe the task log as something written around the change,
// which an agent reads as "write it when the change is finished". A log written at
// the end is a report: if the session is cut short mid-task, it never existed, and
// nothing tells the next agent where the work stopped.
func TestKnowledgeRuleContentOrdersTheTaskLogBeforeTheWork(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	for _, want := range []string{
		"The task log OPENS the task",
		"before you edit a single file",
		"## Plan & Task Breakdown",
		"| **Before starting any task**",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not order the task log before the work: missing %q", want)
		}
	}

	if strings.Contains(content, "\nstatus: done\n") {
		t.Error("a task log template still starts at status: done, so it cannot be created before the change")
	}
}

// A log that is only accurate at the end cannot hand work over mid-task, which is
// precisely when a handover happens. The update trigger has to be the step, the
// course change and the blocker — not the completion of the task.
func TestKnowledgeRuleContentDemandsContinuousUpdates(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	for _, want := range []string{
		"The trigger to update is not the end of the task",
		"could another agent open this file and continue",
		"append-only",
		"| Changing course",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not require the log to be kept current as the work proceeds: missing %q", want)
		}
	}
}

// Resuming after a correction is the moment the framework's tool priority is most
// often abandoned, because urgency makes the native tool the one that comes to
// hand. The skill has to spell out the resume order: read the log, re-open the
// skills, record the change of course, then continue.
func TestKnowledgeRuleContentTeachesTheResumeProtocol(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	for _, want := range []string{
		"Resuming after an interruption, a correction, or a change of course",
		"Read the existing task log first",
		"A correction does not suspend this",
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not teach how to resume interrupted work: missing %q", want)
		}
	}
}

// "Reindexing is automatic" and "call sync when you need certainty" only coexist
// if the skill states the criterion: the trigger is a decision made on the result,
// not an edit.
func TestKnowledgeRuleContentSaysTheRebuildLandsAfterTheWrite(t *testing.T) {
	t.Parallel()
	content := KnowledgeRuleContent(nil, "docs")

	for _, want := range []string{
		"The rebuild lands AFTER the write",
		"decide something on the basis of what a tool returns",
		brand.MCPToolRef("knowledge", "sync"),
	} {
		if !strings.Contains(content, want) {
			t.Errorf("skill does not explain the indexing lag or how to get certainty: missing %q", want)
		}
	}
}

// The mandate block is read before any skill, so the three instructions have to
// fire from there too — otherwise the agent starts coding and only learns about
// the task log once it opens the skill for another reason.
func TestMandateTriggerCarriesTheTaskLogFirstOrder(t *testing.T) {
	t.Parallel()
	trigger := MandateTrigger()

	for _, want := range []string{
		"the task log is the FIRST artifact of it",
		"you finished a step, changed direction",
		"you are resuming after an interruption",
		"the automatic reindex lands after the write",
		"The task log OPENS the task",
	} {
		if !strings.Contains(trigger, want) {
			t.Errorf("doc_rule mandate trigger is missing %q", want)
		}
	}

	if !strings.Contains(trigger, brand.MCPToolRef("sync")) {
		t.Errorf("doc_rule mandate trigger never names %s", brand.MCPToolName("sync"))
	}
}
