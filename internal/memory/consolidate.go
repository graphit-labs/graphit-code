package memory

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
)

type ConsolidationAction struct {
	Type       string
	MemoryIDs  []string
	Title      string
	Reason     string
	NewContent string
	NewTitle   string
}

type ConsolidationReport struct {
	TotalMemories  int
	Duplicates     []ConsolidationAction
	Contradictions []ConsolidationAction
	Stale          []ConsolidationAction
	Suggestions    []ConsolidationAction
	AIAnalysis     string
}

func (r *ConsolidationReport) HasActions() bool {
	return len(r.Duplicates)+len(r.Contradictions)+len(r.Stale)+len(r.Suggestions) > 0
}

func (r *ConsolidationReport) TotalActions() int {
	return len(r.Duplicates) + len(r.Contradictions) + len(r.Stale) + len(r.Suggestions)
}

type memorySnapshot struct {
	ID        string
	Title     string
	Body      string
	Type      string
	CreatedAt string
	Important bool
}

func RunConsolidation(ctx context.Context, scope string, aiClient ai.Client) (*ConsolidationReport, error) {
	dir := RawDir(scope)
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return &ConsolidationReport{}, nil
		}
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
		important := IsImportantMemory(name)
		var id string
		if important {
			id = strings.TrimSuffix(name, ImportantMemorySuffix+".md")
		} else {
			id = strings.TrimSuffix(name, ".md")
		}
		title, createdAt := parseMemoryMeta(absPath)
		body := extractBodyAfterFrontmatter(string(data))
		memType := parseConsolidationType(string(data))

		memories = append(memories, memorySnapshot{
			ID:        id,
			Title:     title,
			Body:      strings.TrimSpace(body),
			Type:      memType,
			CreatedAt: createdAt,
			Important: important,
		})
	}

	report := &ConsolidationReport{TotalMemories: len(memories)}

	if len(memories) == 0 {
		return report, nil
	}

	report.Stale = detectStaleMemories(memories)

	if aiClient != nil && len(memories) > 1 {
		aiReport, err := aiConsolidation(ctx, aiClient, memories)
		if err != nil {

			report.AIAnalysis = fmt.Sprintf("AI analysis failed: %v", err)
			return report, nil
		}
		report.Duplicates = aiReport.Duplicates
		report.Contradictions = aiReport.Contradictions
		report.Suggestions = aiReport.Suggestions
		report.AIAnalysis = aiReport.AIAnalysis
	}

	return report, nil
}

var reConsolidationType = regexp.MustCompile(`(?m)^type:\s*(.+)$`)

func parseConsolidationType(content string) string {
	if m := reConsolidationType.FindStringSubmatch(content); m != nil {
		return strings.TrimSpace(m[1])
	}
	return ""
}

func detectStaleMemories(memories []memorySnapshot) []ConsolidationAction {
	threshold := 30 * 24 * time.Hour
	var stale []ConsolidationAction

	for _, m := range memories {
		if m.Important {
			continue
		}
		if m.CreatedAt == "" {
			continue
		}
		t, err := time.Parse(time.RFC3339, m.CreatedAt)
		if err != nil {
			continue
		}
		age := time.Since(t)
		if age > threshold {
			days := int(age.Hours() / 24)
			stale = append(stale, ConsolidationAction{
				Type:      "update",
				MemoryIDs: []string{m.ID},
				Title:     m.Title,
				Reason:    fmt.Sprintf("Memory is %d days old. Verify it's still accurate or delete it.", days),
			})
		}
	}
	return stale
}

func aiConsolidation(ctx context.Context, client ai.Client, memories []memorySnapshot) (*ConsolidationReport, error) {
	var memoryList strings.Builder
	for i, m := range memories {
		memoryList.WriteString(fmt.Sprintf("--- Memory %d ---\n", i+1))
		memoryList.WriteString(fmt.Sprintf("ID: %s\n", m.ID))
		memoryList.WriteString(fmt.Sprintf("Title: %s\n", m.Title))
		memoryList.WriteString(fmt.Sprintf("Type: %s\n", m.Type))
		memoryList.WriteString(fmt.Sprintf("Created: %s\n", m.CreatedAt))
		memoryList.WriteString(fmt.Sprintf("Important: %v\n", m.Important))
		if m.Body != "" {
			memoryList.WriteString(fmt.Sprintf("Content:\n%s\n", m.Body))
		}
		memoryList.WriteString("\n")
	}

	systemPrompt := `You are a memory consolidation agent. Your job is to analyze a set of agent memories and identify:

1. DUPLICATES: Memories that say the same thing in different words. Propose merging them into one.
2. CONTRADICTIONS: Memories that conflict with each other. Identify which is likely outdated.
3. SUGGESTIONS: Memories that could be improved (vague content, missing context, should be promoted/demoted).

Output your analysis in EXACTLY this format (no markdown fences, no extra text):

## DUPLICATES
- MERGE [ID1, ID2]: "Proposed merged title" — reason
(or "None found" if no duplicates)

## CONTRADICTIONS
- CONFLICT [ID1] vs [ID2]: reason — recommend keeping [IDx]
(or "None found" if no contradictions)

## SUGGESTIONS
- PROMOTE [ID]: reason (for memories that should be important)
- DEMOTE [ID]: reason (for important memories that are too specific)
- UPDATE [ID]: reason (for memories with vague or incomplete content)
- DELETE [ID]: reason (for memories that are no longer relevant)
(or "None found" if no suggestions)

Be concise. Only flag genuine issues. Do not flag memories as duplicates unless they truly overlap.`

	userPrompt := fmt.Sprintf("Analyze these %d memories for duplicates, contradictions, and improvement opportunities:\n\n%s",
		len(memories), memoryList.String())

	response, err := client.Complete(ctx, systemPrompt, userPrompt)
	if err != nil {
		return nil, err
	}

	report := &ConsolidationReport{AIAnalysis: response}
	report.Duplicates = parseConsolidationSection(response, "DUPLICATES", "merge")
	report.Contradictions = parseConsolidationSection(response, "CONTRADICTIONS", "conflict")
	report.Suggestions = parseSuggestionSection(response)
	return report, nil
}

func parseConsolidationSection(response, sectionName, actionType string) []ConsolidationAction {

	sectionHeader := "## " + sectionName
	idx := strings.Index(response, sectionHeader)
	if idx == -1 {
		return nil
	}
	content := response[idx+len(sectionHeader):]

	nextSection := strings.Index(content, "\n## ")
	if nextSection != -1 {
		content = content[:nextSection]
	}

	if strings.Contains(strings.ToLower(content), "none found") {
		return nil
	}

	var actions []ConsolidationAction
	for _, line := range strings.Split(content, "\n") {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "- ") {
			continue
		}
		line = strings.TrimPrefix(line, "- ")

		ids := extractBracketedIDs(line)
		reason := line

		actions = append(actions, ConsolidationAction{
			Type:      actionType,
			MemoryIDs: ids,
			Reason:    reason,
		})
	}
	return actions
}

func parseSuggestionSection(response string) []ConsolidationAction {
	sectionHeader := "## SUGGESTIONS"
	idx := strings.Index(response, sectionHeader)
	if idx == -1 {
		return nil
	}
	content := response[idx+len(sectionHeader):]

	nextSection := strings.Index(content, "\n## ")
	if nextSection != -1 {
		content = content[:nextSection]
	}

	if strings.Contains(strings.ToLower(content), "none found") {
		return nil
	}

	var actions []ConsolidationAction
	for _, line := range strings.Split(content, "\n") {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "- ") {
			continue
		}
		line = strings.TrimPrefix(line, "- ")

		actionType := "update"
		lineLower := strings.ToLower(line)
		switch {
		case strings.HasPrefix(lineLower, "promote"):
			actionType = "promote"
		case strings.HasPrefix(lineLower, "demote"):
			actionType = "demote"
		case strings.HasPrefix(lineLower, "delete"):
			actionType = "delete"
		case strings.HasPrefix(lineLower, "update"):
			actionType = "update"
		}

		ids := extractBracketedIDs(line)
		actions = append(actions, ConsolidationAction{
			Type:      actionType,
			MemoryIDs: ids,
			Reason:    line,
		})
	}
	return actions
}

var reBracketedID = regexp.MustCompile(`\[([A-Z0-9]{10,30})\]`)

func extractBracketedIDs(s string) []string {
	matches := reBracketedID.FindAllStringSubmatch(s, -1)
	var ids []string
	for _, m := range matches {
		ids = append(ids, m[1])
	}
	return ids
}

type GCCandidate struct {
	ID     string
	Title  string
	Reason string
	Age    int
}

type GCReport struct {
	TotalMemories int
	Candidates    []GCCandidate
}

func RunGC(scope string, staleDays int) (*GCReport, error) {
	dir := RawDir(scope)
	entries, err := os.ReadDir(dir)
	if err != nil {
		if os.IsNotExist(err) {
			return &GCReport{}, nil
		}
		return nil, err
	}

	if staleDays <= 0 {
		staleDays = 90
	}
	threshold := time.Duration(staleDays) * 24 * time.Hour

	report := &GCReport{}
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".md" {
			continue
		}
		name := e.Name()
		absPath := filepath.Join(dir, name)
		important := IsImportantMemory(name)
		report.TotalMemories++

		if important {
			continue
		}

		data, readErr := os.ReadFile(absPath)
		if readErr != nil {
			continue
		}

		var id string
		if important {
			id = strings.TrimSuffix(name, ImportantMemorySuffix+".md")
		} else {
			id = strings.TrimSuffix(name, ".md")
		}

		title, createdAt := parseMemoryMeta(absPath)
		body := strings.TrimSpace(extractBodyAfterFrontmatter(string(data)))
		memType := parseConsolidationType(string(data))

		if len(body) < 20 {
			report.Candidates = append(report.Candidates, GCCandidate{
				ID:     id,
				Title:  title,
				Reason: fmt.Sprintf("Empty or near-empty body (%d chars)", len(body)),
			})
			continue
		}

		if createdAt != "" {
			t, err := time.Parse(time.RFC3339, createdAt)
			if err == nil {
				age := time.Since(t)
				days := int(age.Hours() / 24)

				if age > threshold && memType == "" {
					report.Candidates = append(report.Candidates, GCCandidate{
						ID:     id,
						Title:  title,
						Reason: fmt.Sprintf("Unclassified memory, %d days old (threshold: %d days)", days, staleDays),
						Age:    days,
					})
					continue
				}

				if age > 2*threshold {
					report.Candidates = append(report.Candidates, GCCandidate{
						ID:     id,
						Title:  title,
						Reason: fmt.Sprintf("Very old memory, %d days old (2× threshold: %d days)", days, 2*staleDays),
						Age:    days,
					})
				}
			}
		}
	}
	return report, nil
}

func ApplyGC(ctx context.Context, scope string, candidates []GCCandidate, svc *MemoryService) (int, error) {
	deleted := 0
	for _, c := range candidates {
		if err := svc.RemoveMemory(c.ID); err != nil {
			continue
		}
		deleted++
	}
	return deleted, nil
}
