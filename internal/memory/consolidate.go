package memory

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
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

// staleAfter is when an unrevised memory becomes worth re-reading. It is the same
// number the GC uses for its own threshold default, deliberately: two different
// definitions of "stale" in one package is how a memory ends up old enough to
// delete by one rule and current by the other.
const staleAfter = 90 * 24 * time.Hour

// maxBodyCharsPerMemory bounds how much of one memory goes into the analysis
// prompt. Long memories are the norm here, and the tail of a long memory is
// rarely what makes it a duplicate of another.
const maxBodyCharsPerMemory = 4000

// maxCorpusCharsPerBatch bounds one analysis call. The corpus grows without
// limit — this repository is already past 270KB of memory — so a single prompt
// carrying all of it eventually exceeds any context window, and the failure mode
// is a truncated prompt answered confidently rather than an error.
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
}

// RunConsolidation analyses a scope's memories and returns a plan. It writes
// nothing.
func RunConsolidation(ctx context.Context, scope string, aiClient ai.Client) (*ConsolidationReport, error) {
	// The scope's own wiki is where its embeddings live, and they are what lets a
	// batched analysis put near-duplicates in the same prompt. Resolved here because
	// this is the only layer that knows the scope.
	return consolidateDirWithVectors(ctx, RawDir(scope), aiClient, loadMemoryVectors(ctx, WikiDir(scope)))
}

// consolidateDir is RunConsolidation against an explicit directory. Resolving the
// scope is the only thing RunConsolidation adds, and separating them is what lets a
// test exercise this logic instead of reimplementing it — four test helpers used to
// carry their own copy of the loop below, which meant they asserted against a
// duplicate that could drift from what production does.
func consolidateDir(ctx context.Context, dir string, aiClient ai.Client) (*ConsolidationReport, error) {
	return consolidateDirWithVectors(ctx, dir, aiClient, nil)
}

// consolidateDirWithVectors is consolidateDir with the scope's embeddings in hand.
// Nil vectors are a valid state — a scope that has not been embedded yet — and the
// batching falls back to arrival order.
func consolidateDirWithVectors(ctx context.Context, dir string, aiClient ai.Client, vecs map[string][]float32) (*ConsolidationReport, error) {
	memories, err := loadMemorySnapshots(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return &ConsolidationReport{}, nil
		}
		return nil, err
	}

	report := &ConsolidationReport{TotalMemories: len(memories)}
	if len(memories) == 0 {
		return report, nil
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
			return report, nil
		}
		report.Duplicates = aiReport.Duplicates
		report.Contradictions = aiReport.Contradictions
		report.Suggestions = aiReport.Suggestions
		report.AIAnalysis = aiReport.AIAnalysis
	}

	return report, nil
}

func loadMemorySnapshots(dir string) ([]memorySnapshot, error) {
	if dir == "" {
		return nil, os.ErrNotExist
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}

	var memories []memorySnapshot
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := e.Name()
		absPath := filepath.Join(dir, name)
		data, readErr := os.ReadFile(absPath)
		if readErr != nil {
			continue
		}
		// The declared id, never the file name: a consolidation pass feeds these ids straight
		// into UpdateMemoryTyped, so an id read off a suffixed name would write a twin under
		// that suffix instead of updating the memory.
		id := MemoryIDFor(string(data), name)
		fm := ParseMemoryFrontmatter(string(data))
		important := fm.Important
		title := fm.Title
		if title == "" {
			title = id
		}

		memories = append(memories, memorySnapshot{
			ID:        id,
			Title:     title,
			Body:      strings.TrimSpace(extractBodyAfterFrontmatter(string(data))),
			Type:      fm.Type,
			CreatedAt: fm.CreatedAt,
			Important: important,
		})
	}

	// Stable order so batching is reproducible and a diff of two runs is readable.
	sort.Slice(memories, func(i, j int) bool { return memories[i].ID < memories[j].ID })
	return memories, nil
}

// detectStaleMemories flags memories old enough to be worth re-reading. It
// proposes an update, never a deletion: age is evidence that a memory has not
// been revised, not evidence that it is wrong. Important memories are skipped
// because they are the ones that legitimately sit untouched for months.
func detectStaleMemories(memories []memorySnapshot) []ConsolidationAction {
	var stale []ConsolidationAction

	for _, m := range memories {
		if m.Important || m.CreatedAt == "" {
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
	// Ordered before splitting, so that what lands in one prompt is what belongs
	// together rather than what happened to be written on the same day.
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
			// One contract, and a loud failure when it is violated. A second,
			// looser parser here would keep some malformed answers usable at the
			// cost of turning the rest into partially-understood plans applied
			// against real memories — and an unparseable analysis would look
			// identical to a clean corpus. RunConsolidation turns this into
			// AIFailed, which the report states outright.
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

// batchMemories splits the corpus into prompt-sized groups, in the order it is given.
//
// One batch is the common case. When the corpus outgrows a prompt, what decides whether
// a duplicate pair is ever compared is ADJACENCY in this slice — which is why the
// caller orders by similarity first. Cutting a chain still separates the two memories
// either side of each cut, so this remains a trade, just a far better one than cutting
// an arbitrary order.
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
		if m.Body != "" {
			_, _ = fmt.Fprintf(&b, "Content:\n%s\n", truncateBody(m.Body))
		}
		b.WriteString("\n")
	}
	return b.String()
}

// consolidationPlan is the wire shape of the analysis.
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

// parseConsolidationJSON extracts the plan from a response that may be wrapped
// in a fence or padded with prose.
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

// sanitiseGroupActions drops what cannot be applied and normalises what can.
// Doing it here means the apply step receives only actions whose IDs exist and
// whose survivor is a member of the group.
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

// responseExcerpt is enough of an unusable response to identify it in a log without
// pasting an entire model answer into one.
//
// Deliberately not memory.firstLine, which skips lines starting with "#" because it
// summarises wiki bodies. A model that ignored the JSON instruction usually answers
// in markdown, so the heading is the most identifying line there is.
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
