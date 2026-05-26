package wiki

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"time"
)

type LintConfig struct {
	Deep bool

	Fix bool

	StaleDays int
}

func DefaultLintConfig() LintConfig {
	return LintConfig{
		Deep:      false,
		Fix:       false,
		StaleDays: 30,
	}
}

type LintReport struct {
	TotalPages    int
	Orphans       []string
	BrokenLinks   []BrokenLinkInfo
	StalePages    []string
	EmptyPages    []string
	MissingFields []FieldIssue
	Errors        int
	FixesApplied  int
}

type FieldIssue struct {
	Page          string
	MissingFields []string
}

func (r *LintReport) HasIssues() bool {
	return r.Errors > 0
}

func (r *LintReport) Summary() string {
	if r.Errors == 0 {
		return fmt.Sprintf("✓ %d pages — no issues found", r.TotalPages)
	}
	return fmt.Sprintf("⚠ %d pages — %d issue(s) found", r.TotalPages, r.Errors)
}

func LintWiki(wikiDir string, cfg LintConfig) (*LintReport, error) {
	report := &LintReport{}

	graph, err := BuildCrossRefGraph(wikiDir)
	if err != nil {
		return nil, fmt.Errorf("building cross-ref graph: %w", err)
	}

	report.TotalPages = len(graph.AllPages)

	report.Orphans = OrphanPages(graph)
	report.Errors += len(report.Orphans)

	report.BrokenLinks = BrokenLinks(graph)
	report.Errors += len(report.BrokenLinks)

	for page := range graph.AllPages {
		if page == "index" || page == "log" {
			continue
		}

		filePath := filepath.Join(wikiDir, page+".md")
		data, err := os.ReadFile(filePath)
		if err != nil {
			continue
		}
		content := string(data)

		if cfg.StaleDays > 0 {
			if isStale(content, cfg.StaleDays) {
				report.StalePages = append(report.StalePages, page)
				report.Errors++
			}
		}

		body := stripYAMLFrontmatter(content)
		body = stripBacklinksSection(body)
		wordCount := len(strings.Fields(body))
		if wordCount <= 10 {
			report.EmptyPages = append(report.EmptyPages, page)
			report.Errors++
		}

		missing := checkFrontmatter(content)
		if len(missing) > 0 {
			report.MissingFields = append(report.MissingFields, FieldIssue{
				Page:          page,
				MissingFields: missing,
			})
			report.Errors++
		}
	}

	sort.Strings(report.StalePages)
	sort.Strings(report.EmptyPages)

	if cfg.Fix {

		xrefResult, err := InjectBacklinks(wikiDir, graph)
		if err == nil {
			report.FixesApplied += xrefResult.BacklinksAdded
		}
	}

	return report, nil
}

var reFMField = regexp.MustCompile(`(?m)^(\w+):`)

var requiredFields = []string{"title", "tags", "updated"}

func checkFrontmatter(content string) []string {
	fm := extractFrontmatter(content)
	if fm == "" {
		return requiredFields
	}

	present := make(map[string]bool)
	for _, m := range reFMField.FindAllStringSubmatch(fm, -1) {
		present[strings.ToLower(m[1])] = true
	}

	var missing []string
	for _, f := range requiredFields {
		if !present[f] {
			missing = append(missing, f)
		}
	}
	return missing
}

func extractFrontmatter(content string) string {
	trimmed := strings.TrimSpace(content)
	if !strings.HasPrefix(trimmed, "---") {
		return ""
	}
	lines := strings.Split(trimmed, "\n")
	var fmLines []string
	started := false
	for _, line := range lines {
		t := strings.TrimSpace(line)
		if t == "---" {
			if !started {
				started = true
				continue
			}
			break
		}
		if started {
			fmLines = append(fmLines, line)
		}
	}
	return strings.Join(fmLines, "\n")
}

var reFMUpdated = regexp.MustCompile(`(?m)^updated:\s*(.+)$`)

func isStale(content string, staleDays int) bool {
	fm := extractFrontmatter(content)
	m := reFMUpdated.FindStringSubmatch(fm)
	if m == nil {
		return true
	}

	dateStr := strings.TrimSpace(m[1])
	dateStr = strings.Trim(dateStr, "\"'")

	t, err := time.Parse("2006-01-02", dateStr)
	if err != nil {
		return true
	}

	return time.Since(t).Hours() > float64(staleDays*24)
}

func FormatReport(r *LintReport) string {
	var b strings.Builder

	b.WriteString(fmt.Sprintf("# Wiki Lint Report\n\n"))
	b.WriteString(fmt.Sprintf("**%s**\n\n", r.Summary()))

	if len(r.Orphans) > 0 {
		b.WriteString("## Orphan Pages\n\n")
		b.WriteString("> Pages with no inbound or outbound wikilinks. Consider adding [[wikilinks]] or removing them.\n\n")
		for _, p := range r.Orphans {
			b.WriteString(fmt.Sprintf("- `%s.md`\n", p))
		}
		b.WriteString("\n")
	}

	if len(r.BrokenLinks) > 0 {
		b.WriteString("## Broken Links\n\n")
		b.WriteString("> [[wikilinks]] pointing to pages that don't exist.\n\n")
		for _, bl := range r.BrokenLinks {
			b.WriteString(fmt.Sprintf("- `[[%s]]` in `%s.md`\n", bl.Target, bl.Source))
		}
		b.WriteString("\n")
	}

	if len(r.StalePages) > 0 {
		b.WriteString("## Stale Pages\n\n")
		b.WriteString("> Pages not updated recently. Consider re-indexing.\n\n")
		for _, p := range r.StalePages {
			b.WriteString(fmt.Sprintf("- `%s.md`\n", p))
		}
		b.WriteString("\n")
	}

	if len(r.EmptyPages) > 0 {
		b.WriteString("## Empty Pages\n\n")
		b.WriteString("> Pages with ≤ 10 words of content.\n\n")
		for _, p := range r.EmptyPages {
			b.WriteString(fmt.Sprintf("- `%s.md`\n", p))
		}
		b.WriteString("\n")
	}

	if len(r.MissingFields) > 0 {
		b.WriteString("## Missing Frontmatter\n\n")
		b.WriteString("> Pages missing required YAML frontmatter fields.\n\n")
		for _, fi := range r.MissingFields {
			b.WriteString(fmt.Sprintf("- `%s.md` — missing: %s\n", fi.Page, strings.Join(fi.MissingFields, ", ")))
		}
		b.WriteString("\n")
	}

	if r.FixesApplied > 0 {
		b.WriteString(fmt.Sprintf("\n**Fixes applied:** %d backlinks injected\n", r.FixesApplied))
	}

	return b.String()
}
