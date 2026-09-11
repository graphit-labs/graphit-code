package dream

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/memory"
)

func TestReportFingerprintDistinguishesWrittenReports(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	path := filepath.Join(dir, "session.md")

	if fp := reportFingerprint(path); fp != "" {
		t.Errorf("a missing report must fingerprint as empty, got %q", fp)
	}

	if err := os.WriteFile(path, []byte("# Report\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	before := reportFingerprint(path)
	if before == "" {
		t.Fatal("an existing report must have a fingerprint")
	}

	if err := os.WriteFile(path, []byte("# Report\n\nThe agent wrote considerably more.\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if after := reportFingerprint(path); after == before {
		t.Error("a rewritten report must fingerprint differently, or the runner cannot tell the agent wrote it")
	}
}

// The consolidation audit is the only record of the deterministic half of the
// session. It has to reach the report whichever way the report got written.
func TestConsolidationAuditIsAppendedToAnAgentWrittenReport(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	path := filepath.Join(dir, "session.md")

	agentReport := "# Dream Report\n\nWhat the agent found.\n"
	if err := os.WriteFile(path, []byte(agentReport), 0o644); err != nil {
		t.Fatal(err)
	}

	outcomes := []*memory.ConsolidationOutcome{{
		Scope:    "project",
		Analysed: 7,
		Applied: []memory.AppliedAction{{
			Type: memory.ActionMerge, Kept: "01AAAAAAAAAAAAAAAAAAAAAAAA",
			Removed: []string{"01BBBBBBBBBBBBBBBBBBBBBBBB"}, Reason: "duplicates",
		}},
	}}

	if err := appendConsolidationAudit(path, outcomes); err != nil {
		t.Fatalf("append: %v", err)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	got := string(data)

	if !strings.Contains(got, "What the agent found.") {
		t.Error("the agent's report must survive")
	}
	if !strings.Contains(got, "Memory Consolidation (applied by the runner)") {
		t.Error("the audit section is missing")
	}
	if !strings.Contains(got, "01AAAAAAAAAAAAAAAAAAAAAAAA") {
		t.Error("the audit does not name the surviving memory")
	}
}

// With no outcomes there is nothing to append, and the report must not grow an
// empty section that reads as "consolidation ran and did nothing".
func TestConsolidationAuditIsEmptyWithoutOutcomes(t *testing.T) {
	t.Parallel()
	if audit := consolidationAudit(nil); audit != "" {
		t.Errorf("expected no audit section, got %q", audit)
	}
}

// A failed agent must not discard the consolidation that already happened: those
// changes are real and the developer has no other record of them.
func TestConsolidationSurvivesAFailedAgent(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	r := &Runner{projectDir: dir}
	path := filepath.Join(dir, "session.md")

	outcomes := []*memory.ConsolidationOutcome{{
		Scope:    "project",
		Analysed: 3,
		Skipped: []memory.AppliedAction{{
			Type: memory.ActionDelete, Kept: "01AAAAAAAAAAAAAAAAAAAAAAAA",
			Skipped: "memory is marked important",
		}},
	}}

	if err := r.writeConsolidationOnlyReport(path, "session", outcomes, os.ErrDeadlineExceeded); err != nil {
		t.Fatalf("write: %v", err)
	}

	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	got := string(data)

	if !strings.Contains(got, "status: agent-failed") {
		t.Error("the report must say the agent failed")
	}
	if !strings.Contains(got, "marked important") {
		t.Error("the refusal must still be reported")
	}
	if !strings.Contains(got, "had already been applied") {
		t.Error("the report should make clear the consolidation is unaffected")
	}
}

// The briefing exists so the agent does not re-derive and re-report work the runner
// already did — and so the refusals, which need judgement, reach it.
func TestConsolidationBriefingCarriesRefusalsToTheAgent(t *testing.T) {
	t.Parallel()

	outcomes := []*memory.ConsolidationOutcome{{
		Scope:    "project",
		Analysed: 9,
		Applied:  []memory.AppliedAction{{Type: memory.ActionMerge, Kept: "01AAAAAAAAAAAAAAAAAAAAAAAA"}},
		Skipped: []memory.AppliedAction{{
			Type: memory.ActionDelete, Kept: "01BBBBBBBBBBBBBBBBBBBBBBBB",
			Skipped: "memory is marked important",
		}},
	}}

	briefing := buildConsolidationBriefing(outcomes)

	if !strings.Contains(briefing, "do not redo it") {
		t.Error("the briefing must tell the agent the work is already done")
	}
	if !strings.Contains(briefing, "01BBBBBBBBBBBBBBBBBBBBBBBB") {
		t.Error("the refused action must be named, it is the work left for the agent")
	}
	if !strings.Contains(briefing, "must not delete memories") {
		t.Error("the briefing must keep deletion in the runner's hands")
	}
}

// Without a consolidation the agent must be told the store is unsanitised, rather
// than being left to assume either way.
func TestConsolidationBriefingWhenNothingRan(t *testing.T) {
	t.Parallel()
	briefing := buildConsolidationBriefing(nil)

	if !strings.Contains(briefing, "No consolidation ran") {
		t.Error("the briefing must say no consolidation ran")
	}
	if !strings.Contains(briefing, "rather than deleting anything") {
		t.Error("the briefing must still forbid the agent deleting memories")
	}
}
