package knowledge

import (
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type LintSeverity string

const (
	LintError   LintSeverity = "error"
	LintWarning LintSeverity = "warning"
	LintInfo    LintSeverity = "info"
)

type LintFinding struct {
	Severity LintSeverity
	Page     string
	Message  string
}

type LintResult struct {
	Findings     []LintFinding
	TotalPages   int
	HealthyPages int
	StalePages   int
}

// LintKnowledgeWiki performs deterministic health checks on the documents a build compiled.
//
// It takes the DOCUMENTS, not a wiki directory. The checks below were text scans over the rendered
// pages the build had just written — a frontmatter substring search for `title:`, `type:`,
// `content_hash:` and `stale_since:`, and a search for the literal `## Content` heading — which
// asked the file for facts the pass that wrote it was holding. Reading them from the documents is
// the same audit without the round trip, and it survives the pages not existing.
func LintKnowledgeWiki(graph *wiki.CrossRefGraph, docs []knowledgeDoc, slugs []string) *LintResult {
	result := &LintResult{}
	if graph == nil {
		return result
	}

	result.TotalPages = len(graph.AllPages)

	// 1. Broken links — references pointing to non-existent pages
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
	for slug := range graph.AllPages {
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

	// 3, 4, 5. What each document itself is missing.
	//
	// `index` and `log` are not skipped any more because they are not documents: they were
	// generated pages sitting in the same directory, and the only reason this pass had to know
	// their names was that it read a directory.
	for i, doc := range docs {
		slug := ""
		if i < len(slugs) {
			slug = slugs[i]
		}

		for field, value := range map[string]string{
			"title":        doc.title,
			"type":         doc.docType,
			"content_hash": doc.contentHash,
		} {
			if strings.TrimSpace(value) == "" {
				result.Findings = append(result.Findings, LintFinding{
					Severity: LintWarning,
					Page:     slug,
					Message:  "missing frontmatter field: " + field,
				})
			}
		}

		if doc.staleSince != "" {
			result.StalePages++
			result.Findings = append(result.Findings, LintFinding{
				Severity: LintWarning,
				Page:     slug,
				Message:  "page is stale — source or dependency has changed",
			})
		}

		if strings.TrimSpace(wiki.StripFrontmatter(doc.body)) == "" {
			result.Findings = append(result.Findings, LintFinding{
				Severity: LintInfo,
				Page:     slug,
				Message:  "no content section",
			})
		}
	}

	// The "uncited source" check is GONE, and it is worth saying why rather than leaving a check
	// that cannot fail. It looked for a source path that no page's text mentioned, which was
	// meaningful when a page could be a slice of a document. One document has been one page since
	// the per-heading chunker was removed, and every page carried its own `**Source:**` line, so
	// the check has been vacuously true for as long as it has existed.

	// Sort findings by severity (errors first)
	severityOrder := map[LintSeverity]int{LintError: 0, LintWarning: 1, LintInfo: 2}
	sort.Slice(result.Findings, func(i, j int) bool {
		si, sj := severityOrder[result.Findings[i].Severity], severityOrder[result.Findings[j].Severity]
		if si != sj {
			return si < sj
		}
		if result.Findings[i].Page != result.Findings[j].Page {
			return result.Findings[i].Page < result.Findings[j].Page
		}
		return result.Findings[i].Message < result.Findings[j].Message
	})

	result.HealthyPages = result.TotalPages - result.StalePages - len(wiki.OrphanPages(graph))
	if result.HealthyPages < 0 {
		result.HealthyPages = 0
	}

	return result
}
