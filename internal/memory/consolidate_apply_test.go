package memory

import (
	"context"
	"os"
	"sort"
	"strings"
	"testing"
)

type fakeWriter struct {
	stored    map[string]string
	updates   []string
	removals  []string
	promotes  []string
	demotes   []string
	mandatory []string
	failOn    map[string]error
}

func newFakeWriter() *fakeWriter {
	return &fakeWriter{stored: map[string]string{}, failOn: map[string]error{}}
}

func (f *fakeWriter) LiveMemories(_ context.Context) ([]MemoryRecord, error) {
	ids := make([]string, 0, len(f.stored))
	for id := range f.stored {
		ids = append(ids, id)
	}
	sort.Strings(ids)

	records := make([]MemoryRecord, 0, len(ids))
	for _, id := range ids {
		rec, ok := recordFromMarkdown(MemoryFileName(id), []byte(f.stored[id]))
		if !ok {
			return nil, os.ErrInvalid
		}
		records = append(records, rec)
	}
	return records, nil
}

func (f *fakeWriter) UpdateMemory(id, newTitle, newBody string) error {
	return f.UpdateMemoryTyped(id, newTitle, newBody, "")
}

func (f *fakeWriter) UpdateMemoryTyped(id, newTitle, newBody, memType string) error {
	if err := f.failOn["update:"+id]; err != nil {
		return err
	}
	current, ok := f.stored[id]
	if !ok {
		return os.ErrNotExist
	}
	f.stored[id] = updatedMemoryContent(current, memoryUpdate{
		ID: id, Scope: "project", ScopeID: "test",
		NewTitle: newTitle, NewBody: newBody, NewType: memType,
	})
	f.updates = append(f.updates, id)
	return nil
}

func (f *fakeWriter) RemoveMemory(id string) error {
	if err := f.failOn["remove:"+id]; err != nil {
		return err
	}
	if _, ok := f.stored[id]; !ok {
		return os.ErrNotExist
	}
	delete(f.stored, id)
	f.removals = append(f.removals, id)
	return nil
}

func (f *fakeWriter) setRelevance(id string, promote bool) error {
	current, ok := f.stored[id]
	if !ok {
		return os.ErrNotExist
	}
	f.stored[id] = withImportantFlag(current, promote)
	return nil
}

func (f *fakeWriter) PromoteMemory(id string) error {
	if err := f.failOn["promote:"+id]; err != nil {
		return err
	}
	if err := f.setRelevance(id, true); err != nil {
		return err
	}
	f.promotes = append(f.promotes, id)
	return nil
}

func (f *fakeWriter) DemoteMemory(id string) error {
	if err := f.failOn["demote:"+id]; err != nil {
		return err
	}
	if err := f.setRelevance(id, false); err != nil {
		return err
	}
	f.demotes = append(f.demotes, id)
	return nil
}

func (f *fakeWriter) MarkMandatory(id string) error {
	if err := f.failOn["mandatory:"+id]; err != nil {
		return err
	}
	current, ok := f.stored[id]
	if !ok {
		return os.ErrNotExist
	}
	f.stored[id] = withMandatoryFlag(current, true)
	f.mandatory = append(f.mandatory, id)
	return nil
}

func writeMemory(f *fakeWriter, id, title, body, memType string, important bool, tags ...string) {
	tagSet := append([]string{"memory", "project"}, tags...)
	if memType != "" {
		tagSet = append(tagSet, memType)
	}
	f.stored[id] = renderMemoryFile(MemoryFrontmatter{
		ID: id, Title: title, Scope: "project", ScopeID: "test",
		Type: memType, Important: important,
		CreatedAt: "2026-01-01T00:00:00Z", Tags: tagSet,
	}, body)
}

func readMemory(t *testing.T, f *fakeWriter, id string) (MemoryFrontmatter, string, bool) {
	t.Helper()
	content, ok := f.stored[id]
	if !ok {
		t.Fatalf("memory %q is not in the store", id)
	}
	fm := ParseMemoryFrontmatter(content)
	return fm, extractBodyAfterFrontmatter(content), fm.Important
}

func exists(f *fakeWriter, id string) bool {
	_, ok := f.stored[id]
	return ok
}

// The merge the model asked for, applied: one survivor carrying the supplied
// content, the other memory gone.
func TestApplyConsolidation_MergeFoldsIntoSurvivor(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "First", "first body", "fact", false)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Second", "second body", "fact", false)

	report := &ConsolidationReport{
		TotalMemories: 2,
		Duplicates: []ConsolidationAction{{
			Type:       ActionMerge,
			MemoryIDs:  []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"},
			KeepID:     "01BBBBBBBBBBBBBBBBBBBBBBBB",
			NewTitle:   "Merged",
			NewContent: "everything from both",
			Reason:     "same knowledge",
		}},
	}

	outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	if len(outcome.Applied) != 1 {
		t.Fatalf("expected 1 applied action, got %d (skipped=%d failed=%d)",
			len(outcome.Applied), len(outcome.Skipped), len(outcome.Failed))
	}
	if exists(w, "01AAAAAAAAAAAAAAAAAAAAAAAA") {
		t.Error("the folded memory should be gone")
	}
	fm, body, _ := readMemory(t, w, "01BBBBBBBBBBBBBBBBBBBBBBBB")
	if fm.Title != "Merged" {
		t.Errorf("title = %q; want Merged", fm.Title)
	}
	if body != "everything from both" {
		t.Errorf("body = %q; want the merged content", body)
	}
}

// The invariant that makes an unattended merge acceptable: if any member mattered
// enough to be important, the memory replacing them all is important. The analysis
// picks the survivor for content reasons and has no reason to preserve this.
func TestApplyConsolidation_MergePreservesImportance(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Ordinary", "a", "fact", false)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Critical", "b", "correction", true)

	report := &ConsolidationReport{
		TotalMemories: 2,
		Duplicates: []ConsolidationAction{{
			Type:       ActionMerge,
			MemoryIDs:  []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"},
			KeepID:     "01AAAAAAAAAAAAAAAAAAAAAAAA",
			NewContent: "merged",
		}},
	}

	if _, err := ApplyConsolidation(context.Background(), "project", report, w); err != nil {
		t.Fatalf("apply: %v", err)
	}

	fm, _, importantOnDisk := readMemory(t, w, "01AAAAAAAAAAAAAAAAAAAAAAAA")
	if !importantOnDisk {
		t.Error("survivor must be stored as important — importance is a property of the group")
	}
	if !fm.Important {
		t.Error("survivor frontmatter must record importance")
	}
	if fm.Type != string(MemoryTypeCorrection) {
		t.Errorf("type = %q; want correction to survive the merge", fm.Type)
	}
}

// Mandatory is independent from importance and is also a group invariant: if any
// folded memory must be loaded at every session, its survivor must keep that contract.
func TestApplyConsolidation_MergePreservesMandatory(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Ordinary", "a", "fact", false)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Always load", "b", "decision", false)
	w.stored["01BBBBBBBBBBBBBBBBBBBBBBBB"] = withMandatoryFlag(w.stored["01BBBBBBBBBBBBBBBBBBBBBBBB"], true)

	report := &ConsolidationReport{
		TotalMemories: 2,
		Duplicates: []ConsolidationAction{{
			Type:       ActionMerge,
			MemoryIDs:  []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"},
			KeepID:     "01AAAAAAAAAAAAAAAAAAAAAAAA",
			NewContent: "merged",
		}},
	}

	if _, err := ApplyConsolidation(context.Background(), "project", report, w); err != nil {
		t.Fatalf("apply: %v", err)
	}
	fm, _, important := readMemory(t, w, "01AAAAAAAAAAAAAAAAAAAAAAAA")
	if !fm.Mandatory {
		t.Error("survivor must retain the mandatory contract")
	}
	if important {
		t.Error("preserving mandatory must not promote importance")
	}
	if exists(w, "01BBBBBBBBBBBBBBBBBBBBBBBB") {
		t.Error("folded mandatory memory should be removed after its status reaches the survivor")
	}
}

// With no merged content supplied, the union is built from the members rather than
// the action being skipped or — as it once did — the model's one-line justification
// being written in place of both bodies.
func TestApplyConsolidation_MergeWithoutSuppliedContentKeepsEverything(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "First", "detail unique to A", "fact", false)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Second", "detail unique to B", "fact", false)

	report := &ConsolidationReport{
		TotalMemories: 2,
		Duplicates: []ConsolidationAction{{
			Type:      ActionMerge,
			MemoryIDs: []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"},
			KeepID:    "01AAAAAAAAAAAAAAAAAAAAAAAA",
			Reason:    "MERGE: they overlap",
		}},
	}

	if _, err := ApplyConsolidation(context.Background(), "project", report, w); err != nil {
		t.Fatalf("apply: %v", err)
	}

	_, body, _ := readMemory(t, w, "01AAAAAAAAAAAAAAAAAAAAAAAA")
	for _, want := range []string{"detail unique to A", "detail unique to B"} {
		if !strings.Contains(body, want) {
			t.Errorf("merged body lost %q:\n%s", want, body)
		}
	}
	if strings.TrimSpace(body) == "MERGE: they overlap" {
		t.Error("the justification must not replace the content")
	}
}

func TestApplyConsolidation_ContradictionKeepsTheRecommendedMemory(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Outdated", "the old truth", "decision", false)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Current", "the new truth", "decision", false)

	report := &ConsolidationReport{
		TotalMemories: 2,
		Contradictions: []ConsolidationAction{{
			Type:       ActionConflict,
			MemoryIDs:  []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"},
			KeepID:     "01BBBBBBBBBBBBBBBBBBBBBBBB",
			NewContent: "the new truth; previously it was the old truth",
			Reason:     "superseded",
		}},
	}

	if _, err := ApplyConsolidation(context.Background(), "project", report, w); err != nil {
		t.Fatalf("apply: %v", err)
	}

	if exists(w, "01AAAAAAAAAAAAAAAAAAAAAAAA") {
		t.Error("the superseded memory should be removed")
	}
	_, body, _ := readMemory(t, w, "01BBBBBBBBBBBBBBBBBBBBBBBB")
	if !strings.Contains(body, "the new truth") {
		t.Errorf("survivor should state the current truth, got %q", body)
	}
}

// A bare delete cannot remove an important memory. Importance was set by a human,
// or by an agent acting for one, and no unattended analysis outranks it.
func TestApplyConsolidation_RefusesToDeleteImportantMemory(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Critical", "must survive", "correction", true)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Other", "filler", "fact", false)

	report := &ConsolidationReport{
		TotalMemories: 2,
		Suggestions: []ConsolidationAction{{
			Type:      ActionDelete,
			MemoryIDs: []string{"01AAAAAAAAAAAAAAAAAAAAAAAA"},
			KeepID:    "01AAAAAAAAAAAAAAAAAAAAAAAA",
			Reason:    "looks obsolete",
		}},
	}

	outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	if !exists(w, "01AAAAAAAAAAAAAAAAAAAAAAAA") {
		t.Fatal("an important memory must not be deleted by a suggestion")
	}
	if len(outcome.Skipped) != 1 {
		t.Fatalf("the refusal must be reported, got %d skipped", len(outcome.Skipped))
	}
	if !strings.Contains(outcome.Skipped[0].Skipped, "important") {
		t.Errorf("refusal reason should explain importance, got %q", outcome.Skipped[0].Skipped)
	}
}

func TestApplyConsolidation_RefusesToDeleteMandatoryMemory(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Always load", "must survive", "decision", false)
	w.stored["01AAAAAAAAAAAAAAAAAAAAAAAA"] = withMandatoryFlag(w.stored["01AAAAAAAAAAAAAAAAAAAAAAAA"], true)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Other", "filler", "fact", false)

	report := &ConsolidationReport{
		TotalMemories: 2,
		Suggestions: []ConsolidationAction{{
			Type:      ActionDelete,
			MemoryIDs: []string{"01AAAAAAAAAAAAAAAAAAAAAAAA"},
			KeepID:    "01AAAAAAAAAAAAAAAAAAAAAAAA",
			Reason:    "looks obsolete",
		}},
	}

	outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	if !exists(w, "01AAAAAAAAAAAAAAAAAAAAAAAA") {
		t.Fatal("a mandatory memory must not be deleted by a suggestion")
	}
	if len(outcome.Skipped) != 1 {
		t.Fatalf("the refusal must be reported, got %d skipped", len(outcome.Skipped))
	}
	if !strings.Contains(outcome.Skipped[0].Skipped, "mandatory") {
		t.Errorf("refusal reason should explain mandatory status, got %q", outcome.Skipped[0].Skipped)
	}
}

// An empty memory store is indistinguishable from a project that never had one, and
// a corpus of one is exactly when the analysis is least reliable.
func TestApplyConsolidation_RefusesToDeleteLastMemory(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Only", "the only one", "fact", false)

	report := &ConsolidationReport{
		TotalMemories: 1,
		Suggestions: []ConsolidationAction{{
			Type:      ActionDelete,
			MemoryIDs: []string{"01AAAAAAAAAAAAAAAAAAAAAAAA"},
			KeepID:    "01AAAAAAAAAAAAAAAAAAAAAAAA",
		}},
	}

	outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	if !exists(w, "01AAAAAAAAAAAAAAAAAAAAAAAA") {
		t.Error("refused to empty the store, but the memory is gone")
	}
	if len(outcome.Skipped) != 1 {
		t.Errorf("expected the refusal to be reported, got %d", len(outcome.Skipped))
	}
}

// A stale flag is a prompt to re-read, not a rewrite. Applying it with no proposed
// content would mean inventing the replacement.
func TestApplyConsolidation_StaleWithoutContentIsReportedNotApplied(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Aging", "original body", "fact", false)

	report := &ConsolidationReport{
		TotalMemories: 1,
		Stale: []ConsolidationAction{{
			Type:      ActionUpdate,
			MemoryIDs: []string{"01AAAAAAAAAAAAAAAAAAAAAAAA"},
			KeepID:    "01AAAAAAAAAAAAAAAAAAAAAAAA",
			Reason:    "Memory is 200 days old",
		}},
	}

	outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	if len(outcome.Applied) != 0 {
		t.Errorf("nothing should be applied, got %d", len(outcome.Applied))
	}
	if len(outcome.Skipped) != 1 {
		t.Fatalf("the flag should surface as a refusal, got %d", len(outcome.Skipped))
	}
	_, body, _ := readMemory(t, w, "01AAAAAAAAAAAAAAAAAAAAAAAA")
	if body != "original body" {
		t.Errorf("body should be untouched, got %q", body)
	}
}

// Actions naming a memory an earlier action already folded away must not resurrect
// it or fail loudly — the apply step re-reads its own effects as it goes.
func TestApplyConsolidation_SkipsActionsOnAlreadyRemovedMemories(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "A", "a", "fact", false)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "B", "b", "fact", false)
	writeMemory(w, "01CCCCCCCCCCCCCCCCCCCCCCCC", "C", "c", "fact", false)

	report := &ConsolidationReport{
		TotalMemories: 3,
		Duplicates: []ConsolidationAction{{
			Type:       ActionMerge,
			MemoryIDs:  []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"},
			KeepID:     "01AAAAAAAAAAAAAAAAAAAAAAAA",
			NewContent: "merged",
		}},
		Suggestions: []ConsolidationAction{{
			Type:      ActionPromote,
			MemoryIDs: []string{"01BBBBBBBBBBBBBBBBBBBBBBBB"},
			KeepID:    "01BBBBBBBBBBBBBBBBBBBBBBBB",
		}},
	}

	outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	if len(outcome.Failed) != 0 {
		t.Errorf("a vanished memory is a skip, not a failure: %+v", outcome.Failed)
	}
	if len(outcome.Skipped) != 1 {
		t.Fatalf("expected 1 skip, got %d", len(outcome.Skipped))
	}
	if !strings.Contains(outcome.Skipped[0].Skipped, "no longer exists") {
		t.Errorf("skip reason = %q", outcome.Skipped[0].Skipped)
	}
}

// The survivor is written before the others are removed. Reversed, a failure
// between the two steps loses the content permanently; in this order the worst case
// is a duplicate that survives to the next cycle.
func TestApplyConsolidation_FailedSurvivorWriteKeepsEveryMemory(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "A", "a", "fact", false)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "B", "b", "fact", false)

	w.failOn["update:01AAAAAAAAAAAAAAAAAAAAAAAA"] = os.ErrPermission

	report := &ConsolidationReport{
		TotalMemories: 2,
		Duplicates: []ConsolidationAction{{
			Type:       ActionMerge,
			MemoryIDs:  []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"},
			KeepID:     "01AAAAAAAAAAAAAAAAAAAAAAAA",
			NewContent: "merged",
		}},
	}

	outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	if len(outcome.Failed) != 1 {
		t.Fatalf("expected the write failure to be reported, got %d", len(outcome.Failed))
	}
	if len(w.removals) != 0 {
		t.Errorf("nothing may be removed when the survivor write failed, removed %v", w.removals)
	}
	for _, id := range []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"} {
		if !exists(w, id) {
			t.Errorf("memory %s was lost", id)
		}
	}
}

func TestApplyConsolidation_FailedRelevancePreservationKeepsEveryMemory(t *testing.T) {
	for _, tc := range []struct {
		name  string
		setup func(*fakeWriter)
	}{
		{
			name: "important",
			setup: func(w *fakeWriter) {
				writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Protected", "b", "fact", true)
				w.failOn["promote:01AAAAAAAAAAAAAAAAAAAAAAAA"] = os.ErrPermission
			},
		},
		{
			name: "mandatory",
			setup: func(w *fakeWriter) {
				writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Protected", "b", "fact", false)
				w.stored["01BBBBBBBBBBBBBBBBBBBBBBBB"] = withMandatoryFlag(w.stored["01BBBBBBBBBBBBBBBBBBBBBBBB"], true)
				w.failOn["mandatory:01AAAAAAAAAAAAAAAAAAAAAAAA"] = os.ErrPermission
			},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			w := newFakeWriter()
			writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Survivor", "a", "fact", false)
			tc.setup(w)
			report := &ConsolidationReport{
				TotalMemories: 2,
				Duplicates: []ConsolidationAction{{
					Type:       ActionMerge,
					MemoryIDs:  []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"},
					KeepID:     "01AAAAAAAAAAAAAAAAAAAAAAAA",
					NewContent: "merged",
				}},
			}

			outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
			if err != nil {
				t.Fatalf("apply: %v", err)
			}
			if len(outcome.Failed) != 1 {
				t.Fatalf("expected relevance failure, got %+v", outcome.Failed)
			}
			if len(w.removals) != 0 {
				t.Fatalf("relevance failure removed memories: %v", w.removals)
			}
			for _, id := range []string{"01AAAAAAAAAAAAAAAAAAAAAAAA", "01BBBBBBBBBBBBBBBBBBBBBBBB"} {
				if !exists(w, id) {
					t.Errorf("memory %s was lost", id)
				}
			}
		})
	}
}

func TestApplyConsolidation_PromoteAndDemote(t *testing.T) {
	w := newFakeWriter()
	writeMemory(w, "01AAAAAAAAAAAAAAAAAAAAAAAA", "Rising", "a", "convention", false)
	writeMemory(w, "01BBBBBBBBBBBBBBBBBBBBBBBB", "Falling", "b", "fact", true)

	report := &ConsolidationReport{
		TotalMemories: 2,
		Suggestions: []ConsolidationAction{
			{Type: ActionPromote, MemoryIDs: []string{"01AAAAAAAAAAAAAAAAAAAAAAAA"}, KeepID: "01AAAAAAAAAAAAAAAAAAAAAAAA"},
			{Type: ActionDemote, MemoryIDs: []string{"01BBBBBBBBBBBBBBBBBBBBBBBB"}, KeepID: "01BBBBBBBBBBBBBBBBBBBBBBBB"},
		},
	}

	outcome, err := ApplyConsolidation(context.Background(), "project", report, w)
	if err != nil {
		t.Fatalf("apply: %v", err)
	}
	if len(outcome.Applied) != 2 {
		t.Fatalf("expected 2 applied, got %d", len(outcome.Applied))
	}
	if _, _, important := readMemory(t, w, "01AAAAAAAAAAAAAAAAAAAAAAAA"); !important {
		t.Error("promote did not take effect")
	}
	if _, _, important := readMemory(t, w, "01BBBBBBBBBBBBBBBBBBBBBBBB"); important {
		t.Error("demote did not take effect")
	}
}

// A plan reduced to nothing must not look like a clean corpus: the report is the
// only place a developer learns what was proposed and declined.
func TestConsolidationOutcomeMarkdownReportsRefusals(t *testing.T) {
	outcome := &ConsolidationOutcome{
		Scope:    "project",
		Analysed: 12,
		Skipped: []AppliedAction{{
			Type: ActionDelete, Kept: "01AAAAAAAAAAAAAAAAAAAAAAAA",
			Skipped: "memory is marked important",
		}},
	}

	md := outcome.Markdown()
	for _, want := range []string{"project scope", "12 memories analysed", "Refused", "marked important"} {
		if !strings.Contains(md, want) {
			t.Errorf("report is missing %q:\n%s", want, md)
		}
	}
}

func TestResolveSurvivingType(t *testing.T) {
	t.Parallel()
	cases := []struct {
		name    string
		members []memorySnapshot
		want    string
	}{
		{"correction outranks fact", []memorySnapshot{{Type: "fact"}, {Type: "correction"}}, "correction"},
		{"convention outranks decision", []memorySnapshot{{Type: "decision"}, {Type: "convention"}}, "convention"},
		{"untyped members ignored", []memorySnapshot{{Type: ""}, {Type: "skill"}}, "skill"},
		{"all untyped leaves it alone", []memorySnapshot{{Type: ""}, {Type: ""}}, ""},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := resolveSurvivingType(tc.members); got != tc.want {
				t.Errorf("resolveSurvivingType = %q; want %q", got, tc.want)
			}
		})
	}
}
