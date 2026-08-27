package memory

import (
	"fmt"
	"os"
	"sort"
	"strings"
	"time"
)

// MemoryWriter is the subset of MemoryService that applying a consolidation plan
// needs. Every mutation goes through it rather than through os.WriteFile, so each
// change goes through the memory store and the wiki is reindexed — a plan
// applied by writing the raw directory directly leaves the remote and the search index
// describing a store that no longer exists.
type MemoryWriter interface {
	LocalDir() string
	UpdateMemory(id, newTitle, newBody string) error
	// UpdateMemoryTyped also sets the type, so a survivor can inherit the most
	// specific classification in its group.
	UpdateMemoryTyped(id, newTitle, newBody, memType string) error
	RemoveMemory(id string) error
	PromoteMemory(id string) error
	DemoteMemory(id string) error
}

// AppliedAction is one change that happened, or one that was refused.
type AppliedAction struct {
	Type string
	// Kept is the surviving memory, for merges and resolutions.
	Kept string
	// Removed lists the memories folded into Kept.
	Removed []string
	Title   string
	Reason  string
	// Skipped is the reason this action was refused, empty when it was applied.
	Skipped string
	// Err is a failure from the store, distinct from a refusal.
	Err string
}

// ConsolidationOutcome is what a consolidation run actually did. It is the audit
// trail: every refusal is here with its reason, because a plan silently reduced
// to nothing is indistinguishable from a clean corpus.
type ConsolidationOutcome struct {
	Scope    string
	Analysed int
	Applied  []AppliedAction
	Skipped  []AppliedAction
	Failed   []AppliedAction
	// AIFailed carries through from the report.
	AIFailed bool
	// CoverageNote carries through what the analysis could NOT see — a corpus split
	// across several prompts is never compared with itself across the split. Empty
	// when the whole corpus fit one call, which is the common case.
	CoverageNote string
}

// Markdown renders the outcome for the dream report.
func (o *ConsolidationOutcome) Markdown() string {
	var b strings.Builder
	_, _ = fmt.Fprintf(&b, "### Memory consolidation — %s scope\n\n", o.Scope)
	_, _ = fmt.Fprintf(&b, "%d memories analysed. %d actions applied, %d refused, %d failed.\n\n",
		o.Analysed, len(o.Applied), len(o.Skipped), len(o.Failed))

	if o.AIFailed {
		b.WriteString("> The analysis step failed, so only deterministic checks ran.\n\n")
	}

	// Stated even when actions were found: "three duplicates resolved" and "the pass
	// could not see across its own batches" are both true, and dropping the second
	// makes the first read as completeness.
	if o.CoverageNote != "" {
		_, _ = fmt.Fprintf(&b, "> %s\n\n", o.CoverageNote)
	}

	section := func(title string, actions []AppliedAction, withReason bool) {
		if len(actions) == 0 {
			return
		}
		_, _ = fmt.Fprintf(&b, "**%s**\n\n", title)
		b.WriteString("| Action | Kept | Removed | Detail |\n|---|---|---|---|\n")
		for _, a := range actions {
			detail := a.Reason
			if withReason && a.Skipped != "" {
				detail = a.Skipped
			}
			if a.Err != "" {
				detail = a.Err
			}
			_, _ = fmt.Fprintf(&b, "| %s | `%s` | %s | %s |\n",
				a.Type, a.Kept, joinIDs(a.Removed), sanitiseCell(detail))
		}
		b.WriteString("\n")
	}

	section("Applied", o.Applied, false)
	section("Refused", o.Skipped, true)
	section("Failed", o.Failed, true)
	return b.String()
}

func joinIDs(ids []string) string {
	if len(ids) == 0 {
		return "—"
	}
	quoted := make([]string, 0, len(ids))
	for _, id := range ids {
		quoted = append(quoted, "`"+id+"`")
	}
	return strings.Join(quoted, ", ")
}

func sanitiseCell(s string) string {
	s = strings.ReplaceAll(s, "|", "\\|")
	s = strings.ReplaceAll(s, "\n", " ")
	if len(s) > 300 {
		s = s[:300] + "…"
	}
	return s
}

// typePriority orders memory types from most to least specific. When a merge
// group disagrees about type, the most specific one survives: a `correction`
// folded together with a `fact` is still a correction, and demoting it to fact
// loses the reason it exists.
var typePriority = map[string]int{
	string(MemoryTypeCorrection): 6,
	string(MemoryTypeConvention): 5,
	string(MemoryTypeDecision):   4,
	string(MemoryTypeTension):    3,
	string(MemoryTypeSkill):      2,
	string(MemoryTypeFact):       1,
}

// ApplyConsolidation executes a plan and reports what it did.
//
// It never trusts the plan about anything it can verify itself: which memories
// exist, which are important, what their content is. The plan proposes; this
// function decides.
func ApplyConsolidation(scope string, report *ConsolidationReport, w MemoryWriter) (*ConsolidationOutcome, error) {
	outcome := &ConsolidationOutcome{Scope: scope}
	if report == nil {
		return outcome, nil
	}
	outcome.Analysed = report.TotalMemories
	outcome.AIFailed = report.AIFailed
	outcome.CoverageNote = report.CoverageNote()

	if w == nil {
		return outcome, fmt.Errorf("memory writer not configured")
	}

	state, err := loadApplyState(w.LocalDir())
	if err != nil {
		return outcome, fmt.Errorf("reading memory store: %w", err)
	}

	for _, action := range report.Actions() {
		switch action.Type {
		case ActionMerge, ActionConflict:
			applyGroupAction(action, state, w, outcome)
		case ActionPromote:
			applyRelevance(action, state, w, outcome, true)
		case ActionDemote:
			applyRelevance(action, state, w, outcome, false)
		case ActionUpdate:
			applyUpdate(action, state, w, outcome)
		case ActionDelete:
			applyDelete(action, state, w, outcome)
		default:
			outcome.Skipped = append(outcome.Skipped, AppliedAction{
				Type:    action.Type,
				Kept:    action.KeepID,
				Reason:  action.Reason,
				Skipped: fmt.Sprintf("unknown action type %q", action.Type),
			})
		}
	}

	return outcome, nil
}

// applyState is the store as it is on disk, refreshed as actions mutate it, so a
// later action cannot operate on a memory an earlier one removed.
type applyState struct {
	dir     string
	present map[string]memorySnapshot
}

func loadApplyState(dir string) (*applyState, error) {
	snapshots, err := loadMemorySnapshots(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return &applyState{dir: dir, present: map[string]memorySnapshot{}}, nil
		}
		return nil, err
	}
	present := make(map[string]memorySnapshot, len(snapshots))
	for _, s := range snapshots {
		present[s.ID] = s
	}
	return &applyState{dir: dir, present: present}, nil
}

func (s *applyState) get(id string) (memorySnapshot, bool) {
	snap, ok := s.present[id]
	return snap, ok
}

func (s *applyState) remove(id string) { delete(s.present, id) }

func (s *applyState) setImportant(id string, important bool) {
	if snap, ok := s.present[id]; ok {
		snap.Important = important
		s.present[id] = snap
	}
}

func (s *applyState) count() int { return len(s.present) }

// applyGroupAction folds a duplicate group or resolves a contradiction.
//
// The order is: write the survivor first, then remove the others. Reversed, a
// failure between the two steps loses the content permanently; this way the worst
// case is a duplicate that survives to the next cycle.
func applyGroupAction(action ConsolidationAction, state *applyState, w MemoryWriter, outcome *ConsolidationOutcome) {
	members := make([]memorySnapshot, 0, len(action.MemoryIDs))
	for _, id := range action.MemoryIDs {
		if snap, ok := state.get(id); ok {
			members = append(members, snap)
		}
	}
	if len(members) < 2 {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type:    action.Type,
			Kept:    action.KeepID,
			Reason:  action.Reason,
			Skipped: "fewer than two of the named memories still exist",
		})
		return
	}

	keepID := action.KeepID
	if _, ok := state.get(keepID); !ok {
		keepID = members[0].ID
	}

	body := action.NewContent
	if body == "" {
		// The analysis did not supply merged content. Build it by preserving every
		// member verbatim rather than skipping the action: the point of the merge
		// is one memory instead of several, and prose that summarises away a
		// detail is worse than a longer memory that keeps it.
		body = buildUnionBody(members, keepID, action.Type, action.Reason)
	}
	if strings.TrimSpace(body) == "" {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type:    action.Type,
			Kept:    keepID,
			Reason:  action.Reason,
			Skipped: "no content to write — refusing to replace memories with an empty body",
		})
		return
	}

	title := action.NewTitle
	if title == "" {
		if snap, ok := state.get(keepID); ok {
			title = snap.Title
		}
	}

	// The survivor inherits the most specific type in the group. Folding a
	// correction into a fact and keeping "fact" would drop the very thing that
	// makes the memory load-bearing.
	if err := w.UpdateMemoryTyped(keepID, title, body, resolveSurvivingType(members)); err != nil {
		outcome.Failed = append(outcome.Failed, AppliedAction{
			Type: action.Type, Kept: keepID, Reason: action.Reason,
			Err: fmt.Sprintf("writing survivor: %v", err),
		})
		return
	}

	// Importance is a property of the group, not of whichever member the analysis
	// happened to pick. If any of them mattered enough to be marked important, the
	// memory that replaces them all inherits that.
	anyImportant := false
	for _, m := range members {
		if m.Important {
			anyImportant = true
			break
		}
	}
	keptSnap, _ := state.get(keepID)
	if anyImportant && !keptSnap.Important {
		if err := w.PromoteMemory(keepID); err != nil {
			outcome.Failed = append(outcome.Failed, AppliedAction{
				Type: action.Type, Kept: keepID,
				Err: fmt.Sprintf("preserving importance: %v", err),
			})
		} else {
			state.setImportant(keepID, true)
		}
	}

	var removed []string
	for _, m := range members {
		if m.ID == keepID {
			continue
		}
		if err := w.RemoveMemory(m.ID); err != nil {
			outcome.Failed = append(outcome.Failed, AppliedAction{
				Type: action.Type, Kept: keepID, Removed: []string{m.ID},
				Err: fmt.Sprintf("removing folded memory: %v", err),
			})
			continue
		}
		state.remove(m.ID)
		removed = append(removed, m.ID)
	}

	outcome.Applied = append(outcome.Applied, AppliedAction{
		Type:    action.Type,
		Kept:    keepID,
		Removed: removed,
		Title:   title,
		Reason:  action.Reason,
	})
}

// buildUnionBody concatenates every member's content under a provenance heading.
// Nothing is paraphrased, so nothing can be lost.
func buildUnionBody(members []memorySnapshot, keepID, actionType, reason string) string {
	ordered := make([]memorySnapshot, len(members))
	copy(ordered, members)
	sort.SliceStable(ordered, func(i, j int) bool {
		if ordered[i].ID == keepID {
			return true
		}
		if ordered[j].ID == keepID {
			return false
		}
		return ordered[i].CreatedAt < ordered[j].CreatedAt
	})

	var b strings.Builder
	verb := "Consolidated from duplicates"
	if actionType == ActionConflict {
		verb = "Consolidated from contradicting memories"
	}
	_, _ = fmt.Fprintf(&b, "%s on %s.\n", verb, time.Now().UTC().Format("2006-01-02"))
	if reason != "" {
		_, _ = fmt.Fprintf(&b, "\nReason recorded by the analysis: %s\n", reason)
	}

	for _, m := range ordered {
		_, _ = fmt.Fprintf(&b, "\n## %s (`%s`", m.Title, m.ID)
		if m.Type != "" {
			_, _ = fmt.Fprintf(&b, ", %s", m.Type)
		}
		if m.CreatedAt != "" {
			_, _ = fmt.Fprintf(&b, ", created %s", m.CreatedAt)
		}
		b.WriteString(")\n\n")
		if m.Body != "" {
			b.WriteString(m.Body)
			if !strings.HasSuffix(m.Body, "\n") {
				b.WriteString("\n")
			}
		} else {
			b.WriteString("*(no body)*\n")
		}
	}
	return b.String()
}

func applyRelevance(action ConsolidationAction, state *applyState, w MemoryWriter, outcome *ConsolidationOutcome, promote bool) {
	id := action.KeepID
	snap, ok := state.get(id)
	if !ok {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type: action.Type, Kept: id, Reason: action.Reason,
			Skipped: "memory no longer exists",
		})
		return
	}
	if snap.Important == promote {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type: action.Type, Kept: id, Reason: action.Reason,
			Skipped: "already in the requested state",
		})
		return
	}

	var err error
	if promote {
		err = w.PromoteMemory(id)
	} else {
		err = w.DemoteMemory(id)
	}
	if err != nil {
		outcome.Failed = append(outcome.Failed, AppliedAction{
			Type: action.Type, Kept: id, Err: err.Error(),
		})
		return
	}
	state.setImportant(id, promote)
	outcome.Applied = append(outcome.Applied, AppliedAction{
		Type: action.Type, Kept: id, Title: snap.Title, Reason: action.Reason,
	})
}

func applyUpdate(action ConsolidationAction, state *applyState, w MemoryWriter, outcome *ConsolidationOutcome) {
	id := action.KeepID
	snap, ok := state.get(id)
	if !ok {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type: action.Type, Kept: id, Reason: action.Reason,
			Skipped: "memory no longer exists",
		})
		return
	}

	// A stale flag is a prompt to re-read, not a rewrite. Without replacement
	// content there is nothing to apply, and inventing some would be worse than
	// leaving the memory as it is.
	if strings.TrimSpace(action.NewContent) == "" && strings.TrimSpace(action.NewTitle) == "" {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type: action.Type, Kept: id, Title: snap.Title, Reason: action.Reason,
			Skipped: "flagged for review — no replacement content proposed",
		})
		return
	}

	if err := w.UpdateMemory(id, action.NewTitle, action.NewContent); err != nil {
		outcome.Failed = append(outcome.Failed, AppliedAction{
			Type: action.Type, Kept: id, Err: err.Error(),
		})
		return
	}
	outcome.Applied = append(outcome.Applied, AppliedAction{
		Type: action.Type, Kept: id, Title: snap.Title, Reason: action.Reason,
	})
}

// applyDelete is the only path that destroys knowledge without carrying it
// forward, so it is the most constrained.
func applyDelete(action ConsolidationAction, state *applyState, w MemoryWriter, outcome *ConsolidationOutcome) {
	id := action.KeepID
	snap, ok := state.get(id)
	if !ok {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type: action.Type, Kept: id, Reason: action.Reason,
			Skipped: "memory no longer exists",
		})
		return
	}

	// An important memory is never deleted by a suggestion. It was marked
	// important by a human or by an agent acting on one, and no unattended
	// analysis outranks that. Removing it is possible, but only as part of a merge
	// or a resolution, where its content survives in another memory.
	if snap.Important {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type: action.Type, Kept: id, Title: snap.Title, Reason: action.Reason,
			Skipped: "memory is marked important — a bare delete cannot remove it; fold it into another memory instead",
		})
		return
	}

	// Never empty the store. A corpus of one is exactly when an analysis is least
	// reliable, and an empty memory store looks identical to a project that never
	// had one.
	if state.count() <= 1 {
		outcome.Skipped = append(outcome.Skipped, AppliedAction{
			Type: action.Type, Kept: id, Title: snap.Title, Reason: action.Reason,
			Skipped: "refusing to delete the last remaining memory in the scope",
		})
		return
	}

	if err := w.RemoveMemory(id); err != nil {
		outcome.Failed = append(outcome.Failed, AppliedAction{
			Type: action.Type, Kept: id, Err: err.Error(),
		})
		return
	}
	state.remove(id)
	outcome.Applied = append(outcome.Applied, AppliedAction{
		Type: action.Type, Kept: id, Title: snap.Title, Reason: action.Reason,
	})
}

// resolveSurvivingType picks the type a merged memory should keep: the most
// specific one present in the group. Returns empty when no member is typed, which
// UpdateMemoryTyped reads as "leave the existing type alone".
func resolveSurvivingType(members []memorySnapshot) string {
	best := ""
	bestScore := 0
	for _, m := range members {
		if m.Type == "" {
			continue
		}
		if score := typePriority[m.Type]; score > bestScore {
			best, bestScore = m.Type, score
		}
	}
	return best
}
