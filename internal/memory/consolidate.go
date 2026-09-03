package memory

import (
	"context"
	"encoding/json"
	"fmt"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

// Consolidation is split in two halves on purpose.
//
// RunConsolidation ANALYSES and returns a plan. ApplyConsolidation EXECUTES that
// plan through MemoryService. Nothing between them touches a file.
//
// The split is what makes the destructive half reviewable. An analysis that
// mutates as it goes has no state a test can inspect and no artifact a developer
// can read before the memories are gone; and when the model behind the analysis
// is a CLI whose output format is not guaranteed, the apply step is exactly where
// the guarantees have to live. So the invariants are enforced here, in Go, not
// requested in a prompt:
//
//   - content is never dropped — a memory is only removed as part of an action
//     that carried its content into a survivor;
//   - importance is never lost — if any member of a group was important, the
//     survivor is important;
//   - classification is never lost — the survivor keeps a type, and the most
//     specific type in the group wins;
//   - an important memory is never deleted by a bare delete suggestion;
//   - everything refused is reported, with the reason, rather than dropped.
const (
	// ActionMerge folds duplicates into one memory.
	ActionMerge = "merge"
	// ActionConflict resolves memories that contradict each other.
	ActionConflict = "conflict"
	// ActionUpdate rewrites one memory's content.
	ActionUpdate = "update"
	// ActionPromote marks a memory important.
	ActionPromote = "promote"
	// ActionDemote removes the important mark.
	ActionDemote = "demote"
	// ActionDelete removes a memory outright.
	ActionDelete = "delete"
)

const staleAfter = 90 * 24 * time.Hour

const maxBodyCharsPerMemory = 4000

const maxCorpusCharsPerBatch = 60000

type ConsolidationAction struct {
	// Type is one of the Action* constants.
	Type string
	// MemoryIDs are the memories this action operates on.
	MemoryIDs []string
	// KeepID is the survivor for merge and conflict actions. Empty means the
	// caller decides; it is kept separate from MemoryIDs because flattening the
	// two is how a resolution deletes the memory it meant to preserve.
	KeepID string
	// Title is the current title, for reporting.
	Title string
	// Reason is why the action was proposed, verbatim from the analysis.
	Reason string
	// NewContent is the body to write for merge, conflict and update actions.
	NewContent string
	// NewTitle is the title to write, when the action renames.
	NewTitle string
}

type ConsolidationReport struct {
	TotalMemories  int
	Duplicates     []ConsolidationAction
	Contradictions []ConsolidationAction
	Stale          []ConsolidationAction
	Suggestions    []ConsolidationAction
	AIAnalysis     string
	// AIFailed reports that the analysis could not be obtained. Without it a
	// failed model call and a clean corpus are the same empty report.
	AIFailed bool

	// Batches is how many analysis calls the corpus was split into.
	//
	// It is reported because more than one means the pass was INCOMPLETE in a way
	// nothing else shows: two duplicates that land in different batches are never put
	// in front of the model together, so it cannot notice them. The report would then
	// say "nothing to do" about a pair it never looked at — completeness it did not
	// earn. One batch is the common case and says nothing; the field exists for when
	// it stops being.
	Batches int
}

// CoverageNote states what a batched analysis could not see, or "" when it saw
// everything.
//
// Worded as a limit on the pass rather than a finding about the corpus, because that
// is what it is: no duplicate was found across batches for the simple reason that no
// comparison was made.
func (r *ConsolidationReport) CoverageNote() string {
	if r.Batches <= 1 {
		return ""
	}
	return fmt.Sprintf(
		"Analysed in %d batches: the corpus no longer fits one prompt. Memories in "+
			"different batches were never compared with each other, so a duplicate pair "+
			"split across them would not appear above — that is a limit of this pass, "+
			"not a statement about the corpus.", r.Batches)
}

func (r *ConsolidationReport) HasActions() bool {
	return len(r.Duplicates)+len(r.Contradictions)+len(r.Stale)+len(r.Suggestions) > 0
}

func (r *ConsolidationReport) TotalActions() int {
	return len(r.Duplicates) + len(r.Contradictions) + len(r.Stale) + len(r.Suggestions)
}

// Actions returns every proposed action in apply order: duplicates first, so a
// later contradiction resolution sees one survivor instead of three near-copies.
func (r *ConsolidationReport) Actions() []ConsolidationAction {
	out := make([]ConsolidationAction, 0, r.TotalActions())
	out = append(out, r.Duplicates...)
	out = append(out, r.Contradictions...)
	out = append(out, r.Suggestions...)
	out = append(out, r.Stale...)
	return out
}

type memorySnapshot struct {
	ID        string
	Title     string
	Body      string
	Type      string
	CreatedAt string
	Important bool
	Mandatory bool
}

// RunConsolidation analyses a scope's memories and returns a plan. It writes
// nothing.
func RunConsolidation(ctx context.Context, scope string, aiClient ai.Client) (*ConsolidationReport, error) {
	uri := TableURIForScope(scope)
	if uri == "" {
		return &ConsolidationReport{}, nil
	}
	tbl, err := OpenMemoryTable(ctx, uri)
	if err != nil {
		return nil, fmt.Errorf("opening the memory store: %w", err)
	}
	defer func() { _ = tbl.Close() }()

	records, err := tbl.Live(ctx)
	if err != nil {
		return nil, fmt.Errorf("reading the memories: %w", err)
	}
	memories := snapshotsFromRecords(records)
	return consolidateSnapshots(ctx, memories, aiClient, loadMemoryVectors(ctx, WikiDir(scope))), nil
}

// snapshotsFromRecords is the one mapping from stored rows onto the analysis's view of them, shared
// by the pass that PLANS and the pass that APPLIES so the two can never disagree about what a
// memory's title, body or importance is.
//
// The order is the store's, and it is made deterministic here rather than relied upon: batching a
// large corpus into several prompts is reproducible only if the input order is, and a diff of two
// consolidation runs is unreadable otherwise.
func snapshotsFromRecords(records []MemoryRecord) []memorySnapshot {
	memories := make([]memorySnapshot, 0, len(records))
	for _, rec := range records {
		memories = append(memories, memorySnapshot{
			ID:        rec.ID,
			Title:     titleOrID(rec.Title, rec.ID),
			Body:      strings.TrimSpace(rec.Body),
			Type:      rec.Type,
			CreatedAt: rec.CreatedAt,
			Important: rec.Important,
			Mandatory: rec.Mandatory,
		})
	}
	sort.Slice(memories, func(i, j int) bool { return memories[i].ID < memories[j].ID })
	return memories
}

func titleOrID(title, id string) string {
	if title == "" {
		return id
	}
	return title
}

func consolidateSnapshots(ctx context.Context, memories []memorySnapshot, aiClient ai.Client, vecs map[string][]float32) *ConsolidationReport {
	report := &ConsolidationReport{TotalMemories: len(memories)}
	if len(memories) == 0 {
		return report
	}

	report.Stale = detectStaleMemories(memories)

	if aiClient != nil && len(memories) > 1 {
		aiReport, err := aiConsolidation(ctx, aiClient, memories, vecs)
		if err != nil {
			// Surfaced as a flag, not only as prose: the caller has to be able to
			// tell "nothing to do" from "the analysis never happened", and a
			// string nobody reads cannot carry that.
			report.AIAnalysis = fmt.Sprintf("AI analysis failed: %v", err)
			report.AIFailed = true
			return report
		}
		report.Duplicates = aiReport.Duplicates
		report.Contradictions = aiReport.Contradictions
		report.Suggestions = aiReport.Suggestions
		report.AIAnalysis = aiReport.AIAnalysis
	}

	return report
}

// detectStaleMemories flags memories old enough to be worth re-reading. It
// proposes an update, never a deletion: age is evidence that a memory has not
// been revised, not evidence that it is wrong. Important memories are skipped
// because they are the ones that legitimately sit untouched for months.
func detectStaleMemories(memories []memorySnapshot) []ConsolidationAction {
	var stale []ConsolidationAction

	for _, m := range memories {
		if m.Important || m.Mandatory || m.CreatedAt == "" {
			continue
		}
		t, err := time.Parse(time.RFC3339, m.CreatedAt)
		if err != nil {
			continue
		}
		age := time.Since(t)
		if age <= staleAfter {
			continue
		}
		days := int(age.Hours() / 24)
		stale = append(stale, ConsolidationAction{
			Type:      ActionUpdate,
			MemoryIDs: []string{m.ID},
			KeepID:    m.ID,
			Title:     m.Title,
			Reason:    fmt.Sprintf("Memory is %d days old and has not been revised. Verify it is still accurate.", days),
		})
	}
	return stale
}

const consolidationSystemPrompt = `You are a memory consolidation analyst. You receive a set of agent memories and return a machine-readable plan.

Find three things:

1. DUPLICATES — memories that state the same knowledge in different words.
2. CONTRADICTIONS — memories that cannot both be true, or where one has been superseded.
3. SUGGESTIONS — a memory that should be promoted, demoted, or rewritten, or one that is genuinely obsolete.

Reply with ONE JSON object and nothing else. No prose, no markdown fence.

{
  "duplicates": [
    {"ids": ["ID1","ID2"], "keep_id": "ID1", "merged_title": "...", "merged_content": "...", "reason": "..."}
  ],
  "contradictions": [
    {"ids": ["ID1","ID2"], "keep_id": "ID1", "resolved_title": "...", "resolved_content": "...", "reason": "..."}
  ],
  "suggestions": [
    {"action": "promote|demote|update|delete", "id": "ID", "new_title": "...", "new_content": "...", "reason": "..."}
  ]
}

Rules you MUST follow:

- Use the exact IDs given to you. Never invent an ID.
- "keep_id" MUST be one of "ids".
- "merged_content" and "resolved_content" MUST PRESERVE the knowledge of every memory involved. Union, not summary: keep every distinct fact, file path, number, command and caveat from all of them. Losing a detail to make the text shorter is a failure — the merged memory replaces the originals and they will be deleted.
- For a contradiction, the resolved content must state what is true NOW, and keep the superseded claim as history if the fact that it changed is itself useful.
- Only propose "delete" for a memory whose knowledge is genuinely worthless — not for a memory that is merely old, narrow, or duplicated. Duplicates go in "duplicates", where the content is carried forward.
- Only flag a genuine duplicate. Two memories about the same file are not duplicates if they say different things.
- Empty arrays are correct answers. Return them rather than inventing work.`

func aiConsolidation(ctx context.Context, client ai.Client, memories []memorySnapshot, vecs map[string][]float32) (*ConsolidationReport, error) {
	report := &ConsolidationReport{}
	valid := validIDSet(memories)

	var analyses []string
	batches := batchMemories(orderBySimilarity(memories, vecs))
	report.Batches = len(batches)
	for _, batch := range batches {
		response, err := client.Complete(ctx, consolidationSystemPrompt, renderMemoryCorpus(batch))
		if err != nil {
			return nil, err
		}
		analyses = append(analyses, response)

		plan, ok := parseConsolidationJSON(response)
		if !ok {
			return nil, fmt.Errorf("analysis did not return usable JSON (%d bytes): %s",
				len(response), responseExcerpt(response))
		}

		report.Duplicates = append(report.Duplicates, sanitiseGroupActions(plan.Duplicates, ActionMerge, valid)...)
		report.Contradictions = append(report.Contradictions, sanitiseGroupActions(plan.Contradictions, ActionConflict, valid)...)
		report.Suggestions = append(report.Suggestions, sanitiseSingleActions(plan.Suggestions, valid)...)
	}

	report.AIAnalysis = strings.Join(analyses, "\n\n")
	return report, nil
}

func batchMemories(memories []memorySnapshot) [][]memorySnapshot {
	var batches [][]memorySnapshot
	var current []memorySnapshot
	size := 0

	for _, m := range memories {
		cost := len(m.Title) + len(truncateBody(m.Body)) + 128
		if len(current) > 0 && size+cost > maxCorpusCharsPerBatch {
			batches = append(batches, current)
			current = nil
			size = 0
		}
		current = append(current, m)
		size += cost
	}
	if len(current) > 0 {
		batches = append(batches, current)
	}
	return batches
}

func truncateBody(body string) string {
	if len(body) <= maxBodyCharsPerMemory {
		return body
	}
	return body[:maxBodyCharsPerMemory] + "\n…(truncated for analysis)"
}

func renderMemoryCorpus(memories []memorySnapshot) string {
	var b strings.Builder
	_, _ = fmt.Fprintf(&b, "Analyse these %d memories.\n\n", len(memories))
	for i, m := range memories {
		_, _ = fmt.Fprintf(&b, "--- Memory %d ---\n", i+1)
		_, _ = fmt.Fprintf(&b, "ID: %s\n", m.ID)
		_, _ = fmt.Fprintf(&b, "Title: %s\n", m.Title)
		_, _ = fmt.Fprintf(&b, "Type: %s\n", m.Type)
		_, _ = fmt.Fprintf(&b, "Created: %s\n", m.CreatedAt)
		_, _ = fmt.Fprintf(&b, "Important: %v\n", m.Important)
		_, _ = fmt.Fprintf(&b, "Mandatory: %v\n", m.Mandatory)
		if m.Body != "" {
			_, _ = fmt.Fprintf(&b, "Content:\n%s\n", truncateBody(m.Body))
		}
		b.WriteString("\n")
	}
	return b.String()
}

type consolidationPlan struct {
	Duplicates     []planGroup  `json:"duplicates"`
	Contradictions []planGroup  `json:"contradictions"`
	Suggestions    []planSingle `json:"suggestions"`
}

type planGroup struct {
	IDs    []string `json:"ids"`
	KeepID string   `json:"keep_id"`
	Title  string   `json:"merged_title"`
	// ResolvedTitle is the contradiction spelling of Title.
	ResolvedTitle string `json:"resolved_title"`
	Content       string `json:"merged_content"`
	// ResolvedContent is the contradiction spelling of Content.
	ResolvedContent string `json:"resolved_content"`
	Reason          string `json:"reason"`
}

func (g planGroup) title() string {
	if g.Title != "" {
		return g.Title
	}
	return g.ResolvedTitle
}

func (g planGroup) content() string {
	if g.Content != "" {
		return g.Content
	}
	return g.ResolvedContent
}

type planSingle struct {
	Action     string `json:"action"`
	ID         string `json:"id"`
	NewTitle   string `json:"new_title"`
	NewContent string `json:"new_content"`
	Reason     string `json:"reason"`
}

func parseConsolidationJSON(response string) (consolidationPlan, bool) {
	var plan consolidationPlan

	raw := strings.TrimSpace(response)
	if fenced := extractFencedJSON(raw); fenced != "" {
		raw = fenced
	}
	start := strings.Index(raw, "{")
	end := strings.LastIndex(raw, "}")
	if start < 0 || end <= start {
		return plan, false
	}
	if err := json.Unmarshal([]byte(raw[start:end+1]), &plan); err != nil {
		return consolidationPlan{}, false
	}
	return plan, true
}

var reJSONFence = regexp.MustCompile("(?s)```(?:json)?\\s*(\\{.*?\\})\\s*```")

func extractFencedJSON(s string) string {
	if m := reJSONFence.FindStringSubmatch(s); m != nil {
		return m[1]
	}
	return ""
}

func sanitiseGroupActions(groups []planGroup, actionType string, valid map[string]bool) []ConsolidationAction {
	var out []ConsolidationAction
	for _, g := range groups {
		ids := filterKnownIDs(g.IDs, valid)
		if len(ids) < 2 {
			continue
		}
		keep := g.KeepID
		if !valid[keep] || !containsID(ids, keep) {
			keep = ids[0]
		}
		out = append(out, ConsolidationAction{
			Type:       actionType,
			MemoryIDs:  ids,
			KeepID:     keep,
			Reason:     strings.TrimSpace(g.Reason),
			NewTitle:   strings.TrimSpace(g.title()),
			NewContent: strings.TrimSpace(g.content()),
		})
	}
	return out
}

func sanitiseSingleActions(singles []planSingle, valid map[string]bool) []ConsolidationAction {
	var out []ConsolidationAction
	for _, s := range singles {
		action := strings.ToLower(strings.TrimSpace(s.Action))
		switch action {
		case ActionPromote, ActionDemote, ActionUpdate, ActionDelete:
		default:
			continue
		}
		if !valid[s.ID] {
			continue
		}
		out = append(out, ConsolidationAction{
			Type:       action,
			MemoryIDs:  []string{s.ID},
			KeepID:     s.ID,
			Reason:     strings.TrimSpace(s.Reason),
			NewTitle:   strings.TrimSpace(s.NewTitle),
			NewContent: strings.TrimSpace(s.NewContent),
		})
	}
	return out
}

func validIDSet(memories []memorySnapshot) map[string]bool {
	valid := make(map[string]bool, len(memories))
	for _, m := range memories {
		valid[m.ID] = true
	}
	return valid
}

func filterKnownIDs(ids []string, valid map[string]bool) []string {
	seen := make(map[string]bool, len(ids))
	var out []string
	for _, id := range ids {
		id = strings.TrimSpace(id)
		if id == "" || seen[id] || !valid[id] {
			continue
		}
		seen[id] = true
		out = append(out, id)
	}
	return out
}

func containsID(ids []string, id string) bool {
	for _, candidate := range ids {
		if candidate == id {
			return true
		}
	}
	return false
}

func responseExcerpt(s string) string {
	s = strings.TrimSpace(s)
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		s = s[:i]
	}
	if len(s) > 200 {
		s = s[:200] + "…"
	}
	if s == "" {
		return "(empty response)"
	}
	return s
}
