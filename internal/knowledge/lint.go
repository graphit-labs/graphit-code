package knowledge

import (
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// LintSeverity represents the severity level of a lint finding.
type LintSeverity string

const (
	LintError   LintSeverity = "error"
	LintWarning LintSeverity = "warning"
	LintInfo    LintSeverity = "info"
)

// LintFinding represents a single lint issue found in the wiki.
type LintFinding struct {
	Severity LintSeverity
	Page     string
	Message  string
}

// LintResult contains the complete results of a wiki lint check.
type LintResult struct {
	Findings     []LintFinding
	TotalPages   int
	HealthyPages int
	StalePages   int
}

// LintKnowledgeWiki performs deterministic health checks on the knowledge wiki.
// It checks for broken links, orphan pages, missing frontmatter, stale pages,
// empty content, and uncited sources.
func LintKnowledgeWiki(wikiDir string, graph *wiki.CrossRefGraph, sourcePaths []string) *LintResult {
	result := &LintResult{}
	if graph == nil {
		return result
	}

	result.TotalPages = len(graph.AllPages)

	// 1. Broken links — wikilinks pointing to non-existent pages
	for src, targets := range graph.Outbound {
		for _, target := range targets {
			if !graph.AllPages[target] {
				result.Findings = append(result.Findings, LintFinding{
					Severity: LintError,
					Page:     src,
					Message:  "broken link: [[" + target + "]] does not exist",
				})
			}
		}
	}

	// 2. Orphan pages — no inbound and no outbound references
	specialPages := map[string]bool{"index": true, "log": true}
	for slug := range graph.AllPages {
		if specialPages[slug] {
			continue
		}
		hasInbound := len(graph.Inbound[slug]) > 0
		hasOutbound := len(graph.Outbound[slug]) > 0
		if !hasInbound && !hasOutbound {
			result.Findings = append(result.Findings, LintFinding{
				Severity: LintWarning,
				Page:     slug,
				Message:  "orphan page: no inbound or outbound references",
			})
		}
	}

	// 3. Missing frontmatter — check for required fields
	entries, _ := os.ReadDir(wikiDir)
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".md") {
			continue
		}
		slug := strings.TrimSuffix(entry.Name(), ".md")
		if specialPages[slug] {
			continue
		}
		data, err := os.ReadFile(filepath.Join(wikiDir, entry.Name()))
		if err != nil {
			continue
		}
		content := string(data)
		if !strings.HasPrefix(content, "---\n") {
			result.Findings = append(result.Findings, LintFinding{
				Severity: LintWarning,
				Page:     slug,
				Message:  "missing frontmatter",
			})
			continue
		}
		fm := content[:strings.Index(content[4:], "---")+7]
		for _, field := range []string{"title:", "type:", "content_hash:"} {
			if !strings.Contains(fm, field) {
				result.Findings = append(result.Findings, LintFinding{
					Severity: LintWarning,
					Page:     slug,
					Message:  "missing frontmatter field: " + strings.TrimSuffix(field, ":"),
				})
			}
		}

		// 4. Stale pages — stale_since present in frontmatter
		if strings.Contains(fm, "stale_since:") {
			result.StalePages++
			result.Findings = append(result.Findings, LintFinding{
				Severity: LintWarning,
				Page:     slug,
				Message:  "page is stale — source or dependency has changed",
			})
		}

		// 5. Empty content — no ## Content section
		if !strings.Contains(content, "## Content") {
			result.Findings = append(result.Findings, LintFinding{
				Severity: LintInfo,
				Page:     slug,
				Message:  "no content section",
			})
		}
	}

	// 6. Uncited sources — source files not referenced by any wiki page
	if len(sourcePaths) > 0 {
		citedSources := make(map[string]bool)
		for _, entry := range entries {
			if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".md") {
				continue
			}
			data, _ := os.ReadFile(filepath.Join(wikiDir, entry.Name()))
			content := string(data)
			for _, sp := range sourcePaths {
				if strings.Contains(content, sp) {
					citedSources[sp] = true
				}
			}
		}
		for _, sp := range sourcePaths {
			if !citedSources[sp] {
				result.Findings = append(result.Findings, LintFinding{
					Severity: LintInfo,
					Page:     "",
					Message:  "uncited source: " + sp + " — no wiki page references it",
				})
			}
		}
	}

	// Sort findings by severity (errors first)
	severityOrder := map[LintSeverity]int{LintError: 0, LintWarning: 1, LintInfo: 2}
	sort.Slice(result.Findings, func(i, j int) bool {
		si, sj := severityOrder[result.Findings[i].Severity], severityOrder[result.Findings[j].Severity]
		if si != sj {
			return si < sj
		}
		return result.Findings[i].Page < result.Findings[j].Page
	})

	result.HealthyPages = result.TotalPages - result.StalePages - len(wiki.OrphanPages(graph))
	if result.HealthyPages < 0 {
		result.HealthyPages = 0
	}

	return result
}
